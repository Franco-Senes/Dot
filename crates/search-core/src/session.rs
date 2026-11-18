//! Persistencia atómica de sesiones (`$XDG_DATA_HOME/search/session.json`),
//! pestañas fijadas, múltiples ventanas, splits (preparado para Fase 6) y
//! pila LIFO de pestañas cerradas (`Ctrl+Shift+T`).
//!
//! Coincide con el esquema JSON documentado en `PORTING.md` §4 y `Session.swift`.

use crate::paths;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Capacidad mínima garantizada para la pila de pestañas cerradas (`Ctrl+Shift+T`).
pub const DEFAULT_CLOSED_TAB_CAPACITY: usize = 32;

/// Entrada persistente de una pestaña en `session.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionTab {
    pub url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<String>,
    #[serde(default, rename = "groupID", skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub touched: Option<f64>,
    #[serde(default)]
    pub muted: bool,
}

impl SessionTab {
    pub fn new(url: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            title: title.into(),
            pin: None,
            name: None,
            home: None,
            group_id: None,
            touched: Some(now_unix_secs()),
            muted: false,
        }
    }

    pub fn is_pinned(&self) -> bool {
        self.pin.is_some()
    }

    /// Calcula una letra compacta por defecto para una pestaña fijada (estilo `Pins.swift`).
    pub fn default_pin_label(title: &str, url: &str) -> String {
        let from_title = title
            .trim()
            .chars()
            .find(|c| c.is_alphanumeric())
            .map(|c| c.to_uppercase().to_string());
        if let Some(ch) = from_title {
            return ch;
        }
        if let Ok(parsed) = url::Url::parse(url) {
            if let Some(host) = parsed.host_str() {
                let bare = host.strip_prefix("www.").unwrap_or(host);
                if let Some(ch) = bare.chars().find(|c| c.is_alphanumeric()) {
                    return ch.to_uppercase().to_string();
                }
            }
        }
        "•".to_string()
    }
}

/// Grupo de pestañas conservado por compatibilidad con el esquema de `Session.swift`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabGroup {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default)]
    pub collapsed: bool,
}

/// Eje de división de Split View (preparado para Fase 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitAxis {
    #[default]
    Horizontal,
    Vertical,
}

/// Par de pestañas divididas en Split View (índices relativos a `tabs`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionSplit {
    pub tabs: Vec<usize>,
    #[serde(default)]
    pub axis: SplitAxis,
    #[serde(default)]
    pub sizes: Vec<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused: Option<usize>,
}

/// Estado persistente de una ventana individual (usado tanto en `windows` como en memoria).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionWindow {
    pub tabs: Vec<SessionTab>,
    pub active: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidebar: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<TabGroup>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub splits: Vec<SessionSplit>,
}

impl SessionWindow {
    pub fn clamped_active(&self) -> usize {
        if self.tabs.is_empty() {
            0
        } else {
            self.active.min(self.tabs.len() - 1)
        }
    }
}

/// Esquema raíz de `$XDG_DATA_HOME/search/session.json` (`PORTING.md` §4).
///
/// La ventana principal vive en la raíz (`tabs`, `active`, `sidebar`, `groups`, `splits`)
/// para mantener compatibilidad directa con el esquema del original; las ventanas adicionales
/// se almacenan en `windows`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionState {
    pub tabs: Vec<SessionTab>,
    pub active: usize,
    pub sidebar: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<TabGroup>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub splits: Vec<SessionSplit>,
    #[serde(default)]
    pub windows: Vec<SessionWindow>,
}

impl SessionState {
    /// Construye un `SessionState` a partir de todas las ventanas no privadas abiertas.
    pub fn from_windows(windows: Vec<SessionWindow>, sidebar: bool) -> Self {
        let mut iter = windows.into_iter().filter(|w| !w.tabs.is_empty());
        let Some(primary) = iter.next() else {
            return Self {
                sidebar,
                ..Self::default()
            };
        };

        let active = primary.clamped_active();
        let extra: Vec<SessionWindow> = iter
            .map(|mut w| {
                w.active = w.clamped_active();
                w
            })
            .collect();

        Self {
            tabs: primary.tabs,
            active,
            sidebar,
            groups: primary.groups,
            splits: primary.splits,
            windows: extra,
        }
    }

    /// Reconstruye la lista ordenada de ventanas a restaurar al arrancar.
    pub fn to_windows(&self) -> Vec<SessionWindow> {
        let mut out = Vec::new();
        if !self.tabs.is_empty() {
            out.push(SessionWindow {
                tabs: self.tabs.clone(),
                active: self.active.min(self.tabs.len().saturating_sub(1)),
                sidebar: Some(self.sidebar),
                groups: self.groups.clone(),
                splits: self.splits.clone(),
            });
        }
        for win in &self.windows {
            if !win.tabs.is_empty() {
                let mut cloned = win.clone();
                cloned.active = cloned.clamped_active();
                out.push(cloned);
            }
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty() && self.windows.iter().all(|w| w.tabs.is_empty())
    }

    /// Carga `session.json`. Si no existe devuelve sesión vacía; si el JSON está corrupto
    /// respalda el archivo en `session.json.bak`, registra `warn!` y devuelve sesión vacía limpia.
    pub fn load(path: &Path) -> Self {
        if !path.exists() {
            return Self::default();
        }

        let raw = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(err) => {
                crate::warn!(
                    "No se pudo leer la sesión '{}': {err}. Arrancando limpio.",
                    path.display()
                );
                return Self::default();
            }
        };

        match serde_json::from_str::<Self>(&raw) {
            Ok(mut state) => {
                state.normalize();
                state
            }
            Err(err) => {
                crate::warn!(
                    "Sesión JSON corrupta en '{}': {err}. Guardando copia .bak y arrancando limpio.",
                    path.display()
                );
                if let Err(bak_err) = paths::backup_corrupt_file(path) {
                    crate::warn!(
                        "No se pudo respaldar la sesión corrupta '{}': {bak_err}",
                        path.display()
                    );
                }
                Self::default()
            }
        }
    }

    /// Guarda la sesión de forma atómica en `path`.
    pub fn save(&self, path: &Path) -> Result<()> {
        let mut normalized = self.clone();
        normalized.normalize();
        let mut json = serde_json::to_vec_pretty(&normalized)?;
        json.push(b'\n');
        paths::write_atomic(path, &json)
    }

    fn normalize(&mut self) {
        if self.tabs.is_empty() {
            self.active = 0;
        } else {
            self.active = self.active.min(self.tabs.len() - 1);
        }
        for win in &mut self.windows {
            win.active = win.clamped_active();
        }
    }
}

/// Información suficiente para reabrir una pestaña cerrada con `Ctrl+Shift+T`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosedTab {
    pub url: String,
    pub title: String,
    pub pinned: bool,
    pub pin_label: Option<String>,
    pub home: Option<String>,
    pub position: usize,
    pub muted: bool,
}

/// Pila LIFO acotada de pestañas cerradas (capacidad ≥ 20).
#[derive(Debug, Clone)]
pub struct ClosedTabStack {
    max_len: usize,
    entries: VecDeque<ClosedTab>,
}

impl Default for ClosedTabStack {
    fn default() -> Self {
        Self::new(DEFAULT_CLOSED_TAB_CAPACITY)
    }
}

impl ClosedTabStack {
    pub fn new(capacity: usize) -> Self {
        let max_len = capacity.max(20);
        Self {
            max_len,
            entries: VecDeque::with_capacity(max_len),
        }
    }

    pub fn push(&mut self, tab: ClosedTab) {
        if self.entries.len() >= self.max_len {
            self.entries.pop_front();
        }
        self.entries.push_back(tab);
    }

    pub fn pop(&mut self) -> Option<ClosedTab> {
        self.entries.pop_back()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

/// Marca temporal actual en segundos Unix (compatible con `touched` en `session.json`).
pub fn now_unix_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| (d.as_secs() as f64) + f64::from(d.subsec_millis()) / 1000.0)
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::SearchPaths;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(1);

    fn temp_session_path(label: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "search-session-test-{label}-{}-{n}",
            std::process::id()
        ));
        let sp = SearchPaths::with_base(&root);
        sp.ensure_dirs().expect("ensure_dirs");
        (root, sp.session_file())
    }

    #[test]
    fn roundtrip_single_window_with_pins_groups_and_splits() {
        let (root, file) = temp_session_path("single");

        let mut pinned = SessionTab::new("https://example.com/", "Example Domain");
        pinned.pin = Some("E".to_string());
        pinned.home = Some("https://example.com/".to_string());
        pinned.touched = Some(1795032000.0);

        let mut regular = SessionTab::new("https://docs.rs/cef/latest/cef/", "cef - Rust");
        regular.muted = true;
        regular.touched = Some(1795032100.0);

        let state = SessionState {
            tabs: vec![pinned, regular],
            active: 1,
            sidebar: true,
            groups: vec![TabGroup {
                id: "grp-1".to_string(),
                title: "Docs".to_string(),
                color: Some("blue".to_string()),
                collapsed: false,
            }],
            splits: vec![SessionSplit {
                tabs: vec![0, 1],
                axis: SplitAxis::Horizontal,
                sizes: vec![0.5, 0.5],
                focused: Some(1),
            }],
            windows: Vec::new(),
        };

        state.save(&file).expect("save session");
        let text = fs::read_to_string(&file).expect("read json");
        assert!(text.contains("\"pin\": \"E\""));
        assert!(text.contains("\"sidebar\": true"));
        assert!(text.contains("\"muted\": true"));

        let loaded = SessionState::load(&file);
        assert_eq!(loaded, state);
        assert!(loaded.tabs[0].is_pinned());
        assert!(!loaded.tabs[1].is_pinned());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn roundtrip_multi_window_persistence() {
        let (root, file) = temp_session_path("multi");

        let w1 = SessionWindow {
            tabs: vec![
                SessionTab::new("https://a.example/", "A"),
                SessionTab::new("https://b.example/", "B"),
            ],
            active: 1,
            sidebar: Some(false),
            groups: Vec::new(),
            splits: Vec::new(),
        };
        let w2 = SessionWindow {
            tabs: vec![SessionTab::new("https://c.example/", "C")],
            active: 99, // Debe acotarse al índice válido 0.
            sidebar: Some(false),
            groups: Vec::new(),
            splits: Vec::new(),
        };

        let state = SessionState::from_windows(vec![w1, w2], false);
        state.save(&file).expect("save multi");

        let loaded = SessionState::load(&file);
        let restored = loaded.to_windows();
        assert_eq!(restored.len(), 2);
        assert_eq!(restored[0].tabs.len(), 2);
        assert_eq!(restored[0].active, 1);
        assert_eq!(restored[1].tabs.len(), 1);
        assert_eq!(restored[1].active, 0);
        assert_eq!(restored[1].tabs[0].url, "https://c.example/");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn corrupt_session_json_creates_bak_and_returns_clean_default() {
        let (root, file) = temp_session_path("corrupt");
        paths::write_atomic(&file, b"{not valid session json").expect("write corrupt");

        let loaded = SessionState::load(&file);
        assert!(loaded.is_empty());
        assert_eq!(loaded.active, 0);

        let bak = file.with_extension("json.bak");
        assert!(bak.exists(), "debe existir respaldo .bak");
        assert_eq!(
            fs::read_to_string(&bak).expect("read bak"),
            "{not valid session json"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn closed_tab_stack_is_lifo_and_enforces_min_20_capacity() {
        let mut stack = ClosedTabStack::new(25);
        for i in 0..30 {
            stack.push(ClosedTab {
                url: format!("https://site-{i}.example/"),
                title: format!("Site {i}"),
                pinned: false,
                pin_label: None,
                home: None,
                position: i,
                muted: false,
            });
        }

        assert_eq!(stack.len(), 25);
        assert_eq!(stack.pop().expect("latest").url, "https://site-29.example/");
        assert_eq!(
            stack.pop().expect("second latest").url,
            "https://site-28.example/"
        );

        // Un límite menor que 20 se eleva automáticamente a 20.
        let mut min_stack = ClosedTabStack::new(5);
        for i in 0..22 {
            min_stack.push(ClosedTab {
                url: format!("https://m-{i}.example/"),
                title: String::new(),
                pinned: false,
                pin_label: None,
                home: None,
                position: 0,
                muted: false,
            });
        }
        assert_eq!(min_stack.len(), 20);
    }
}
