//! Fuente única de verdad para todos los switches de línea de comandos, `base::Feature`
//! (`--enable-features` / `--disable-features`) y preferencias internas del `RequestContext`
//! verificados contra Chromium `156.0.8078.96` (`PORTING.md` §2.3).
//!
//! Principios innegociables aplicados aquí:
//! - Cero telemetría, cero cuentas, cero sincronización y cero servicios de Google
//!   (incluido Safe Browsing).
//! - Gestor de contraseñas y autocompletado propios de Chromium completamente apagados
//!   (las credenciales las gestiona `Vault` mediante Secret Service).
//! - **Site Isolation (`site-per-process` / `IsolateOrigins`) NUNCA se desactiva**, ni siquiera
//!   mediante `DOT_CHROMIUM_FLAGS` (o el respaldo obsoleto `SEARCH_CHROMIUM_FLAGS`).

use indexmap::{IndexMap, IndexSet};

/// Switches prohibidos por seguridad: intentarlos añadir por `DOT_CHROMIUM_FLAGS` se ignora.
const FORBIDDEN_SWITCHES: &[&str] = &[
    "disable-site-isolation-trials",
    "disable-site-isolation-for-policy",
    "single-process",
];

/// Features críticas de aislamiento de sitios que jamás pueden incluirse en `--disable-features`.
const FORBIDDEN_DISABLED_FEATURES: &[&str] =
    &["SitePerProcess", "site-per-process", "IsolateOrigins"];

/// Descripción documentada de un switch base de privacidad/minimalismo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwitchSpec {
    pub name: &'static str,
    pub value: Option<&'static str>,
    pub reason: &'static str,
}

/// Lista base de switches obligatorios de privacidad, cero-Google y minimalismo verificados en `libcef.so`.
pub const BASE_PRIVACY_SWITCHES: &[SwitchSpec] = &[
    SwitchSpec {
        name: "disable-background-networking",
        value: None,
        reason: "Corta peticiones automáticas en segundo plano (variaciones, pruebas de campo y sondas de red).",
    },
    SwitchSpec {
        name: "disable-component-update",
        value: None,
        reason: "Desactiva descargas automáticas del actualizador de componentes de Google.",
    },
    SwitchSpec {
        name: "disable-sync",
        value: None,
        reason: "Desactiva por completo el subsistema de sincronización de cuentas de Chrome.",
    },
    SwitchSpec {
        name: "disable-default-apps",
        value: None,
        reason: "Impide instalar o registrar aplicaciones web predeterminadas de Google.",
    },
    SwitchSpec {
        name: "disable-domain-reliability",
        value: None,
        reason: "Impide enviar informes de fallos de red/DNS a los colectores de Google.",
    },
    SwitchSpec {
        name: "disable-breakpad",
        value: None,
        reason: "Desactiva la recopilación y subida de informes de caídas (crashpad/breakpad).",
    },
    SwitchSpec {
        name: "no-pings",
        value: None,
        reason: "Bloquea el envío de peticiones de auditoría en hipervínculos (<a ping>).",
    },
    SwitchSpec {
        name: "no-first-run",
        value: None,
        reason: "Suprime tareas e interfaces de primera ejecución de Chromium.",
    },
    SwitchSpec {
        name: "no-default-browser-check",
        value: None,
        reason: "Evita que Chromium compruebe por su cuenta si es el navegador predeterminado.",
    },
    SwitchSpec {
        name: "use-mock-keychain",
        value: None,
        reason: "Impide que el almacén interno de Chromium invoque el llavero del sistema (Vault gestiona las claves).",
    },
    SwitchSpec {
        name: "password-store",
        value: Some("basic"),
        reason: "Evita que Chromium conecte por su cuenta con GNOME Keyring o KWallet.",
    },
    SwitchSpec {
        name: "no-startup-window",
        value: None,
        reason: "Impide que la capa de arranque cree ventanas propias de Chrome.",
    },
    SwitchSpec {
        name: "noerrdialogs",
        value: None,
        reason: "Suprime cuadros de diálogo modales nativos de error de Chromium.",
    },
    SwitchSpec {
        name: "hide-crash-restore-bubble",
        value: None,
        reason: "Oculta burbujas propias de restauración de sesión de Chrome.",
    },
    SwitchSpec {
        name: "disable-session-crashed-bubble",
        value: None,
        reason: "Evita mostrar avisos de cierre inesperado de la interfaz Chrome.",
    },
];

/// Features `base::Feature` desactivadas por defecto (todas verificadas en `libcef.so` 156.0.8078.96).
pub const BASE_DISABLED_FEATURES: &[(&str, &str)] = &[
    (
        "Translate",
        "Desactiva el servicio de traducción en la nube de Google.",
    ),
    (
        "OptimizationHints",
        "Desactiva consultas de sugerencias de optimización a servidores de Google.",
    ),
    (
        "OptimizationTargetPrediction",
        "Desactiva descarga y evaluación de modelos remotos de predicción de Google.",
    ),
    (
        "MediaRouter",
        "Desactiva Google Cast / enrutamiento multimedia en red local no solicitado.",
    ),
    (
        "DialMediaRouteProvider",
        "Impide el descubrimiento periódico SSDP/DIAL en la red local.",
    ),
    (
        "AutofillServerCommunication",
        "Corta toda comunicación con los servidores de autocompletado de Google.",
    ),
    (
        "SafeBrowsing",
        "Desactiva Google Safe Browsing conforme al Principio #1 de AGENTS.md.",
    ),
    (
        "PasswordManager",
        "Desactiva el gestor de contraseñas interno de Chromium (delegado en Vault).",
    ),
];

/// Features VA-API verificadas en Chromium 156 Linux para decodificación de vídeo por hardware.
pub const VAAPI_LINUX_FEATURES: &[(&str, &str)] = &[
    (
        "AcceleratedVideoDecodeLinuxGL",
        "Habilita la ruta de decodificación de vídeo acelerada sobre GL en Linux.",
    ),
    (
        "AcceleratedVideoDecodeLinuxZeroCopyGL",
        "Permite transferencia zero-copy de fotogramas decodificados por VA-API.",
    ),
    (
        "VaapiIgnoreDriverChecks",
        "Permite usar VA-API en GPU AMD Radeon Vega / Mesa sin bloqueos de lista blanca.",
    ),
];

/// Valor tipado para preferencias de `cef::RequestContext::set_preference`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrefValue {
    Bool(bool),
    Int(i32),
    String(String),
}

/// Preferencia documentada para aplicar sobre cada `cef::RequestContext`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextPreference {
    pub name: &'static str,
    pub value: PrefValue,
    pub reason: &'static str,
}

/// Configuración de alto nivel derivada de los ajustes del usuario y del entorno de ejecución.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChromiumConfig {
    /// Límite de procesos renderer (`Some(6)` por defecto; `None` o `Some(0)` no fija tope).
    pub renderer_process_limit: Option<u32>,
    /// Agrupa pestañas del mismo sitio registrable en un proceso (`--process-per-site`).
    pub process_per_site: bool,
    /// Modo ligero de V8 sin optimizaciones pesadas JIT (`--js-flags=--lite-mode`).
    pub lite_mode: bool,
    /// Decodificación de vídeo por hardware VA-API en Linux.
    pub hardware_video_decode: bool,
    /// Estado opcional de ahorro de memoria `performance_tuning.high_efficiency_mode.state`.
    pub high_efficiency_mode_state: Option<i32>,
    /// Exigir gesto del usuario para reproducir contenido multimedia con audio.
    pub autoplay_user_gesture_required: bool,
    /// Modo software headless (p. ej. Xvfb con `LIBGL_ALWAYS_SOFTWARE=1` o `DOT_HEADLESS_SW=1`).
    pub headless_software_gl: bool,
    /// Desactivar el sandbox de namespace de subprocesos cuando ya corremos dentro de Flatpak/`bwrap`.
    pub no_sandbox: bool,
    /// Backend Ozone explícito (`"wayland"`, `"x11"` o `"headless"`) sincronizado con el servidor gráfico activo.
    pub ozone_platform: Option<String>,
}

impl Default for ChromiumConfig {
    fn default() -> Self {
        Self {
            renderer_process_limit: Some(6),
            process_per_site: false,
            lite_mode: false,
            hardware_video_decode: true,
            high_efficiency_mode_state: None,
            autoplay_user_gesture_required: false,
            headless_software_gl: false,
            no_sandbox: false,
            ozone_platform: None,
        }
    }
}

/// Conjunto resuelto de switches, `--enable-features`, `--disable-features` y preferencias.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedChromiumFlags {
    switches: IndexMap<String, Option<String>>,
    enabled_features: IndexSet<String>,
    disabled_features: IndexSet<String>,
    preferences: Vec<ContextPreference>,
}

impl ResolvedChromiumFlags {
    /// Construye la configuración resuelta leyendo `DOT_CHROMIUM_FLAGS` (respaldo obsoleto
    /// `SEARCH_CHROMIUM_FLAGS`) del entorno si existe.
    pub fn from_env(config: &ChromiumConfig) -> Self {
        let env_raw =
            crate::env::var("DOT_CHROMIUM_FLAGS", "SEARCH_CHROMIUM_FLAGS").unwrap_or_default();
        Self::build(config, &env_raw)
    }

    /// Construye la configuración resuelta aplicando `env_overrides` (sintaxis de `DOT_CHROMIUM_FLAGS`).
    ///
    /// Sintaxis admitida en `DOT_CHROMIUM_FLAGS` (separada por espacios):
    /// - `--foo` o `foo` -> añade switch sin valor
    /// - `--foo=bar` o `foo=bar` -> añade o reemplaza switch con valor
    /// - `!--foo`, `!foo`, `---foo`, `--no-switch:foo` o `-switch:foo` -> elimina switch
    /// - `+feature:Nombre` -> mueve `Nombre` a `--enable-features`
    /// - `-feature:Nombre` -> mueve `Nombre` a `--disable-features` (salvo Site Isolation)
    pub fn build(config: &ChromiumConfig, env_overrides: &str) -> Self {
        let mut switches: IndexMap<String, Option<String>> = IndexMap::new();
        let mut enabled_features: IndexSet<String> = IndexSet::new();
        let mut disabled_features: IndexSet<String> = IndexSet::new();

        for spec in BASE_PRIVACY_SWITCHES {
            switches.insert(spec.name.to_string(), spec.value.map(str::to_string));
        }

        for &(feat, _reason) in BASE_DISABLED_FEATURES {
            disabled_features.insert(feat.to_string());
        }

        if let Some(limit) = config.renderer_process_limit.filter(|&n| n > 0) {
            switches.insert(
                "renderer-process-limit".to_string(),
                Some(limit.to_string()),
            );
        }

        if config.process_per_site {
            switches.insert("process-per-site".to_string(), None);
        }

        if config.lite_mode {
            switches.insert("js-flags".to_string(), Some("--lite-mode".to_string()));
        }

        if config.autoplay_user_gesture_required {
            switches.insert(
                "autoplay-policy".to_string(),
                Some("user-gesture-required".to_string()),
            );
        }

        if config.no_sandbox {
            switches.insert("no-sandbox".to_string(), None);
        }

        if let Some(ref platform) = config.ozone_platform {
            let trimmed = platform.trim();
            if !trimmed.is_empty() {
                switches.insert("ozone-platform".to_string(), Some(trimmed.to_string()));
            }
        }

        if config.headless_software_gl {
            switches.insert("disable-gpu".to_string(), None);
            switches.insert("disable-gpu-compositing".to_string(), None);
        } else if config.hardware_video_decode {
            for &(feat, _reason) in VAAPI_LINUX_FEATURES {
                disabled_features.swap_remove(feat);
                enabled_features.insert(feat.to_string());
            }
        }

        apply_runtime_overrides(
            env_overrides,
            &mut switches,
            &mut enabled_features,
            &mut disabled_features,
        );

        let mut preferences = vec![
            ContextPreference {
                name: "safebrowsing.enabled",
                value: PrefValue::Bool(false),
                reason: "Desactiva Google Safe Browsing en el perfil de navegación.",
            },
            ContextPreference {
                name: "credentials_enable_service",
                value: PrefValue::Bool(false),
                reason: "Desactiva el servicio interno de guardado de contraseñas de Chromium.",
            },
            ContextPreference {
                name: "autofill.profile_enabled",
                value: PrefValue::Bool(false),
                reason: "Desactiva el autocompletado de direcciones/perfiles de Chromium.",
            },
            ContextPreference {
                name: "autofill.credit_card_enabled",
                value: PrefValue::Bool(false),
                reason: "Desactiva el autocompletado de tarjetas bancarias de Chromium.",
            },
            ContextPreference {
                name: "translate.enabled",
                value: PrefValue::Bool(false),
                reason: "Desactiva las sugerencias y peticiones de traducción a Google.",
            },
        ];

        if let Some(state) = config.high_efficiency_mode_state {
            preferences.push(ContextPreference {
                name: "performance_tuning.high_efficiency_mode.state",
                value: PrefValue::Int(state),
                reason: "Controla el modo de ahorro de memoria de pestañas en segundo plano.",
            });
        }

        Self {
            switches,
            enabled_features,
            disabled_features,
            preferences,
        }
    }

    pub fn switches(&self) -> &IndexMap<String, Option<String>> {
        &self.switches
    }

    pub fn enabled_features(&self) -> &IndexSet<String> {
        &self.enabled_features
    }

    pub fn disabled_features(&self) -> &IndexSet<String> {
        &self.disabled_features
    }

    pub fn preferences(&self) -> &[ContextPreference] {
        &self.preferences
    }

    /// Devuelve todos los pares `(switch, valor_opcional)` listos para aplicar a `cef::CommandLine`,
    /// incluyendo `enable-features` y `disable-features` consolidados.
    pub fn to_command_line_pairs(&self) -> Vec<(String, Option<String>)> {
        let mut out: Vec<(String, Option<String>)> = self
            .switches
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        if !self.enabled_features.is_empty() {
            let joined = self
                .enabled_features
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(",");
            out.push(("enable-features".to_string(), Some(joined)));
        }

        if !self.disabled_features.is_empty() {
            let joined = self
                .disabled_features
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(",");
            out.push(("disable-features".to_string(), Some(joined)));
        }

        out
    }

    /// Verifica que Site Isolation permanezca intacto.
    pub fn is_site_isolation_intact(&self) -> bool {
        let bad_switch = FORBIDDEN_SWITCHES
            .iter()
            .any(|s| self.switches.contains_key(*s));
        let bad_feature = FORBIDDEN_DISABLED_FEATURES.iter().any(|f| {
            self.disabled_features
                .iter()
                .any(|df| df.eq_ignore_ascii_case(f))
        });
        !bad_switch && !bad_feature
    }
}

fn apply_runtime_overrides(
    raw: &str,
    switches: &mut IndexMap<String, Option<String>>,
    enabled_features: &mut IndexSet<String>,
    disabled_features: &mut IndexSet<String>,
) {
    for token in raw.split_whitespace() {
        if token.is_empty() {
            continue;
        }

        if let Some(feat_list) = token.strip_prefix("+feature:") {
            for feat in feat_list
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                disabled_features.swap_remove(feat);
                enabled_features.insert(feat.to_string());
            }
            continue;
        }

        if let Some(feat_list) = token.strip_prefix("-feature:") {
            for feat in feat_list
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                if is_forbidden_disabled_feature(feat) {
                    crate::warn!(
                        "Ignorando intento de desactivar feature de Site Isolation: {}",
                        feat
                    );
                    continue;
                }
                enabled_features.swap_remove(feat);
                disabled_features.insert(feat.to_string());
            }
            continue;
        }

        if let Some(removed) = parse_switch_removal(token) {
            let clean = removed.trim_start_matches('-');
            if !clean.is_empty() {
                switches.shift_remove(clean);
            }
            continue;
        }

        let stripped = token.trim_start_matches('-');
        if stripped.is_empty() {
            continue;
        }

        let (name, value) = match stripped.split_once('=') {
            Some((k, v)) => (k.trim(), Some(v.to_string())),
            None => (stripped.trim(), None),
        };

        if name.is_empty() {
            continue;
        }

        if FORBIDDEN_SWITCHES
            .iter()
            .any(|bad| name.eq_ignore_ascii_case(bad))
        {
            crate::warn!(
                "Ignorando switch prohibido en DOT_CHROMIUM_FLAGS: --{}",
                name
            );
            continue;
        }

        if name == "enable-features" {
            if let Some(val) = value {
                for feat in val.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    disabled_features.swap_remove(feat);
                    enabled_features.insert(feat.to_string());
                }
            }
            continue;
        }

        if name == "disable-features" {
            if let Some(val) = value {
                for feat in val.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    if is_forbidden_disabled_feature(feat) {
                        crate::warn!(
                            "Ignorando intento de desactivar feature de Site Isolation: {}",
                            feat
                        );
                        continue;
                    }
                    enabled_features.swap_remove(feat);
                    disabled_features.insert(feat.to_string());
                }
            }
            continue;
        }

        switches.insert(name.to_string(), value);
    }
}

fn parse_switch_removal(token: &str) -> Option<&str> {
    token
        .strip_prefix("!--")
        .or_else(|| token.strip_prefix('!'))
        .or_else(|| token.strip_prefix("---"))
        .or_else(|| token.strip_prefix("--no-switch:"))
        .or_else(|| token.strip_prefix("-switch:"))
}

fn is_forbidden_disabled_feature(feat: &str) -> bool {
    FORBIDDEN_DISABLED_FEATURES
        .iter()
        .any(|bad| feat.eq_ignore_ascii_case(bad))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_flags_include_zero_google_privacy_and_vaapi() {
        let cfg = ChromiumConfig::default();
        let resolved = ResolvedChromiumFlags::build(&cfg, "");

        assert!(resolved
            .switches()
            .contains_key("disable-background-networking"));
        assert!(resolved.switches().contains_key("disable-component-update"));
        assert!(resolved.switches().contains_key("disable-sync"));
        assert!(resolved.switches().contains_key("disable-default-apps"));
        assert!(resolved
            .switches()
            .contains_key("disable-domain-reliability"));
        assert!(resolved.switches().contains_key("disable-breakpad"));
        assert!(resolved.switches().contains_key("no-pings"));
        assert!(resolved.switches().contains_key("use-mock-keychain"));
        assert_eq!(
            resolved.switches().get("password-store"),
            Some(&Some("basic".to_string()))
        );
        assert_eq!(
            resolved.switches().get("renderer-process-limit"),
            Some(&Some("6".to_string()))
        );

        for expected in [
            "Translate",
            "OptimizationHints",
            "OptimizationTargetPrediction",
            "MediaRouter",
            "DialMediaRouteProvider",
            "AutofillServerCommunication",
            "SafeBrowsing",
            "PasswordManager",
        ] {
            assert!(
                resolved.disabled_features().contains(expected),
                "falta feature desactivada {expected}"
            );
        }

        assert!(resolved
            .enabled_features()
            .contains("AcceleratedVideoDecodeLinuxGL"));
        assert!(resolved
            .enabled_features()
            .contains("AcceleratedVideoDecodeLinuxZeroCopyGL"));
        assert!(resolved
            .enabled_features()
            .contains("VaapiIgnoreDriverChecks"));
        assert!(resolved.is_site_isolation_intact());
    }

    #[test]
    fn request_context_preferences_disable_google_and_password_services() {
        let cfg = ChromiumConfig {
            high_efficiency_mode_state: Some(2),
            ..ChromiumConfig::default()
        };
        let resolved = ResolvedChromiumFlags::build(&cfg, "");

        let prefs = resolved.preferences();
        assert!(prefs
            .iter()
            .any(|p| p.name == "safebrowsing.enabled" && p.value == PrefValue::Bool(false)));
        assert!(prefs
            .iter()
            .any(|p| p.name == "credentials_enable_service" && p.value == PrefValue::Bool(false)));
        assert!(prefs
            .iter()
            .any(|p| p.name == "autofill.profile_enabled" && p.value == PrefValue::Bool(false)));
        assert!(
            prefs
                .iter()
                .any(|p| p.name == "autofill.credit_card_enabled"
                    && p.value == PrefValue::Bool(false))
        );
        assert!(prefs
            .iter()
            .any(|p| p.name == "translate.enabled" && p.value == PrefValue::Bool(false)));
        assert!(prefs.iter().any(|p| {
            p.name == "performance_tuning.high_efficiency_mode.state"
                && p.value == PrefValue::Int(2)
        }));
    }

    #[test]
    fn runtime_overrides_add_remove_switches_and_toggle_features() {
        let cfg = ChromiumConfig::default();
        let overrides = "!--renderer-process-limit --renderer-process-limit=4 --process-per-site --no-switch:no-pings +feature:BackForwardCache -feature:VaapiIgnoreDriverChecks";
        let resolved = ResolvedChromiumFlags::build(&cfg, overrides);

        assert_eq!(
            resolved.switches().get("renderer-process-limit"),
            Some(&Some("4".to_string()))
        );
        assert!(resolved.switches().contains_key("process-per-site"));
        assert!(!resolved.switches().contains_key("no-pings"));
        assert!(resolved.enabled_features().contains("BackForwardCache"));
        assert!(!resolved
            .enabled_features()
            .contains("VaapiIgnoreDriverChecks"));
        assert!(resolved
            .disabled_features()
            .contains("VaapiIgnoreDriverChecks"));
    }

    #[test]
    fn site_isolation_cannot_be_disabled_even_via_overrides() {
        let cfg = ChromiumConfig::default();
        let malicious = "--disable-site-isolation-trials --single-process -feature:SitePerProcess -feature:site-per-process --disable-features=IsolateOrigins";
        let resolved = ResolvedChromiumFlags::build(&cfg, malicious);

        assert!(resolved.is_site_isolation_intact());
        assert!(!resolved
            .switches()
            .contains_key("disable-site-isolation-trials"));
        assert!(!resolved.switches().contains_key("single-process"));
    }

    #[test]
    fn headless_software_mode_disables_gpu_and_skips_vaapi() {
        let cfg = ChromiumConfig {
            headless_software_gl: true,
            hardware_video_decode: true,
            ..ChromiumConfig::default()
        };
        let resolved = ResolvedChromiumFlags::build(&cfg, "");
        assert!(resolved.switches().contains_key("disable-gpu"));
        assert!(resolved.switches().contains_key("disable-gpu-compositing"));
        assert!(!resolved
            .enabled_features()
            .contains("AcceleratedVideoDecodeLinuxGL"));
    }
}
