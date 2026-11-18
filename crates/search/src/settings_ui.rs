//! Ventana minimalista de preferencias (`AdwPreferencesDialog`, `Ctrl+,`) — `Settings.swift`.
//!
//! Todos los cambios se persisten atómicamente mediante `SettingsStore` y se propagan
//! en vivo sin reiniciar (incluida la disposición de pestañas arriba vs barra lateral).

use crate::settings::SettingsStore;
use gnome_platform::adw::{
    ApplicationWindow, ComboRow, EntryRow, PreferencesDialog, PreferencesGroup, PreferencesPage,
    SwitchRow,
};
use gnome_platform::gtk::{AsEditable, AsWidget, StringList};
use search_core::address::SearchEngine;
use search_core::settings::ColorLook;

const ENGINE_ORDER: [SearchEngine; 5] = [
    SearchEngine::DuckDuckGo,
    SearchEngine::Google,
    SearchEngine::Brave,
    SearchEngine::Kagi,
    SearchEngine::Custom,
];

const ENGINE_LABELS: [&str; 5] = ["DuckDuckGo", "Google", "Brave", "Kagi", "Personalizado"];

const LOOK_ORDER: [ColorLook; 3] = [ColorLook::System, ColorLook::Light, ColorLook::Dark];
const LOOK_LABELS: [&str; 3] = ["Sistema", "Claro", "Oscuro"];

/// Abre el diálogo modal de ajustes sobre `parent`.
pub fn present_settings_dialog(parent: &ApplicationWindow, settings: &SettingsStore) {
    let dialog = PreferencesDialog::new();
    dialog.set_title("Preferencias");
    dialog.set_search_enabled(false);

    let page = PreferencesPage::new();
    page.set_title("General");
    page.set_icon_name("preferences-system-symbolic");

    let snap = settings.snapshot();

    // Grupo 1: Apariencia y pestañas.
    let appearance_group = PreferencesGroup::new();
    appearance_group.set_title("Apariencia y pestañas");

    let look_row = ComboRow::new();
    look_row.set_title("Esquema de color");
    look_row.set_subtitle("Sincronizado con el escritorio o forzado");
    let look_model = StringList::new(&LOOK_LABELS);
    look_row.set_string_model(&look_model);
    let look_idx = LOOK_ORDER.iter().position(|&l| l == snap.look).unwrap_or(0) as u32;
    look_row.set_selected(look_idx);
    {
        let st = settings.clone();
        let row_ref = look_row.clone();
        look_row.connect_selected_notify(move || {
            let idx = row_ref.selected() as usize;
            if let Some(&look) = LOOK_ORDER.get(idx) {
                st.update(|s| s.look = look);
            }
        });
    }
    appearance_group.add(&look_row);

    let sidebar_row = SwitchRow::new();
    sidebar_row.set_title("Barra lateral vertical de pestañas");
    sidebar_row
        .set_subtitle("Alterna al instante entre barra superior y barra lateral (Ctrl+Shift+S)");
    sidebar_row.set_active(snap.vertical_tabs);
    {
        let st = settings.clone();
        let row_ref = sidebar_row.clone();
        sidebar_row.connect_active_notify(move || {
            let vertical = row_ref.is_active();
            st.update(|s| s.vertical_tabs = vertical);
        });
    }
    appearance_group.add(&sidebar_row);
    page.add(&appearance_group);

    // Grupo 2: Búsqueda y privacidad.
    let privacy_group = PreferencesGroup::new();
    privacy_group.set_title("Búsqueda y privacidad");

    let engine_row = ComboRow::new();
    engine_row.set_title("Motor de búsqueda");
    let engine_model = StringList::new(&ENGINE_LABELS);
    engine_row.set_string_model(&engine_model);
    let engine_idx = ENGINE_ORDER
        .iter()
        .position(|&e| e == snap.search_engine)
        .unwrap_or(0) as u32;
    engine_row.set_selected(engine_idx);

    let custom_url_row = EntryRow::new();
    custom_url_row.set_title("URL de búsqueda personalizada (%s)");
    custom_url_row.set_text(&snap.custom_search_url);
    custom_url_row.set_show_apply_button(true);
    custom_url_row.set_visible(snap.search_engine == SearchEngine::Custom);

    {
        let st = settings.clone();
        let row_ref = engine_row.clone();
        let custom_ref = custom_url_row.clone();
        engine_row.connect_selected_notify(move || {
            let idx = row_ref.selected() as usize;
            if let Some(&engine) = ENGINE_ORDER.get(idx) {
                custom_ref.set_visible(engine == SearchEngine::Custom);
                st.update(|s| s.search_engine = engine);
            }
        });
    }
    {
        let st = settings.clone();
        let custom_ref = custom_url_row.clone();
        custom_url_row.connect_apply(move || {
            let val = custom_ref.text().trim().to_string();
            st.update(|s| s.custom_search_url = val);
        });
    }

    privacy_group.add(&engine_row);
    privacy_group.add(&custom_url_row);

    let cookies_row = SwitchRow::new();
    cookies_row.set_title("Bloquear cookies de terceros");
    cookies_row.set_subtitle("Evita el rastreo entre sitios distintos");
    cookies_row.set_active(snap.block_third_party_cookies());
    {
        let st = settings.clone();
        let row_ref = cookies_row.clone();
        cookies_row.connect_active_notify(move || {
            let block = row_ref.is_active();
            st.update(|s| s.set_block_third_party_cookies(block));
        });
    }
    privacy_group.add(&cookies_row);

    let gpc_row = SwitchRow::new();
    gpc_row.set_title("Enviar señal Global Privacy Control (GPC)");
    gpc_row.set_subtitle("Incluye la cabecera Sec-GPC: 1 y navigator.globalPrivacyControl");
    gpc_row.set_active(snap.send_gpc);
    {
        let st = settings.clone();
        let row_ref = gpc_row.clone();
        gpc_row.connect_active_notify(move || {
            let gpc = row_ref.is_active();
            st.update(|s| s.send_gpc = gpc);
        });
    }
    privacy_group.add(&gpc_row);

    let click_ids_row = SwitchRow::new();
    click_ids_row.set_title("Limpiar identificadores de rastreo");
    click_ids_row.set_subtitle("Elimina parámetros como utm_*, fbclid y gclid al navegar y copiar");
    click_ids_row.set_active(snap.strip_click_ids);
    {
        let st = settings.clone();
        let row_ref = click_ids_row.clone();
        click_ids_row.connect_active_notify(move || {
            let strip = row_ref.is_active();
            st.update(|s| s.strip_click_ids = strip);
        });
    }
    privacy_group.add(&click_ids_row);

    page.add(&privacy_group);
    dialog.add(&page);
    dialog.present(Some(parent));
}
