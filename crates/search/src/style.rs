//! Hoja CSS sobria y sincronización de esquema claro/oscuro con `AdwStyleManager` (`Design.swift`).

use gnome_platform::adw::{ColorScheme, StyleManager};
use gnome_platform::gdk::Display;
use gnome_platform::gtk::{CssProvider, STYLE_PROVIDER_PRIORITY_APPLICATION};
use search_core::settings::ColorLook;

const BASE_CSS: &str = r#"
headerbar.search-headerbar {
    min-height: 40px;
    padding-top: 2px;
    padding-bottom: 2px;
}

entry.search-address-entry {
    min-height: 30px;
    border-radius: 9px;
    padding-left: 12px;
    padding-right: 12px;
    font-size: 0.95rem;
    background-color: alpha(currentColor, 0.06);
    border: 1px solid transparent;
    transition: background-color 120ms ease, border-color 120ms ease;
}

entry.search-address-entry:focus-within {
    background-color: alpha(currentColor, 0.09);
    border-color: alpha(@accent_bg_color, 0.55);
}

entry.search-address-entry.insecure-http {
    border-color: alpha(@warning_color, 0.65);
}

progressbar.search-load-progress {
    min-height: 2px;
}

progressbar.search-load-progress trough {
    min-height: 2px;
    background: transparent;
}

progressbar.search-load-progress progress {
    min-height: 2px;
    border-radius: 0;
}

label.search-status-pill {
    background-color: alpha(@window_bg_color, 0.92);
    color: @window_fg_color;
    border: 1px solid alpha(currentColor, 0.12);
    border-radius: 6px;
    padding: 3px 8px;
    font-size: 0.80rem;
}

box.search-find-bar {
    background-color: @headerbar_bg_color;
    border-bottom: 1px solid alpha(currentColor, 0.10);
    padding: 4px 10px;
}

label.search-find-count {
    font-variant-numeric: tabular-nums;
    opacity: 0.72;
    font-size: 0.85rem;
    min-width: 64px;
}
"#;

/// Registra el proveedor CSS de la aplicación en el `GdkDisplay` activo.
pub fn install_css() {
    if let Some(display) = Display::default_display() {
        let provider = CssProvider::new();
        provider.load_from_string(BASE_CSS);
        provider.add_to_display(&display, STYLE_PROVIDER_PRIORITY_APPLICATION);
    }
}

/// Aplica el esquema de color (`system`, `light` o `dark`) sobre `AdwStyleManager`.
pub fn apply_color_look(style_manager: &StyleManager, look: ColorLook) {
    let scheme = match look {
        ColorLook::System => ColorScheme::Default,
        ColorLook::Light => ColorScheme::ForceLight,
        ColorLook::Dark => ColorScheme::ForceDark,
    };
    style_manager.set_color_scheme(scheme);
}
