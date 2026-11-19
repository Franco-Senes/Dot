//! Notas rápidas ("Quick Scratchpad Notes") por espacio de trabajo o globales,
//! con fijado (`pinned`), búsqueda y persistencia atómica en `notes.json`.

use crate::paths;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NOTE_SEQ: AtomicU64 = AtomicU64::new(1);

fn generate_note_id() -> String {
    let seq = NOTE_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("note-{:x}-{:x}-{}", std::process::id(), nanos, seq)
}

/// Nota rápida del bloc lateral del navegador.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub created_at_epoch: u64,
    #[serde(default)]
    pub updated_at_epoch: u64,
}

/// Almacén en memoria y en disco de notas rápidas (`notes.json`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NotesStore {
    pub notes: Vec<Note>,
}

impl NotesStore {
    /// Crea un almacén de notas vacío.
    pub fn new() -> Self {
        Self::default()
    }

    /// Carga `notes.json`. Si está corrupto crea `.bak` y devuelve vacío.
    pub fn load(path: &Path) -> Self {
        if !path.exists() {
            return Self::default();
        }
        let raw = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(err) => {
                crate::warn!(
                    "No se pudo leer '{}': {err}. Arrancando sin notas.",
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

    /// Guarda las notas atómicamente en `path`.
    pub fn save(&self, path: &Path) -> Result<()> {
        let mut normalized = self.clone();
        normalized.sanitize();
        let mut json = serde_json::to_vec_pretty(&normalized)?;
        json.push(b'\n');
        paths::write_atomic(path, &json)
    }

    /// Sanea IDs duplicados o vacíos y normaliza `workspace_id`.
    pub fn sanitize(&mut self) {
        let mut seen = HashSet::new();
        self.notes
            .retain(|n| !n.id.trim().is_empty() && seen.insert(n.id.clone()));
        for note in &mut self.notes {
            let trimmed_title = note.title.trim();
            note.title = if trimmed_title.is_empty() {
                "Nota sin título".to_string()
            } else {
                trimmed_title.to_string()
            };
            if let Some(ref ws) = note.workspace_id {
                if ws.trim().is_empty() {
                    note.workspace_id = None;
                }
            }
        }
    }

    /// Crea una nueva nota (global si `workspace_id` es `None`).
    pub fn create(
        &mut self,
        title: impl Into<String>,
        body: impl Into<String>,
        workspace_id: Option<String>,
        now_epoch: u64,
    ) -> &Note {
        let raw_title = title.into();
        let clean_title = if raw_title.trim().is_empty() {
            "Nota sin título".to_string()
        } else {
            raw_title.trim().to_string()
        };
        let clean_ws = workspace_id
            .map(|w| w.trim().to_string())
            .filter(|w| !w.is_empty());

        let note = Note {
            id: generate_note_id(),
            title: clean_title,
            body: body.into(),
            workspace_id: clean_ws,
            pinned: false,
            created_at_epoch: now_epoch,
            updated_at_epoch: now_epoch,
        };
        self.notes.push(note);
        &self.notes[self.notes.len() - 1]
    }

    /// Actualiza título y/o cuerpo de una nota existente.
    pub fn update(
        &mut self,
        id: &str,
        title: Option<String>,
        body: Option<String>,
        now_epoch: u64,
    ) -> Result<&Note> {
        let pos = self
            .notes
            .iter()
            .position(|n| n.id == id)
            .ok_or_else(|| anyhow::anyhow!("Nota no encontrada: {id}"))?;
        if let Some(new_title) = title {
            let trimmed = new_title.trim();
            if trimmed.is_empty() {
                bail!("El título de la nota no puede estar vacío");
            }
            self.notes[pos].title = trimmed.to_string();
        }
        if let Some(new_body) = body {
            self.notes[pos].body = new_body;
        }
        self.notes[pos].updated_at_epoch = now_epoch;
        Ok(&self.notes[pos])
    }

    /// Elimina una nota por ID.
    pub fn delete(&mut self, id: &str) -> Result<Note> {
        let pos = self
            .notes
            .iter()
            .position(|n| n.id == id)
            .ok_or_else(|| anyhow::anyhow!("Nota no encontrada: {id}"))?;
        Ok(self.notes.remove(pos))
    }

    /// Marca o desmarca una nota como fijada (`pinned`).
    pub fn set_pinned(&mut self, id: &str, pinned: bool, now_epoch: u64) -> Result<()> {
        let note = self
            .notes
            .iter_mut()
            .find(|n| n.id == id)
            .ok_or_else(|| anyhow::anyhow!("Nota no encontrada: {id}"))?;
        note.pinned = pinned;
        note.updated_at_epoch = now_epoch;
        Ok(())
    }

    /// Lista notas para un espacio de trabajo (o globales si `workspace_id` es `None`).
    /// Cuando `workspace_id` es `Some(ws)`, incluye las de `ws` y las globales (`None`).
    /// Ordena fijadas primero y luego por `updated_at_epoch` descendente.
    pub fn list_for_workspace(&self, workspace_id: Option<&str>) -> Vec<&Note> {
        let mut out: Vec<&Note> = self
            .notes
            .iter()
            .filter(|n| match workspace_id {
                Some(ws) => n.workspace_id.as_deref() == Some(ws) || n.workspace_id.is_none(),
                None => n.workspace_id.is_none(),
            })
            .collect();
        out.sort_by(|a, b| {
            b.pinned
                .cmp(&a.pinned)
                .then_with(|| b.updated_at_epoch.cmp(&a.updated_at_epoch))
        });
        out
    }

    /// Busca notas por subcadena (insensible a mayúsculas) en título o cuerpo.
    pub fn search(&self, query: &str, workspace_id: Option<&str>) -> Vec<&Note> {
        let needle = query.trim().to_ascii_lowercase();
        let base = self.list_for_workspace(workspace_id);
        if needle.is_empty() {
            return base;
        }
        base.into_iter()
            .filter(|n| {
                n.title.to_ascii_lowercase().contains(&needle)
                    || n.body.to_ascii_lowercase().contains(&needle)
            })
            .collect()
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
            .join(format!("search-notes-{name}-{nanos}"))
            .join("notes.json")
    }

    #[test]
    fn create_update_pin_search_and_delete() {
        let mut store = NotesStore::new();
        let id_global = store
            .create("Global Todo", "Check Rust release", None, 100)
            .id
            .clone();
        let id_ws1 = store
            .create("API Ideas", "Design omnibox", Some("ws-1".into()), 200)
            .id
            .clone();
        let _id_ws2 = store
            .create("Other", "Unrelated note", Some("ws-2".into()), 300)
            .id
            .clone();

        store.set_pinned(&id_global, true, 250).unwrap();
        let ws1_notes = store.list_for_workspace(Some("ws-1"));
        assert_eq!(ws1_notes.len(), 2);
        assert_eq!(ws1_notes[0].id, id_global);
        assert_eq!(ws1_notes[1].id, id_ws1);

        let hits = store.search("omnibox", Some("ws-1"));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, id_ws1);

        store
            .update(&id_ws1, Some("Omnibox Spec".into()), None, 400)
            .unwrap();
        assert_eq!(store.search("spec", Some("ws-1")).len(), 1);

        store.delete(&id_global).unwrap();
        assert_eq!(store.list_for_workspace(None).len(), 0);
    }

    #[test]
    fn save_load_and_corrupt_bak() {
        let path = temp_path("persist");
        let mut store = NotesStore::new();
        store.create("Saved", "Body text", None, 123);
        store.save(&path).unwrap();

        let loaded = NotesStore::load(&path);
        assert_eq!(loaded.notes.len(), 1);
        assert_eq!(loaded.notes[0].body, "Body text");

        fs::write(&path, "{bad").unwrap();
        let recovered = NotesStore::load(&path);
        assert!(recovered.notes.is_empty());
        assert!(path.with_extension("json.bak").exists());

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
