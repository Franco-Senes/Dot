//! User-Agent presets, custom profiles, and per-domain overrides.
//!
//! Provides built-in and user-defined User-Agent presets (with optional
//! Client Hints metadata), validation against header-injection characters,
//! per-domain wildcard overrides, and JSON persistence.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

/// Maximum allowed length in bytes for a preset identifier.
const MAX_PRESET_ID_LEN: usize = 64;

/// Maximum allowed length in bytes for a User-Agent string.
const MAX_USER_AGENT_LEN: usize = 1024;

/// Errors returned by User-Agent preset validation and registry operations.
#[derive(Debug, Error)]
pub enum UserAgentPresetError {
    #[error("invalid preset id '{0}': must be 1..=64 ASCII lowercase alphanumeric, '-', or '_'")]
    InvalidPresetId(String),

    #[error("preset name must not be empty")]
    EmptyName,

    #[error("user-agent string must not be empty")]
    EmptyUserAgent,

    #[error("user-agent string contains invalid or non-printable ASCII characters")]
    InvalidUserAgentChars,

    #[error("user-agent string exceeds maximum length of {MAX_USER_AGENT_LEN} bytes (got {0})")]
    UserAgentTooLong(usize),

    #[error("preset id '{0}' already exists")]
    DuplicatePresetId(String),

    #[error("preset id '{0}' not found")]
    PresetNotFound(String),

    #[error("cannot delete built-in preset '{0}'")]
    CannotDeleteBuiltin(String),

    #[error("cannot modify built-in preset '{0}'")]
    CannotModifyBuiltin(String),

    #[error("custom preset must not be marked as built-in")]
    BuiltinFlagNotAllowed,

    #[error("invalid domain pattern '{0}'")]
    InvalidDomainPattern(String),

    #[error("header value contains invalid control characters")]
    InvalidHeaderValue,

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Broad device/usage category for a User-Agent preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresetCategory {
    Desktop,
    Mobile,
    Tablet,
    Bot,
    Custom,
}

/// Browser family represented by a preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserFamily {
    Chrome,
    Firefox,
    Safari,
    Edge,
    Brave,
    Opera,
    Other,
}

/// Operating system platform represented by a preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformOs {
    Linux,
    Windows,
    MacOs,
    Android,
    Ios,
    Other,
}

/// A User-Agent preset definition with optional Client Hints metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserAgentPreset {
    pub id: String,
    pub name: String,
    pub user_agent: String,
    pub category: PresetCategory,
    pub browser: BrowserFamily,
    pub platform: PlatformOs,
    pub is_builtin: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sec_ch_ua: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sec_ch_ua_mobile: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sec_ch_ua_platform: Option<String>,
}

impl UserAgentPreset {
    /// Validates all fields of this preset.
    pub fn validate(&self) -> Result<(), UserAgentPresetError> {
        validate_preset_id(&self.id)?;
        if self.name.trim().is_empty() {
            return Err(UserAgentPresetError::EmptyName);
        }
        validate_user_agent(&self.user_agent)?;
        if let Some(ref val) = self.sec_ch_ua {
            validate_header_value(val)?;
        }
        if let Some(ref val) = self.sec_ch_ua_platform {
            validate_header_value(val)?;
        }
        Ok(())
    }
}

/// Associates a domain pattern (exact host or `*.domain`) with a preset id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainOverride {
    pub domain_pattern: String,
    pub preset_id: String,
}

/// Registry holding built-in and custom User-Agent presets, the global active
/// preset selection, and per-domain overrides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserAgentRegistry {
    presets: Vec<UserAgentPreset>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    active_preset_id: Option<String>,
    #[serde(default)]
    domain_overrides: Vec<DomainOverride>,
}

impl Default for UserAgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl UserAgentRegistry {
    /// Creates a new registry pre-populated with built-in presets and no active override.
    pub fn new() -> Self {
        Self {
            presets: builtin_presets(),
            active_preset_id: None,
            domain_overrides: Vec::new(),
        }
    }

    /// Returns all presets in the registry, optionally filtered by category.
    pub fn list_presets(&self, category: Option<PresetCategory>) -> Vec<&UserAgentPreset> {
        self.presets
            .iter()
            .filter(|preset| category.is_none_or(|cat| preset.category == cat))
            .collect()
    }

    /// Looks up a preset by its identifier.
    pub fn get_preset(&self, id: &str) -> Option<&UserAgentPreset> {
        self.presets.iter().find(|preset| preset.id == id)
    }

    /// Adds a new custom preset to the registry.
    pub fn add_custom_preset(
        &mut self,
        preset: UserAgentPreset,
    ) -> Result<(), UserAgentPresetError> {
        if preset.is_builtin {
            return Err(UserAgentPresetError::BuiltinFlagNotAllowed);
        }
        preset.validate()?;
        if self.presets.iter().any(|existing| existing.id == preset.id) {
            return Err(UserAgentPresetError::DuplicatePresetId(preset.id));
        }
        self.presets.push(preset);
        Ok(())
    }

    /// Updates an existing custom preset in place.
    pub fn update_custom_preset(
        &mut self,
        preset: UserAgentPreset,
    ) -> Result<(), UserAgentPresetError> {
        if preset.is_builtin {
            return Err(UserAgentPresetError::BuiltinFlagNotAllowed);
        }
        preset.validate()?;
        let existing = self
            .presets
            .iter_mut()
            .find(|item| item.id == preset.id)
            .ok_or_else(|| UserAgentPresetError::PresetNotFound(preset.id.clone()))?;
        if existing.is_builtin {
            return Err(UserAgentPresetError::CannotModifyBuiltin(preset.id));
        }
        *existing = preset;
        Ok(())
    }

    /// Removes a custom preset by id.
    ///
    /// If the removed preset was active or referenced by domain overrides,
    /// those references are cleared.
    pub fn remove_preset(&mut self, id: &str) -> Result<UserAgentPreset, UserAgentPresetError> {
        let index = self
            .presets
            .iter()
            .position(|preset| preset.id == id)
            .ok_or_else(|| UserAgentPresetError::PresetNotFound(id.to_string()))?;

        if self.presets[index].is_builtin {
            return Err(UserAgentPresetError::CannotDeleteBuiltin(id.to_string()));
        }

        let removed = self.presets.remove(index);
        if self.active_preset_id.as_deref() == Some(id) {
            self.active_preset_id = None;
        }
        self.domain_overrides
            .retain(|override_entry| override_entry.preset_id != id);

        Ok(removed)
    }

    /// Sets or clears the global active preset.
    pub fn set_active_preset(&mut self, id: Option<&str>) -> Result<(), UserAgentPresetError> {
        match id {
            Some(target_id) => {
                if self.get_preset(target_id).is_none() {
                    return Err(UserAgentPresetError::PresetNotFound(target_id.to_string()));
                }
                self.active_preset_id = Some(target_id.to_string());
                Ok(())
            }
            None => {
                self.active_preset_id = None;
                Ok(())
            }
        }
    }

    /// Returns the currently active global preset, if one is selected.
    pub fn active_preset(&self) -> Option<&UserAgentPreset> {
        self.active_preset_id
            .as_deref()
            .and_then(|id| self.get_preset(id))
    }

    /// Returns all configured domain overrides.
    pub fn domain_overrides(&self) -> &[DomainOverride] {
        &self.domain_overrides
    }

    /// Sets or updates a per-domain override for an exact host or `*.domain` pattern.
    pub fn set_domain_override(
        &mut self,
        domain_pattern: &str,
        preset_id: &str,
    ) -> Result<(), UserAgentPresetError> {
        let normalized = normalize_domain_pattern(domain_pattern)?;
        if self.get_preset(preset_id).is_none() {
            return Err(UserAgentPresetError::PresetNotFound(preset_id.to_string()));
        }

        if let Some(existing) = self
            .domain_overrides
            .iter_mut()
            .find(|entry| entry.domain_pattern == normalized)
        {
            existing.preset_id = preset_id.to_string();
        } else {
            self.domain_overrides.push(DomainOverride {
                domain_pattern: normalized,
                preset_id: preset_id.to_string(),
            });
        }
        Ok(())
    }

    /// Removes a per-domain override by pattern. Returns `true` if one was removed.
    pub fn remove_domain_override(&mut self, domain_pattern: &str) -> bool {
        let normalized = domain_pattern.trim().to_ascii_lowercase();
        let before = self.domain_overrides.len();
        self.domain_overrides
            .retain(|entry| entry.domain_pattern != normalized);
        self.domain_overrides.len() != before
    }

    /// Resolves the effective preset for a given URL, checking domain overrides
    /// before falling back to the global active preset.
    pub fn resolve_for_url(&self, raw_url: &str) -> Option<&UserAgentPreset> {
        if let Ok(parsed) = Url::parse(raw_url) {
            if let Some(host) = parsed.host_str() {
                return self.resolve_for_host(host);
            }
        }
        self.active_preset()
    }

    /// Resolves the effective preset for a hostname, preferring exact matches,
    /// then the most specific wildcard `*.domain` match, then the active preset.
    pub fn resolve_for_host(&self, host: &str) -> Option<&UserAgentPreset> {
        let normalized_host = host.trim().trim_end_matches('.').to_ascii_lowercase();
        if normalized_host.is_empty() {
            return self.active_preset();
        }

        if let Some(exact) = self
            .domain_overrides
            .iter()
            .find(|entry| entry.domain_pattern == normalized_host)
        {
            if let Some(preset) = self.get_preset(&exact.preset_id) {
                return Some(preset);
            }
        }

        let mut best_wildcard: Option<(&DomainOverride, usize)> = None;
        for entry in &self.domain_overrides {
            if let Some(suffix) = entry.domain_pattern.strip_prefix("*.") {
                let matches = normalized_host == suffix
                    || (normalized_host.len() > suffix.len()
                        && normalized_host.ends_with(suffix)
                        && normalized_host.as_bytes()[normalized_host.len() - suffix.len() - 1]
                            == b'.');
                if matches {
                    let specificity = suffix.len();
                    if best_wildcard.is_none_or(|(_, best_len)| specificity > best_len) {
                        best_wildcard = Some((entry, specificity));
                    }
                }
            }
        }

        if let Some((matched, _)) = best_wildcard {
            if let Some(preset) = self.get_preset(&matched.preset_id) {
                return Some(preset);
            }
        }

        self.active_preset()
    }

    /// Builds HTTP request headers (`User-Agent` and any configured Client Hints)
    /// for an optional target URL or host.
    pub fn build_headers(&self, target_url_or_host: Option<&str>) -> Vec<(String, String)> {
        let resolved = match target_url_or_host {
            Some(target) if target.contains("://") => self.resolve_for_url(target),
            Some(host) => self.resolve_for_host(host),
            None => self.active_preset(),
        };

        let Some(preset) = resolved else {
            return Vec::new();
        };

        let mut headers = Vec::with_capacity(4);
        headers.push(("User-Agent".to_string(), preset.user_agent.clone()));

        if let Some(ref ch_ua) = preset.sec_ch_ua {
            headers.push(("Sec-CH-UA".to_string(), ch_ua.clone()));
        }
        if let Some(is_mobile) = preset.sec_ch_ua_mobile {
            headers.push((
                "Sec-CH-UA-Mobile".to_string(),
                if is_mobile { "?1" } else { "?0" }.to_string(),
            ));
        }
        if let Some(ref platform) = preset.sec_ch_ua_platform {
            headers.push(("Sec-CH-UA-Platform".to_string(), platform.clone()));
        }

        headers
    }

    /// Serializes the registry state to pretty-printed JSON.
    pub fn to_json(&self) -> Result<String, UserAgentPresetError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Deserializes a registry from JSON and validates all presets, active
    /// selection, and domain overrides.
    pub fn from_json(json: &str) -> Result<Self, UserAgentPresetError> {
        let mut registry: Self = serde_json::from_str(json)?;

        let mut seen_ids = Vec::with_capacity(registry.presets.len());
        for preset in &registry.presets {
            preset.validate()?;
            if seen_ids.iter().any(|id| id == &preset.id) {
                return Err(UserAgentPresetError::DuplicatePresetId(preset.id.clone()));
            }
            seen_ids.push(preset.id.clone());
        }

        if let Some(ref active_id) = registry.active_preset_id {
            if !seen_ids.iter().any(|id| id == active_id) {
                return Err(UserAgentPresetError::PresetNotFound(active_id.clone()));
            }
        }

        for override_entry in &mut registry.domain_overrides {
            override_entry.domain_pattern =
                normalize_domain_pattern(&override_entry.domain_pattern)?;
            if !seen_ids.iter().any(|id| id == &override_entry.preset_id) {
                return Err(UserAgentPresetError::PresetNotFound(
                    override_entry.preset_id.clone(),
                ));
            }
        }

        Ok(registry)
    }
}

/// Validates a preset identifier.
pub fn validate_preset_id(id: &str) -> Result<(), UserAgentPresetError> {
    if id.is_empty()
        || id.len() > MAX_PRESET_ID_LEN
        || !id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err(UserAgentPresetError::InvalidPresetId(id.to_string()));
    }
    Ok(())
}

/// Validates a User-Agent header string against emptiness, length limits, and
/// non-printable or header-injection characters.
pub fn validate_user_agent(ua: &str) -> Result<(), UserAgentPresetError> {
    if ua.trim().is_empty() {
        return Err(UserAgentPresetError::EmptyUserAgent);
    }
    if ua.len() > MAX_USER_AGENT_LEN {
        return Err(UserAgentPresetError::UserAgentTooLong(ua.len()));
    }
    if !ua.bytes().all(|b| (0x20..=0x7E).contains(&b)) {
        return Err(UserAgentPresetError::InvalidUserAgentChars);
    }
    Ok(())
}

/// Validates an auxiliary HTTP header value (e.g. `Sec-CH-UA`).
fn validate_header_value(val: &str) -> Result<(), UserAgentPresetError> {
    if val.trim().is_empty() || !val.bytes().all(|b| (0x20..=0x7E).contains(&b)) {
        return Err(UserAgentPresetError::InvalidHeaderValue);
    }
    Ok(())
}

/// Normalizes and validates an exact host or wildcard `*.domain` pattern.
pub fn normalize_domain_pattern(pattern: &str) -> Result<String, UserAgentPresetError> {
    let trimmed = pattern.trim().trim_end_matches('.').to_ascii_lowercase();
    if trimmed.is_empty() {
        return Err(UserAgentPresetError::InvalidDomainPattern(
            pattern.to_string(),
        ));
    }

    let host_part = trimmed.strip_prefix("*.").unwrap_or(&trimmed);
    if host_part.is_empty() || host_part.contains('*') {
        return Err(UserAgentPresetError::InvalidDomainPattern(
            pattern.to_string(),
        ));
    }

    for label in host_part.split('.') {
        if label.is_empty()
            || label.starts_with('-')
            || label.ends_with('-')
            || !label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(UserAgentPresetError::InvalidDomainPattern(
                pattern.to_string(),
            ));
        }
    }

    Ok(trimmed)
}

/// Returns the default set of built-in User-Agent presets.
pub fn builtin_presets() -> Vec<UserAgentPreset> {
    vec![
        UserAgentPreset {
            id: "firefox-linux".to_string(),
            name: "Firefox on Linux".to_string(),
            user_agent: "Mozilla/5.0 (X11; Linux x86_64; rv:132.0) Gecko/20100101 Firefox/132.0"
                .to_string(),
            category: PresetCategory::Desktop,
            browser: BrowserFamily::Firefox,
            platform: PlatformOs::Linux,
            is_builtin: true,
            description: Some("Standard Mozilla Firefox desktop user agent on Linux".to_string()),
            sec_ch_ua: None,
            sec_ch_ua_mobile: None,
            sec_ch_ua_platform: None,
        },
        UserAgentPreset {
            id: "chrome-linux".to_string(),
            name: "Chrome on Linux".to_string(),
            user_agent: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36".to_string(),
            category: PresetCategory::Desktop,
            browser: BrowserFamily::Chrome,
            platform: PlatformOs::Linux,
            is_builtin: true,
            description: Some("Google Chrome desktop user agent on Linux".to_string()),
            sec_ch_ua: Some("\"Chromium\";v=\"130\", \"Google Chrome\";v=\"130\", \"Not?A_Brand\";v=\"99\"".to_string()),
            sec_ch_ua_mobile: Some(false),
            sec_ch_ua_platform: Some("\"Linux\"".to_string()),
        },
        UserAgentPreset {
            id: "chrome-windows".to_string(),
            name: "Chrome on Windows".to_string(),
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36".to_string(),
            category: PresetCategory::Desktop,
            browser: BrowserFamily::Chrome,
            platform: PlatformOs::Windows,
            is_builtin: true,
            description: Some("Google Chrome desktop user agent on Windows 10/11".to_string()),
            sec_ch_ua: Some("\"Chromium\";v=\"130\", \"Google Chrome\";v=\"130\", \"Not?A_Brand\";v=\"99\"".to_string()),
            sec_ch_ua_mobile: Some(false),
            sec_ch_ua_platform: Some("\"Windows\"".to_string()),
        },
        UserAgentPreset {
            id: "safari-macos".to_string(),
            name: "Safari on macOS".to_string(),
            user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15".to_string(),
            category: PresetCategory::Desktop,
            browser: BrowserFamily::Safari,
            platform: PlatformOs::MacOs,
            is_builtin: true,
            description: Some("Apple Safari desktop user agent on macOS".to_string()),
            sec_ch_ua: None,
            sec_ch_ua_mobile: None,
            sec_ch_ua_platform: None,
        },
        UserAgentPreset {
            id: "chrome-android".to_string(),
            name: "Chrome on Android".to_string(),
            user_agent: "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Mobile Safari/537.36".to_string(),
            category: PresetCategory::Mobile,
            browser: BrowserFamily::Chrome,
            platform: PlatformOs::Android,
            is_builtin: true,
            description: Some("Google Chrome mobile user agent on Android".to_string()),
            sec_ch_ua: Some("\"Chromium\";v=\"130\", \"Google Chrome\";v=\"130\", \"Not?A_Brand\";v=\"99\"".to_string()),
            sec_ch_ua_mobile: Some(true),
            sec_ch_ua_platform: Some("\"Android\"".to_string()),
        },
        UserAgentPreset {
            id: "safari-ios".to_string(),
            name: "Safari on iOS".to_string(),
            user_agent: "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1".to_string(),
            category: PresetCategory::Mobile,
            browser: BrowserFamily::Safari,
            platform: PlatformOs::Ios,
            is_builtin: true,
            description: Some("Apple Safari mobile user agent on iPhone".to_string()),
            sec_ch_ua: None,
            sec_ch_ua_mobile: None,
            sec_ch_ua_platform: None,
        },
        UserAgentPreset {
            id: "googlebot".to_string(),
            name: "Googlebot".to_string(),
            user_agent: "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)".to_string(),
            category: PresetCategory::Bot,
            browser: BrowserFamily::Other,
            platform: PlatformOs::Other,
            is_builtin: true,
            description: Some("Google search crawler user agent".to_string()),
            sec_ch_ua: None,
            sec_ch_ua_mobile: None,
            sec_ch_ua_platform: None,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_custom_preset(id: &str) -> UserAgentPreset {
        UserAgentPreset {
            id: id.to_string(),
            name: "Custom Test Agent".to_string(),
            user_agent: "CustomBrowser/1.0 (Linux x86_64)".to_string(),
            category: PresetCategory::Custom,
            browser: BrowserFamily::Other,
            platform: PlatformOs::Linux,
            is_builtin: false,
            description: Some("Test custom preset".to_string()),
            sec_ch_ua: None,
            sec_ch_ua_mobile: None,
            sec_ch_ua_platform: None,
        }
    }

    #[test]
    fn registry_crud_and_builtin_protection() {
        let mut registry = UserAgentRegistry::new();
        assert!(registry.list_presets(None).len() >= 7);
        assert!(registry.active_preset().is_none());

        // Cannot delete or modify built-in presets.
        assert!(matches!(
            registry.remove_preset("firefox-linux"),
            Err(UserAgentPresetError::CannotDeleteBuiltin(_))
        ));

        let mut modified_builtin = registry.get_preset("firefox-linux").unwrap().clone();
        modified_builtin.is_builtin = false;
        assert!(matches!(
            registry.update_custom_preset(modified_builtin),
            Err(UserAgentPresetError::CannotModifyBuiltin(_))
        ));

        // Add, activate, update, and remove a custom preset.
        registry
            .add_custom_preset(sample_custom_preset("my-custom_1"))
            .unwrap();
        assert!(matches!(
            registry.add_custom_preset(sample_custom_preset("my-custom_1")),
            Err(UserAgentPresetError::DuplicatePresetId(_))
        ));

        registry.set_active_preset(Some("my-custom_1")).unwrap();
        assert_eq!(registry.active_preset().unwrap().id, "my-custom_1");

        let mut updated = sample_custom_preset("my-custom_1");
        updated.user_agent = "CustomBrowser/2.0 (Linux x86_64)".to_string();
        registry.update_custom_preset(updated).unwrap();
        assert_eq!(
            registry.active_preset().unwrap().user_agent,
            "CustomBrowser/2.0 (Linux x86_64)"
        );

        let removed = registry.remove_preset("my-custom_1").unwrap();
        assert_eq!(removed.id, "my-custom_1");
        assert!(registry.active_preset().is_none());
    }

    #[test]
    fn domain_overrides_precedence_and_headers() {
        let mut registry = UserAgentRegistry::new();
        registry.set_active_preset(Some("firefox-linux")).unwrap();

        registry
            .set_domain_override("*.example.com", "chrome-windows")
            .unwrap();
        registry
            .set_domain_override("*.app.example.com", "chrome-android")
            .unwrap();
        registry
            .set_domain_override("api.app.example.com", "googlebot")
            .unwrap();

        // Exact match wins over wildcards.
        assert_eq!(
            registry
                .resolve_for_url("https://api.app.example.com/v1/search")
                .unwrap()
                .id,
            "googlebot"
        );
        // More specific wildcard wins over broader wildcard.
        assert_eq!(
            registry
                .resolve_for_url("https://web.app.example.com/home")
                .unwrap()
                .id,
            "chrome-android"
        );
        // Broader wildcard matches apex and other subdomains.
        assert_eq!(
            registry.resolve_for_url("https://example.com/").unwrap().id,
            "chrome-windows"
        );
        // Unmatched host falls back to global active preset.
        assert_eq!(
            registry
                .resolve_for_url("https://unrelated.org/")
                .unwrap()
                .id,
            "firefox-linux"
        );

        // Client Hints headers are included when defined on the resolved preset.
        let headers = registry.build_headers(Some("https://web.app.example.com/"));
        assert!(headers
            .iter()
            .any(|(k, v)| k == "Sec-CH-UA-Mobile" && v == "?1"));
        assert!(headers
            .iter()
            .any(|(k, v)| k == "Sec-CH-UA-Platform" && v == "\"Android\""));

        assert!(registry.remove_domain_override("*.app.example.com"));
        assert!(!registry.remove_domain_override("*.app.example.com"));
    }

    #[test]
    fn validation_rejects_injection_and_malformed_inputs() {
        let mut bad_ua = sample_custom_preset("bad-ua");
        bad_ua.user_agent = "BadAgent/1.0\r\nX-Injected: true".to_string();
        assert!(matches!(
            bad_ua.validate(),
            Err(UserAgentPresetError::InvalidUserAgentChars)
        ));

        let mut empty_ua = sample_custom_preset("empty-ua");
        empty_ua.user_agent = "   ".to_string();
        assert!(matches!(
            empty_ua.validate(),
            Err(UserAgentPresetError::EmptyUserAgent)
        ));

        let bad_id = sample_custom_preset("Invalid ID!");
        assert!(matches!(
            bad_id.validate(),
            Err(UserAgentPresetError::InvalidPresetId(_))
        ));

        let mut registry = UserAgentRegistry::new();
        assert!(matches!(
            registry.set_domain_override("foo.*.example.com", "firefox-linux"),
            Err(UserAgentPresetError::InvalidDomainPattern(_))
        ));
        assert!(matches!(
            registry.set_domain_override("-bad.example.com", "firefox-linux"),
            Err(UserAgentPresetError::InvalidDomainPattern(_))
        ));
    }

    #[test]
    fn json_roundtrip_and_invalid_state_rejection() {
        let mut registry = UserAgentRegistry::new();
        registry
            .add_custom_preset(sample_custom_preset("custom-alpha"))
            .unwrap();
        registry.set_active_preset(Some("custom-alpha")).unwrap();
        registry
            .set_domain_override("docs.example.org", "safari-macos")
            .unwrap();

        let json = registry.to_json().unwrap();
        let restored = UserAgentRegistry::from_json(&json).unwrap();
        assert_eq!(registry, restored);

        let broken_json = r#"{
            "presets": [],
            "active_preset_id": "missing-id",
            "domain_overrides": []
        }"#;
        assert!(matches!(
            UserAgentRegistry::from_json(broken_json),
            Err(UserAgentPresetError::PresetNotFound(_))
        ));
    }
}
