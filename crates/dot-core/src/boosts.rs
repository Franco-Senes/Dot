//! Arc Boosts por sitio (origen): tipografía, escala de texto, presets de contraste,
//! ocultación de imágenes y CSS personalizado saneado, con persistencia atómica en `boosts.json`.

use crate::paths;
use crate::site_prefs;
use anyhow::Result;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Escala mínima de fuente permitida (%).
pub const MIN_FONT_SCALE_PCT: u16 = 80;
/// Escala máxima de fuente permitida (%).
pub const MAX_FONT_SCALE_PCT: u16 = 150;
/// Escala de fuente por defecto (%).
pub const DEFAULT_FONT_SCALE_PCT: u16 = 100;
/// Límite máximo de bytes para CSS personalizado por sitio.
pub const MAX_CUSTOM_CSS_BYTES: usize = 64 * 1024;

static END_STYLE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)</\s*style").unwrap_or_else(|_| unreachable!()));
static END_SCRIPT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)</\s*script").unwrap_or_else(|_| unreachable!()));

/// Familia tipográfica forzada para un sitio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoostFont {
    Sans,
    Serif,
    Mono,
}

/// Preset visual de color/contraste para un sitio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContrastPreset {
    #[default]
    Normal,
    HighContrast,
    Sepia,
    InvertDark,
}

/// Configuración de Boost almacenada para un origen (`scheme://host[:port]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteBoost {
    pub origin: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_override: Option<BoostFont>,
    #[serde(default = "default_font_scale_pct")]
    pub font_scale_pct: u16,
    #[serde(default)]
    pub contrast_preset: ContrastPreset,
    #[serde(default)]
    pub hide_images: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub custom_css: String,
}

fn default_font_scale_pct() -> u16 {
    DEFAULT_FONT_SCALE_PCT
}

impl SiteBoost {
    /// Crea un Boost por defecto (desactivado) para un origen ya normalizado.
    pub fn new(origin: impl Into<String>) -> Self {
        Self {
            origin: origin.into(),
            enabled: false,
            font_override: None,
            font_scale_pct: DEFAULT_FONT_SCALE_PCT,
            contrast_preset: ContrastPreset::Normal,
            hide_images: false,
            custom_css: String::new(),
        }
    }

    /// Devuelve `true` si todos los campos están en sus valores por defecto.
    pub fn is_default(&self) -> bool {
        !self.enabled
            && self.font_override.is_none()
            && self.font_scale_pct == DEFAULT_FONT_SCALE_PCT
            && self.contrast_preset == ContrastPreset::Normal
            && !self.hide_images
            && self.custom_css.trim().is_empty()
    }

    /// Normaliza escala y CSS personalizado del Boost.
    pub fn sanitize(&mut self) {
        self.font_scale_pct = self
            .font_scale_pct
            .clamp(MIN_FONT_SCALE_PCT, MAX_FONT_SCALE_PCT);
        self.custom_css = sanitize_custom_css(&self.custom_css);
    }
}

/// Normaliza una URL o cadena de origen a `scheme://host[:port]`.
pub fn normalize_origin(input: &str) -> Option<String> {
    site_prefs::normalize_origin(input)
}

/// Sanea CSS personalizado evitando cierres prematuros `</style>` / `</script>` y bytes NUL.
pub fn sanitize_custom_css(raw: &str) -> String {
    let mut clean: String = raw.chars().filter(|&c| c != '\0').collect();
    // Iteramos por si hubiera intentos anidados tipo `</st</style>yle>`.
    for _ in 0..4 {
        let next = END_STYLE_RE
            .replace_all(&clean, "/* blocked-end-style */")
            .into_owned();
        let next = END_SCRIPT_RE
            .replace_all(&next, "/* blocked-end-script */")
            .into_owned();
        if next == clean {
            break;
        }
        clean = next;
    }
    if clean.len() > MAX_CUSTOM_CSS_BYTES {
        let mut end = MAX_CUSTOM_CSS_BYTES;
        while end > 0 && !clean.is_char_boundary(end) {
            end -= 1;
        }
        clean.truncate(end);
    }
    clean.trim().to_string()
}

/// Compila un [`SiteBoost`] en CSS inyectable listo para la página web.
/// Devuelve cadena vacía si `boost.enabled` es `false`.
pub fn compile_boost_css(boost: &SiteBoost) -> String {
    if !boost.enabled {
        return String::new();
    }

    let mut out = String::new();

    if let Some(font) = boost.font_override {
        let family = match font {
            BoostFont::Sans => {
                r#"system-ui, -apple-system, "Segoe UI", Roboto, "Noto Sans", sans-serif"#
            }
            BoostFont::Serif => r#"Georgia, "Liberation Serif", "Noto Serif", serif"#,
            BoostFont::Mono => {
                r#""JetBrains Mono", "Fira Code", "Liberation Mono", ui-monospace, monospace"#
            }
        };
        if matches!(font, BoostFont::Mono) {
            out.push_str(&format!(
                "html, body, p, div, span, li, td, th, h1, h2, h3, h4, h5, h6, input, textarea, select, button, code, pre {{ font-family: {family} !important; }}\n"
            ));
        } else {
            out.push_str(&format!(
                "html, body, p, div, span, li, td, th, h1, h2, h3, h4, h5, h6, input, textarea, select, button {{ font-family: {family} !important; }}\n"
            ));
        }
    }

    let scale = boost
        .font_scale_pct
        .clamp(MIN_FONT_SCALE_PCT, MAX_FONT_SCALE_PCT);
    if scale != DEFAULT_FONT_SCALE_PCT {
        out.push_str(&format!("html {{ font-size: {scale}% !important; }}\n"));
    }

    match boost.contrast_preset {
        ContrastPreset::Normal => {}
        ContrastPreset::HighContrast => {
            out.push_str("html { filter: contrast(1.25) saturate(1.1) !important; }\n");
        }
        ContrastPreset::Sepia => {
            out.push_str("html { filter: sepia(0.35) contrast(0.95) !important; }\n");
        }
        ContrastPreset::InvertDark => {
            out.push_str(
                "html { filter: invert(0.92) hue-rotate(180deg) !important; background-color: #121212 !important; }\n\
                 img, video, picture, svg, canvas { filter: invert(1) hue-rotate(180deg) !important; }\n",
            );
        }
    }

    if boost.hide_images {
        out.push_str("img, picture, svg, video, [role=\"img\"] { display: none !important; }\n");
    }

    let custom = sanitize_custom_css(&boost.custom_css);
    if !custom.is_empty() {
        out.push_str(&custom);
        out.push('\n');
    }

    out
}

/// Almacén persistente de Boosts por origen (`boosts.json`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BoostStore {
    pub boosts: BTreeMap<String, SiteBoost>,
}

impl BoostStore {
    /// Crea un almacén vacío.
    pub fn new() -> Self {
        Self::default()
    }

    /// Carga `boosts.json`. Si está corrupto crea respaldo `.bak` y devuelve vacío.
    pub fn load(path: &Path) -> Self {
        if !path.exists() {
            return Self::default();
        }
        let raw = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(err) => {
                crate::warn!(
                    "No se pudo leer '{}': {err}. Arrancando sin Boosts.",
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

    /// Guarda los Boosts atómicamente en `path`.
    pub fn save(&self, path: &Path) -> Result<()> {
        let mut normalized = self.clone();
        normalized.sanitize();
        let mut json = serde_json::to_vec_pretty(&normalized)?;
        json.push(b'\n');
        paths::write_atomic(path, &json)
    }

    /// Sanea orígenes, acota escalas (`80..=150`), limpia CSS y descarta entradas por defecto.
    pub fn sanitize(&mut self) {
        let mut clean = BTreeMap::new();
        for (key, mut boost) in std::mem::take(&mut self.boosts) {
            if let Some(norm) = normalize_origin(&boost.origin).or_else(|| normalize_origin(&key)) {
                boost.origin = norm.clone();
                boost.sanitize();
                if !boost.is_default() {
                    clean.insert(norm, boost);
                }
            }
        }
        self.boosts = clean;
    }

    /// Obtiene el Boost almacenado para una URL u origen.
    pub fn get(&self, origin_or_url: &str) -> Option<&SiteBoost> {
        let origin = normalize_origin(origin_or_url)?;
        self.boosts.get(&origin)
    }

    /// Devuelve el Boost almacenado o uno por defecto para el origen normalizado.
    pub fn get_or_default(&self, origin_or_url: &str) -> Option<SiteBoost> {
        let origin = normalize_origin(origin_or_url)?;
        Some(
            self.boosts
                .get(&origin)
                .cloned()
                .unwrap_or_else(|| SiteBoost::new(origin)),
        )
    }

    /// Inserta o actualiza el Boost de un origen (lo elimina si queda por defecto).
    pub fn upsert(&mut self, mut boost: SiteBoost) -> Result<()> {
        let origin = normalize_origin(&boost.origin)
            .ok_or_else(|| anyhow::anyhow!("Origen o URL inválido: {}", boost.origin))?;
        boost.origin = origin.clone();
        boost.sanitize();
        if boost.is_default() {
            self.boosts.remove(&origin);
        } else {
            self.boosts.insert(origin, boost);
        }
        Ok(())
    }

    /// Restaura (elimina) el Boost de un origen. Devuelve `true` si existía.
    pub fn reset_site(&mut self, origin_or_url: &str) -> bool {
        let Some(origin) = normalize_origin(origin_or_url) else {
            return false;
        };
        self.boosts.remove(&origin).is_some()
    }

    /// Elimina todos los Boosts guardados.
    pub fn clear(&mut self) {
        self.boosts.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_file(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "search_boosts_test_{name}_{}_{}.json",
            std::process::id(),
            nanos
        ))
    }

    #[test]
    fn sanitize_blocks_style_and_script_breakouts() {
        let malicious = "body { color: red; } </style><script>alert(1)</script> /* </StYlE > */";
        let sanitized = sanitize_custom_css(malicious);
        assert!(!sanitized.to_ascii_lowercase().contains("</style"));
        assert!(!sanitized.to_ascii_lowercase().contains("</script"));

        let nested = "a { } </st</style>yle> b { }";
        let sanitized_nested = sanitize_custom_css(nested);
        assert!(!sanitized_nested.to_ascii_lowercase().contains("</style"));

        let with_nul = "body\0 { margin: 0; }";
        assert_eq!(sanitize_custom_css(with_nul), "body { margin: 0; }");
    }

    #[test]
    fn compile_css_respects_enabled_and_all_options() {
        let mut boost = SiteBoost::new("https://example.com");
        boost.font_override = Some(BoostFont::Serif);
        boost.font_scale_pct = 120;
        boost.contrast_preset = ContrastPreset::InvertDark;
        boost.hide_images = true;
        boost.custom_css = "main { max-width: 72ch; }".to_string();

        // Desactivado => vacío
        assert!(compile_boost_css(&boost).is_empty());

        boost.enabled = true;
        let css = compile_boost_css(&boost);
        assert!(css.contains("Liberation Serif"));
        assert!(css.contains("font-size: 120% !important"));
        assert!(css.contains("invert(0.92) hue-rotate(180deg)"));
        assert!(css.contains("display: none !important"));
        assert!(css.contains("main { max-width: 72ch; }"));
    }

    #[test]
    fn store_roundtrip_clamp_and_corrupt_backup() {
        let path = temp_file("roundtrip");
        let mut store = BoostStore::new();

        let mut boost = SiteBoost::new("HTTPS://Docs.Rust-Lang.org/book/ch01.html");
        boost.enabled = true;
        boost.font_override = Some(BoostFont::Mono);
        boost.font_scale_pct = 999;
        boost.contrast_preset = ContrastPreset::Sepia;
        boost.custom_css = "p { line-height: 1.8; }</style>".to_string();

        store.upsert(boost).expect("upsert valido");
        store.save(&path).expect("guardado atomico");

        let loaded = BoostStore::load(&path);
        let saved = loaded
            .get("https://docs.rust-lang.org/other")
            .expect("origen normalizado");
        assert_eq!(saved.origin, "https://docs.rust-lang.org");
        assert_eq!(saved.font_scale_pct, MAX_FONT_SCALE_PCT);
        assert!(!saved.custom_css.to_ascii_lowercase().contains("</style"));

        // Corrupto -> crea .bak y arranca vacío
        fs::write(&path, "{corrupt json").expect("escribir corrupto");
        let recovered = BoostStore::load(&path);
        assert!(recovered.boosts.is_empty());
        let bak_path = path.with_extension("json.bak");
        assert!(bak_path.exists());

        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&bak_path);
    }

    #[test]
    fn reset_site_removes_entry() {
        let mut store = BoostStore::new();
        let mut boost = SiteBoost::new("https://example.org");
        boost.enabled = true;
        store.upsert(boost).expect("upsert");
        assert!(store.get("https://example.org/foo").is_some());
        assert!(store.reset_site("https://example.org/bar"));
        assert!(store.get("https://example.org").is_none());
    }
}
