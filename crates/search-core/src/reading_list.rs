//! Lista de lectura ("Leer más tarde") con estimación de minutos de lectura
//! y persistencia atómica en `reading-list.json`.

use crate::paths;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Velocidad media de lectura en palabras por minuto.
pub const WORDS_PER_MINUTE: usize = 200;

static READING_SEQ: AtomicU64 = AtomicU64::new(1);

fn generate_item_id() -> String {
    let seq = READING_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("rl-{:x}-{:x}-{}", std::process::id(), nanos, seq)
}

/// Estima los minutos de lectura a partir de un conteo de palabras (200 ppm).
pub fn estimate_reading_minutes(word_count: usize) -> u32 {
    if word_count == 0 {
        return 0;
    }
    let mins = word_count.div_ceil(WORDS_PER_MINUTE);
    u32::try_from(mins).unwrap_or(u32::MAX).max(1)
}

/// Estima los minutos de lectura contando palabras separadas por espacios en un texto.
pub fn estimate_reading_minutes_from_text(text: &str) -> u32 {
    let count = text.split_whitespace().count();
    estimate_reading_minutes(count)
}

/// Elemento guardado en la lista de lectura.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadingListItem {
    pub id: String,
    pub url: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub word_count: Option<u32>,
    #[serde(default)]
    pub estimated_minutes: u32,
    #[serde(default)]
    pub added_at_epoch: u64,
    #[serde(default)]
    pub read: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_at_epoch: Option<u64>,
}

/// Almacén en memoria y en disco de la lista de lectura (`reading-list.json`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ReadingListStore {
    pub items: Vec<ReadingListItem>,
}

impl ReadingListStore {
    /// Crea una lista de lectura vacía.
    pub fn new() -> Self {
        Self::default()
    }

    /// Carga `reading-list.json`. Si está corrupto crea `.bak` y devuelve vacío.
    pub fn load(path: &Path) -> Self {
        if !path.exists() {
            return Self::default();
        }
        let raw = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(err) => {
                crate::warn!(
                    "No se pudo leer '{}': {err}. Arrancando con lista de lectura vacía.",
                    path.display()
                );
                return Self::default();
            }
        };
        match serde_json::from_str::<Self>(&raw) {
            Ok(mut store) => {
                store.sanitize();
                store
            }
            Err(err) => {
                crate::warn!(
                    "JSON corrupto en '{}': {err}. Guardando respaldo .bak y arrancando vacío.",
                    path.display()
                );
                if let Err(bak_err) = paths::backup_corrupt_file(path) {
                    crate::warn!("No se pudo crear respaldo .bak: {bak_err}");
                }
                Self::default()
            }
        }
    }

    /// Guarda la lista atómicamente en `path`.
    pub fn save(&self, path: &Path) -> Result<()> {
        let mut normalized = self.clone();
        normalized.sanitize();
        let mut json = serde_json::to_vec_pretty(&normalized)?;
        json.push(b'\n');
        paths::write_atomic(path, &json)
    }

    /// Sanea URLs vacías, IDs duplicados y recalcula minutos si falta.
    pub fn sanitize(&mut self) {
        let mut seen_ids = HashSet::new();
        let mut seen_urls = HashSet::new();
        self.items.retain(|item| {
            let url = item.url.trim();
            !item.id.trim().is_empty()
                && !url.is_empty()
                && seen_ids.insert(item.id.clone())
                && seen_urls.insert(url.to_string())
        });
        for item in &mut self.items {
            item.url = item.url.trim().to_string();
            let t = item.title.trim();
            item.title = if t.is_empty() {
                item.url.clone()
            } else {
                t.to_string()
            };
            if item.estimated_minutes == 0 {
                if let Some(wc) = item.word_count {
                    item.estimated_minutes = estimate_reading_minutes(wc as usize);
                }
            }
            if !item.read {
                item.read_at_epoch = None;
            }
        }
    }

    /// Añade o actualiza un artículo en la lista de lectura (deduplica por URL).
    pub fn add(
        &mut self,
        url: impl Into<String>,
        title: impl Into<String>,
        word_count: Option<u32>,
        now_epoch: u64,
    ) -> Result<&ReadingListItem> {
        let raw_url = url.into();
        let clean_url = raw_url.trim().to_string();
        if clean_url.is_empty() {
            bail!("La URL de la lista de lectura no puede estar vacía");
        }
        let raw_title = title.into();
        let clean_title = if raw_title.trim().is_empty() {
            clean_url.clone()
        } else {
            raw_title.trim().to_string()
        };
        let estimated_minutes = word_count
            .map(|wc| estimate_reading_minutes(wc as usize))
            .unwrap_or(1);

        if let Some(pos) = self.items.iter().position(|i| i.url == clean_url) {
            self.items[pos].title = clean_title;
            if let Some(wc) = word_count {
                self.items[pos].word_count = Some(wc);
                self.items[pos].estimated_minutes = estimate_reading_minutes(wc as usize);
            }
            return Ok(&self.items[pos]);
        }

        let item = ReadingListItem {
            id: generate_item_id(),
            url: clean_url,
            title: clean_title,
            word_count,
            estimated_minutes,
            added_at_epoch: now_epoch,
            read: false,
            read_at_epoch: None,
        };
        self.items.insert(0, item);
        Ok(&self.items[0])
    }

    /// Marca un elemento como leído. Devuelve `true` si existía.
    pub fn mark_read(&mut self, id: &str, now_epoch: u64) -> bool {
        if let Some(item) = self.items.iter_mut().find(|i| i.id == id) {
            item.read = true;
            item.read_at_epoch = Some(now_epoch);
            true
        } else {
            false
        }
    }

    /// Marca un elemento como no leído. Devuelve `true` si existía.
    pub fn mark_unread(&mut self, id: &str) -> bool {
        if let Some(item) = self.items.iter_mut().find(|i| i.id == id) {
            item.read = false;
            item.read_at_epoch = None;
            true
        } else {
            false
        }
    }

    /// Elimina un elemento por ID. Devuelve `true` si existía.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.items.len();
        self.items.retain(|i| i.id != id);
        self.items.len() < before
    }

    /// Devuelve solo los elementos sin leer.
    pub fn filter_unread(&self) -> Vec<&ReadingListItem> {
        self.items.iter().filter(|i| !i.read).collect()
    }

    /// Devuelve todos los elementos.
    pub fn filter_all(&self) -> &[ReadingListItem] {
        &self.items
    }

    /// Cantidad de elementos pendientes de lectura.
    pub fn unread_count(&self) -> usize {
        self.items.iter().filter(|i| !i.read).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir()
            .join(format!("search-reading-list-{name}-{nanos}"))
            .join("reading-list.json")
    }

    #[test]
    fn estimate_minutes_handles_zero_and_rounding() {
        assert_eq!(estimate_reading_minutes(0), 0);
        assert_eq!(estimate_reading_minutes(1), 1);
        assert_eq!(estimate_reading_minutes(200), 1);
        assert_eq!(estimate_reading_minutes(201), 2);
        assert_eq!(estimate_reading_minutes(1000), 5);
        assert_eq!(estimate_reading_minutes_from_text("one two three four"), 1);
    }

    #[test]
    fn add_mark_read_unread_filter_and_remove() {
        let mut store = ReadingListStore::new();
        let id1 = store
            .add("https://rust-lang.org", "Rust", Some(850), 100)
            .unwrap()
            .id
            .clone();
        let id2 = store
            .add("https://docs.rs", "Docs.rs", Some(400), 200)
            .unwrap()
            .id
            .clone();

        assert_eq!(store.unread_count(), 2);
        assert_eq!(store.filter_all().len(), 2);
        assert_eq!(store.filter_unread().len(), 2);

        assert!(store.mark_read(&id1, 300));
        assert_eq!(store.unread_count(), 1);
        assert_eq!(store.filter_unread().len(), 1);
        assert_eq!(store.filter_unread()[0].id, id2);

        assert!(store.mark_unread(&id1));
        assert_eq!(store.unread_count(), 2);

        assert!(store.remove(&id2));
        assert_eq!(store.filter_all().len(), 1);
    }

    #[test]
    fn deduplicates_url_and_persists_atomically() {
        let path = temp_path("persist");
        let mut store = ReadingListStore::new();
        store
            .add("https://example.com/post", "Old Title", Some(300), 10)
            .unwrap();
        store
            .add("https://example.com/post", "New Title", Some(600), 20)
            .unwrap();
        assert_eq!(store.filter_all().len(), 1);
        assert_eq!(store.filter_all()[0].title, "New Title");
        assert_eq!(store.filter_all()[0].estimated_minutes, 3);

        store.save(&path).unwrap();
        let loaded = ReadingListStore::load(&path);
        assert_eq!(loaded.filter_all().len(), 1);
        assert_eq!(loaded.filter_all()[0].title, "New Title");

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
