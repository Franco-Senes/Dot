//! Pila de pestañas cerradas recientemente con capacidad máxima de 50 entradas.
//!
//! Mantiene el orden más-reciente-primero, soporta pestañas divididas (`is_split` + `partner_url`)
//! y persistencia atómica en JSON con respaldo `.bak` ante corrupción.

use crate::paths;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Número máximo de pestañas cerradas retenidas en la pila.
pub const MAX_CLOSED_TABS: usize = 50;

/// Entrada de una pestaña cerrada recientemente.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClosedTabEntry {
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub is_split: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partner_url: Option<String>,
    #[serde(default)]
    pub closed_at_epoch: u64,
}

impl ClosedTabEntry {
    /// Crea una nueva entrada normalizando título y URL.
    pub fn new(
        title: impl Into<String>,
        url: impl Into<String>,
        is_split: bool,
        partner_url: Option<String>,
        closed_at_epoch: u64,
    ) -> Self {
        let url_str = url.into().trim().to_string();
        let title_str = title.into().trim().to_string();
        let partner = partner_url
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty());
        Self {
            title: if title_str.is_empty() {
                url_str.clone()
            } else {
                title_str
            },
            url: url_str,
            is_split,
            partner_url: if is_split { partner } else { None },
            closed_at_epoch,
        }
    }
}

/// Pila LIFO acotada a [`MAX_CLOSED_TABS`] para reabrir pestañas cerradas (`Ctrl+Shift+T`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ClosedTabsStack {
    entries: Vec<ClosedTabEntry>,
}

impl ClosedTabsStack {
    /// Crea una pila vacía.
    pub fn new() -> Self {
        Self::default()
    }

    /// Carga la pila desde disco; si el JSON está corrupto crea `.bak` y devuelve vacío.
    pub fn load(path: &Path) -> Self {
        if !path.exists() {
            return Self::default();
        }
        let raw = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(err) => {
                crate::warn!(
                    "No se pudo leer '{}': {err}. Arrancando con pila de cerradas vacía.",
                    path.display()
                );
                return Self::default();
            }
        };
        match serde_json::from_str::<Self>(&raw) {
            Ok(mut stack) => {
                stack.sanitize();
                stack
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

    /// Guarda la pila atómicamente en `path`.
    pub fn save(&self, path: &Path) -> Result<()> {
        let mut normalized = self.clone();
        normalized.sanitize();
        let mut json = serde_json::to_vec_pretty(&normalized)?;
        json.push(b'\n');
        paths::write_atomic(path, &json)
    }

    /// Sanea entradas vacías y recorta a [`MAX_CLOSED_TABS`].
    pub fn sanitize(&mut self) {
        self.entries.retain(|e| !e.url.trim().is_empty());
        self.entries.truncate(MAX_CLOSED_TABS);
    }

    /// Inserta una pestaña cerrada al tope de la pila (ignora URLs vacías).
    /// Si supera [`MAX_CLOSED_TABS`], descarta la más antigua.
    pub fn push(&mut self, entry: ClosedTabEntry) {
        if entry.url.trim().is_empty() {
            return;
        }
        self.entries.insert(0, entry);
        self.entries.truncate(MAX_CLOSED_TABS);
    }

    /// Atajo para construir e insertar una entrada.
    pub fn push_tab(
        &mut self,
        title: impl Into<String>,
        url: impl Into<String>,
        is_split: bool,
        partner_url: Option<String>,
        closed_at_epoch: u64,
    ) {
        self.push(ClosedTabEntry::new(
            title,
            url,
            is_split,
            partner_url,
            closed_at_epoch,
        ));
    }

    /// Extrae y devuelve la pestaña cerrada más reciente.
    pub fn pop(&mut self) -> Option<ClosedTabEntry> {
        if self.entries.is_empty() {
            None
        } else {
            Some(self.entries.remove(0))
        }
    }

    /// Consulta la pestaña cerrada más reciente sin extraerla.
    pub fn peek(&self) -> Option<&ClosedTabEntry> {
        self.entries.first()
    }

    /// Lista todas las pestañas cerradas en orden más-reciente-primero.
    pub fn list(&self) -> &[ClosedTabEntry] {
        &self.entries
    }

    /// Vacía la pila por completo.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Elimina entradas cerradas en o después de `cutoff_epoch` (para borrar datos recientes).
    pub fn remove_since(&mut self, cutoff_epoch: u64) -> usize {
        let before = self.entries.len();
        self.entries.retain(|e| e.closed_at_epoch < cutoff_epoch);
        before.saturating_sub(self.entries.len())
    }

    /// Número de entradas actuales.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Indica si la pila está vacía.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir()
            .join(format!("search-closed-tabs-{name}-{nanos}"))
            .join("closed-tabs.json")
    }

    #[test]
    fn push_pop_peek_order_and_split_fields() {
        let mut stack = ClosedTabsStack::new();
        assert!(stack.is_empty());

        stack.push_tab("First", "https://one.example", false, None, 100);
        stack.push_tab(
            "Split Left",
            "https://left.example",
            true,
            Some("https://right.example".into()),
            200,
        );

        assert_eq!(stack.len(), 2);
        let top = stack.peek().unwrap();
        assert_eq!(top.url, "https://left.example");
        assert!(top.is_split);
        assert_eq!(top.partner_url.as_deref(), Some("https://right.example"));

        let popped = stack.pop().unwrap();
        assert_eq!(popped.title, "Split Left");
        assert_eq!(stack.len(), 1);
        assert_eq!(stack.peek().unwrap().url, "https://one.example");

        let last = stack.pop().unwrap();
        assert_eq!(last.url, "https://one.example");
        assert!(stack.pop().is_none());
    }

    #[test]
    fn cap_at_50_drops_oldest() {
        let mut stack = ClosedTabsStack::new();
        for i in 0..60u64 {
            stack.push_tab(
                format!("Tab {i}"),
                format!("https://example.com/{i}"),
                false,
                None,
                i,
            );
        }
        assert_eq!(stack.len(), MAX_CLOSED_TABS);
        assert_eq!(stack.list()[0].url, "https://example.com/59");
        assert_eq!(stack.list()[49].url, "https://example.com/10");
    }

    #[test]
    fn clear_and_remove_since() {
        let mut stack = ClosedTabsStack::new();
        stack.push_tab("Old", "https://old.example", false, None, 1000);
        stack.push_tab("Recent", "https://recent.example", false, None, 5000);

        let removed = stack.remove_since(3000);
        assert_eq!(removed, 1);
        assert_eq!(stack.len(), 1);
        assert_eq!(stack.peek().unwrap().url, "https://old.example");

        stack.clear();
        assert!(stack.is_empty());
    }

    #[test]
    fn ignores_empty_url_and_persists_atomically() {
        let path = temp_path("persist");
        let mut stack = ClosedTabsStack::new();
        stack.push_tab("Empty", "   ", false, None, 1);
        assert!(stack.is_empty());

        stack.push_tab("", "https://saved.example", false, None, 42);
        stack.save(&path).unwrap();

        let loaded = ClosedTabsStack::load(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.peek().unwrap().title, "https://saved.example");

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn corrupt_json_creates_bak_and_starts_empty() {
        let path = temp_path("corrupt");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{broken json").unwrap();

        let loaded = ClosedTabsStack::load(&path);
        assert!(loaded.is_empty());
        let bak = path.with_extension("json.bak");
        assert!(bak.exists());

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
