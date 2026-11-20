//! Registro centralizado de acciones GIO (`app.*` y `win.*`), aceleradores de teclado con `Ctrl`
//! y diálogo de atajos (`AdwShortcutsDialog`, `Ctrl+?`) — `Shortcuts.swift` + `ShortcutsPage.swift`.

use crate::settings::SettingsStore;
use crate::window::BrowserWindow;
use dot_core::address::clean_link;
use gnome_platform::adw::{
    Application, ApplicationWindow, ShortcutsDialog, ShortcutsItem, ShortcutsSection,
};
use gnome_platform::gdk::Display;
use gnome_platform::gio::SimpleAction;
use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Registra las acciones globales de aplicación y declara todos los atajos de teclado.
pub fn install_app_actions_and_accels(app: &Application, settings: &SettingsStore) {
    let quit_action = SimpleAction::new("quit");
    let app_quit = app.clone();
    let settings_quit = settings.clone();
    let last_quit_press: Rc<Cell<Option<Instant>>> = Rc::new(Cell::new(None));
    quit_action.connect_activate(move || {
        if settings_quit.with(|s| s.confirm_quit) {
            let now = Instant::now();
            let confirmed = last_quit_press
                .get()
                .map(|prev| now.duration_since(prev) <= Duration::from_secs(3))
                .unwrap_or(false);
            if !confirmed {
                last_quit_press.set(Some(now));
                dot_core::info!("Pulsa Ctrl+Q de nuevo en los próximos 3 s para confirmar salida");
                return;
            }
        }
        app_quit.quit();
    });
    app.add_action(&quit_action);

    // Fase 1 — Navegación, búsqueda, zoom y aplicación.
    app.set_accels_for_action("app.quit", &["<Control>q"]);
    app.set_accels_for_action("win.focus-address", &["<Control>l", "F6", "<Alt>d"]);
    app.set_accels_for_action("win.reload", &["<Control>r", "F5"]);
    app.set_accels_for_action("win.reload-ignore-cache", &["<Control><Shift>r"]);
    app.set_accels_for_action("win.stop-or-escape", &["Escape"]);
    app.set_accels_for_action("win.go-back", &["<Alt>Left", "<Control>bracketleft"]);
    app.set_accels_for_action("win.go-forward", &["<Alt>Right", "<Control>bracketright"]);
    app.set_accels_for_action("win.find", &["<Control>f"]);
    app.set_accels_for_action("win.find-next", &["<Control>g"]);
    app.set_accels_for_action("win.find-prev", &["<Control><Shift>g"]);
    app.set_accels_for_action(
        "win.zoom-in",
        &["<Control>equal", "<Control>plus", "<Control>KP_Add"],
    );
    app.set_accels_for_action("win.zoom-out", &["<Control>minus", "<Control>KP_Subtract"]);
    app.set_accels_for_action("win.zoom-reset", &["<Control>0", "<Control>KP_0"]);
    app.set_accels_for_action("win.dev-tools", &["<Control><Shift>i", "F12"]);
    app.set_accels_for_action("win.copy-clean-url", &["<Control><Shift>c"]);
    app.set_accels_for_action("win.paste-and-go", &["<Control><Shift>v"]);
    app.set_accels_for_action("win.toggle-fullscreen", &["F11"]);

    // Fase 2 — Pestañas, selector difuso, ventanas, disposición y preferencias.
    app.set_accels_for_action("win.new-tab", &["<Control>t"]);
    app.set_accels_for_action("win.close-tab", &["<Control>w"]);
    app.set_accels_for_action("win.reopen-tab", &["<Control><Shift>t"]);
    app.set_accels_for_action("win.next-tab", &["<Control>Tab", "<Control>Page_Down"]);
    app.set_accels_for_action(
        "win.prev-tab",
        &[
            "<Control><Shift>Tab",
            "<Control><Shift>ISO_Left_Tab",
            "<Control>Page_Up",
        ],
    );
    app.set_accels_for_action("win.tab-1", &["<Control>1", "<Alt>1"]);
    app.set_accels_for_action("win.tab-2", &["<Control>2", "<Alt>2"]);
    app.set_accels_for_action("win.tab-3", &["<Control>3", "<Alt>3"]);
    app.set_accels_for_action("win.tab-4", &["<Control>4", "<Alt>4"]);
    app.set_accels_for_action("win.tab-5", &["<Control>5", "<Alt>5"]);
    app.set_accels_for_action("win.tab-6", &["<Control>6", "<Alt>6"]);
    app.set_accels_for_action("win.tab-7", &["<Control>7", "<Alt>7"]);
    app.set_accels_for_action("win.tab-8", &["<Control>8", "<Alt>8"]);
    app.set_accels_for_action("win.tab-last", &["<Control>9", "<Alt>9"]);
    app.set_accels_for_action("win.tab-switcher", &["<Control>k"]);
    app.set_accels_for_action("win.new-window", &["<Control>n"]);
    app.set_accels_for_action("win.new-private-window", &["<Control><Shift>n"]);
    app.set_accels_for_action("win.toggle-tab-layout", &["<Control><Shift>s"]);
    app.set_accels_for_action("win.toggle-fold-tabs", &["<Control>s"]);
    app.set_accels_for_action(
        "win.duplicate-tab",
        &["<Control><Shift>d", "<Control><Alt>d"],
    );
    app.set_accels_for_action("win.toggle-mute", &["<Control>m", "<Control><Shift>m"]);
    app.set_accels_for_action("win.view-source", &["<Control>u"]);
    app.set_accels_for_action("win.print-pdf", &["<Control>p"]);
    app.set_accels_for_action("win.screenshot", &["<Control><Shift>x"]);
    app.set_accels_for_action("win.toggle-force-dark", &["<Alt><Shift>d"]);
    app.set_accels_for_action("win.open-settings", &["<Control>comma"]);
    app.set_accels_for_action(
        "win.open-shortcuts",
        &["<Control>question", "<Control><Shift>slash"],
    );

    // Fase 3 — Historial, marcadores y descargas.
    app.set_accels_for_action("win.toggle-history", &["<Control>h", "<Control>y"]);
    app.set_accels_for_action("win.bookmark-page", &["<Control>d", "<Control><Shift>b"]);
    app.set_accels_for_action("win.toggle-bookmarks", &["<Control><Shift>o"]);
    app.set_accels_for_action(
        "win.toggle-downloads",
        &["<Control><Shift>y", "<Control><Shift>j"],
    );

    // Fase 4 — Shield, Curtain y modo lectura.
    app.set_accels_for_action("win.toggle-reader", &["<Control><Alt>r", "F9"]);
    app.set_accels_for_action("win.curtain-pick", &["<Control><Shift>h"]);
    app.set_accels_for_action("win.toggle-curtain", &["<Control><Shift>e"]);

    // Fase 5 — Contraseñas (Vault).
    app.set_accels_for_action("win.open-passwords", &["<Control><Alt>l"]);

    // Fase 6 — Split View y Picture-in-Picture.
    app.set_accels_for_action("win.split-tab", &["<Control><Shift>backslash"]);
    app.set_accels_for_action("win.split-focus-other", &["<Control><Alt>backslash"]);
    app.set_accels_for_action("win.split-separate", &["<Control><Alt>s"]);
    app.set_accels_for_action("win.toggle-pip", &["<Control><Shift>p"]);

    // Fase 7 — Arc Boosts, Arc Peek y Little Arc.
    app.set_accels_for_action("win.boost-site", &["<Control><Alt>b"]);
    app.set_accels_for_action("win.little-arc", &["<Control><Alt>n"]);
}

/// Registra todas las acciones de ventana (`win.*`) sobre `gtk_win`.
pub fn install_window_actions(gtk_win: &ApplicationWindow, win: &BrowserWindow) {
    // Fase 1.
    add_win_action(gtk_win, "focus-address", win, |w| w.focus_address_bar());

    add_win_action(gtk_win, "reload", win, |w| {
        if let Some(bv) = w.ensure_active_browser_view() {
            bv.reload();
        }
    });

    add_win_action(gtk_win, "reload-ignore-cache", win, |w| {
        if let Some(bv) = w.ensure_active_browser_view() {
            bv.reload_ignore_cache();
            w.show_toast("Recargando sin caché", Some(1600));
        }
    });

    add_win_action(gtk_win, "stop-or-escape", win, |w| w.handle_escape());

    add_win_action(gtk_win, "go-back", win, |w| {
        if let Some(bv) = w.active_browser_view() {
            bv.go_back();
        }
    });

    add_win_action(gtk_win, "go-forward", win, |w| {
        if let Some(bv) = w.active_browser_view() {
            bv.go_forward();
        }
    });

    add_win_action(gtk_win, "find", win, |w| w.open_find_bar());

    add_win_action(gtk_win, "find-next", win, |w| w.find_bar().find_next());

    add_win_action(gtk_win, "find-prev", win, |w| w.find_bar().find_previous());

    add_win_action(gtk_win, "zoom-in", win, |w| w.zoom_in());

    add_win_action(gtk_win, "zoom-out", win, |w| w.zoom_out());

    add_win_action(gtk_win, "zoom-reset", win, |w| w.zoom_reset());

    add_win_action(gtk_win, "dev-tools", win, |w| {
        if let Some(bv) = w.ensure_active_browser_view() {
            bv.toggle_dev_tools();
        }
    });

    add_win_action(gtk_win, "copy-clean-url", win, |w| {
        if let Some(bv) = w.active_browser_view() {
            let current = bv.current_url();
            if !current.is_empty() && current != "about:blank" {
                let cleaned = clean_link(&current);
                if let Some(display) = Display::default_display() {
                    display.set_clipboard_text(&cleaned);
                    w.show_toast("Enlace limpio copiado", Some(1800));
                }
            }
        }
    });

    add_win_action(gtk_win, "paste-and-go", win, |w| {
        if let Some(display) = Display::default_display() {
            let w_async = w.downgrade();
            display.read_clipboard_text_async(move |opt_text| {
                if let Some(text) = opt_text {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        if let Some(w_live) = w_async.upgrade() {
                            w_live.navigate_typed_input(trimmed);
                        }
                    }
                }
            });
        }
    });

    add_win_action(gtk_win, "toggle-fullscreen", win, |w| w.toggle_fullscreen());

    // Fase 2.
    add_win_action(gtk_win, "new-tab", win, |w| {
        let _ = w.open_tab(None, true, false, false);
    });

    add_win_action(gtk_win, "close-tab", win, |w| w.close_active_tab());

    add_win_action(gtk_win, "reopen-tab", win, |w| w.reopen_closed_tab());

    add_win_action(gtk_win, "next-tab", win, |w| w.select_next_tab());

    add_win_action(gtk_win, "prev-tab", win, |w| w.select_previous_tab());

    for idx in 1..=8 {
        let name = format!("tab-{idx}");
        add_win_action(gtk_win, &name, win, move |w| w.select_tab_shortcut(idx));
    }

    add_win_action(gtk_win, "tab-last", win, |w| w.select_tab_shortcut(9));

    add_win_action(gtk_win, "tab-switcher", win, |w| w.open_tab_switcher());

    add_win_action(gtk_win, "new-window", win, |w| w.request_new_window());

    add_win_action(gtk_win, "new-private-window", win, |w| {
        w.request_new_private_window()
    });

    add_win_action(gtk_win, "toggle-tab-layout", win, |w| w.toggle_tab_layout());

    add_win_action(gtk_win, "toggle-fold-tabs", win, |w| w.toggle_fold_tabs());

    add_win_action(gtk_win, "duplicate-tab", win, |w| w.duplicate_active_tab());

    add_win_action(gtk_win, "toggle-mute", win, |w| w.toggle_mute_active_tab());

    add_win_action(gtk_win, "open-settings", win, |w| w.open_settings_tab(None));

    add_win_action(gtk_win, "open-shortcuts", win, |w| {
        present_shortcuts_dialog(w.gtk_window())
    });

    // Fase 3.
    add_win_action(gtk_win, "toggle-history", win, |w| w.toggle_history_panel());

    add_win_action(gtk_win, "bookmark-page", win, |w| w.open_bookmark_popover());

    add_win_action(gtk_win, "toggle-bookmarks", win, |w| {
        w.toggle_bookmarks_panel()
    });

    add_win_action(gtk_win, "toggle-downloads", win, |w| {
        w.toggle_downloads_panel()
    });

    // Fase 4.
    add_win_action(gtk_win, "toggle-reader", win, |w| w.toggle_reader_mode());

    add_win_action(gtk_win, "curtain-pick", win, |w| w.toggle_curtain_picker());

    add_win_action(gtk_win, "toggle-curtain", win, |w| w.toggle_curtain_panel());

    // Fase 5.
    add_win_action(gtk_win, "open-passwords", win, |w| w.open_passwords_panel());

    // Fase 6.
    add_win_action(gtk_win, "split-tab", win, |w| w.split_current_tab(None));

    add_win_action(gtk_win, "split-focus-other", win, |w| {
        w.focus_other_split_pane()
    });

    add_win_action(gtk_win, "split-separate", win, |w| {
        w.separate_active_split()
    });

    add_win_action(gtk_win, "split-swap", win, |w| w.swap_active_split());

    add_win_action(gtk_win, "split-close-pane", win, |w| {
        w.close_active_split_pane()
    });

    add_win_action(gtk_win, "toggle-pip", win, |w| {
        w.toggle_pip_for_active_tab()
    });

    add_win_action(gtk_win, "view-source", win, |w| w.view_active_tab_source());

    add_win_action(gtk_win, "print-pdf", win, |w| w.export_active_tab_pdf());

    add_win_action(gtk_win, "screenshot", win, |w| {
        w.capture_active_tab_screenshot()
    });

    add_win_action(gtk_win, "toggle-force-dark", win, |w| {
        w.toggle_force_dark_active_tab()
    });

    // Fase 7 — Arc Boosts, Arc Peek y Little Arc.
    add_win_action(gtk_win, "boost-site", win, |w| {
        w.open_boost_for_active_tab()
    });
    add_win_action(gtk_win, "peek-url", win, |w| w.open_peek_for_active_tab());
    add_win_action(gtk_win, "little-arc", win, |w| w.open_little_arc());
}

/// Muestra el diálogo nativo `AdwShortcutsDialog` (`Ctrl+?`) con todos los atajos activos.
pub fn present_shortcuts_dialog(parent: &ApplicationWindow) {
    let dialog = ShortcutsDialog::new();

    let tabs_sec = ShortcutsSection::new("Pestañas y ventanas");
    for (title, accel) in [
        ("Nueva pestaña", "<Control>t"),
        ("Cerrar pestaña (protege fijadas)", "<Control>w"),
        ("Reabrir última pestaña cerrada", "<Control><Shift>t"),
        ("Selector difuso de pestañas abiertas", "<Control>k"),
        ("Pestaña siguiente", "<Control>Tab <Control>Page_Down"),
        ("Pestaña anterior", "<Control><Shift>Tab <Control>Page_Up"),
        (
            "Ir a pestaña 1…8",
            "<Control>1...<Control>8 <Alt>1...<Alt>8",
        ),
        ("Ir a la última pestaña", "<Control>9 <Alt>9"),
        ("Duplicar pestaña", "<Control><Shift>d <Control><Alt>d"),
        ("Dividir pestaña (Split View)", "<Control><Shift>backslash"),
        ("Cambiar foco entre paneles", "<Control><Alt>backslash"),
        ("Separar paneles divididos", "<Control><Alt>s"),
        ("Picture-in-Picture", "<Control><Shift>p"),
        ("Silenciar o activar sonido", "<Control>m <Control><Shift>m"),
        (
            "Alternar pestañas arriba / barra lateral",
            "<Control><Shift>s",
        ),
        ("Plegar o mostrar barra de pestañas", "<Control>s"),
        ("Nueva ventana", "<Control>n"),
        ("Nueva ventana privada", "<Control><Shift>n"),
    ] {
        tabs_sec.add(&ShortcutsItem::new(title, accel));
    }
    dialog.add(&tabs_sec);

    let nav_sec = ShortcutsSection::new("Navegación");
    for (title, accel) in [
        (
            "Enfocar barra de dirección y búsqueda",
            "<Control>l F6 <Alt>d",
        ),
        ("Atrás", "<Alt>Left <Control>bracketleft"),
        ("Adelante", "<Alt>Right <Control>bracketright"),
        ("Recargar página", "<Control>r F5"),
        ("Recargar sin caché", "<Control><Shift>r"),
        ("Detener carga / cerrar barra", "Escape"),
        ("Copiar dirección limpia", "<Control><Shift>c"),
        ("Pegar e ir", "<Control><Shift>v"),
    ] {
        nav_sec.add(&ShortcutsItem::new(title, accel));
    }
    dialog.add(&nav_sec);

    let data_sec = ShortcutsSection::new("Historial, marcadores y descargas");
    for (title, accel) in [
        ("Panel de historial", "<Control>h <Control>y"),
        (
            "Añadir o editar marcador de la página",
            "<Control>d <Control><Shift>b",
        ),
        ("Panel de marcadores", "<Control><Shift>o"),
        ("Panel de descargas", "<Control><Shift>y <Control><Shift>j"),
        ("Gestor de contraseñas", "<Control><Alt>l"),
    ] {
        data_sec.add(&ShortcutsItem::new(title, accel));
    }
    dialog.add(&data_sec);

    let page_sec = ShortcutsSection::new("Página y aplicación");
    for (title, accel) in [
        ("Buscar en la página", "<Control>f"),
        (
            "Coincidencia siguiente / anterior",
            "<Control>g <Control><Shift>g",
        ),
        (
            "Acercar / alejar / restablecer zoom",
            "<Control>plus <Control>minus <Control>0",
        ),
        ("Pantalla completa", "F11"),
        ("Modo lectura", "<Control><Alt>r F9"),
        ("Curtain: ocultar elemento", "<Control><Shift>h"),
        ("Curtain: ver elementos ocultos", "<Control><Shift>e"),
        ("Inspector web (DevTools)", "<Control><Shift>i F12"),
        ("Ver código fuente", "<Control>u"),
        ("Exportar página a PDF", "<Control>p"),
        ("Captura de pantalla PNG", "<Control><Shift>x"),
        ("Forzar modo oscuro CSS", "<Alt><Shift>d"),
        ("Arc Boosts (personalizar sitio)", "<Control><Alt>b"),
        ("Little Arc (ventana rápida)", "<Control><Alt>n"),
        ("Ajustes (plasma://settings)", "<Control>comma"),
        ("Atajos de teclado", "<Control>question"),
        ("Salir de Dot", "<Control>q"),
    ] {
        page_sec.add(&ShortcutsItem::new(title, accel));
    }
    dialog.add(&page_sec);

    dialog.present(Some(parent));
}

fn add_win_action(
    gtk_win: &ApplicationWindow,
    name: &str,
    win: &BrowserWindow,
    cb: impl Fn(&BrowserWindow) + 'static,
) {
    let weak = win.downgrade();
    let action = SimpleAction::new(name);
    action.connect_activate(move || {
        if let Some(w) = weak.upgrade() {
            cb(&w);
        }
    });
    gtk_win.add_action(&action);
}
