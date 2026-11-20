//! Modelo y persistencia atómica de carpetas de pestañas fijadas estilo Arc (`PinFolders.swift`).
//!
//! - Hasta 3 niveles de anidación (`MAX_PIN_FOLDER_DEPTH = 3`).
//! - Prevención de ciclos y saneamiento automático al cargar.
//! - Persistencia atómica en `$XDG_DATA_HOME/search/pin-folders.json` con respaldo `.bak` ante JSON corrupto.

use crate::paths;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Profundidad máxima permitida (carpeta -> subcarpeta -> sub-subcarpeta).
pub const MAX_PIN_FOLDER_DEPTH: usize = 3;

static FOLDER_SEQ: AtomicU64 = AtomicU64::new(1);

fn generate_folder_id() -> String {
    let seq = FOLDER_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("pf-{:x}-{:x}-{}", std::process::id(), nanos, seq)
}

/// Carpeta de filas fijadas en la barra lateral vertical estilo Arc.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinFolder {
    pub id: String,
    pub name: String,
    #[serde(default, rename = "parentID", skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub collapsed: bool,
}

/// Almacén en memoria y en disco de carpetas de pines (`pin-folders.json`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PinFolderStore {
    pub folders: Vec<PinFolder>,
}

impl PinFolderStore {
    /// Carga `pin-folders.json`. Si no existe devuelve vacío; si el JSON está corrupto
    /// respalda el archivo en `.bak`, registra `warn!` y devuelve un almacén limpio.
    pub fn load(path: &Path) -> Self {
        if !path.exists() {
            return Self::default();
        }
        let raw = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(err) => {
                crate::warn!(
                    "No se pudo leer '{}': {err}. Arrancando sin carpetas de pines.",
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

    /// Sanea duplicados, padres inexistentes, auto-referencias, ciclos y profundidad > 3.
    pub fn sanitize(&mut self) {
        let mut seen = HashSet::new();
        self.folders
            .retain(|f| !f.id.trim().is_empty() && seen.insert(f.id.clone()));

        let valid_ids: HashSet<String> = self.folders.iter().map(|f| f.id.clone()).collect();
        for f in &mut self.folders {
            let trimmed = f.name.trim();
            if trimmed.is_empty() {
                f.name = "Carpeta".to_string();
            } else if trimmed.len() != f.name.len() {
                f.name = trimmed.to_string();
            }
            if let Some(ref pid) = f.parent_id {
                if pid == &f.id || !valid_ids.contains(pid) {
                    f.parent_id = None;
                }
            }
        }

        // Romper cualquier ciclo recorriendo hacia arriba desde cada carpeta.
        let len = self.folders.len();
        for idx in 0..len {
            let start_id = self.folders[idx].id.clone();
            let mut visited = HashSet::new();
            visited.insert(start_id);
            let mut cur = self.folders[idx].parent_id.clone();
            while let Some(pid) = cur {
                if !visited.insert(pid.clone()) {
                    self.folders[idx].parent_id = None;
                    break;
                }
                cur = self
                    .folders
                    .iter()
                    .find(|f| f.id == pid)
                    .and_then(|f| f.parent_id.clone());
            }
        }

        // Acotar la profundidad máxima a `MAX_PIN_FOLDER_DEPTH`.
        for idx in 0..len {
            let fid = self.folders[idx].id.clone();
            let mut chain = self.ancestors(&fid);
            while chain.len() >= MAX_PIN_FOLDER_DEPTH {
                chain.remove(0);
                self.folders[idx].parent_id = chain.first().cloned();
            }
        }
    }

    pub fn contains(&self, id: &str) -> bool {
        self.folders.iter().any(|f| f.id == id)
    }

    pub fn get(&self, id: &str) -> Option<&PinFolder> {
        self.folders.iter().find(|f| f.id == id)
    }

    /// Ancestros de `id` desde el padre inmediato hasta la raíz.
    pub fn ancestors(&self, id: &str) -> Vec<String> {
        let mut chain = Vec::new();
        let mut cur = self.get(id).and_then(|f| f.parent_id.clone());
        while let Some(pid) = cur {
            if chain.contains(&pid) {
                break;
            }
            if let Some(found) = self.get(&pid) {
                chain.push(pid);
                cur = found.parent_id.clone();
            } else {
                break;
            }
        }
        chain
    }

    /// Nivel de profundidad (1 en raíz, 2 dentro de una carpeta, máximo 3).
    pub fn depth(&self, id: &str) -> usize {
        self.ancestors(id).len() + 1
    }

    /// Altura del subárbol de carpetas que cuelga de `id` (1 si no tiene subcarpetas).
    pub fn height(&self, id: &str) -> usize {
        let mut best = 1usize;
        let mut stack = vec![(id.to_string(), 1usize)];
        let mut seen = HashSet::new();
        while let Some((cur, level)) = stack.pop() {
            if !seen.insert(cur.clone()) {
                continue;
            }
            best = best.max(level);
            for child in self
                .folders
                .iter()
                .filter(|f| f.parent_id.as_deref() == Some(cur.as_str()))
            {
                stack.push((child.id.clone(), level + 1));
            }
        }
        best
    }

    /// Comprueba si se puede crear una nueva carpeta dentro de `parent_id` respetando `MAX_PIN_FOLDER_DEPTH`.
    pub fn can_create_in(&self, parent_id: Option<&str>) -> bool {
        match parent_id {
            None => true,
            Some(pid) => self.contains(pid) && self.depth(pid) < MAX_PIN_FOLDER_DEPTH,
        }
    }

    /// Comprueba si la carpeta `id` puede moverse dentro de `new_parent` sin ciclos ni exceder profundidad 3.
    pub fn can_move(&self, id: &str, new_parent: Option<&str>) -> bool {
        if !self.contains(id) {
            return false;
        }
        let Some(pid) = new_parent else {
            return true;
        };
        if !self.contains(pid) || pid == id || self.ancestors(pid).iter().any(|a| a == id) {
            return false;
        }
        self.depth(pid) + self.height(id) <= MAX_PIN_FOLDER_DEPTH
    }

    /// Crea una carpeta nueva si el nivel de anidación es válido.
    pub fn create_folder(&mut self, name: &str, parent_id: Option<&str>) -> Option<PinFolder> {
        if !self.can_create_in(parent_id) {
            return None;
        }
        let clean = name.trim();
        let final_name = if clean.is_empty() {
            "Nueva carpeta".to_string()
        } else {
            clean.to_string()
        };
        let folder = PinFolder {
            id: generate_folder_id(),
            name: final_name,
            parent_id: parent_id.map(str::to_string),
            collapsed: false,
        };
        self.folders.push(folder.clone());
        Some(folder)
    }

    pub fn rename_folder(&mut self, id: &str, new_name: &str) -> bool {
        let clean = new_name.trim();
        if clean.is_empty() {
            return false;
        }
        if let Some(f) = self.folders.iter_mut().find(|f| f.id == id) {
            f.name = clean.to_string();
            true
        } else {
            false
        }
    }

    pub fn set_collapsed(&mut self, id: &str, collapsed: bool) -> bool {
        if let Some(f) = self.folders.iter_mut().find(|f| f.id == id) {
            if f.collapsed != collapsed {
                f.collapsed = collapsed;
                return true;
            }
        }
        false
    }

    pub fn toggle_collapsed(&mut self, id: &str) -> Option<bool> {
        let f = self.folders.iter_mut().find(|f| f.id == id)?;
        f.collapsed = !f.collapsed;
        Some(f.collapsed)
    }

    pub fn move_folder(&mut self, id: &str, new_parent: Option<&str>) -> bool {
        if !self.can_move(id, new_parent) {
            return false;
        }
        if let Some(f) = self.folders.iter_mut().find(|f| f.id == id) {
            f.parent_id = new_parent.map(str::to_string);
            true
        } else {
            false
        }
    }

    /// Elimina la carpeta `id` y sube sus subcarpetas directas a su padre anterior.
    /// Devuelve `Some(parent_id_anterior)` para que las pestañas fijadas en `id` también suban allí.
    pub fn remove_folder(&mut self, id: &str) -> Option<Option<String>> {
        let idx = self.folders.iter().position(|f| f.id == id)?;
        let parent_of_removed = self.folders[idx].parent_id.clone();
        self.folders.remove(idx);
        for f in &mut self.folders {
            if f.parent_id.as_deref() == Some(id) {
                f.parent_id = parent_of_removed.clone();
            }
        }
        Some(parent_of_removed)
    }

    pub fn children_of(&self, parent_id: Option<&str>) -> Vec<PinFolder> {
        self.folders
            .iter()
            .filter(|f| f.parent_id.as_deref() == parent_id)
            .cloned()
            .collect()
    }

    /// Devuelve todas las carpetas en orden jerárquico DFS con su profundidad (`0..=2`).
    pub fn flattened(&self) -> Vec<(PinFolder, usize)> {
        let mut out = Vec::with_capacity(self.folders.len());
        self.walk_dfs(None, 0, &mut out);
        out
    }

    fn walk_dfs(&self, parent: Option<&str>, depth: usize, out: &mut Vec<(PinFolder, usize)>) {
        if depth >= MAX_PIN_FOLDER_DEPTH {
            return;
        }
        for folder in self
            .folders
            .iter()
            .filter(|f| f.parent_id.as_deref() == parent)
        {
            out.push((folder.clone(), depth));
            self.walk_dfs(Some(&folder.id), depth + 1, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::DotPaths;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_SEQ: AtomicU64 = AtomicU64::new(1);

    fn temp_pin_folders_path(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let n = TEST_SEQ.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "search-pin-folders-test-{tag}-{}-{n}",
            std::process::id()
        ));
        let sp = DotPaths::with_base(&root);
        sp.ensure_dirs().expect("ensure_dirs");
        (root, sp.pin_folders_file())
    }

    #[test]
    fn enforces_max_three_nesting_levels_and_prevents_cycles() {
        let mut store = PinFolderStore::default();
        let l1 = store.create_folder("Trabajo", None).expect("level 1");
        let l2 = store
            .create_folder("Proyectos", Some(&l1.id))
            .expect("level 2");
        let l3 = store.create_folder("Rust", Some(&l2.id)).expect("level 3");
        // 4º nivel prohibido.
        assert!(!store.can_create_in(Some(&l3.id)));
        assert!(store
            .create_folder("Demasiado hondo", Some(&l3.id))
            .is_none());

        // Mover una carpeta dentro de sí misma o de un descendiente debe fallar.
        assert!(!store.can_move(&l1.id, Some(&l1.id)));
        assert!(!store.can_move(&l1.id, Some(&l2.id)));
        assert!(!store.can_move(&l1.id, Some(&l3.id)));
        assert!(!store.move_folder(&l1.id, Some(&l3.id)));

        // Orden DFS correcto.
        let flat = store.flattened();
        assert_eq!(flat.len(), 3);
        assert_eq!((flat[0].0.name.as_str(), flat[0].1), ("Trabajo", 0));
        assert_eq!((flat[1].0.name.as_str(), flat[1].1), ("Proyectos", 1));
        assert_eq!((flat[2].0.name.as_str(), flat[2].1), ("Rust", 2));
    }

    #[test]
    fn removing_folder_promotes_child_folders_to_parent() {
        let mut store = PinFolderStore::default();
        let root = store.create_folder("Root", None).expect("root");
        let mid = store.create_folder("Mid", Some(&root.id)).expect("mid");
        let leaf = store.create_folder("Leaf", Some(&mid.id)).expect("leaf");

        let new_parent = store.remove_folder(&mid.id).expect("removed");
        assert_eq!(new_parent.as_deref(), Some(root.id.as_str()));
        assert!(!store.contains(&mid.id));
        assert_eq!(
            store.get(&leaf.id).and_then(|f| f.parent_id.as_deref()),
            Some(root.id.as_str())
        );
    }

    #[test]
    fn sanitize_breaks_cycles_and_dangling_parents() {
        let mut store = PinFolderStore {
            folders: vec![
                PinFolder {
                    id: "a".to_string(),
                    name: "A".to_string(),
                    parent_id: Some("b".to_string()),
                    collapsed: false,
                },
                PinFolder {
                    id: "b".to_string(),
                    name: "B".to_string(),
                    parent_id: Some("a".to_string()),
                    collapsed: false,
                },
                PinFolder {
                    id: "c".to_string(),
                    name: "  C  ".to_string(),
                    parent_id: Some("missing".to_string()),
                    collapsed: true,
                },
            ],
        };
        store.sanitize();
        assert_eq!(store.get("c").expect("c").name, "C");
        assert!(store.get("c").expect("c").parent_id.is_none());
        // Al menos una arista del ciclo se ha cortado.
        assert!(
            store.get("a").expect("a").parent_id.is_none()
                || store.get("b").expect("b").parent_id.is_none()
        );
    }

    #[test]
    fn roundtrip_and_corrupt_json_bak_recovery() {
        let (root, file) = temp_pin_folders_path("roundtrip");
        let mut store = PinFolderStore::default();
        let work = store.create_folder("Work", None).expect("work");
        let _sub = store.create_folder("Docs", Some(&work.id)).expect("docs");
        store.toggle_collapsed(&work.id);

        store.save(&file).expect("save");
        let loaded = PinFolderStore::load(&file);
        assert_eq!(loaded, store);
        assert!(loaded.get(&work.id).expect("work").collapsed);

        // Corrupción -> respaldo .bak + almacén limpio.
        paths::write_atomic(&file, b"{broken pin-folders json").expect("write broken");
        let recovered = PinFolderStore::load(&file);
        assert!(recovered.folders.is_empty());
        assert!(file.with_extension("json.bak").exists());

        let _ = fs::remove_dir_all(&root);
    }
}
