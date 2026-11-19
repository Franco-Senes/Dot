//! Selector difuso de pestañas abiertas (`Ctrl+K`) sobre **todas** las ventanas (`TabSwitcher.swift`).
//!
//! Usa `search_core::fuzzy::fuzzy_match_tab` sobre título y URL.
//! Teclas: `↑`/`↓` moverse, `Enter` activar (y presentar su ventana),
//! `Ctrl+W`/`Supr` cerrar la pestaña marcada (si no está fijada), `Esc` cerrar el selector.

use crate::icons::{self, IconId};
use gnome_platform::adw::{ApplicationWindow, Dialog};
use gnome_platform::gdk::{self, ModifierType, Texture};
use gnome_platform::gtk::{
    Align, AsController, AsEditable, AsWidget, BoxWidget, EllipsizeMode, EventControllerKey, Image,
    Label, ListBox, ListBoxRow, Orientation, PolicyType, PropagationPhase, ScrolledWindow,
    SearchEntry, SelectionMode,
};
use search_core::address::sanitize_url_for_ui;
use search_core::fuzzy::fuzzy_match_tab;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Clone)]
pub struct SwitcherTabItem {
    pub window_id: u64,
    pub window_label: Option<String>,
    pub is_private: bool,
    pub tab_id: u64,
    pub title: String,
    pub url: String,
    pub pinned: bool,
    pub active: bool,
    pub audible: bool,
    pub muted: bool,
    pub favicon: Option<Texture>,
}

pub struct TabSwitcherCallbacks {
    pub list_tabs: Box<dyn Fn() -> Vec<SwitcherTabItem>>,
    pub on_activate: Box<dyn Fn(u64, u64)>,
    pub on_close_tab: Box<dyn Fn(u64, u64) -> bool>,
}

/// Abre el diálogo centrado `Ctrl+K` sobre `parent`.
pub fn present_tab_switcher(parent: &ApplicationWindow, callbacks: TabSwitcherCallbacks) {
    let dialog = Dialog::new();
    dialog.set_title("Cambiar de pestaña");
    dialog.set_content_width(580);
    dialog.set_content_height(420);
    dialog.set_follows_content_size(false);

    let root = BoxWidget::new(Orientation::Vertical, 8);
    root.add_css_class("search-switcher-root");

    let search = SearchEntry::new();
    search.set_placeholder_text("Buscar pestañas abiertas por título o dirección…");
    search.set_hexpand(true);

    let list_box = ListBox::new();
    list_box.add_css_class("search-switcher-list");
    list_box.set_selection_mode(SelectionMode::Single);
    list_box.set_activate_on_single_click(true);

    let scrolled = ScrolledWindow::new();
    scrolled.set_policy(PolicyType::Never, PolicyType::Automatic);
    scrolled.set_vexpand(true);
    scrolled.set_child(Some(&list_box));

    let hint = Label::new(Some(
        "↑↓ Navegar  ·  Enter Abrir  ·  Ctrl+W / Supr Cerrar  ·  Esc Salir",
    ));
    hint.add_css_class("search-switcher-url");
    hint.set_halign(Align::Center);

    root.append(&search);
    root.append(&scrolled);
    root.append(&hint);

    dialog.set_child(Some(&root));
    dialog.set_focus_widget(Some(&search));

    let cbs = Rc::new(callbacks);
    let filtered: Rc<RefCell<Vec<SwitcherTabItem>>> = Rc::new(RefCell::new(Vec::new()));
    let selected_idx: Rc<Cell<usize>> = Rc::new(Cell::new(0));

    let refresh = {
        let cbs_ref = cbs.clone();
        let list_ref = list_box.clone();
        let filt_ref = filtered.clone();
        let sel_ref = selected_idx.clone();
        let search_ref = search.clone();
        Rc::new(move |preferred_index: Option<usize>| {
            let query = search_ref.text();
            let all_tabs = (cbs_ref.list_tabs)();

            let mut scored: Vec<(i64, usize, SwitcherTabItem)> = all_tabs
                .into_iter()
                .enumerate()
                .filter_map(|(ord, item)| {
                    let score = fuzzy_match_tab(&query, &item.title, &item.url)?;
                    Some((score, ord, item))
                })
                .collect();

            if !query.trim().is_empty() {
                scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
            }

            let items: Vec<SwitcherTabItem> = scored.into_iter().map(|(_, _, it)| it).collect();
            list_ref.remove_all();

            for item in &items {
                list_ref.append(&build_switcher_row(item));
            }

            let count = items.len();
            *filt_ref.borrow_mut() = items;

            if count > 0 {
                let target = preferred_index.unwrap_or(0).min(count - 1);
                sel_ref.set(target);
                if let Some(row) = list_ref.row_at_index(target as i32) {
                    list_ref.select_row(Some(&row));
                }
            } else {
                sel_ref.set(0);
            }
        })
    };

    refresh(None);

    // Filtrado en vivo con cada pulsación.
    {
        let refresh_change = refresh.clone();
        search.connect_search_changed(move || {
            refresh_change(Some(0));
        });
    }

    // Enter desde el cuadro de búsqueda activa la fila seleccionada.
    {
        let dialog_act = dialog.clone();
        let cbs_act = cbs.clone();
        let filt_act = filtered.clone();
        let sel_act = selected_idx.clone();
        search.connect_activate(move || {
            let item = filt_act.borrow().get(sel_act.get()).cloned();
            if let Some(it) = item {
                dialog_act.force_close();
                (cbs_act.on_activate)(it.window_id, it.tab_id);
            }
        });
    }

    // Esc desde el cuadro de búsqueda cierra el selector.
    {
        let dialog_stop = dialog.clone();
        search.connect_stop_search(move || {
            dialog_stop.force_close();
        });
    }

    // Clic sobre una fila activa esa pestaña.
    {
        let dialog_row = dialog.clone();
        let cbs_row = cbs.clone();
        let filt_row = filtered.clone();
        list_box.connect_row_activated(move |row| {
            let idx = row.index();
            if idx >= 0 {
                let item = filt_row.borrow().get(idx as usize).cloned();
                if let Some(it) = item {
                    dialog_row.force_close();
                    (cbs_row.on_activate)(it.window_id, it.tab_id);
                }
            }
        });
    }

    // Mantener `selected_idx` sincronizado si se selecciona con ratón.
    {
        let sel_track = selected_idx.clone();
        list_box.connect_row_selected(move |opt_row| {
            if let Some(row) = opt_row {
                let idx = row.index();
                if idx >= 0 {
                    sel_track.set(idx as usize);
                }
            }
        });
    }

    // Controlador de teclado en fase Capture para ↑, ↓, Enter, Ctrl+W, Supr y Esc.
    {
        let key = EventControllerKey::new();
        key.set_propagation_phase(PropagationPhase::Capture);
        let dialog_key = dialog.clone();
        let list_key = list_box.clone();
        let filt_key = filtered.clone();
        let sel_key = selected_idx.clone();
        let cbs_key = cbs.clone();
        let refresh_key = refresh.clone();

        key.connect_key_pressed(move |keyval, _keycode, raw_state| {
            let mods = ModifierType::from_bits_truncate(raw_state);
            let ctrl = mods.contains(ModifierType::CONTROL_MASK);
            let lower = gdk::keyval_to_lower(keyval);
            let count = filt_key.borrow().len();

            if keyval == gdk::keys::ESCAPE {
                dialog_key.force_close();
                return true;
            }

            if keyval == gdk::keys::DOWN || (ctrl && lower == 0x6e) {
                if count > 0 {
                    let next = (sel_key.get() + 1) % count;
                    sel_key.set(next);
                    if let Some(row) = list_key.row_at_index(next as i32) {
                        list_key.select_row(Some(&row));
                    }
                }
                return true;
            }

            if keyval == gdk::keys::UP || (ctrl && lower == 0x70) {
                if count > 0 {
                    let prev = if sel_key.get() == 0 {
                        count - 1
                    } else {
                        sel_key.get() - 1
                    };
                    sel_key.set(prev);
                    if let Some(row) = list_key.row_at_index(prev as i32) {
                        list_key.select_row(Some(&row));
                    }
                }
                return true;
            }

            if keyval == gdk::keys::RETURN || keyval == gdk::keys::KP_ENTER {
                let item = filt_key.borrow().get(sel_key.get()).cloned();
                if let Some(it) = item {
                    dialog_key.force_close();
                    (cbs_key.on_activate)(it.window_id, it.tab_id);
                }
                return true;
            }

            if (ctrl && lower == 0x77) || keyval == gdk::keys::DELETE {
                let cur = sel_key.get();
                let item = filt_key.borrow().get(cur).cloned();
                if let Some(it) = item {
                    if !it.pinned && (cbs_key.on_close_tab)(it.window_id, it.tab_id) {
                        refresh_key(Some(cur));
                    }
                }
                return true;
            }

            false
        });
        root.add_controller(&key);
    }

    dialog.present(Some(parent));
    search.grab_focus();
}

fn build_switcher_row(item: &SwitcherTabItem) -> ListBoxRow {
    let row = ListBoxRow::new();
    row.add_css_class("search-switcher-row");

    let hbox = BoxWidget::new(Orientation::Horizontal, 10);
    hbox.set_valign(Align::Center);

    if let Some(ref tex) = item.favicon {
        let img = Image::new();
        img.set_pixel_size(18);
        img.set_from_texture(Some(tex));
        hbox.append(&img);
    } else if item.url.starts_with("plasma://settings") {
        let img = icons::image(IconId::Settings, 17);
        hbox.append(&img);
    } else {
        let letter = icons::monogram_letter(&item.title, &item.url);
        let badge = icons::monogram_badge(&letter, &item.url, false);
        hbox.append(&badge);
    }

    let text_col = BoxWidget::new(Orientation::Vertical, 1);
    text_col.set_hexpand(true);

    let title_str = if item.title.trim().is_empty() {
        "Nueva pestaña"
    } else {
        item.title.trim()
    };
    let title_lbl = Label::new(Some(title_str));
    title_lbl.set_ellipsize(EllipsizeMode::End);
    title_lbl.set_xalign(0.0);

    let clean_url = sanitize_url_for_ui(&item.url);
    let url_lbl = Label::new(Some(&clean_url));
    url_lbl.add_css_class("search-switcher-url");
    url_lbl.set_ellipsize(EllipsizeMode::End);
    url_lbl.set_xalign(0.0);

    text_col.append(&title_lbl);
    text_col.append(&url_lbl);
    hbox.append(&text_col);

    if item.active {
        let badge = Label::new(Some("Activa"));
        badge.add_css_class("search-switcher-badge");
        hbox.append(&badge);
    }
    if item.pinned {
        let badge = Label::new(Some("Fijada"));
        badge.add_css_class("search-switcher-badge");
        hbox.append(&badge);
    }
    if item.is_private {
        let badge = Label::new(Some("Privada"));
        badge.add_css_class("search-switcher-badge");
        hbox.append(&badge);
    } else if let Some(ref win_lbl) = item.window_label {
        let badge = Label::new(Some(win_lbl));
        badge.add_css_class("search-switcher-badge");
        hbox.append(&badge);
    }
    if item.muted || item.audible {
        let audio = icons::image(
            if item.muted {
                IconId::VolumeMuted
            } else {
                IconId::VolumeHigh
            },
            14,
        );
        hbox.append(&audio);
    }

    row.set_child(Some(&hbox));
    row
}
