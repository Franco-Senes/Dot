//! Registro centralizado de acciones GIO (`app.*` y `win.*`) y aceleradores de teclado con `Ctrl`
//! para Linux (`Shortcuts.swift` / `KeyLayout.swift` — Fase 1).

use crate::window::BrowserWindow;
use gnome_platform::adw::{Application, ApplicationWindow};
use gnome_platform::gdk::Display;
use gnome_platform::gio::SimpleAction;
use search_core::address::clean_link;

/// Registra las acciones globales de aplicación y declara todos los atajos de teclado de Fase 1.
pub fn install_app_actions_and_accels(app: &Application) {
    let quit_action = SimpleAction::new("quit");
    let app_quit = app.clone();
    quit_action.connect_activate(move || {
        app_quit.quit();
    });
    app.add_action(&quit_action);

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
}

/// Registra las acciones de navegación, búsqueda, zoom e inspector sobre una ventana concreta.
pub fn install_window_actions(gtk_win: &ApplicationWindow, win: &BrowserWindow) {
    add_win_action(gtk_win, "focus-address", {
        let w = win.clone();
        move || w.focus_address_bar()
    });

    add_win_action(gtk_win, "reload", {
        let w = win.clone();
        move || w.browser_view().reload()
    });

    add_win_action(gtk_win, "reload-ignore-cache", {
        let w = win.clone();
        move || w.browser_view().reload_ignore_cache()
    });

    add_win_action(gtk_win, "stop-or-escape", {
        let w = win.clone();
        move || w.handle_escape()
    });

    add_win_action(gtk_win, "go-back", {
        let w = win.clone();
        move || w.browser_view().go_back()
    });

    add_win_action(gtk_win, "go-forward", {
        let w = win.clone();
        move || w.browser_view().go_forward()
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
        move || w.browser_view().toggle_dev_tools()
    });

    add_win_action(gtk_win, "copy-clean-url", {
        let w = win.clone();
        move || {
            let current = w.browser_view().current_url();
            if !current.is_empty() && current != "about:blank" {
                let cleaned = clean_link(&current);
                if let Some(display) = Display::default_display() {
                    display.set_clipboard_text(&cleaned);
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
}

fn add_win_action(win: &ApplicationWindow, name: &str, cb: impl Fn() + 'static) {
    let action = SimpleAction::new(name);
    action.connect_activate(cb);
    win.add_action(&action);
}
