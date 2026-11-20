//! Integración en tiempo real de `Shield` en CEF (`Shield.swift` + `YouTubeAds.swift` + `SiteCard.swift`).
//!
//! - Compila/carga la caché SHA-256 (`$XDG_CACHE_HOME/search/filters`) en un hilo en segundo plano
//!   para no bloquear el arranque de la aplicación.
//! - Estado protegido con `Arc<RwLock<...>>` (`Send + Sync`) para evaluarse con seguridad desde
//!   el hilo IO de CEF en `CefResourceRequestHandler::on_before_resource_load` sin tocar `RefCell`.
//! - Se aplica por igual a ventanas normales y ventanas privadas.

use crate::settings::SettingsStore;
use dot_core::domain::{raw_url_host, site_host};
use dot_core::shield::{
    build_cosmetic_inject_script, is_host_paused, is_youtube_player_url, set_host_paused,
    youtube_page_script, youtube_skipper_script, CategoryStats, CategoryToggles, CompiledShield,
    ShieldCategory, YOUTUBE_HOSTS,
};
use gnome_platform::adw::{ActionRow, SwitchRow};
use gnome_platform::gtk::{
    Align, AsWidget, BoxWidget, Button, EllipsizeMode, Label, ListBox, Orientation, PolicyType,
    Popover, PositionType, ScrolledWindow, SelectionMode, Separator,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex, RwLock};

const MAX_RECENT_DOMAINS_PER_SITE: usize = 25;

/// Registro de un dominio bloqueado recientemente en una pestaña/sitio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockedDomainEntry {
    pub domain: String,
    pub category: ShieldCategory,
    pub count: u64,
}

struct ShieldState {
    compiled: Option<Arc<CompiledShield>>,
    trouble: Option<String>,
    enabled: bool,
    paused_hosts: Vec<String>,
    categories: CategoryToggles,
}

#[derive(Default)]
struct ShieldTelemetry {
    total_stats: CategoryStats,
    per_site_stats: HashMap<String, CategoryStats>,
    recent_blocked_by_site: HashMap<String, VecDeque<BlockedDomainEntry>>,
    domain_exceptions: HashMap<String, HashSet<String>>,
}

impl ShieldTelemetry {
    fn record_block(&mut self, top_site: &str, blocked_domain: &str, category: ShieldCategory) {
        self.total_stats.increment(category);
        self.per_site_stats
            .entry(top_site.to_string())
            .or_default()
            .increment(category);

        let list = self
            .recent_blocked_by_site
            .entry(top_site.to_string())
            .or_default();

        if let Some(pos) = list
            .iter()
            .position(|e| e.domain == blocked_domain && e.category == category)
        {
            if let Some(mut entry) = list.remove(pos) {
                entry.count = entry.count.saturating_add(1);
                list.push_front(entry);
            }
        } else {
            if list.len() >= MAX_RECENT_DOMAINS_PER_SITE {
                list.pop_back();
            }
            list.push_front(BlockedDomainEntry {
                domain: blocked_domain.to_string(),
                category,
                count: 1,
            });
        }
    }

    fn is_domain_allowed(&self, site: &str, domain: &str) -> bool {
        self.domain_exceptions
            .get(site)
            .is_some_and(|set| set.contains(domain))
    }

    fn toggle_domain_exception(&mut self, site: &str, domain: &str) -> bool {
        let set = self.domain_exceptions.entry(site.to_string()).or_default();
        if set.contains(domain) {
            set.remove(domain);
            if set.is_empty() {
                self.domain_exceptions.remove(site);
            }
            false
        } else {
            set.insert(domain.to_string());
            true
        }
    }
}

/// Gestor compartido y thread-safe de Shield para todas las pestañas (normales y privadas).
#[derive(Clone)]
pub struct ShieldManager {
    state: Arc<RwLock<ShieldState>>,
    telemetry: Arc<Mutex<ShieldTelemetry>>,
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
            categories: CategoryToggles::default(),
        }));
        let telemetry = Arc::new(Mutex::new(ShieldTelemetry::default()));

        let mgr = Self {
            state: state.clone(),
            telemetry,
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
                    dot_core::info!("Shield listo ({rules} reglas, cache_hit={hit}, id={id})");
                }
                Err(err) => {
                    let msg = format!("{err:#}");
                    dot_core::warn!("Error al compilar las reglas de Shield: {msg}");
                    if let Ok(mut guard) = state.write() {
                        guard.trouble = Some(msg);
                    }
                }
            });

        mgr
    }

    #[cfg(test)]
    fn new_for_tests(compiled: CompiledShield) -> Self {
        Self {
            state: Arc::new(RwLock::new(ShieldState {
                compiled: Some(Arc::new(compiled)),
                trouble: None,
                enabled: true,
                paused_hosts: Vec::new(),
                categories: CategoryToggles::default(),
            })),
            telemetry: Arc::new(Mutex::new(ShieldTelemetry::default())),
        }
    }

    /// Evalúa en el hilo IO de CEF si una petición saliente debe cancelarse.
    pub fn should_block_request(&self, request_url: &str, top_level_url: &str) -> bool {
        let top_site = site_host(top_level_url).unwrap_or_else(|| "desconocido".to_string());
        let req_host = raw_url_host(request_url).unwrap_or_else(|| "desconocido".to_string());

        if let Ok(tel) = self.telemetry.lock() {
            if tel.is_domain_allowed(&top_site, &req_host) {
                return false;
            }
        }

        let category = {
            let Ok(guard) = self.state.read() else {
                return false;
            };
            let Some(ref compiled) = guard.compiled else {
                return false;
            };
            compiled.evaluate_request(
                request_url,
                top_level_url,
                &guard.paused_hosts,
                guard.enabled,
                guard.categories,
            )
        };

        if let Some(cat) = category {
            if let Ok(mut tel) = self.telemetry.lock() {
                tel.record_block(&top_site, &req_host, cat);
            }
            true
        } else {
            false
        }
    }

    /// Devuelve el script de inyección CSS cosmética para `page_url` si Shield está activo ahí.
    pub fn cosmetic_inject_script_for_url(&self, page_url: &str) -> Option<String> {
        let guard = self.state.read().ok()?;
        let compiled = guard.compiled.as_ref()?;
        let css = compiled.cosmetic_css_for_url_with_categories(
            page_url,
            &guard.paused_hosts,
            guard.enabled,
            guard.categories,
        )?;
        Some(build_cosmetic_inject_script(&css))
    }

    /// Devuelve los dos scripts anti-anuncios de YouTube si `page_url` es un host reproductor
    /// de YouTube y Shield está activo y no pausado para ese sitio.
    pub fn youtube_scripts_for_url(&self, page_url: &str) -> Option<(String, String)> {
        let guard = self.state.read().ok()?;
        if !guard.enabled || !guard.categories.youtube {
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

    pub fn categories(&self) -> CategoryToggles {
        self.state.read().map(|g| g.categories).unwrap_or_default()
    }

    pub fn set_category_enabled(&self, category: ShieldCategory, enabled: bool) {
        if let Ok(mut guard) = self.state.write() {
            guard.categories.set_enabled(category, enabled);
        }
    }

    pub fn total_stats(&self) -> CategoryStats {
        self.telemetry
            .lock()
            .map(|t| t.total_stats)
            .unwrap_or_default()
    }

    pub fn site_stats(&self, host: &str) -> CategoryStats {
        let norm = site_host(host).unwrap_or_else(|| host.to_ascii_lowercase());
        self.telemetry
            .lock()
            .ok()
            .and_then(|t| t.per_site_stats.get(&norm).copied())
            .unwrap_or_default()
    }

    pub fn recent_blocked_for_site(&self, host: &str) -> Vec<BlockedDomainEntry> {
        let norm = site_host(host).unwrap_or_else(|| host.to_ascii_lowercase());
        self.telemetry
            .lock()
            .ok()
            .and_then(|t| t.recent_blocked_by_site.get(&norm).cloned())
            .map(|q| q.into_iter().collect())
            .unwrap_or_default()
    }

    pub fn is_domain_allowed_on_site(&self, site: &str, domain: &str) -> bool {
        let norm_site = site_host(site).unwrap_or_else(|| site.to_ascii_lowercase());
        let norm_domain = domain.trim().to_ascii_lowercase();
        self.telemetry
            .lock()
            .map(|t| t.is_domain_allowed(&norm_site, &norm_domain))
            .unwrap_or(false)
    }

    pub fn toggle_domain_exception(&self, site: &str, domain: &str) -> bool {
        let norm_site = site_host(site).unwrap_or_else(|| site.to_ascii_lowercase());
        let norm_domain = domain.trim().to_ascii_lowercase();
        self.telemetry
            .lock()
            .map(|mut t| t.toggle_domain_exception(&norm_site, &norm_domain))
            .unwrap_or(false)
    }

    pub fn clear_site_exceptions(&self, site: &str) {
        let norm_site = site_host(site).unwrap_or_else(|| site.to_ascii_lowercase());
        if let Ok(mut t) = self.telemetry.lock() {
            t.domain_exceptions.remove(&norm_site);
        }
    }

    pub fn site_exception_count(&self, site: &str) -> usize {
        let norm_site = site_host(site).unwrap_or_else(|| site.to_ascii_lowercase());
        self.telemetry
            .lock()
            .ok()
            .and_then(|t| t.domain_exceptions.get(&norm_site).map(HashSet::len))
            .unwrap_or(0)
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

fn format_category_subtitle(tab_count: u64, total_count: u64) -> String {
    format!("{tab_count} en esta pestaña · {total_count} en total")
}

/// Abre un popover compacto de información del sitio (`SiteCard.swift`) anclado a `anchor`
/// con estado TLS, bloqueos por categoría, dominios recientes y controles de excepción rápida.
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
    let reload_cb: Rc<dyn Fn()> = Rc::new(on_reload);

    let pop = Popover::new();
    pop.set_parent(anchor);
    pop.set_has_arrow(true);
    pop.set_autohide(true);
    pop.set_position(PositionType::Bottom);

    let root = BoxWidget::new(Orientation::Vertical, 10);
    root.set_margins(12, 12, 12, 12);
    root.set_size_request(340, -1);

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
        let reload = reload_cb.clone();
        toggle_btn.connect_clicked(move || {
            sh.toggle_pause_for_host(&st, &host_owned);
            pop_ref.popdown();
            reload();
        });
        root.append(&toggle_btn);
    } else if is_private {
        let priv_lbl = Label::new(Some(
            "Las excepciones por sitio no se guardan en ventanas privadas.",
        ));
        priv_lbl.add_css_class("search-panel-subtitle");
        priv_lbl.set_wrap(true);
        priv_lbl.set_xalign(0.0);
        root.append(&priv_lbl);
    }

    // Botón para recargar cuando se ajustan categorías o excepciones de dominio.
    let reload_btn = Button::with_label("Recargar página");
    reload_btn.add_css_class("suggested-action");
    reload_btn.set_halign(Align::Fill);
    reload_btn.set_visible(false);
    {
        let pop_ref = pop.clone();
        let reload = reload_cb.clone();
        reload_btn.connect_clicked(move || {
            pop_ref.popdown();
            reload();
        });
    }

    if global_on && shield.trouble().is_none() {
        root.append(&Separator::new(Orientation::Horizontal));

        let cat_heading = Label::new(Some("Protección por categoría"));
        cat_heading.add_css_class("heading");
        cat_heading.set_xalign(0.0);
        root.append(&cat_heading);

        let cat_list = ListBox::new();
        cat_list.set_selection_mode(SelectionMode::None);
        cat_list.add_css_class("boxed-list");

        let toggles = shield.categories();
        let tab_stats = shield.site_stats(&host);
        let total_stats = shield.total_stats();

        for category in ShieldCategory::ALL {
            let row = SwitchRow::new();
            row.set_title(category.label_es());
            row.set_subtitle(&format_category_subtitle(
                tab_stats.get(category),
                total_stats.get(category),
            ));
            row.set_active(toggles.is_enabled(category));

            let sh = shield.clone();
            let row_ref = row.clone();
            let reload_hint = reload_btn.clone();
            row.connect_active_notify(move || {
                sh.set_category_enabled(category, row_ref.is_active());
                reload_hint.set_visible(true);
            });
            cat_list.append(&row);
        }
        root.append(&cat_list);

        root.append(&Separator::new(Orientation::Horizontal));

        let recent_header = BoxWidget::new(Orientation::Horizontal, 8);
        let recent_heading = Label::new(Some("Dominios bloqueados recientes"));
        recent_heading.add_css_class("heading");
        recent_heading.set_xalign(0.0);
        recent_heading.set_hexpand(true);
        recent_header.append(&recent_heading);

        let clear_btn = Button::with_label("Limpiar excepciones");
        clear_btn.add_css_class("flat");
        clear_btn.set_visible(shield.site_exception_count(&host) > 0);
        recent_header.append(&clear_btn);
        root.append(&recent_header);

        let recent_list = ListBox::new();
        recent_list.set_selection_mode(SelectionMode::None);
        recent_list.add_css_class("boxed-list");

        let recent = shield.recent_blocked_for_site(&host);
        let exception_buttons: Rc<std::cell::RefCell<Vec<(Button, String)>>> =
            Rc::new(std::cell::RefCell::new(Vec::new()));

        if recent.is_empty() {
            let empty_box = BoxWidget::new(Orientation::Vertical, 2);
            empty_box.set_margins(10, 10, 10, 10);
            let empty_title = Label::new(Some("Ningún dominio bloqueado en esta pestaña"));
            empty_title.set_xalign(0.0);
            let empty_sub = Label::new(Some(
                "Los anuncios y rastreadores bloqueados aparecerán aquí.",
            ));
            empty_sub.add_css_class("dim-label");
            empty_sub.add_css_class("caption");
            empty_sub.set_wrap(true);
            empty_sub.set_xalign(0.0);
            empty_box.append(&empty_title);
            empty_box.append(&empty_sub);
            recent_list.append(&empty_box);
        } else {
            for entry in recent {
                let row = ActionRow::new();
                row.set_use_markup(false);
                row.set_title(&entry.domain);
                row.set_subtitle(&format!("{} ({})", entry.category.label_es(), entry.count));

                let allowed = shield.is_domain_allowed_on_site(&host, &entry.domain);
                let ex_btn = Button::with_label(if allowed { "Bloquear" } else { "Permitir" });
                ex_btn.add_css_class("flat");
                ex_btn.set_valign(Align::Center);
                ex_btn.set_tooltip_text(Some(if allowed {
                    "Volver a bloquear este dominio en este sitio"
                } else {
                    "Permitir este dominio en este sitio"
                }));

                let sh = shield.clone();
                let host_owned = host.clone();
                let domain_owned = entry.domain.clone();
                let btn_ref = ex_btn.clone();
                let clear_ref = clear_btn.clone();
                let reload_hint = reload_btn.clone();
                ex_btn.connect_clicked(move || {
                    let now_allowed = sh.toggle_domain_exception(&host_owned, &domain_owned);
                    btn_ref.set_label(if now_allowed { "Bloquear" } else { "Permitir" });
                    btn_ref.set_tooltip_text(Some(if now_allowed {
                        "Volver a bloquear este dominio en este sitio"
                    } else {
                        "Permitir este dominio en este sitio"
                    }));
                    clear_ref.set_visible(sh.site_exception_count(&host_owned) > 0);
                    reload_hint.set_visible(true);
                });

                exception_buttons
                    .borrow_mut()
                    .push((ex_btn.clone(), entry.domain));
                row.add_suffix(&ex_btn);
                recent_list.append(&row);
            }
        }

        {
            let sh = shield.clone();
            let host_owned = host.clone();
            let clear_ref = clear_btn.clone();
            let reload_hint = reload_btn.clone();
            let btns = exception_buttons.clone();
            clear_btn.connect_clicked(move || {
                sh.clear_site_exceptions(&host_owned);
                clear_ref.set_visible(false);
                for (btn, _) in btns.borrow().iter() {
                    btn.set_label("Permitir");
                    btn.set_tooltip_text(Some("Permitir este dominio en este sitio"));
                }
                reload_hint.set_visible(true);
            });
        }

        let scroller = ScrolledWindow::new();
        scroller.set_policy(PolicyType::Never, PolicyType::Automatic);
        scroller.set_min_content_height(80);
        scroller.set_max_content_height(160);
        scroller.set_child(Some(&recent_list));
        root.append(&scroller);
        root.append(&reload_btn);
    }

    pop.set_child(Some(&root));
    pop.popup();
    Some(pop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shield_manager_tracks_category_stats_recent_domains_and_exceptions() {
        let compiled = CompiledShield::compile_embedded_in_memory().expect("compile shield");
        let mgr = ShieldManager::new_for_tests(compiled);

        let site = "https://www.nytimes.com/article";
        assert!(mgr.should_block_request(
            "https://pagead2.googlesyndication.com/pagead/js/adsbygoogle.js",
            site
        ));
        assert!(mgr.should_block_request("https://www.google-analytics.com/analytics.js", site));
        assert!(mgr.should_block_request("https://connect.facebook.net/en_US/sdk.js", site));

        let stats = mgr.site_stats("nytimes.com");
        assert_eq!(stats.ads, 1);
        assert_eq!(stats.trackers, 1);
        assert_eq!(stats.social_annoyances, 1);
        assert_eq!(stats.total(), 3);

        let recent = mgr.recent_blocked_for_site("nytimes.com");
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].domain, "connect.facebook.net");

        // Excepción rápida por dominio en este sitio.
        assert!(mgr.toggle_domain_exception("nytimes.com", "connect.facebook.net"));
        assert!(mgr.is_domain_allowed_on_site("nytimes.com", "connect.facebook.net"));
        assert!(!mgr.should_block_request("https://connect.facebook.net/en_US/sdk.js", site));
        assert_eq!(mgr.site_exception_count("nytimes.com"), 1);

        // Limpiar excepciones vuelve a bloquear.
        mgr.clear_site_exceptions("nytimes.com");
        assert_eq!(mgr.site_exception_count("nytimes.com"), 0);
        assert!(mgr.should_block_request("https://connect.facebook.net/en_US/sdk.js", site));

        // Desactivar categoría Trackers.
        mgr.set_category_enabled(ShieldCategory::Trackers, false);
        assert!(!mgr.should_block_request("https://www.google-analytics.com/analytics.js", site));
    }
}
