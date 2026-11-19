//! Hoja CSS sobria, pulida y con transiciones suaves (`Design.swift`) + sincronización
//! de esquema claro/oscuro con `AdwStyleManager`.
//!
//! Nota GTK4 CSS: usar exclusivamente propiedades estándar (`margin-left`/`margin-right`,
//! `padding-left`/`padding-right`, etc.) para cero advertencias del analizador CSS.

use gnome_platform::adw::{ColorScheme, StyleManager};
use gnome_platform::gdk::Display;
use gnome_platform::gtk::{CssProvider, STYLE_PROVIDER_PRIORITY_APPLICATION};
use search_core::settings::ColorLook;

const BASE_CSS: &str = r#"
/* ============================================================================
 * Cabecera minimalista y barra de direcciones (Omnibox)
 * ============================================================================ */
headerbar.search-headerbar {
    min-height: 42px;
    padding-top: 3px;
    padding-bottom: 3px;
}

window.search-private-window headerbar.search-headerbar {
    background-color: mix(@headerbar_bg_color, @accent_bg_color, 0.08);
}

label.search-private-badge {
    font-size: 0.75rem;
    font-weight: 600;
    padding: 2px 9px;
    border-radius: 999px;
    background-color: alpha(@accent_bg_color, 0.16);
    color: @accent_color;
    margin-left: 6px;
}

entry.search-address-entry {
    min-height: 31px;
    border-radius: 10px;
    padding-left: 12px;
    padding-right: 12px;
    font-size: 0.94rem;
    background-color: alpha(currentColor, 0.055);
    border: 1px solid alpha(currentColor, 0.06);
    box-shadow: none;
    transition: background-color 140ms ease, border-color 140ms ease, box-shadow 140ms ease;
}

entry.search-address-entry:hover {
    background-color: alpha(currentColor, 0.08);
}

entry.search-address-entry:focus-within {
    background-color: @view_bg_color;
    border-color: alpha(@accent_bg_color, 0.65);
    box-shadow: 0 0 0 2px alpha(@accent_bg_color, 0.24);
}

entry.search-address-entry.insecure-http {
    border-color: alpha(@warning_color, 0.72);
}

/* ============================================================================
 * Marco de tarjeta redondeado estilo Arc / Search (`layout_cards`)
 * ============================================================================ */
box.search-main-hbox.cards-enabled {
    background-color: @sidebar_bg_color;
}

box.search-stage-shell {
    background-color: transparent;
    padding: 0;
    transition: padding 160ms ease;
}

box.search-main-hbox.cards-enabled box.search-stage-shell {
    padding-top: 4px;
    padding-right: 7px;
    padding-bottom: 7px;
    padding-left: 3px;
}

box.search-stage-card {
    background-color: @view_bg_color;
    border-radius: 0;
    border: none;
    box-shadow: none;
    transition: border-radius 160ms ease, box-shadow 160ms ease;
}

box.search-main-hbox.cards-enabled box.search-stage-card {
    border-radius: 11px;
    border: 1px solid alpha(currentColor, 0.10);
    box-shadow: 0 2px 10px alpha(#000000, 0.08), 0 0 1px alpha(#000000, 0.12);
}

/* Barra superior compacta opcional */
window.search-compact-tabs tabbar tab {
    min-height: 28px;
}

/* Desactivar transiciones CSS cuando ui_motion = instant */
window.search-motion-instant * {
    transition-duration: 0ms;
}

/* ============================================================================
 * Progreso, estado de enlace y barra de búsqueda en página
 * ============================================================================ */
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
    background-color: alpha(@window_bg_color, 0.94);
    color: @window_fg_color;
    border: 1px solid alpha(currentColor, 0.12);
    border-radius: 7px;
    padding: 3px 9px;
    font-size: 0.79rem;
    box-shadow: 0 2px 8px alpha(#000000, 0.12);
    transition: opacity 140ms ease;
}

box.search-find-bar {
    background-color: @headerbar_bg_color;
    border-bottom: 1px solid alpha(currentColor, 0.10);
    padding: 5px 10px;
}

label.search-find-count {
    font-variant-numeric: tabular-nums;
    opacity: 0.72;
    font-size: 0.84rem;
    min-width: 64px;
}

/* ============================================================================
 * Barra lateral vertical estilo Arc (Favoritos + Carpetas + Fijadas + Hoy)
 * ============================================================================ */
box.search-sidebar {
    background-color: @sidebar_bg_color;
    border-right: 1px solid alpha(currentColor, 0.08);
    padding: 7px 7px 6px 7px;
}

box.search-main-hbox.cards-enabled box.search-sidebar {
    border-right: none;
}

box.search-sidebar-resize-handle {
    min-width: 6px;
    background-color: transparent;
    border-right: 2px solid transparent;
    transition: border-color 140ms ease;
}

box.search-sidebar-resize-handle:hover,
box.search-sidebar-resize-handle.dragging {
    border-right-color: alpha(@accent_bg_color, 0.55);
}

box.search-sidebar-favorites {
    padding: 2px 1px 6px 1px;
}

button.search-fav-tile {
    min-width: 38px;
    min-height: 38px;
    padding: 6px;
    border-radius: 10px;
    background-color: alpha(currentColor, 0.055);
    border: 1px solid alpha(currentColor, 0.05);
    box-shadow: none;
    transition: background-color 140ms ease, border-color 140ms ease, box-shadow 140ms ease;
}

button.search-fav-tile:hover {
    background-color: alpha(currentColor, 0.10);
    border-color: alpha(currentColor, 0.10);
    box-shadow: 0 1px 4px alpha(#000000, 0.08);
}

button.search-fav-tile.active {
    background-color: @view_bg_color;
    border-color: alpha(@accent_bg_color, 0.55);
    box-shadow: 0 1px 6px alpha(#000000, 0.10);
}

box.search-sidebar-section-header {
    padding: 3px 4px 1px 6px;
}

label.search-sidebar-section-label {
    font-size: 0.72rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    opacity: 0.55;
}

box.search-folder-editor {
    padding: 4px;
    border-radius: 8px;
    background-color: alpha(currentColor, 0.05);
    margin-bottom: 3px;
}

button.search-folder-row {
    border-radius: 8px;
    padding: 4px 7px;
    margin: 1px 0;
    transition: background-color 130ms ease;
}

button.search-folder-row:hover {
    background-color: alpha(currentColor, 0.065);
}

label.search-folder-name {
    font-weight: 500;
    font-size: 0.88rem;
}

label.search-folder-count {
    font-size: 0.72rem;
    font-variant-numeric: tabular-nums;
    padding: 1px 6px;
    border-radius: 999px;
    background-color: alpha(currentColor, 0.08);
    opacity: 0.72;
}

separator.search-sidebar-divider {
    margin-top: 5px;
    margin-bottom: 5px;
    margin-left: 6px;
    margin-right: 6px;
    opacity: 0.55;
}

listbox.search-sidebar-list {
    background-color: transparent;
}

row.search-tab-row,
box.search-tab-row {
    border-radius: 8px;
    padding: 5px 7px;
    margin: 1px 0;
    transition: background-color 130ms ease, box-shadow 130ms ease;
}

box.search-roomy-sidebar row.search-tab-row,
box.search-roomy-sidebar box.search-tab-row {
    padding: 8px 8px;
}

row.search-tab-row:hover,
box.search-tab-row:hover {
    background-color: alpha(currentColor, 0.065);
}

row.search-tab-row.active,
box.search-tab-row.active {
    background-color: @view_bg_color;
    box-shadow: 0 1px 3px alpha(#000000, 0.07);
    font-weight: 500;
}

button.search-tab-mini-btn {
    min-width: 22px;
    min-height: 22px;
    padding: 0;
    border-radius: 6px;
    opacity: 0.68;
    transition: opacity 130ms ease, background-color 130ms ease;
}

button.search-tab-mini-btn:hover {
    opacity: 1.0;
    background-color: alpha(currentColor, 0.10);
}

box.search-sidebar-footer {
    padding-top: 4px;
    border-top: 1px solid alpha(currentColor, 0.07);
}

button.search-sidebar-new-tab {
    border-radius: 8px;
    padding: 5px 8px;
    font-size: 0.88rem;
    opacity: 0.80;
    transition: opacity 130ms ease, background-color 130ms ease;
}

button.search-sidebar-new-tab:hover {
    opacity: 1.0;
    background-color: alpha(currentColor, 0.07);
}

button.search-sidebar-settings-btn {
    min-width: 30px;
    min-height: 30px;
    padding: 0;
    border-radius: 8px;
    opacity: 0.70;
    transition: opacity 130ms ease, background-color 130ms ease;
}

button.search-sidebar-settings-btn:hover {
    opacity: 1.0;
    background-color: alpha(currentColor, 0.08);
}

/* ============================================================================
 * Monogramas de respaldo para pestañas sin favicon (`Design.swift`)
 * ============================================================================ */
label.search-monogram {
    min-width: 18px;
    min-height: 18px;
    border-radius: 5px;
    font-size: 0.72rem;
    font-weight: 700;
    padding: 0 2px;
}

label.search-monogram.large {
    min-width: 22px;
    min-height: 22px;
    border-radius: 6px;
    font-size: 0.82rem;
}

label.search-monogram.tint-0 { background-color: alpha(#3b82f6, 0.18); color: #2563eb; }
label.search-monogram.tint-1 { background-color: alpha(#8b5cf6, 0.18); color: #7c3aed; }
label.search-monogram.tint-2 { background-color: alpha(#10b981, 0.18); color: #059669; }
label.search-monogram.tint-3 { background-color: alpha(#f59e0b, 0.20); color: #b45309; }
label.search-monogram.tint-4 { background-color: alpha(#ec4899, 0.18); color: #db2777; }
label.search-monogram.tint-5 { background-color: alpha(#06b6d4, 0.18); color: #0891b2; }

/* ============================================================================
 * Página nativa `plasma://settings`
 * ============================================================================ */
box.search-settings-root {
    background-color: @window_bg_color;
}

box.search-settings-rail {
    background-color: @sidebar_bg_color;
}

label.search-settings-heading {
    font-size: 1.08rem;
    font-weight: 700;
}

label.search-settings-section-title {
    font-size: 1.40rem;
    font-weight: 700;
}

button.search-settings-rail-btn {
    border-radius: 8px;
    padding: 7px 10px;
    margin: 1px 0;
    transition: background-color 130ms ease, color 130ms ease;
}

button.search-settings-rail-btn:hover {
    background-color: alpha(currentColor, 0.065);
}

button.search-settings-rail-btn.active {
    background-color: alpha(@accent_bg_color, 0.16);
    color: @accent_color;
    font-weight: 600;
}

label.search-settings-match-badge {
    font-size: 0.72rem;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    padding: 1px 6px;
    border-radius: 999px;
    background-color: alpha(@accent_bg_color, 0.20);
    color: @accent_color;
}

listbox.search-settings-results {
    background-color: transparent;
}

listbox.search-settings-results row.search-settings-result-row {
    padding: 0;
    margin: 2px 0;
}

button.search-settings-result-btn {
    border-radius: 10px;
    background-color: alpha(currentColor, 0.04);
    border: 1px solid alpha(currentColor, 0.06);
    transition: background-color 130ms ease, border-color 130ms ease;
}

button.search-settings-result-btn:hover {
    background-color: alpha(currentColor, 0.08);
    border-color: alpha(@accent_bg_color, 0.35);
}

label.search-badge-protected {
    font-size: 0.74rem;
    font-weight: 600;
    padding: 3px 9px;
    border-radius: 999px;
    background-color: alpha(#10b981, 0.16);
    color: #059669;
}

label.search-keycap {
    font-size: 0.78rem;
    font-weight: 600;
    font-family: monospace;
    padding: 3px 8px;
    border-radius: 6px;
    background-color: alpha(currentColor, 0.07);
    border: 1px solid alpha(currentColor, 0.10);
}

/* ============================================================================
 * Selector difuso `Ctrl+K`, desplegable del omnibox y paneles laterales
 * ============================================================================ */
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
    transition: background-color 120ms ease;
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
    padding: 2px 7px;
    border-radius: 999px;
    background-color: alpha(currentColor, 0.09);
    opacity: 0.86;
}

popover.search-omnibox-popover > contents {
    padding: 4px;
    border-radius: 11px;
    box-shadow: 0 8px 26px alpha(#000000, 0.16);
}

listbox.search-omnibox-list {
    background-color: transparent;
}

listbox.search-omnibox-list row.search-omnibox-row {
    border-radius: 8px;
    margin: 1px 0;
    transition: background-color 120ms ease;
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
    transition: background-color 120ms ease;
}

box.search-panel-editor {
    padding: 8px;
    border-radius: 8px;
    background-color: alpha(currentColor, 0.05);
}
box.search-reader-pill {
    background-color: alpha(@window_bg_color, 0.92);
    border: 1px solid alpha(currentColor, 0.14);
    border-radius: 999px;
    padding: 3px 6px;
    opacity: 0.72;
    transition: opacity 120ms ease;
}

box.search-reader-pill:hover {
    opacity: 1.0;
}

button.search-reader-btn {
    min-width: 24px;
    min-height: 24px;
    padding: 2px 7px;
    border-radius: 999px;
    font-size: 0.78rem;
}

button.search-reader-btn.active {
    background-color: alpha(@accent_bg_color, 0.22);
    color: @accent_color;
    font-weight: 600;
}

label.search-reader-size {
    font-variant-numeric: tabular-nums;
    font-size: 0.78rem;
    opacity: 0.75;
    padding: 0 4px;
}

label.search-toast-pill {
    background-color: alpha(@window_bg_color, 0.94);
    color: @window_fg_color;
    border: 1px solid alpha(currentColor, 0.14);
    border-radius: 999px;
    padding: 6px 14px;
    font-size: 0.84rem;
    font-weight: 500;
}

/* ============================================================================
 * Split View (`GtkPaned`) y Picture-in-Picture (`GtkWindow`)
 * ============================================================================ */
paned.search-split-paned > separator {
    min-width: 5px;
    background-color: alpha(currentColor, 0.08);
    transition: background-color 140ms ease;
}

paned.search-split-paned > separator:hover {
    background-color: alpha(@accent_bg_color, 0.45);
}

box.search-split-pane {
    border: 2px solid transparent;
    transition: border-color 140ms ease;
}

box.search-split-pane.focused {
    border-color: alpha(@accent_bg_color, 0.45);
}

image.search-split-tab-favicon {
    margin-left: -3px;
    opacity: 0.92;
}

window.search-pip-window {
    background-color: #000000;
    border-radius: 10px;
}

box.search-pip-controls {
    margin-top: 8px;
    margin-right: 8px;
    padding: 3px 5px;
    border-radius: 999px;
    background-color: alpha(#141416, 0.68);
}

button.search-pip-btn {
    min-width: 24px;
    min-height: 24px;
    padding: 2px;
    color: #ffffff;
}

button.search-pip-btn:hover {
    background-color: alpha(#ffffff, 0.18);
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
