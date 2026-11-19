//! Integración en tiempo real de `Shield` en CEF (`Shield.swift` + `YouTubeAds.swift` + `SiteCard.swift`).
//!
//! - Compila/carga la caché SHA-256 (`$XDG_CACHE_HOME/search/filters`) en un hilo en segundo plano
//!   para no bloquear el arranque de la aplicación.
//! - Estado protegido con `Arc<RwLock<...>>` (`Send + Sync`) para evaluarse con seguridad desde
//!   el hilo IO de CEF en `CefResourceRequestHandler::on_before_resource_load` sin tocar `RefCell`.
//! - Se aplica por igual a ventanas normales y ventanas privadas.

use crate::settings::SettingsStore;
use gnome_platform::gtk::{
    Align, AsWidget, BoxWidget, Button, EllipsizeMode, Label, Orientation, Popover, PositionType,
    Separator,
};
use search_core::domain::site_host;
use search_core::shield::{
    build_cosmetic_inject_script, is_host_paused, is_youtube_player_url, set_host_paused,
    youtube_page_script, youtube_skipper_script, CompiledShield, YOUTUBE_HOSTS,
};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

struct ShieldState {
    compiled: Option<Arc<CompiledShield>>,
    trouble: Option<String>,
    enabled: bool,
    paused_hosts: Vec<String>,
}

/// Gestor compartido y thread-safe de Shield para todas las pestañas (normales y privadas).
#[derive(Clone)]
pub struct ShieldManager {
    state: Arc<RwLock<ShieldState>>,
}

impl ShieldManager {
    /// Inicializa Shield con el estado actual de `SettingsStore` y compila la lista de reglas
    /// en segundo plano usando `$XDG_CACHE_HOME/search/filters`.
    pub fn new_async(cache_dir: PathBuf, settings: &SettingsStore) -> Self {
        let snap = settings.snapshot();
        let state = Arc::new(RwLock::new(ShieldState {
            compiled: None,
            trouble: None,
            enabled: snap.shield_enabled,
            paused_hosts: snap.shield_paused,
        }));

        let mgr = Self {
            state: state.clone(),
        };

        // Mantener el estado thread-safe sincronizado con cualquier cambio en ajustes.
        {
            let state_sub = state.clone();
            settings.subscribe(move |updated| {
                if let Ok(mut guard) = state_sub.write() {
                    guard.enabled = updated.shield_enabled;
                    guard.paused_hosts = updated.shield_paused.clone();
                }
            });
        }

        // Compilación y caché SHA-256 en segundo plano para no frenar el arranque en frío.
        let _ = std::thread::Builder::new()
            .name("search-shield-compile".to_string())
            .spawn(move || match CompiledShield::load_or_compile(&cache_dir) {
                Ok(compiled) => {
                    let hit = compiled.was_cache_hit();
                    let rules = compiled.rule_count();
                    let id = compiled.identifier().to_string();
                    if let Ok(mut guard) = state.write() {
                        guard.compiled = Some(Arc::new(compiled));
                        guard.trouble = None;
                    }
                    search_core::info!("Shield listo ({rules} reglas, cache_hit={hit}, id={id})");
                }
                Err(err) => {
                    let msg = format!("{err:#}");
                    search_core::warn!("Error al compilar las reglas de Shield: {msg}");
                    if let Ok(mut guard) = state.write() {
                        guard.trouble = Some(msg);
                    }
                }
            });

        mgr
    }

    /// Evalúa en el hilo IO de CEF si una petición saliente debe cancelarse.
    pub fn should_block_request(&self, request_url: &str, top_level_url: &str) -> bool {
        let Ok(guard) = self.state.read() else {
            return false;
        };
        let Some(ref compiled) = guard.compiled else {
            return false;
        };
        compiled.should_block_request(
            request_url,
            top_level_url,
            &guard.paused_hosts,
            guard.enabled,
        )
    }

    /// Devuelve el script de inyección CSS cosmética para `page_url` si Shield está activo ahí.
    pub fn cosmetic_inject_script_for_url(&self, page_url: &str) -> Option<String> {
        let guard = self.state.read().ok()?;
        let compiled = guard.compiled.as_ref()?;
        let css = compiled.cosmetic_css_for_url(page_url, &guard.paused_hosts, guard.enabled)?;
        Some(build_cosmetic_inject_script(&css))
    }

    /// Devuelve los dos scripts anti-anuncios de YouTube si `page_url` es un host reproductor
    /// de YouTube y Shield está activo y no pausado para ese sitio.
    pub fn youtube_scripts_for_url(&self, page_url: &str) -> Option<(String, String)> {
        let guard = self.state.read().ok()?;
        if !guard.enabled {
            return None;
        }
        if let Some(site) = site_host(page_url) {
            if is_host_paused(&site, &guard.paused_hosts) {
                return None;
            }
        }
        if !is_youtube_player_url(page_url) {
            return None;
        }
        Some((
            youtube_page_script(YOUTUBE_HOSTS),
            youtube_skipper_script(YOUTUBE_HOSTS),
        ))
    }

    pub fn is_enabled(&self) -> bool {
        self.state.read().map(|g| g.enabled).unwrap_or(true)
    }

    pub fn is_paused_on(&self, host: &str) -> bool {
        self.state
            .read()
            .map(|g| is_host_paused(host, &g.paused_hosts))
            .unwrap_or(false)
    }

    pub fn trouble(&self) -> Option<String> {
        self.state.read().ok().and_then(|g| g.trouble.clone())
    }

    /// Alterna la excepción de Shield para `host` y la persiste en `settings.json`.
    /// Devuelve `true` si tras el cambio Shield queda **pausado** en ese dominio.
    pub fn toggle_pause_for_host(&self, settings: &SettingsStore, host: &str) -> bool {
        let currently_paused = self.is_paused_on(host);
        let next_paused = !currently_paused;
        settings.update(|s| {
            set_host_paused(&mut s.shield_paused, host, next_paused);
        });
        next_paused
    }
}

/// Abre un popover compacto de información del sitio (`SiteCard.swift`) anclado a `anchor`
/// con estado TLS, bloqueos en esta página y conmutador de Shield para el dominio actual.
pub fn open_site_card_popover(
    anchor: &impl AsWidget,
    url: &str,
    blocked_count: u64,
    is_private: bool,
    shield: &ShieldManager,
    settings: &SettingsStore,
    on_reload: impl Fn() + 'static,
) -> Option<Popover> {
    let host = site_host(url)?;
    let is_https = url.starts_with("https://");

    let pop = Popover::new();
    pop.set_parent(anchor);
    pop.set_has_arrow(true);
    pop.set_autohide(true);
    pop.set_position(PositionType::Bottom);

    let root = BoxWidget::new(Orientation::Vertical, 8);
    root.set_margins(10, 10, 12, 12);
    root.set_size_request(280, -1);

    let host_lbl = Label::new(Some(&host));
    host_lbl.add_css_class("search-panel-title");
    host_lbl.set_xalign(0.0);
    host_lbl.set_ellipsize(EllipsizeMode::End);
    root.append(&host_lbl);

    let tls_text = if is_https {
        "Conexión segura (HTTPS)"
    } else {
        "Conexión no segura (HTTP)"
    };
    let tls_lbl = Label::new(Some(tls_text));
    tls_lbl.add_css_class("search-panel-subtitle");
    tls_lbl.set_xalign(0.0);
    root.append(&tls_lbl);

    root.append(&Separator::new(Orientation::Horizontal));

    let global_on = shield.is_enabled();
    let paused = shield.is_paused_on(&host);
    let active_here = global_on && !paused;

    let status_text = if let Some(err) = shield.trouble() {
        format!("Shield con error: {err}")
    } else if !global_on {
        "Shield está desactivado en Preferencias".to_string()
    } else if paused {
        "Shield pausado en este sitio".to_string()
    } else if blocked_count == 1 {
        "Shield activo · 1 elemento bloqueado".to_string()
    } else {
        format!("Shield activo · {blocked_count} elementos bloqueados")
    };

    let shield_lbl = Label::new(Some(&status_text));
    shield_lbl.set_xalign(0.0);
    shield_lbl.set_ellipsize(EllipsizeMode::End);
    root.append(&shield_lbl);

    if global_on && !is_private && shield.trouble().is_none() {
        let btn_text = if active_here {
            "Pausar Shield en este sitio"
        } else {
            "Reactivar Shield en este sitio"
        };
        let toggle_btn = Button::with_label(btn_text);
        toggle_btn.set_halign(Align::Fill);
        if !active_here {
            toggle_btn.add_css_class("suggested-action");
        }
        let sh = shield.clone();
        let st = settings.clone();
        let pop_ref = pop.clone();
        let host_owned = host.clone();
        toggle_btn.connect_clicked(move || {
            sh.toggle_pause_for_host(&st, &host_owned);
            pop_ref.popdown();
            on_reload();
        });
        root.append(&toggle_btn);
    } else if is_private {
        let priv_lbl = Label::new(Some(
            "Las excepciones por sitio no se guardan en ventanas privadas.",
        ));
        priv_lbl.add_css_class("search-panel-subtitle");
        priv_lbl.set_xalign(0.0);
        root.append(&priv_lbl);
    }

    pop.set_child(Some(&root));
    pop.popup();
    Some(pop)
}
