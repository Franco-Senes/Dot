//! Ciclo de vida de `AdwApplication` (`io.github.searchlinux.Search`), gestión multi-ventana
//! (normales y privadas), persistencia atómica de `session.json` con debounce (~1 s) y volcado
//! al cerrar, pila global de pestañas cerradas (`Ctrl+Shift+T`), selector `Ctrl+K` sobre todas
//! las ventanas y modos de verificación headless (`--smoke-test`, `--smoke-test-session-*`).

use crate::actions;
use crate::bookmarks_ui::BookmarksManager;
use crate::browser_view;
use crate::cef_engine::CefEngine;
use crate::downloads::DownloadManager;
use crate::history_ui::HistoryManager;
use crate::settings::SettingsStore;
use crate::style;
use crate::tab_switcher::{self, SwitcherTabItem, TabSwitcherCallbacks};
use crate::vault::VaultService;
use crate::window::{BrowserWindow, SharedDataStores, WindowCallbacks};
use anyhow::{anyhow, Result};
use gnome_platform::adw::Application;
use gnome_platform::gio::ApplicationFlags;
use gnome_platform::glib::{self, SourceId};
use search_core::paths::SearchPaths;
use search_core::session::{ClosedTabStack, SessionState, SessionTab, SessionWindow};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const APP_ID: &str = "io.github.searchlinux.Search";
const SESSION_DEBOUNCE_MS: u64 = 1000;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SmokeMode {
    #[default]
    None,
    SingleUrl(String),
    SessionCreate,
    SessionRestore,
}

#[derive(Debug, Clone, Default)]
pub struct CliOptions {
    pub urls: Vec<String>,
    pub smoke_mode: SmokeMode,
}

#[derive(Clone)]
struct AppController {
    inner: Rc<AppControllerInner>,
}

struct AppControllerInner {
    app: Application,
    paths: SearchPaths,
    settings: SettingsStore,
    stores: SharedDataStores,
    cef: RefCell<Option<CefEngine>>,
    windows: RefCell<Vec<BrowserWindow>>,
    closed_tabs: RefCell<ClosedTabStack>,
    save_debounce: Cell<Option<SourceId>>,
    session_enabled: Cell<bool>,
    shutting_down: Cell<bool>,
}

impl AppController {
    fn new(
        app: Application,
        paths: SearchPaths,
        settings: SettingsStore,
        session_enabled: bool,
    ) -> Self {
        let stores = SharedDataStores {
            history: HistoryManager::load(paths.history_file()),
            bookmarks: BookmarksManager::load(paths.bookmarks_file()),
            downloads: DownloadManager::load(paths.downloads_file(), settings.clone()),
            vault: VaultService::start(paths.clone()),
        };
        Self {
            inner: Rc::new(AppControllerInner {
                app,
                paths,
                settings,
                stores,
                cef: RefCell::new(None),
                windows: RefCell::new(Vec::new()),
                closed_tabs: RefCell::new(ClosedTabStack::default()),
                save_debounce: Cell::new(None),
                session_enabled: Cell::new(session_enabled),
                shutting_down: Cell::new(false),
            }),
        }
    }

    fn set_cef_engine(&self, engine: CefEngine) {
        *self.inner.cef.borrow_mut() = Some(engine);
    }

    fn build_window_callbacks(&self) -> WindowCallbacks {
        let mut cbs = WindowCallbacks::default();
        {
            let ctrl = self.clone();
            cbs.on_session_dirty = Some(Box::new(move || {
                ctrl.schedule_session_save();
            }));
        }
        {
            let ctrl = self.clone();
            cbs.on_tab_closed = Some(Box::new(move |closed| {
                ctrl.inner.closed_tabs.borrow_mut().push(closed);
            }));
        }
        {
            let ctrl = self.clone();
            cbs.on_reopen_closed_tab = Some(Box::new(move |target_win| {
                if let Some(closed) = ctrl.inner.closed_tabs.borrow_mut().pop() {
                    let spec = SessionTab {
                        url: closed.url,
                        title: closed.title,
                        pin: closed.pin_label,
                        name: None,
                        home: closed.home,
                        group_id: None,
                        touched: None,
                        muted: closed.muted,
                    };
                    let _ =
                        target_win.insert_tab_from_spec(spec, Some(closed.position), true, false);
                }
            }));
        }
        {
            let ctrl = self.clone();
            cbs.on_new_window = Some(Box::new(move || {
                if let Ok(win) = ctrl.spawn_window(false, None) {
                    win.present();
                }
            }));
        }
        {
            let ctrl = self.clone();
            cbs.on_new_private_window = Some(Box::new(move || {
                if let Ok(win) = ctrl.spawn_window(true, None) {
                    win.present();
                }
            }));
        }
        {
            let ctrl = self.clone();
            cbs.on_open_tab_switcher = Some(Box::new(move |host_win| {
                ctrl.open_tab_switcher(&host_win);
            }));
        }
        {
            let ctrl = self.clone();
            cbs.on_move_tab_to_new_window = Some(Box::new(move |spec, is_private| {
                if let Ok(win) = ctrl.spawn_window_with_tab(is_private, spec) {
                    win.present();
                }
            }));
        }
        {
            let ctrl = self.clone();
            cbs.on_window_closed = Some(Box::new(move |win_id| {
                // Guardar el estado actual antes de quitar la última ventana normal,
                // o actualizar tras quitar una secundaria si quedan ventanas normales.
                let normal_count = ctrl
                    .inner
                    .windows
                    .borrow()
                    .iter()
                    .filter(|w| !w.is_private())
                    .count();
                let is_closing_normal = ctrl
                    .inner
                    .windows
                    .borrow()
                    .iter()
                    .find(|w| w.id() == win_id)
                    .map(|w| !w.is_private())
                    .unwrap_or(false);

                if is_closing_normal && normal_count == 1 {
                    // Última ventana normal: guardar su sesión antes de perderla.
                    ctrl.save_session_now();
                    ctrl.inner.windows.borrow_mut().retain(|w| w.id() != win_id);
                } else {
                    ctrl.inner.windows.borrow_mut().retain(|w| w.id() != win_id);
                    if is_closing_normal {
                        ctrl.save_session_now();
                    }
                }
            }));
        }
        {
            let ctrl = self.clone();
            cbs.on_list_open_tabs = Some(Box::new(move || {
                ctrl.inner
                    .windows
                    .borrow()
                    .iter()
                    .flat_map(BrowserWindow::open_tab_infos)
                    .collect()
            }));
        }
        {
            let ctrl = self.clone();
            cbs.on_switch_to_tab = Some(Box::new(move |window_id, tab_id| {
                let target = ctrl
                    .inner
                    .windows
                    .borrow()
                    .iter()
                    .find(|w| w.id() == window_id)
                    .cloned();
                if let Some(win) = target {
                    win.select_tab_by_id(tab_id);
                    win.present();
                }
            }));
        }
        {
            let ctrl = self.clone();
            cbs.on_is_any_window_active = Some(Box::new(move || {
                ctrl.inner
                    .windows
                    .borrow()
                    .iter()
                    .any(|w| w.gtk_window().is_active())
            }));
        }
        cbs
    }

    fn spawn_window(&self, is_private: bool, initial_url: Option<&str>) -> Result<BrowserWindow> {
        let ctx = {
            let borrow = self.inner.cef.borrow();
            let engine = borrow
                .as_ref()
                .ok_or_else(|| anyhow!("El motor CEF no está inicializado"))?;
            if is_private {
                engine
                    .create_ephemeral_context()
                    .ok_or_else(|| anyhow!("No se pudo crear RequestContext efímero"))?
            } else {
                engine.persistent_context()
            }
        };

        let win = BrowserWindow::new(
            &self.inner.app,
            ctx,
            self.inner.settings.clone(),
            self.inner.stores.clone(),
            is_private,
            self.build_window_callbacks(),
        )?;
        win.open_tab(initial_url, true, false, false)?;
        self.inner.windows.borrow_mut().push(win.clone());
        if !is_private {
            self.schedule_session_save();
        }
        Ok(win)
    }

    fn spawn_window_with_tab(&self, is_private: bool, spec: SessionTab) -> Result<BrowserWindow> {
        let ctx = {
            let borrow = self.inner.cef.borrow();
            let engine = borrow
                .as_ref()
                .ok_or_else(|| anyhow!("El motor CEF no está inicializado"))?;
            if is_private {
                engine
                    .create_ephemeral_context()
                    .ok_or_else(|| anyhow!("No se pudo crear RequestContext efímero"))?
            } else {
                engine.persistent_context()
            }
        };

        let win = BrowserWindow::new(
            &self.inner.app,
            ctx,
            self.inner.settings.clone(),
            self.inner.stores.clone(),
            is_private,
            self.build_window_callbacks(),
        )?;
        win.insert_tab_from_spec(spec, None, true, false)?;
        self.inner.windows.borrow_mut().push(win.clone());
        if !is_private {
            self.schedule_session_save();
        }
        Ok(win)
    }

    fn spawn_restored_window(&self, saved: &SessionWindow) -> Result<BrowserWindow> {
        let ctx = {
            let borrow = self.inner.cef.borrow();
            let engine = borrow
                .as_ref()
                .ok_or_else(|| anyhow!("El motor CEF no está inicializado"))?;
            engine.persistent_context()
        };

        let win = BrowserWindow::new(
            &self.inner.app,
            ctx,
            self.inner.settings.clone(),
            self.inner.stores.clone(),
            false,
            self.build_window_callbacks(),
        )?;
        win.restore_from_session(saved)?;
        self.inner.windows.borrow_mut().push(win.clone());
        Ok(win)
    }

    fn open_tab_switcher(&self, host_win: &BrowserWindow) {
        let ctrl_list = self.clone();
        let ctrl_act = self.clone();
        let ctrl_close = self.clone();

        let callbacks = TabSwitcherCallbacks {
            list_tabs: Box::new(move || {
                let wins = ctrl_list.inner.windows.borrow();
                let total_wins = wins.len();
                let mut items: Vec<SwitcherTabItem> = Vec::new();
                for (w_idx, win) in wins.iter().enumerate() {
                    let lbl = if total_wins > 1 {
                        Some(format!("Ventana {}", w_idx + 1))
                    } else {
                        None
                    };
                    items.extend(win.switcher_items(lbl));
                }
                items
            }),
            on_activate: Box::new(move |window_id, tab_id| {
                let target = ctrl_act
                    .inner
                    .windows
                    .borrow()
                    .iter()
                    .find(|w| w.id() == window_id)
                    .cloned();
                if let Some(win) = target {
                    win.select_tab_by_id(tab_id);
                    win.present();
                }
            }),
            on_close_tab: Box::new(move |window_id, tab_id| {
                let target = ctrl_close
                    .inner
                    .windows
                    .borrow()
                    .iter()
                    .find(|w| w.id() == window_id)
                    .cloned();
                target.map(|w| w.close_tab_by_id(tab_id)).unwrap_or(false)
            }),
        };

        tab_switcher::present_tab_switcher(host_win.gtk_window(), callbacks);
    }

    fn schedule_session_save(&self) {
        if !self.inner.session_enabled.get() || self.inner.shutting_down.get() {
            return;
        }
        if let Some(prev) = self.inner.save_debounce.take() {
            prev.remove();
        }
        let ctrl = self.clone();
        let src =
            glib::timeout_add_local_once(Duration::from_millis(SESSION_DEBOUNCE_MS), move || {
                ctrl.inner.save_debounce.set(None);
                ctrl.save_session_now();
            });
        self.inner.save_debounce.set(Some(src));
    }

    fn save_session_now(&self) {
        if !self.inner.session_enabled.get() {
            return;
        }
        if let Some(prev) = self.inner.save_debounce.take() {
            prev.remove();
        }
        let normal_windows: Vec<SessionWindow> = self
            .inner
            .windows
            .borrow()
            .iter()
            .filter(|w| !w.is_private())
            .map(BrowserWindow::to_session_window)
            .filter(|sw| !sw.tabs.is_empty())
            .collect();

        if normal_windows.is_empty() {
            return;
        }

        let sidebar = self.inner.settings.with(|s| s.vertical_tabs);
        let state = SessionState::from_windows(normal_windows, sidebar);
        let session_path = self.inner.paths.session_file();
        if let Err(err) = state.save(&session_path) {
            search_core::warn!(
                "No se pudo guardar la sesión en '{}': {err:#}",
                session_path.display()
            );
        }
    }

    fn shutdown(&self) {
        self.inner.shutting_down.set(true);
        self.save_session_now();
        self.inner.stores.history.flush_now();
        self.inner.stores.bookmarks.flush_now();
        self.inner.stores.downloads.flush_now();
        let windows: Vec<BrowserWindow> = self.inner.windows.borrow_mut().drain(..).collect();
        for win in windows {
            win.close_all_browsers();
            win.gtk_window().destroy();
        }
        if let Some(engine) = self.inner.cef.borrow_mut().take() {
            engine.shutdown();
        }
    }
}

/// Ejecuta la aplicación GTK4/libadwaita + CEF y devuelve el código de salida del proceso.
pub fn run(
    startup_instant: Instant,
    paths: SearchPaths,
    settings: SettingsStore,
    cli: CliOptions,
) -> i32 {
    let is_smoke = cli.smoke_mode != SmokeMode::None;
    let flags = if is_smoke {
        ApplicationFlags::NON_UNIQUE
    } else {
        ApplicationFlags::HANDLES_OPEN
    };

    let app = Application::new(APP_ID, flags);
    let session_enabled = !matches!(cli.smoke_mode, SmokeMode::SingleUrl(_));
    let controller = AppController::new(
        app.clone(),
        paths.clone(),
        settings.clone(),
        session_enabled,
    );
    let fatal_failed = Arc::new(AtomicBool::new(false));

    // Startup: CSS, tema claro/oscuro, acciones y motor CEF.
    {
        let app_startup = app.clone();
        let settings_startup = settings.clone();
        let ctrl_startup = controller.clone();
        let paths_startup = paths.clone();
        let failed_startup = fatal_failed.clone();

        app.connect_startup(move || {
            style::install_css();
            let look = settings_startup.with(|s| s.look);
            style::apply_color_look(&app_startup.style_manager(), look);

            {
                let app_theme = app_startup.clone();
                let ctrl_layout = ctrl_startup.clone();
                settings_startup.subscribe(move |updated| {
                    style::apply_color_look(&app_theme.style_manager(), updated.look);
                    ctrl_layout.schedule_session_save();
                });
            }

            actions::install_app_actions_and_accels(&app_startup);

            let snapshot = settings_startup.snapshot();
            match CefEngine::initialize(&paths_startup, &snapshot) {
                Ok(engine) => {
                    ctrl_startup.set_cef_engine(engine);
                }
                Err(err) => {
                    search_core::error!("Fallo crítico al inicializar CEF: {err:#}");
                    failed_startup.store(true, Ordering::SeqCst);
                    app_startup.quit();
                }
            }
        });
    }

    // Activate: restaura sesión previa (con carga diferida) o ejecuta modo smoke test.
    {
        let app_act = app.clone();
        let ctrl_act = controller.clone();
        let paths_act = paths.clone();
        let settings_act = settings.clone();
        let smoke_mode = cli.smoke_mode.clone();
        let cli_urls = cli.urls.clone();
        let failed_act = fatal_failed.clone();

        app.connect_activate(move || {
            if let Some(existing) = ctrl_act.inner.windows.borrow().first() {
                existing.present();
                return;
            }

            match &smoke_mode {
                SmokeMode::SingleUrl(smoke_url) => {
                    match ctrl_act.spawn_window(false, Some(smoke_url)) {
                        Ok(win) => {
                            attach_smoke_test_harness(
                                &app_act,
                                &win,
                                startup_instant,
                                smoke_url.clone(),
                                failed_act.clone(),
                            );
                            win.present();
                            let startup_ms = startup_instant.elapsed().as_millis();
                            search_core::info!("Ventana principal presentada en {startup_ms} ms");
                        }
                        Err(err) => {
                            search_core::error!("No se pudo crear ventana smoke test: {err:#}");
                            failed_act.store(true, Ordering::SeqCst);
                            app_act.quit();
                        }
                    }
                    return;
                }
                SmokeMode::SessionCreate => {
                    if let Err(err) = run_smoke_session_create(&app_act, &ctrl_act, startup_instant) {
                        search_core::error!("Error en --smoke-test-session-create: {err:#}");
                        failed_act.store(true, Ordering::SeqCst);
                        app_act.quit();
                    }
                    return;
                }
                SmokeMode::SessionRestore => {
                    if let Err(err) = run_smoke_session_restore(
                        &app_act,
                        &ctrl_act,
                        &paths_act,
                        startup_instant,
                        failed_act.clone(),
                    ) {
                        search_core::error!("Error en --smoke-test-session-restore: {err:#}");
                        failed_act.store(true, Ordering::SeqCst);
                        app_act.quit();
                    }
                    return;
                }
                SmokeMode::None => {}
            }

            // Arranque normal: restaurar `session.json` con carga diferida.
            let saved = SessionState::load(&paths_act.session_file());
            if !saved.is_empty() {
                if saved.sidebar != settings_act.with(|s| s.vertical_tabs) {
                    settings_act.update(|s| s.vertical_tabs = saved.sidebar);
                }
                let restored_windows = saved.to_windows();
                for sw in &restored_windows {
                    match ctrl_act.spawn_restored_window(sw) {
                        Ok(win) => win.present(),
                        Err(err) => {
                            search_core::error!("Error restaurando ventana de sesión: {err:#}");
                        }
                    }
                }
                if !cli_urls.is_empty() {
                    if let Some(first) = ctrl_act.inner.windows.borrow().first().cloned() {
                        for u in &cli_urls {
                            let _ = first.open_tab(Some(u), true, false, false);
                        }
                    }
                }
                if !ctrl_act.inner.windows.borrow().is_empty() {
                    let startup_ms = startup_instant.elapsed().as_millis();
                    search_core::info!(
                        "Sesión restaurada ({} ventanas, {} navegadores CEF activos) en {startup_ms} ms",
                        ctrl_act.inner.windows.borrow().len(),
                        browser_view::active_browser_count()
                    );
                    return;
                }
            }

            let first_url = cli_urls.first().map(String::as_str);
            match ctrl_act.spawn_window(false, first_url) {
                Ok(win) => {
                    for extra in cli_urls.iter().skip(1) {
                        let _ = win.open_tab(Some(extra), false, false, true);
                    }
                    win.present();
                    let startup_ms = startup_instant.elapsed().as_millis();
                    search_core::info!("Ventana principal presentada en {startup_ms} ms");
                }
                Err(err) => {
                    search_core::error!("No se pudo crear la ventana principal: {err:#}");
                    failed_act.store(true, Ordering::SeqCst);
                    app_act.quit();
                }
            }
        });
    }

    // Open: cuando se invoca `search <url>` desde otra instancia o manejador XDG.
    {
        let app_open = app.clone();
        let ctrl_open = controller.clone();
        let failed_open = fatal_failed.clone();

        app.connect_open(move |uris| {
            let existing = ctrl_open
                .inner
                .windows
                .borrow()
                .iter()
                .find(|w| !w.is_private())
                .cloned()
                .or_else(|| ctrl_open.inner.windows.borrow().first().cloned());

            if let Some(win) = existing {
                for uri in &uris {
                    let _ = win.open_tab(Some(uri), true, false, false);
                }
                win.present();
                return;
            }

            let first = uris.first().map(String::as_str);
            match ctrl_open.spawn_window(false, first) {
                Ok(win) => {
                    for extra in uris.iter().skip(1) {
                        let _ = win.open_tab(Some(extra), false, false, true);
                    }
                    win.present();
                }
                Err(err) => {
                    search_core::error!("No se pudo abrir URL externa: {err:#}");
                    failed_open.store(true, Ordering::SeqCst);
                    app_open.quit();
                }
            }
        });
    }

    // Shutdown: guardar sesión atómicamente, destruir ventanas y apagar CEF limpiamente.
    {
        let ctrl_shut = controller.clone();
        app.connect_shutdown(move || {
            ctrl_shut.shutdown();
        });
    }

    let argv0 = std::env::args()
        .next()
        .unwrap_or_else(|| "search".to_string());
    let mut g_args = vec![argv0];
    if !is_smoke {
        g_args.extend(cli.urls);
    }

    let status = app.run_with_args(&g_args);
    if fatal_failed.load(Ordering::SeqCst) {
        1
    } else {
        status
    }
}

fn attach_smoke_test_harness(
    app: &Application,
    win: &BrowserWindow,
    startup_instant: Instant,
    smoke_url: String,
    fatal_failed: Arc<AtomicBool>,
) {
    let Some(bv) = win.active_browser_view() else {
        fatal_failed.store(true, Ordering::SeqCst);
        app.quit();
        return;
    };

    struct SmokeProgress {
        loaded: Cell<bool>,
        painted: Cell<bool>,
        completed: Cell<bool>,
        load_ms: Cell<u128>,
        paint_ms: Cell<u128>,
        frame_size: Cell<(i32, i32)>,
    }

    let progress = Rc::new(SmokeProgress {
        loaded: Cell::new(false),
        painted: Cell::new(bv.non_empty_paint_count() > 0),
        completed: Cell::new(false),
        load_ms: Cell::new(0),
        paint_ms: Cell::new(0),
        frame_size: Cell::new((0, 0)),
    });

    let try_finish = {
        let app_done = app.clone();
        let win_done = win.clone();
        let bv_done = bv.clone();
        let prog = progress.clone();
        move || {
            if prog.completed.get() || !prog.loaded.get() || !prog.painted.get() {
                return;
            }
            prog.completed.set(true);
            let total_ms = startup_instant.elapsed().as_millis();
            let load_ms = prog.load_ms.get();
            let paint_ms = prog.paint_ms.get();
            let (fw, fh) = prog.frame_size.get();
            let frames = bv_done.non_empty_paint_count();
            let title = bv_done.title();
            let final_url = bv_done.current_url();

            search_core::info!(
                "Smoke test completado: total_ms={total_ms} load_ms={load_ms} paint_ms={paint_ms} frames={frames} viewport={fw}x{fh} title=\"{title}\" url=\"{final_url}\""
            );

            let app_quit = app_done.clone();
            let win_close = win_done.clone();
            glib::idle_add_local_once(move || {
                win_close.close_all_browsers();
                app_quit.quit();
            });
        }
    };

    bv.with_callbacks_mut(|cbs| {
        let prog_load = progress.clone();
        let finish_load = try_finish.clone();
        cbs.on_load_finished = Some(Box::new(move |_url, _status| {
            if !prog_load.loaded.replace(true) {
                prog_load.load_ms.set(startup_instant.elapsed().as_millis());
            }
            finish_load();
        }));

        let prog_paint = progress.clone();
        let finish_paint = try_finish.clone();
        cbs.on_non_empty_paint = Some(Box::new(move |_count, w, h| {
            if !prog_paint.painted.replace(true) {
                prog_paint
                    .paint_ms
                    .set(startup_instant.elapsed().as_millis());
            }
            prog_paint.frame_size.set((w, h));
            finish_paint();
        }));
    });

    let prog_timeout = progress;
    let app_timeout = app.clone();
    let win_timeout = win.clone();
    glib::timeout_add_local_once(Duration::from_secs(12), move || {
        if prog_timeout.completed.replace(true) {
            return;
        }
        search_core::error!(
            "Tiempo agotado en --smoke-test para '{}': loaded={} painted={}",
            smoke_url,
            prog_timeout.loaded.get(),
            prog_timeout.painted.get()
        );
        fatal_failed.store(true, Ordering::SeqCst);
        win_timeout.close_all_browsers();
        app_timeout.quit();
    });
}

/// Crea 15 pestañas (2 fijadas + 13 regulares), guarda `session.json` y sale con código 0.
fn run_smoke_session_create(
    app: &Application,
    ctrl: &AppController,
    startup_instant: Instant,
) -> Result<()> {
    let win = ctrl.spawn_window(
        false,
        Some("data:text/html,<title>Pin 1</title><h1>Pinned 1</h1>"),
    )?;
    // Fijar la primera pestaña y abrir la segunda fijada.
    if let Some(first_id) = win.switcher_items(None).first().map(|t| t.tab_id) {
        win.toggle_pin_tab(first_id);
    }
    win.open_tab(
        Some("data:text/html,<title>Pin 2</title><h1>Pinned 2</h1>"),
        false,
        true,
        true,
    )?;

    for i in 3..=15 {
        let url = format!("data:text/html,<title>Pestana {i}</title><h1>Tab {i}</h1>");
        // Abrir la pestaña 7 como activa; el resto diferidas para rapidez.
        let select = i == 7;
        win.open_tab(Some(&url), select, false, !select)?;
    }

    if win.tab_count() != 15 || win.pinned_count() != 2 {
        return Err(anyhow!(
            "Se esperaban 15 pestañas (2 fijadas), obtenidas {} ({} fijadas)",
            win.tab_count(),
            win.pinned_count()
        ));
    }

    // Verificar además que Ctrl+W sobre una pestaña fijada NO la cierra.
    win.select_tab_shortcut(1);
    win.close_active_tab();
    if win.tab_count() != 15 || win.pinned_count() != 2 {
        return Err(anyhow!(
            "Ctrl+W cerró una pestaña fijada: quedaron {} ({} fijadas)",
            win.tab_count(),
            win.pinned_count()
        ));
    }
    // Volver a seleccionar la pestaña 7 como activa para la persistencia.
    win.select_tab_shortcut(7);

    ctrl.save_session_now();
    let elapsed_ms = startup_instant.elapsed().as_millis();
    search_core::info!(
        "Smoke session-create completado: tabs={} pinned={} elapsed_ms={elapsed_ms}",
        win.tab_count(),
        win.pinned_count()
    );

    let app_quit = app.clone();
    let win_close = win.clone();
    glib::idle_add_local_once(move || {
        win_close.close_all_browsers();
        app_quit.quit();
    });
    Ok(())
}

/// Restaura `session.json` (15 pestañas, 2 fijadas), verifica que SOLO 1 `cef::Browser`
/// está instanciado (carga diferida) y espera al primer fotograma OSR de la pestaña activa.
fn run_smoke_session_restore(
    app: &Application,
    ctrl: &AppController,
    paths: &SearchPaths,
    startup_instant: Instant,
    fatal_failed: Arc<AtomicBool>,
) -> Result<()> {
    let saved = SessionState::load(&paths.session_file());
    let windows_spec = saved.to_windows();
    let first_spec = windows_spec
        .first()
        .ok_or_else(|| anyhow!("session.json está vacío al intentar restaurar"))?;

    let win = ctrl.spawn_restored_window(first_spec)?;
    win.present();
    let present_ms = startup_instant.elapsed().as_millis();

    let total_tabs = win.tab_count();
    let pinned_tabs = win.pinned_count();
    let awake_browsers = browser_view::active_browser_count();

    if total_tabs != 15 || pinned_tabs != 2 || awake_browsers != 1 {
        return Err(anyhow!(
            "Fallo de restauración diferida: tabs={total_tabs} (esperado 15), pinned={pinned_tabs} (esperado 2), awake_browsers={awake_browsers} (esperado 1)"
        ));
    }

    search_core::info!(
        "Restauración de sesión verificada al presentar ({present_ms} ms): tabs={total_tabs} pinned={pinned_tabs} awake_browsers={awake_browsers}"
    );

    attach_smoke_test_harness(
        app,
        &win,
        startup_instant,
        "session-restore-15-tabs".to_string(),
        fatal_failed,
    );
    Ok(())
}
