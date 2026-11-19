//! Motor declarativo de bloqueo de anuncios y rastreadores (`Shield.swift` + `YouTubeAds.swift`).
//!
//! Compila las reglas embebidas en formato compatible con `WKContentRuleList`:
//! - 44 dominios de anuncios y rastreadores de terceros (`UNWANTED_DOMAINS`).
//! - 1 regla cosmética global con los 23 selectores fiables (`COSMETIC_SLOTS`).
//! - 9 reglas de red específicas para rutas publicitarias de YouTube (`YOUTUBE_AD_PATHS`,
//!   `qoe?adformat=`, `google.com/pagead/` e `imasdk.googleapis.com` en YouTube).
//! - 4 reglas cosméticas específicas por sitio (`SITE_COSMETIC_SLOTS`).
//!
//! La lista serializada canónicamente se guarda en caché por hash SHA-256 bajo
//! `$XDG_CACHE_HOME/search/filters/office-shield-<sha256>.json`, reutilizando la caché si
//! el hash no cambia y limpiando entradas antiguas del mismo prefijo.

use crate::domain::{is_same_site_url, matches_domain_pattern, raw_url_host, site_host};
use crate::paths;
use anyhow::{Context, Result};
use regex::RegexSet;
use ring::digest;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Prefijo de archivos gestionados por Shield dentro de `$XDG_CACHE_HOME/search/filters`.
pub const CACHE_PREFIX: &str = "office-shield-";

/// Terceros dedicados exclusivamente a publicidad o seguimiento (`Shield.unwanted`).
/// Las peticiones de origen (first-party) jamás se bloquean mediante esta lista.
pub const UNWANTED_DOMAINS: &[&str] = &[
    "doubleclick.net",
    "googlesyndication.com",
    "googleadservices.com",
    "googletagservices.com",
    "google-analytics.com",
    "googletagmanager.com",
    "adservice.google.com",
    "amazon-adsystem.com",
    "adnxs.com",
    "adsrvr.org",
    "criteo.com",
    "criteo.net",
    "taboola.com",
    "outbrain.com",
    "rubiconproject.com",
    "pubmatic.com",
    "openx.net",
    "casalemedia.com",
    "smartadserver.com",
    "sharethrough.com",
    "indexww.com",
    "bidswitch.net",
    "33across.com",
    "teads.tv",
    "moatads.com",
    "adroll.com",
    "scorecardresearch.com",
    "quantserve.com",
    "chartbeat.com",
    "hotjar.com",
    "mouseflow.com",
    "fullstory.com",
    "clarity.ms",
    "mixpanel.com",
    "amplitude.com",
    "segment.com",
    "segment.io",
    "branch.io",
    "appsflyer.com",
    "adjust.com",
    "analytics.tiktok.com",
    "connect.facebook.net",
    "ads-twitter.com",
    "analytics.twitter.com",
];

/// Contenedores cosméticos fiables que solo contienen publicidad (`Shield.slots`).
pub const COSMETIC_SLOTS: &[&str] = &[
    ".adsbygoogle",
    "ins.adsbygoogle",
    r#"[id^="google_ads_"]"#,
    r#"[id^="div-gpt-ad"]"#,
    r#"[id^="taboola-"]"#,
    "#taboola-below-article",
    r#"iframe[src*="doubleclick.net"]"#,
    r#"iframe[src*="googlesyndication"]"#,
    r#"iframe[src*="amazon-adsystem"]"#,
    ".ad-slot",
    ".ad-slot-container",
    ".top-banner-ad-container",
    ".ad-leaderboard",
    ".ad-billboard",
    ".ad-giga",
    ".ad-mpu",
    ".ad-mrec",
    ".ad-unit",
    ".adunit",
    ".adslot",
    ".dfp-ad",
    ".gpt-ad",
    ".w_ad",
];

/// Reglas cosméticas acotadas a sitios concretos (`Shield.slotsBySite`).
pub const SITE_COSMETIC_SLOTS: &[(&[&str], &str)] = &[
    (&["*as.com", "*elpais.com"], ".ad"),
    (&["*theguardian.com"], ".top-fronts-banner-ad-container"),
    (
        &["*independent.co.uk", "*the-independent.com"],
        "#billboard-wrapper",
    ),
    (&["*cnn.com"], ".ad-slot-header__wrapper"),
];

/// Hosts exactos de YouTube donde operan los scripts anti-anuncios (`YouTubeAds.hosts`).
pub const YOUTUBE_HOSTS: &[&str] = &[
    "youtube.com",
    "www.youtube.com",
    "m.youtube.com",
    "music.youtube.com",
    "youtube-nocookie.com",
    "www.youtube-nocookie.com",
];

/// Rutas de anuncios de YouTube bloqueadas a nivel de red (`YouTubeAds.paths`).
pub const YOUTUBE_AD_PATHS: &[&str] = &[
    "/api/stats/ads",
    "/pagead/",
    "/ptracking",
    "/get_midroll_info",
    "/youtubei/v1/player/ad_break",
    "/pcs/activeview",
];

/// Regla declarativa compatible con `WKContentRuleList`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentRule {
    pub action: RuleAction,
    pub trigger: RuleTrigger,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleTrigger {
    #[serde(rename = "if-domain", default, skip_serializing_if = "Vec::is_empty")]
    pub if_domain: Vec<String>,
    #[serde(rename = "load-type", default, skip_serializing_if = "Vec::is_empty")]
    pub load_type: Vec<String>,
    #[serde(
        rename = "unless-domain",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub unless_domain: Vec<String>,
    #[serde(rename = "url-filter")]
    pub url_filter: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleAction {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    #[serde(rename = "type")]
    pub action_type: String,
}

/// Filtro regex exacto para un dominio y sus subdominios (`Shield.networkFilter(for:)`).
pub fn network_filter_for_domain(domain: &str) -> String {
    let escaped = domain.replace('.', r"\.");
    format!(r"^https?://([^/]+\.)?{escaped}[:/]")
}

/// Filtro regex para una ruta de anuncios en YouTube (`YouTubeAds.filter(path:)`).
pub fn youtube_path_filter(path: &str) -> String {
    let escaped = path.replace('.', r"\.");
    format!(r"^https?://([^/]+\.)?youtube\.com{escaped}")
}

/// Construye las 9 reglas de red específicas de YouTube (`YouTubeAds.rules`).
pub fn youtube_ad_rules() -> Vec<ContentRule> {
    let mut rules = Vec::with_capacity(YOUTUBE_AD_PATHS.len() + 3);
    for &path in YOUTUBE_AD_PATHS {
        rules.push(ContentRule {
            trigger: RuleTrigger {
                url_filter: youtube_path_filter(path),
                load_type: Vec::new(),
                if_domain: Vec::new(),
                unless_domain: Vec::new(),
            },
            action: RuleAction {
                action_type: "block".to_string(),
                selector: None,
            },
        });
    }

    // Informes de calidad QoE exclusivos de anuncios (nunca del vídeo).
    rules.push(ContentRule {
        trigger: RuleTrigger {
            url_filter: r"^https?://([^/]+\.)?youtube\.com/api/stats/qoe\?.*adformat=".to_string(),
            load_type: Vec::new(),
            if_domain: Vec::new(),
            unless_domain: Vec::new(),
        },
        action: RuleAction {
            action_type: "block".to_string(),
            selector: None,
        },
    });

    // Rutas de anuncios de Google y SDK IMA solo desde páginas de YouTube.
    let yt_domains = vec![
        "*youtube.com".to_string(),
        "*youtube-nocookie.com".to_string(),
    ];
    for filter in [
        r"^https?://([^/]+\.)?google\.com/pagead/",
        r"^https?://imasdk\.googleapis\.com/",
    ] {
        rules.push(ContentRule {
            trigger: RuleTrigger {
                url_filter: filter.to_string(),
                load_type: Vec::new(),
                if_domain: yt_domains.clone(),
                unless_domain: Vec::new(),
            },
            action: RuleAction {
                action_type: "block".to_string(),
                selector: None,
            },
        });
    }

    rules
}

/// Construye la lista completa embebida de Shield + YouTubeAds en formato `WKContentRuleList`.
pub fn embedded_rules() -> Vec<ContentRule> {
    let mut rules = Vec::with_capacity(
        UNWANTED_DOMAINS.len() + 1 + YOUTUBE_AD_PATHS.len() + 3 + SITE_COSMETIC_SLOTS.len(),
    );

    // 44 reglas de bloqueo de anuncios y rastreadores de terceros.
    for &domain in UNWANTED_DOMAINS {
        rules.push(ContentRule {
            trigger: RuleTrigger {
                url_filter: network_filter_for_domain(domain),
                load_type: vec!["third-party".to_string()],
                if_domain: Vec::new(),
                unless_domain: Vec::new(),
            },
            action: RuleAction {
                action_type: "block".to_string(),
                selector: None,
            },
        });
    }

    // 1 regla cosmética global.
    rules.push(ContentRule {
        trigger: RuleTrigger {
            url_filter: ".*".to_string(),
            load_type: Vec::new(),
            if_domain: Vec::new(),
            unless_domain: Vec::new(),
        },
        action: RuleAction {
            action_type: "css-display-none".to_string(),
            selector: Some(COSMETIC_SLOTS.join(", ")),
        },
    });

    // 9 reglas de red de YouTube.
    rules.extend(youtube_ad_rules());

    // 4 reglas cosméticas por sitio.
    for &(sites, selector) in SITE_COSMETIC_SLOTS {
        rules.push(ContentRule {
            trigger: RuleTrigger {
                url_filter: ".*".to_string(),
                load_type: Vec::new(),
                if_domain: sites.iter().map(|s| (*s).to_string()).collect(),
                unless_domain: Vec::new(),
            },
            action: RuleAction {
                action_type: "css-display-none".to_string(),
                selector: Some(selector.to_string()),
            },
        });
    }

    rules
}

/// Serializa una lista de reglas a JSON determinista (claves en orden estable por declaración).
pub fn canonical_rules_json(rules: &[ContentRule]) -> Result<Vec<u8>> {
    serde_json::to_vec(rules).context("No se pudo serializar la lista de reglas de Shield")
}

/// Calcula el hash SHA-256 en hexadecimal minúsculas (64 caracteres).
pub fn sha256_hex(bytes: &[u8]) -> String {
    let sum = digest::digest(&digest::SHA256, bytes);
    let mut out = String::with_capacity(64);
    for b in sum.as_ref() {
        use std::fmt::Write;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoadTypeFilter {
    Any,
    ThirdParty,
    FirstParty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NetworkAction {
    Block,
    IgnorePrevious,
}

#[derive(Debug, Clone)]
struct CompiledNetworkRule {
    load_type: LoadTypeFilter,
    if_domain: Vec<String>,
    unless_domain: Vec<String>,
    action: NetworkAction,
}

#[derive(Debug, Clone)]
struct CompiledCosmeticRule {
    if_domain: Vec<String>,
    unless_domain: Vec<String>,
    selector: String,
}

/// Motor compilado en memoria de Shield (inmutable y `Send + Sync`, apto para el hilo IO de CEF).
#[derive(Debug)]
pub struct CompiledShield {
    identifier: String,
    sha256: String,
    rule_count: usize,
    cache_hit: bool,
    cache_path: Option<PathBuf>,
    network_set: RegexSet,
    network_rules: Vec<CompiledNetworkRule>,
    cosmetic_rules: Vec<CompiledCosmeticRule>,
}

impl CompiledShield {
    /// Carga o compila las reglas embebidas usando `$XDG_CACHE_HOME/search/filters`.
    /// Si el archivo con el SHA-256 actual ya existe y es íntegro, se reutiliza sin reescribir.
    pub fn load_or_compile(cache_dir: &Path) -> Result<Self> {
        Self::load_or_compile_rules(embedded_rules(), Some(cache_dir))
    }

    /// Compila en memoria sin tocar el disco (útil para tests rápidos o modo solo memoria).
    pub fn compile_embedded_in_memory() -> Result<Self> {
        Self::load_or_compile_rules(embedded_rules(), None)
    }

    /// Carga o compila una lista arbitraria de `ContentRule` con soporte de caché por SHA-256.
    pub fn load_or_compile_rules(
        rules: Vec<ContentRule>,
        cache_dir: Option<&Path>,
    ) -> Result<Self> {
        let canonical = canonical_rules_json(&rules)?;
        let hash = sha256_hex(&canonical);
        let identifier = format!("{CACHE_PREFIX}{hash}");

        let (effective_rules, cache_hit, cache_path) = if let Some(dir) = cache_dir {
            paths::create_dir_0700(dir)?;
            let target = dir.join(format!("{identifier}.json"));
            let mut hit = false;
            let mut loaded_rules: Option<Vec<ContentRule>> = None;

            if target.is_file() {
                if let Ok(cached_bytes) = fs::read(&target) {
                    if sha256_hex(&cached_bytes) == hash {
                        if let Ok(parsed) =
                            serde_json::from_slice::<Vec<ContentRule>>(&cached_bytes)
                        {
                            hit = true;
                            loaded_rules = Some(parsed);
                        }
                    }
                }
            }

            if !hit {
                paths::write_atomic(&target, &canonical)?;
            }

            prune_stale_cache_files(dir, &identifier);
            (loaded_rules.unwrap_or(rules), hit, Some(target))
        } else {
            (rules, false, None)
        };

        Self::compile_inner(identifier, hash, effective_rules, cache_hit, cache_path)
    }

    fn compile_inner(
        identifier: String,
        sha256: String,
        rules: Vec<ContentRule>,
        cache_hit: bool,
        cache_path: Option<PathBuf>,
    ) -> Result<Self> {
        let rule_count = rules.len();
        let mut patterns = Vec::new();
        let mut network_rules = Vec::new();
        let mut cosmetic_rules = Vec::new();

        for rule in rules {
            match rule.action.action_type.as_str() {
                "block" | "ignore-previous-rules" => {
                    let load_type = parse_load_type(&rule.trigger.load_type);
                    let action = if rule.action.action_type == "block" {
                        NetworkAction::Block
                    } else {
                        NetworkAction::IgnorePrevious
                    };
                    patterns.push(rule.trigger.url_filter);
                    network_rules.push(CompiledNetworkRule {
                        load_type,
                        if_domain: rule.trigger.if_domain,
                        unless_domain: rule.trigger.unless_domain,
                        action,
                    });
                }
                "css-display-none" => {
                    if let Some(sel) = rule.action.selector {
                        let trimmed = sel.trim();
                        if !trimmed.is_empty() {
                            cosmetic_rules.push(CompiledCosmeticRule {
                                if_domain: rule.trigger.if_domain,
                                unless_domain: rule.trigger.unless_domain,
                                selector: trimmed.to_string(),
                            });
                        }
                    }
                }
                _ => {}
            }
        }

        let network_set = RegexSet::new(&patterns)
            .context("Error al compilar el RegexSet de reglas de red de Shield")?;

        Ok(Self {
            identifier,
            sha256,
            rule_count,
            cache_hit,
            cache_path,
            network_set,
            network_rules,
            cosmetic_rules,
        })
    }

    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    pub fn rule_count(&self) -> usize {
        self.rule_count
    }

    pub fn network_rule_count(&self) -> usize {
        self.network_rules.len()
    }

    pub fn cosmetic_rule_count(&self) -> usize {
        self.cosmetic_rules.len()
    }

    pub fn was_cache_hit(&self) -> bool {
        self.cache_hit
    }

    pub fn cache_path(&self) -> Option<&Path> {
        self.cache_path.as_deref()
    }

    /// Evalúa si una petición de red debe cancelarse (`ReturnValue::CANCEL` en CEF).
    pub fn should_block_request(
        &self,
        request_url: &str,
        top_level_url: &str,
        paused_hosts: &[String],
        shield_enabled: bool,
    ) -> bool {
        if !shield_enabled {
            return false;
        }
        if !request_url.starts_with("http://") && !request_url.starts_with("https://") {
            return false;
        }
        if let Some(top_site) = site_host(top_level_url) {
            if is_host_paused(&top_site, paused_hosts) {
                return false;
            }
        }

        let top_raw = raw_url_host(top_level_url);
        let matches = self.network_set.matches(request_url);
        if !matches.matched_any() {
            return false;
        }

        let mut blocked = false;
        let mut same_site_cached: Option<bool> = None;

        for idx in matches.iter() {
            let rule = &self.network_rules[idx];
            match rule.load_type {
                LoadTypeFilter::Any => {}
                LoadTypeFilter::ThirdParty => {
                    let same = *same_site_cached
                        .get_or_insert_with(|| is_same_site_url(request_url, top_level_url));
                    if same {
                        continue;
                    }
                }
                LoadTypeFilter::FirstParty => {
                    let same = *same_site_cached
                        .get_or_insert_with(|| is_same_site_url(request_url, top_level_url));
                    if !same {
                        continue;
                    }
                }
            }

            if !domain_conditions_match(top_raw.as_deref(), &rule.if_domain, &rule.unless_domain) {
                continue;
            }

            match rule.action {
                NetworkAction::Block => blocked = true,
                NetworkAction::IgnorePrevious => blocked = false,
            }
        }

        blocked
    }

    /// Genera la hoja CSS cosmética aplicable a `page_url` (`<style id="office-shield">`).
    pub fn cosmetic_css_for_url(
        &self,
        page_url: &str,
        paused_hosts: &[String],
        shield_enabled: bool,
    ) -> Option<String> {
        if !shield_enabled {
            return None;
        }
        let top_raw = raw_url_host(page_url)?;
        if let Some(site) = site_host(page_url) {
            if is_host_paused(&site, paused_hosts) {
                return None;
            }
        }

        let mut rules_css = Vec::new();
        for rule in &self.cosmetic_rules {
            if domain_conditions_match(Some(&top_raw), &rule.if_domain, &rule.unless_domain) {
                rules_css.push(format!("{} {{ display: none !important; }}", rule.selector));
            }
        }

        if rules_css.is_empty() {
            None
        } else {
            Some(rules_css.join("\n"))
        }
    }
}

fn parse_load_type(values: &[String]) -> LoadTypeFilter {
    let has_third = values.iter().any(|v| v == "third-party");
    let has_first = values.iter().any(|v| v == "first-party");
    match (has_first, has_third) {
        (false, true) => LoadTypeFilter::ThirdParty,
        (true, false) => LoadTypeFilter::FirstParty,
        _ => LoadTypeFilter::Any,
    }
}

fn domain_conditions_match(
    host: Option<&str>,
    if_domain: &[String],
    unless_domain: &[String],
) -> bool {
    if !if_domain.is_empty() {
        let Some(h) = host else {
            return false;
        };
        if !if_domain.iter().any(|pat| matches_domain_pattern(h, pat)) {
            return false;
        }
    }
    if !unless_domain.is_empty() {
        if let Some(h) = host {
            if unless_domain
                .iter()
                .any(|pat| matches_domain_pattern(h, pat))
            {
                return false;
            }
        }
    }
    true
}

/// Elimina archivos `office-shield*.json` obsoletos sin tocar otros archivos en el directorio.
fn prune_stale_cache_files(dir: &Path, keep_identifier: &str) {
    let keep_name = format!("{keep_identifier}.json");
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name == keep_name {
            continue;
        }
        let is_legacy = name == "office-shield" || name == "office-shield.json";
        let is_old_hashed = name.starts_with(CACHE_PREFIX);
        if is_legacy || is_old_hashed {
            let _ = fs::remove_file(&path);
        }
    }
}

/// Comprueba si Shield está pausado para `host` (normalizando `www.` y mayúsculas).
pub fn is_host_paused(host: &str, paused: &[String]) -> bool {
    let Some(norm) = site_host(host) else {
        return false;
    };
    paused
        .iter()
        .filter_map(|p| site_host(p))
        .any(|p| p == norm)
}

/// Activa o desactiva la pausa de Shield para `host` manteniendo la lista ordenada y sin duplicados.
/// Devuelve `true` si la lista cambió.
pub fn set_host_paused(paused: &mut Vec<String>, host: &str, pause_on_host: bool) -> bool {
    let Some(norm) = site_host(host) else {
        return false;
    };
    let before = paused.clone();
    paused.retain(|item| site_host(item).as_deref() != Some(norm.as_str()));
    if pause_on_host {
        paused.push(norm);
    }
    paused.sort();
    paused.dedup();
    *paused != before
}

/// Comprueba si una URL corresponde a un host reproductor de YouTube (`YouTubeAds.hosts`).
pub fn is_youtube_player_url(page_url: &str) -> bool {
    raw_url_host(page_url)
        .map(|h| YOUTUBE_HOSTS.contains(&h.as_str()))
        .unwrap_or(false)
}

/// Construye el script inyectable para aplicar CSS cosmético de Shield en `<style id="office-shield">`.
pub fn build_cosmetic_inject_script(css: &str) -> String {
    let literal = js_string_literal(css);
    format!(
        r#"(function(){{
  var css = {literal};
  function put() {{
    var root = document.head || document.documentElement;
    if (!root) return false;
    var s = document.getElementById('office-shield');
    if (!s) {{
      s = document.createElement('style');
      s.id = 'office-shield';
      root.appendChild(s);
    }}
    s.textContent = css;
    return true;
  }}
  if (!put()) {{
    document.addEventListener('DOMContentLoaded', put, {{ once: true }});
  }}
}})();"#
    )
}

/// Script en el mundo de la página para eliminar la programación de anuncios de YouTube
/// (`YouTubeAds.page(hosts:)`). No usa `eval`, `Function` ni expone globales de Search.
pub fn youtube_page_script(hosts: &[&str]) -> String {
    let hosts_json = serde_json::to_string(hosts).unwrap_or_else(|_| "[]".to_string());
    format!(
        r#"(function () {{
  'use strict';
  if ({hosts_json}.indexOf(location.hostname) < 0) return;
  if (window.__searchYtPruneInstalled) return;
  var apply = Reflect.apply, owns = Object.prototype.hasOwnProperty,
      define = Object.defineProperty, isArray = Array.isArray;
  var ads = ['adPlacements', 'adSlots', 'playerAds', 'adBreakHeartbeatParams'];
  var marks = ['responseContext', 'playabilityStatus', 'streamingData', 'videoDetails'];
  function plain(o) {{ return o !== null && typeof o === 'object' && !isArray(o); }}
  function youtubes(o) {{
    for (var i = 0; i < marks.length; i++) if (apply(owns, o, [marks[i]])) return true;
    return false;
  }}
  function strip(o) {{
    if (!plain(o) || !youtubes(o)) return;
    for (var i = 0; i < ads.length; i++) if (apply(owns, o, [ads[i]])) delete o[ads[i]];
  }}
  function one(o) {{
    if (!plain(o)) return;
    strip(o);
    if (apply(owns, o, ['playerResponse'])) strip(o.playerResponse);
  }}
  function prune(value) {{
    try {{
      if (isArray(value)) {{
        for (var i = 0; i < value.length && i < 8; i++) one(value[i]);
      }} else {{
        one(value);
      }}
    }} catch (e) {{}}
    return value;
  }}
  try {{
    var parse = JSON.parse;
    if (typeof parse === 'function') {{
      define(JSON, 'parse', {{
        value: ({{ parse(text, reviver) {{ return prune(apply(parse, JSON, arguments)); }} }}).parse,
        writable: true, enumerable: false, configurable: true
      }});
    }}
  }} catch (e) {{}}
  try {{
    var json = typeof Response === 'function' && Response.prototype.json;
    var then = Promise.prototype.then;
    if (typeof json === 'function') {{
      define(Response.prototype, 'json', {{
        value: ({{ json() {{ return apply(then, apply(json, this, []), [prune]); }} }}).json,
        writable: true, enumerable: true, configurable: true
      }});
    }}
  }} catch (e) {{}}
  ['ytInitialPlayerResponse', 'ytInitialData'].forEach(function (name) {{
    try {{
      if (apply(owns, window, [name])) {{
        prune(window[name]);
        return;
      }}
      var held;
      define(window, name, {{
        get: function () {{ return held; }},
        set: function (value) {{ held = prune(value); }},
        enumerable: true, configurable: true
      }});
    }} catch (e) {{}}
  }});
  define(window, '__searchYtPruneInstalled', {{ value: true, enumerable: false }});
}})();"#
    )
}

/// Script de respaldo para YouTube (`YouTubeAds.skipper(hosts:)`): si se reproduce un anuncio
/// (`ad-showing`), lo silencia y acelera a 16× restaurando volumen y velocidad al terminar.
pub fn youtube_skipper_script(hosts: &[&str]) -> String {
    let hosts_json = serde_json::to_string(hosts).unwrap_or_else(|_| "[]".to_string());
    format!(
        r#"(function () {{
  'use strict';
  if ({hosts_json}.indexOf(location.hostname) < 0) return;
  if (window.__searchYtSkipperInstalled) return;
  Object.defineProperty(window, '__searchYtSkipperInstalled', {{ value: true, enumerable: false }});
  var held = new WeakMap();
  function look(event) {{
    var video = event.target;
    if (!(video instanceof HTMLVideoElement)) return;
    var player = video.closest('.html5-video-player');
    var ad = !!player && player.classList.contains('ad-showing');
    var was = held.get(video);
    try {{
      if (ad) {{
        if (!was) {{
          was = {{ muted: video.muted, rate: video.playbackRate }};
          held.set(video, was);
          video.muted = true;
        }}
        if (video.playbackRate !== 16) video.playbackRate = 16;
      }} else if (was) {{
        held.delete(video);
        video.playbackRate = was.rate;
        video.muted = was.muted;
      }}
    }} catch (e) {{}}
  }}
  ['playing', 'timeupdate', 'durationchange', 'emptied'].forEach(function (type) {{
    document.addEventListener(type, look, true);
  }});
}})();"#
    )
}

/// Escapa cualquier texto como literal de cadena JS seguro (evita `<`, U+2028 y U+2029).
pub fn js_string_literal(text: &str) -> String {
    serde_json::to_string(text)
        .unwrap_or_else(|_| "\"\"".to_string())
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
        .replace('<', "\\u003c")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_SEQ: AtomicU64 = AtomicU64::new(1);

    fn temp_filters_dir(label: &str) -> PathBuf {
        let n = TEST_SEQ.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "search-shield-test-{label}-{}-{n}",
            std::process::id()
        ))
    }

    #[test]
    fn embedded_rules_include_all_shield_and_youtube_rules() {
        let rules = embedded_rules();
        // 44 unwanted + 1 global cosmetic + 9 YouTube network + 4 site cosmetic = 58
        assert_eq!(UNWANTED_DOMAINS.len(), 44);
        assert_eq!(COSMETIC_SLOTS.len(), 23);
        assert_eq!(SITE_COSMETIC_SLOTS.len(), 4);
        assert_eq!(youtube_ad_rules().len(), 9);
        assert_eq!(rules.len(), 58);
    }

    #[test]
    fn network_filter_matches_only_listed_host_and_subdomains() {
        let re = regex::Regex::new(&network_filter_for_domain("segment.com"))
            .expect("regex de dominio válida");
        for url in [
            "https://segment.com/track",
            "http://segment.com/track",
            "https://cdn.segment.com/track",
            "https://a.b.segment.com/track",
            "https://segment.com:8443/track",
            "https://segment.com/",
            "https://segment.com/?event=test",
            "https://segment.com/#event",
        ] {
            assert!(re.is_match(url), "debía coincidir: {url}");
        }
        for url in [
            "https://segment.company/app.js",
            "https://cdn.segment.company/app.js",
            "https://segment.com.example/app.js",
            "https://segment.com-example/app.js",
            "https://notsegment.com/app.js",
            "https://example.com/segment.com/track",
            "https://segmentXcom/app.js",
            "ftp://segment.com/track",
        ] {
            assert!(!re.is_match(url), "NO debía coincidir: {url}");
        }
    }

    #[test]
    fn blocks_third_party_trackers_but_allows_first_party_and_paused_sites() {
        let shield = CompiledShield::compile_embedded_in_memory().expect("compile shield");
        let no_paused: Vec<String> = Vec::new();

        // Terceros bloqueados.
        assert!(shield.should_block_request(
            "https://pagead2.googlesyndication.com/pagead/js/adsbygoogle.js",
            "https://www.nytimes.com/article",
            &no_paused,
            true
        ));
        assert!(shield.should_block_request(
            "https://securepubads.g.doubleclick.net/tag/js/gpt.js",
            "https://www.theguardian.com/uk",
            &no_paused,
            true
        ));

        // First-party permitido (los propios scripts de segment.com al visitar segment.com).
        assert!(!shield.should_block_request(
            "https://cdn.segment.com/app.js",
            "https://segment.com/docs",
            &no_paused,
            true
        ));

        // Sitio pausado permitido.
        let mut paused = Vec::new();
        assert!(set_host_paused(&mut paused, "www.nytimes.com", true));
        assert_eq!(paused, vec!["nytimes.com".to_string()]);
        assert!(!shield.should_block_request(
            "https://pagead2.googlesyndication.com/pagead/js/adsbygoogle.js",
            "https://www.nytimes.com/article",
            &paused,
            true
        ));

        // Shield globalmente desactivado.
        assert!(!shield.should_block_request(
            "https://pagead2.googlesyndication.com/pagead/js/adsbygoogle.js",
            "https://www.theguardian.com/uk",
            &no_paused,
            false
        ));
    }

    #[test]
    fn youtube_rules_block_ad_paths_and_never_the_video() {
        let shield = CompiledShield::compile_embedded_in_memory().expect("compile shield");
        let yt = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";
        let other = "https://example.com/video";
        let empty: Vec<String> = Vec::new();

        for ad_url in [
            "https://www.youtube.com/api/stats/ads?ver=2",
            "https://www.youtube.com/pagead/adview?x",
            "https://www.youtube.com/ptracking?pltype=adhost",
            "https://www.youtube.com/get_midroll_info?ei=1",
            "https://www.youtube.com/youtubei/v1/player/ad_break?key=1",
            "https://www.youtube.com/pcs/activeview?x",
            "https://s.youtube.com/api/stats/qoe?event=streamingstats&adformat=15_2",
            "https://www.google.com/pagead/lvz?x",
            "https://imasdk.googleapis.com/js/sdkloader/ima3.js",
        ] {
            assert!(
                shield.should_block_request(ad_url, yt, &empty, true),
                "YouTube ad path debía bloquearse: {ad_url}"
            );
        }

        // IMA SDK fuera de YouTube no se bloquea para no romper reproductores de terceros.
        assert!(!shield.should_block_request(
            "https://imasdk.googleapis.com/js/sdkloader/ima3.js",
            other,
            &empty,
            true
        ));

        for video_url in [
            "https://rr1---sn-abc.googlevideo.com/videoplayback?expire=1&itag=18",
            "https://www.youtube.com/youtubei/v1/player?key=1",
            "https://www.youtube.com/youtubei/v1/next?key=1",
            "https://www.youtube.com/api/stats/watchtime?x",
            "https://www.youtube.com/api/stats/playback?x",
            "https://s.youtube.com/api/stats/qoe?event=streamingstats&fmt=18",
            "https://www.youtube.com/watch?v=pagead",
            "https://i.ytimg.com/vi/x/hqdefault.jpg",
            "https://example.com/api/stats/ads",
            "https://notyoutube.com/pagead/x",
        ] {
            assert!(
                !shield.should_block_request(video_url, yt, &empty, true),
                "Vídeo/recurso legítimo NO debía bloquearse: {video_url}"
            );
        }
    }

    #[test]
    fn cosmetic_css_applies_global_and_site_specific_rules_respecting_pause() {
        let shield = CompiledShield::compile_embedded_in_memory().expect("compile shield");
        let empty: Vec<String> = Vec::new();

        let elpais_css = shield
            .cosmetic_css_for_url("https://elpais.com/tecnologia/", &empty, true)
            .expect("CSS para El País");
        assert!(elpais_css.contains(".adsbygoogle"));
        assert!(elpais_css.contains(".ad { display: none !important; }"));
        assert!(!elpais_css.contains(".ad-slot-header__wrapper"));

        let cnn_css = shield
            .cosmetic_css_for_url("https://edition.cnn.com/world", &empty, true)
            .expect("CSS para CNN");
        assert!(cnn_css.contains(".ad-slot-header__wrapper { display: none !important; }"));
        assert!(!cnn_css.contains(".top-fronts-banner-ad-container"));

        let paused = vec!["elpais.com".to_string()];
        assert!(shield
            .cosmetic_css_for_url("https://www.elpais.com/", &paused, true)
            .is_none());
    }

    #[test]
    fn sha256_cache_hits_on_repeat_recompiles_on_change_and_prunes_stale_only() {
        let dir = temp_filters_dir("cache");
        paths::create_dir_0700(&dir).expect("create temp cache dir");

        // Crear archivos antiguos de Shield y un archivo ajeno que debe respetarse.
        paths::write_atomic(&dir.join("office-shield-old.json"), b"[]").expect("old");
        paths::write_atomic(&dir.join("office-shield.json"), b"[]").expect("legacy");
        paths::write_atomic(&dir.join("unrelated.json"), b"{\"keep\":true}").expect("unrelated");

        let first = CompiledShield::load_or_compile(&dir).expect("first compile");
        assert!(!first.was_cache_hit());
        assert!(first.identifier().starts_with(CACHE_PREFIX));
        assert_eq!(first.sha256().len(), 64);
        assert!(first.cache_path().expect("cache path").exists());
        assert!(!dir.join("office-shield-old.json").exists());
        assert!(!dir.join("office-shield.json").exists());
        assert!(dir.join("unrelated.json").exists());

        // Segunda carga con el mismo hash: acierto de caché.
        let second = CompiledShield::load_or_compile(&dir).expect("second load");
        assert!(second.was_cache_hit());
        assert_eq!(first.identifier(), second.identifier());

        // Si las reglas cambian, el hash cambia y se limpia el archivo anterior.
        let mut modified = embedded_rules();
        modified.push(ContentRule {
            trigger: RuleTrigger {
                url_filter: network_filter_for_domain("tracker.example.test"),
                load_type: vec!["third-party".to_string()],
                if_domain: Vec::new(),
                unless_domain: Vec::new(),
            },
            action: RuleAction {
                action_type: "block".to_string(),
                selector: None,
            },
        });
        let third =
            CompiledShield::load_or_compile_rules(modified, Some(&dir)).expect("third compile");
        assert!(!third.was_cache_hit());
        assert_ne!(third.identifier(), first.identifier());
        assert!(!first.cache_path().expect("old path").exists());
        assert!(third.cache_path().expect("new path").exists());
        assert!(dir.join("unrelated.json").exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn youtube_scripts_do_not_use_eval_or_webkit_globals() {
        let page_js = youtube_page_script(YOUTUBE_HOSTS);
        let skip_js = youtube_skipper_script(YOUTUBE_HOSTS);
        assert!(!page_js.contains("eval("));
        assert!(!page_js.contains("Function("));
        assert!(!page_js.contains("webkit"));
        assert!(page_js.contains("ytInitialPlayerResponse"));
        assert!(skip_js.contains("ad-showing"));
        assert!(skip_js.contains("playbackRate = 16"));
    }
}
