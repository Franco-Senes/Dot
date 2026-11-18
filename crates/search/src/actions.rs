//! Registro centralizado de acciones GIO (`app.*` y `win.*`), aceleradores de teclado con `Ctrl`
//! y diálogo de atajos (`AdwShortcutsDialog`, `Ctrl+?`) — `Shortcuts.swift` + `ShortcutsPage.swift`.

use crate::settings_ui;
use crate::window::BrowserWindow;
use gnome_platform::adw::{
    Application, ApplicationWindow, ShortcutsDialog, ShortcutsItem, ShortcutsSection,
};
use gnome_platform::gdk::Display;
use gnome_platform::gio::SimpleAction;
use search_core::address::clean_link;

/// Registra las acciones globales de aplicación y declara todos los atajos de teclado de Fases 1 y 2.
pub fn install_app_actions_and_accels(app: &Application) {
    let quit_action = SimpleAction::new("quit");
    let app_quit = app.clone();
    quit_action.connect_activate(move || {
        app_quit.quit();
    });
    app.add_action(&quit_action);

    // Fase 1 — Navegación, búsqueda, zoom y aplicación.
    app.set_accels_for_action("app.quit", &["<Control>q"]);
    app.set_accels_for_action("win.focus-address", &["<Control>l", "F6", "<Alt>d"]);
    app.set_accels_for_action("win.reload", &["<Control>r", "F5"]);
    app.set_accels_for_action(
        "win.reload-ignore-cache",
        &["<Control><Shift>r", "<Control><Alt>r"],
    );
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
    app.set_accels_for_action("win.tab-1", &["<Control>1"]);
    app.set_accels_for_action("win.tab-2", &["<Control>2"]);
    app.set_accels_for_action("win.tab-3", &["<Control>3"]);
    app.set_accels_for_action("win.tab-4", &["<Control>4"]);
    app.set_accels_for_action("win.tab-5", &["<Control>5"]);
    app.set_accels_for_action("win.tab-6", &["<Control>6"]);
    app.set_accels_for_action("win.tab-7", &["<Control>7"]);
    app.set_accels_for_action("win.tab-8", &["<Control>8"]);
    app.set_accels_for_action("win.tab-last", &["<Control>9"]);
    app.set_accels_for_action("win.tab-switcher", &["<Control>k"]);
    app.set_accels_for_action("win.new-window", &["<Control>n"]);
    app.set_accels_for_action("win.new-private-window", &["<Control><Shift>n"]);
    app.set_accels_for_action("win.toggle-tab-layout", &["<Control><Shift>s"]);
    app.set_accels_for_action("win.toggle-fold-tabs", &["<Control>s"]);
    app.set_accels_for_action("win.duplicate-tab", &["<Control><Alt>d"]);
    app.set_accels_for_action("win.toggle-mute", &["<Control><Shift>m"]);
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
}

/// Registra todas las acciones de ventana (`win.*`) sobre `gtk_win`.
pub fn install_window_actions(gtk_win: &ApplicationWindow, win: &BrowserWindow) {
    // Fase 1.
    add_win_action(gtk_win, "focus-address", {
        let w = win.clone();
        move || w.focus_address_bar()
    });

    add_win_action(gtk_win, "reload", {
        let w = win.clone();
        move || {
            if let Some(bv) = w.ensure_active_browser_view() {
                bv.reload();
            }
        }
    });

    add_win_action(gtk_win, "reload-ignore-cache", {
        let w = win.clone();
        move || {
            if let Some(bv) = w.ensure_active_browser_view() {
                bv.reload_ignore_cache();
            }
        }
    });

    add_win_action(gtk_win, "stop-or-escape", {
        let w = win.clone();
        move || w.handle_escape()
    });

    add_win_action(gtk_win, "go-back", {
        let w = win.clone();
        move || {
            if let Some(bv) = w.active_browser_view() {
                bv.go_back();
            }
        }
    });

    add_win_action(gtk_win, "go-forward", {
        let w = win.clone();
        move || {
            if let Some(bv) = w.active_browser_view() {
                bv.go_forward();
            }
        }
    });

    add_win_action(gtk_win, "find", {
        let w = win.clone();
        move || w.open_find_bar()
    });

    add_win_action(gtk_win, "find-next", {
        let w = win.clone();
        move || w.find_bar().find_next()
    });

    add_win_action(gtk_win, "find-prev", {
        let w = win.clone();
        move || w.find_bar().find_previous()
    });

    add_win_action(gtk_win, "zoom-in", {
        let w = win.clone();
        move || w.zoom_in()
    });

    add_win_action(gtk_win, "zoom-out", {
        let w = win.clone();
        move || w.zoom_out()
    });

    add_win_action(gtk_win, "zoom-reset", {
        let w = win.clone();
        move || w.zoom_reset()
    });

    add_win_action(gtk_win, "dev-tools", {
        let w = win.clone();
        move || {
            if let Some(bv) = w.ensure_active_browser_view() {
                bv.toggle_dev_tools();
            }
        }
    });

    add_win_action(gtk_win, "copy-clean-url", {
        let w = win.clone();
        move || {
            if let Some(bv) = w.active_browser_view() {
                let current = bv.current_url();
                if !current.is_empty() && current != "about:blank" {
                    let cleaned = clean_link(&current);
                    if let Some(display) = Display::default_display() {
                        display.set_clipboard_text(&cleaned);
                    }
                }
            }
        }
    });

    add_win_action(gtk_win, "paste-and-go", {
        let w = win.clone();
        move || {
            if let Some(display) = Display::default_display() {
                let w_async = w.clone();
                display.read_clipboard_text_async(move |opt_text| {
                    if let Some(text) = opt_text {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            w_async.navigate_typed_input(trimmed);
                        }
                    }
                });
            }
        }
    });

    add_win_action(gtk_win, "toggle-fullscreen", {
        let w = win.clone();
        move || w.toggle_fullscreen()
    });

    // Fase 2.
    add_win_action(gtk_win, "new-tab", {
        let w = win.clone();
        move || {
            let _ = w.open_tab(None, true, false, false);
        }
    });

    add_win_action(gtk_win, "close-tab", {
        let w = win.clone();
        move || w.close_active_tab()
    });

    add_win_action(gtk_win, "reopen-tab", {
        let w = win.clone();
        move || w.reopen_closed_tab()
    });

    add_win_action(gtk_win, "next-tab", {
        let w = win.clone();
        move || w.select_next_tab()
    });

    add_win_action(gtk_win, "prev-tab", {
        let w = win.clone();
        move || w.select_previous_tab()
    });

    for idx in 1..=8 {
        let w = win.clone();
        let name = format!("tab-{idx}");
        add_win_action(gtk_win, &name, move || w.select_tab_shortcut(idx));
    }

    add_win_action(gtk_win, "tab-last", {
        let w = win.clone();
        move || w.select_tab_shortcut(9)
    });

    add_win_action(gtk_win, "tab-switcher", {
        let w = win.clone();
        move || w.open_tab_switcher()
    });

    add_win_action(gtk_win, "new-window", {
        let w = win.clone();
        move || w.request_new_window()
    });

    add_win_action(gtk_win, "new-private-window", {
        let w = win.clone();
        move || w.request_new_private_window()
    });

    add_win_action(gtk_win, "toggle-tab-layout", {
        let w = win.clone();
        move || w.toggle_tab_layout()
    });

    add_win_action(gtk_win, "toggle-fold-tabs", {
        let w = win.clone();
        move || w.toggle_fold_tabs()
    });

    add_win_action(gtk_win, "duplicate-tab", {
        let w = win.clone();
        move || w.duplicate_active_tab()
    });

    add_win_action(gtk_win, "toggle-mute", {
        let w = win.clone();
        move || w.toggle_mute_active_tab()
    });

    add_win_action(gtk_win, "open-settings", {
        let w = win.clone();
        move || settings_ui::present_settings_dialog(w.gtk_window(), w.settings())
    });

    add_win_action(gtk_win, "open-shortcuts", {
        let w = win.clone();
        move || present_shortcuts_dialog(w.gtk_window())
    });

    // Fase 3.
    add_win_action(gtk_win, "toggle-history", {
        let w = win.clone();
        move || w.toggle_history_panel()
    });

    add_win_action(gtk_win, "bookmark-page", {
        let w = win.clone();
        move || w.open_bookmark_popover()
    });

    add_win_action(gtk_win, "toggle-bookmarks", {
        let w = win.clone();
        move || w.toggle_bookmarks_panel()
    });

    add_win_action(gtk_win, "toggle-downloads", {
        let w = win.clone();
        move || w.toggle_downloads_panel()
    });
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
        ("Ir a pestaña 1…8", "<Control>1...<Control>8"),
        ("Ir a la última pestaña", "<Control>9"),
        ("Duplicar pestaña", "<Control><Alt>d"),
        ("Silenciar o activar sonido", "<Control><Shift>m"),
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
        ("Inspector web (DevTools)", "<Control><Shift>i F12"),
        ("Preferencias", "<Control>comma"),
        ("Atajos de teclado", "<Control>question"),
        ("Salir de Search", "<Control>q"),
    ] {
        page_sec.add(&ShortcutsItem::new(title, accel));
    }
    dialog.add(&page_sec);

    dialog.present(Some(parent));
}

fn add_win_action(win: &ApplicationWindow, name: &str, cb: impl Fn() + 'static) {
    let action = SimpleAction::new(name);
    action.connect_activate(cb);
    win.add_action(&action);
}
