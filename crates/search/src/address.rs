//! Barra única minimalista de dirección y búsqueda centrada en la `AdwHeaderBar`
//! (`Omnibox.swift` + `AtRestLine.swift` + `TabOffers.swift`).
//!
//! Incluye en Fase 3:
//! - Desplegable `GtkPopover` que no roba foco con sugerencias de historial (frecencia),
//!   pestañas abiertas ("Cambiar a pestaña") y marcadores.
//! - Completado inline del dominio más probable (texto seleccionado tras el cursor con
//!   `gtk_editable_select_region`); Retroceso/Supr lo quita sin reinsertarlo.
//! - Navegación completa por teclado: `↑`/`↓`, `Enter`, `Shift+Supr` (borra entrada de
//!   historial del disco y del desplegable) y `Escape`.

use crate::icons::{self, IconId};
use gnome_platform::adw::Clamp;
use gnome_platform::gdk::{self, ModifierType};
use gnome_platform::gtk::{
    Align, AsEditable, AsWidget, BoxWidget, EllipsizeMode, Entry, EventControllerFocus,
    EventControllerKey, InputHints, InputPurpose, Label, ListBox, ListBoxRow, Orientation, Popover,
    PositionType, SelectionMode,
};
use search_core::address::{display_at_rest, sanitize_url_for_ui};
use search_core::history::{inline_completion, OmniboxSuggestion, OmniboxSuggestionKind};
use std::cell::RefCell;
use std::rc::Rc;

type SubmitCallback = Rc<dyn Fn(String)>;
type EscapeCallback = Rc<dyn Fn()>;
type SuggestProvider = Rc<dyn Fn(&str) -> Vec<OmniboxSuggestion>>;
type SwitchTabCallback = Rc<dyn Fn(u64, u64)>;
type DeleteHistoryCallback = Rc<dyn Fn(String)>;

#[derive(Clone)]
pub struct AddressBar {
    clamp: Clamp,
    entry: Entry,
    popover: Popover,
    list_box: ListBox,
    state: Rc<RefCell<AddressState>>,
}

#[derive(Default)]
struct AddressState {
    committed_url: String,
    user_typed: String,
    editing: bool,
    suppress_change: bool,
    last_was_delete: bool,
    popover_open: bool,
    suggestions: Vec<OmniboxSuggestion>,
    selected_index: Option<usize>,
    on_submit: Option<SubmitCallback>,
    on_escape: Option<EscapeCallback>,
    suggest_provider: Option<SuggestProvider>,
    on_switch_tab: Option<SwitchTabCallback>,
    on_delete_history: Option<DeleteHistoryCallback>,
}

impl AddressBar {
    pub fn new() -> Self {
        let entry = Entry::new();
        entry.set_placeholder_text("Buscar o escribir dirección");
        entry.set_input_purpose(InputPurpose::Url);
        entry.set_input_hints(InputHints::NO_SPELLCHECK | InputHints::NO_EMOJI);
        entry.set_hexpand(true);
        entry.set_alignment(0.5);
        entry.add_css_class("search-address-entry");

        let clamp = Clamp::new();
        clamp.set_maximum_size(560);
        clamp.set_tightening_threshold(360);
        clamp.set_hexpand(true);
        clamp.set_child(Some(&entry));

        // Popover discreto que NO roba el foco del Entry.
        let popover = Popover::new();
        popover.add_css_class("search-omnibox-popover");
        popover.set_parent(&entry);
        popover.set_autohide(false);
        popover.set_has_arrow(false);
        popover.set_position(PositionType::Bottom);
        popover.set_offset(0, 4);
        popover.set_focusable(false);

        let list_box = ListBox::new();
        list_box.add_css_class("search-omnibox-list");
        list_box.set_selection_mode(SelectionMode::Single);
        list_box.set_activate_on_single_click(true);
        list_box.set_focusable(false);
        list_box.set_size_request(520, -1);
        popover.set_child(Some(&list_box));

        let state = Rc::new(RefCell::new(AddressState::default()));

        let bar = Self {
            clamp,
            entry: entry.clone(),
            popover: popover.clone(),
            list_box: list_box.clone(),
            state: state.clone(),
        };

        // Foco del campo de dirección.
        {
            let focus_ctrl = EventControllerFocus::new();
            let bar_enter = bar.clone();
            focus_ctrl.connect_enter(move || {
                let full = {
                    let mut st = bar_enter.state.borrow_mut();
                    st.editing = true;
                    st.suppress_change = true;
                    st.last_was_delete = false;
                    st.selected_index = None;
                    if st.committed_url.is_empty() || st.committed_url == "about:blank" {
                        String::new()
                    } else {
                        sanitize_url_for_ui(&st.committed_url)
                    }
                };
                bar_enter.entry.set_alignment(0.0);
                bar_enter.entry.set_text(&full);
                bar_enter.state.borrow_mut().suppress_change = false;

                let entry_select = bar_enter.entry.clone();
                gnome_platform::glib::idle_add_local_once(move || {
                    entry_select.select_all();
                });
            });

            let bar_leave = bar.clone();
            focus_ctrl.connect_leave(move || {
                // Diferimos brevemente el cierre por si el clic iba dirigido a una fila del popover.
                let b = bar_leave.clone();
                gnome_platform::glib::idle_add_local_once(move || {
                    if b.entry.has_focus() {
                        return;
                    }
                    b.close_popover();
                    let rest_text = {
                        let mut st = b.state.borrow_mut();
                        st.editing = false;
                        st.suppress_change = true;
                        display_at_rest(&st.committed_url)
                    };
                    b.entry.set_alignment(0.5);
                    b.entry.set_text(&rest_text);
                    b.state.borrow_mut().suppress_change = false;
                });
            });

            entry.add_controller(&focus_ctrl);
        }

        // Cambios al teclear: sugerencias + completado inline del dominio más probable.
        {
            let bar_chg = bar.clone();
            entry.connect_changed(move || {
                bar_chg.on_entry_changed();
            });
        }

        // Enter confirma sugerencia seleccionada o la dirección/búsqueda tecleada.
        {
            let bar_act = bar.clone();
            entry.connect_activate(move || {
                bar_act.activate_current();
            });
        }

        // Clic en una fila del desplegable de sugerencias.
        {
            let bar_row = bar.clone();
            list_box.connect_row_activated(move |row| {
                let idx = row.index();
                if idx >= 0 {
                    bar_row.activate_suggestion_at(idx as usize);
                }
            });
        }

        // Teclado: ↑/↓, Shift+Supr, Retroceso/Supr (control de inline) y Escape.
        {
            let key_ctrl = EventControllerKey::new();
            let bar_key = bar.clone();
            key_ctrl.connect_key_pressed(move |keyval, _keycode, mods| {
                bar_key.handle_key_pressed(keyval, ModifierType::from_bits_truncate(mods))
            });
            entry.add_controller(&key_ctrl);
        }

        bar
    }

    pub fn widget(&self) -> &Clamp {
        &self.clamp
    }

    pub fn entry(&self) -> &Entry {
        &self.entry
    }

    pub fn has_focus(&self) -> bool {
        self.entry.has_focus()
    }

    /// Desancla el popover antes de destruir la ventana.
    pub fn cleanup(&self) {
        self.close_popover();
        self.popover.unparent();
    }

    /// Enfoca la barra de direcciones y selecciona todo su contenido (`Ctrl+L` / `F6` / `Alt+D`).
    pub fn focus_and_select(&self) {
        self.close_popover();
        let full = {
            let mut st = self.state.borrow_mut();
            st.editing = true;
            st.suppress_change = true;
            st.last_was_delete = false;
            st.selected_index = None;
            if st.committed_url.is_empty() || st.committed_url == "about:blank" {
                String::new()
            } else {
                sanitize_url_for_ui(&st.committed_url)
            }
        };
        self.entry.set_alignment(0.0);
        self.entry.set_text(&full);
        self.state.borrow_mut().suppress_change = false;
        self.entry.grab_focus();
        self.entry.select_all();
    }

    /// Actualiza la URL confirmada por el navegador sin pisar lo que el usuario esté tecleando.
    pub fn set_committed_url(&self, raw_url: &str) {
        let clean_committed = sanitize_url_for_ui(raw_url);
        let is_insecure_http = clean_committed.starts_with("http://")
            && url::Url::parse(&clean_committed)
                .ok()
                .and_then(|u| {
                    u.host_str()
                        .map(|h| !search_core::address::is_loopback_host(h))
                })
                .unwrap_or(false);

        if is_insecure_http {
            self.entry.add_css_class("insecure-http");
        } else {
            self.entry.remove_css_class("insecure-http");
        }

        let editing = {
            let mut st = self.state.borrow_mut();
            st.committed_url = clean_committed.clone();
            st.editing || self.entry.has_focus()
        };

        if !editing {
            let rest = display_at_rest(&clean_committed);
            self.state.borrow_mut().suppress_change = true;
            self.entry.set_alignment(0.5);
            self.entry.set_text(&rest);
            self.state.borrow_mut().suppress_change = false;
        }
    }

    #[allow(dead_code)]
    pub fn committed_url(&self) -> String {
        self.state.borrow().committed_url.clone()
    }

    pub fn connect_submit(&self, cb: impl Fn(String) + 'static) {
        self.state.borrow_mut().on_submit = Some(Rc::new(cb));
    }

    pub fn connect_escape(&self, cb: impl Fn() + 'static) {
        self.state.borrow_mut().on_escape = Some(Rc::new(cb));
    }

    pub fn connect_suggest_provider(
        &self,
        provider: impl Fn(&str) -> Vec<OmniboxSuggestion> + 'static,
    ) {
        self.state.borrow_mut().suggest_provider = Some(Rc::new(provider));
    }

    pub fn connect_switch_tab(&self, cb: impl Fn(u64, u64) + 'static) {
        self.state.borrow_mut().on_switch_tab = Some(Rc::new(cb));
    }

    pub fn connect_delete_history(&self, cb: impl Fn(String) + 'static) {
        self.state.borrow_mut().on_delete_history = Some(Rc::new(cb));
    }

    fn on_entry_changed(&self) {
        let (should_run, was_delete, provider) = {
            let st = self.state.borrow();
            if st.suppress_change || !st.editing {
                return;
            }
            (true, st.last_was_delete, st.suggest_provider.clone())
        };
        if !should_run {
            return;
        }

        let typed = self.entry.text();
        {
            let mut st = self.state.borrow_mut();
            st.user_typed = typed.clone();
            st.selected_index = None;
        }

        if typed.trim().is_empty() {
            self.close_popover();
            return;
        }

        let Some(provider) = provider else {
            self.close_popover();
            return;
        };

        let suggestions = provider(&typed);
        if suggestions.is_empty() {
            self.close_popover();
            return;
        }

        // Completado inline del dominio más probable solo al escribir hacia delante
        // y cuando el cursor está al final del texto.
        if !was_delete {
            let typed_len_chars = typed.chars().count() as i32;
            let cursor_at_end =
                self.entry.position() >= typed_len_chars && self.entry.selection_bounds().is_none();
            if cursor_at_end {
                if let Some(suffix) = inline_completion(&typed, &suggestions) {
                    let combined = format!("{typed}{suffix}");
                    let total_chars = combined.chars().count() as i32;
                    self.state.borrow_mut().suppress_change = true;
                    self.entry.set_text(&combined);
                    self.entry.select_region(typed_len_chars, total_chars);
                    self.state.borrow_mut().suppress_change = false;
                }
            }
        }

        self.render_suggestions(suggestions);
    }

    fn render_suggestions(&self, suggestions: Vec<OmniboxSuggestion>) {
        self.list_box.remove_all();

        for item in &suggestions {
            let row = ListBoxRow::new();
            row.add_css_class("search-omnibox-row");
            row.set_focusable(false);

            let hbox = BoxWidget::new(Orientation::Horizontal, 8);
            hbox.set_margins(10, 10, 6, 6);

            let icon_id = match item.kind {
                OmniboxSuggestionKind::OpenTab { .. } => IconId::LayoutTabs,
                OmniboxSuggestionKind::Bookmark { .. } => IconId::Star,
                OmniboxSuggestionKind::History { .. } => IconId::History,
            };
            let icon = icons::image(icon_id, 15);
            hbox.append(&icon);

            let col = BoxWidget::new(Orientation::Vertical, 1);
            col.set_hexpand(true);

            let title = Label::new(Some(&item.title));
            title.set_xalign(0.0);
            title.set_ellipsize(EllipsizeMode::End);
            col.append(&title);

            let url_lbl = Label::new(Some(&item.display_url));
            url_lbl.add_css_class("search-omnibox-url");
            url_lbl.set_xalign(0.0);
            url_lbl.set_ellipsize(EllipsizeMode::End);
            col.append(&url_lbl);

            hbox.append(&col);

            if matches!(item.kind, OmniboxSuggestionKind::OpenTab { .. }) {
                let badge = Label::new(Some("Cambiar a pestaña"));
                badge.add_css_class("search-omnibox-badge");
                badge.set_valign(Align::Center);
                hbox.append(&badge);
            }

            row.set_child(Some(&hbox));
            self.list_box.append(&row);
        }

        let is_empty = suggestions.is_empty();
        {
            let mut st = self.state.borrow_mut();
            st.suggestions = suggestions;
            st.selected_index = None;
            st.popover_open = !is_empty;
        }

        if is_empty {
            self.popover.popdown();
        } else {
            let w = self.entry.width().max(420);
            self.list_box.set_size_request(w, -1);
            self.popover.popup();
        }
    }

    fn close_popover(&self) {
        let was_open = {
            let mut st = self.state.borrow_mut();
            let prev = st.popover_open;
            st.popover_open = false;
            st.selected_index = None;
            st.suggestions.clear();
            prev
        };
        if was_open {
            self.popover.popdown();
        }
        self.list_box.remove_all();
    }

    fn handle_key_pressed(&self, keyval: u32, mods: ModifierType) -> bool {
        if keyval == gdk::keys::ESCAPE {
            let pop_open = self.state.borrow().popover_open;
            if pop_open {
                let user_typed = self.state.borrow().user_typed.clone();
                self.close_popover();
                self.state.borrow_mut().suppress_change = true;
                self.entry.set_text(&user_typed);
                self.entry.set_position(-1);
                self.state.borrow_mut().suppress_change = false;
                return true;
            }
            let (rest, on_esc) = {
                let mut st = self.state.borrow_mut();
                st.editing = false;
                st.suppress_change = true;
                (display_at_rest(&st.committed_url), st.on_escape.clone())
            };
            self.entry.set_alignment(0.5);
            self.entry.set_text(&rest);
            self.state.borrow_mut().suppress_change = false;
            if let Some(cb) = on_esc {
                cb();
            }
            return true;
        }

        // Shift+Supr elimina la entrada de historial resaltada (disco + desplegable).
        if keyval == gdk::keys::DELETE && mods.contains(ModifierType::SHIFT_MASK) {
            let delete_target = {
                let st = self.state.borrow();
                if st.popover_open && !st.suggestions.is_empty() {
                    let idx = st.selected_index.unwrap_or(0);
                    st.suggestions.get(idx).and_then(|s| match &s.kind {
                        OmniboxSuggestionKind::History { key } => Some((
                            key.clone(),
                            st.on_delete_history.clone(),
                            st.user_typed.clone(),
                            st.suggest_provider.clone(),
                        )),
                        _ => None,
                    })
                } else {
                    None
                }
            };
            if let Some((key, Some(del_cb), user_typed, provider)) = delete_target {
                del_cb(key);
                if let Some(prov) = provider {
                    let updated = prov(&user_typed);
                    if updated.is_empty() {
                        self.close_popover();
                    } else {
                        self.render_suggestions(updated);
                    }
                }
                return true;
            }
        }

        if keyval == gdk::keys::DOWN {
            return self.move_selection(1);
        }
        if keyval == gdk::keys::UP {
            return self.move_selection(-1);
        }

        if keyval == gdk::keys::BACKSPACE || keyval == gdk::keys::DELETE {
            self.state.borrow_mut().last_was_delete = true;
            return false;
        }

        // Cualquier otra tecla de escritura reactiva el completado inline.
        if keyval != gdk::keys::SHIFT_L
            && keyval != gdk::keys::SHIFT_R
            && keyval != gdk::keys::CONTROL_L
            && keyval != gdk::keys::CONTROL_R
            && keyval != gdk::keys::ALT_L
            && keyval != gdk::keys::ALT_R
        {
            self.state.borrow_mut().last_was_delete = false;
        }
        false
    }

    fn move_selection(&self, delta: isize) -> bool {
        let preview = {
            let mut st = self.state.borrow_mut();
            if !st.popover_open || st.suggestions.is_empty() {
                return false;
            }
            let len = st.suggestions.len() as isize;
            let next = match st.selected_index {
                None => {
                    if delta > 0 {
                        0
                    } else {
                        (len - 1) as usize
                    }
                }
                Some(cur) => ((cur as isize + delta).rem_euclid(len)) as usize,
            };
            st.selected_index = Some(next);
            Some((next, st.suggestions[next].display_url.clone()))
        };

        if let Some((idx, text)) = preview {
            if let Some(row) = self.list_box.row_at_index(idx as i32) {
                self.list_box.select_row(Some(&row));
            }
            self.state.borrow_mut().suppress_change = true;
            self.entry.set_text(&text);
            self.entry.set_position(-1);
            self.state.borrow_mut().suppress_change = false;
            return true;
        }
        false
    }

    fn activate_current(&self) {
        let selected_idx = self.state.borrow().selected_index;
        if let Some(idx) = selected_idx {
            self.activate_suggestion_at(idx);
            return;
        }
        let typed = self.entry.text();
        self.close_popover();
        let submit = self.state.borrow().on_submit.clone();
        if let Some(cb) = submit {
            cb(typed);
        }
    }

    fn activate_suggestion_at(&self, idx: usize) {
        let action = {
            let st = self.state.borrow();
            st.suggestions
                .get(idx)
                .cloned()
                .map(|s| (s, st.on_submit.clone(), st.on_switch_tab.clone()))
        };
        self.close_popover();
        let Some((suggestion, on_submit, on_switch_tab)) = action else {
            return;
        };
        match suggestion.kind {
            OmniboxSuggestionKind::OpenTab { window_id, tab_id } => {
                if let Some(cb) = on_switch_tab {
                    cb(window_id, tab_id);
                }
            }
            OmniboxSuggestionKind::Bookmark { .. } | OmniboxSuggestionKind::History { .. } => {
                if let Some(cb) = on_submit {
                    cb(suggestion.url);
                }
            }
        }
    }
}

impl Default for AddressBar {
    fn default() -> Self {
        Self::new()
    }
}
