use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;
use zeroize::Zeroize;

/// Errors returned by proxy profile configuration, validation, and resolution.
#[derive(Debug, Error)]
pub enum ProxyError {
    #[error("proxy profile id must not be empty")]
    EmptyProfileId,
    #[error("proxy profile name must not be empty")]
    EmptyProfileName,
    #[error("duplicate proxy profile id: {0}")]
    DuplicateProfileId(String),
    #[error("proxy profile not found: {0}")]
    ProfileNotFound(String),
    #[error("invalid proxy host: {0}")]
    InvalidHost(String),
    #[error("invalid proxy port: {0}")]
    InvalidPort(u16),
    #[error("invalid PAC script url: {0}")]
    InvalidPacUrl(String),
    #[error("invalid bypass rule: {0}")]
    InvalidBypassRule(String),
    #[error("invalid target url: {0}")]
    InvalidUrl(String),
    #[error("manual proxy mode requires at least one proxy endpoint")]
    MissingManualServers,
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Supported proxy transport protocols.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyScheme {
    Http,
    Https,
    Socks4,
    Socks5,
}

impl ProxyScheme {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
            Self::Socks4 => "socks4",
            Self::Socks5 => "socks5",
        }
    }

    pub fn default_port(&self) -> u16 {
        match self {
            Self::Http => 80,
            Self::Https => 443,
            Self::Socks4 | Self::Socks5 => 1080,
        }
    }
}

impl fmt::Display for ProxyScheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Optional proxy authentication credentials with automatic memory zeroization on drop.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyAuth {
    pub username: String,
    pub password: String,
}

impl ProxyAuth {
    pub fn new(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            username: username.into(),
            password: password.into(),
        }
    }

    /// Explicitly clears sensitive credential material from memory.
    pub fn clear(&mut self) {
        self.username.zeroize();
        self.password.zeroize();
    }
}

impl Drop for ProxyAuth {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

impl fmt::Debug for ProxyAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProxyAuth")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .finish()
    }
}

/// A single proxy server endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyEndpoint {
    pub scheme: ProxyScheme,
    pub host: String,
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<ProxyAuth>,
}

impl ProxyEndpoint {
    pub fn new(scheme: ProxyScheme, host: impl Into<String>, port: u16) -> Self {
        Self {
            scheme,
            host: host.into(),
            port,
            auth: None,
        }
    }

    pub fn with_auth(mut self, auth: ProxyAuth) -> Self {
        self.auth = Some(auth);
        self
    }

    pub fn validate(&self) -> Result<(), ProxyError> {
        validate_host(&self.host)?;
        if self.port == 0 {
            return Err(ProxyError::InvalidPort(self.port));
        }
        Ok(())
    }

    /// Formats the endpoint as a URI without embedding credentials.
    pub fn to_proxy_uri(&self) -> String {
        if self.host.contains(':') && !self.host.starts_with('[') {
            format!("{}://[{}]:{}", self.scheme, self.host, self.port)
        } else {
            format!("{}://{}:{}", self.scheme, self.host, self.port)
        }
    }
}

/// Per-protocol proxy servers and bypass rules for manual proxy configuration.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ManualProxyRules {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<ProxyEndpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http: Option<ProxyEndpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub https: Option<ProxyEndpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub socks: Option<ProxyEndpoint>,
    #[serde(default)]
    pub bypass_list: Vec<String>,
}

impl ManualProxyRules {
    pub fn validate(&self) -> Result<(), ProxyError> {
        let mut has_any = false;
        for endpoint in [&self.default, &self.http, &self.https, &self.socks]
            .into_iter()
            .flatten()
        {
            has_any = true;
            endpoint.validate()?;
        }
        if !has_any {
            return Err(ProxyError::MissingManualServers);
        }
        for rule in &self.bypass_list {
            validate_bypass_rule(rule)?;
        }
        Ok(())
    }

    /// Selects the configured proxy endpoint for the given target URL scheme.
    pub fn endpoint_for_scheme(&self, scheme: &str) -> Option<&ProxyEndpoint> {
        match scheme.to_ascii_lowercase().as_str() {
            "http" => self
                .http
                .as_ref()
                .or(self.default.as_ref())
                .or(self.socks.as_ref()),
            "https" | "wss" => self
                .https
                .as_ref()
                .or(self.default.as_ref())
                .or(self.socks.as_ref()),
            "ws" => self
                .http
                .as_ref()
                .or(self.default.as_ref())
                .or(self.socks.as_ref()),
            _ => self.default.as_ref().or(self.socks.as_ref()),
        }
    }

    /// Evaluates whether the target URL matches any bypass rule.
    pub fn should_bypass(&self, target: &Url) -> Result<bool, ProxyError> {
        let Some(host) = target.host_str() else {
            return Err(ProxyError::InvalidUrl(target.as_str().to_string()));
        };
        let normalized_host = trim_brackets(host).to_ascii_lowercase();
        let target_port = target.port_or_known_default();
        let parsed_ip = normalized_host.parse::<IpAddr>().ok();

        for raw_rule in &self.bypass_list {
            let rule = raw_rule.trim();
            if rule.eq_ignore_ascii_case("<local>") {
                if is_local_host(&normalized_host, parsed_ip) {
                    return Ok(true);
                }
                continue;
            }

            if let Some((net_ip, prefix)) = parse_cidr(rule) {
                if let Some(ip) = parsed_ip {
                    if ip_in_cidr(ip, net_ip, prefix) {
                        return Ok(true);
                    }
                }
                continue;
            }

            let (rule_host, rule_port) = split_host_port(rule);
            if let Some(expected_port) = rule_port {
                if target_port != Some(expected_port) {
                    continue;
                }
            }

            let rule_host_norm = trim_brackets(rule_host).to_ascii_lowercase();
            if let Some(suffix) = rule_host_norm.strip_prefix("*.") {
                if normalized_host == suffix
                    || (normalized_host.ends_with(suffix)
                        && normalized_host
                            .as_bytes()
                            .get(normalized_host.len().saturating_sub(suffix.len() + 1))
                            == Some(&b'.'))
                {
                    return Ok(true);
                }
            } else if let Some(suffix) = rule_host_norm.strip_prefix('.') {
                if normalized_host == suffix
                    || (normalized_host.ends_with(suffix)
                        && normalized_host
                            .as_bytes()
                            .get(normalized_host.len().saturating_sub(suffix.len() + 1))
                            == Some(&b'.'))
                {
                    return Ok(true);
                }
            } else if normalized_host == rule_host_norm {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Formats manual proxy rules into Chromium's `--proxy-server` syntax.
    pub fn to_chromium_proxy_rules(&self) -> String {
        let mut parts = Vec::new();
        if let Some(ep) = &self.http {
            parts.push(format!("http={}", ep.to_proxy_uri()));
        }
        if let Some(ep) = &self.https {
            parts.push(format!("https={}", ep.to_proxy_uri()));
        }
        if let Some(ep) = &self.socks {
            parts.push(format!("socks={}", ep.to_proxy_uri()));
        }
        if parts.is_empty() {
            if let Some(ep) = &self.default {
                return ep.to_proxy_uri();
            }
        } else if let Some(ep) = &self.default {
            parts.push(ep.to_proxy_uri());
        }
        parts.join(";")
    }
}

/// High-level proxy mode for a profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum ProxyMode {
    Direct,
    System,
    AutoDetect,
    PacScript { url: String },
    Manual(ManualProxyRules),
}

/// Named proxy configuration profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyProfile {
    pub id: String,
    pub name: String,
    pub mode: ProxyMode,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub dns_over_proxy: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

fn default_true() -> bool {
    true
}

impl ProxyProfile {
    pub fn new(id: impl Into<String>, name: impl Into<String>, mode: ProxyMode) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            mode,
            enabled: true,
            dns_over_proxy: true,
            notes: None,
        }
    }

    pub fn validate(&self) -> Result<(), ProxyError> {
        if self.id.trim().is_empty() {
            return Err(ProxyError::EmptyProfileId);
        }
        if self.name.trim().is_empty() {
            return Err(ProxyError::EmptyProfileName);
        }
        match &self.mode {
            ProxyMode::Direct | ProxyMode::System | ProxyMode::AutoDetect => Ok(()),
            ProxyMode::PacScript { url } => validate_pac_url(url),
            ProxyMode::Manual(rules) => rules.validate(),
        }
    }

    /// Resolves how a given target URL should be routed under this profile.
    pub fn resolve_for_url(&self, raw_url: &str) -> Result<ProxyResolution, ProxyError> {
        let parsed =
            Url::parse(raw_url).map_err(|_| ProxyError::InvalidUrl(raw_url.to_string()))?;
        if !self.enabled {
            return Ok(ProxyResolution::Direct);
        }

        match &self.mode {
            ProxyMode::Direct => Ok(ProxyResolution::Direct),
            ProxyMode::System => Ok(ProxyResolution::System),
            ProxyMode::AutoDetect => Ok(ProxyResolution::AutoDetect),
            ProxyMode::PacScript { url } => Ok(ProxyResolution::PacScript(url.clone())),
            ProxyMode::Manual(rules) => {
                if rules.should_bypass(&parsed)? {
                    return Ok(ProxyResolution::Direct);
                }
                match rules.endpoint_for_scheme(parsed.scheme()) {
                    Some(endpoint) => Ok(ProxyResolution::Via {
                        endpoint: endpoint.clone(),
                        dns_over_proxy: self.dns_over_proxy,
                    }),
                    None => Ok(ProxyResolution::Direct),
                }
            }
        }
    }

    /// Produces Chromium/CEF command-line flags representing this profile.
    pub fn to_chromium_args(&self) -> Vec<String> {
        if !self.enabled {
            return vec!["--no-proxy-server".to_string()];
        }

        match &self.mode {
            ProxyMode::Direct => vec!["--no-proxy-server".to_string()],
            ProxyMode::System => Vec::new(),
            ProxyMode::AutoDetect => vec!["--proxy-auto-detect".to_string()],
            ProxyMode::PacScript { url } => vec![format!("--proxy-pac-url={url}")],
            ProxyMode::Manual(rules) => {
                let mut args = Vec::new();
                let server_spec = rules.to_chromium_proxy_rules();
                if !server_spec.is_empty() {
                    args.push(format!("--proxy-server={server_spec}"));
                }
                if !rules.bypass_list.is_empty() {
                    args.push(format!(
                        "--proxy-bypass-list={}",
                        rules.bypass_list.join(";")
                    ));
                }
                if self.dns_over_proxy {
                    if let Some(ep) = rules.socks.as_ref().or(rules.default.as_ref()) {
                        if matches!(ep.scheme, ProxyScheme::Socks5) {
                            args.push(format!(
                                "--host-resolver-rules=MAP * ~NOTFOUND , EXCLUDE {}",
                                ep.host
                            ));
                        }
                    }
                }
                args
            }
        }
    }
}

/// Outcome of resolving a URL against a proxy profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProxyResolution {
    Direct,
    System,
    AutoDetect,
    PacScript(String),
    Via {
        endpoint: ProxyEndpoint,
        dns_over_proxy: bool,
    },
}

/// Persistent collection of proxy profiles with an optional active profile selection.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ProxyProfileStore {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_profile_id: Option<String>,
    #[serde(default)]
    pub profiles: Vec<ProxyProfile>,
}

impl ProxyProfileStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn validate(&self) -> Result<(), ProxyError> {
        let mut seen = Vec::with_capacity(self.profiles.len());
        for profile in &self.profiles {
            profile.validate()?;
            if seen.iter().any(|id| id == &profile.id) {
                return Err(ProxyError::DuplicateProfileId(profile.id.clone()));
            }
            seen.push(profile.id.clone());
        }
        if let Some(active_id) = &self.active_profile_id {
            if !self.profiles.iter().any(|p| &p.id == active_id) {
                return Err(ProxyError::ProfileNotFound(active_id.clone()));
            }
        }
        Ok(())
    }

    pub fn add_profile(&mut self, profile: ProxyProfile) -> Result<(), ProxyError> {
        profile.validate()?;
        if self.profiles.iter().any(|p| p.id == profile.id) {
            return Err(ProxyError::DuplicateProfileId(profile.id));
        }
        self.profiles.push(profile);
        Ok(())
    }

    pub fn update_profile(&mut self, profile: ProxyProfile) -> Result<(), ProxyError> {
        profile.validate()?;
        let slot = self
            .profiles
            .iter_mut()
            .find(|p| p.id == profile.id)
            .ok_or_else(|| ProxyError::ProfileNotFound(profile.id.clone()))?;
        *slot = profile;
        Ok(())
    }

    pub fn remove_profile(&mut self, id: &str) -> Result<ProxyProfile, ProxyError> {
        let idx = self
            .profiles
            .iter()
            .position(|p| p.id == id)
            .ok_or_else(|| ProxyError::ProfileNotFound(id.to_string()))?;
        let removed = self.profiles.remove(idx);
        if self.active_profile_id.as_deref() == Some(id) {
            self.active_profile_id = None;
        }
        Ok(removed)
    }

    pub fn get_profile(&self, id: &str) -> Option<&ProxyProfile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    pub fn set_active(&mut self, id: Option<&str>) -> Result<(), ProxyError> {
        match id {
            None => {
                self.active_profile_id = None;
                Ok(())
            }
            Some(target) => {
                if self.profiles.iter().any(|p| p.id == target) {
                    self.active_profile_id = Some(target.to_string());
                    Ok(())
                } else {
                    Err(ProxyError::ProfileNotFound(target.to_string()))
                }
            }
        }
    }

    pub fn active_profile(&self) -> Option<&ProxyProfile> {
        let id = self.active_profile_id.as_deref()?;
        self.get_profile(id)
    }

    pub fn resolve_url(&self, raw_url: &str) -> Result<ProxyResolution, ProxyError> {
        match self.active_profile() {
            Some(profile) => profile.resolve_for_url(raw_url),
            None => {
                let _ =
                    Url::parse(raw_url).map_err(|_| ProxyError::InvalidUrl(raw_url.to_string()))?;
                Ok(ProxyResolution::System)
            }
        }
    }

    pub fn to_json(&self) -> Result<String, ProxyError> {
        self.validate()?;
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn from_json(json: &str) -> Result<Self, ProxyError> {
        let store: Self = serde_json::from_str(json)?;
        store.validate()?;
        Ok(store)
    }
}

fn validate_host(raw: &str) -> Result<(), ProxyError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_whitespace) {
        return Err(ProxyError::InvalidHost(raw.to_string()));
    }
    let unbracketed = trim_brackets(trimmed);
    if unbracketed.is_empty() {
        return Err(ProxyError::InvalidHost(raw.to_string()));
    }
    if unbracketed.parse::<IpAddr>().is_ok() {
        return Ok(());
    }
    if unbracketed.contains('/') || unbracketed.contains('?') || unbracketed.contains('#') {
        return Err(ProxyError::InvalidHost(raw.to_string()));
    }
    Ok(())
}

fn validate_pac_url(raw: &str) -> Result<(), ProxyError> {
    let parsed = Url::parse(raw).map_err(|_| ProxyError::InvalidPacUrl(raw.to_string()))?;
    match parsed.scheme() {
        "http" | "https" | "file" | "data" => Ok(()),
        _ => Err(ProxyError::InvalidPacUrl(raw.to_string())),
    }
}

fn validate_bypass_rule(raw: &str) -> Result<(), ProxyError> {
    let rule = raw.trim();
    if rule.is_empty() || rule.chars().any(char::is_whitespace) {
        return Err(ProxyError::InvalidBypassRule(raw.to_string()));
    }
    if rule.eq_ignore_ascii_case("<local>") {
        return Ok(());
    }
    if rule.contains('/') {
        return if parse_cidr(rule).is_some() {
            Ok(())
        } else {
            Err(ProxyError::InvalidBypassRule(raw.to_string()))
        };
    }
    let (host, port_opt) = split_host_port(rule);
    if let Some(port) = port_opt {
        if port == 0 {
            return Err(ProxyError::InvalidBypassRule(raw.to_string()));
        }
    }
    let stripped = host
        .strip_prefix("*.")
        .or_else(|| host.strip_prefix('.'))
        .unwrap_or(host);
    validate_host(stripped).map_err(|_| ProxyError::InvalidBypassRule(raw.to_string()))
}

fn trim_brackets(s: &str) -> &str {
    if let Some(inner) = s.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
        inner
    } else {
        s
    }
}

fn split_host_port(rule: &str) -> (&str, Option<u16>) {
    if let Some(rest) = rule.strip_prefix('[') {
        if let Some(end) = rest.find(']') {
            let host = &rule[..=end + 1];
            let tail = &rest[end + 1..];
            if let Some(port_str) = tail.strip_prefix(':') {
                if let Ok(port) = port_str.parse::<u16>() {
                    return (host, Some(port));
                }
            }
            return (host, None);
        }
    }
    if let Some((host, port_str)) = rule.rsplit_once(':') {
        if !host.contains(':') {
            if let Ok(port) = port_str.parse::<u16>() {
                return (host, Some(port));
            }
        }
    }
    (rule, None)
}

fn parse_cidr(rule: &str) -> Option<(IpAddr, u8)> {
    let (ip_str, prefix_str) = rule.split_once('/')?;
    let ip = trim_brackets(ip_str).parse::<IpAddr>().ok()?;
    let prefix = prefix_str.parse::<u8>().ok()?;
    match ip {
        IpAddr::V4(_) if prefix <= 32 => Some((ip, prefix)),
        IpAddr::V6(_) if prefix <= 128 => Some((ip, prefix)),
        _ => None,
    }
}

fn ip_in_cidr(ip: IpAddr, net: IpAddr, prefix: u8) -> bool {
    match (ip, net) {
        (IpAddr::V4(a), IpAddr::V4(b)) => {
            let mask = if prefix == 0 {
                0u32
            } else {
                u32::MAX << (32 - u32::from(prefix))
            };
            (u32::from(a) & mask) == (u32::from(b) & mask)
        }
        (IpAddr::V6(a), IpAddr::V6(b)) => {
            let mask = if prefix == 0 {
                0u128
            } else {
                u128::MAX << (128 - u32::from(prefix))
            };
            (u128::from(a) & mask) == (u128::from(b) & mask)
        }
        _ => false,
    }
}

fn is_local_host(host: &str, parsed_ip: Option<IpAddr>) -> bool {
    if host == "localhost" || !host.contains('.') && parsed_ip.is_none() {
        return true;
    }
    match parsed_ip {
        Some(IpAddr::V4(v4)) => v4.is_loopback() || v4 == Ipv4Addr::UNSPECIFIED,
        Some(IpAddr::V6(v6)) => v6.is_loopback() || v6 == Ipv6Addr::UNSPECIFIED,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manual_profile() -> ProxyProfile {
        let rules = ManualProxyRules {
            default: Some(ProxyEndpoint::new(ProxyScheme::Socks5, "127.0.0.1", 1080)),
            http: Some(
                ProxyEndpoint::new(ProxyScheme::Http, "proxy.corp.local", 8080)
                    .with_auth(ProxyAuth::new("alice", "s3cr3t")),
            ),
            https: None,
            socks: None,
            bypass_list: vec![
                "<local>".to_string(),
                "10.0.0.0/8".to_string(),
                "*.internal.example".to_string(),
                "exact.host:8443".to_string(),
            ],
        };
        ProxyProfile::new("work", "Work VPN", ProxyMode::Manual(rules))
    }

    #[test]
    fn resolves_schemes_and_bypass_rules() {
        let profile = sample_manual_profile();
        profile.validate().unwrap();

        // HTTP uses the HTTP-specific proxy.
        let res_http = profile.resolve_for_url("http://example.com/news").unwrap();
        match res_http {
            ProxyResolution::Via {
                endpoint,
                dns_over_proxy,
            } => {
                assert_eq!(endpoint.scheme, ProxyScheme::Http);
                assert_eq!(endpoint.host, "proxy.corp.local");
                assert_eq!(endpoint.port, 8080);
                assert!(dns_over_proxy);
            }
            other => panic!("unexpected resolution: {other:?}"),
        }

        // HTTPS falls back to default SOCKS5 endpoint.
        let res_https = profile.resolve_for_url("https://example.org/").unwrap();
        match res_https {
            ProxyResolution::Via { endpoint, .. } => {
                assert_eq!(endpoint.scheme, ProxyScheme::Socks5);
                assert_eq!(endpoint.port, 1080);
            }
            other => panic!("unexpected resolution: {other:?}"),
        }

        // Bypass rules: <local>, CIDR, wildcard, and host:port.
        assert_eq!(
            profile.resolve_for_url("http://localhost:3000").unwrap(),
            ProxyResolution::Direct
        );
        assert_eq!(
            profile.resolve_for_url("http://intranet/docs").unwrap(),
            ProxyResolution::Direct
        );
        assert_eq!(
            profile.resolve_for_url("https://10.42.0.19/api").unwrap(),
            ProxyResolution::Direct
        );
        assert_eq!(
            profile
                .resolve_for_url("https://git.internal.example/repo")
                .unwrap(),
            ProxyResolution::Direct
        );
        assert_eq!(
            profile.resolve_for_url("https://exact.host:8443/").unwrap(),
            ProxyResolution::Direct
        );
        // Different port on exact.host should NOT bypass.
        assert!(matches!(
            profile.resolve_for_url("https://exact.host:9443/").unwrap(),
            ProxyResolution::Via { .. }
        ));
    }

    #[test]
    fn store_crud_validation_and_json_roundtrip() {
        let mut store = ProxyProfileStore::new();
        let pac = ProxyProfile::new(
            "office-pac",
            "Office PAC",
            ProxyMode::PacScript {
                url: "https://pac.corp.example/proxy.pac".to_string(),
            },
        );
        let manual = sample_manual_profile();

        store.add_profile(pac).unwrap();
        store.add_profile(manual).unwrap();
        store.set_active(Some("work")).unwrap();

        let json = store.to_json().unwrap();
        let restored = ProxyProfileStore::from_json(&json).unwrap();
        assert_eq!(restored, store);
        assert_eq!(
            restored.active_profile().map(|p| p.id.as_str()),
            Some("work")
        );

        // Removing active profile clears active_profile_id.
        let mut mutable = restored;
        mutable.remove_profile("work").unwrap();
        assert_eq!(mutable.active_profile_id, None);
        assert_eq!(
            mutable.resolve_url("https://example.com").unwrap(),
            ProxyResolution::System
        );
    }

    #[test]
    fn rejects_invalid_profiles_and_rules() {
        let empty_manual =
            ProxyProfile::new("bad", "Bad", ProxyMode::Manual(ManualProxyRules::default()));
        assert!(matches!(
            empty_manual.validate(),
            Err(ProxyError::MissingManualServers)
        ));

        let bad_pac = ProxyProfile::new(
            "bad-pac",
            "Bad PAC",
            ProxyMode::PacScript {
                url: "ftp://example.com/proxy.pac".to_string(),
            },
        );
        assert!(matches!(
            bad_pac.validate(),
            Err(ProxyError::InvalidPacUrl(_))
        ));

        let bad_cidr = ManualProxyRules {
            default: Some(ProxyEndpoint::new(ProxyScheme::Http, "127.0.0.1", 8080)),
            bypass_list: vec!["10.0.0.0/33".to_string()],
            ..Default::default()
        };
        assert!(matches!(
            bad_cidr.validate(),
            Err(ProxyError::InvalidBypassRule(_))
        ));

        let bad_port = ProxyEndpoint::new(ProxyScheme::Http, "127.0.0.1", 0);
        assert!(matches!(
            bad_port.validate(),
            Err(ProxyError::InvalidPort(0))
        ));
    }

    #[test]
    fn chromium_args_and_debug_redaction() {
        let profile = sample_manual_profile();
        let args = profile.to_chromium_args();
        assert!(args
            .iter()
            .any(|a| a.starts_with("--proxy-server=http=http://proxy.corp.local:8080;")));
        assert!(args
            .iter()
            .any(|a| a.starts_with("--proxy-bypass-list=<local>;10.0.0.0/8;")));
        assert!(args
            .iter()
            .any(|a| a == "--host-resolver-rules=MAP * ~NOTFOUND , EXCLUDE 127.0.0.1"));

        let auth = ProxyAuth::new("bob", "topsecret");
        let debug_str = format!("{auth:?}");
        assert!(!debug_str.contains("topsecret"));
        assert!(debug_str.contains("[REDACTED]"));
    }
}
