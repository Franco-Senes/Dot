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

window.search-private-window headerbar.search-headerbar {
    background-color: mix(@headerbar_bg_color, @accent_bg_color, 0.08);
}

label.search-private-badge {
    font-size: 0.76rem;
    font-weight: 600;
    padding: 2px 8px;
    border-radius: 999px;
    background-color: alpha(@accent_bg_color, 0.16);
    color: @accent_color;
    margin-left: 4px;
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

box.search-sidebar {
    background-color: @sidebar_bg_color;
    border-right: 1px solid alpha(currentColor, 0.09);
    padding: 6px;
}

box.search-sidebar-pinned {
    padding: 2px 2px 6px 2px;
}

button.search-pinned-pill {
    min-width: 34px;
    min-height: 32px;
    padding: 4px 8px;
    border-radius: 8px;
    background-color: alpha(currentColor, 0.05);
    border: 1px solid transparent;
}

button.search-pinned-pill:hover {
    background-color: alpha(currentColor, 0.10);
}

button.search-pinned-pill.active {
    background-color: alpha(@accent_bg_color, 0.18);
    border-color: alpha(@accent_bg_color, 0.45);
}

listbox.search-sidebar-list {
    background-color: transparent;
}

listbox.search-sidebar-list row.search-tab-row {
    border-radius: 8px;
    padding: 4px 6px;
    margin: 1px 0;
}

listbox.search-sidebar-list row.search-tab-row:hover {
    background-color: alpha(currentColor, 0.06);
}

listbox.search-sidebar-list row.search-tab-row.active {
    background-color: alpha(@accent_bg_color, 0.16);
    font-weight: 500;
}

button.search-tab-mini-btn {
    min-width: 22px;
    min-height: 22px;
    padding: 0;
    border-radius: 6px;
    opacity: 0.65;
}

button.search-tab-mini-btn:hover {
    opacity: 1.0;
    background-color: alpha(currentColor, 0.10);
}

box.search-switcher-root {
    padding: 12px;
}

listbox.search-switcher-list {
    background-color: transparent;
}

listbox.search-switcher-list row.search-switcher-row {
    border-radius: 8px;
    padding: 6px 10px;
    margin: 1px 0;
}

listbox.search-switcher-list row.search-switcher-row:selected {
    background-color: alpha(@accent_bg_color, 0.20);
}

label.search-switcher-url {
    font-size: 0.80rem;
    opacity: 0.65;
}

label.search-switcher-badge {
    font-size: 0.72rem;
    padding: 1px 6px;
    border-radius: 999px;
    background-color: alpha(currentColor, 0.09);
    opacity: 0.82;
}

popover.search-omnibox-popover > contents {
    padding: 4px;
    border-radius: 10px;
}

listbox.search-omnibox-list {
    background-color: transparent;
}

listbox.search-omnibox-list row.search-omnibox-row {
    border-radius: 7px;
    margin: 1px 0;
}

listbox.search-omnibox-list row.search-omnibox-row:selected {
    background-color: alpha(@accent_bg_color, 0.18);
}

label.search-omnibox-url {
    font-size: 0.78rem;
    opacity: 0.64;
}

label.search-omnibox-badge {
    font-size: 0.72rem;
    font-weight: 600;
    padding: 2px 7px;
    border-radius: 999px;
    background-color: alpha(@accent_bg_color, 0.16);
    color: @accent_color;
}

box.search-side-panel {
    background-color: @sidebar_bg_color;
    border-left: 1px solid alpha(currentColor, 0.09);
}

label.search-panel-title {
    font-weight: 600;
    font-size: 0.96rem;
}

label.search-panel-section {
    font-weight: 600;
    font-size: 0.78rem;
    opacity: 0.65;
}

label.search-panel-subtitle {
    font-size: 0.78rem;
    opacity: 0.65;
}

label.search-panel-empty {
    font-size: 0.88rem;
    opacity: 0.60;
}

listbox.search-panel-list {
    background-color: transparent;
}

listbox.search-panel-list row.search-panel-row {
    padding: 0;
}

button.search-panel-item-btn {
    padding: 4px 6px;
    border-radius: 7px;
}

box.search-panel-editor {
    padding: 8px;
    border-radius: 8px;
    background-color: alpha(currentColor, 0.05);
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
