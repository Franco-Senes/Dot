//! Modelo declarativo y persistencia atómica de ajustes (`$XDG_CONFIG_HOME/search/settings.json`).
//! Coincide exactamente con el esquema documentado en `PORTING.md` §4 y §6.

use crate::address::SearchEngine;
use crate::chromium_flags::ChromiumConfig;
use crate::paths;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Esquema de color de la interfaz (`system`, `light` o `dark`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ColorLook {
    #[default]
    System,
    Light,
    Dark,
}

impl ColorLook {
    pub fn as_id(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub fn from_id(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "light" => Self::Light,
            "dark" => Self::Dark,
            _ => Self::System,
        }
    }
}

/// Ajustes persistentes del navegador en `$XDG_CONFIG_HOME/search/settings.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    #[serde(rename = "search.engine")]
    pub search_engine: SearchEngine,

    #[serde(rename = "search.custom")]
    pub custom_search_url: String,

    #[serde(rename = "look")]
    pub look: ColorLook,

    #[serde(rename = "sidebar")]
    pub vertical_tabs: bool,

    #[serde(rename = "sidebar.width")]
    pub sidebar_width: f64,

    #[serde(rename = "downloads.dir")]
    pub downloads_dir: String,

    #[serde(rename = "downloads.ask")]
    pub downloads_ask: bool,

    #[serde(rename = "shield")]
    pub shield_enabled: bool,

    #[serde(rename = "shield.paused")]
    pub shield_paused: Vec<String>,

    #[serde(rename = "privacy.gpc")]
    pub send_gpc: bool,

    #[serde(rename = "privacy.clickIDs")]
    pub strip_click_ids: bool,

    /// En `upstream` `sites.keep = false` significa bloquear cookies de terceros (`!sites.keep`).
    #[serde(rename = "sites.keep")]
    pub sites_keep: bool,

    #[serde(rename = "passwords.save")]
    pub vault_save: bool,

    #[serde(rename = "passwords.fill")]
    pub vault_fill: bool,

    #[serde(rename = "passwords.never")]
    pub passwords_never: Vec<String>,

    #[serde(rename = "pageZoom")]
    pub page_zoom: f64,

    #[serde(rename = "reader.fontSize")]
    pub reader_font_size: i32,

    #[serde(rename = "hardwareVideoDecode")]
    pub hardware_video_decode: bool,

    #[serde(rename = "rendererProcessLimit")]
    pub renderer_process_limit: u32,

    #[serde(rename = "update.check")]
    pub update_check: bool,

    #[serde(rename = "bench")]
    pub bench_enabled: bool,

    #[serde(rename = "extensions.experimental")]
    pub experimental_extensions: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            search_engine: SearchEngine::DuckDuckGo,
            custom_search_url: String::new(),
            look: ColorLook::System,
            vertical_tabs: false,
            sidebar_width: 220.0,
            downloads_dir: String::new(),
            downloads_ask: false,
            shield_enabled: true,
            shield_paused: Vec::new(),
            send_gpc: true,
            strip_click_ids: true,
            sites_keep: false,
            vault_save: true,
            vault_fill: true,
            passwords_never: Vec::new(),
            page_zoom: 1.0,
            reader_font_size: 18,
            hardware_video_decode: true,
            renderer_process_limit: 6,
            update_check: true,
            bench_enabled: false,
            experimental_extensions: false,
        }
    }
}

impl AppSettings {
    pub fn block_third_party_cookies(&self) -> bool {
        !self.sites_keep
    }

    pub fn set_block_third_party_cookies(&mut self, block: bool) {
        self.sites_keep = !block;
    }

    /// Construye el `ChromiumConfig` correspondiente a los ajustes actuales.
    pub fn to_chromium_config(
        &self,
        headless_software_gl: bool,
        no_sandbox: bool,
        ozone_platform: Option<String>,
    ) -> ChromiumConfig {
        ChromiumConfig {
            renderer_process_limit: (self.renderer_process_limit > 0)
                .then_some(self.renderer_process_limit),
            hardware_video_decode: self.hardware_video_decode,
            headless_software_gl,
            no_sandbox,
            ozone_platform,
            ..ChromiumConfig::default()
        }
    }

    /// Carga `settings.json` desde disco. Si no existe, devuelve los valores por defecto.
    /// Si el archivo está corrupto, guarda `<archivo>.bak`, emite `warn!` y arranca limpio.
    pub fn load(path: &Path) -> Self {
        if !path.exists() {
            return Self::default();
        }

        let raw = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(err) => {
                crate::warn!(
                    "No se pudo leer '{}': {err}. Usando ajustes por defecto.",
                    path.display()
                );
                return Self::default();
            }
        };

        match serde_json::from_str::<Self>(&raw) {
            Ok(mut parsed) => {
                parsed.clamp_invariants();
                parsed
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

    /// Guarda los ajustes de forma atómica en `path`.
    pub fn save(&self, path: &Path) -> Result<()> {
        let mut normalized = self.clone();
        normalized.clamp_invariants();
        let mut json = serde_json::to_vec_pretty(&normalized)?;
        json.push(b'\n');
        paths::write_atomic(path, &json)
    }

    fn clamp_invariants(&mut self) {
        if !self.page_zoom.is_finite() || self.page_zoom < 0.25 || self.page_zoom > 5.0 {
            self.page_zoom = 1.0;
        }
        if !self.sidebar_width.is_finite()
            || self.sidebar_width < 140.0
            || self.sidebar_width > 600.0
        {
            self.sidebar_width = 220.0;
        }
        self.reader_font_size = self.reader_font_size.clamp(12, 36);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::SearchPaths;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(1);

    fn temp_settings_path(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "search-settings-test-{tag}-{}-{n}",
            std::process::id()
        ));
        let sp = SearchPaths::with_base(&root);
        sp.ensure_dirs().expect("ensure_dirs");
        (root, sp.settings_file())
    }

    #[test]
    fn default_settings_match_privacy_and_minimalism_spec() {
        let s = AppSettings::default();
        assert_eq!(s.search_engine, SearchEngine::DuckDuckGo);
        assert_eq!(s.look, ColorLook::System);
        assert!(s.block_third_party_cookies());
        assert!(s.send_gpc);
        assert!(s.strip_click_ids);
        assert_eq!(s.page_zoom, 1.0);
        assert_eq!(s.renderer_process_limit, 6);
    }

    #[test]
    fn roundtrip_save_and_load_preserves_schema_keys() {
        let (root, file) = temp_settings_path("roundtrip");
        let mut s = AppSettings {
            search_engine: SearchEngine::Kagi,
            look: ColorLook::Dark,
            page_zoom: 1.25,
            renderer_process_limit: 4,
            ..AppSettings::default()
        };
        s.set_block_third_party_cookies(false);

        s.save(&file).expect("save settings");
        let text = fs::read_to_string(&file).expect("read json");
        assert!(text.contains("\"search.engine\": \"kagi\""));
        assert!(text.contains("\"look\": \"dark\""));
        assert!(text.contains("\"sites.keep\": true"));

        let loaded = AppSettings::load(&file);
        assert_eq!(loaded, s);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn corrupt_settings_file_creates_bak_and_returns_defaults() {
        let (root, file) = temp_settings_path("corrupt");
        paths::write_atomic(&file, b"{broken json!!!").expect("write broken");

        let loaded = AppSettings::load(&file);
        assert_eq!(loaded, AppSettings::default());

        let bak = file.with_extension("json.bak");
        assert!(bak.exists());
        let _ = fs::remove_dir_all(&root);
    }
}
