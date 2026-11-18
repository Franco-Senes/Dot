//! Ventana principal minimalista (`AdwApplicationWindow`) de Fase 1:
//! - `AdwHeaderBar` limpia que contiene **únicamente** la barra única centrada de dirección/búsqueda
//!   (sin botones de recarga, casa, extensiones ni menú hamburguesa).
//! - Barra de progreso fina (`.osd`) visible exclusivamente mientras carga la página.
//! - Barra discreta `FindBar` (`Ctrl+F`) revelada solo bajo demanda.
//! - Vista `BrowserView` desacoplada para que la Fase 2 pueda alojar múltiples instancias en `AdwTabView`.

use crate::actions;
use crate::address::AddressBar;
use crate::browser_view::{BrowserCallbacks, BrowserView};
use crate::find_bar::FindBar;
use crate::settings::SettingsStore;
use anyhow::Result;
use cef::RequestContext;
use gnome_platform::adw::{
    Application, ApplicationWindow, CenteringPolicy, HeaderBar, ToolbarStyle, ToolbarView,
};
use gnome_platform::gtk::{Align, AsWidget, BoxWidget, Orientation, Overlay, ProgressBar};
use search_core::address::resolve_address;

#[derive(Clone)]
pub struct BrowserWindow {
    window: ApplicationWindow,
    toolbar_view: ToolbarView,
    address_bar: AddressBar,
    find_bar: FindBar,
    progress_bar: ProgressBar,
    browser_view: BrowserView,
    settings: SettingsStore,
}

impl BrowserWindow {
    pub fn new(
        app: &Application,
        request_context: RequestContext,
        settings: SettingsStore,
        initial_input: Option<&str>,
    ) -> Result<Self> {
        let window = ApplicationWindow::new(app);
        window.set_title("Search");
        window.set_default_size(1180, 780);

        let toolbar_view = ToolbarView::new();
        toolbar_view.set_top_bar_style(ToolbarStyle::Flat);

        // Barra superior estrictamente minimalista: solo el campo centrado de dirección/búsqueda.
        let header_bar = HeaderBar::new();
        header_bar.add_css_class("search-headerbar");
        header_bar.set_centering_policy(CenteringPolicy::Strict);

        let address_bar = AddressBar::new();
        header_bar.set_title_widget(Some(address_bar.widget()));
        toolbar_view.add_top_bar(&header_bar);

        let find_bar = FindBar::new();

        let initial_url = match initial_input.map(str::trim).filter(|s| !s.is_empty()) {
            Some(raw) => {
                let (engine, custom) =
                    settings.with(|s| (s.search_engine, s.custom_search_url.clone()));
                resolve_address(raw, engine, &custom).url
            }
            None => "about:blank".to_string(),
        };

        let is_dark = app.style_manager().is_dark();
        let browser_view =
            BrowserView::new(request_context, settings.clone(), &initial_url, is_dark)?;

        if initial_url != "about:blank" {
            address_bar.set_committed_url(&initial_url);
        }

        // Contenedor central: FindBar arriba (oculta por defecto) + Overlay con BrowserView y ProgressBar fina.
        let progress_bar = ProgressBar::new();
        progress_bar.add_css_class("osd");
        progress_bar.add_css_class("search-load-progress");
        progress_bar.set_valign(Align::Start);
        progress_bar.set_hexpand(true);
        progress_bar.set_can_target(false);
        progress_bar.set_visible(false);

        let stage_overlay = Overlay::new();
        stage_overlay.set_hexpand(true);
        stage_overlay.set_vexpand(true);
        stage_overlay.set_child(Some(browser_view.widget()));
        stage_overlay.add_overlay(&progress_bar);
        stage_overlay.set_measure_overlay(&progress_bar, false);

        let content_box = BoxWidget::new(Orientation::Vertical, 0);
        content_box.set_hexpand(true);
        content_box.set_vexpand(true);
        content_box.append(find_bar.widget());
        content_box.append(&stage_overlay);

        toolbar_view.set_content(Some(&content_box));
        window.set_content(Some(&toolbar_view));

        let bw = Self {
            window,
            toolbar_view,
            address_bar,
            find_bar,
            progress_bar,
            browser_view,
            settings,
        };

        bw.wire_signals();
        actions::install_window_actions(&bw.window, &bw);

        if initial_url == "about:blank" {
            bw.focus_address_bar();
        } else {
            bw.browser_view.grab_focus();
        }

        Ok(bw)
    }

    pub fn present(&self) {
        self.window.present();
    }

    pub fn gtk_window(&self) -> &ApplicationWindow {
        &self.window
    }

    pub fn browser_view(&self) -> &BrowserView {
        &self.browser_view
    }

    pub fn find_bar(&self) -> &FindBar {
        &self.find_bar
    }

    pub fn focus_address_bar(&self) {
        self.address_bar.focus_and_select();
    }

    pub fn open_find_bar(&self) {
        self.find_bar.open_and_focus();
    }

    pub fn navigate_typed_input(&self, raw_typed: &str) {
        let (engine, custom) = self
            .settings
            .with(|s| (s.search_engine, s.custom_search_url.clone()));
        let resolved = resolve_address(raw_typed, engine, &custom);
        self.address_bar.set_committed_url(&resolved.url);
        self.browser_view.load_url(&resolved.url);
        self.browser_view.grab_focus();
    }

    pub fn handle_escape(&self) {
        if self.window.is_fullscreen() {
            self.set_fullscreen(false);
            return;
        }
        if self.find_bar.is_open() {
            self.find_bar.close();
            return;
        }
        if self.address_bar.has_focus() {
            self.browser_view.grab_focus();
            return;
        }
        if self.browser_view.is_loading() {
            self.browser_view.stop_load();
        }
    }

    pub fn set_fullscreen(&self, fullscreen: bool) {
        self.window.set_fullscreen(fullscreen);
        self.toolbar_view.set_reveal_top_bars(!fullscreen);
    }

    pub fn toggle_fullscreen(&self) {
        self.set_fullscreen(!self.window.is_fullscreen());
    }

    pub fn zoom_in(&self) {
        self.browser_view.zoom_in();
        let z = self.browser_view.zoom_factor();
        self.settings.update(|s| s.page_zoom = z);
    }

    pub fn zoom_out(&self) {
        self.browser_view.zoom_out();
        let z = self.browser_view.zoom_factor();
        self.settings.update(|s| s.page_zoom = z);
    }

    pub fn zoom_reset(&self) {
        self.browser_view.zoom_reset();
        self.settings.update(|s| s.page_zoom = 1.0);
    }

    fn wire_signals(&self) {
        // Barra de direcciones: envío y tecla Escape.
        {
            let w = self.clone();
            self.address_bar.connect_submit(move |typed| {
                w.navigate_typed_input(&typed);
            });
        }
        {
            let bv = self.browser_view.clone();
            self.address_bar.connect_escape(move || {
                bv.grab_focus();
            });
        }

        // Barra de búsqueda en página.
        {
            let bv = self.browser_view.clone();
            self.find_bar
                .connect_find(move |query, forward, find_next| {
                    bv.find(&query, forward, false, find_next);
                });
        }
        {
            let bv = self.browser_view.clone();
            self.find_bar.connect_close(move || {
                bv.stop_finding(true);
                bv.grab_focus();
            });
        }

        // Señales de la vista web hacia la ventana.
        let mut callbacks = BrowserCallbacks::default();
        {
            let gtk_win = self.window.clone();
            callbacks.on_title_changed = Some(Box::new(move |title| {
                let clean = title.trim();
                if clean.is_empty() {
                    gtk_win.set_title("Search");
                } else {
                    gtk_win.set_title(clean);
                }
            }));
        }
        {
            let addr = self.address_bar.clone();
            callbacks.on_address_changed = Some(Box::new(move |url| {
                addr.set_committed_url(&url);
            }));
        }
        {
            let pbar = self.progress_bar.clone();
            callbacks.on_loading_state_changed = Some(Box::new(move |is_loading, _back, _fwd| {
                if is_loading {
                    pbar.set_fraction(0.08);
                    pbar.set_visible(true);
                } else {
                    pbar.set_fraction(1.0);
                    pbar.set_visible(false);
                }
            }));
        }
        {
            let pbar = self.progress_bar.clone();
            let bv_loading = self.browser_view.clone();
            callbacks.on_progress_changed = Some(Box::new(move |progress| {
                if bv_loading.is_loading() && progress < 1.0 {
                    pbar.set_fraction(progress.max(0.06));
                    pbar.set_visible(true);
                } else {
                    pbar.set_visible(false);
                }
            }));
        }
        {
            let w_fs = self.clone();
            callbacks.on_fullscreen_changed = Some(Box::new(move |fullscreen| {
                w_fs.set_fullscreen(fullscreen);
            }));
        }
        {
            let fb = self.find_bar.clone();
            callbacks.on_find_result = Some(Box::new(move |ordinal, count| {
                fb.set_match_status(ordinal, count);
            }));
        }
        self.browser_view.set_callbacks(callbacks);

        // Cierre limpio del `cef::Browser` al cerrar la ventana.
        {
            let bv = self.browser_view.clone();
            self.window.connect_close_request(move || {
                bv.close_browser();
                false
            });
        }
    }
}
