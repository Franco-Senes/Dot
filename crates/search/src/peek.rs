//! Arc Peek (`open_peek_dialog`) y Little Arc (`open_little_arc_window`).
//!
//! - **Arc Peek**: tarjeta flotante centrada (`adw::Dialog`) con vista previa ligera
//!   (`BrowserView`), cabecera compacta con dominio/URL, botones *"Abrir en pestaña"*
//!   y *"Copiar enlace limpio"*, cierre con `Esc` y destrucción limpia (`close_browser()`).
//! - **Little Arc**: ventana flotante compacta (`720x520`, `adw::ApplicationWindow`)
//!   con omnibox mínima, botón *"Expandir a ventana"* y cierre con `Esc`, usando
//!   `WeakRef` en los cierres GTK para evitar ciclos de referencias.

#![allow(dead_code)]

use crate::browser_view::{BrowserCallbacks, BrowserView};
use crate::curtain::CurtainManager;
use crate::settings::SettingsStore;
use crate::shield::ShieldManager;
use anyhow::Result;
use cef::RequestContext;
use gnome_platform::adw::{Application, ApplicationWindow, Dialog, HeaderBar, ToolbarView};
use gnome_platform::gdk;
use gnome_platform::gtk::{
    Align, AsEditable, AsWidget, BoxWidget, Button, EllipsizeMode, Entry, EventControllerKey,
    InputPurpose, Label, Orientation,
};
use gnome_platform::object::{ObjectSubclass, WeakRef};
use search_core::address::{
    clean_link, resolve_address, safe_display_host, sanitize_url_for_ui, SearchEngine,
};
use std::cell::RefCell;
use std::rc::Rc;
use url::Url;

const PEEK_DIALOG_WIDTH: i32 = 920;
const PEEK_DIALOG_HEIGHT: i32 = 640;
const LITTLE_ARC_WIDTH: i32 = 720;
const LITTLE_ARC_HEIGHT: i32 = 520;

/// Extrae el dominio legible y la URL sanitizada para mostrar en la cabecera de Peek.
pub fn format_peek_domain_and_url(raw_url: &str) -> (String, String) {
    let clean_display = sanitize_url_for_ui(raw_url);
    let domain = Url::parse(raw_url.trim())
        .ok()
        .and_then(|parsed| parsed.host_str().map(safe_display_host))
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| {
            if clean_display.is_empty() {
                "Vista previa".to_string()
            } else {
                clean_display.clone()
            }
        });
    (domain, clean_display)
}

/// Limpia parámetros de seguimiento de una URL antes de copiarla al portapapeles.
pub fn clean_url_for_copy(raw_url: &str) -> String {
    clean_link(raw_url)
}

/// Resuelve la entrada de la omnibox mínima de Little Arc a una URL navegable.
pub fn resolve_little_arc_input(
    raw: &str,
    search_engine: SearchEngine,
    custom_template: &str,
) -> String {
    resolve_address(raw, search_engine, custom_template)
        .as_str()
        .to_string()
}

struct PeekDialogWidgets {
    dialog: Dialog,
    root: BoxWidget,
    domain_label: Label,
    url_label: Label,
    open_tab_btn: Button,
    copy_btn: Button,
    close_btn: Button,
}

fn build_peek_dialog_shell(initial_url: &str) -> PeekDialogWidgets {
    let dialog = Dialog::new();
    dialog.set_title("Vista previa");
    dialog.set_content_width(PEEK_DIALOG_WIDTH);
    dialog.set_content_height(PEEK_DIALOG_HEIGHT);
    dialog.set_follows_content_size(false);

    let root = BoxWidget::new(Orientation::Vertical, 0);
    root.add_css_class("search-peek-root");
    root.set_hexpand(true);
    root.set_vexpand(true);

    let header = BoxWidget::new(Orientation::Horizontal, 8);
    header.add_css_class("search-peek-header");
    header.set_margins(10, 10, 12, 12);

    let title_box = BoxWidget::new(Orientation::Vertical, 2);
    title_box.set_hexpand(true);
    title_box.set_valign(Align::Center);

    let (domain_text, url_text) = format_peek_domain_and_url(initial_url);

    let domain_label = Label::new(Some(&domain_text));
    domain_label.add_css_class("heading");
    domain_label.set_halign(Align::Start);
    domain_label.set_ellipsize(EllipsizeMode::End);

    let url_label = Label::new(Some(&url_text));
    url_label.add_css_class("caption");
    url_label.add_css_class("dim-label");
    url_label.set_halign(Align::Start);
    url_label.set_ellipsize(EllipsizeMode::End);

    title_box.append(&domain_label);
    title_box.append(&url_label);

    let copy_btn = Button::with_label("Copiar enlace limpio");
    copy_btn.set_valign(Align::Center);
    copy_btn.set_tooltip_text(Some("Copiar URL sin parámetros de rastreo"));

    let open_tab_btn = Button::with_label("Abrir en pestaña");
    open_tab_btn.add_css_class("suggested-action");
    open_tab_btn.set_valign(Align::Center);

    let close_btn = Button::from_icon_name("window-close-symbolic");
    close_btn.add_css_class("flat");
    close_btn.set_valign(Align::Center);
    close_btn.set_tooltip_text(Some("Cerrar (Esc)"));

    header.append(&title_box);
    header.append(&copy_btn);
    header.append(&open_tab_btn);
    header.append(&close_btn);

    root.append(&header);
    dialog.set_child(Some(&root));

    let dialog_weak: WeakRef<Dialog> = dialog.downgrade();
    close_btn.connect_clicked(move || {
        if let Some(dlg) = dialog_weak.upgrade() {
            dlg.close();
        }
    });

    let key_ctrl = EventControllerKey::new();
    let dialog_weak_esc: WeakRef<Dialog> = dialog.downgrade();
    key_ctrl.connect_key_pressed(move |keyval, _keycode, _mods| {
        if keyval == gdk::keys::ESCAPE {
            if let Some(dlg) = dialog_weak_esc.upgrade() {
                dlg.close();
            }
            return true;
        }
        false
    });
    root.add_controller(&key_ctrl);

    PeekDialogWidgets {
        dialog,
        root,
        domain_label,
        url_label,
        open_tab_btn,
        copy_btn,
        close_btn,
    }
}

/// Abre **Arc Peek**: tarjeta flotante centrada (`adw::Dialog`) con un `BrowserView` ligero
/// para previsualizar `initial_url`. Al cerrarse destruye el navegador con `close_browser()`.
#[allow(clippy::too_many_arguments)]
pub fn open_peek_dialog(
    parent: &ApplicationWindow,
    request_context: RequestContext,
    settings: SettingsStore,
    shield: ShieldManager,
    curtain: CurtainManager,
    initial_url: &str,
    dark_background: bool,
    on_open_in_tab: impl Fn(String) + 'static,
) -> Result<()> {
    let shell = build_peek_dialog_shell(initial_url);

    let bv = BrowserView::new(
        request_context,
        settings,
        shield,
        curtain,
        initial_url,
        dark_background,
    )?;

    let current_url = Rc::new(RefCell::new(initial_url.to_string()));
    let dialog_weak: WeakRef<Dialog> = shell.dialog.downgrade();

    let mut cbs = BrowserCallbacks::default();
    {
        let current_url = current_url.clone();
        let domain_label = shell.domain_label.clone();
        let url_label = shell.url_label.clone();
        cbs.on_address_changed = Some(Box::new(move |new_url| {
            *current_url.borrow_mut() = new_url.clone();
            let (domain, display_url) = format_peek_domain_and_url(&new_url);
            domain_label.set_text(&domain);
            url_label.set_text(&display_url);
        }));
    }
    {
        let dlg_weak = dialog_weak.clone();
        cbs.on_title_changed = Some(Box::new(move |title| {
            let trimmed = title.trim();
            if !trimmed.is_empty() {
                if let Some(dlg) = dlg_weak.upgrade() {
                    dlg.set_title(trimmed);
                }
            }
        }));
    }
    bv.set_callbacks(cbs);

    shell.root.append(bv.widget());

    {
        let current_url = current_url.clone();
        shell.copy_btn.connect_clicked(move || {
            let cleaned = clean_url_for_copy(&current_url.borrow());
            if let Some(disp) = gdk::Display::default_display() {
                disp.set_clipboard_text(&cleaned);
            }
        });
    }

    {
        let current_url = current_url.clone();
        let dlg_weak = dialog_weak.clone();
        shell.open_tab_btn.connect_clicked(move || {
            let url = current_url.borrow().clone();
            on_open_in_tab(url);
            if let Some(dlg) = dlg_weak.upgrade() {
                dlg.close();
            }
        });
    }

    let browser_slot = Rc::new(RefCell::new(Some(bv)));
    shell.dialog.connect_closed(move || {
        if let Some(view) = browser_slot.borrow_mut().take() {
            view.close_browser();
        }
    });

    shell.dialog.present(Some(parent));
    Ok(())
}

struct LittleArcWindowWidgets {
    window: ApplicationWindow,
    toolbar_view: ToolbarView,
    omnibox: Entry,
    expand_btn: Button,
}

fn build_little_arc_window_shell(app: &Application, initial_url: &str) -> LittleArcWindowWidgets {
    let window = ApplicationWindow::new(app);
    window.set_default_size(LITTLE_ARC_WIDTH, LITTLE_ARC_HEIGHT);
    window.set_title("Little Arc");
    window.add_css_class("search-little-arc-window");

    let toolbar_view = ToolbarView::new();
    let header = HeaderBar::new();

    let omnibox = Entry::new();
    omnibox.set_hexpand(true);
    omnibox.set_placeholder_text("Escribe una URL o búsqueda…");
    omnibox.set_input_purpose(InputPurpose::Url);
    omnibox.set_text(initial_url);

    let expand_btn = Button::with_label("Expandir a ventana");
    expand_btn.add_css_class("suggested-action");
    expand_btn.set_valign(Align::Center);

    header.set_title_widget(Some(&omnibox));
    header.pack_end(&expand_btn);

    toolbar_view.add_top_bar(&header);
    window.set_content(Some(&toolbar_view));

    let key_ctrl = EventControllerKey::new();
    let win_weak_esc: WeakRef<ApplicationWindow> = window.downgrade();
    key_ctrl.connect_key_pressed(move |keyval, _keycode, _mods| {
        if keyval == gdk::keys::ESCAPE {
            if let Some(win) = win_weak_esc.upgrade() {
                win.close();
            }
            return true;
        }
        false
    });
    window.add_controller(&key_ctrl);

    LittleArcWindowWidgets {
        window,
        toolbar_view,
        omnibox,
        expand_btn,
    }
}

/// Abre **Little Arc**: ventana flotante compacta (`720x520`) con omnibox mínima,
/// botón *"Expandir a ventana"* (entrega la URL a la ventana principal y cierra)
/// y cierre rápido con `Esc`.
#[allow(clippy::too_many_arguments)]
pub fn open_little_arc_window(
    app: &Application,
    request_context: RequestContext,
    settings: SettingsStore,
    shield: ShieldManager,
    curtain: CurtainManager,
    initial_url: &str,
    dark_background: bool,
    on_expand_to_main: impl Fn(String) + 'static,
) -> Result<()> {
    let LittleArcWindowWidgets {
        window: win,
        toolbar_view,
        omnibox,
        expand_btn,
    } = build_little_arc_window_shell(app, initial_url);

    let bv = BrowserView::new(
        request_context,
        settings.clone(),
        shield,
        curtain,
        initial_url,
        dark_background,
    )?;

    toolbar_view.set_content(Some(bv.widget()));

    let current_url = Rc::new(RefCell::new(initial_url.to_string()));
    let win_weak: WeakRef<ApplicationWindow> = win.downgrade();

    let omnibox_slot = Rc::new(RefCell::new(Some(omnibox.clone())));
    let browser_slot = Rc::new(RefCell::new(Some(bv)));

    {
        let current_url = current_url.clone();
        let omnibox_slot = omnibox_slot.clone();
        let win_weak_cb = win_weak.clone();
        if let Some(ref view) = *browser_slot.borrow() {
            let cbs = BrowserCallbacks {
                on_address_changed: Some(Box::new(move |new_url| {
                    *current_url.borrow_mut() = new_url.clone();
                    if let Some(ref entry) = *omnibox_slot.borrow() {
                        entry.set_text(&new_url);
                    }
                })),
                on_title_changed: Some(Box::new(move |title| {
                    let trimmed = title.trim();
                    if !trimmed.is_empty() {
                        if let Some(w) = win_weak_cb.upgrade() {
                            w.set_title(trimmed);
                        }
                    }
                })),
                ..Default::default()
            };
            view.set_callbacks(cbs);
        }
    }

    {
        let omnibox_slot = omnibox_slot.clone();
        let browser_slot = browser_slot.clone();
        let settings = settings.clone();
        omnibox.connect_activate(move || {
            let raw = match *omnibox_slot.borrow() {
                Some(ref entry) => entry.text(),
                None => return,
            };
            let (engine, custom_tpl) =
                settings.with(|s| (s.search_engine, s.custom_search_url.clone()));
            let target = resolve_little_arc_input(&raw, engine, &custom_tpl);
            if let Some(ref view) = *browser_slot.borrow() {
                view.load_url(&target);
            }
        });
    }

    {
        let current_url = current_url.clone();
        let win_weak_expand = win_weak.clone();
        expand_btn.connect_clicked(move || {
            let url = current_url.borrow().clone();
            on_expand_to_main(url);
            if let Some(w) = win_weak_expand.upgrade() {
                w.close();
            }
        });
    }

    {
        let browser_slot = browser_slot.clone();
        let omnibox_slot = omnibox_slot.clone();
        win.connect_close_request(move || {
            let _ = omnibox_slot.borrow_mut().take();
            if let Some(view) = browser_slot.borrow_mut().take() {
                view.close_browser();
            }
            false
        });
    }

    win.present();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_peek_domain_and_url_extracts_host_and_sanitizes() {
        let (domain, url) =
            format_peek_domain_and_url("https://docs.rs/gtk4/latest/gtk4/?utm_source=test");
        assert_eq!(domain, "docs.rs");
        assert!(url.starts_with("https://docs.rs/"));
    }

    #[test]
    fn clean_url_for_copy_strips_tracking_params() {
        let cleaned = clean_url_for_copy(
            "https://example.com/article?utm_source=newsletter&id=42&fbclid=xyz",
        );
        assert_eq!(cleaned, "https://example.com/article?id=42");
    }

    #[test]
    fn resolve_little_arc_input_handles_urls_and_queries() {
        let url = resolve_little_arc_input("rust-lang.org", SearchEngine::DuckDuckGo, "");
        assert_eq!(url, "https://rust-lang.org/");

        let search = resolve_little_arc_input("gtk4 rust", SearchEngine::DuckDuckGo, "");
        assert!(search.contains("duckduckgo.com"));
        assert!(search.contains("gtk4"));
    }

    #[test]
    fn peek_dialog_shell_has_no_reference_cycle() {
        let _guard = crate::GTK_TEST_MUTEX
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if !gnome_platform::init() || gnome_platform::gdk::Display::default_display().is_none() {
            return;
        }
        let shell = build_peek_dialog_shell("https://example.com/demo");
        let weak = shell.dialog.downgrade();
        assert!(weak.upgrade().is_some());
        drop(shell);
        assert!(weak.upgrade().is_none());
    }
}
