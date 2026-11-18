//! Ventana principal (`AdwApplicationWindow`) de Fase 2:
//! - Modelo único `AdwTabView` con soporte de carga diferida (lazy): las pestañas restauradas
//!   en segundo plano no instancian `cef::Browser` hasta ser seleccionadas.
//! - Dos disposiciones conmutables en vivo sin reiniciar (`Ctrl+Shift+S` y Preferencias `Ctrl+,`):
//!   - Pestañas arriba (`AdwTabBar` integrada bajo la `AdwHeaderBar` minimalista).
//!   - Barra lateral izquierda vertical (`SidebarTabs` enlazada al mismo `AdwTabView`).
//! - Plegado rápido (`Ctrl+S`), pestañas fijadas protegidas contra `Ctrl+W`, indicador de carga,
//!   favicons descargados vía CEF (`GdkTexture` + `GBytesIcon`), indicador/silencio de audio,
//!   menú contextual completo y ventanas privadas (`RequestContext` efímero).

use crate::actions;
use crate::address::AddressBar;
use crate::browser_view::{BrowserCallbacks, BrowserView};
use crate::find_bar::FindBar;
use crate::settings::SettingsStore;
use crate::sidebar_tabs::{SidebarCallbacks, SidebarTabItem, SidebarTabs};
use crate::tab_switcher::SwitcherTabItem;
use anyhow::Result;
use cef::RequestContext;
use gnome_platform::adw::{
    Application, ApplicationWindow, CenteringPolicy, HeaderBar, TabBar, TabPage, TabView,
    TabViewShortcuts, ToolbarStyle, ToolbarView,
};
use gnome_platform::gdk::{Rectangle, Texture};
use gnome_platform::gio::{Icon, Menu, SimpleAction, SimpleActionGroup};
use gnome_platform::glib::{self, Bytes};
use gnome_platform::gtk::{
    Align, AsWidget, BoxWidget, Label, Orientation, Overlay, PopoverMenu, PositionType,
    ProgressBar, Revealer, RevealerTransitionType,
};
use search_core::address::{display_at_rest, resolve_address, sanitize_url_for_ui};
use search_core::session::{now_unix_secs, ClosedTab, SessionTab, SessionWindow};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_WINDOW_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_TAB_ID: AtomicU64 = AtomicU64::new(1);

/// Eventos que una ventana comunica al controlador global de aplicación (`app.rs`).
#[derive(Default)]
pub struct WindowCallbacks {
    pub on_session_dirty: Option<Box<dyn Fn()>>,
    pub on_tab_closed: Option<Box<dyn Fn(ClosedTab)>>,
    pub on_reopen_closed_tab: Option<Box<dyn Fn(BrowserWindow)>>,
    pub on_new_window: Option<Box<dyn Fn()>>,
    pub on_new_private_window: Option<Box<dyn Fn()>>,
    pub on_open_tab_switcher: Option<Box<dyn Fn(BrowserWindow)>>,
    pub on_move_tab_to_new_window: Option<Box<dyn Fn(SessionTab, bool)>>,
    pub on_window_closed: Option<Box<dyn Fn(u64)>>,
}

struct TabRecord {
    id: u64,
    page: TabPage,
    slot: BoxWidget,
    url: String,
    title: String,
    pin: Option<String>,
    home: Option<String>,
    touched: f64,
    loading: bool,
    progress: f64,
    audible: bool,
    muted: bool,
    favicon_texture: Option<Texture>,
    browser: Option<BrowserView>,
}

impl TabRecord {
    fn display_title(&self) -> String {
        let trimmed = self.title.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
        let rest = display_at_rest(&self.url);
        if !rest.is_empty() {
            return rest;
        }
        "Nueva pestaña".to_string()
    }

    fn tooltip_text(&self) -> String {
        let t = self.display_title();
        if self.url.is_empty() || self.url == "about:blank" {
            t
        } else {
            format!("{t}\n{}", sanitize_url_for_ui(&self.url))
        }
    }

    fn to_session_tab(&self) -> SessionTab {
        SessionTab {
            url: self.url.clone(),
            title: self.title.clone(),
            pin: self.pin.clone(),
            name: None,
            home: self.home.clone(),
            group_id: None,
            touched: Some(self.touched),
            muted: self.muted,
        }
    }

    fn to_closed_tab(&self, position: usize) -> ClosedTab {
        ClosedTab {
            url: self.url.clone(),
            title: self.title.clone(),
            pinned: self.pin.is_some(),
            pin_label: self.pin.clone(),
            home: self.home.clone(),
            position,
            muted: self.muted,
        }
    }
}

struct WindowState {
    id: u64,
    is_private: bool,
    app: Application,
    window: ApplicationWindow,
    toolbar_view: ToolbarView,
    address_bar: AddressBar,
    find_bar: FindBar,
    progress_bar: ProgressBar,
    tab_view: TabView,
    top_bar_revealer: Revealer,
    sidebar_tabs: SidebarTabs,
    sidebar_revealer: Revealer,
    tab_menu: Menu,
    tab_close_action: SimpleAction,
    active_popover: RefCell<Option<PopoverMenu>>,
    menu_target_tab: Cell<Option<u64>>,
    request_context: RequestContext,
    settings: SettingsStore,
    tabs: RefCell<Vec<TabRecord>>,
    folded: Cell<bool>,
    suppress_auto_wake: Cell<bool>,
    closing_window: Cell<bool>,
    callbacks: WindowCallbacks,
}

#[derive(Clone)]
pub struct BrowserWindow {
    inner: Rc<WindowState>,
}

impl BrowserWindow {
    pub fn new(
        app: &Application,
        request_context: RequestContext,
        settings: SettingsStore,
        is_private: bool,
        callbacks: WindowCallbacks,
    ) -> Result<Self> {
        let id = NEXT_WINDOW_ID.fetch_add(1, Ordering::Relaxed);
        let window = ApplicationWindow::new(app);
        window.set_title(if is_private {
            "Search — Privada"
        } else {
            "Search"
        });
        window.set_default_size(1180, 780);
        if is_private {
            window.add_css_class("search-private-window");
        }

        let toolbar_view = ToolbarView::new();
        toolbar_view.set_top_bar_style(ToolbarStyle::Flat);

        // Barra superior minimalista: solo la barra centrada de dirección/búsqueda
        // (+ insignia discreta únicamente en ventanas privadas).
        let header_bar = HeaderBar::new();
        header_bar.add_css_class("search-headerbar");
        header_bar.set_centering_policy(CenteringPolicy::Strict);

        if is_private {
            let badge = Label::new(Some("Privada"));
            badge.add_css_class("search-private-badge");
            badge.set_tooltip_text(Some(
                "Ventana privada: contexto en memoria, sin sesión ni historial",
            ));
            header_bar.pack_start(&badge);
        }

        let address_bar = AddressBar::new();
        header_bar.set_title_widget(Some(address_bar.widget()));
        toolbar_view.add_top_bar(&header_bar);

        // Modelo único de pestañas (`AdwTabView`) y barra superior (`AdwTabBar`).
        let tab_view = TabView::new();
        tab_view.set_hexpand(true);
        tab_view.set_vexpand(true);
        tab_view.set_default_icon(&Icon::themed("web-browser-symbolic"));
        // Desactivamos los atajos internos de AdwTabView para usar nuestras acciones `win.*`
        // con comportamiento uniforme (Ctrl+Tab cíclico, Ctrl+1..9, etc.).
        tab_view.set_shortcuts(TabViewShortcuts::NONE);

        let tab_menu = Menu::new();
        tab_view.set_menu_model(Some(&tab_menu));

        let tab_bar = TabBar::new();
        tab_bar.set_view(Some(&tab_view));
        tab_bar.set_autohide(false);
        tab_bar.set_expand_tabs(true);

        let top_bar_revealer = Revealer::new();
        top_bar_revealer.set_transition_type(RevealerTransitionType::SlideDown);
        top_bar_revealer.set_transition_duration(140);
        top_bar_revealer.set_child(Some(&tab_bar));
        toolbar_view.add_top_bar(&top_bar_revealer);

        // Barra lateral vertical de pestañas enlazada al mismo `AdwTabView`.
        let initial_sidebar_width = settings.with(|s| s.sidebar_width.round() as i32);
        let sidebar_tabs = SidebarTabs::new(initial_sidebar_width);
        let sidebar_revealer = Revealer::new();
        sidebar_revealer.set_transition_type(RevealerTransitionType::SlideRight);
        sidebar_revealer.set_transition_duration(140);
        sidebar_revealer.set_child(Some(sidebar_tabs.widget()));

        let find_bar = FindBar::new();

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
        stage_overlay.set_child(Some(&tab_view));
        stage_overlay.add_overlay(&progress_bar);
        stage_overlay.set_measure_overlay(&progress_bar, false);

        let content_box = BoxWidget::new(Orientation::Vertical, 0);
        content_box.set_hexpand(true);
        content_box.set_vexpand(true);
        content_box.append(find_bar.widget());
        content_box.append(&stage_overlay);

        let main_hbox = BoxWidget::new(Orientation::Horizontal, 0);
        main_hbox.set_hexpand(true);
        main_hbox.set_vexpand(true);
        main_hbox.append(&sidebar_revealer);
        main_hbox.append(&content_box);

        toolbar_view.set_content(Some(&main_hbox));
        window.set_content(Some(&toolbar_view));

        let tab_close_action = SimpleAction::new("close");

        let bw = Self {
            inner: Rc::new(WindowState {
                id,
                is_private,
                app: app.clone(),
                window,
                toolbar_view,
                address_bar,
                find_bar,
                progress_bar,
                tab_view,
                top_bar_revealer,
                sidebar_tabs,
                sidebar_revealer,
                tab_menu,
                tab_close_action,
                active_popover: RefCell::new(None),
                menu_target_tab: Cell::new(None),
                request_context,
                settings: settings.clone(),
                tabs: RefCell::new(Vec::new()),
                folded: Cell::new(false),
                suppress_auto_wake: Cell::new(false),
                closing_window: Cell::new(false),
                callbacks,
            }),
        };

        bw.install_tab_context_actions();
        bw.wire_signals();
        actions::install_window_actions(&bw.inner.window, &bw);
        bw.apply_layout_from_settings(&settings.snapshot());

        // Suscripción en vivo para cambiar entre pestañas arriba / barra lateral sin reiniciar.
        {
            let w_sub = bw.clone();
            settings.subscribe(move |updated| {
                w_sub.apply_layout_from_settings(updated);
            });
        }

        Ok(bw)
    }

    pub fn id(&self) -> u64 {
        self.inner.id
    }

    pub fn is_private(&self) -> bool {
        self.inner.is_private
    }

    pub fn present(&self) {
        self.inner.window.present();
    }

    pub fn gtk_window(&self) -> &ApplicationWindow {
        &self.inner.window
    }

    pub fn settings(&self) -> &SettingsStore {
        &self.inner.settings
    }

    pub fn find_bar(&self) -> &FindBar {
        &self.inner.find_bar
    }

    pub fn tab_count(&self) -> usize {
        self.inner.tabs.borrow().len()
    }

    pub fn pinned_count(&self) -> usize {
        self.inner
            .tabs
            .borrow()
            .iter()
            .filter(|t| t.pin.is_some())
            .count()
    }

    /// Devuelve la vista `BrowserView` activa (si ya está instanciada).
    pub fn active_browser_view(&self) -> Option<BrowserView> {
        let sel = self.inner.tab_view.selected_page()?;
        self.inner
            .tabs
            .borrow()
            .iter()
            .find(|t| t.page == sel)
            .and_then(|t| t.browser.clone())
    }

    /// Despierta si hace falta la pestaña activa y devuelve su `BrowserView`.
    pub fn ensure_active_browser_view(&self) -> Option<BrowserView> {
        let id = self.selected_tab_id()?;
        self.ensure_tab_awake(id)
    }

    pub fn focus_address_bar(&self) {
        self.inner.address_bar.focus_and_select();
    }

    pub fn open_find_bar(&self) {
        self.inner.find_bar.open_and_focus();
    }

    /// Abre una pestaña nueva (inmediata si `select == true`, o diferida si `lazy == true`).
    pub fn open_tab(
        &self,
        raw_input: Option<&str>,
        select: bool,
        pinned: bool,
        lazy: bool,
    ) -> Result<u64> {
        let url = match raw_input.map(str::trim).filter(|s| !s.is_empty()) {
            Some(raw) => {
                let (engine, custom) = self
                    .inner
                    .settings
                    .with(|s| (s.search_engine, s.custom_search_url.clone()));
                resolve_address(raw, engine, &custom).url
            }
            None => "about:blank".to_string(),
        };

        let pin = if pinned {
            Some(SessionTab::default_pin_label("", &url))
        } else {
            None
        };
        let home = if pinned { Some(url.clone()) } else { None };

        let tab_spec = SessionTab {
            url,
            title: String::new(),
            pin,
            name: None,
            home,
            group_id: None,
            touched: Some(now_unix_secs()),
            muted: false,
        };
        self.insert_tab_from_spec(tab_spec, None, select, lazy)
    }

    /// Inserta una pestaña a partir de un `SessionTab` en `position` (o al final).
    pub fn insert_tab_from_spec(
        &self,
        spec: SessionTab,
        position: Option<usize>,
        select: bool,
        lazy: bool,
    ) -> Result<u64> {
        let tab_id = NEXT_TAB_ID.fetch_add(1, Ordering::Relaxed);
        let slot = BoxWidget::new(Orientation::Vertical, 0);
        slot.set_hexpand(true);
        slot.set_vexpand(true);

        let is_pinned = spec.pin.is_some();
        let prev_suppress = self.inner.suppress_auto_wake.replace(true);
        let page = if is_pinned {
            self.inner.tab_view.append_pinned(&slot)
        } else if let Some(pos) = position {
            let n_pinned = self.inner.tab_view.n_pinned_pages().max(0) as usize;
            let n_total = self.inner.tab_view.n_pages().max(0) as usize;
            let target = pos.clamp(n_pinned, n_total) as i32;
            self.inner.tab_view.insert(&slot, target)
        } else {
            self.inner.tab_view.append(&slot)
        };
        self.inner.suppress_auto_wake.set(prev_suppress);

        let mut rec = TabRecord {
            id: tab_id,
            page: page.clone(),
            slot,
            url: if spec.url.trim().is_empty() {
                "about:blank".to_string()
            } else {
                spec.url
            },
            title: spec.title,
            pin: spec.pin,
            home: spec.home,
            touched: spec.touched.unwrap_or_else(now_unix_secs),
            loading: false,
            progress: 0.0,
            audible: false,
            muted: spec.muted,
            favicon_texture: None,
            browser: None,
        };

        let display_title = rec.display_title();
        let tooltip = rec.tooltip_text();
        page.set_title(&display_title);
        page.set_tooltip(Some(&tooltip));
        Self::apply_page_audio_indicator(&page, rec.audible, rec.muted);

        let is_blank = rec.url == "about:blank";
        let should_wake_now = !lazy;
        if should_wake_now {
            let bv = self.create_browser_view_for_tab(tab_id, &rec.url, rec.muted)?;
            rec.slot.append(bv.widget());
            rec.browser = Some(bv);
        }

        self.inner.tabs.borrow_mut().push(rec);
        self.sync_order_with_tab_view();

        if select {
            self.inner.tab_view.set_selected_page(&page);
            self.on_active_tab_changed();
            if is_blank {
                self.focus_address_bar();
            } else if let Some(bv) = self.active_browser_view() {
                bv.grab_focus();
            }
        } else {
            self.sync_sidebar();
        }

        self.notify_session_dirty();
        Ok(tab_id)
    }

    /// Restaura una ventana completa desde `SessionWindow` con carga diferida (lazy)
    /// para todas las pestañas no activas.
    pub fn restore_from_session(&self, saved: &SessionWindow) -> Result<()> {
        if saved.tabs.is_empty() {
            self.open_tab(None, true, false, false)?;
            return Ok(());
        }

        let active_idx = saved.clamped_active();
        let prev_suppress = self.inner.suppress_auto_wake.replace(true);

        // Primero restaurar fijadas y luego regulares respetando el orden guardado.
        let mut created_ids = Vec::with_capacity(saved.tabs.len());
        for tab in &saved.tabs {
            let id = self.insert_tab_from_spec(tab.clone(), None, false, true)?;
            created_ids.push(id);
        }

        self.inner.suppress_auto_wake.set(prev_suppress);

        let target_id = created_ids
            .get(active_idx)
            .copied()
            .or_else(|| created_ids.first().copied());
        if let Some(id) = target_id {
            self.select_tab_by_id(id);
        }

        Ok(())
    }

    /// Exporta la instantánea actual de esta ventana para `session.json` (solo ventanas no privadas).
    pub fn to_session_window(&self) -> SessionWindow {
        self.sync_order_with_tab_view();
        let selected_page = self.inner.tab_view.selected_page();
        let tabs_borrow = self.inner.tabs.borrow();
        let mut active = 0usize;
        let mut out_tabs = Vec::with_capacity(tabs_borrow.len());
        for (idx, t) in tabs_borrow.iter().enumerate() {
            if selected_page.as_ref() == Some(&t.page) {
                active = idx;
            }
            out_tabs.push(t.to_session_tab());
        }
        SessionWindow {
            tabs: out_tabs,
            active,
            sidebar: Some(self.inner.settings.with(|s| s.vertical_tabs)),
            groups: Vec::new(),
            splits: Vec::new(),
        }
    }

    /// Instantánea de todas las pestañas abiertas de esta ventana para el selector `Ctrl+K`.
    pub fn switcher_items(&self, window_label: Option<String>) -> Vec<SwitcherTabItem> {
        self.sync_order_with_tab_view();
        let selected_page = self.inner.tab_view.selected_page();
        self.inner
            .tabs
            .borrow()
            .iter()
            .map(|t| SwitcherTabItem {
                window_id: self.inner.id,
                window_label: window_label.clone(),
                is_private: self.inner.is_private,
                tab_id: t.id,
                title: t.display_title(),
                url: t.url.clone(),
                pinned: t.pin.is_some(),
                active: selected_page.as_ref() == Some(&t.page),
                audible: t.audible,
                muted: t.muted,
                favicon: t.favicon_texture.clone(),
            })
            .collect()
    }

    pub fn navigate_typed_input(&self, raw_typed: &str) {
        let (engine, custom) = self
            .inner
            .settings
            .with(|s| (s.search_engine, s.custom_search_url.clone()));
        let resolved = resolve_address(raw_typed, engine, &custom);
        self.inner.address_bar.set_committed_url(&resolved.url);

        if let Some(tab_id) = self.selected_tab_id() {
            {
                let mut tabs = self.inner.tabs.borrow_mut();
                if let Some(t) = tabs.iter_mut().find(|t| t.id == tab_id) {
                    t.url = resolved.url.clone();
                    t.touched = now_unix_secs();
                    let title = t.display_title();
                    let tip = t.tooltip_text();
                    t.page.set_title(&title);
                    t.page.set_tooltip(Some(&tip));
                }
            }
            if let Some(bv) = self.ensure_tab_awake(tab_id) {
                bv.load_url(&resolved.url);
                bv.grab_focus();
            }
            self.sync_sidebar();
            self.notify_session_dirty();
        }
    }

    pub fn select_tab_by_id(&self, tab_id: u64) -> bool {
        let page = self
            .inner
            .tabs
            .borrow()
            .iter()
            .find(|t| t.id == tab_id)
            .map(|t| t.page.clone());
        let Some(page) = page else {
            return false;
        };
        let was_selected = self.inner.tab_view.selected_page().as_ref() == Some(&page);
        self.inner.tab_view.set_selected_page(&page);
        if was_selected {
            self.on_active_tab_changed();
        }
        true
    }

    pub fn select_next_tab(&self) {
        let n = self.inner.tab_view.n_pages();
        if n <= 1 {
            return;
        }
        if !self.inner.tab_view.select_next_page() {
            if let Some(first) = self.inner.tab_view.nth_page(0) {
                self.inner.tab_view.set_selected_page(&first);
            }
        }
    }

    pub fn select_previous_tab(&self) {
        let n = self.inner.tab_view.n_pages();
        if n <= 1 {
            return;
        }
        if !self.inner.tab_view.select_previous_page() {
            if let Some(last) = self.inner.tab_view.nth_page(n - 1) {
                self.inner.tab_view.set_selected_page(&last);
            }
        }
    }

    /// `Ctrl+1..8` va a la pestaña 1..8 (1-indexed); `Ctrl+9` va siempre a la última pestaña.
    pub fn select_tab_shortcut(&self, one_based: usize) {
        let n = self.inner.tab_view.n_pages();
        if n <= 0 {
            return;
        }
        let idx = if one_based >= 9 {
            n - 1
        } else {
            ((one_based.saturating_sub(1)) as i32).min(n - 1)
        };
        if let Some(page) = self.inner.tab_view.nth_page(idx) {
            self.inner.tab_view.set_selected_page(&page);
        }
    }

    /// `Ctrl+W`: cierra la pestaña activa, **PERO NUNCA cierra una pestaña fijada**
    /// (igual que `Browser.close` en upstream: duerme la pestaña fijada y cambia a otra).
    pub fn close_active_tab(&self) {
        let Some(active_id) = self.selected_tab_id() else {
            return;
        };
        let is_pinned = self
            .inner
            .tabs
            .borrow()
            .iter()
            .find(|t| t.id == active_id)
            .map(|t| t.pin.is_some())
            .unwrap_or(false);

        if is_pinned {
            self.put_tab_to_sleep(active_id);
            // Cambiar a la primera pestaña no fijada, o a la siguiente si todas están fijadas.
            let next_target = {
                let tabs = self.inner.tabs.borrow();
                tabs.iter()
                    .find(|t| t.id != active_id && t.pin.is_none())
                    .or_else(|| tabs.iter().find(|t| t.id != active_id))
                    .map(|t| t.id)
            };
            if let Some(other_id) = next_target {
                self.select_tab_by_id(other_id);
            }
            return;
        }

        let _ = self.close_tab_by_id(active_id);
    }

    /// Cierra una pestaña no fijada concreta por su ID. Devuelve `true` si se cerró.
    pub fn close_tab_by_id(&self, tab_id: u64) -> bool {
        let page_opt = {
            let tabs = self.inner.tabs.borrow();
            tabs.iter()
                .find(|t| t.id == tab_id && t.pin.is_none())
                .map(|t| t.page.clone())
        };
        let Some(page) = page_opt else {
            return false;
        };
        self.inner.tab_view.close_page(&page);
        true
    }

    pub fn close_other_tabs(&self, keep_tab_id: u64) {
        let to_close: Vec<TabPage> = self
            .inner
            .tabs
            .borrow()
            .iter()
            .filter(|t| t.id != keep_tab_id && t.pin.is_none())
            .map(|t| t.page.clone())
            .collect();
        for page in to_close {
            self.inner.tab_view.close_page(&page);
        }
    }

    pub fn duplicate_tab(&self, tab_id: u64) {
        let spec_and_pos = {
            self.sync_order_with_tab_view();
            let tabs = self.inner.tabs.borrow();
            tabs.iter()
                .enumerate()
                .find(|(_, t)| t.id == tab_id)
                .map(|(idx, t)| {
                    let mut spec = t.to_session_tab();
                    // Una pestaña duplicada entra como pestaña normal justo después de la original.
                    spec.pin = None;
                    spec.home = None;
                    spec.touched = Some(now_unix_secs());
                    (spec, idx + 1)
                })
        };
        if let Some((spec, pos)) = spec_and_pos {
            let _ = self.insert_tab_from_spec(spec, Some(pos), true, false);
        }
    }

    pub fn duplicate_active_tab(&self) {
        if let Some(id) = self.selected_tab_id() {
            self.duplicate_tab(id);
        }
    }

    pub fn toggle_pin_tab(&self, tab_id: u64) {
        let target = {
            let mut tabs = self.inner.tabs.borrow_mut();
            let Some(t) = tabs.iter_mut().find(|t| t.id == tab_id) else {
                return;
            };
            let new_pinned = t.pin.is_none();
            if new_pinned {
                t.pin = Some(SessionTab::default_pin_label(&t.title, &t.url));
                if t.home.is_none() {
                    t.home = Some(t.url.clone());
                }
            } else {
                t.pin = None;
                t.home = None;
            }
            (t.page.clone(), new_pinned)
        };
        self.inner.tab_view.set_page_pinned(&target.0, target.1);
        self.sync_order_with_tab_view();
        self.sync_sidebar();
        self.notify_session_dirty();
    }

    pub fn toggle_mute_tab(&self, tab_id: u64) {
        let next_muted = {
            let mut tabs = self.inner.tabs.borrow_mut();
            let Some(t) = tabs.iter_mut().find(|t| t.id == tab_id) else {
                return;
            };
            let muted = if let Some(ref bv) = t.browser {
                bv.toggle_audio_muted()
            } else {
                !t.muted
            };
            t.muted = muted;
            Self::apply_page_audio_indicator(&t.page, t.audible, t.muted);
            muted
        };
        let _ = next_muted;
        self.sync_sidebar();
        self.notify_session_dirty();
    }

    pub fn toggle_mute_active_tab(&self) {
        if let Some(id) = self.selected_tab_id() {
            self.toggle_mute_tab(id);
        }
    }

    pub fn reload_tab(&self, tab_id: u64) {
        if let Some(bv) = self.ensure_tab_awake(tab_id) {
            bv.reload();
        }
    }

    pub fn move_tab_to_new_window(&self, tab_id: u64) {
        let spec_opt = {
            let tabs = self.inner.tabs.borrow();
            tabs.iter()
                .find(|t| t.id == tab_id)
                .map(TabRecord::to_session_tab)
        };
        let Some(mut spec) = spec_opt else {
            return;
        };
        // Si estaba fijada, desfijar antes de moverla para que pueda desprenderse limpiamente.
        if spec.pin.is_some() {
            self.toggle_pin_tab(tab_id);
            spec.pin = None;
            spec.home = None;
        }
        let is_private = self.inner.is_private;
        if let Some(ref cb) = self.inner.callbacks.on_move_tab_to_new_window {
            cb(spec, is_private);
        }
        if self.tab_count() > 1 {
            let _ = self.close_tab_by_id(tab_id);
        }
    }

    pub fn reopen_closed_tab(&self) {
        if let Some(ref cb) = self.inner.callbacks.on_reopen_closed_tab {
            cb(self.clone());
        }
    }

    pub fn request_new_window(&self) {
        if let Some(ref cb) = self.inner.callbacks.on_new_window {
            cb();
        }
    }

    pub fn request_new_private_window(&self) {
        if let Some(ref cb) = self.inner.callbacks.on_new_private_window {
            cb();
        }
    }

    pub fn open_tab_switcher(&self) {
        if let Some(ref cb) = self.inner.callbacks.on_open_tab_switcher {
            cb(self.clone());
        }
    }

    pub fn toggle_tab_layout(&self) {
        self.inner.settings.update(|s| {
            s.vertical_tabs = !s.vertical_tabs;
        });
    }

    pub fn toggle_fold_tabs(&self) {
        let next = !self.inner.folded.get();
        self.inner.folded.set(next);
        self.apply_layout_from_settings(&self.inner.settings.snapshot());
    }

    pub fn handle_escape(&self) {
        if self.inner.window.is_fullscreen() {
            self.set_fullscreen(false);
            return;
        }
        if self.inner.find_bar.is_open() {
            self.inner.find_bar.close();
            return;
        }
        if self.inner.address_bar.has_focus() {
            if let Some(bv) = self.active_browser_view() {
                bv.grab_focus();
            }
            return;
        }
        if let Some(bv) = self.active_browser_view() {
            if bv.is_loading() {
                bv.stop_load();
            }
        }
    }

    pub fn set_fullscreen(&self, fullscreen: bool) {
        self.inner.window.set_fullscreen(fullscreen);
        self.inner.toolbar_view.set_reveal_top_bars(!fullscreen);
        if fullscreen {
            self.inner.sidebar_revealer.set_reveal_child(false);
        } else {
            self.apply_layout_from_settings(&self.inner.settings.snapshot());
        }
    }

    pub fn toggle_fullscreen(&self) {
        self.set_fullscreen(!self.inner.window.is_fullscreen());
    }

    pub fn zoom_in(&self) {
        if let Some(bv) = self.ensure_active_browser_view() {
            bv.zoom_in();
            let z = bv.zoom_factor();
            self.inner.settings.update(|s| s.page_zoom = z);
        }
    }

    pub fn zoom_out(&self) {
        if let Some(bv) = self.ensure_active_browser_view() {
            bv.zoom_out();
            let z = bv.zoom_factor();
            self.inner.settings.update(|s| s.page_zoom = z);
        }
    }

    pub fn zoom_reset(&self) {
        if let Some(bv) = self.ensure_active_browser_view() {
            bv.zoom_reset();
            self.inner.settings.update(|s| s.page_zoom = 1.0);
        }
    }

    /// Cierra limpiamente todos los `cef::Browser` activos de esta ventana (p. ej. al salir).
    pub fn close_all_browsers(&self) {
        if let Some(pop) = self.inner.active_popover.borrow_mut().take() {
            pop.unparent();
        }
        let mut tabs = self.inner.tabs.borrow_mut();
        for tab in tabs.iter_mut() {
            if let Some(bv) = tab.browser.take() {
                bv.close_browser();
            }
        }
    }

    fn selected_tab_id(&self) -> Option<u64> {
        let sel = self.inner.tab_view.selected_page()?;
        self.inner
            .tabs
            .borrow()
            .iter()
            .find(|t| t.page == sel)
            .map(|t| t.id)
    }

    fn tab_id_for_page(&self, page: &TabPage) -> Option<u64> {
        self.inner
            .tabs
            .borrow()
            .iter()
            .find(|t| &t.page == page)
            .map(|t| t.id)
    }

    fn put_tab_to_sleep(&self, tab_id: u64) {
        let mut tabs = self.inner.tabs.borrow_mut();
        let Some(tab) = tabs.iter_mut().find(|t| t.id == tab_id) else {
            return;
        };
        if let Some(bv) = tab.browser.take() {
            tab.slot.remove(bv.widget());
            bv.close_browser();
        }
        tab.loading = false;
        tab.audible = false;
        tab.page.set_loading(false);
        Self::apply_page_audio_indicator(&tab.page, false, tab.muted);
    }

    fn ensure_tab_awake(&self, tab_id: u64) -> Option<BrowserView> {
        let existing_or_spec = {
            let tabs = self.inner.tabs.borrow();
            let tab = tabs.iter().find(|t| t.id == tab_id)?;
            if let Some(ref bv) = tab.browser {
                return Some(bv.clone());
            }
            (tab.url.clone(), tab.muted, tab.slot.clone())
        };

        let (url, muted, slot) = existing_or_spec;
        let bv = match self.create_browser_view_for_tab(tab_id, &url, muted) {
            Ok(v) => v,
            Err(err) => {
                search_core::error!("No se pudo despertar la pestaña {tab_id}: {err:#}");
                return None;
            }
        };

        slot.append(bv.widget());
        {
            let mut tabs = self.inner.tabs.borrow_mut();
            if let Some(tab) = tabs.iter_mut().find(|t| t.id == tab_id) {
                tab.browser = Some(bv.clone());
            }
        }
        Some(bv)
    }

    fn create_browser_view_for_tab(
        &self,
        tab_id: u64,
        url: &str,
        muted: bool,
    ) -> Result<BrowserView> {
        let is_dark = self.inner.app.style_manager().is_dark();
        let bv = BrowserView::new(
            self.inner.request_context.clone(),
            self.inner.settings.clone(),
            url,
            is_dark,
        )?;
        if muted {
            bv.set_audio_muted(true);
        }

        let mut cbs = BrowserCallbacks::default();
        {
            let w = self.clone();
            cbs.on_title_changed = Some(Box::new(move |title| {
                w.handle_tab_title_changed(tab_id, title);
            }));
        }
        {
            let w = self.clone();
            cbs.on_address_changed = Some(Box::new(move |new_url| {
                w.handle_tab_address_changed(tab_id, new_url);
            }));
        }
        {
            let w = self.clone();
            cbs.on_loading_state_changed = Some(Box::new(move |is_loading, _back, _fwd| {
                w.handle_tab_loading_changed(tab_id, is_loading);
            }));
        }
        {
            let w = self.clone();
            cbs.on_progress_changed = Some(Box::new(move |progress| {
                w.handle_tab_progress_changed(tab_id, progress);
            }));
        }
        {
            let w = self.clone();
            cbs.on_favicon_png = Some(Box::new(move |png_bytes| {
                w.handle_tab_favicon_png(tab_id, png_bytes);
            }));
        }
        {
            let w = self.clone();
            cbs.on_audio_state_changed = Some(Box::new(move |audible, is_muted| {
                w.handle_tab_audio_changed(tab_id, audible, is_muted);
            }));
        }
        {
            let w = self.clone();
            cbs.on_open_new_tab = Some(Box::new(move |target_url, background| {
                let _ = w.open_tab(Some(&target_url), !background, false, false);
            }));
        }
        {
            let w = self.clone();
            cbs.on_fullscreen_changed = Some(Box::new(move |fullscreen| {
                w.set_fullscreen(fullscreen);
            }));
        }
        {
            let w = self.clone();
            cbs.on_find_result = Some(Box::new(move |ordinal, count| {
                if w.selected_tab_id() == Some(tab_id) {
                    w.inner.find_bar.set_match_status(ordinal, count);
                }
            }));
        }

        bv.set_callbacks(cbs);
        Ok(bv)
    }

    fn handle_tab_title_changed(&self, tab_id: u64, title: String) {
        let is_selected = self.selected_tab_id() == Some(tab_id);
        let updated_title = {
            let mut tabs = self.inner.tabs.borrow_mut();
            let Some(tab) = tabs.iter_mut().find(|t| t.id == tab_id) else {
                return;
            };
            tab.title = title;
            let disp = tab.display_title();
            let tip = tab.tooltip_text();
            tab.page.set_title(&disp);
            tab.page.set_tooltip(Some(&tip));
            disp
        };
        if is_selected {
            self.update_window_title(&updated_title);
        }
        self.sync_sidebar();
        self.notify_session_dirty();
    }

    fn handle_tab_address_changed(&self, tab_id: u64, new_url: String) {
        let is_selected = self.selected_tab_id() == Some(tab_id);
        {
            let mut tabs = self.inner.tabs.borrow_mut();
            let Some(tab) = tabs.iter_mut().find(|t| t.id == tab_id) else {
                return;
            };
            tab.url = new_url.clone();
            tab.touched = now_unix_secs();
            let disp = tab.display_title();
            let tip = tab.tooltip_text();
            tab.page.set_title(&disp);
            tab.page.set_tooltip(Some(&tip));
        }
        if is_selected {
            self.inner.address_bar.set_committed_url(&new_url);
        }
        self.sync_sidebar();
        self.notify_session_dirty();
    }

    fn handle_tab_loading_changed(&self, tab_id: u64, is_loading: bool) {
        let is_selected = self.selected_tab_id() == Some(tab_id);
        {
            let mut tabs = self.inner.tabs.borrow_mut();
            let Some(tab) = tabs.iter_mut().find(|t| t.id == tab_id) else {
                return;
            };
            tab.loading = is_loading;
            tab.progress = if is_loading { 0.08 } else { 1.0 };
            tab.page.set_loading(is_loading);
        }
        if is_selected {
            if is_loading {
                self.inner.progress_bar.set_fraction(0.08);
                self.inner.progress_bar.set_visible(true);
            } else {
                self.inner.progress_bar.set_fraction(1.0);
                self.inner.progress_bar.set_visible(false);
            }
        }
        self.sync_sidebar();
    }

    fn handle_tab_progress_changed(&self, tab_id: u64, progress: f64) {
        let is_selected = self.selected_tab_id() == Some(tab_id);
        let is_loading = {
            let mut tabs = self.inner.tabs.borrow_mut();
            let Some(tab) = tabs.iter_mut().find(|t| t.id == tab_id) else {
                return;
            };
            tab.progress = progress;
            tab.loading
        };
        if is_selected {
            if is_loading && progress < 1.0 {
                self.inner.progress_bar.set_fraction(progress.max(0.06));
                self.inner.progress_bar.set_visible(true);
            } else {
                self.inner.progress_bar.set_visible(false);
            }
        }
    }

    fn handle_tab_favicon_png(&self, tab_id: u64, png_bytes: Vec<u8>) {
        let gbytes = Bytes::from_slice(&png_bytes);
        let Some(tex) = Texture::from_bytes(&gbytes) else {
            return;
        };
        let gicon = Icon::from_bytes(&gbytes);
        {
            let mut tabs = self.inner.tabs.borrow_mut();
            let Some(tab) = tabs.iter_mut().find(|t| t.id == tab_id) else {
                return;
            };
            tab.favicon_texture = Some(tex);
            tab.page.set_icon(Some(&gicon));
        }
        self.sync_sidebar();
    }

    fn handle_tab_audio_changed(&self, tab_id: u64, audible: bool, muted: bool) {
        {
            let mut tabs = self.inner.tabs.borrow_mut();
            let Some(tab) = tabs.iter_mut().find(|t| t.id == tab_id) else {
                return;
            };
            tab.audible = audible;
            tab.muted = muted;
            Self::apply_page_audio_indicator(&tab.page, audible, muted);
        }
        self.sync_sidebar();
    }

    fn apply_page_audio_indicator(page: &TabPage, audible: bool, muted: bool) {
        if muted {
            let icon = Icon::themed("audio-volume-muted-symbolic");
            page.set_indicator_icon(Some(&icon));
            page.set_indicator_tooltip(Some("Activar sonido de la pestaña"));
            page.set_indicator_activatable(true);
        } else if audible {
            let icon = Icon::themed("audio-volume-high-symbolic");
            page.set_indicator_icon(Some(&icon));
            page.set_indicator_tooltip(Some("Silenciar pestaña"));
            page.set_indicator_activatable(true);
        } else {
            page.set_indicator_icon(None);
            page.set_indicator_tooltip(None);
            page.set_indicator_activatable(false);
        }
    }

    fn on_active_tab_changed(&self) {
        if self.inner.suppress_auto_wake.get() {
            return;
        }
        let Some(active_id) = self.selected_tab_id() else {
            return;
        };

        // Despertar de forma diferida si esta pestaña seguía dormida.
        let _ = self.ensure_tab_awake(active_id);

        let info = {
            let mut tabs = self.inner.tabs.borrow_mut();
            let Some(tab) = tabs.iter_mut().find(|t| t.id == active_id) else {
                return;
            };
            tab.touched = now_unix_secs();
            (
                tab.url.clone(),
                tab.display_title(),
                tab.loading,
                tab.progress,
            )
        };

        let (url, title, loading, progress) = info;
        self.inner.address_bar.set_committed_url(&url);
        self.update_window_title(&title);

        if loading && progress < 1.0 {
            self.inner.progress_bar.set_fraction(progress.max(0.06));
            self.inner.progress_bar.set_visible(true);
        } else {
            self.inner.progress_bar.set_visible(false);
        }

        if self.inner.find_bar.is_open() {
            self.inner.find_bar.close();
        }

        self.sync_sidebar();
        self.notify_session_dirty();
    }

    fn update_window_title(&self, page_title: &str) {
        let clean = page_title.trim();
        let full = if self.inner.is_private {
            if clean.is_empty() || clean == "Nueva pestaña" {
                "Search — Privada".to_string()
            } else {
                format!("{clean} — Privada")
            }
        } else if clean.is_empty() || clean == "Nueva pestaña" {
            "Search".to_string()
        } else {
            clean.to_string()
        };
        self.inner.window.set_title(&full);
    }

    fn apply_layout_from_settings(&self, s: &search_core::settings::AppSettings) {
        let folded = self.inner.folded.get();
        let fullscreen = self.inner.window.is_fullscreen();
        self.inner
            .sidebar_tabs
            .set_width(s.sidebar_width.round() as i32);

        if s.vertical_tabs {
            self.inner.top_bar_revealer.set_reveal_child(false);
            self.inner.top_bar_revealer.set_visible(false);
            self.inner.sidebar_revealer.set_visible(true);
            self.inner
                .sidebar_revealer
                .set_reveal_child(!folded && !fullscreen);
            self.sync_sidebar();
        } else {
            self.inner.sidebar_revealer.set_reveal_child(false);
            self.inner.sidebar_revealer.set_visible(false);
            self.inner.top_bar_revealer.set_visible(true);
            self.inner
                .top_bar_revealer
                .set_reveal_child(!folded && !fullscreen);
        }
    }

    fn sync_order_with_tab_view(&self) {
        let tv = &self.inner.tab_view;
        let mut tabs = self.inner.tabs.borrow_mut();
        tabs.sort_by_key(|t| {
            let pos = tv.page_position(&t.page);
            if pos < 0 {
                i32::MAX
            } else {
                pos
            }
        });
    }

    fn sync_sidebar(&self) {
        if !self.inner.settings.with(|s| s.vertical_tabs) {
            return;
        }
        self.sync_order_with_tab_view();
        let selected_page = self.inner.tab_view.selected_page();
        let items: Vec<SidebarTabItem> = self
            .inner
            .tabs
            .borrow()
            .iter()
            .map(|t| SidebarTabItem {
                id: t.id,
                title: t.display_title(),
                tooltip: t.tooltip_text(),
                pin_label: t.pin.clone(),
                pinned: t.pin.is_some(),
                selected: selected_page.as_ref() == Some(&t.page),
                loading: t.loading,
                audible: t.audible,
                muted: t.muted,
                favicon: t.favicon_texture.clone(),
            })
            .collect();
        self.inner.sidebar_tabs.sync(&items);
    }

    fn populate_tab_context_menu(&self, tab_id: u64) {
        self.inner.menu_target_tab.set(Some(tab_id));
        let (is_pinned, is_muted) = {
            let tabs = self.inner.tabs.borrow();
            match tabs.iter().find(|t| t.id == tab_id) {
                Some(t) => (t.pin.is_some(), t.muted),
                None => (false, false),
            }
        };

        self.inner.tab_close_action.set_enabled(!is_pinned);
        self.inner.tab_menu.remove_all();

        let sec1 = Menu::new();
        sec1.append("Recargar", Some("tab.reload"));
        sec1.append("Duplicar", Some("tab.duplicate"));
        sec1.append(
            if is_pinned {
                "Desfijar pestaña"
            } else {
                "Fijar pestaña"
            },
            Some("tab.toggle-pin"),
        );
        sec1.append(
            if is_muted {
                "Activar sonido"
            } else {
                "Silenciar pestaña"
            },
            Some("tab.toggle-mute"),
        );
        self.inner.tab_menu.append_section(None, &sec1);

        let sec2 = Menu::new();
        sec2.append("Mover a ventana nueva", Some("tab.move-to-new-window"));
        self.inner.tab_menu.append_section(None, &sec2);

        let sec3 = Menu::new();
        sec3.append("Cerrar pestaña", Some("tab.close"));
        sec3.append("Cerrar las demás pestañas", Some("tab.close-others"));
        self.inner.tab_menu.append_section(None, &sec3);
    }

    fn install_tab_context_actions(&self) {
        let group = SimpleActionGroup::new();

        let add = |name: &str, cb: Box<dyn Fn()>| {
            let act = SimpleAction::new(name);
            act.connect_activate(cb);
            group.add_action(&act);
        };

        {
            let w = self.clone();
            add(
                "reload",
                Box::new(move || {
                    if let Some(id) = w.inner.menu_target_tab.get() {
                        w.reload_tab(id);
                    }
                }),
            );
        }
        {
            let w = self.clone();
            add(
                "duplicate",
                Box::new(move || {
                    if let Some(id) = w.inner.menu_target_tab.get() {
                        w.duplicate_tab(id);
                    }
                }),
            );
        }
        {
            let w = self.clone();
            add(
                "toggle-pin",
                Box::new(move || {
                    if let Some(id) = w.inner.menu_target_tab.get() {
                        w.toggle_pin_tab(id);
                    }
                }),
            );
        }
        {
            let w = self.clone();
            add(
                "toggle-mute",
                Box::new(move || {
                    if let Some(id) = w.inner.menu_target_tab.get() {
                        w.toggle_mute_tab(id);
                    }
                }),
            );
        }
        {
            let w = self.clone();
            add(
                "move-to-new-window",
                Box::new(move || {
                    if let Some(id) = w.inner.menu_target_tab.get() {
                        w.move_tab_to_new_window(id);
                    }
                }),
            );
        }
        {
            let w = self.clone();
            self.inner.tab_close_action.connect_activate(move || {
                if let Some(id) = w.inner.menu_target_tab.get() {
                    let _ = w.close_tab_by_id(id);
                }
            });
            group.add_action(&self.inner.tab_close_action);
        }
        {
            let w = self.clone();
            add(
                "close-others",
                Box::new(move || {
                    if let Some(id) = w.inner.menu_target_tab.get() {
                        w.close_other_tabs(id);
                    }
                }),
            );
        }

        self.inner.window.insert_action_group("tab", Some(&group));
    }

    fn wire_signals(&self) {
        // Barra de direcciones: envío y tecla Escape.
        {
            let w = self.clone();
            self.inner.address_bar.connect_submit(move |typed| {
                w.navigate_typed_input(&typed);
            });
        }
        {
            let w = self.clone();
            self.inner.address_bar.connect_escape(move || {
                if let Some(bv) = w.active_browser_view() {
                    bv.grab_focus();
                }
            });
        }

        // Barra de búsqueda en página.
        {
            let w = self.clone();
            self.inner
                .find_bar
                .connect_find(move |query, forward, find_next| {
                    if let Some(bv) = w.ensure_active_browser_view() {
                        bv.find(&query, forward, false, find_next);
                    }
                });
        }
        {
            let w = self.clone();
            self.inner.find_bar.connect_close(move || {
                if let Some(bv) = w.active_browser_view() {
                    bv.stop_finding(true);
                    bv.grab_focus();
                }
            });
        }

        // Señales del modelo único `AdwTabView`.
        {
            let w = self.clone();
            self.inner.tab_view.connect_selected_page_notify(move || {
                w.on_active_tab_changed();
            });
        }
        {
            let w = self.clone();
            self.inner
                .tab_view
                .connect_page_reordered(move |_page, _pos| {
                    w.sync_order_with_tab_view();
                    w.sync_sidebar();
                    w.notify_session_dirty();
                });
        }
        {
            let w = self.clone();
            self.inner
                .tab_view
                .connect_indicator_activated(move |page| {
                    if let Some(id) = w.tab_id_for_page(&page) {
                        w.toggle_mute_tab(id);
                    }
                });
        }
        {
            let w = self.clone();
            self.inner.tab_view.connect_setup_menu(move |opt_page| {
                if let Some(page) = opt_page {
                    if let Some(id) = w.tab_id_for_page(&page) {
                        w.populate_tab_context_menu(id);
                    }
                }
            });
        }
        {
            let w = self.clone();
            self.inner.tab_view.connect_close_page(move |page| {
                // Proteger siempre las pestañas fijadas contra cierre (clic central o botón).
                if page.is_pinned() {
                    w.inner.tab_view.close_page_finish(&page, false);
                    return true;
                }

                let pos = w.inner.tab_view.page_position(&page).max(0) as usize;
                let removed = {
                    let mut tabs = w.inner.tabs.borrow_mut();
                    tabs.iter()
                        .position(|t| t.page == page)
                        .map(|idx| tabs.remove(idx))
                };

                if let Some(mut rec) = removed {
                    let closed_meta = rec.to_closed_tab(pos);
                    if let Some(bv) = rec.browser.take() {
                        bv.close_browser();
                    }
                    if !w.inner.is_private
                        && !closed_meta.url.is_empty()
                        && closed_meta.url != "about:blank"
                    {
                        if let Some(ref cb) = w.inner.callbacks.on_tab_closed {
                            cb(closed_meta);
                        }
                    }
                }

                w.inner.tab_view.close_page_finish(&page, true);
                w.sync_sidebar();
                w.notify_session_dirty();

                if w.inner.tab_view.n_pages() == 0 && !w.inner.closing_window.get() {
                    let win_close = w.clone();
                    glib::idle_add_local_once(move || {
                        win_close.inner.window.close();
                    });
                }
                true
            });
        }

        // Callbacks de la barra lateral vertical (`SidebarTabs`).
        let mut scbs = SidebarCallbacks::default();
        {
            let w = self.clone();
            scbs.on_select = Some(Box::new(move |id| {
                w.select_tab_by_id(id);
            }));
        }
        {
            let w = self.clone();
            scbs.on_close = Some(Box::new(move |id| {
                let _ = w.close_tab_by_id(id);
            }));
        }
        {
            let w = self.clone();
            scbs.on_toggle_mute = Some(Box::new(move |id| {
                w.toggle_mute_tab(id);
            }));
        }
        {
            let w = self.clone();
            scbs.on_context_menu = Some(Box::new(move |id, anchor, x, y| {
                w.populate_tab_context_menu(id);
                if let Some(old) = w.inner.active_popover.borrow_mut().take() {
                    old.unparent();
                }
                let pop = PopoverMenu::from_model(Some(&w.inner.tab_menu));
                pop.set_parent(&anchor);
                pop.set_has_arrow(false);
                pop.set_position(PositionType::Right);
                pop.set_pointing_to(Some(Rectangle {
                    x: x.round() as i32,
                    y: y.round() as i32,
                    width: 1,
                    height: 1,
                }));
                pop.popup();
                *w.inner.active_popover.borrow_mut() = Some(pop);
            }));
        }
        {
            let w = self.clone();
            scbs.on_reorder = Some(Box::new(move |src_id, dst_id| {
                let pages = {
                    let tabs = w.inner.tabs.borrow();
                    let src = tabs
                        .iter()
                        .find(|t| t.id == src_id)
                        .map(|t| (t.page.clone(), t.pin.is_some()));
                    let dst = tabs
                        .iter()
                        .find(|t| t.id == dst_id)
                        .map(|t| (t.page.clone(), t.pin.is_some()));
                    (src, dst)
                };
                if let (Some((src_page, src_pin)), Some((dst_page, dst_pin))) = pages {
                    if src_pin == dst_pin {
                        let dst_pos = w.inner.tab_view.page_position(&dst_page);
                        if dst_pos >= 0 {
                            w.inner.tab_view.reorder_page(&src_page, dst_pos);
                        }
                    }
                }
            }));
        }
        {
            let w = self.clone();
            scbs.on_new_tab = Some(Box::new(move || {
                let _ = w.open_tab(None, true, false, false);
            }));
        }
        self.inner.sidebar_tabs.set_callbacks(scbs);

        // Al cerrarse la ventana: avisar al controlador (guarda sesión antes de destruir) y cerrar CEF.
        {
            let w = self.clone();
            self.inner.window.connect_close_request(move || {
                w.inner.closing_window.set(true);
                let win_id = w.inner.id;
                if let Some(ref cb) = w.inner.callbacks.on_window_closed {
                    cb(win_id);
                }
                w.close_all_browsers();
                false
            });
        }
    }

    fn notify_session_dirty(&self) {
        if self.inner.is_private || self.inner.suppress_auto_wake.get() {
            return;
        }
        if let Some(ref cb) = self.inner.callbacks.on_session_dirty {
            cb();
        }
    }
}
