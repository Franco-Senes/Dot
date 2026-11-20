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
use crate::bookmarks_ui::{self, BookmarksManager, BookmarksPanel};
use crate::boosts_ui;
use crate::browser_view::{BrowserCallbacks, BrowserView, PageContextMenuInfo};
use crate::curtain::{CurtainManager, CurtainPanel, CurtainPanelCallbacks};
use crate::downloads::DownloadManager;
use crate::downloads_ui::DownloadsPanel;
use crate::find_bar::FindBar;
use crate::history_ui::{HistoryManager, HistoryPanel};
use crate::icons::{self, IconId};
use crate::peek;
use crate::pip::PipController;
use crate::reader::ReaderToolbar;
use crate::settings::SettingsStore;
use crate::settings_page::SettingsPage;
use crate::shield::{self, ShieldManager};
use crate::sidebar_tabs::{
    PinFoldersManager, SidebarCallbacks, SidebarRenderOptions, SidebarTabItem, SidebarTabs,
};
use crate::split_view::{SplitViewCallbacks, SplitViewContainer};
use crate::tab_switcher::SwitcherTabItem;
use crate::vault::VaultService;
use crate::vault_ui::{self, VaultWindowUi};
use anyhow::Result;
use cef::RequestContext;
use dot_core::address::{
    canonical_internal_url, display_at_rest, parse_internal_page, resolve_address,
    sanitize_url_for_ui, InternalPage,
};
use dot_core::boosts::{compile_boost_css, normalize_origin, BoostStore};
use dot_core::history::{suggest_omnibox, OpenTabInfo};
use dot_core::newtab::{
    default_shortcuts, render_newtab_html, NewTabData, NewTabTheme, WorkspacePill,
};
use dot_core::paths::DotPaths;
use dot_core::reader::render_reader_document;
use dot_core::session::{now_unix_secs, ClosedTab, PinStyle, SessionTab, SessionWindow};
use dot_core::settings::{AppSettings, NewTabPosition, UiMotion};
use dot_core::split::TabSplit;
use gnome_platform::adw::{
    Application, ApplicationWindow, CenteringPolicy, HeaderBar, OverlaySplitView, TabBar, TabPage,
    TabView, TabViewShortcuts, Toast, ToastOverlay, ToolbarStyle, ToolbarView,
};
use gnome_platform::gdk::{Display, Rectangle, Texture};
use gnome_platform::gio::{Menu, SimpleAction, SimpleActionGroup};
use gnome_platform::glib;
use gnome_platform::gtk::{
    Align, AsWidget, BoxWidget, Button, CssProvider, FileDialog, GestureDrag, Label, Orientation,
    Overflow, Overlay, PackType, Popover, PopoverMenu, PositionType, ProgressBar, Revealer,
    RevealerTransitionType, STYLE_PROVIDER_PRIORITY_APPLICATION,
};
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_WINDOW_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_TAB_ID: AtomicU64 = AtomicU64::new(1);

/// Almacenes compartidos de datos de navegación (Fases 3, 3b y 4) inyectados desde `AppController`.
#[derive(Clone)]
pub struct SharedDataStores {
    pub paths: DotPaths,
    pub history: HistoryManager,
    pub bookmarks: BookmarksManager,
    pub downloads: DownloadManager,
    pub pin_folders: PinFoldersManager,
    pub shield: ShieldManager,
    pub curtain: CurtainManager,
    pub vault: VaultService,
    pub boosts: Rc<RefCell<BoostStore>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SidePanelKind {
    None,
    History,
    Bookmarks,
    Downloads,
    Curtain,
}

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
    pub on_list_open_tabs: Option<Box<dyn Fn() -> Vec<OpenTabInfo>>>,
    pub on_switch_to_tab: Option<Box<dyn Fn(u64, u64)>>,
    pub on_is_any_window_active: Option<Box<dyn Fn() -> bool>>,
}

struct TabRecord {
    id: u64,
    page: TabPage,
    slot: BoxWidget,
    url: String,
    title: String,
    pin: Option<String>,
    pin_style: Option<PinStyle>,
    folder_id: Option<String>,
    home: Option<String>,
    touched: f64,
    loading: bool,
    progress: f64,
    audible: bool,
    muted: bool,
    favicon_texture: Option<Texture>,
    browser: Option<BrowserView>,
    settings_page: Option<SettingsPage>,
}

impl TabRecord {
    fn is_settings_tab(&self) -> bool {
        matches!(
            parse_internal_page(&self.url),
            Some(InternalPage::Settings { .. })
        )
    }

    fn effective_pin_style(&self) -> Option<PinStyle> {
        self.pin.as_ref()?;
        if self.folder_id.is_some() {
            return Some(PinStyle::Row);
        }
        Some(self.pin_style.unwrap_or(PinStyle::Square))
    }

    fn display_title(&self) -> String {
        self.display_title_for(false)
    }

    fn display_title_for(&self, show_domain: bool) -> String {
        if self.is_settings_tab() {
            return "Ajustes".to_string();
        }
        if show_domain {
            let rest = display_at_rest(&self.url);
            if !rest.is_empty() {
                return rest;
            }
        }
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
            pin_style: self.effective_pin_style(),
            folder_id: self.folder_id.clone(),
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
    main_hbox: BoxWidget,
    address_bar: AddressBar,
    find_bar: FindBar,
    progress_bar: ProgressBar,
    tab_view: TabView,
    top_bar_revealer: Revealer,
    sidebar_tabs: SidebarTabs,
    sidebar_revealer: Revealer,
    side_split: OverlaySplitView,
    active_panel: Cell<SidePanelKind>,
    history_panel: RefCell<Option<HistoryPanel>>,
    bookmarks_panel: RefCell<Option<BookmarksPanel>>,
    downloads_panel: RefCell<Option<DownloadsPanel>>,
    curtain_panel: RefCell<Option<CurtainPanel>>,
    reader_toolbar: RefCell<Option<ReaderToolbar>>,
    toast_overlay: ToastOverlay,
    active_toast: RefCell<Option<Toast>>,
    space_accent_provider: CssProvider,
    bookmark_popover: RefCell<Option<Popover>>,
    site_card_popover: RefCell<Option<Popover>>,
    vault_ui: VaultWindowUi,
    tab_menu: Menu,
    tab_close_action: SimpleAction,
    active_popover: RefCell<Option<PopoverMenu>>,
    menu_target_tab: Cell<Option<u64>>,
    request_context: RequestContext,
    settings: SettingsStore,
    stores: SharedDataStores,
    tabs: RefCell<Vec<TabRecord>>,
    splits: RefCell<Vec<TabSplit>>,
    next_split_id: Cell<u64>,
    split_view: SplitViewContainer,
    pip: PipController,
    page_context_popover: RefCell<Option<Popover>>,
    folded: Cell<bool>,
    suppress_auto_wake: Cell<bool>,
    closing_window: Cell<bool>,
    settings_sub_id: Cell<Option<u64>>,
    pin_folders_sub_id: Cell<Option<u64>>,
    callbacks: WindowCallbacks,
}

#[derive(Clone)]
pub struct BrowserWindow {
    inner: Rc<WindowState>,
}

#[derive(Clone)]
pub struct WeakBrowserWindow {
    inner: Weak<WindowState>,
}

impl WeakBrowserWindow {
    pub fn upgrade(&self) -> Option<BrowserWindow> {
        self.inner.upgrade().map(|inner| BrowserWindow { inner })
    }
}

impl BrowserWindow {
    pub fn downgrade(&self) -> WeakBrowserWindow {
        WeakBrowserWindow {
            inner: Rc::downgrade(&self.inner),
        }
    }
    pub fn new(
        app: &Application,
        request_context: RequestContext,
        settings: SettingsStore,
        stores: SharedDataStores,
        is_private: bool,
        callbacks: WindowCallbacks,
    ) -> Result<Self> {
        let id = NEXT_WINDOW_ID.fetch_add(1, Ordering::Relaxed);
        let window = ApplicationWindow::new(app);
        window.set_title(if is_private { "Dot — Privada" } else { "Dot" });
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
        let dark_now = app.style_manager().is_dark();
        let tab_view = TabView::new();
        tab_view.set_hexpand(true);
        tab_view.set_vexpand(true);
        tab_view.set_default_icon(&icons::gicon(IconId::Globe, dark_now));
        // Desactivamos los atajos internos de AdwTabView para usar nuestras acciones `win.*`
        // con comportamiento uniforme (Ctrl+Tab cíclico, Ctrl+1..9, etc.).
        tab_view.set_shortcuts(TabViewShortcuts::NONE);

        let tab_menu = Menu::new();
        tab_view.set_menu_model(Some(&tab_menu));

        let tab_bar = TabBar::new();
        tab_bar.set_view(Some(&tab_view));
        tab_bar.set_autohide(false);
        tab_bar.set_expand_tabs(true);

        let initial_motion = settings.with(|s| s.ui_motion);

        let top_bar_revealer = Revealer::new();
        top_bar_revealer.set_transition_type(RevealerTransitionType::SlideDown);
        top_bar_revealer.set_transition_duration(initial_motion.duration_ms(180));
        top_bar_revealer.set_child(Some(&tab_bar));
        toolbar_view.add_top_bar(&top_bar_revealer);

        // Barra lateral vertical de pestañas estilo Arc enlazada al mismo `AdwTabView`.
        let initial_sidebar_width = settings.with(|s| s.sidebar_width.round() as i32);
        let sidebar_tabs = SidebarTabs::new(initial_sidebar_width);

        // Borde derecho arrastrable para redimensionar la barra lateral sin salir del `Revealer`.
        let sidebar_resize_handle = BoxWidget::new(Orientation::Vertical, 0);
        sidebar_resize_handle.add_css_class("search-sidebar-resize-handle");
        sidebar_resize_handle.set_size_request(6, -1);
        sidebar_resize_handle.set_halign(Align::End);
        sidebar_resize_handle.set_vexpand(true);
        sidebar_resize_handle.set_cursor_from_name(Some("col-resize"));

        let sidebar_overlay = Overlay::new();
        sidebar_overlay.set_child(Some(sidebar_tabs.widget()));
        sidebar_overlay.add_overlay(&sidebar_resize_handle);
        sidebar_overlay.set_measure_overlay(&sidebar_resize_handle, false);

        let sidebar_revealer = Revealer::new();
        sidebar_revealer.set_transition_type(RevealerTransitionType::SlideRight);
        sidebar_revealer.set_transition_duration(initial_motion.duration_ms(220));
        sidebar_revealer.set_child(Some(&sidebar_overlay));

        let find_bar = FindBar::new();
        find_bar.set_motion_duration(initial_motion.duration_ms(150));

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

        // Barra discreta de Vault «¿Guardar la contraseña…?» flotando sobre la página.
        let vault_ui = VaultWindowUi::new();
        stage_overlay.add_overlay(vault_ui.offer_widget());
        stage_overlay.set_measure_overlay(vault_ui.offer_widget(), false);

        // Marco de tarjeta redondeado (`layout_cards` estilo Arc / Search) con recorte de esquinas.
        let stage_card = BoxWidget::new(Orientation::Vertical, 0);
        stage_card.add_css_class("search-stage-card");
        stage_card.set_hexpand(true);
        stage_card.set_vexpand(true);
        stage_card.set_overflow(Overflow::Hidden);
        stage_card.append(find_bar.widget());
        stage_card.append(&stage_overlay);

        let stage_shell = BoxWidget::new(Orientation::Vertical, 0);
        stage_shell.add_css_class("search-stage-shell");
        stage_shell.set_hexpand(true);
        stage_shell.set_vexpand(true);
        stage_shell.append(&stage_card);

        let main_hbox = BoxWidget::new(Orientation::Horizontal, 0);
        main_hbox.add_css_class("search-main-hbox");
        main_hbox.set_hexpand(true);
        main_hbox.set_vexpand(true);
        main_hbox.append(&sidebar_revealer);
        main_hbox.append(&stage_shell);

        // Redimensionado fluido de la barra lateral con ratón (`col-resize`) y guardado al soltar.
        {
            let drag = GestureDrag::new();
            drag.set_button(1);
            let drag_start_width = Rc::new(Cell::new(initial_sidebar_width));
            let drag_applied_width = Rc::new(Cell::new(initial_sidebar_width));

            {
                let tabs_ref = sidebar_tabs.clone();
                let handle_ref = sidebar_resize_handle.clone();
                let hbox_ref = main_hbox.clone();
                let start_w = drag_start_width.clone();
                let applied_w = drag_applied_width.clone();
                drag.connect_drag_begin(move |_x, _y| {
                    let w = tabs_ref.width().clamp(
                        AppSettings::SIDEBAR_WIDTH_MIN as i32,
                        AppSettings::SIDEBAR_WIDTH_MAX as i32,
                    );
                    start_w.set(w);
                    applied_w.set(w);
                    handle_ref.add_css_class("dragging");
                    hbox_ref.set_cursor_from_name(Some("col-resize"));
                });
            }
            {
                let tabs_ref = sidebar_tabs.clone();
                let start_w = drag_start_width.clone();
                let applied_w = drag_applied_width.clone();
                drag.connect_drag_update(move |dx, _dy| {
                    // Compensar el desplazamiento del propio handle conforme cambia el ancho.
                    let true_delta = dx + f64::from(applied_w.get() - start_w.get());
                    let next = (f64::from(start_w.get()) + true_delta).round() as i32;
                    let clamped = next.clamp(
                        AppSettings::SIDEBAR_WIDTH_MIN as i32,
                        AppSettings::SIDEBAR_WIDTH_MAX as i32,
                    );
                    if clamped != applied_w.get() {
                        applied_w.set(clamped);
                        tabs_ref.set_width(clamped);
                    }
                });
            }
            {
                let settings_ref = settings.clone();
                let handle_ref = sidebar_resize_handle.clone();
                let hbox_ref = main_hbox.clone();
                let applied_w = drag_applied_width.clone();
                drag.connect_drag_end(move |_dx, _dy| {
                    handle_ref.remove_css_class("dragging");
                    hbox_ref.set_cursor_from_name(None);
                    let final_w = f64::from(applied_w.get()).clamp(
                        AppSettings::SIDEBAR_WIDTH_MIN,
                        AppSettings::SIDEBAR_WIDTH_MAX,
                    );
                    let prev_w = settings_ref.with(|s| s.sidebar_width);
                    if (prev_w - final_w).abs() >= 0.5 {
                        settings_ref.update(|s| s.sidebar_width = final_w);
                    }
                });
            }

            sidebar_resize_handle.add_controller(&drag);
        }

        // Panel lateral discreto derecho (`AdwOverlaySplitView`) para Historial, Marcadores y Descargas.
        // Cero botones permanentes en la cabecera: solo se muestra bajo demanda con atajos y cierra con Esc.
        let side_split = OverlaySplitView::new();
        side_split.set_sidebar_position(PackType::End);
        side_split.set_collapsed(true);
        side_split.set_show_sidebar(false);
        side_split.set_pin_sidebar(false);
        side_split.set_enable_show_gesture(false);
        side_split.set_enable_hide_gesture(true);
        side_split.set_min_sidebar_width(340.0);
        side_split.set_max_sidebar_width(420.0);
        side_split.set_sidebar_width_fraction(0.32);
        side_split.set_content(Some(&main_hbox));

        toolbar_view.set_content(Some(&side_split));
        let toast_overlay = ToastOverlay::new();
        toast_overlay.set_child(Some(&toolbar_view));
        window.set_content(Some(&toast_overlay));

        let tab_close_action = SimpleAction::new("close");
        let space_accent_provider = CssProvider::new();
        if let Some(display) = Display::default_display() {
            space_accent_provider.add_to_display(&display, STYLE_PROVIDER_PRIORITY_APPLICATION + 2);
        }

        let bw = Self {
            inner: Rc::new(WindowState {
                id,
                is_private,
                app: app.clone(),
                window,
                toolbar_view,
                main_hbox,
                address_bar,
                find_bar,
                progress_bar,
                tab_view,
                top_bar_revealer,
                sidebar_tabs,
                sidebar_revealer,
                side_split,
                active_panel: Cell::new(SidePanelKind::None),
                history_panel: RefCell::new(None),
                bookmarks_panel: RefCell::new(None),
                downloads_panel: RefCell::new(None),
                curtain_panel: RefCell::new(None),
                reader_toolbar: RefCell::new(None),
                toast_overlay,
                active_toast: RefCell::new(None),
                space_accent_provider,
                bookmark_popover: RefCell::new(None),
                site_card_popover: RefCell::new(None),
                vault_ui,
                tab_menu,
                tab_close_action,
                active_popover: RefCell::new(None),
                menu_target_tab: Cell::new(None),
                request_context,
                settings: settings.clone(),
                stores,
                tabs: RefCell::new(Vec::new()),
                splits: RefCell::new(Vec::new()),
                next_split_id: Cell::new(1),
                split_view: SplitViewContainer::new(),
                pip: PipController::new(),
                page_context_popover: RefCell::new(None),
                folded: Cell::new(false),
                suppress_auto_wake: Cell::new(false),
                closing_window: Cell::new(false),
                settings_sub_id: Cell::new(None),
                pin_folders_sub_id: Cell::new(None),
                callbacks,
            }),
        };

        // Barra flotante discreta del modo lectura (solo visible cuando la pestaña está en lectura).
        let reader_toolbar = ReaderToolbar::new(
            settings.clone(),
            {
                let w = bw.clone();
                move |opts| {
                    if let Some(bv) = w.active_browser_view() {
                        bv.apply_reader_options(opts);
                    }
                }
            },
            {
                let w = bw.clone();
                move || {
                    if let Some(bv) = w.active_browser_view() {
                        bv.exit_reader_mode();
                    }
                }
            },
        );
        stage_overlay.add_overlay(reader_toolbar.widget());
        stage_overlay.set_measure_overlay(reader_toolbar.widget(), false);
        *bw.inner.reader_toolbar.borrow_mut() = Some(reader_toolbar);

        {
            let w_focus = bw.clone();
            let w_ratio = bw.clone();
            bw.inner.split_view.set_callbacks(SplitViewCallbacks {
                on_focus_pane: Some(Box::new(move |tab_id| {
                    w_focus.focus_split_pane(tab_id);
                })),
                on_ratio_changed: Some(Box::new(move |ratio| {
                    w_ratio.update_active_split_ratio(ratio);
                })),
            });
        }

        bw.init_side_panels();
        bw.install_tab_context_actions();
        bw.wire_signals();
        actions::install_window_actions(&bw.inner.window, &bw);
        bw.apply_layout_from_settings(&settings.snapshot());

        // Suscripción en vivo para aplicar cambios de disposición, tema, tarjetas y movimiento sin reiniciar.
        {
            let w_sub = bw.downgrade();
            let sub_id = settings.subscribe(move |updated| {
                if let Some(w) = w_sub.upgrade() {
                    w.apply_layout_from_settings(updated);
                }
            });
            bw.inner.settings_sub_id.set(Some(sub_id));
        }
        {
            let w_pf = bw.downgrade();
            let sub_id = bw.inner.stores.pin_folders.subscribe(move || {
                if let Some(w) = w_pf.upgrade() {
                    w.sync_sidebar();
                }
            });
            bw.inner.pin_folders_sub_id.set(Some(sub_id));
        }
        {
            let w_dark = bw.downgrade();
            bw.inner.app.style_manager().connect_dark_notify(move || {
                if let Some(w) = w_dark.upgrade() {
                    w.refresh_theme_icons();
                }
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

    #[allow(dead_code)]
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
        // Si la pestaña activa es `dot://settings`, `Ctrl+F` enfoca el buscador de ajustes.
        if let Some(sp) = self.active_settings_page() {
            sp.focus_search();
            return;
        }
        self.inner.find_bar.open_and_focus();
    }

    pub fn active_settings_page(&self) -> Option<SettingsPage> {
        let sel = self.inner.tab_view.selected_page()?;
        self.inner
            .tabs
            .borrow()
            .iter()
            .find(|t| t.page == sel)
            .and_then(|t| t.settings_page.clone())
    }

    pub fn is_active_tab_settings(&self) -> bool {
        self.active_settings_page().is_some()
    }

    pub fn active_tab_url(&self) -> String {
        let Some(sel) = self.inner.tab_view.selected_page() else {
            return String::new();
        };
        self.inner
            .tabs
            .borrow()
            .iter()
            .find(|t| t.page == sel)
            .map(|t| t.url.clone())
            .unwrap_or_default()
    }

    /// Abre o enfoca la pestaña interna `dot://settings` (`Ctrl+,` o botón lateral).
    pub fn open_settings_tab(&self, section: Option<&str>) {
        let existing = {
            let tabs = self.inner.tabs.borrow();
            tabs.iter()
                .find(|t| t.is_settings_tab())
                .map(|t| (t.id, t.settings_page.clone()))
        };
        if let Some((tab_id, sp_opt)) = existing {
            self.select_tab_by_id(tab_id);
            if let Some(sec) = section {
                if let Some(sp) = sp_opt.or_else(|| self.active_settings_page()) {
                    sp.select_section(sec);
                }
            }
            return;
        }
        let target_url = canonical_internal_url(&InternalPage::Settings {
            section: section.map(str::to_string),
        });
        let _ = self.open_tab(Some(&target_url), true, false, false);
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
        let pin_style = if pinned { Some(PinStyle::Square) } else { None };
        let home = if pinned { Some(url.clone()) } else { None };

        let tab_spec = SessionTab {
            url,
            title: String::new(),
            pin,
            pin_style,
            folder_id: None,
            name: None,
            home,
            group_id: None,
            touched: Some(now_unix_secs()),
            muted: false,
        };

        let pos = if !pinned
            && self.inner.settings.with(|s| s.new_tab_position) == NewTabPosition::NextToCurrent
        {
            self.inner
                .tab_view
                .selected_page()
                .map(|p| self.inner.tab_view.page_position(&p))
                .filter(|&p| p >= 0)
                .map(|p| (p as usize) + 1)
        } else {
            None
        };

        self.insert_tab_from_spec(tab_spec, pos, select, lazy)
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

        let eff_pin_style = spec.effective_pin_style();
        let folder_id = if is_pinned { spec.folder_id } else { None };
        let url_clean = if spec.url.trim().is_empty() {
            "about:blank".to_string()
        } else {
            spec.url
        };
        let internal_settings = match parse_internal_page(&url_clean) {
            Some(InternalPage::Settings { section }) => Some(section),
            _ => None,
        };

        let mut rec = TabRecord {
            id: tab_id,
            page: page.clone(),
            slot,
            url: url_clean,
            title: spec.title,
            pin: spec.pin,
            pin_style: eff_pin_style,
            folder_id,
            home: spec.home,
            touched: spec.touched.unwrap_or_else(now_unix_secs),
            loading: false,
            progress: 0.0,
            audible: false,
            muted: spec.muted,
            favicon_texture: None,
            browser: None,
            settings_page: None,
        };

        let show_domain = self.inner.settings.with(|s| s.tab_show_domain);
        let display_title = rec.display_title_for(show_domain);
        let tooltip = rec.tooltip_text();
        page.set_title(&display_title);
        page.set_tooltip(Some(&tooltip));
        let show_audio = self.inner.settings.with(|s| s.show_audio_indicators);
        Self::apply_page_audio_indicator(&page, rec.audible && show_audio, rec.muted && show_audio);

        let is_blank = rec.url == "about:blank";
        if let Some(sec_opt) = internal_settings {
            let dark = self.inner.app.style_manager().is_dark();
            page.set_icon(Some(&icons::gicon(IconId::Settings, dark)));
            let sp = SettingsPage::new(
                &self.inner.window,
                &self.inner.settings,
                &self.inner.stores.history,
                &self.inner.stores.downloads,
                &self.inner.stores.paths,
                sec_opt.as_deref(),
            );
            rec.slot.append(sp.widget());
            rec.settings_page = Some(sp);
        } else if !lazy {
            let is_newtab = matches!(parse_internal_page(&rec.url), Some(InternalPage::NewTab));
            let initial = if is_newtab {
                "about:blank"
            } else {
                rec.url.as_str()
            };
            let bv = self.create_browser_view_for_tab(tab_id, initial, rec.muted)?;
            if is_newtab {
                let html = self.render_newtab_page_html();
                bv.load_internal_html("dot://newtab", &html);
            }
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
    /// para todas las pestañas no activas cuando `lazy_restore` está activo.
    pub fn restore_from_session(&self, saved: &SessionWindow) -> Result<()> {
        if saved.tabs.is_empty() {
            self.open_tab(None, true, false, false)?;
            return Ok(());
        }

        let active_idx = saved.clamped_active();
        let lazy_restore = self.inner.settings.with(|s| s.lazy_restore);
        let prev_suppress = self.inner.suppress_auto_wake.replace(true);

        let mut created_ids = Vec::with_capacity(saved.tabs.len());
        for tab in &saved.tabs {
            let id = self.insert_tab_from_spec(tab.clone(), None, false, lazy_restore)?;
            created_ids.push(id);
        }

        let mut restored_splits = Vec::new();
        for saved_split in &saved.splits {
            let split_id = self.inner.next_split_id.get();
            self.inner.next_split_id.set(split_id.saturating_add(1));
            if let Some(sp) = TabSplit::from_session_split(split_id, saved_split, |idx| {
                created_ids.get(idx).copied()
            }) {
                restored_splits.push(sp);
            }
        }
        *self.inner.splits.borrow_mut() = restored_splits;

        self.inner.suppress_auto_wake.set(prev_suppress);

        let target_id = created_ids
            .get(active_idx)
            .copied()
            .or_else(|| created_ids.first().copied());
        if let Some(id) = target_id {
            self.select_tab_by_id(id);
        }
        self.sync_split_presentation();
        self.sync_sidebar();

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
        let splits = self
            .inner
            .splits
            .borrow()
            .iter()
            .filter_map(|sp| {
                sp.to_session_split(|tab_id| tabs_borrow.iter().position(|t| t.id == tab_id))
            })
            .collect();
        SessionWindow {
            tabs: out_tabs,
            active,
            sidebar: Some(self.inner.settings.with(|s| s.vertical_tabs)),
            groups: Vec::new(),
            splits,
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

    /// Instantánea de pestañas abiertas para sugerencias del omnibox ("Cambiar a pestaña").
    pub fn open_tab_infos(&self) -> Vec<OpenTabInfo> {
        self.sync_order_with_tab_view();
        self.inner
            .tabs
            .borrow()
            .iter()
            .map(|t| OpenTabInfo {
                window_id: self.inner.id,
                tab_id: t.id,
                title: t.display_title(),
                url: t.url.clone(),
                is_private: self.inner.is_private,
            })
            .collect()
    }

    pub fn toggle_history_panel(&self) {
        self.toggle_side_panel(SidePanelKind::History);
    }

    pub fn toggle_bookmarks_panel(&self) {
        self.toggle_side_panel(SidePanelKind::Bookmarks);
    }

    pub fn toggle_downloads_panel(&self) {
        self.toggle_side_panel(SidePanelKind::Downloads);
    }

    pub fn toggle_curtain_panel(&self) {
        self.toggle_side_panel(SidePanelKind::Curtain);
    }

    pub fn toggle_curtain_picker(&self) {
        let Some(bv) = self.ensure_active_browser_view() else {
            return;
        };
        let url = bv.current_url();
        if CurtainManager::host_for_url(&url).is_none() || bv.is_reader_active() {
            self.show_toast("Curtain solo funciona en páginas web", Some(2600));
            return;
        }
        let next = !bv.is_curtain_picking();
        if next {
            self.close_side_panel();
            bv.grab_focus();
        }
        bv.set_curtain_picking(next);
    }

    pub fn toggle_reader_mode(&self) {
        let Some(bv) = self.ensure_active_browser_view() else {
            return;
        };
        if bv.is_reader_active() {
            bv.exit_reader_mode();
            return;
        }
        let url = bv.current_url();
        if !url.starts_with("http://")
            && !url.starts_with("https://")
            && !url.starts_with("file://")
        {
            self.show_toast("Esta página no admite el modo lectura", Some(2600));
            return;
        }
        bv.trigger_reader_extract();
    }

    pub fn open_site_card_popover(&self) {
        if let Some(old) = self.inner.site_card_popover.borrow_mut().take() {
            old.popdown();
            old.unparent();
        }
        let Some(bv) = self.active_browser_view() else {
            return;
        };
        let url = bv.current_url();
        let blocked = bv.blocked_count();
        let bv_reload = bv.clone();
        if let Some(pop) = shield::open_site_card_popover(
            self.inner.address_bar.entry(),
            &url,
            blocked,
            self.inner.is_private,
            &self.inner.stores.shield,
            &self.inner.settings,
            move || bv_reload.reload(),
        ) {
            *self.inner.site_card_popover.borrow_mut() = Some(pop);
        } else {
            self.show_toast("No hay información de sitio para esta página", Some(2400));
        }
    }

    pub fn show_toast(&self, text: &str, duration_ms: Option<u64>) {
        if let Some(prev) = self.inner.active_toast.borrow_mut().take() {
            prev.dismiss();
        }
        let toast = Toast::new(text);
        let secs = match duration_ms {
            Some(ms) => ((ms.saturating_add(999)) / 1000).max(1) as u32,
            None => 0,
        };
        toast.set_timeout(secs);
        self.inner.toast_overlay.add_toast(&toast);
        *self.inner.active_toast.borrow_mut() = Some(toast);
    }

    pub fn hide_toast(&self) {
        if let Some(prev) = self.inner.active_toast.borrow_mut().take() {
            prev.dismiss();
        }
    }

    pub fn show_undo_toast(&self, text: &str, action_name: &str) {
        if let Some(prev) = self.inner.active_toast.borrow_mut().take() {
            prev.dismiss();
        }
        let toast = Toast::new(text);
        toast.set_timeout(5);
        toast.set_button_label(Some("Deshacer"));
        toast.set_action_name(Some(action_name));
        self.inner.toast_overlay.add_toast(&toast);
        *self.inner.active_toast.borrow_mut() = Some(toast);
    }

    pub fn open_boost_for_active_tab(&self) {
        let Some(bv) = self.active_browser_view() else {
            self.show_toast("No hay ninguna pestaña activa", Some(2200));
            return;
        };
        let url = bv.current_url();
        if url.is_empty()
            || url == "about:blank"
            || url.starts_with("dot://")
            || url.starts_with("plasma://")
            || url.starts_with("search://")
        {
            self.show_toast("Arc Boosts solo funciona en páginas web", Some(2400));
            return;
        }
        let weak = self.downgrade();
        boosts_ui::open_boost_dialog(
            &self.inner.window,
            &url,
            self.inner.stores.boosts.clone(),
            self.inner.stores.paths.boosts_file(),
            Rc::new(move |origin, css| {
                let Some(w) = weak.upgrade() else {
                    return;
                };
                let tabs = w.inner.tabs.borrow();
                for tab in tabs.iter() {
                    let matches = normalize_origin(&tab.url)
                        .map(|o| o == origin)
                        .unwrap_or(false);
                    if matches {
                        if let Some(ref tab_bv) = tab.browser {
                            tab_bv.apply_boost_css(&css);
                        }
                    }
                }
            }),
        );
    }

    pub fn open_peek_url(&self, url: &str) {
        let dark_bg = self.inner.app.style_manager().is_dark();
        let weak = self.downgrade();
        if let Err(err) = peek::open_peek_dialog(
            &self.inner.window,
            self.inner.request_context.clone(),
            self.inner.settings.clone(),
            self.inner.stores.shield.clone(),
            self.inner.stores.curtain.clone(),
            url,
            dark_bg,
            move |open_url| {
                if let Some(w) = weak.upgrade() {
                    let _ = w.open_tab(Some(&open_url), true, false, false);
                }
            },
        ) {
            self.show_toast(&format!("No se pudo abrir vista rápida: {err}"), Some(2600));
        }
    }

    pub fn open_peek_for_active_tab(&self) {
        let Some(bv) = self.active_browser_view() else {
            return;
        };
        let url = bv.current_url();
        if !url.is_empty() && url != "about:blank" {
            self.open_peek_url(&url);
        }
    }

    pub fn open_little_arc(&self) {
        let dark_bg = self.inner.app.style_manager().is_dark();
        let initial_url = self
            .active_browser_view()
            .map(|bv| bv.current_url())
            .filter(|u| {
                !u.is_empty()
                    && !u.starts_with("dot://")
                    && !u.starts_with("plasma://")
                    && !u.starts_with("search://")
            })
            .unwrap_or_else(|| "about:blank".to_string());
        let weak = self.downgrade();
        if let Err(err) = peek::open_little_arc_window(
            &self.inner.app,
            self.inner.request_context.clone(),
            self.inner.settings.clone(),
            self.inner.stores.shield.clone(),
            self.inner.stores.curtain.clone(),
            &initial_url,
            dark_bg,
            move |expand_url| {
                if let Some(w) = weak.upgrade() {
                    let _ = w.open_tab(Some(&expand_url), true, false, false);
                    w.inner.window.present();
                }
            },
        ) {
            self.show_toast(&format!("No se pudo abrir Little Arc: {err}"), Some(2600));
        }
    }

    fn render_newtab_page_html(&self) -> String {
        let dark = self.inner.app.style_manager().is_dark();
        let (engine_name, action_url) = self.inner.settings.with(|s| {
            let name = format!("{:?}", s.search_engine);
            let url = resolve_address("search", s.search_engine, &s.custom_search_url).url;
            let base = url
                .split('?')
                .next()
                .unwrap_or("https://duckduckgo.com/")
                .to_string();
            (name, base)
        });
        let tab_count = self.inner.tabs.borrow().len();
        let data = NewTabData {
            theme: if dark {
                NewTabTheme::Dark
            } else {
                NewTabTheme::Light
            },
            search_engine_name: engine_name,
            search_action_url: action_url,
            speed_dial: Vec::new(),
            workspaces: vec![WorkspacePill {
                id: "personal".to_string(),
                name: "Personal".to_string(),
                color_id: "blue".to_string(),
                active: true,
                tab_count,
            }],
            recently_closed_count: 0,
            reading_list_preview: Vec::new(),
            shortcuts: default_shortcuts(),
        };
        render_newtab_html(&data)
    }

    fn refresh_curtain_css_for_host(&self, host: &str) {
        let css = self.inner.stores.curtain.css_for_host(host, None);
        let views: Vec<BrowserView> = self
            .inner
            .tabs
            .borrow()
            .iter()
            .filter_map(|t| {
                let bv = t.browser.as_ref()?;
                let tab_host = CurtainManager::host_for_url(&t.url)?;
                (tab_host == host).then(|| bv.clone())
            })
            .collect();
        for bv in views {
            bv.apply_curtain_css(&css);
        }
    }

    pub fn open_passwords_panel(&self) {
        vault_ui::present_passwords(&self.inner.window, &self.inner.stores.vault);
    }

    pub fn close_side_panel(&self) {
        self.inner.side_split.set_show_sidebar(false);
        self.inner.active_panel.set(SidePanelKind::None);
        if let Some(ref p) = *self.inner.history_panel.borrow() {
            p.set_panel_visible(false);
        }
        if let Some(ref p) = *self.inner.bookmarks_panel.borrow() {
            p.set_panel_visible(false);
        }
        if let Some(ref p) = *self.inner.curtain_panel.borrow() {
            p.set_panel_visible(false);
        }
    }

    /// Abre el popover discreto de marcador (`Ctrl+D` / `Ctrl+Shift+B`) anclado a la barra de direcciones.
    pub fn open_bookmark_popover(&self) {
        if let Some(old) = self.inner.bookmark_popover.borrow_mut().take() {
            old.popdown();
            old.unparent();
        }
        let Some(active_id) = self.selected_tab_id() else {
            return;
        };
        let (url, title) = {
            let tabs = self.inner.tabs.borrow();
            match tabs.iter().find(|t| t.id == active_id) {
                Some(t) => (t.url.clone(), t.display_title()),
                None => return,
            }
        };
        if let Some(pop) = bookmarks_ui::open_bookmark_popover(
            self.inner.address_bar.entry(),
            &self.inner.stores.bookmarks,
            &url,
            &title,
        ) {
            *self.inner.bookmark_popover.borrow_mut() = Some(pop);
        }
    }

    fn init_side_panels(&self) {
        let hist_panel = HistoryPanel::new(
            self.inner.stores.history.clone(),
            {
                let w = self.clone();
                move |url, new_tab| {
                    if new_tab {
                        let _ = w.open_tab(Some(&url), true, false, false);
                    } else {
                        w.navigate_typed_input(&url);
                    }
                    w.close_side_panel();
                }
            },
            {
                let w = self.clone();
                move || w.close_side_panel()
            },
        );
        *self.inner.history_panel.borrow_mut() = Some(hist_panel);

        let bm_panel = BookmarksPanel::new(
            self.inner.window.clone(),
            self.inner.stores.bookmarks.clone(),
            {
                let w = self.clone();
                move |url, new_tab| {
                    if new_tab {
                        let _ = w.open_tab(Some(&url), true, false, false);
                    } else {
                        w.navigate_typed_input(&url);
                    }
                    w.close_side_panel();
                }
            },
            {
                let w = self.clone();
                move || w.close_side_panel()
            },
        );
        *self.inner.bookmarks_panel.borrow_mut() = Some(bm_panel);

        let dl_panel = DownloadsPanel::new(
            self.inner.window.clone(),
            self.inner.stores.downloads.clone(),
            {
                let w = self.clone();
                move |url| {
                    if let Some(bv) = w.ensure_active_browser_view() {
                        bv.start_download(&url);
                    }
                }
            },
            {
                let w = self.clone();
                move || w.close_side_panel()
            },
        );
        *self.inner.downloads_panel.borrow_mut() = Some(dl_panel);

        let curtain_panel = CurtainPanel::new(
            self.inner.stores.curtain.clone(),
            CurtainPanelCallbacks {
                on_start_pick: Rc::new({
                    let w = self.clone();
                    move || {
                        w.close_side_panel();
                        w.toggle_curtain_picker();
                    }
                }),
                on_peek: Rc::new({
                    let w = self.clone();
                    move |host, selector| {
                        if let Some(bv) = w.active_browser_view() {
                            if CurtainManager::host_for_url(&bv.current_url()).as_deref()
                                == Some(host.as_str())
                            {
                                let css_without =
                                    w.inner.stores.curtain.css_for_host(&host, Some(&selector));
                                bv.peek_curtain_selector(&css_without, &selector);
                            }
                        }
                    }
                }),
                on_unpeek: Rc::new({
                    let w = self.clone();
                    move |host| {
                        if let Some(bv) = w.active_browser_view() {
                            if CurtainManager::host_for_url(&bv.current_url()).as_deref()
                                == Some(host.as_str())
                            {
                                let full_css = w.inner.stores.curtain.css_for_host(&host, None);
                                bv.unpeek_curtain(&full_css);
                            }
                        }
                    }
                }),
                on_rules_changed: Rc::new({
                    let w = self.clone();
                    move |host| {
                        w.refresh_curtain_css_for_host(&host);
                    }
                }),
                on_close: Rc::new({
                    let w = self.clone();
                    move || w.close_side_panel()
                }),
            },
        );
        *self.inner.curtain_panel.borrow_mut() = Some(curtain_panel);
    }

    fn toggle_side_panel(&self, kind: SidePanelKind) {
        let already =
            self.inner.active_panel.get() == kind && self.inner.side_split.shows_sidebar();
        if already {
            self.close_side_panel();
            return;
        }

        if let Some(ref p) = *self.inner.history_panel.borrow() {
            p.set_panel_visible(false);
        }
        if let Some(ref p) = *self.inner.bookmarks_panel.borrow() {
            p.set_panel_visible(false);
        }
        if let Some(ref p) = *self.inner.curtain_panel.borrow() {
            p.set_panel_visible(false);
        }

        match kind {
            SidePanelKind::None => {
                self.close_side_panel();
                return;
            }
            SidePanelKind::History => {
                if let Some(ref p) = *self.inner.history_panel.borrow() {
                    self.inner.side_split.set_sidebar(Some(p.widget()));
                    p.set_panel_visible(true);
                }
            }
            SidePanelKind::Bookmarks => {
                if let Some(ref p) = *self.inner.bookmarks_panel.borrow() {
                    self.inner.side_split.set_sidebar(Some(p.widget()));
                    p.set_panel_visible(true);
                }
            }
            SidePanelKind::Downloads => {
                if let Some(ref p) = *self.inner.downloads_panel.borrow() {
                    self.inner.side_split.set_sidebar(Some(p.widget()));
                    p.refresh();
                }
            }
            SidePanelKind::Curtain => {
                let current_url = self
                    .active_browser_view()
                    .map(|bv| bv.current_url())
                    .unwrap_or_default();
                if let Some(ref p) = *self.inner.curtain_panel.borrow() {
                    p.set_current_url(&current_url);
                    self.inner.side_split.set_sidebar(Some(p.widget()));
                    p.set_panel_visible(true);
                }
            }
        }

        self.inner.active_panel.set(kind);
        self.inner.side_split.set_show_sidebar(true);
    }

    pub fn navigate_typed_input(&self, raw_typed: &str) {
        let (engine, custom) = self
            .inner
            .settings
            .with(|s| (s.search_engine, s.custom_search_url.clone()));
        let resolved = resolve_address(raw_typed, engine, &custom);

        // Enrutamiento de páginas internas `dot://...` (y alias `plasma://...` / `search://...`).
        if let Some(internal) = parse_internal_page(&resolved.url) {
            match internal {
                InternalPage::History => {
                    self.toggle_history_panel();
                    return;
                }
                InternalPage::Bookmarks => {
                    self.toggle_bookmarks_panel();
                    return;
                }
                InternalPage::Downloads => {
                    self.toggle_downloads_panel();
                    return;
                }
                InternalPage::Settings { section } => {
                    self.inner.address_bar.set_committed_url(&resolved.url);
                    if let Some(tab_id) = self.selected_tab_id() {
                        self.mount_settings_page_in_tab(tab_id, &resolved.url, section.as_deref());
                    } else {
                        self.open_settings_tab(section.as_deref());
                    }
                    return;
                }
                InternalPage::NewTab => {
                    self.inner.address_bar.set_committed_url("dot://newtab");
                    if let Some(tab_id) = self.selected_tab_id() {
                        self.unmount_settings_page_in_tab(tab_id);
                        {
                            let mut tabs = self.inner.tabs.borrow_mut();
                            if let Some(t) = tabs.iter_mut().find(|t| t.id == tab_id) {
                                t.url = "dot://newtab".to_string();
                                t.title = "Nueva pestaña".to_string();
                                t.touched = now_unix_secs();
                                t.page.set_title("Nueva pestaña");
                                t.page.set_tooltip(Some("dot://newtab"));
                            }
                        }
                        if let Some(bv) = self.ensure_tab_awake(tab_id) {
                            let html = self.render_newtab_page_html();
                            bv.load_internal_html("dot://newtab", &html);
                            bv.grab_focus();
                        }
                        self.sync_sidebar();
                        self.notify_session_dirty();
                    }
                    return;
                }
            }
        }

        self.inner.address_bar.set_committed_url(&resolved.url);

        if let Some(tab_id) = self.selected_tab_id() {
            self.unmount_settings_page_in_tab(tab_id);
            let show_domain = self.inner.settings.with(|s| s.tab_show_domain);
            {
                let mut tabs = self.inner.tabs.borrow_mut();
                if let Some(t) = tabs.iter_mut().find(|t| t.id == tab_id) {
                    t.url = resolved.url.clone();
                    t.touched = now_unix_secs();
                    let title = t.display_title_for(show_domain);
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

    /// `Ctrl+W`: cierra la pestaña activa; en pestañas fijadas respeta `pins_go_home`
    /// y `protect_pinned` estilo Arc (devuelve a su URL inicial o la duerme sin cerrarla).
    pub fn close_active_tab(&self) {
        let Some(active_id) = self.selected_tab_id() else {
            return;
        };
        let (is_pinned, current_url, home_url) = {
            let tabs = self.inner.tabs.borrow();
            match tabs.iter().find(|t| t.id == active_id) {
                Some(t) => (t.pin.is_some(), t.url.clone(), t.home.clone()),
                None => return,
            }
        };

        if is_pinned {
            let (pins_go_home, protect_pinned) = self
                .inner
                .settings
                .with(|s| (s.pins_go_home, s.protect_pinned));

            if pins_go_home {
                if let Some(home) = home_url.filter(|h| !h.is_empty() && *h != current_url) {
                    self.navigate_typed_input(&home);
                    return;
                }
            }

            if protect_pinned {
                self.put_tab_to_sleep(active_id);
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

            self.unpin_tab(active_id);
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
        let closed_count = to_close.len();
        for page in to_close {
            self.inner.tab_view.close_page(&page);
        }
        if closed_count > 0 {
            self.show_toast("Otras pestañas cerradas", Some(1800));
        }
    }

    pub fn close_tabs_to_right(&self, anchor_tab_id: u64) {
        self.sync_order_with_tab_view();
        let to_close: Vec<TabPage> = {
            let tabs = self.inner.tabs.borrow();
            let Some(pos) = tabs.iter().position(|t| t.id == anchor_tab_id) else {
                return;
            };
            tabs.iter()
                .skip(pos + 1)
                .filter(|t| t.pin.is_none())
                .map(|t| t.page.clone())
                .collect()
        };
        let closed_count = to_close.len();
        for page in to_close {
            self.inner.tab_view.close_page(&page);
        }
        if closed_count > 0 {
            self.show_toast("Pestañas a la derecha cerradas", Some(1800));
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
                    spec.pin_style = None;
                    spec.folder_id = None;
                    spec.home = None;
                    spec.touched = Some(now_unix_secs());
                    (spec, idx + 1)
                })
        };
        if let Some((spec, pos)) = spec_and_pos {
            let _ = self.insert_tab_from_spec(spec, Some(pos), true, false);
            self.show_toast("Pestaña duplicada", Some(1800));
        }
    }

    pub fn duplicate_active_tab(&self) {
        if let Some(id) = self.selected_tab_id() {
            self.duplicate_tab(id);
        }
    }

    pub fn toggle_pin_tab(&self, tab_id: u64) {
        let already_pinned = self
            .inner
            .tabs
            .borrow()
            .iter()
            .find(|t| t.id == tab_id)
            .map(|t| t.pin.is_some())
            .unwrap_or(false);
        if already_pinned {
            self.unpin_tab(tab_id);
        } else {
            self.pin_tab_as(tab_id, PinStyle::Square, None);
        }
    }

    /// Fija (o convierte el estilo de fijación de) una pestaña como Favorito cuadrado,
    /// fila fijada raíz o fila dentro de una carpeta de pines estilo Arc.
    pub fn pin_tab_as(&self, tab_id: u64, style: PinStyle, folder_id: Option<String>) {
        let effective_style = if folder_id.is_some() {
            PinStyle::Row
        } else {
            style
        };
        let page_opt = {
            let mut tabs = self.inner.tabs.borrow_mut();
            let Some(t) = tabs.iter_mut().find(|t| t.id == tab_id) else {
                return;
            };
            if t.pin.is_none() {
                t.pin = Some(SessionTab::default_pin_label(&t.title, &t.url));
            }
            if t.home.is_none() {
                t.home = Some(t.url.clone());
            }
            t.pin_style = Some(effective_style);
            t.folder_id = if effective_style == PinStyle::Row {
                folder_id
            } else {
                None
            };
            Some(t.page.clone())
        };
        if let Some(page) = page_opt {
            self.inner.tab_view.set_page_pinned(&page, true);
        }
        self.sync_order_with_tab_view();
        self.sync_split_presentation();
        self.sync_sidebar();
        self.notify_session_dirty();
    }

    /// Desfija una pestaña y la devuelve a la sección de pestañas regulares.
    pub fn unpin_tab(&self, tab_id: u64) {
        let page_opt = {
            let mut tabs = self.inner.tabs.borrow_mut();
            let Some(t) = tabs.iter_mut().find(|t| t.id == tab_id) else {
                return;
            };
            t.pin = None;
            t.pin_style = None;
            t.folder_id = None;
            t.home = None;
            Some(t.page.clone())
        };
        if let Some(page) = page_opt {
            self.inner.tab_view.set_page_pinned(&page, false);
        }
        self.sync_order_with_tab_view();
        self.sync_split_presentation();
        self.sync_sidebar();
        self.notify_session_dirty();
    }

    /// Mueve una pestaña a una carpeta de pines (fijándola como fila automáticamente)
    /// o la saca a la raíz fijada si `folder_id` es `None`.
    pub fn move_tab_to_folder(&self, tab_id: u64, folder_id: Option<String>) {
        let valid_folder = folder_id.filter(|fid| self.inner.stores.pin_folders.contains(fid));
        self.pin_tab_as(tab_id, PinStyle::Row, valid_folder);
    }

    pub fn create_pin_folder(&self, name: &str, parent_id: Option<&str>, move_tab_id: Option<u64>) {
        if let Some(created) = self.inner.stores.pin_folders.create_folder(name, parent_id) {
            if let Some(tid) = move_tab_id {
                self.move_tab_to_folder(tid, Some(created.id));
            } else {
                self.sync_sidebar();
            }
        }
    }

    pub fn rename_pin_folder(&self, folder_id: &str, new_name: &str) {
        let _ = self
            .inner
            .stores
            .pin_folders
            .rename_folder(folder_id, new_name);
    }

    pub fn delete_pin_folder(&self, folder_id: &str) {
        if let Some(promoted_parent) = self.inner.stores.pin_folders.remove_folder(folder_id) {
            {
                let mut tabs = self.inner.tabs.borrow_mut();
                for t in tabs.iter_mut() {
                    if t.folder_id.as_deref() == Some(folder_id) {
                        t.folder_id = promoted_parent.clone();
                    }
                }
            }
            self.sync_sidebar();
            self.notify_session_dirty();
        }
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
        self.sync_split_presentation();
        self.sync_sidebar();
        self.notify_session_dirty();
        self.show_toast(
            if next_muted {
                "Audio silenciado"
            } else {
                "Audio activado"
            },
            Some(1800),
        );
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
        self.show_toast("Pestaña movida a nueva ventana", Some(2000));
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
        if let Some(pop) = self.inner.site_card_popover.borrow_mut().take() {
            pop.popdown();
            pop.unparent();
            return;
        }
        if let Some(pop) = self.inner.bookmark_popover.borrow_mut().take() {
            pop.popdown();
            pop.unparent();
            return;
        }
        if let Some(bv) = self.active_browser_view() {
            if bv.is_curtain_picking() {
                bv.set_curtain_picking(false);
                return;
            }
        }
        if self.inner.vault_ui.dismiss() {
            return;
        }
        if self.inner.side_split.shows_sidebar() {
            self.close_side_panel();
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
            self.show_toast("Modo Zen (F11 o Esc para salir)", Some(2200));
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
            let pct = (z * 100.0).round() as i64;
            self.show_toast(&format!("Zoom: {pct}%"), Some(1600));
        }
    }

    pub fn zoom_out(&self) {
        if let Some(bv) = self.ensure_active_browser_view() {
            bv.zoom_out();
            let z = bv.zoom_factor();
            self.inner.settings.update(|s| s.page_zoom = z);
            let pct = (z * 100.0).round() as i64;
            self.show_toast(&format!("Zoom: {pct}%"), Some(1600));
        }
    }

    pub fn zoom_reset(&self) {
        if let Some(bv) = self.ensure_active_browser_view() {
            bv.zoom_reset();
            self.inner.settings.update(|s| s.page_zoom = 1.0);
            self.show_toast("Zoom restablecido (100%)", Some(1600));
        }
    }

    pub fn view_active_tab_source(&self) {
        let Some(bv) = self.active_browser_view() else {
            return;
        };
        let url = bv.current_url();
        if url.is_empty()
            || url == "about:blank"
            || url.starts_with("view-source:")
            || url.starts_with("dot://")
            || url.starts_with("plasma://")
            || url.starts_with("search://")
        {
            self.show_toast("Esta página no tiene código fuente visible", Some(2000));
            return;
        }
        let target = format!("view-source:{url}");
        let _ = self.open_tab(Some(&target), true, false, false);
    }

    pub fn toggle_force_dark_active_tab(&self) {
        let Some(bv) = self.ensure_active_browser_view() else {
            return;
        };
        let enabled = bv.toggle_force_dark();
        self.show_toast(
            if enabled {
                "Modo oscuro forzado activado"
            } else {
                "Modo oscuro forzado desactivado"
            },
            Some(2000),
        );
    }

    pub fn capture_active_tab_screenshot(&self) {
        let Some(bv) = self.active_browser_view() else {
            self.show_toast("No hay ninguna página activa para capturar", Some(2000));
            return;
        };
        let dir = resolve_pictures_dir();
        if let Err(err) = std::fs::create_dir_all(&dir) {
            self.show_toast(&format!("No se pudo preparar carpeta: {err}"), Some(2600));
            return;
        }
        let stamp = now_unix_secs() as u64;
        let filename = format!("search-captura-{stamp}.png");
        let path = dir.join(&filename);
        match bv.snapshot_png(&path) {
            Ok((w, h)) => {
                self.show_toast(
                    &format!("Captura guardada ({w}x{h}): {filename}"),
                    Some(2600),
                );
            }
            Err(_) => {
                self.show_toast("Aún no hay fotograma listo para capturar", Some(2200));
            }
        }
    }

    pub fn export_active_tab_pdf(&self) {
        let Some(bv) = self.active_browser_view() else {
            self.show_toast("No hay ninguna página activa para exportar", Some(2000));
            return;
        };
        let raw_title = bv.title();
        let safe_base: String = raw_title
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let trimmed = safe_base.trim();
        let initial_name = if trimmed.is_empty() {
            "pagina.pdf".to_string()
        } else {
            format!("{trimmed}.pdf")
        };

        let dialog = FileDialog::new();
        dialog.set_title("Exportar página a PDF");
        dialog.set_initial_name(&initial_name);
        let default_dir = resolve_documents_dir();
        dialog.set_initial_folder(&default_dir);

        let weak_win = self.downgrade();
        dialog.save(Some(&self.inner.window), move |opt_path| {
            let Some(path) = opt_path else {
                return;
            };
            let Some(win) = weak_win.upgrade() else {
                return;
            };
            let Some(bv_now) = win.active_browser_view() else {
                return;
            };
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("documento.pdf")
                .to_string();
            let weak_cb = win.downgrade();
            bv_now.print_to_pdf(&path, move |ok| {
                if let Some(w) = weak_cb.upgrade() {
                    if ok {
                        w.show_toast(&format!("PDF guardado: {filename}"), Some(2600));
                    } else {
                        w.show_toast("No se pudo exportar la página a PDF", Some(2600));
                    }
                }
            });
        });
    }

    /// Cierra limpiamente todos los `cef::Browser` activos de esta ventana (p. ej. al salir).
    pub fn close_all_browsers(&self) {
        if let Some(id) = self.inner.settings_sub_id.take() {
            self.inner.settings.unsubscribe(id);
        }
        if let Some(id) = self.inner.pin_folders_sub_id.take() {
            self.inner.stores.pin_folders.unsubscribe(id);
        }
        self.hide_toast();
        self.inner.address_bar.cleanup();
        self.inner.sidebar_tabs.cleanup();
        if let Some(pop) = self.inner.site_card_popover.borrow_mut().take() {
            pop.popdown();
            pop.unparent();
        }
        if let Some(pop) = self.inner.bookmark_popover.borrow_mut().take() {
            pop.popdown();
            pop.unparent();
        }
        if let Some(pop) = self.inner.active_popover.borrow_mut().take() {
            pop.unparent();
        }
        let browsers: Vec<_> = {
            let mut tabs = self.inner.tabs.borrow_mut();
            tabs.iter_mut().filter_map(|t| t.browser.take()).collect()
        };
        for bv in browsers {
            bv.close_browser();
        }
    }

    pub fn selected_tab_id(&self) -> Option<u64> {
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
        let taken_bv = {
            let mut tabs = self.inner.tabs.borrow_mut();
            let Some(tab) = tabs.iter_mut().find(|t| t.id == tab_id) else {
                return;
            };
            let bv = tab.browser.take();
            if let Some(ref view) = bv {
                tab.slot.remove(view.widget());
            }
            tab.loading = false;
            tab.audible = false;
            tab.page.set_loading(false);
            Self::apply_page_audio_indicator(&tab.page, false, tab.muted);
            bv
        };
        if let Some(bv) = taken_bv {
            bv.close_browser();
        }
    }

    fn mount_settings_page_in_tab(&self, tab_id: u64, canonical_url: &str, section: Option<&str>) {
        let dark = self.inner.app.style_manager().is_dark();
        let (existing_sp, taken_bv) = {
            let mut tabs = self.inner.tabs.borrow_mut();
            let Some(tab) = tabs.iter_mut().find(|t| t.id == tab_id) else {
                return;
            };
            let bv = tab.browser.take();
            if let Some(ref view) = bv {
                tab.slot.remove(view.widget());
            }
            tab.url = canonical_url.to_string();
            tab.title = "Ajustes".to_string();
            tab.loading = false;
            tab.progress = 1.0;
            tab.audible = false;
            tab.favicon_texture = None;
            tab.touched = now_unix_secs();
            tab.page.set_title("Ajustes");
            tab.page.set_tooltip(Some(&tab.tooltip_text()));
            tab.page.set_loading(false);
            tab.page
                .set_icon(Some(&icons::gicon(IconId::Settings, dark)));
            Self::apply_page_audio_indicator(&tab.page, false, false);
            ((tab.settings_page.clone(), tab.slot.clone()), bv)
        };
        if let Some(bv) = taken_bv {
            bv.close_browser();
        }

        if let Some(sp) = existing_sp.0 {
            if let Some(sec) = section {
                sp.select_section(sec);
            }
        } else {
            let sp = SettingsPage::new(
                &self.inner.window,
                &self.inner.settings,
                &self.inner.stores.history,
                &self.inner.stores.downloads,
                &self.inner.stores.paths,
                section,
            );
            existing_sp.1.append(sp.widget());
            let mut tabs = self.inner.tabs.borrow_mut();
            if let Some(tab) = tabs.iter_mut().find(|t| t.id == tab_id) {
                tab.settings_page = Some(sp);
            }
        }

        if self.selected_tab_id() == Some(tab_id) {
            self.inner.progress_bar.set_visible(false);
            self.update_window_title("Ajustes");
        }
        self.sync_split_presentation();
        self.sync_sidebar();
        self.notify_session_dirty();
    }

    fn unmount_settings_page_in_tab(&self, tab_id: u64) {
        let dark = self.inner.app.style_manager().is_dark();
        let mut tabs = self.inner.tabs.borrow_mut();
        if let Some(tab) = tabs.iter_mut().find(|t| t.id == tab_id) {
            if let Some(sp) = tab.settings_page.take() {
                tab.slot.remove(sp.widget());
                tab.page.set_icon(Some(&icons::gicon(IconId::Globe, dark)));
            }
        }
    }

    pub fn ensure_tab_awake(&self, tab_id: u64) -> Option<BrowserView> {
        let existing_or_spec = {
            let tabs = self.inner.tabs.borrow();
            let tab = tabs.iter().find(|t| t.id == tab_id)?;
            if let Some(InternalPage::Settings { section }) = parse_internal_page(&tab.url) {
                let url = tab.url.clone();
                let has_sp = tab.settings_page.is_some();
                drop(tabs);
                if !has_sp {
                    self.mount_settings_page_in_tab(tab_id, &url, section.as_deref());
                }
                return None;
            }
            if let Some(ref bv) = tab.browser {
                return Some(bv.clone());
            }
            (tab.url.clone(), tab.muted, tab.slot.clone())
        };

        let (url, muted, slot) = existing_or_spec;
        let bv = match self.create_browser_view_for_tab(tab_id, &url, muted) {
            Ok(v) => v,
            Err(err) => {
                dot_core::error!("No se pudo despertar la pestaña {tab_id}: {err:#}");
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
            self.inner.stores.shield.clone(),
            self.inner.stores.curtain.clone(),
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
            cbs.on_favicon_texture = Some(Box::new(move |tex| {
                w.handle_tab_favicon_texture(tab_id, tex);
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
                let open_bg = background
                    || w.inner
                        .settings
                        .with(|s| s.open_links_in_background && background);
                let _ = w.open_tab(Some(&target_url), !open_bg, false, false);
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
        {
            let w = self.clone();
            let bv_ref = bv.clone();
            cbs.on_load_finished = Some(Box::new(move |url, status| {
                if status == 0 || (200..400).contains(&status) {
                    if let Some(css) = w
                        .inner
                        .stores
                        .boosts
                        .borrow()
                        .get(&url)
                        .map(compile_boost_css)
                        .filter(|c| !c.is_empty())
                    {
                        bv_ref.apply_boost_css(&css);
                    }
                    if !w.inner.is_private && !bv_ref.last_load_was_error() {
                        let title = bv_ref.title();
                        w.inner.stores.history.record_visit(&url, &title);
                    }
                }
            }));
        }
        {
            let w = self.clone();
            cbs.on_before_download = Some(Box::new(move |ev| {
                w.inner.stores.downloads.handle_before_download(
                    ev,
                    &w.inner.window,
                    w.inner.is_private,
                );
            }));
        }
        {
            let w = self.clone();
            cbs.on_download_updated = Some(Box::new(move |ev| {
                let any_active = w
                    .inner
                    .callbacks
                    .on_is_any_window_active
                    .as_ref()
                    .map(|cb| cb())
                    .unwrap_or_else(|| w.inner.window.is_active());
                w.inner.stores.downloads.handle_download_updated(
                    ev,
                    w.inner.is_private,
                    &w.inner.app,
                    any_active,
                );
            }));
        }
        {
            let w = self.clone();
            let bv_ref = bv.clone();
            cbs.on_curtain_pick = Some(Box::new(move |snapshot| {
                let url = bv_ref.current_url();
                let Some(host) = CurtainManager::host_for_url(&url) else {
                    return;
                };
                if w.inner
                    .stores
                    .curtain
                    .hide_from_snapshot(&host, &snapshot)
                    .is_some()
                {
                    w.refresh_curtain_css_for_host(&host);
                    w.show_toast("Elemento ocultado · Esc para salir", None);
                }
            }));
        }
        {
            let w = self.clone();
            cbs.on_curtain_picking_changed = Some(Box::new(move |active| {
                if w.selected_tab_id() != Some(tab_id) {
                    return;
                }
                if active {
                    w.show_toast(
                        "Haz clic en un elemento para ocultarlo · Esc para salir",
                        None,
                    );
                } else {
                    w.hide_toast();
                }
            }));
        }
        {
            let w = self.clone();
            let bv_ref = bv.clone();
            cbs.on_reader_extracted = Some(Box::new(move |ok, reason, article_opt| {
                if !ok {
                    if reason.as_deref() != Some("already_reader") {
                        w.show_toast(
                            "Esta página no parece un artículo para el modo lectura",
                            Some(3000),
                        );
                    }
                    return;
                }
                let Some(article) = article_opt else {
                    w.show_toast(
                        "Esta página no parece un artículo para el modo lectura",
                        Some(3000),
                    );
                    return;
                };
                if !article.is_readable() {
                    w.show_toast(
                        "Esta página no parece un artículo para el modo lectura",
                        Some(3000),
                    );
                    return;
                }
                let orig_url = if article.url.is_empty() {
                    bv_ref.current_url()
                } else {
                    article.url.clone()
                };
                let opts = w
                    .inner
                    .reader_toolbar
                    .borrow()
                    .as_ref()
                    .map(ReaderToolbar::options)
                    .unwrap_or_default();
                let html = render_reader_document(&article, opts);
                bv_ref.enter_reader_html(&orig_url, &html);
            }));
        }
        {
            let w = self.clone();
            cbs.on_reader_state_changed = Some(Box::new(move |active| {
                if w.selected_tab_id() == Some(tab_id) {
                    if let Some(ref bar) = *w.inner.reader_toolbar.borrow() {
                        bar.set_visible(active);
                    }
                }
            }));
        }

        crate::vault::attach(
            &bv,
            &mut cbs,
            tab_id,
            &self.inner.stores.vault,
            &self.inner.vault_ui,
            &self.inner.settings,
        );

        {
            let w = self.clone();
            cbs.on_context_menu = Some(Box::new(move |info| {
                w.show_page_context_menu(tab_id, info);
            }));
        }
        bv.set_callbacks(cbs);
        Ok(bv)
    }

    fn handle_tab_title_changed(&self, tab_id: u64, title: String) {
        let is_selected = self.selected_tab_id() == Some(tab_id);
        let show_domain = self.inner.settings.with(|s| s.tab_show_domain);
        let (updated_title, current_url) = {
            let mut tabs = self.inner.tabs.borrow_mut();
            let Some(tab) = tabs.iter_mut().find(|t| t.id == tab_id) else {
                return;
            };
            tab.title = title.clone();
            let disp = tab.display_title_for(show_domain);
            let tip = tab.tooltip_text();
            tab.page.set_title(&disp);
            tab.page.set_tooltip(Some(&tip));
            (tab.display_title(), tab.url.clone())
        };
        if !self.inner.is_private && !title.trim().is_empty() {
            self.inner.stores.history.retitle(&current_url, &title);
        }
        if is_selected {
            self.update_window_title(&updated_title);
        }
        self.sync_split_presentation();
        self.sync_sidebar();
        self.notify_session_dirty();
    }

    fn handle_tab_address_changed(&self, tab_id: u64, new_url: String) {
        let is_selected = self.selected_tab_id() == Some(tab_id);
        let show_domain = self.inner.settings.with(|s| s.tab_show_domain);
        {
            let mut tabs = self.inner.tabs.borrow_mut();
            let Some(tab) = tabs.iter_mut().find(|t| t.id == tab_id) else {
                return;
            };
            tab.url = new_url.clone();
            tab.touched = now_unix_secs();
            let disp = tab.display_title_for(show_domain);
            let tip = tab.tooltip_text();
            tab.page.set_title(&disp);
            tab.page.set_tooltip(Some(&tip));
        }
        if is_selected {
            self.inner.address_bar.set_committed_url(&new_url);
            if let Some(ref p) = *self.inner.curtain_panel.borrow() {
                p.set_current_url(&new_url);
            }
        }
        self.sync_split_presentation();
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

    fn handle_tab_favicon_texture(&self, tab_id: u64, tex: Texture) {
        let gicon = tex.to_gicon();
        {
            let mut tabs = self.inner.tabs.borrow_mut();
            let Some(tab) = tabs.iter_mut().find(|t| t.id == tab_id) else {
                return;
            };
            if tab.is_settings_tab() {
                return;
            }
            tab.favicon_texture = Some(tex);
            tab.page.set_icon(Some(&gicon));
        }
        self.sync_sidebar();
    }

    fn handle_tab_audio_changed(&self, tab_id: u64, audible: bool, muted: bool) {
        let show_audio = self.inner.settings.with(|s| s.show_audio_indicators);
        {
            let mut tabs = self.inner.tabs.borrow_mut();
            let Some(tab) = tabs.iter_mut().find(|t| t.id == tab_id) else {
                return;
            };
            tab.audible = audible;
            tab.muted = muted;
            Self::apply_page_audio_indicator(&tab.page, audible && show_audio, muted && show_audio);
        }
        self.sync_sidebar();
    }

    fn apply_page_audio_indicator(page: &TabPage, audible: bool, muted: bool) {
        let dark = icons::is_dark_theme();
        if muted {
            let icon = icons::gicon(IconId::VolumeMuted, dark);
            page.set_indicator_icon(Some(&icon));
            page.set_indicator_tooltip(Some("Activar sonido de la pestaña"));
            page.set_indicator_activatable(true);
        } else if audible {
            let icon = icons::gicon(IconId::VolumeHigh, dark);
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
        self.inner.vault_ui.on_tab_selected(active_id);

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
        if let Some(ref p) = *self.inner.curtain_panel.borrow() {
            p.set_current_url(&url);
        }
        let reader_active = self
            .active_browser_view()
            .map(|bv| bv.is_reader_active())
            .unwrap_or(false);
        if let Some(ref bar) = *self.inner.reader_toolbar.borrow() {
            bar.set_visible(reader_active);
        }
        let picking_active = self
            .active_browser_view()
            .map(|bv| bv.is_curtain_picking())
            .unwrap_or(false);
        if !picking_active {
            self.hide_toast();
        }

        if loading && progress < 1.0 {
            self.inner.progress_bar.set_fraction(progress.max(0.06));
            self.inner.progress_bar.set_visible(true);
        } else {
            self.inner.progress_bar.set_visible(false);
        }

        if self.inner.find_bar.is_open() {
            self.inner.find_bar.close();
        }

        self.sync_split_presentation();
        self.sync_sidebar();
        self.notify_session_dirty();
    }

    fn update_window_title(&self, page_title: &str) {
        let clean = page_title.trim();
        let full = if self.inner.is_private {
            if clean.is_empty() || clean == "Nueva pestaña" {
                "Dot — Privada".to_string()
            } else {
                format!("{clean} — Dot (Privada)")
            }
        } else if clean.is_empty() || clean == "Nueva pestaña" {
            "Dot".to_string()
        } else {
            format!("{clean} — Dot")
        };
        self.inner.window.set_title(&full);
    }

    fn apply_layout_from_settings(&self, s: &dot_core::settings::AppSettings) {
        let folded = self.inner.folded.get();
        let fullscreen = self.inner.window.is_fullscreen();
        let sidebar_ms = s.ui_motion.duration_ms(220);
        let top_ms = s.ui_motion.duration_ms(180);
        let folder_ms = s.ui_motion.duration_ms(180);
        let find_ms = s.ui_motion.duration_ms(150);
        let settings_ms = s.ui_motion.duration_ms(160);

        self.inner
            .sidebar_revealer
            .set_transition_duration(sidebar_ms);
        self.inner.top_bar_revealer.set_transition_duration(top_ms);
        self.inner.find_bar.set_motion_duration(find_ms);
        self.inner.sidebar_tabs.set_motion_duration(folder_ms);
        self.inner
            .sidebar_tabs
            .set_width(s.sidebar_width.round() as i32);
        self.inner.sidebar_tabs.set_roomy(s.roomy_sidebar);

        // Clases CSS globales de densidad y movimiento instantáneo.
        if s.compact_tabs {
            self.inner.window.add_css_class("search-compact-tabs");
        } else {
            self.inner.window.remove_css_class("search-compact-tabs");
        }
        if s.ui_motion == UiMotion::Instant {
            self.inner.window.add_css_class("search-motion-instant");
        } else {
            self.inner.window.remove_css_class("search-motion-instant");
        }

        // Marco de tarjeta redondeado en modo lateral estilo Arc (`layout_cards`).
        let show_cards = s.vertical_tabs && s.layout_cards && !fullscreen;
        if show_cards {
            self.inner.main_hbox.add_css_class("cards-enabled");
        } else {
            self.inner.main_hbox.remove_css_class("cards-enabled");
        }

        // Actualizar títulos, audio, zoom y duración de animaciones en pestañas existentes.
        {
            let tabs = self.inner.tabs.borrow();
            for tab in tabs.iter() {
                let disp = tab.display_title_for(s.tab_show_domain);
                tab.page.set_title(&disp);
                Self::apply_page_audio_indicator(
                    &tab.page,
                    tab.audible && s.show_audio_indicators,
                    tab.muted && s.show_audio_indicators,
                );
                if let Some(ref sp) = tab.settings_page {
                    sp.set_motion_duration(settings_ms);
                }
                if let Some(ref bv) = tab.browser {
                    if (bv.zoom_factor() - s.page_zoom).abs() > 0.005 {
                        bv.set_zoom_factor(s.page_zoom);
                    }
                }
            }
        }

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

    fn refresh_theme_icons(&self) {
        let dark = self.inner.app.style_manager().is_dark();
        let show_audio = self.inner.settings.with(|s| s.show_audio_indicators);
        self.inner
            .tab_view
            .set_default_icon(&icons::gicon(IconId::Globe, dark));
        {
            let tabs = self.inner.tabs.borrow();
            for tab in tabs.iter() {
                if tab.is_settings_tab() {
                    tab.page
                        .set_icon(Some(&icons::gicon(IconId::Settings, dark)));
                }
                if let Some(ref sp) = tab.settings_page {
                    sp.refresh_theme_icons(dark);
                }
                Self::apply_page_audio_indicator(
                    &tab.page,
                    tab.audible && show_audio,
                    tab.muted && show_audio,
                );
            }
        }
        self.inner.sidebar_tabs.refresh_theme_icons(dark);
        self.sync_sidebar();
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
        let snap = self.inner.settings.snapshot();
        if !snap.vertical_tabs {
            return;
        }
        self.sync_order_with_tab_view();
        let selected_page = self.inner.tab_view.selected_page();
        let tabs_borrow = self.inner.tabs.borrow();
        let splits_borrow = self.inner.splits.borrow();
        let items: Vec<SidebarTabItem> = tabs_borrow
            .iter()
            .map(|t| {
                let split_info = splits_borrow.iter().find(|sp| sp.contains(t.id));
                let is_primary = split_info.and_then(|sp| sp.left()) == Some(t.id);
                let partner = if is_primary {
                    split_info
                        .and_then(|sp| sp.partner(t.id))
                        .and_then(|pid| tabs_borrow.iter().find(|pt| pt.id == pid))
                } else {
                    None
                };
                let split_hidden = split_info.is_some() && !is_primary && t.pin.is_none();
                SidebarTabItem {
                    id: t.id,
                    url: t.url.clone(),
                    title: t.display_title_for(snap.tab_show_domain),
                    tooltip: t.tooltip_text(),
                    pin_label: t.pin.clone(),
                    pin_style: t.effective_pin_style(),
                    folder_id: t.folder_id.clone(),
                    selected: selected_page.as_ref() == Some(&t.page),
                    loading: t.loading,
                    audible: t.audible,
                    muted: t.muted,
                    favicon: t.favicon_texture.clone(),
                    split_partner_favicon: partner.and_then(|p| p.favicon_texture.clone()),
                    split_partner_title: partner.map(|p| p.display_title_for(snap.tab_show_domain)),
                    split_partner_url: partner.map(|p| p.url.clone()),
                    split_hidden,
                }
            })
            .collect();
        drop(splits_borrow);
        drop(tabs_borrow);
        let folders = self.inner.stores.pin_folders.snapshot();
        let opts = SidebarRenderOptions {
            glyph: snap.tab_glyph,
            roomy: snap.roomy_sidebar,
            show_audio_indicators: snap.show_audio_indicators,
            motion_ms: snap.ui_motion.duration_ms(180),
        };
        self.inner.sidebar_tabs.sync(&items, &folders, opts);
    }

    fn populate_tab_context_menu(&self, tab_id: u64) {
        self.inner.menu_target_tab.set(Some(tab_id));
        let (pin_style, folder_id, is_muted) = {
            let tabs = self.inner.tabs.borrow();
            match tabs.iter().find(|t| t.id == tab_id) {
                Some(t) => (t.effective_pin_style(), t.folder_id.clone(), t.muted),
                None => (None, None, false),
            }
        };
        let is_pinned = pin_style.is_some();

        self.inner.tab_close_action.set_enabled(!is_pinned);
        self.inner.tab_menu.remove_all();

        let sec1 = Menu::new();
        sec1.append("Recargar", Some("tab.reload"));
        sec1.append("Duplicar", Some("tab.duplicate"));
        sec1.append(
            if is_muted {
                "Activar sonido"
            } else {
                "Silenciar pestaña"
            },
            Some("tab.toggle-mute"),
        );
        self.inner.tab_menu.append_section(None, &sec1);

        // Sección de fijación estilo Arc + submenú de carpetas de pines.
        let sec_pin = Menu::new();
        match pin_style {
            None => {
                sec_pin.append("Fijar como favorito (cuadrado)", Some("tab.pin-square"));
                sec_pin.append("Fijar en barra lateral", Some("tab.pin-row"));
            }
            Some(PinStyle::Square) => {
                sec_pin.append("Mover a filas fijadas", Some("tab.pin-row"));
                sec_pin.append("Desfijar pestaña", Some("tab.unpin"));
            }
            Some(PinStyle::Row) => {
                sec_pin.append("Mover a favoritos (cuadrado)", Some("tab.pin-square"));
                sec_pin.append("Desfijar pestaña", Some("tab.unpin"));
            }
        }

        let folder_submenu = Menu::new();
        if folder_id.is_some() {
            folder_submenu.append("Sacar de la carpeta (raíz)", Some("tab.move-to-folder('')"));
        }
        let all_folders = self.inner.stores.pin_folders.flattened();
        if !all_folders.is_empty() {
            let list_sec = Menu::new();
            for (folder, depth) in all_folders {
                let prefix = "  ".repeat(depth);
                let mark = if folder_id.as_deref() == Some(folder.id.as_str()) {
                    "✓ "
                } else {
                    ""
                };
                let label = format!("{prefix}{mark}{}", folder.name);
                let detailed = format!("tab.move-to-folder('{}')", folder.id);
                list_sec.append(&label, Some(&detailed));
            }
            folder_submenu.append_section(None, &list_sec);
        }
        let create_sec = Menu::new();
        create_sec.append("Crear carpeta nueva…", Some("tab.create-folder-and-move"));
        folder_submenu.append_section(None, &create_sec);

        sec_pin.append_submenu("Mover a carpeta", &folder_submenu);
        self.inner.tab_menu.append_section(None, &sec_pin);

        let sec2 = Menu::new();
        sec2.append("Modo lectura", Some("tab.toggle-reader"));
        sec2.append("Ocultar elemento (Curtain)", Some("tab.curtain-pick"));
        sec2.append("Elementos ocultos…", Some("tab.toggle-curtain"));
        sec2.append("Información del sitio y Shield…", Some("tab.site-info"));
        let in_split = self
            .inner
            .splits
            .borrow()
            .iter()
            .any(|sp| sp.contains(tab_id));
        if in_split {
            sec2.append("Separar pestañas", Some("tab.split-separate"));
            sec2.append("Intercambiar paneles", Some("tab.split-swap"));
        } else {
            sec2.append("Abrir al lado", Some("tab.split-beside"));
        }
        sec2.append("Mover a ventana nueva", Some("tab.move-to-new-window"));
        self.inner.tab_menu.append_section(None, &sec2);

        let sec3 = Menu::new();
        sec3.append("Cerrar pestaña", Some("tab.close"));
        sec3.append("Cerrar las demás pestañas", Some("tab.close-others"));
        sec3.append("Cerrar pestañas a la derecha", Some("tab.close-right"));
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
                "split-beside",
                Box::new(move || {
                    if let Some(id) = w.inner.menu_target_tab.get() {
                        w.split_tab_by_id(id, None);
                    }
                }),
            );
        }
        {
            let w = self.clone();
            add(
                "split-separate",
                Box::new(move || {
                    if let Some(id) = w.inner.menu_target_tab.get() {
                        w.separate_split_for_tab(id);
                    }
                }),
            );
        }
        {
            let w = self.clone();
            add(
                "split-swap",
                Box::new(move || {
                    if let Some(id) = w.inner.menu_target_tab.get() {
                        w.swap_split_for_tab(id);
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
                "pin-square",
                Box::new(move || {
                    if let Some(id) = w.inner.menu_target_tab.get() {
                        w.pin_tab_as(id, PinStyle::Square, None);
                    }
                }),
            );
        }
        {
            let w = self.clone();
            add(
                "pin-row",
                Box::new(move || {
                    if let Some(id) = w.inner.menu_target_tab.get() {
                        w.pin_tab_as(id, PinStyle::Row, None);
                    }
                }),
            );
        }
        {
            let w = self.clone();
            add(
                "unpin",
                Box::new(move || {
                    if let Some(id) = w.inner.menu_target_tab.get() {
                        w.unpin_tab(id);
                    }
                }),
            );
        }
        {
            let w = self.clone();
            let move_act = SimpleAction::new_string_param("move-to-folder");
            move_act.connect_activate_string(move |folder_id| {
                if let Some(id) = w.inner.menu_target_tab.get() {
                    let target = if folder_id.trim().is_empty() {
                        None
                    } else {
                        Some(folder_id)
                    };
                    w.move_tab_to_folder(id, target);
                }
            });
            group.add_action(&move_act);
        }
        {
            let w = self.clone();
            add(
                "create-folder-and-move",
                Box::new(move || {
                    if let Some(id) = w.inner.menu_target_tab.get() {
                        if !w.inner.settings.with(|s| s.vertical_tabs) {
                            w.inner.settings.update(|s| s.vertical_tabs = true);
                        }
                        if w.inner.folded.get() {
                            w.toggle_fold_tabs();
                        }
                        w.inner.sidebar_tabs.begin_create_folder(None, Some(id));
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
                "toggle-reader",
                Box::new(move || {
                    if let Some(id) = w.inner.menu_target_tab.get() {
                        w.select_tab_by_id(id);
                        w.toggle_reader_mode();
                    }
                }),
            );
        }
        {
            let w = self.clone();
            add(
                "curtain-pick",
                Box::new(move || {
                    if let Some(id) = w.inner.menu_target_tab.get() {
                        w.select_tab_by_id(id);
                        w.toggle_curtain_picker();
                    }
                }),
            );
        }
        {
            let w = self.clone();
            add(
                "toggle-curtain",
                Box::new(move || {
                    if let Some(id) = w.inner.menu_target_tab.get() {
                        w.select_tab_by_id(id);
                        w.toggle_curtain_panel();
                    }
                }),
            );
        }
        {
            let w = self.clone();
            add(
                "site-info",
                Box::new(move || {
                    if let Some(id) = w.inner.menu_target_tab.get() {
                        w.select_tab_by_id(id);
                        w.open_site_card_popover();
                    }
                }),
            );
        }
        {
            let weak = self.downgrade();
            add(
                "move-to-new-window",
                Box::new(move || {
                    if let Some(w) = weak.upgrade() {
                        if let Some(id) = w.inner.menu_target_tab.get() {
                            w.move_tab_to_new_window(id);
                        }
                    }
                }),
            );
        }
        {
            let weak = self.downgrade();
            self.inner.tab_close_action.connect_activate(move || {
                if let Some(w) = weak.upgrade() {
                    if let Some(id) = w.inner.menu_target_tab.get() {
                        let _ = w.close_tab_by_id(id);
                    }
                }
            });
            group.add_action(&self.inner.tab_close_action);
        }
        {
            let weak = self.downgrade();
            add(
                "close-others",
                Box::new(move || {
                    if let Some(w) = weak.upgrade() {
                        if let Some(id) = w.inner.menu_target_tab.get() {
                            w.close_other_tabs(id);
                        }
                    }
                }),
            );
        }
        {
            let weak = self.downgrade();
            add(
                "close-right",
                Box::new(move || {
                    if let Some(w) = weak.upgrade() {
                        if let Some(id) = w.inner.menu_target_tab.get() {
                            w.close_tabs_to_right(id);
                        }
                    }
                }),
            );
        }

        self.inner.window.insert_action_group("tab", Some(&group));
    }

    fn wire_signals(&self) {
        // Barra de direcciones: envío, autocompletado unificado, cambio de pestaña,
        // borrado con Shift+Supr y tecla Escape.
        {
            let weak = self.downgrade();
            self.inner.address_bar.connect_submit(move |typed| {
                if let Some(w) = weak.upgrade() {
                    w.navigate_typed_input(&typed);
                }
            });
        }
        {
            let weak = self.downgrade();
            self.inner.address_bar.connect_escape(move || {
                if let Some(w) = weak.upgrade() {
                    if let Some(bv) = w.active_browser_view() {
                        bv.grab_focus();
                    }
                }
            });
        }
        {
            let weak = self.downgrade();
            self.inner
                .address_bar
                .connect_suggest_provider(move |typed| {
                    let Some(w) = weak.upgrade() else {
                        return Vec::new();
                    };
                    let open_tabs = w
                        .inner
                        .callbacks
                        .on_list_open_tabs
                        .as_ref()
                        .map(|cb| cb())
                        .unwrap_or_else(|| w.open_tab_infos());
                    let now = now_unix_secs();
                    w.inner.stores.history.with_store(|hist| {
                        w.inner
                            .stores
                            .bookmarks
                            .with_store(|bms| suggest_omnibox(typed, hist, bms, &open_tabs, 8, now))
                    })
                });
        }
        {
            let weak = self.downgrade();
            self.inner
                .address_bar
                .connect_switch_tab(move |win_id, tab_id| {
                    let Some(w) = weak.upgrade() else {
                        return;
                    };
                    if let Some(ref cb) = w.inner.callbacks.on_switch_to_tab {
                        cb(win_id, tab_id);
                    } else if win_id == w.inner.id {
                        w.select_tab_by_id(tab_id);
                    }
                });
        }
        {
            let weak = self.downgrade();
            self.inner.address_bar.connect_delete_history(move |key| {
                if let Some(w) = weak.upgrade() {
                    w.inner.stores.history.remove_by_key(&key);
                }
            });
        }

        // Sincronizar estado del panel lateral derecho al cerrarse haciendo clic fuera.
        {
            let weak = self.downgrade();
            self.inner.side_split.connect_show_sidebar_notify(move || {
                let Some(w) = weak.upgrade() else {
                    return;
                };
                if !w.inner.side_split.shows_sidebar() {
                    w.inner.active_panel.set(SidePanelKind::None);
                    if let Some(ref p) = *w.inner.history_panel.borrow() {
                        p.set_panel_visible(false);
                    }
                    if let Some(ref p) = *w.inner.bookmarks_panel.borrow() {
                        p.set_panel_visible(false);
                    }
                    if let Some(ref p) = *w.inner.curtain_panel.borrow() {
                        p.set_panel_visible(false);
                    }
                }
            });
        }

        // Barra de búsqueda en página.
        {
            let weak = self.downgrade();
            self.inner
                .find_bar
                .connect_find(move |query, forward, find_next| {
                    if let Some(w) = weak.upgrade() {
                        if let Some(bv) = w.ensure_active_browser_view() {
                            bv.find(&query, forward, false, find_next);
                        }
                    }
                });
        }
        {
            let weak = self.downgrade();
            self.inner.find_bar.connect_close(move || {
                if let Some(w) = weak.upgrade() {
                    if let Some(bv) = w.active_browser_view() {
                        bv.stop_finding(true);
                        bv.grab_focus();
                    }
                }
            });
        }

        // Señales del modelo único `AdwTabView`.
        {
            let weak = self.downgrade();
            self.inner.tab_view.connect_selected_page_notify(move || {
                if let Some(w) = weak.upgrade() {
                    w.on_active_tab_changed();
                }
            });
        }
        {
            let weak = self.downgrade();
            self.inner
                .tab_view
                .connect_page_reordered(move |_page, _pos| {
                    if let Some(w) = weak.upgrade() {
                        w.sync_order_with_tab_view();
                        w.sync_sidebar();
                        w.notify_session_dirty();
                    }
                });
        }
        {
            let weak = self.downgrade();
            self.inner
                .tab_view
                .connect_indicator_activated(move |page| {
                    if let Some(w) = weak.upgrade() {
                        if let Some(id) = w.tab_id_for_page(&page) {
                            w.toggle_mute_tab(id);
                        }
                    }
                });
        }
        {
            let weak = self.downgrade();
            self.inner.tab_view.connect_setup_menu(move |opt_page| {
                if let Some(w) = weak.upgrade() {
                    if let Some(page) = opt_page {
                        if let Some(id) = w.tab_id_for_page(&page) {
                            w.populate_tab_context_menu(id);
                        }
                    }
                }
            });
        }
        {
            let weak = self.downgrade();
            self.inner.tab_view.connect_close_page(move |page| {
                let Some(w) = weak.upgrade() else {
                    return false;
                };
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
                    if w.inner.pip.active_tab_id() == Some(rec.id) {
                        w.inner.pip.close(false, true);
                    }
                    w.remove_tab_from_splits(rec.id);
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
                w.sync_split_presentation();
                w.sync_sidebar();
                w.notify_session_dirty();

                if w.inner.tab_view.n_pages() == 0 && !w.inner.closing_window.get() {
                    let win_close = w.downgrade();
                    glib::idle_add_local_once(move || {
                        if let Some(w_close) = win_close.upgrade() {
                            w_close.inner.window.close();
                        }
                    });
                }
                true
            });
        }

        // Callbacks de la barra lateral vertical (`SidebarTabs`).
        let mut scbs = SidebarCallbacks::default();
        {
            let weak = self.downgrade();
            scbs.on_select = Some(Box::new(move |id| {
                if let Some(w) = weak.upgrade() {
                    w.select_tab_by_id(id);
                }
            }));
        }
        {
            let weak = self.downgrade();
            scbs.on_close = Some(Box::new(move |id| {
                if let Some(w) = weak.upgrade() {
                    let _ = w.close_tab_by_id(id);
                }
            }));
        }
        {
            let weak = self.downgrade();
            scbs.on_toggle_mute = Some(Box::new(move |id| {
                if let Some(w) = weak.upgrade() {
                    w.toggle_mute_tab(id);
                }
            }));
        }
        {
            let weak = self.downgrade();
            scbs.on_context_menu = Some(Box::new(move |id, anchor, x, y| {
                let Some(w) = weak.upgrade() else {
                    return;
                };
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
            let weak = self.downgrade();
            scbs.on_reorder = Some(Box::new(move |src_id, dst_id| {
                let Some(w) = weak.upgrade() else {
                    return;
                };
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
            let weak = self.downgrade();
            scbs.on_drop_tab_into_folder = Some(Box::new(move |tab_id, folder_id| {
                if let Some(w) = weak.upgrade() {
                    w.move_tab_to_folder(tab_id, Some(folder_id));
                }
            }));
        }
        {
            let weak = self.downgrade();
            scbs.on_new_tab = Some(Box::new(move || {
                if let Some(w) = weak.upgrade() {
                    let _ = w.open_tab(None, true, false, false);
                }
            }));
        }
        {
            let weak = self.downgrade();
            scbs.on_open_settings = Some(Box::new(move || {
                if let Some(w) = weak.upgrade() {
                    w.open_settings_tab(None);
                }
            }));
        }
        {
            let weak = self.downgrade();
            scbs.on_create_folder = Some(Box::new(move |name, parent_id, move_tab_id| {
                if let Some(w) = weak.upgrade() {
                    w.create_pin_folder(&name, parent_id.as_deref(), move_tab_id);
                }
            }));
        }
        {
            let weak = self.downgrade();
            scbs.on_rename_folder = Some(Box::new(move |folder_id, new_name| {
                if let Some(w) = weak.upgrade() {
                    w.rename_pin_folder(&folder_id, &new_name);
                }
            }));
        }
        {
            let weak = self.downgrade();
            scbs.on_delete_folder = Some(Box::new(move |folder_id| {
                if let Some(w) = weak.upgrade() {
                    w.delete_pin_folder(&folder_id);
                }
            }));
        }
        {
            let weak = self.downgrade();
            scbs.on_toggle_folder_collapsed = Some(Box::new(move |folder_id, collapsed| {
                if let Some(w) = weak.upgrade() {
                    w.inner
                        .stores
                        .pin_folders
                        .set_collapsed_silent(&folder_id, collapsed);
                }
            }));
        }
        {
            let weak = self.downgrade();
            scbs.on_workspace_changed = Some(Box::new(move |ws_id, accent_hex| {
                if let Some(w) = weak.upgrade() {
                    let css = format!(
                        ".space-pill.active {{ border-color: {accent_hex}; background-color: alpha({accent_hex}, 0.18); }} \
                         .search-sidebar {{ border-left: 2px solid alpha({accent_hex}, 0.55); }}"
                    );
                    w.inner.space_accent_provider.load_from_string(&css);
                    w.show_toast(&format!("Espacio: {ws_id}"), Some(1800));
                }
            }));
        }
        {
            let weak = self.downgrade();
            scbs.on_clear_today = Some(Box::new(move |ids| {
                let Some(w) = weak.upgrade() else {
                    return;
                };
                let mut closed = 0usize;
                for id in ids {
                    if w.close_tab_by_id(id) {
                        closed += 1;
                    }
                }
                if closed > 0 {
                    let msg = if closed == 1 {
                        "1 pestaña de Hoy cerrada".to_string()
                    } else {
                        format!("{closed} pestañas de Hoy cerradas")
                    };
                    w.show_undo_toast(&msg, "win.reopen-tab");
                }
            }));
        }
        {
            let weak = self.downgrade();
            scbs.on_tidy_tabs = Some(Box::new(move || {
                if let Some(w) = weak.upgrade() {
                    w.show_toast("Pestañas ordenadas por dominio", Some(2000));
                }
            }));
        }
        self.inner.sidebar_tabs.set_callbacks(scbs);

        // Al cerrarse la ventana: avisar al controlador (guarda sesión antes de destruir) y cerrar CEF.
        {
            let weak = self.downgrade();
            self.inner.window.connect_close_request(move || {
                let Some(w) = weak.upgrade() else {
                    return false;
                };
                w.inner.closing_window.set(true);
                if w.inner.pip.is_active() {
                    w.inner.pip.close(false, true);
                }
                let win_id = w.inner.id;
                if let Some(ref cb) = w.inner.callbacks.on_window_closed {
                    cb(win_id);
                }
                w.close_all_browsers();
                false
            });
        }
    }

    // ========================================================================
    // Split View (`split_view.rs` + `TabSplit`)
    // ========================================================================

    pub fn split_current_tab(&self, url: Option<&str>) {
        if let Some(id) = self.selected_tab_id() {
            self.split_tab_by_id(id, url);
        }
    }

    pub fn split_tab_by_id(&self, tab_id: u64, url: Option<&str>) {
        let existing_partner = self
            .inner
            .splits
            .borrow()
            .iter()
            .find(|sp| sp.contains(tab_id))
            .and_then(|sp| sp.partner(tab_id));

        if let Some(partner_id) = existing_partner {
            if let Some(target_url) = url {
                let _ = self.ensure_tab_awake(partner_id);
                if let Some(bv) = self.tab_browser_view(partner_id) {
                    bv.load_url(target_url);
                }
                self.focus_split_pane(partner_id);
            } else {
                self.focus_split_pane(partner_id);
            }
            return;
        }

        let Ok(partner_id) = self.open_tab(url, true, false, false) else {
            return;
        };
        let split_id = self.inner.next_split_id.get();
        self.inner.next_split_id.set(split_id.saturating_add(1));
        let split = TabSplit::new_pair(split_id, tab_id, partner_id, 0.5);
        self.inner.splits.borrow_mut().push(split);

        self.sync_split_presentation();
        self.sync_split_presentation();
        self.sync_sidebar();
        self.notify_session_dirty();
    }

    pub fn focus_split_pane(&self, tab_id: u64) {
        let in_split = {
            let mut splits = self.inner.splits.borrow_mut();
            if let Some(sp) = splits.iter_mut().find(|sp| sp.contains(tab_id)) {
                sp.focused = Some(tab_id);
                true
            } else {
                false
            }
        };
        if !in_split {
            return;
        }
        if self.selected_tab_id() != Some(tab_id) {
            self.select_tab_by_id(tab_id);
        } else {
            self.inner.split_view.set_focused_pane(tab_id);
        }
        if let Some(bv) = self.tab_browser_view(tab_id) {
            bv.grab_focus();
        }
    }

    pub fn focus_other_split_pane(&self) {
        let Some(active_id) = self.selected_tab_id() else {
            return;
        };
        let partner = self
            .inner
            .splits
            .borrow()
            .iter()
            .find(|sp| sp.contains(active_id))
            .and_then(|sp| sp.partner(active_id));
        if let Some(other_id) = partner {
            self.focus_split_pane(other_id);
        }
    }

    pub fn separate_active_split(&self) {
        if let Some(id) = self.selected_tab_id() {
            self.separate_split_for_tab(id);
        }
    }

    pub fn separate_split_for_tab(&self, tab_id: u64) {
        let removed_tabs: Option<Vec<u64>> = {
            let mut splits = self.inner.splits.borrow_mut();
            splits
                .iter()
                .position(|sp| sp.contains(tab_id))
                .map(|idx| splits.remove(idx).tabs)
        };
        if let Some(tab_ids) = removed_tabs {
            self.inner.split_view.detach();
            for tid in tab_ids {
                self.return_tab_widget_to_slot(tid);
            }
            self.sync_split_presentation();
            self.sync_sidebar();
            self.notify_session_dirty();
        }
    }

    pub fn swap_active_split(&self) {
        if let Some(id) = self.selected_tab_id() {
            self.swap_split_for_tab(id);
        }
    }

    pub fn swap_split_for_tab(&self, tab_id: u64) {
        let swapped = {
            let mut splits = self.inner.splits.borrow_mut();
            if let Some(sp) = splits.iter_mut().find(|sp| sp.contains(tab_id)) {
                sp.swap();
                true
            } else {
                false
            }
        };
        if swapped {
            self.sync_split_presentation();
            self.sync_sidebar();
            self.notify_session_dirty();
        }
    }

    pub fn close_active_split_pane(&self) {
        let Some(active_id) = self.selected_tab_id() else {
            return;
        };
        let in_split = self
            .inner
            .splits
            .borrow()
            .iter()
            .any(|sp| sp.contains(active_id));
        if in_split {
            let _ = self.close_tab_by_id(active_id);
        }
    }

    fn update_active_split_ratio(&self, ratio: f64) {
        let Some(active_id) = self.selected_tab_id() else {
            return;
        };
        let changed = {
            let mut splits = self.inner.splits.borrow_mut();
            if let Some(sp) = splits.iter_mut().find(|sp| sp.contains(active_id)) {
                sp.set_primary_fraction(ratio);
                true
            } else {
                false
            }
        };
        if changed {
            self.notify_session_dirty();
        }
    }

    fn remove_tab_from_splits(&self, tab_id: u64) {
        let mut dissolved_tabs = Vec::new();
        {
            let mut splits = self.inner.splits.borrow_mut();
            if let Some(idx) = splits.iter().position(|sp| sp.contains(tab_id)) {
                if !splits[idx].remove(tab_id) {
                    let rem = splits.remove(idx);
                    dissolved_tabs = rem.tabs;
                }
            }
        }
        if !dissolved_tabs.is_empty() {
            self.inner.split_view.detach();
            for tid in dissolved_tabs {
                self.return_tab_widget_to_slot(tid);
            }
        }
    }

    fn return_tab_widget_to_slot(&self, tab_id: u64) {
        if self.inner.pip.active_tab_id() == Some(tab_id) {
            return;
        }
        let tabs = self.inner.tabs.borrow();
        let Some(rec) = tabs.iter().find(|t| t.id == tab_id) else {
            return;
        };
        if let Some(ref bv) = rec.browser {
            bv.widget().unparent();
            rec.slot.append(bv.widget());
        } else if let Some(ref sp) = rec.settings_page {
            sp.widget().unparent();
            rec.slot.append(sp.widget());
        }
    }

    pub fn tab_browser_view(&self, tab_id: u64) -> Option<BrowserView> {
        self.inner
            .tabs
            .borrow()
            .iter()
            .find(|t| t.id == tab_id)
            .and_then(|t| t.browser.clone())
    }

    pub fn tab_summaries(&self) -> Vec<dot_core::bench_proto::TabSummary> {
        self.sync_order_with_tab_view();
        let selected_page = self.inner.tab_view.selected_page();
        self.inner
            .tabs
            .borrow()
            .iter()
            .map(|t| dot_core::bench_proto::TabSummary {
                id: t.id.to_string(),
                title: t.display_title(),
                url: t.url.clone(),
                active: selected_page.as_ref() == Some(&t.page),
                bench: false,
                loading: t.loading,
                asleep: t.browser.is_none() && t.settings_page.is_none(),
                pinned: t.pin.is_some(),
                private: self.inner.is_private,
                group: t.folder_id.clone(),
            })
            .collect()
    }

    pub fn tab_summary(&self, tab_id: u64) -> Option<dot_core::bench_proto::TabSummary> {
        self.tab_summaries()
            .into_iter()
            .find(|s| s.id == tab_id.to_string())
    }

    pub fn pair_split_tabs(&self, dragged_id: u64, with_id: u64, side: &str) -> bool {
        if dragged_id == with_id {
            return false;
        }
        let has_both = {
            let tabs = self.inner.tabs.borrow();
            tabs.iter().any(|t| t.id == dragged_id) && tabs.iter().any(|t| t.id == with_id)
        };
        if !has_both {
            return false;
        }
        self.separate_split_for_tab(dragged_id);
        self.separate_split_for_tab(with_id);

        let (left, right) = if side == "left" {
            (dragged_id, with_id)
        } else {
            (with_id, dragged_id)
        };
        let split_id = self.inner.next_split_id.get();
        self.inner.next_split_id.set(split_id.saturating_add(1));
        let mut split = TabSplit::new_pair(split_id, left, right, 0.5);
        split.focused = Some(dragged_id);
        self.inner.splits.borrow_mut().push(split);

        self.select_tab_by_id(dragged_id);
        self.sync_split_presentation();
        self.sync_sidebar();
        self.notify_session_dirty();
        true
    }

    pub fn set_split_fraction(&self, split_id: u64, fraction: f64) -> bool {
        let changed = {
            let mut splits = self.inner.splits.borrow_mut();
            if let Some(sp) = splits.iter_mut().find(|sp| sp.id == split_id) {
                sp.set_primary_fraction(fraction);
                true
            } else {
                false
            }
        };
        if changed {
            self.sync_split_presentation();
            self.notify_session_dirty();
        }
        changed
    }

    pub fn split_state_json(&self) -> serde_json::Value {
        let splits: Vec<serde_json::Value> = self
            .inner
            .splits
            .borrow()
            .iter()
            .map(|sp| {
                let tabs: Vec<String> = sp.tabs.iter().map(u64::to_string).collect();
                serde_json::json!({
                    "id": sp.id.to_string(),
                    "tabs": tabs,
                    "axis": "horizontal",
                    "sizes": sp.sizes,
                    "focused": sp.focused.map(|id| id.to_string()),
                })
            })
            .collect();
        serde_json::json!({ "splits": splits })
    }

    fn sync_split_presentation(&self) {
        let Some(active_id) = self.selected_tab_id() else {
            self.inner.split_view.detach();
            return;
        };

        let active_split = self
            .inner
            .splits
            .borrow()
            .iter()
            .find(|sp| sp.contains(active_id))
            .cloned();

        let Some(sp) = active_split else {
            self.inner.split_view.detach();
            self.return_tab_widget_to_slot(active_id);
            return;
        };

        let (Some(left_id), Some(right_id)) = (sp.left(), sp.right()) else {
            self.inner.split_view.detach();
            return;
        };

        if self.inner.pip.active_tab_id() == Some(left_id)
            || self.inner.pip.active_tab_id() == Some(right_id)
        {
            self.inner.split_view.detach();
            if self.inner.pip.active_tab_id() != Some(active_id) {
                self.return_tab_widget_to_slot(active_id);
            }
            return;
        }

        let _ = self.ensure_tab_awake(left_id);
        let _ = self.ensure_tab_awake(right_id);

        let (active_slot, left_bv, right_bv) = {
            let tabs = self.inner.tabs.borrow();
            let active_slot = tabs
                .iter()
                .find(|t| t.id == active_id)
                .map(|t| t.slot.clone());
            let left_bv = tabs
                .iter()
                .find(|t| t.id == left_id)
                .and_then(|t| t.browser.clone());
            let right_bv = tabs
                .iter()
                .find(|t| t.id == right_id)
                .and_then(|t| t.browser.clone());
            (active_slot, left_bv, right_bv)
        };

        let (Some(slot), Some(lbv), Some(rbv)) = (active_slot, left_bv, right_bv) else {
            self.inner.split_view.detach();
            return;
        };

        self.inner.split_view.detach();
        slot.append(self.inner.split_view.widget());
        self.inner.split_view.set_panes(
            left_id,
            lbv.widget(),
            right_id,
            rbv.widget(),
            sp.primary_fraction(),
            active_id,
        );
    }

    // ========================================================================
    // Picture-in-Picture (`pip.rs`)
    // ========================================================================

    pub fn toggle_pip_for_active_tab(&self) {
        if self.inner.pip.is_active() {
            self.inner.pip.close(true, false);
            return;
        }
        if let Some(tab_id) = self.selected_tab_id() {
            self.open_pip_for_tab(tab_id);
        }
    }

    pub fn open_pip_for_tab(&self, tab_id: u64) {
        let _ = self.ensure_tab_awake(tab_id);
        let Some(bv) = self.tab_browser_view(tab_id) else {
            return;
        };
        let w = self.clone();
        self.inner.pip.open(
            tab_id,
            bv,
            Some(&self.inner.window),
            move |tid, browser, focus_tab| {
                let tabs = w.inner.tabs.borrow();
                if let Some(rec) = tabs.iter().find(|t| t.id == tid) {
                    browser.widget().unparent();
                    rec.slot.append(browser.widget());
                }
                drop(tabs);
                if focus_tab {
                    w.select_tab_by_id(tid);
                    w.inner.window.present();
                }
                w.sync_split_presentation();
            },
        );
        self.sync_split_presentation();
    }

    // ========================================================================
    // Menú contextual de página (enlaces en Split View + Picture-in-Picture)
    // ========================================================================

    fn show_page_context_menu(&self, tab_id: u64, info: PageContextMenuInfo) {
        if let Some(old) = self.inner.page_context_popover.borrow_mut().take() {
            old.popdown();
            old.unparent();
        }
        let Some(bv) = self.tab_browser_view(tab_id) else {
            return;
        };

        let pop = Popover::new();
        pop.set_parent(bv.widget());
        pop.set_has_arrow(false);
        pop.set_position(PositionType::Bottom);
        pop.set_pointing_to(Some(Rectangle {
            x: info.x.max(0),
            y: info.y.max(0),
            width: 1,
            height: 1,
        }));

        let menu_box = BoxWidget::new(Orientation::Vertical, 2);
        menu_box.set_margins(4, 4, 4, 4);

        let add_btn = |label: &str| {
            let btn = Button::new();
            btn.add_css_class("flat");
            let lbl = Label::new(Some(label));
            lbl.set_halign(Align::Start);
            btn.set_child(Some(&lbl));
            btn
        };

        if let Some(ref link) = info.link_url {
            let open_new = add_btn("Abrir enlace en una pestaña nueva");
            {
                let w = self.clone();
                let u = link.clone();
                let p = pop.clone();
                open_new.connect_clicked(move || {
                    p.popdown();
                    let _ = w.open_tab(Some(&u), true, false, false);
                });
            }
            menu_box.append(&open_new);

            let open_side = add_btn("Abrir enlace al lado");
            {
                let w = self.clone();
                let u = link.clone();
                let p = pop.clone();
                open_side.connect_clicked(move || {
                    p.popdown();
                    w.split_tab_by_id(tab_id, Some(&u));
                });
            }
            menu_box.append(&open_side);

            let peek_btn = add_btn("Vista rápida (Arc Peek)");
            {
                let weak = self.downgrade();
                let u = link.clone();
                let p = pop.clone();
                peek_btn.connect_clicked(move || {
                    p.popdown();
                    if let Some(w) = weak.upgrade() {
                        w.open_peek_url(&u);
                    }
                });
            }
            menu_box.append(&peek_btn);
        }

        if info.is_video {
            let pip_btn = add_btn("Picture-in-Picture");
            {
                let w = self.clone();
                let p = pop.clone();
                pip_btn.connect_clicked(move || {
                    p.popdown();
                    w.open_pip_for_tab(tab_id);
                });
            }
            menu_box.append(&pip_btn);
        }

        if bv.can_go_back() {
            let back_btn = add_btn("Atrás");
            let b = bv.clone();
            let p = pop.clone();
            back_btn.connect_clicked(move || {
                p.popdown();
                b.go_back();
            });
            menu_box.append(&back_btn);
        }

        if bv.can_go_forward() {
            let fwd_btn = add_btn("Adelante");
            let b = bv.clone();
            let p = pop.clone();
            fwd_btn.connect_clicked(move || {
                p.popdown();
                b.go_forward();
            });
            menu_box.append(&fwd_btn);
        }

        let reload_btn = add_btn("Recargar");
        {
            let b = bv.clone();
            let p = pop.clone();
            reload_btn.connect_clicked(move || {
                p.popdown();
                b.reload();
            });
        }
        menu_box.append(&reload_btn);

        let inspect_btn = add_btn("Inspeccionar");
        {
            let b = bv.clone();
            let p = pop.clone();
            inspect_btn.connect_clicked(move || {
                p.popdown();
                b.toggle_dev_tools();
            });
        }
        menu_box.append(&inspect_btn);

        pop.set_child(Some(&menu_box));
        pop.popup();
        *self.inner.page_context_popover.borrow_mut() = Some(pop);
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

fn resolve_user_dir(xdg_var: &str, fallback_subdir: &str) -> std::path::PathBuf {
    if let Some(val) = std::env::var_os(xdg_var) {
        let p = std::path::PathBuf::from(val);
        if !p.as_os_str().is_empty() {
            return p;
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let candidate = std::path::PathBuf::from(&home).join(fallback_subdir);
        if candidate.is_dir() {
            return candidate;
        }
        return std::path::PathBuf::from(home);
    }
    std::path::PathBuf::from(".")
}

fn resolve_pictures_dir() -> std::path::PathBuf {
    if let Some(val) = std::env::var_os("XDG_PICTURES_DIR") {
        let p = std::path::PathBuf::from(val);
        if !p.as_os_str().is_empty() {
            return p;
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home_path = std::path::PathBuf::from(&home);
        for sub in ["Pictures", "Imágenes", "Downloads", "Descargas"] {
            let candidate = home_path.join(sub);
            if candidate.is_dir() {
                return candidate;
            }
        }
        return home_path.join("Pictures");
    }
    resolve_user_dir("XDG_DOWNLOAD_DIR", "Downloads")
}

fn resolve_documents_dir() -> std::path::PathBuf {
    if let Some(val) = std::env::var_os("XDG_DOCUMENTS_DIR") {
        let p = std::path::PathBuf::from(val);
        if !p.as_os_str().is_empty() {
            return p;
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home_path = std::path::PathBuf::from(&home);
        for sub in ["Documents", "Documentos", "Downloads", "Descargas"] {
            let candidate = home_path.join(sub);
            if candidate.is_dir() {
                return candidate;
            }
        }
        return home_path;
    }
    resolve_user_dir("XDG_DOWNLOAD_DIR", "Downloads")
}
