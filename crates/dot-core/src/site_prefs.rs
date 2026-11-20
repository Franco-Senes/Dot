//! Preferencias por sitio (origen): zoom (`25..=500%`), modo oscuro forzado,
//! silencio por defecto y preset de User-Agent, con persistencia atómica en `site-prefs.json`.

use crate::paths;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use url::Url;

/// Zoom mínimo permitido por sitio (%).
pub const MIN_ZOOM_PCT: u16 = 25;
/// Zoom máximo permitido por sitio (%).
pub const MAX_ZOOM_PCT: u16 = 500;
/// Zoom por defecto (%).
pub const DEFAULT_ZOOM_PCT: u16 = 100;

/// Presets de User-Agent seleccionables por sitio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UaPreset {
    #[default]
    Default,
    ChromeLinux,
    FirefoxLinux,
    SafariMacOs,
    MobileAndroid,
}

impl UaPreset {
    /// Cadena User-Agent asociada al preset (`None` si usa el del navegador).
    pub fn as_user_agent_str(self) -> Option<&'static str> {
        match self {
            Self::Default => None,
            Self::ChromeLinux => Some(
                "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
            ),
            Self::FirefoxLinux => {
                Some("Mozilla/5.0 (X11; Linux x86_64; rv:133.0) Gecko/20100101 Firefox/133.0")
            }
            Self::SafariMacOs => Some(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15",
            ),
            Self::MobileAndroid => Some(
                "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Mobile Safari/537.36",
            ),
        }
    }
}

/// Preferencias almacenadas para un origen concreto (`scheme://host[:port]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SitePref {
    pub origin: String,
    #[serde(default = "default_zoom_pct")]
    pub zoom_pct: u16,
    #[serde(default)]
    pub force_dark: bool,
    #[serde(default)]
    pub mute_by_default: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent_preset: Option<UaPreset>,
}

fn default_zoom_pct() -> u16 {
    DEFAULT_ZOOM_PCT
}

impl SitePref {
    fn new(origin: String) -> Self {
        Self {
            origin,
            zoom_pct: DEFAULT_ZOOM_PCT,
            force_dark: false,
            mute_by_default: false,
            user_agent_preset: None,
        }
    }

    fn is_default(&self) -> bool {
        self.zoom_pct == DEFAULT_ZOOM_PCT
            && !self.force_dark
            && !self.mute_by_default
            && self.user_agent_preset.is_none()
    }
}

/// Normaliza una URL o cadena de origen a `scheme://host[:port]`.
pub fn normalize_origin(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parsed = Url::parse(trimmed)
        .or_else(|_| Url::parse(&format!("https://{trimmed}")))
        .ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    let scheme = parsed.scheme().to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    match parsed.port() {
        Some(port) => Some(format!("{scheme}://{host}:{port}")),
        None => Some(format!("{scheme}://{host}")),
    }
}

/// Almacén de preferencias por origen (`site-prefs.json`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SitePrefsStore {
    pub prefs: BTreeMap<String, SitePref>,
}

impl SitePrefsStore {
    /// Crea un almacén vacío.
    pub fn new() -> Self {
        Self::default()
    }

    /// Carga `site-prefs.json`. Si está corrupto crea `.bak` y devuelve vacío.
    pub fn load(path: &Path) -> Self {
        if !path.exists() {
            return Self::default();
        }
        let raw = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(err) => {
                crate::warn!(
                    "No se pudo leer '{}': {err}. Arrancando sin preferencias por sitio.",
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

    /// Guarda las preferencias atómicamente en `path`.
    pub fn save(&self, path: &Path) -> Result<()> {
        let mut normalized = self.clone();
        normalized.sanitize();
        let mut json = serde_json::to_vec_pretty(&normalized)?;
        json.push(b'\n');
        paths::write_atomic(path, &json)
    }

    /// Sanea orígenes, acota `zoom_pct` a `25..=500` y descarta entradas por defecto.
    pub fn sanitize(&mut self) {
        let mut clean = BTreeMap::new();
        for (key, mut pref) in std::mem::take(&mut self.prefs) {
            if let Some(norm) = normalize_origin(&pref.origin).or_else(|| normalize_origin(&key)) {
                pref.origin = norm.clone();
                pref.zoom_pct = pref.zoom_pct.clamp(MIN_ZOOM_PCT, MAX_ZOOM_PCT);
                if !pref.is_default() {
                    clean.insert(norm, pref);
                }
            }
        }
        self.prefs = clean;
    }

    /// Obtiene las preferencias para una URL u origen.
    pub fn get(&self, origin_or_url: &str) -> Option<&SitePref> {
        let origin = normalize_origin(origin_or_url)?;
        self.prefs.get(&origin)
    }

    fn get_or_insert_mut(&mut self, origin_or_url: &str) -> Result<&mut SitePref> {
        let origin = normalize_origin(origin_or_url)
            .ok_or_else(|| anyhow::anyhow!("Origen o URL inválido: {origin_or_url}"))?;
        Ok(self
            .prefs
            .entry(origin.clone())
            .or_insert_with(|| SitePref::new(origin)))
    }

    fn prune_if_default(&mut self, origin: &str) {
        if let Some(pref) = self.prefs.get(origin) {
            if pref.is_default() {
                self.prefs.remove(origin);
            }
        }
    }

    /// Fija el zoom (`25..=500%`) para un origen y devuelve el valor acotado.
    pub fn set_zoom(&mut self, origin_or_url: &str, zoom_pct: u16) -> Result<u16> {
        let clamped = zoom_pct.clamp(MIN_ZOOM_PCT, MAX_ZOOM_PCT);
        let origin = {
            let pref = self.get_or_insert_mut(origin_or_url)?;
            pref.zoom_pct = clamped;
            pref.origin.clone()
        };
        self.prune_if_default(&origin);
        Ok(clamped)
    }

    /// Activa o desactiva el modo oscuro forzado para un origen.
    pub fn set_force_dark(&mut self, origin_or_url: &str, force_dark: bool) -> Result<()> {
        let origin = {
            let pref = self.get_or_insert_mut(origin_or_url)?;
            pref.force_dark = force_dark;
            pref.origin.clone()
        };
        self.prune_if_default(&origin);
        Ok(())
    }

    /// Activa o desactiva el silencio por defecto para un origen.
    pub fn set_mute(&mut self, origin_or_url: &str, mute: bool) -> Result<()> {
        let origin = {
            let pref = self.get_or_insert_mut(origin_or_url)?;
            pref.mute_by_default = mute;
            pref.origin.clone()
        };
        self.prune_if_default(&origin);
        Ok(())
    }

    /// Fija el preset de User-Agent para un origen (`None` o `Some(UaPreset::Default)` restaura).
    pub fn set_user_agent(&mut self, origin_or_url: &str, preset: Option<UaPreset>) -> Result<()> {
        let normalized_preset = match preset {
            Some(UaPreset::Default) | None => None,
            other => other,
        };
        let origin = {
            let pref = self.get_or_insert_mut(origin_or_url)?;
            pref.user_agent_preset = normalized_preset;
            pref.origin.clone()
        };
        self.prune_if_default(&origin);
        Ok(())
    }

    /// Restaura las preferencias de un origen. Devuelve `true` si existía.
    pub fn reset_site(&mut self, origin_or_url: &str) -> bool {
        let Some(origin) = normalize_origin(origin_or_url) else {
            return false;
        };
        self.prefs.remove(&origin).is_some()
    }

    /// Elimina todas las preferencias por sitio.
    pub fn clear(&mut self) {
        self.prefs.clear();
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
            .join(format!("search-site-prefs-{name}-{nanos}"))
            .join("site-prefs.json")
    }

    #[test]
    fn zoom_clamping_and_origin_normalization() {
        let mut store = SitePrefsStore::new();
        assert_eq!(
            store.set_zoom("https://Docs.RS/some/page?x=1", 10).unwrap(),
            MIN_ZOOM_PCT
        );
        assert_eq!(
            store.get("https://docs.rs/other").unwrap().zoom_pct,
            MIN_ZOOM_PCT
        );

        assert_eq!(
            store.set_zoom("https://docs.rs", 999).unwrap(),
            MAX_ZOOM_PCT
        );
        assert_eq!(store.get("https://docs.rs").unwrap().zoom_pct, MAX_ZOOM_PCT);

        // Restoring default prunes the entry if no other flags are set.
        assert_eq!(
            store.set_zoom("https://docs.rs", DEFAULT_ZOOM_PCT).unwrap(),
            DEFAULT_ZOOM_PCT
        );
        assert!(store.get("https://docs.rs").is_none());
    }

    #[test]
    fn dark_mute_ua_and_persistence() {
        let path = temp_path("persist");
        let mut store = SitePrefsStore::new();
        store
            .set_force_dark("https://example.com:8443/path", true)
            .unwrap();
        store.set_mute("https://example.com:8443", true).unwrap();
        store
            .set_user_agent("https://example.com:8443", Some(UaPreset::FirefoxLinux))
            .unwrap();
        store.save(&path).unwrap();

        let loaded = SitePrefsStore::load(&path);
        let pref = loaded.get("https://example.com:8443/anything").unwrap();
        assert!(pref.force_dark);
        assert!(pref.mute_by_default);
        assert_eq!(pref.user_agent_preset, Some(UaPreset::FirefoxLinux));
        assert!(pref
            .user_agent_preset
            .unwrap()
            .as_user_agent_str()
            .unwrap()
            .contains("Firefox"));

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
