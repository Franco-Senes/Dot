//! DNS-over-HTTPS (DoH) preset registry, validation, and resolver settings.

use std::collections::HashSet;
use std::net::IpAddr;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

/// Minimum allowed DoH query timeout in milliseconds.
pub const MIN_DOH_TIMEOUT_MS: u32 = 200;
/// Maximum allowed DoH query timeout in milliseconds.
pub const MAX_DOH_TIMEOUT_MS: u32 = 30_000;
/// Default DoH query timeout in milliseconds.
pub const DEFAULT_DOH_TIMEOUT_MS: u32 = 3_000;

/// Errors returned when validating or managing DoH presets and settings.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DohError {
    #[error("preset id must not be empty")]
    EmptyId,
    #[error("preset id '{0}' must be 1..=64 ASCII alphanumeric, '-' or '_' characters")]
    InvalidId(String),
    #[error("preset name must not be empty")]
    EmptyName,
    #[error("preset provider must not be empty")]
    EmptyProvider,
    #[error("invalid DoH endpoint URL '{0}'")]
    InvalidEndpointUrl(String),
    #[error("DoH endpoint must use https scheme, got '{0}'")]
    InsecureScheme(String),
    #[error("DoH endpoint URL must include a host")]
    MissingHost,
    #[error("DoH endpoint URL must not contain embedded credentials")]
    EmbeddedCredentials,
    #[error("duplicate preset id '{0}'")]
    DuplicatePresetId(String),
    #[error("preset '{0}' was not found")]
    PresetNotFound(String),
    #[error("built-in preset '{0}' cannot be removed")]
    CannotDeleteBuiltin(String),
    #[error("timeout_ms {0} is out of range ({MIN_DOH_TIMEOUT_MS}..={MAX_DOH_TIMEOUT_MS})")]
    InvalidTimeout(u32),
    #[error("active_preset_id is required when DoH mode is {0:?}")]
    MissingActivePreset(DohMode),
    #[error("strict DoH mode cannot enable fallback_to_system")]
    StrictModeWithFallback,
    #[error("failed to serialize or deserialize DoH settings: {0}")]
    Serialization(String),
}

/// Operating mode for DNS-over-HTTPS resolution.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DohMode {
    /// Use the operating system's default DNS resolver.
    Off,
    /// Prefer DoH, falling back to system DNS if DoH fails.
    #[default]
    Automatic,
    /// Require DoH for all queries and fail closed on resolver errors.
    Strict,
}

/// Wire protocol supported by a DoH endpoint.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DohWireFormat {
    /// RFC 8484 binary DNS wire format (`application/dns-message`).
    #[default]
    DnsWire,
    /// JSON DNS-over-HTTPS format (`application/dns-json`).
    DnsJson,
}

/// Content filtering policy advertised by a DoH preset.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DohFilteringPolicy {
    /// No DNS-level filtering.
    #[default]
    Unfiltered,
    /// Blocks known malware and phishing domains.
    MalwareBlocking,
    /// Blocks malware, phishing, trackers, and advertisements.
    MalwareAndAdBlocking,
    /// Blocks malware and adult content.
    FamilySafe,
}

/// A named DNS-over-HTTPS resolver preset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DohPreset {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub endpoint_url: String,
    #[serde(default)]
    pub bootstrap_ips: Vec<IpAddr>,
    #[serde(default)]
    pub wire_format: DohWireFormat,
    #[serde(default)]
    pub filtering: DohFilteringPolicy,
    #[serde(default)]
    pub no_logs_policy: bool,
    #[serde(default = "default_true")]
    pub dnssec_validation: bool,
    #[serde(default)]
    pub ecs_disabled: bool,
    #[serde(default)]
    pub builtin: bool,
    #[serde(default)]
    pub description: String,
}

fn default_true() -> bool {
    true
}

impl DohPreset {
    /// Validates all fields of the preset, including HTTPS endpoint URL rules.
    pub fn validate(&self) -> Result<(), DohError> {
        validate_preset_id(&self.id)?;

        if self.name.trim().is_empty() {
            return Err(DohError::EmptyName);
        }
        if self.provider.trim().is_empty() {
            return Err(DohError::EmptyProvider);
        }

        let _ = self.normalized_endpoint()?;
        Ok(())
    }

    /// Parses and normalizes the endpoint URL, stripping an optional `{?dns}` RFC 8484 template suffix.
    pub fn normalized_endpoint(&self) -> Result<Url, DohError> {
        let trimmed = self.endpoint_url.trim();
        let without_template = trimmed.strip_suffix("{?dns}").unwrap_or(trimmed);
        let parsed = Url::parse(without_template)
            .map_err(|_| DohError::InvalidEndpointUrl(self.endpoint_url.clone()))?;

        if parsed.scheme() != "https" {
            return Err(DohError::InsecureScheme(parsed.scheme().to_string()));
        }
        if parsed.host_str().is_none() {
            return Err(DohError::MissingHost);
        }
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err(DohError::EmbeddedCredentials);
        }

        Ok(parsed)
    }

    /// Returns `true` if the endpoint URL includes an RFC 8484 `{?dns}` URI template suffix.
    pub fn uses_uri_template(&self) -> bool {
        self.endpoint_url.trim().ends_with("{?dns}")
    }
}

/// User-configurable DNS-over-HTTPS settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DohSettings {
    #[serde(default)]
    pub mode: DohMode,
    #[serde(default = "default_active_preset")]
    pub active_preset_id: Option<String>,
    #[serde(default)]
    pub custom_presets: Vec<DohPreset>,
    #[serde(default = "default_true")]
    pub fallback_to_system: bool,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u32,
    #[serde(default)]
    pub cache_ttl_override_secs: Option<u32>,
}

fn default_active_preset() -> Option<String> {
    Some("cloudflare".to_string())
}

fn default_timeout_ms() -> u32 {
    DEFAULT_DOH_TIMEOUT_MS
}

impl Default for DohSettings {
    fn default() -> Self {
        Self {
            mode: DohMode::Automatic,
            active_preset_id: default_active_preset(),
            custom_presets: Vec::new(),
            fallback_to_system: true,
            timeout_ms: DEFAULT_DOH_TIMEOUT_MS,
            cache_ttl_override_secs: None,
        }
    }
}

impl DohSettings {
    /// Validates settings against a preset registry.
    pub fn validate(&self, registry: &DohPresetRegistry) -> Result<(), DohError> {
        if !(MIN_DOH_TIMEOUT_MS..=MAX_DOH_TIMEOUT_MS).contains(&self.timeout_ms) {
            return Err(DohError::InvalidTimeout(self.timeout_ms));
        }

        if self.mode == DohMode::Strict && self.fallback_to_system {
            return Err(DohError::StrictModeWithFallback);
        }

        if self.mode != DohMode::Off {
            let active_id = self
                .active_preset_id
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .ok_or(DohError::MissingActivePreset(self.mode))?;

            if registry.get(active_id).is_none() {
                return Err(DohError::PresetNotFound(active_id.to_string()));
            }
        }

        for preset in &self.custom_presets {
            preset.validate()?;
        }

        Ok(())
    }

    /// Serializes settings to pretty-printed JSON.
    pub fn to_json(&self) -> Result<String, DohError> {
        serde_json::to_string_pretty(self).map_err(|e| DohError::Serialization(e.to_string()))
    }

    /// Deserializes settings from JSON.
    pub fn from_json(json: &str) -> Result<Self, DohError> {
        serde_json::from_str(json).map_err(|e| DohError::Serialization(e.to_string()))
    }
}

/// Registry containing built-in and user-defined DoH presets.
#[derive(Debug, Clone)]
pub struct DohPresetRegistry {
    presets: Vec<DohPreset>,
}

impl Default for DohPresetRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl DohPresetRegistry {
    /// Creates a registry populated with Search Linux's built-in DoH presets.
    pub fn new() -> Self {
        Self {
            presets: builtin_presets(),
        }
    }

    /// Builds a registry from built-in presets plus custom presets from `settings`, validating all entries.
    pub fn from_settings(settings: &DohSettings) -> Result<Self, DohError> {
        let mut registry = Self::new();
        for preset in &settings.custom_presets {
            registry.add_custom(preset.clone())?;
        }
        settings.validate(&registry)?;
        Ok(registry)
    }

    /// Returns a preset by its unique identifier.
    pub fn get(&self, id: &str) -> Option<&DohPreset> {
        self.presets.iter().find(|p| p.id == id)
    }

    /// Returns all registered presets in deterministic order.
    pub fn list(&self) -> Vec<&DohPreset> {
        self.presets.iter().collect()
    }

    /// Returns presets matching the given filtering policy.
    pub fn filter_by_policy(&self, policy: DohFilteringPolicy) -> Vec<&DohPreset> {
        self.presets
            .iter()
            .filter(|p| p.filtering == policy)
            .collect()
    }

    /// Returns presets that advertise both a no-logs policy and EDNS Client Subnet disabled.
    pub fn filter_privacy_first(&self) -> Vec<&DohPreset> {
        self.presets
            .iter()
            .filter(|p| p.no_logs_policy && p.ecs_disabled)
            .collect()
    }

    /// Resolves the active preset for the given settings, or `None` when `DohMode::Off`.
    pub fn resolve_active<'a>(
        &'a self,
        settings: &DohSettings,
    ) -> Result<Option<&'a DohPreset>, DohError> {
        if settings.mode == DohMode::Off {
            return Ok(None);
        }

        let active_id = settings
            .active_preset_id
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .ok_or(DohError::MissingActivePreset(settings.mode))?;

        self.get(active_id)
            .map(Some)
            .ok_or_else(|| DohError::PresetNotFound(active_id.to_string()))
    }

    /// Validates and adds a custom preset, forcing `builtin = false`.
    pub fn add_custom(&mut self, mut preset: DohPreset) -> Result<(), DohError> {
        preset.builtin = false;
        preset.validate()?;

        if self.presets.iter().any(|existing| existing.id == preset.id) {
            return Err(DohError::DuplicatePresetId(preset.id));
        }

        self.presets.push(preset);
        Ok(())
    }

    /// Removes a custom preset by id. Built-in presets cannot be removed.
    pub fn remove_custom(&mut self, id: &str) -> Result<DohPreset, DohError> {
        let Some(pos) = self.presets.iter().position(|p| p.id == id) else {
            return Err(DohError::PresetNotFound(id.to_string()));
        };

        if self.presets[pos].builtin {
            return Err(DohError::CannotDeleteBuiltin(id.to_string()));
        }

        Ok(self.presets.remove(pos))
    }

    /// Returns all unique bootstrap IP addresses across the active preset (if any).
    pub fn active_bootstrap_ips(&self, settings: &DohSettings) -> Result<Vec<IpAddr>, DohError> {
        let Some(preset) = self.resolve_active(settings)? else {
            return Ok(Vec::new());
        };

        let mut seen = HashSet::new();
        let mut ips = Vec::new();
        for &ip in &preset.bootstrap_ips {
            if seen.insert(ip) {
                ips.push(ip);
            }
        }
        Ok(ips)
    }
}

fn validate_preset_id(id: &str) -> Result<(), DohError> {
    if id.is_empty() {
        return Err(DohError::EmptyId);
    }
    if id.len() > 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(DohError::InvalidId(id.to_string()));
    }
    Ok(())
}

fn parse_ips(raw: &[&str]) -> Vec<IpAddr> {
    raw.iter()
        .filter_map(|s| s.parse::<IpAddr>().ok())
        .collect()
}

fn builtin_presets() -> Vec<DohPreset> {
    vec![
        DohPreset {
            id: "cloudflare".to_string(),
            name: "Cloudflare (1.1.1.1)".to_string(),
            provider: "Cloudflare".to_string(),
            endpoint_url: "https://cloudflare-dns.com/dns-query".to_string(),
            bootstrap_ips: parse_ips(&["1.1.1.1", "1.0.0.1", "2606:4700:4700::1111"]),
            wire_format: DohWireFormat::DnsWire,
            filtering: DohFilteringPolicy::Unfiltered,
            no_logs_policy: true,
            dnssec_validation: true,
            ecs_disabled: true,
            builtin: true,
            description: "Fast global Anycast resolver with DNSSEC and no ECS.".to_string(),
        },
        DohPreset {
            id: "cloudflare-family".to_string(),
            name: "Cloudflare Family".to_string(),
            provider: "Cloudflare".to_string(),
            endpoint_url: "https://family.cloudflare-dns.com/dns-query".to_string(),
            bootstrap_ips: parse_ips(&["1.1.1.3", "1.0.0.3", "2606:4700:4700::1113"]),
            wire_format: DohWireFormat::DnsWire,
            filtering: DohFilteringPolicy::FamilySafe,
            no_logs_policy: true,
            dnssec_validation: true,
            ecs_disabled: true,
            builtin: true,
            description: "Blocks malware and adult content.".to_string(),
        },
        DohPreset {
            id: "quad9".to_string(),
            name: "Quad9 (Recommended)".to_string(),
            provider: "Quad9".to_string(),
            endpoint_url: "https://dns.quad9.net/dns-query".to_string(),
            bootstrap_ips: parse_ips(&["9.9.9.9", "149.112.112.112", "2620:fe::fe"]),
            wire_format: DohWireFormat::DnsWire,
            filtering: DohFilteringPolicy::MalwareBlocking,
            no_logs_policy: true,
            dnssec_validation: true,
            ecs_disabled: true,
            builtin: true,
            description: "Swiss non-profit resolver with threat-intelligence malware blocking."
                .to_string(),
        },
        DohPreset {
            id: "mullvad".to_string(),
            name: "Mullvad DNS".to_string(),
            provider: "Mullvad".to_string(),
            endpoint_url: "https://dns.mullvad.net/dns-query".to_string(),
            bootstrap_ips: parse_ips(&["194.242.2.2", "2a07:e340::2"]),
            wire_format: DohWireFormat::DnsWire,
            filtering: DohFilteringPolicy::Unfiltered,
            no_logs_policy: true,
            dnssec_validation: true,
            ecs_disabled: true,
            builtin: true,
            description: "Privacy-focused unfiltered resolver operated by Mullvad.".to_string(),
        },
        DohPreset {
            id: "mullvad-adblock".to_string(),
            name: "Mullvad Adblock".to_string(),
            provider: "Mullvad".to_string(),
            endpoint_url: "https://adblock.dns.mullvad.net/dns-query".to_string(),
            bootstrap_ips: parse_ips(&["194.242.2.3", "2a07:e340::3"]),
            wire_format: DohWireFormat::DnsWire,
            filtering: DohFilteringPolicy::MalwareAndAdBlocking,
            no_logs_policy: true,
            dnssec_validation: true,
            ecs_disabled: true,
            builtin: true,
            description: "Mullvad resolver blocking ads, trackers, and malware.".to_string(),
        },
        DohPreset {
            id: "adguard".to_string(),
            name: "AdGuard DNS".to_string(),
            provider: "AdGuard".to_string(),
            endpoint_url: "https://dns.adguard-dns.com/dns-query".to_string(),
            bootstrap_ips: parse_ips(&["94.140.14.14", "94.140.15.15", "2a10:50c0::ad1:ff"]),
            wire_format: DohWireFormat::DnsWire,
            filtering: DohFilteringPolicy::MalwareAndAdBlocking,
            no_logs_policy: true,
            dnssec_validation: true,
            ecs_disabled: true,
            builtin: true,
            description: "Blocks ads, trackers, and phishing domains.".to_string(),
        },
        DohPreset {
            id: "opendns".to_string(),
            name: "Cisco OpenDNS".to_string(),
            provider: "Cisco".to_string(),
            endpoint_url: "https://doh.opendns.com/dns-query".to_string(),
            bootstrap_ips: parse_ips(&["208.67.222.222", "208.67.220.220"]),
            wire_format: DohWireFormat::DnsWire,
            filtering: DohFilteringPolicy::MalwareBlocking,
            no_logs_policy: false,
            dnssec_validation: true,
            ecs_disabled: false,
            builtin: true,
            description: "OpenDNS resolver with phishing and malware protection.".to_string(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_custom_preset(id: &str) -> DohPreset {
        DohPreset {
            id: id.to_string(),
            name: "Custom Home DoH".to_string(),
            provider: "Self-Hosted".to_string(),
            endpoint_url: "https://dns.example.org/dns-query{?dns}".to_string(),
            bootstrap_ips: parse_ips(&["192.0.2.53", "192.0.2.53"]),
            wire_format: DohWireFormat::DnsWire,
            filtering: DohFilteringPolicy::MalwareAndAdBlocking,
            no_logs_policy: true,
            dnssec_validation: true,
            ecs_disabled: true,
            builtin: true, // Intentionally true to verify add_custom overrides it.
            description: "Local Pi-hole / Unbound DoH gateway.".to_string(),
        }
    }

    #[test]
    fn builtin_presets_are_valid_and_discoverable() {
        let registry = DohPresetRegistry::new();
        let all = registry.list();
        assert!(all.len() >= 6);

        for preset in all {
            assert!(preset.builtin);
            assert!(
                preset.validate().is_ok(),
                "built-in preset {} failed validation",
                preset.id
            );
        }

        let privacy_first = registry.filter_privacy_first();
        assert!(privacy_first.iter().any(|p| p.id == "mullvad"));
        assert!(!privacy_first.iter().any(|p| p.id == "opendns"));

        let adblock = registry.filter_by_policy(DohFilteringPolicy::MalwareAndAdBlocking);
        assert!(adblock.iter().any(|p| p.id == "adguard"));
    }

    #[test]
    fn custom_preset_lifecycle_and_template_normalization() {
        let mut registry = DohPresetRegistry::new();
        let custom = sample_custom_preset("home-dns");
        assert!(custom.uses_uri_template());

        let normalized = custom.normalized_endpoint().unwrap();
        assert_eq!(normalized.as_str(), "https://dns.example.org/dns-query");

        registry.add_custom(custom).unwrap();
        let stored = registry.get("home-dns").unwrap();
        assert!(!stored.builtin);

        // Duplicate custom id is rejected.
        assert_eq!(
            registry.add_custom(sample_custom_preset("home-dns")),
            Err(DohError::DuplicatePresetId("home-dns".to_string()))
        );

        // Built-in preset cannot be deleted.
        assert_eq!(
            registry.remove_custom("cloudflare"),
            Err(DohError::CannotDeleteBuiltin("cloudflare".to_string()))
        );

        // Custom preset can be removed.
        let removed = registry.remove_custom("home-dns").unwrap();
        assert_eq!(removed.id, "home-dns");
        assert!(registry.get("home-dns").is_none());
    }

    #[test]
    fn settings_validation_and_json_roundtrip() {
        let custom = sample_custom_preset("lab-dns");
        let settings = DohSettings {
            mode: DohMode::Strict,
            active_preset_id: Some("lab-dns".to_string()),
            custom_presets: vec![custom],
            fallback_to_system: false,
            timeout_ms: 2_500,
            cache_ttl_override_secs: Some(300),
        };

        let json = settings.to_json().unwrap();
        let decoded = DohSettings::from_json(&json).unwrap();
        assert_eq!(settings, decoded);

        let registry = DohPresetRegistry::from_settings(&decoded).unwrap();
        let active = registry.resolve_active(&decoded).unwrap().unwrap();
        assert_eq!(active.id, "lab-dns");

        let ips = registry.active_bootstrap_ips(&decoded).unwrap();
        assert_eq!(ips.len(), 1);
        assert_eq!(ips[0], "192.0.2.53".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn rejects_invalid_urls_modes_and_timeouts() {
        let registry = DohPresetRegistry::new();

        let mut insecure = sample_custom_preset("http-only");
        insecure.endpoint_url = "http://dns.example.org/dns-query".to_string();
        assert_eq!(
            insecure.validate(),
            Err(DohError::InsecureScheme("http".to_string()))
        );

        let mut with_creds = sample_custom_preset("creds");
        with_creds.endpoint_url = "https://user:pass@dns.example.org/dns-query".to_string();
        assert_eq!(with_creds.validate(), Err(DohError::EmbeddedCredentials));

        let strict_with_fallback = DohSettings {
            mode: DohMode::Strict,
            fallback_to_system: true,
            ..DohSettings::default()
        };
        assert_eq!(
            strict_with_fallback.validate(&registry),
            Err(DohError::StrictModeWithFallback)
        );

        let bad_timeout = DohSettings {
            timeout_ms: 50,
            ..DohSettings::default()
        };
        assert_eq!(
            bad_timeout.validate(&registry),
            Err(DohError::InvalidTimeout(50))
        );

        let off_mode = DohSettings {
            mode: DohMode::Off,
            active_preset_id: None,
            ..DohSettings::default()
        };
        assert!(off_mode.validate(&registry).is_ok());
        assert_eq!(registry.resolve_active(&off_mode).unwrap(), None);
    }
}
