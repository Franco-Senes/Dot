//! Ciclo de vida de `AdwApplication` (`io.github.searchlinux.Search`), gestión de apertura
//! de URLs externas (`HANDLES_OPEN`) y modo de verificación headless `--smoke-test <url>`.

use crate::actions;
use crate::cef_engine::CefEngine;
use crate::settings::SettingsStore;
use crate::style;
use crate::window::BrowserWindow;
use gnome_platform::adw::Application;
use gnome_platform::gio::ApplicationFlags;
use gnome_platform::glib;
use search_core::paths::SearchPaths;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const APP_ID: &str = "io.github.searchlinux.Search";

#[derive(Debug, Clone, Default)]
pub struct CliOptions {
    pub urls: Vec<String>,
    pub smoke_test_url: Option<String>,
}

/// Ejecuta la aplicación GTK4/libadwaita + CEF y devuelve el código de salida del proceso.
pub fn run(
    startup_instant: Instant,
    paths: SearchPaths,
    settings: SettingsStore,
    cli: CliOptions,
) -> i32 {
    let is_smoke_test = cli.smoke_test_url.is_some();
    let flags = if is_smoke_test {
        ApplicationFlags::NON_UNIQUE
    } else {
        ApplicationFlags::HANDLES_OPEN
    };

    let app = Application::new(APP_ID, flags);
    let cef_holder: Rc<RefCell<Option<CefEngine>>> = Rc::new(RefCell::new(None));
    let windows: Rc<RefCell<Vec<BrowserWindow>>> = Rc::new(RefCell::new(Vec::new()));
    let fatal_failed = Arc::new(AtomicBool::new(false));

    // Startup: CSS, tema claro/oscuro, acciones y motor CEF.
    {
        let app_startup = app.clone();
        let settings_startup = settings.clone();
        let cef_slot = cef_holder.clone();
        let paths_startup = paths.clone();
        let failed_startup = fatal_failed.clone();

        app.connect_startup(move || {
            style::install_css();
            let look = settings_startup.with(|s| s.look);
            style::apply_color_look(&app_startup.style_manager(), look);

            {
                let app_theme = app_startup.clone();
                settings_startup.subscribe(move |updated| {
                    style::apply_color_look(&app_theme.style_manager(), updated.look);
                });
            }

            actions::install_app_actions_and_accels(&app_startup);

            let snapshot = settings_startup.snapshot();
            match CefEngine::initialize(&paths_startup, &snapshot) {
                Ok(engine) => {
                    *cef_slot.borrow_mut() = Some(engine);
                }
                Err(err) => {
                    search_core::error!("Fallo crítico al inicializar CEF: {err:#}");
                    failed_startup.store(true, Ordering::SeqCst);
                    app_startup.quit();
                }
            }
        });
    }

    // Activate: abre ventana principal (o ejecuta `--smoke-test <url>`).
    {
        let app_act = app.clone();
        let cef_act = cef_holder.clone();
        let settings_act = settings.clone();
        let wins_act = windows.clone();
        let smoke_opt = cli.smoke_test_url.clone();
        let initial_url_opt = cli.urls.first().cloned();
        let failed_act = fatal_failed.clone();

        app.connect_activate(move || {
            if let Some(existing) = wins_act.borrow().first() {
                existing.present();
                return;
            }

            let Some(ctx) = cef_act.borrow().as_ref().map(CefEngine::persistent_context) else {
                failed_act.store(true, Ordering::SeqCst);
                app_act.quit();
                return;
            };

            let target = smoke_opt.as_deref().or(initial_url_opt.as_deref());
            match BrowserWindow::new(&app_act, ctx, settings_act.clone(), target) {
                Ok(win) => {
                    if let Some(ref smoke_url) = smoke_opt {
                        attach_smoke_test_harness(
                            &app_act,
                            &win,
                            startup_instant,
                            smoke_url.clone(),
                            failed_act.clone(),
                        );
                    }
                    win.present();
                    let startup_ms = startup_instant.elapsed().as_millis();
                    search_core::info!("Ventana principal presentada en {startup_ms} ms");
                    wins_act.borrow_mut().push(win);
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
        let cef_open = cef_holder.clone();
        let settings_open = settings.clone();
        let wins_open = windows.clone();
        let failed_open = fatal_failed.clone();

        app.connect_open(move |uris| {
            let first_uri = uris.first().map(String::as_str);
            if let Some(existing) = wins_open.borrow().first() {
                if let Some(u) = first_uri {
                    existing.navigate_typed_input(u);
                }
                existing.present();
                return;
            }

            let Some(ctx) = cef_open
                .borrow()
                .as_ref()
                .map(CefEngine::persistent_context)
            else {
                failed_open.store(true, Ordering::SeqCst);
                app_open.quit();
                return;
            };

            match BrowserWindow::new(&app_open, ctx, settings_open.clone(), first_uri) {
                Ok(win) => {
                    win.present();
                    wins_open.borrow_mut().push(win);
                }
                Err(err) => {
                    search_core::error!("No se pudo abrir URL externa: {err:#}");
                    failed_open.store(true, Ordering::SeqCst);
                    app_open.quit();
                }
            }
        });
    }

    // Shutdown: destruir ventanas restantes y apagar CEF limpiamente.
    {
        let wins_shut = windows.clone();
        let cef_shut = cef_holder.clone();
        app.connect_shutdown(move || {
            for win in wins_shut.borrow_mut().drain(..) {
                win.browser_view().close_browser();
                win.gtk_window().destroy();
            }
            if let Some(engine) = cef_shut.borrow_mut().take() {
                engine.shutdown();
            }
        });
    }

    // En modo normal pasamos URLs posicionales a `GApplication::run` para disparar `open`
    // (también redirigiendo a la instancia única si ya estaba abierta); en `--smoke-test`
    // pasamos solo `argv[0]` para disparar `activate`.
    let argv0 = std::env::args()
        .next()
        .unwrap_or_else(|| "search".to_string());
    let mut g_args = vec![argv0];
    if !is_smoke_test {
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
        painted: Cell::new(win.browser_view().non_empty_paint_count() > 0),
        completed: Cell::new(false),
        load_ms: Cell::new(0),
        paint_ms: Cell::new(0),
        frame_size: Cell::new((0, 0)),
    });

    let try_finish = {
        let app_done = app.clone();
        let win_done = win.clone();
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
            let frames = win_done.browser_view().non_empty_paint_count();
            let title = win_done.browser_view().title();
            let final_url = win_done.browser_view().current_url();

            search_core::info!(
                "Smoke test completado: total_ms={total_ms} load_ms={load_ms} paint_ms={paint_ms} frames={frames} viewport={fw}x{fh} title=\"{title}\" url=\"{final_url}\""
            );

            let app_quit = app_done.clone();
            let win_close = win_done.clone();
            glib::idle_add_local_once(move || {
                win_close.browser_view().close_browser();
                app_quit.quit();
            });
        }
    };

    win.browser_view().with_callbacks_mut(|cbs| {
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

    // Tiempo límite de seguridad (12 s) para que ninguna prueba headless pueda bloquearse.
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
        win_timeout.browser_view().close_browser();
        app_timeout.quit();
    });
}
