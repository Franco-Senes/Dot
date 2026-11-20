use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

/// Errors returned by dyslexia font profile validation, resolution, and serialization.
#[derive(Debug, Error)]
pub enum DyslexiaProfileError {
    #[error("invalid profile id '{0}': must be non-empty and contain only ASCII alphanumeric, '-', or '_'")]
    InvalidProfileId(String),

    #[error("invalid profile name: must not be empty")]
    InvalidProfileName,

    #[error("invalid font family '{0}': must be non-empty and free of CSS control characters")]
    InvalidFontFamily(String),

    #[error("value for '{field}' ({value}) is out of allowed range [{min}, {max}]")]
    OutOfRange {
        field: &'static str,
        value: f32,
        min: f32,
        max: f32,
    },

    #[error("invalid hex color '{0}': expected #RGB, #RRGGBB, or #RRGGBBAA")]
    InvalidHexColor(String),

    #[error("invalid host pattern '{0}'")]
    InvalidHostPattern(String),

    #[error("invalid URL '{0}': {1}")]
    InvalidUrl(String, String),

    #[error("profile '{0}' not found")]
    ProfileNotFound(String),

    #[error("duplicate profile id '{0}'")]
    DuplicateProfileId(String),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Supported font family presets and custom font fallback for dyslexic readers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum FontFamilyPreset {
    OpenDyslexic,
    Lexend,
    AtkinsonHyperlegible,
    ComicNeue,
    Verdana,
    Custom(String),
}

impl FontFamilyPreset {
    /// Returns the primary CSS font family name.
    pub fn primary_name(&self) -> &str {
        match self {
            Self::OpenDyslexic => "OpenDyslexic",
            Self::Lexend => "Lexend",
            Self::AtkinsonHyperlegible => "Atkinson Hyperlegible",
            Self::ComicNeue => "Comic Neue",
            Self::Verdana => "Verdana",
            Self::Custom(name) => name.trim(),
        }
    }

    /// Validates that the font family name is non-empty and safe to emit into CSS.
    pub fn validate(&self) -> Result<(), DyslexiaProfileError> {
        let name = self.primary_name();
        if name.is_empty() {
            return Err(DyslexiaProfileError::InvalidFontFamily(name.to_string()));
        }
        let has_disallowed = name.chars().any(|c| {
            c.is_control() || matches!(c, ';' | '{' | '}' | '<' | '>' | '"' | '\'' | '\\')
        });
        if has_disallowed {
            return Err(DyslexiaProfileError::InvalidFontFamily(name.to_string()));
        }
        Ok(())
    }

    /// Formats the font-family CSS declaration value with safe sans-serif fallbacks.
    pub fn to_css_value(&self) -> String {
        format!("\"{}\", \"Verdana\", sans-serif", self.primary_name())
    }
}

/// Background and foreground color tint presets designed to reduce visual stress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "preset", rename_all = "snake_case")]
pub enum ColorTintPreset {
    None,
    WarmCream,
    SoftBlue,
    MintGreen,
    MutedSepia,
    DarkHighContrast,
    Custom {
        background_hex: String,
        foreground_hex: String,
        link_hex: Option<String>,
    },
}

impl ColorTintPreset {
    /// Validates any custom hex color strings.
    pub fn validate(&self) -> Result<(), DyslexiaProfileError> {
        if let Self::Custom {
            background_hex,
            foreground_hex,
            link_hex,
        } = self
        {
            validate_hex_color(background_hex)?;
            validate_hex_color(foreground_hex)?;
            if let Some(link) = link_hex {
                validate_hex_color(link)?;
            }
        }
        Ok(())
    }

    /// Resolves `(background_hex, foreground_hex, optional_link_hex)` when tinting is active.
    pub fn resolved_colors(&self) -> Option<(&str, &str, Option<&str>)> {
        match self {
            Self::None => None,
            Self::WarmCream => Some(("#FDF6E3", "#2E2A24", Some("#1A5FB4"))),
            Self::SoftBlue => Some(("#EBF3FA", "#1D2939", Some("#1849A9"))),
            Self::MintGreen => Some(("#ECF6EE", "#1D3022", Some("#14532D"))),
            Self::MutedSepia => Some(("#F4ECD8", "#3B2F23", Some("#7C2D12"))),
            Self::DarkHighContrast => Some(("#1A1B1E", "#EAEAEA", Some("#7DD3FC"))),
            Self::Custom {
                background_hex,
                foreground_hex,
                link_hex,
            } => Some((
                background_hex.as_str(),
                foreground_hex.as_str(),
                link_hex.as_deref(),
            )),
        }
    }
}

/// Horizontal reading ruler / line-focus overlay settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadingRulerConfig {
    pub enabled: bool,
    pub height_px: u16,
    pub opacity: f32,
    pub highlight_hex: String,
}

impl Default for ReadingRulerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            height_px: 34,
            opacity: 0.22,
            highlight_hex: "#FFE066".to_string(),
        }
    }
}

impl ReadingRulerConfig {
    pub fn validate(&self) -> Result<(), DyslexiaProfileError> {
        check_range(
            "reading_ruler.height_px",
            self.height_px as f32,
            16.0,
            120.0,
        )?;
        check_range("reading_ruler.opacity", self.opacity, 0.05, 0.95)?;
        validate_hex_color(&self.highlight_hex)?;
        Ok(())
    }
}

/// Typography aids that reduce crowding and improve letter/word tracking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypographyAids {
    pub bionic_bold_prefix_ratio: f32,
    pub disable_italics: bool,
    pub disable_justified_text: bool,
    pub underline_links: bool,
}

impl Default for TypographyAids {
    fn default() -> Self {
        Self {
            bionic_bold_prefix_ratio: 0.0,
            disable_italics: true,
            disable_justified_text: true,
            underline_links: true,
        }
    }
}

impl TypographyAids {
    pub fn validate(&self) -> Result<(), DyslexiaProfileError> {
        check_range(
            "typography_aids.bionic_bold_prefix_ratio",
            self.bionic_bold_prefix_ratio,
            0.0,
            0.8,
        )?;
        Ok(())
    }
}

/// Complete dyslexia accessibility font and reading profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DyslexiaFontProfile {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub font_family: FontFamilyPreset,
    pub font_size_px: f32,
    pub line_height: f32,
    pub letter_spacing_em: f32,
    pub word_spacing_em: f32,
    pub paragraph_spacing_em: f32,
    pub max_line_width_ch: u16,
    pub color_tint: ColorTintPreset,
    pub reading_ruler: ReadingRulerConfig,
    pub typography_aids: TypographyAids,
}

impl DyslexiaFontProfile {
    /// Built-in OpenDyslexic profile with generous spacing and warm cream tint.
    pub fn default_opendyslexic() -> Self {
        Self {
            id: "opendyslexic-default".to_string(),
            name: "OpenDyslexic Comfort".to_string(),
            enabled: true,
            font_family: FontFamilyPreset::OpenDyslexic,
            font_size_px: 18.0,
            line_height: 1.75,
            letter_spacing_em: 0.08,
            word_spacing_em: 0.22,
            paragraph_spacing_em: 1.25,
            max_line_width_ch: 68,
            color_tint: ColorTintPreset::WarmCream,
            reading_ruler: ReadingRulerConfig::default(),
            typography_aids: TypographyAids::default(),
        }
    }

    /// Built-in Lexend profile tuned for clean, modern sans-serif readability.
    pub fn lexend_comfort() -> Self {
        Self {
            id: "lexend-comfort".to_string(),
            name: "Lexend Balanced".to_string(),
            enabled: true,
            font_family: FontFamilyPreset::Lexend,
            font_size_px: 17.0,
            line_height: 1.65,
            letter_spacing_em: 0.05,
            word_spacing_em: 0.16,
            paragraph_spacing_em: 1.1,
            max_line_width_ch: 72,
            color_tint: ColorTintPreset::SoftBlue,
            reading_ruler: ReadingRulerConfig {
                enabled: true,
                ..ReadingRulerConfig::default()
            },
            typography_aids: TypographyAids::default(),
        }
    }

    /// Built-in Atkinson Hyperlegible profile for maximum character disambiguation.
    pub fn atkinson_high_clarity() -> Self {
        Self {
            id: "atkinson-clarity".to_string(),
            name: "Atkinson High Clarity".to_string(),
            enabled: true,
            font_family: FontFamilyPreset::AtkinsonHyperlegible,
            font_size_px: 18.5,
            line_height: 1.8,
            letter_spacing_em: 0.06,
            word_spacing_em: 0.2,
            paragraph_spacing_em: 1.3,
            max_line_width_ch: 66,
            color_tint: ColorTintPreset::DarkHighContrast,
            reading_ruler: ReadingRulerConfig::default(),
            typography_aids: TypographyAids::default(),
        }
    }

    /// Validates all profile fields, bounds, and sub-configurations.
    pub fn validate(&self) -> Result<(), DyslexiaProfileError> {
        validate_profile_id(&self.id)?;
        if self.name.trim().is_empty() {
            return Err(DyslexiaProfileError::InvalidProfileName);
        }
        self.font_family.validate()?;
        check_range("font_size_px", self.font_size_px, 10.0, 48.0)?;
        check_range("line_height", self.line_height, 1.0, 3.0)?;
        check_range("letter_spacing_em", self.letter_spacing_em, 0.0, 0.5)?;
        check_range("word_spacing_em", self.word_spacing_em, 0.0, 1.0)?;
        check_range("paragraph_spacing_em", self.paragraph_spacing_em, 0.0, 3.0)?;
        check_range(
            "max_line_width_ch",
            self.max_line_width_ch as f32,
            30.0,
            120.0,
        )?;
        self.color_tint.validate()?;
        self.reading_ruler.validate()?;
        self.typography_aids.validate()?;
        Ok(())
    }

    /// Generates an injectable user stylesheet representing this profile.
    pub fn to_user_stylesheet(&self) -> Result<String, DyslexiaProfileError> {
        self.validate()?;
        if !self.enabled {
            return Ok(String::new());
        }

        let mut css = String::new();
        css.push_str(&format!(
            "html, body, p, li, article, main, section {{\n  \
             font-family: {} !important;\n  \
             font-size: {:.2}px !important;\n  \
             line-height: {:.2} !important;\n  \
             letter-spacing: {:.3}em !important;\n  \
             word-spacing: {:.3}em !important;\n",
            self.font_family.to_css_value(),
            self.font_size_px,
            self.line_height,
            self.letter_spacing_em,
            self.word_spacing_em,
        ));

        if self.typography_aids.disable_justified_text {
            css.push_str("  text-align: left !important;\n");
        }
        if let Some((bg, fg, _)) = self.color_tint.resolved_colors() {
            css.push_str(&format!(
                "  background-color: {bg} !important;\n  color: {fg} !important;\n"
            ));
        }
        css.push_str("}\n");

        css.push_str(&format!(
            "p, li {{\n  margin-bottom: {:.2}em !important;\n  max-width: {}ch !important;\n}}\n",
            self.paragraph_spacing_em, self.max_line_width_ch
        ));

        if self.typography_aids.disable_italics {
            css.push_str("em, i {\n  font-style: normal !important;\n}\n");
        }

        if self.typography_aids.underline_links {
            if let Some((_, _, Some(link_hex))) = self.color_tint.resolved_colors() {
                css.push_str(&format!(
                    "a {{\n  text-decoration: underline !important;\n  color: {link_hex} !important;\n}}\n"
                ));
            } else {
                css.push_str("a {\n  text-decoration: underline !important;\n}\n");
            }
        }

        if self.reading_ruler.enabled {
            css.push_str(&format!(
                ".search-linux-reading-ruler {{\n  \
                 height: {}px !important;\n  \
                 opacity: {:.2} !important;\n  \
                 background-color: {} !important;\n  \
                 pointer-events: none !important;\n}}\n",
                self.reading_ruler.height_px,
                self.reading_ruler.opacity,
                self.reading_ruler.highlight_hex
            ));
        }

        Ok(css)
    }
}

/// Per-site profile override or opt-out rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteOverride {
    pub host_pattern: String,
    pub profile_id: String,
    pub force_disabled: bool,
}

impl SiteOverride {
    pub fn validate(&self) -> Result<(), DyslexiaProfileError> {
        validate_host_pattern(&self.host_pattern)?;
        validate_profile_id(&self.profile_id)?;
        Ok(())
    }

    /// Checks whether a parsed URL host matches this rule (`example.com` or `*.example.com`).
    pub fn matches_host(&self, host: &str) -> bool {
        let normalized_host = host.trim().to_ascii_lowercase();
        let pattern = self.host_pattern.trim().to_ascii_lowercase();
        if let Some(suffix) = pattern.strip_prefix("*.") {
            normalized_host == suffix || normalized_host.ends_with(&format!(".{suffix}"))
        } else {
            normalized_host == pattern
        }
    }
}

/// Persistent collection of dyslexia profiles, active selection, and per-site overrides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DyslexiaProfileStore {
    pub profiles: Vec<DyslexiaFontProfile>,
    pub active_profile_id: Option<String>,
    pub site_overrides: Vec<SiteOverride>,
}

impl DyslexiaProfileStore {
    /// Creates a store pre-populated with the built-in dyslexia profiles.
    pub fn with_defaults() -> Self {
        let default_profile = DyslexiaFontProfile::default_opendyslexic();
        let active_id = default_profile.id.clone();
        Self {
            profiles: vec![
                default_profile,
                DyslexiaFontProfile::lexend_comfort(),
                DyslexiaFontProfile::atkinson_high_clarity(),
            ],
            active_profile_id: Some(active_id),
            site_overrides: Vec::new(),
        }
    }

    /// Validates and adds a new profile, rejecting duplicate IDs.
    pub fn add_profile(
        &mut self,
        profile: DyslexiaFontProfile,
    ) -> Result<(), DyslexiaProfileError> {
        profile.validate()?;
        if self.profiles.iter().any(|p| p.id == profile.id) {
            return Err(DyslexiaProfileError::DuplicateProfileId(profile.id));
        }
        self.profiles.push(profile);
        Ok(())
    }

    /// Sets the active profile ID after verifying it exists.
    pub fn set_active_profile(&mut self, id: &str) -> Result<(), DyslexiaProfileError> {
        if !self.profiles.iter().any(|p| p.id == id) {
            return Err(DyslexiaProfileError::ProfileNotFound(id.to_string()));
        }
        self.active_profile_id = Some(id.to_string());
        Ok(())
    }

    /// Adds or replaces a site override for a host pattern.
    pub fn add_site_override(&mut self, entry: SiteOverride) -> Result<(), DyslexiaProfileError> {
        entry.validate()?;
        if !entry.force_disabled && !self.profiles.iter().any(|p| p.id == entry.profile_id) {
            return Err(DyslexiaProfileError::ProfileNotFound(entry.profile_id));
        }
        if let Some(existing) = self
            .site_overrides
            .iter_mut()
            .find(|o| o.host_pattern.eq_ignore_ascii_case(&entry.host_pattern))
        {
            *existing = entry;
        } else {
            self.site_overrides.push(entry);
        }
        Ok(())
    }

    /// Resolves the effective profile for a target URL, respecting site overrides and opt-outs.
    pub fn resolve_for_url(
        &self,
        raw_url: &str,
    ) -> Result<Option<&DyslexiaFontProfile>, DyslexiaProfileError> {
        let parsed = Url::parse(raw_url)
            .map_err(|e| DyslexiaProfileError::InvalidUrl(raw_url.to_string(), e.to_string()))?;

        if let Some(host) = parsed.host_str() {
            if let Some(site_rule) = self.site_overrides.iter().find(|o| o.matches_host(host)) {
                if site_rule.force_disabled {
                    return Ok(None);
                }
                let profile = self
                    .profiles
                    .iter()
                    .find(|p| p.id == site_rule.profile_id)
                    .ok_or_else(|| {
                        DyslexiaProfileError::ProfileNotFound(site_rule.profile_id.clone())
                    })?;
                return Ok(if profile.enabled { Some(profile) } else { None });
            }
        }

        match &self.active_profile_id {
            Some(active_id) => {
                let profile = self
                    .profiles
                    .iter()
                    .find(|p| &p.id == active_id)
                    .ok_or_else(|| DyslexiaProfileError::ProfileNotFound(active_id.clone()))?;
                Ok(if profile.enabled { Some(profile) } else { None })
            }
            None => Ok(None),
        }
    }

    /// Validates the entire store and serializes it to pretty JSON.
    pub fn to_json(&self) -> Result<String, DyslexiaProfileError> {
        self.validate()?;
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Deserializes from JSON and validates all profiles, active selection, and site overrides.
    pub fn from_json(json: &str) -> Result<Self, DyslexiaProfileError> {
        let store: Self = serde_json::from_str(json)?;
        store.validate()?;
        Ok(store)
    }

    /// Validates all profiles, active selection, and site overrides in the store.
    pub fn validate(&self) -> Result<(), DyslexiaProfileError> {
        for (idx, profile) in self.profiles.iter().enumerate() {
            profile.validate()?;
            if self.profiles[idx + 1..]
                .iter()
                .any(|other| other.id == profile.id)
            {
                return Err(DyslexiaProfileError::DuplicateProfileId(profile.id.clone()));
            }
        }
        if let Some(active_id) = &self.active_profile_id {
            if !self.profiles.iter().any(|p| &p.id == active_id) {
                return Err(DyslexiaProfileError::ProfileNotFound(active_id.clone()));
            }
        }
        for rule in &self.site_overrides {
            rule.validate()?;
            if !rule.force_disabled && !self.profiles.iter().any(|p| p.id == rule.profile_id) {
                return Err(DyslexiaProfileError::ProfileNotFound(
                    rule.profile_id.clone(),
                ));
            }
        }
        Ok(())
    }
}

fn check_range(
    field: &'static str,
    value: f32,
    min: f32,
    max: f32,
) -> Result<(), DyslexiaProfileError> {
    if !value.is_finite() || value < min || value > max {
        return Err(DyslexiaProfileError::OutOfRange {
            field,
            value,
            min,
            max,
        });
    }
    Ok(())
}

fn validate_profile_id(id: &str) -> Result<(), DyslexiaProfileError> {
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(DyslexiaProfileError::InvalidProfileId(id.to_string()));
    }
    Ok(())
}

fn validate_hex_color(color: &str) -> Result<(), DyslexiaProfileError> {
    let hex = color
        .strip_prefix('#')
        .ok_or_else(|| DyslexiaProfileError::InvalidHexColor(color.to_string()))?;
    let valid_len = matches!(hex.len(), 3 | 6 | 8);
    if !valid_len || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(DyslexiaProfileError::InvalidHexColor(color.to_string()));
    }
    Ok(())
}

fn validate_host_pattern(pattern: &str) -> Result<(), DyslexiaProfileError> {
    let trimmed = pattern.trim();
    let host_part = trimmed.strip_prefix("*.").unwrap_or(trimmed);
    if host_part.is_empty()
        || host_part.contains('*')
        || host_part.contains('/')
        || host_part.chars().any(char::is_whitespace)
    {
        return Err(DyslexiaProfileError::InvalidHostPattern(
            pattern.to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_generates_valid_stylesheet() {
        let profile = DyslexiaFontProfile::default_opendyslexic();
        let css = profile
            .to_user_stylesheet()
            .expect("default profile should generate valid CSS");

        assert!(css.contains("\"OpenDyslexic\""));
        assert!(css.contains("letter-spacing: 0.080em"));
        assert!(css.contains("background-color: #FDF6E3"));
        assert!(css.contains("font-style: normal"));
        assert!(css.contains("text-align: left"));
    }

    #[test]
    fn validation_rejects_out_of_range_and_unsafe_font_names() {
        let mut bad_spacing = DyslexiaFontProfile::lexend_comfort();
        bad_spacing.letter_spacing_em = 1.25;
        assert!(matches!(
            bad_spacing.validate(),
            Err(DyslexiaProfileError::OutOfRange {
                field: "letter_spacing_em",
                ..
            })
        ));

        let mut bad_font = DyslexiaFontProfile::lexend_comfort();
        bad_font.font_family = FontFamilyPreset::Custom("BadFont; color: red".to_string());
        assert!(matches!(
            bad_font.validate(),
            Err(DyslexiaProfileError::InvalidFontFamily(_))
        ));

        let mut bad_color = DyslexiaFontProfile::lexend_comfort();
        bad_color.color_tint = ColorTintPreset::Custom {
            background_hex: "not-hex".to_string(),
            foreground_hex: "#111111".to_string(),
            link_hex: None,
        };
        assert!(matches!(
            bad_color.validate(),
            Err(DyslexiaProfileError::InvalidHexColor(_))
        ));
    }

    #[test]
    fn store_resolves_site_overrides_and_roundtrips_json() {
        let mut store = DyslexiaProfileStore::with_defaults();
        store
            .add_site_override(SiteOverride {
                host_pattern: "*.wikipedia.org".to_string(),
                profile_id: "lexend-comfort".to_string(),
                force_disabled: false,
            })
            .expect("override should be added");

        store
            .add_site_override(SiteOverride {
                host_pattern: "canvas.example.edu".to_string(),
                profile_id: "opendyslexic-default".to_string(),
                force_disabled: true,
            })
            .expect("opt-out override should be added");

        let json = store.to_json().expect("store should serialize");
        let restored = DyslexiaProfileStore::from_json(&json).expect("store should deserialize");
        assert_eq!(store, restored);

        let wiki = restored
            .resolve_for_url("https://en.wikipedia.org/wiki/Typography")
            .expect("URL should resolve")
            .expect("profile should be active");
        assert_eq!(wiki.id, "lexend-comfort");

        let canvas = restored
            .resolve_for_url("https://canvas.example.edu/editor")
            .expect("URL should resolve");
        assert!(canvas.is_none());

        let fallback = restored
            .resolve_for_url("https://doc.rust-lang.org/book/")
            .expect("URL should resolve")
            .expect("default profile should apply");
        assert_eq!(fallback.id, "opendyslexic-default");
    }
}
