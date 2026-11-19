//! Hoja CSS sobria, pulida y con transiciones suaves (`Design.swift`) + sincronización
//! de esquema claro/oscuro con `AdwStyleManager` y supresión de transiciones al cambiar de tema.
//!
//! Nota GTK4 CSS: usar exclusivamente propiedades estándar (`margin-left`/`margin-right`,
//! `padding-left`/`padding-right`, etc.) para cero advertencias del analizador CSS.

use gnome_platform::adw::{ColorScheme, StyleManager};
use gnome_platform::gdk::Display;
use gnome_platform::glib;
use gnome_platform::gtk::{CssProvider, STYLE_PROVIDER_PRIORITY_APPLICATION};
use search_core::settings::ColorLook;
use std::cell::RefCell;
use std::time::Duration;

thread_local! {
    static THEME_SWITCH_PROVIDER: RefCell<Option<CssProvider>> = const { RefCell::new(None) };
}

const BASE_CSS: &str = r#"
/* ============================================================================
 * Interacciones globales: foco accesible, pulsación 0.96 y supresión de tema
 * ============================================================================ */
button:focus-visible,
togglebutton:focus-visible,
entry:focus-visible,
row:focus-visible {
    outline-style: solid;
    outline-width: 2px;
    outline-color: alpha(@accent_bg_color, 0.72);
    outline-offset: 1px;
}

button:not(:disabled),
togglebutton:not(:disabled) {
    transition-property: transform, background-color, border-color, box-shadow, opacity;
    transition-duration: 140ms;
    transition-timing-function: cubic-bezier(0.2, 0, 0, 1);
}

button:not(:disabled):active,
togglebutton:not(:disabled):active {
    transform: scale(0.96);
    transition-property: transform;
    transition-duration: 150ms;
    transition-timing-function: ease-out;
}

/* Desactivar transiciones CSS cuando ui_motion = instant o durante cambio de tema */
window.search-motion-instant *,
window.search-theme-switching * {
    transition-duration: 0ms;
}

/* ============================================================================
 * Primitivas reutilizables (.pill, .kbd-chip, .zoom-pill, .toast-bar, etc.)
 * ============================================================================ */
.pill {
    font-size: 0.75rem;
    font-weight: 600;
    padding: 2px 9px;
    border-radius: 999px;
    border: 1px solid transparent;
    background-color: alpha(currentColor, 0.08);
}

.pill.accent {
    background-color: alpha(@accent_bg_color, 0.16);
    color: @accent_color;
}

.pill.numeric,
.zoom-pill {
    font-variant-numeric: tabular-nums;
}

.zoom-pill {
    font-size: 0.76rem;
    font-weight: 600;
    padding: 2px 8px;
    border-radius: 999px;
    border: 1px solid alpha(currentColor, 0.10);
    background-color: alpha(currentColor, 0.06);
}

.kbd-chip {
    font-family: monospace;
    font-size: 0.72rem;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    padding: 2px 6px;
    border-radius: 6px;
    background-color: alpha(currentColor, 0.075);
    border: 1px solid alpha(currentColor, 0.14);
    box-shadow: 0 1px 0 alpha(#000000, 0.10);
    opacity: 0.88;
}

.workspace-pill,
button.workspace-pill {
    font-size: 0.78rem;
    font-weight: 500;
    padding: 3px 10px;
    border-radius: 999px;
    border: 1px solid transparent;
    background-color: alpha(currentColor, 0.06);
    transition: background-color 120ms cubic-bezier(0.2, 0, 0, 1),
                border-color 120ms cubic-bezier(0.2, 0, 0, 1),
                color 120ms cubic-bezier(0.2, 0, 0, 1);
}

.workspace-pill:hover,
button.workspace-pill:hover {
    background-color: alpha(currentColor, 0.11);
}

.workspace-pill.active,
button.workspace-pill.active {
    background-color: alpha(@accent_bg_color, 0.18);
    border-color: alpha(@accent_bg_color, 0.45);
    color: @accent_color;
    font-weight: 600;
}

.toast-bar {
    background-color: alpha(@window_bg_color, 0.96);
    color: @window_fg_color;
    border: 1px solid transparent;
    border-radius: 999px;
    padding: 6px 14px;
    font-size: 0.84rem;
    font-weight: 500;
    box-shadow: 0 6px 20px alpha(#000000, 0.16), 0 1px 4px alpha(#000000, 0.08);
}

.empty-state {
    padding: 28px 20px;
    opacity: 0.68;
}

button.primary,
.primary {
    background-color: @accent_bg_color;
    color: @accent_fg_color;
    border: 1px solid transparent;
    font-weight: 600;
}

button.primary:hover,
.primary:hover {
    background-color: mix(@accent_bg_color, #ffffff, 0.08);
}

button.destructive,
.destructive {
    background-color: alpha(@error_bg_color, 0.14);
    color: @error_color;
    border: 1px solid alpha(@error_color, 0.24);
    font-weight: 600;
}

button.destructive:hover,
.destructive:hover {
    background-color: alpha(@error_bg_color, 0.22);
    border-color: alpha(@error_color, 0.42);
}

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
    border: 1px solid alpha(currentColor, 0.08);
    box-shadow: none;
    transition: background-color 140ms cubic-bezier(0.2, 0, 0, 1),
                border-color 140ms cubic-bezier(0.2, 0, 0, 1),
                box-shadow 140ms cubic-bezier(0.2, 0, 0, 1);
}

entry.search-address-entry:hover {
    background-color: alpha(currentColor, 0.085);
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
    transition: padding 150ms cubic-bezier(0.2, 0, 0, 1);
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
    border: 1px solid transparent;
    box-shadow: none;
    transition: border-radius 150ms cubic-bezier(0.2, 0, 0, 1),
                box-shadow 150ms cubic-bezier(0.2, 0, 0, 1);
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
    box-shadow: 0 2px 8px alpha(#000000, 0.10), 0 1px 2px alpha(#000000, 0.06);
    transition: opacity 140ms cubic-bezier(0.2, 0, 0, 1);
}

box.search-find-bar {
    background-color: @headerbar_bg_color;
    border-bottom: 1px solid alpha(currentColor, 0.10);
    padding: 6px 10px;
}

box.search-find-toggles {
    padding: 2px;
    border-radius: 9px;
    background-color: alpha(currentColor, 0.05);
    border: 1px solid alpha(currentColor, 0.07);
}

togglebutton.search-find-toggle,
button.search-find-toggle {
    min-width: 28px;
    min-height: 26px;
    padding: 0 7px;
    border-radius: 7px;
    border: 1px solid transparent;
    background-color: transparent;
    font-size: 0.78rem;
    font-weight: 600;
    opacity: 0.76;
    transition: background-color 120ms cubic-bezier(0.2, 0, 0, 1),
                border-color 120ms cubic-bezier(0.2, 0, 0, 1),
                color 120ms cubic-bezier(0.2, 0, 0, 1),
                opacity 120ms cubic-bezier(0.2, 0, 0, 1);
}

togglebutton.search-find-toggle:hover,
button.search-find-toggle:hover {
    opacity: 1.0;
    background-color: alpha(currentColor, 0.08);
}

togglebutton.search-find-toggle:checked,
button.search-find-toggle.active {
    opacity: 1.0;
    background-color: alpha(@accent_bg_color, 0.18);
    border-color: alpha(@accent_bg_color, 0.42);
    color: @accent_color;
}

label.search-find-count {
    font-variant-numeric: tabular-nums;
    opacity: 0.76;
    font-size: 0.83rem;
    min-width: 72px;
    padding: 2px 6px;
    border-radius: 6px;
}

label.search-find-count.no-matches {
    opacity: 1.0;
    color: @warning_color;
    background-color: alpha(@warning_color, 0.14);
    font-weight: 600;
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
    transition: border-color 140ms cubic-bezier(0.2, 0, 0, 1);
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
    transition: background-color 130ms cubic-bezier(0.2, 0, 0, 1),
                border-color 130ms cubic-bezier(0.2, 0, 0, 1),
                box-shadow 130ms cubic-bezier(0.2, 0, 0, 1),
                transform 150ms cubic-bezier(0.2, 0, 0, 1);
}

button.search-fav-tile:hover {
    background-color: alpha(currentColor, 0.10);
    border-color: alpha(currentColor, 0.10);
    box-shadow: 0 2px 6px alpha(#000000, 0.08), 0 1px 2px alpha(#000000, 0.05);
}

button.search-fav-tile.active {
    background-color: @view_bg_color;
    border-color: alpha(@accent_bg_color, 0.55);
    box-shadow: 0 2px 8px alpha(#000000, 0.10), 0 1px 2px alpha(#000000, 0.06);
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
    transition: background-color 120ms cubic-bezier(0.2, 0, 0, 1);
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
    transition: background-color 120ms cubic-bezier(0.2, 0, 0, 1),
                box-shadow 120ms cubic-bezier(0.2, 0, 0, 1);
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
    box-shadow: 0 1px 3px alpha(#000000, 0.07), 0 0 1px alpha(#000000, 0.05);
    font-weight: 500;
}

button.search-tab-mini-btn {
    min-width: 22px;
    min-height: 22px;
    padding: 0;
    border-radius: 6px;
    opacity: 0.68;
    transition: opacity 120ms cubic-bezier(0.2, 0, 0, 1),
                background-color 120ms cubic-bezier(0.2, 0, 0, 1),
                transform 150ms cubic-bezier(0.2, 0, 0, 1);
}

box.search-sidebar row.search-tab-row button.search-tab-mini-btn,
box.search-sidebar box.search-tab-row button.search-tab-mini-btn {
    opacity: 0;
}

box.search-sidebar row.search-tab-row:hover button.search-tab-mini-btn,
box.search-sidebar row.search-tab-row:focus-within button.search-tab-mini-btn,
box.search-sidebar row.search-tab-row.active button.search-tab-mini-btn,
box.search-sidebar box.search-tab-row:hover button.search-tab-mini-btn,
box.search-sidebar box.search-tab-row:focus-within button.search-tab-mini-btn,
box.search-sidebar box.search-tab-row.active button.search-tab-mini-btn {
    opacity: 0.78;
}

button.search-tab-mini-btn:hover,
box.search-sidebar row.search-tab-row button.search-tab-mini-btn:hover,
box.search-sidebar row.search-tab-row button.search-tab-mini-btn:focus-visible,
box.search-sidebar box.search-tab-row button.search-tab-mini-btn:hover,
box.search-sidebar box.search-tab-row button.search-tab-mini-btn:focus-visible {
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
    transition: opacity 120ms cubic-bezier(0.2, 0, 0, 1),
                background-color 120ms cubic-bezier(0.2, 0, 0, 1);
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
    transition: opacity 120ms cubic-bezier(0.2, 0, 0, 1),
                background-color 120ms cubic-bezier(0.2, 0, 0, 1);
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
    transition: background-color 120ms cubic-bezier(0.2, 0, 0, 1),
                color 120ms cubic-bezier(0.2, 0, 0, 1);
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
    transition: background-color 120ms cubic-bezier(0.2, 0, 0, 1),
                border-color 120ms cubic-bezier(0.2, 0, 0, 1);
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
 * Paleta de comandos y selector `Ctrl+K`, desplegable del omnibox y paneles
 * ============================================================================ */
box.search-switcher-root {
    padding: 12px;
}

listbox.search-switcher-list {
    background-color: transparent;
}

listbox.search-switcher-list row.search-switcher-row {
    border-radius: 8px;
    padding: 7px 10px;
    margin: 1px 0;
    transition: background-color 120ms cubic-bezier(0.2, 0, 0, 1);
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
    font-weight: 600;
    padding: 2px 7px;
    border-radius: 999px;
    background-color: alpha(currentColor, 0.09);
    opacity: 0.86;
}

label.search-switcher-badge.command {
    background-color: alpha(@accent_bg_color, 0.16);
    color: @accent_color;
    opacity: 1.0;
}

label.search-switcher-section {
    font-size: 0.72rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    opacity: 0.56;
    padding-left: 4px;
}

popover.search-omnibox-popover > contents {
    padding: 4px;
    border-radius: 12px;
    border: 1px solid transparent;
    box-shadow: 0 8px 24px alpha(#000000, 0.16), 0 2px 6px alpha(#000000, 0.08);
}

listbox.search-omnibox-list {
    background-color: transparent;
}

listbox.search-omnibox-list row.search-omnibox-row {
    border-radius: 8px;
    margin: 1px 0;
    transition: background-color 120ms cubic-bezier(0.2, 0, 0, 1);
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
    transition: background-color 120ms cubic-bezier(0.2, 0, 0, 1);
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
    box-shadow: 0 4px 14px alpha(#000000, 0.12), 0 1px 3px alpha(#000000, 0.06);
    opacity: 0.76;
    transition: opacity 120ms cubic-bezier(0.2, 0, 0, 1);
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
    box-shadow: 0 6px 20px alpha(#000000, 0.14), 0 1px 4px alpha(#000000, 0.08);
}

/* ============================================================================
 * Split View (`GtkPaned`) y Picture-in-Picture (`GtkWindow`)
 * ============================================================================ */
paned.search-split-paned > separator {
    min-width: 5px;
    background-color: alpha(currentColor, 0.08);
    transition: background-color 140ms cubic-bezier(0.2, 0, 0, 1);
}

paned.search-split-paned > separator:hover {
    background-color: alpha(@accent_bg_color, 0.45);
}

box.search-split-pane {
    border: 2px solid transparent;
    transition: border-color 140ms cubic-bezier(0.2, 0, 0, 1);
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

/* ============================================================================
 * Arc Spaces, Omnibox Superpowers, Peek Card y Tidy Tabs
 * ============================================================================ */
.space-pill,
button.space-pill {
    font-size: 0.78rem;
    font-weight: 500;
    padding: 3px 10px;
    border-radius: 999px;
    border: 1px solid transparent;
    background-color: alpha(currentColor, 0.06);
    transition: background-color 120ms cubic-bezier(0.2, 0, 0, 1),
                border-color 120ms cubic-bezier(0.2, 0, 0, 1),
                color 120ms cubic-bezier(0.2, 0, 0, 1);
}

.space-pill:hover,
button.space-pill:hover {
    background-color: alpha(currentColor, 0.11);
}

.space-pill.active,
button.space-pill.active {
    background-color: alpha(@accent_bg_color, 0.18);
    border-color: alpha(@accent_bg_color, 0.45);
    color: @accent_color;
    font-weight: 600;
}

row.omnibox-calc-row,
listbox.search-omnibox-list row.omnibox-calc-row {
    border-radius: 8px;
    margin: 2px 0;
    background-color: alpha(@accent_bg_color, 0.10);
    border: 1px solid alpha(@accent_bg_color, 0.24);
    transition: background-color 120ms cubic-bezier(0.2, 0, 0, 1),
                border-color 120ms cubic-bezier(0.2, 0, 0, 1);
}

row.omnibox-calc-row:hover,
listbox.search-omnibox-list row.omnibox-calc-row:hover,
row.omnibox-calc-row:selected,
listbox.search-omnibox-list row.omnibox-calc-row:selected {
    background-color: alpha(@accent_bg_color, 0.18);
    border-color: alpha(@accent_bg_color, 0.42);
}

.color-swatch {
    min-width: 24px;
    min-height: 24px;
    border-radius: 6px;
    border: 1px solid alpha(currentColor, 0.22);
    box-shadow: 0 1px 3px alpha(#000000, 0.12);
}

.peek-card {
    background-color: @window_bg_color;
    color: @window_fg_color;
    border-radius: 14px;
    border: 1px solid alpha(currentColor, 0.12);
    box-shadow: 0 16px 40px alpha(#000000, 0.24), 0 4px 12px alpha(#000000, 0.10);
    padding: 8px;
}

.tidy-btn,
button.tidy-btn {
    font-size: 0.76rem;
    font-weight: 600;
    padding: 3px 10px;
    border-radius: 999px;
    border: 1px solid alpha(@accent_bg_color, 0.28);
    background-color: alpha(@accent_bg_color, 0.10);
    color: @accent_color;
    transition: background-color 120ms cubic-bezier(0.2, 0, 0, 1),
                border-color 120ms cubic-bezier(0.2, 0, 0, 1);
}

.tidy-btn:hover,
button.tidy-btn:hover {
    background-color: alpha(@accent_bg_color, 0.18);
    border-color: alpha(@accent_bg_color, 0.45);
}
"#;

/// Registra el proveedor CSS de la aplicación en el `GdkDisplay` activo.
pub fn install_css() {
    if let Some(display) = Display::default_display() {
        let provider = CssProvider::new();
        provider.load_from_string(BASE_CSS);
        provider.add_to_display(&display, STYLE_PROVIDER_PRIORITY_APPLICATION);

        THEME_SWITCH_PROVIDER.with(|slot| {
            let mut guard = slot.borrow_mut();
            if guard.is_none() {
                let switch_provider = CssProvider::new();
                switch_provider.add_to_display(&display, STYLE_PROVIDER_PRIORITY_APPLICATION + 1);
                *guard = Some(switch_provider);
            }
        });
    }
}

/// Aplica el esquema de color (`system`, `light` o `dark`) sobre `AdwStyleManager`,
/// suprimiendo transiciones durante el cambio de tema para evitar parpadeos de color.
pub fn apply_color_look(style_manager: &StyleManager, look: ColorLook) {
    let scheme = match look {
        ColorLook::System => ColorScheme::Default,
        ColorLook::Light => ColorScheme::ForceLight,
        ColorLook::Dark => ColorScheme::ForceDark,
    };

    THEME_SWITCH_PROVIDER.with(|slot| {
        if let Some(provider) = slot.borrow().as_ref() {
            provider.load_from_string("* { transition-duration: 0ms; }");
            let p_restore = provider.clone();
            glib::timeout_add_local_once(Duration::from_millis(32), move || {
                p_restore.load_from_string("");
            });
        }
    });

    style_manager.set_color_scheme(scheme);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_css_has_no_gtk4_deprecated_logical_properties_or_transition_all() {
        assert!(!BASE_CSS.contains("margin-start"));
        assert!(!BASE_CSS.contains("margin-end"));
        assert!(!BASE_CSS.contains("padding-start"));
        assert!(!BASE_CSS.contains("padding-end"));
        assert!(!BASE_CSS.contains("transition: all"));
        assert!(!BASE_CSS.contains("transition-property: all"));
    }

    #[test]
    fn base_css_loads_into_gtk_css_provider() {
        let _guard = crate::GTK_TEST_MUTEX
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if !gnome_platform::init() || gnome_platform::gdk::Display::default_display().is_none() {
            return;
        }
        let provider = CssProvider::new();
        provider.load_from_string(BASE_CSS);
    }

    #[test]
    fn base_css_contains_arc_and_omnibox_superpower_classes() {
        for selector in [
            ".space-pill",
            ".space-pill.active",
            ".omnibox-calc-row",
            ".color-swatch",
            ".peek-card",
            ".tidy-btn",
        ] {
            assert!(BASE_CSS.contains(selector), "missing selector {selector}");
        }
    }
}
