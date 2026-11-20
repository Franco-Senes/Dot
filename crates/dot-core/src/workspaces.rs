//! Espacios de trabajo estilo Arc (`workspaces.json` con respaldo `.bak`).
//!
//! Permite crear, renombrar, eliminar y cambiar entre espacios de trabajo,
//! asignar identificadores de color e icono y mover pestañas entre espacios.

use crate::paths;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static WORKSPACE_SEQ: AtomicU64 = AtomicU64::new(1);

fn generate_workspace_id() -> String {
    let seq = WORKSPACE_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("ws-{:x}-{:x}-{}", std::process::id(), nanos, seq)
}

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Espacio de trabajo aislado con su propia lista de pestañas e identidad visual.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    #[serde(default = "default_color_id")]
    pub color_id: String,
    #[serde(default = "default_icon_id")]
    pub icon_id: String,
    #[serde(default)]
    pub tab_urls: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_tab_index: Option<usize>,
    #[serde(default)]
    pub created_at_epoch: u64,
}

fn default_color_id() -> String {
    "slate".to_string()
}

fn default_icon_id() -> String {
    "layers".to_string()
}

/// Almacén en memoria y en disco de espacios de trabajo (`workspaces.json`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspaceStore {
    pub workspaces: Vec<Workspace>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_workspace_id: Option<String>,
}

impl WorkspaceStore {
    /// Crea un almacén vacío.
    pub fn new() -> Self {
        Self::default()
    }

    /// Carga `workspaces.json`. Si está corrupto crea `.bak` y devuelve vacío.
    pub fn load(path: &Path) -> Self {
        if !path.exists() {
            return Self::default();
        }
        let raw = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(err) => {
                crate::warn!(
                    "No se pudo leer '{}': {err}. Arrancando sin espacios de trabajo.",
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
                    "JSON corrupto en '{}': {err}. Guardando respaldo .bak y arrancando limpio.",
                    path.display()
                );
                if let Err(bak_err) = paths::backup_corrupt_file(path) {
                    crate::warn!("No se pudo crear respaldo .bak: {bak_err}");
                }
                Self::default()
            }
        }
    }

    /// Guarda el almacén atómicamente en `path`.
    pub fn save(&self, path: &Path) -> Result<()> {
        let mut normalized = self.clone();
        normalized.sanitize();
        let mut json = serde_json::to_vec_pretty(&normalized)?;
        json.push(b'\n');
        paths::write_atomic(path, &json)
    }

    /// Sanea duplicados, nombres vacíos, índices de pestaña fuera de rango y workspace activo.
    pub fn sanitize(&mut self) {
        let mut seen = HashSet::new();
        self.workspaces
            .retain(|w| !w.id.trim().is_empty() && seen.insert(w.id.clone()));

        for ws in &mut self.workspaces {
            let trimmed = ws.name.trim();
            if trimmed.is_empty() {
                ws.name = "Workspace".to_string();
            } else if trimmed.len() != ws.name.len() {
                ws.name = trimmed.to_string();
            }
            if ws.color_id.trim().is_empty() {
                ws.color_id = default_color_id();
            }
            if ws.icon_id.trim().is_empty() {
                ws.icon_id = default_icon_id();
            }
            ws.tab_urls.retain(|u| !u.trim().is_empty());
            if let Some(idx) = ws.active_tab_index {
                if idx >= ws.tab_urls.len() {
                    ws.active_tab_index = ws.tab_urls.len().checked_sub(1);
                }
            }
        }

        if let Some(ref active_id) = self.active_workspace_id {
            if !self.workspaces.iter().any(|w| &w.id == active_id) {
                self.active_workspace_id = self.workspaces.first().map(|w| w.id.clone());
            }
        } else if !self.workspaces.is_empty() {
            self.active_workspace_id = self.workspaces.first().map(|w| w.id.clone());
        }
    }

    /// Crea un nuevo espacio de trabajo y lo activa si no había ninguno activo.
    pub fn create(
        &mut self,
        name: impl Into<String>,
        color_id: impl Into<String>,
        icon_id: impl Into<String>,
    ) -> &Workspace {
        let id = generate_workspace_id();
        let raw_name = name.into();
        let clean_name = if raw_name.trim().is_empty() {
            "Workspace".to_string()
        } else {
            raw_name.trim().to_string()
        };
        let raw_color = color_id.into();
        let clean_color = if raw_color.trim().is_empty() {
            default_color_id()
        } else {
            raw_color.trim().to_string()
        };
        let raw_icon = icon_id.into();
        let clean_icon = if raw_icon.trim().is_empty() {
            default_icon_id()
        } else {
            raw_icon.trim().to_string()
        };

        let ws = Workspace {
            id: id.clone(),
            name: clean_name,
            color_id: clean_color,
            icon_id: clean_icon,
            tab_urls: Vec::new(),
            active_tab_index: None,
            created_at_epoch: now_epoch_secs(),
        };
        self.workspaces.push(ws);
        if self.active_workspace_id.is_none() {
            self.active_workspace_id = Some(id);
        }
        // Acabamos de insertar el elemento, por lo que `last()` siempre existe.
        &self.workspaces[self.workspaces.len() - 1]
    }

    /// Renombra un espacio de trabajo existente.
    pub fn rename(&mut self, id: &str, new_name: impl Into<String>) -> Result<()> {
        let name = new_name.into();
        let trimmed = name.trim();
        if trimmed.is_empty() {
            bail!("El nombre del espacio de trabajo no puede estar vacío");
        }
        let ws = self
            .workspaces
            .iter_mut()
            .find(|w| w.id == id)
            .ok_or_else(|| anyhow::anyhow!("Espacio de trabajo no encontrado: {id}"))?;
        ws.name = trimmed.to_string();
        Ok(())
    }

    /// Actualiza color e icono de un espacio de trabajo.
    pub fn set_appearance(
        &mut self,
        id: &str,
        color_id: impl Into<String>,
        icon_id: impl Into<String>,
    ) -> Result<()> {
        let ws = self
            .workspaces
            .iter_mut()
            .find(|w| w.id == id)
            .ok_or_else(|| anyhow::anyhow!("Espacio de trabajo no encontrado: {id}"))?;
        let c = color_id.into();
        if !c.trim().is_empty() {
            ws.color_id = c.trim().to_string();
        }
        let i = icon_id.into();
        if !i.trim().is_empty() {
            ws.icon_id = i.trim().to_string();
        }
        Ok(())
    }

    /// Elimina un espacio de trabajo y reasigna el activo si corresponde.
    pub fn delete(&mut self, id: &str) -> Result<Workspace> {
        let pos = self
            .workspaces
            .iter()
            .position(|w| w.id == id)
            .ok_or_else(|| anyhow::anyhow!("Espacio de trabajo no encontrado: {id}"))?;
        let removed = self.workspaces.remove(pos);
        if self.active_workspace_id.as_deref() == Some(id) {
            self.active_workspace_id = self.workspaces.first().map(|w| w.id.clone());
        }
        Ok(removed)
    }

    /// Cambia el espacio de trabajo activo.
    pub fn switch(&mut self, id: &str) -> Result<&Workspace> {
        let pos = self
            .workspaces
            .iter()
            .position(|w| w.id == id)
            .ok_or_else(|| anyhow::anyhow!("Espacio de trabajo no encontrado: {id}"))?;
        self.active_workspace_id = Some(id.to_string());
        Ok(&self.workspaces[pos])
    }

    /// Devuelve el espacio de trabajo activo actual.
    pub fn active(&self) -> Option<&Workspace> {
        let active_id = self.active_workspace_id.as_deref()?;
        self.workspaces.iter().find(|w| w.id == active_id)
    }

    /// Añade una pestaña al final de un espacio de trabajo y la marca activa.
    pub fn add_tab(&mut self, workspace_id: &str, url: impl Into<String>) -> Result<()> {
        let url_str = url.into();
        let trimmed = url_str.trim();
        if trimmed.is_empty() {
            bail!("La URL de la pestaña no puede estar vacía");
        }
        let ws = self
            .workspaces
            .iter_mut()
            .find(|w| w.id == workspace_id)
            .ok_or_else(|| anyhow::anyhow!("Espacio de trabajo no encontrado: {workspace_id}"))?;
        ws.tab_urls.push(trimmed.to_string());
        ws.active_tab_index = Some(ws.tab_urls.len() - 1);
        Ok(())
    }

    /// Mueve una pestaña desde `from_workspace_id` en `tab_index` hacia `to_workspace_id`.
    pub fn move_tab(
        &mut self,
        from_workspace_id: &str,
        tab_index: usize,
        to_workspace_id: &str,
    ) -> Result<()> {
        if from_workspace_id == to_workspace_id {
            return Ok(());
        }
        let from_pos = self
            .workspaces
            .iter()
            .position(|w| w.id == from_workspace_id)
            .ok_or_else(|| anyhow::anyhow!("Espacio origen no encontrado: {from_workspace_id}"))?;
        let to_pos = self
            .workspaces
            .iter()
            .position(|w| w.id == to_workspace_id)
            .ok_or_else(|| anyhow::anyhow!("Espacio destino no encontrado: {to_workspace_id}"))?;

        if tab_index >= self.workspaces[from_pos].tab_urls.len() {
            bail!("Índice de pestaña fuera de rango: {tab_index}");
        }

        let url = self.workspaces[from_pos].tab_urls.remove(tab_index);
        let from_len = self.workspaces[from_pos].tab_urls.len();
        self.workspaces[from_pos].active_tab_index = if from_len == 0 {
            None
        } else {
            Some(
                self.workspaces[from_pos]
                    .active_tab_index
                    .unwrap_or(0)
                    .min(from_len - 1),
            )
        };

        self.workspaces[to_pos].tab_urls.push(url);
        self.workspaces[to_pos].active_tab_index = Some(self.workspaces[to_pos].tab_urls.len() - 1);
        Ok(())
    }

    /// Lista todos los espacios de trabajo.
    pub fn list(&self) -> &[Workspace] {
        &self.workspaces
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
            .join(format!("search-workspaces-{name}-{nanos}"))
            .join("workspaces.json")
    }

    #[test]
    fn create_rename_switch_and_delete() {
        let mut store = WorkspaceStore::new();
        let id1 = store.create("Work", "blue", "briefcase").id.clone();
        let id2 = store.create("Personal", "emerald", "home").id.clone();

        assert_eq!(store.active().unwrap().id, id1);

        store.switch(&id2).unwrap();
        assert_eq!(store.active().unwrap().name, "Personal");

        store.rename(&id2, "Home").unwrap();
        store.set_appearance(&id2, "amber", "star").unwrap();
        assert_eq!(store.active().unwrap().name, "Home");
        assert_eq!(store.active().unwrap().color_id, "amber");
        assert_eq!(store.active().unwrap().icon_id, "star");

        let removed = store.delete(&id2).unwrap();
        assert_eq!(removed.id, id2);
        assert_eq!(store.active().unwrap().id, id1);
        assert_eq!(store.list().len(), 1);
    }

    #[test]
    fn move_tab_between_workspaces() {
        let mut store = WorkspaceStore::new();
        let ws1 = store.create("Dev", "indigo", "code").id.clone();
        let ws2 = store.create("Research", "purple", "book").id.clone();

        store.add_tab(&ws1, "https://docs.rs").unwrap();
        store.add_tab(&ws1, "https://crates.io").unwrap();

        store.move_tab(&ws1, 0, &ws2).unwrap();
        assert_eq!(store.workspaces[0].tab_urls, vec!["https://crates.io"]);
        assert_eq!(store.workspaces[0].active_tab_index, Some(0));
        assert_eq!(store.workspaces[1].tab_urls, vec!["https://docs.rs"]);
        assert_eq!(store.workspaces[1].active_tab_index, Some(0));
    }

    #[test]
    fn save_load_and_corrupt_bak() {
        let path = temp_path("persist");
        let mut store = WorkspaceStore::new();
        let id = store.create("Main", "cyan", "globe").id.clone();
        store.add_tab(&id, "https://example.com").unwrap();
        store.save(&path).unwrap();

        let loaded = WorkspaceStore::load(&path);
        assert_eq!(loaded.workspaces.len(), 1);
        assert_eq!(
            loaded.active().unwrap().tab_urls,
            vec!["https://example.com"]
        );

        fs::write(&path, "not valid json").unwrap();
        let recovered = WorkspaceStore::load(&path);
        assert!(recovered.workspaces.is_empty());
        assert!(path.with_extension("json.bak").exists());

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
