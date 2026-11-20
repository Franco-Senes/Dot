//! Barra discreta de búsqueda dentro de la página (`Ctrl+F`, `Ctrl+G`, `Ctrl+Shift+G`, `Escape`)
//! conectada a `BrowserHost::find` y `CefFindHandler` (`Find.swift` / `PageFind.swift`).
//!
//! Incluye contador en vivo (`1 / 12` en `tabular-nums`, `"Sin coincidencias"` en estado de aviso),
//! conmutadores de mayúsculas/minúsculas (`Aa`), palabra completa y resaltar todas las coincidencias.

use crate::icons::{self, IconId};
use gnome_platform::gdk::{self, ModifierType};
use gnome_platform::gtk::{
    Align, AsController, AsEditable, AsWidget, BoxWidget, Button, EventControllerKey, Image, Label,
    Orientation, Revealer, RevealerTransitionType, SearchEntry,
};
use std::cell::RefCell;
use std::rc::Rc;

type QueryCallback = Rc<dyn Fn(String, bool, bool)>;
type OptionsQueryCallback = Rc<dyn Fn(String, bool, bool, FindOptions)>;
type CloseCallback = Rc<dyn Fn()>;

/// Opciones activas de búsqueda dentro de la página.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FindOptions {
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub highlight_all: bool,
}

impl Default for FindOptions {
    fn default() -> Self {
        Self {
            case_sensitive: false,
            whole_word: false,
            highlight_all: true,
        }
    }
}

#[derive(Clone)]
pub struct FindBar {
    revealer: Revealer,
    entry: SearchEntry,
    count_label: Label,
    case_btn: Button,
    word_btn: Button,
    highlight_btn: Button,
    highlight_img: Image,
    state: Rc<RefCell<FindState>>,
}

#[derive(Default)]
struct FindState {
    last_query: String,
    last_options: FindOptions,
    last_status_text: String,
    options: FindOptions,
    on_find: Option<QueryCallback>,
    on_find_with_options: Option<OptionsQueryCallback>,
    on_close: Option<CloseCallback>,
}

impl FindBar {
    pub fn new() -> Self {
        let revealer = Revealer::new();
        revealer.set_transition_type(RevealerTransitionType::SlideDown);
        revealer.set_transition_duration(140);
        revealer.set_reveal_child(false);

        let row = BoxWidget::new(Orientation::Horizontal, 8);
        row.add_css_class("search-find-bar");
        row.set_hexpand(true);

        let entry = SearchEntry::new();
        entry.set_placeholder_text("Buscar en la página…");
        entry.set_hexpand(true);

        let toggles_box = BoxWidget::new(Orientation::Horizontal, 2);
        toggles_box.add_css_class("search-find-toggles");
        toggles_box.set_valign(Align::Center);

        let case_btn = Button::with_label("Aa");
        case_btn.add_css_class("flat");
        case_btn.add_css_class("search-find-toggle");
        case_btn.set_tooltip_text(Some("Distinguir mayúsculas y minúsculas (Alt+C)"));

        let word_btn = Button::with_label("Palabra");
        word_btn.add_css_class("flat");
        word_btn.add_css_class("search-find-toggle");
        word_btn.set_tooltip_text(Some("Coincidir palabra completa (Alt+W)"));

        let highlight_img = icons::image(IconId::Eye, 14);
        let highlight_btn = Button::new();
        highlight_btn.add_css_class("flat");
        highlight_btn.add_css_class("search-find-toggle");
        highlight_btn.add_css_class("active");
        highlight_btn.set_child(Some(&highlight_img));
        highlight_btn.set_tooltip_text(Some("Resaltar todas las coincidencias (Alt+H)"));

        toggles_box.append(&case_btn);
        toggles_box.append(&word_btn);
        toggles_box.append(&highlight_btn);

        let count_label = Label::new(Some(""));
        count_label.add_css_class("search-find-count");
        count_label.set_halign(Align::End);
        count_label.set_valign(Align::Center);

        let prev_btn = icons::icon_button(IconId::ArrowUp, 15);
        prev_btn.set_tooltip_text(Some("Coincidencia anterior (Shift+Enter / Ctrl+Shift+G)"));

        let next_btn = icons::icon_button(IconId::ArrowDown, 15);
        next_btn.set_tooltip_text(Some("Coincidencia siguiente (Enter / Ctrl+G)"));

        let close_btn = icons::icon_button(IconId::Close, 15);
        close_btn.set_tooltip_text(Some("Cerrar búsqueda (Escape)"));

        row.append(&entry);
        row.append(&toggles_box);
        row.append(&count_label);
        row.append(&prev_btn);
        row.append(&next_btn);
        row.append(&close_btn);
        revealer.set_child(Some(&row));

        let state = Rc::new(RefCell::new(FindState::default()));

        let bar = Self {
            revealer,
            entry,
            count_label,
            case_btn,
            word_btn,
            highlight_btn,
            highlight_img,
            state,
        };

        {
            let b = bar.clone();
            bar.entry.connect_search_changed(move || {
                b.trigger_search(true, false);
            });
        }

        {
            let b = bar.clone();
            bar.entry.connect_activate(move || {
                b.find_next();
            });
        }

        {
            let b = bar.clone();
            bar.entry.connect_next_match(move || {
                b.find_next();
            });
        }

        {
            let b = bar.clone();
            bar.entry.connect_previous_match(move || {
                b.find_previous();
            });
        }

        {
            let b = bar.clone();
            bar.entry.connect_stop_search(move || {
                b.close();
            });
        }

        {
            let b = bar.clone();
            bar.case_btn.connect_clicked(move || {
                let next = !b.is_case_sensitive();
                b.set_case_sensitive(next);
                if !b.entry.text().is_empty() {
                    b.trigger_search(true, false);
                }
            });
        }

        {
            let b = bar.clone();
            bar.word_btn.connect_clicked(move || {
                let next = !b.is_whole_word();
                b.set_whole_word(next);
                if !b.entry.text().is_empty() {
                    b.trigger_search(true, false);
                }
            });
        }

        {
            let b = bar.clone();
            bar.highlight_btn.connect_clicked(move || {
                let next = !b.is_highlight_all();
                b.set_highlight_all(next);
                if !b.entry.text().is_empty() {
                    b.trigger_search(true, false);
                }
            });
        }

        {
            let b = bar.clone();
            prev_btn.connect_clicked(move || {
                b.find_previous();
            });
        }

        {
            let b = bar.clone();
            next_btn.connect_clicked(move || {
                b.find_next();
            });
        }

        {
            let b = bar.clone();
            close_btn.connect_clicked(move || {
                b.close();
            });
        }

        {
            let key_ctrl = EventControllerKey::new();
            let b = bar.clone();
            let key_ctrl_ref = key_ctrl.clone();
            key_ctrl.connect_key_pressed(move |keyval, _keycode, _mods| {
                let mods = key_ctrl_ref.current_event_state();
                if keyval == gdk::keys::ESCAPE {
                    b.close();
                    return true;
                }
                if keyval == gdk::keys::RETURN || keyval == gdk::keys::KP_ENTER {
                    if mods.contains(ModifierType::SHIFT_MASK) {
                        b.find_previous();
                    } else {
                        b.find_next();
                    }
                    return true;
                }
                if mods.contains(ModifierType::ALT_MASK) {
                    let lower = gdk::keyval_to_lower(keyval);
                    if lower == 0x63 {
                        // Alt+C: mayúsculas/minúsculas
                        let next = !b.is_case_sensitive();
                        b.set_case_sensitive(next);
                        if !b.entry.text().is_empty() {
                            b.trigger_search(true, false);
                        }
                        return true;
                    }
                    if lower == 0x77 {
                        // Alt+W: palabra completa
                        let next = !b.is_whole_word();
                        b.set_whole_word(next);
                        if !b.entry.text().is_empty() {
                            b.trigger_search(true, false);
                        }
                        return true;
                    }
                    if lower == 0x68 {
                        // Alt+H: resaltar todo
                        let next = !b.is_highlight_all();
                        b.set_highlight_all(next);
                        if !b.entry.text().is_empty() {
                            b.trigger_search(true, false);
                        }
                        return true;
                    }
                }
                false
            });
            bar.entry.add_controller(&key_ctrl);
        }

        bar
    }

    pub fn widget(&self) -> &Revealer {
        &self.revealer
    }

    pub fn set_motion_duration(&self, ms: u32) {
        self.revealer.set_transition_duration(ms);
    }

    pub fn is_open(&self) -> bool {
        self.revealer.reveals_child()
    }

    #[allow(dead_code)]
    pub fn options(&self) -> FindOptions {
        self.state.borrow().options
    }

    #[allow(dead_code)]
    pub fn status_text(&self) -> String {
        self.state.borrow().last_status_text.clone()
    }

    pub fn is_case_sensitive(&self) -> bool {
        self.state.borrow().options.case_sensitive
    }

    pub fn is_whole_word(&self) -> bool {
        self.state.borrow().options.whole_word
    }

    pub fn is_highlight_all(&self) -> bool {
        self.state.borrow().options.highlight_all
    }

    pub fn set_case_sensitive(&self, enabled: bool) {
        self.state.borrow_mut().options.case_sensitive = enabled;
        sync_toggle_class(&self.case_btn, enabled);
    }

    pub fn set_whole_word(&self, enabled: bool) {
        self.state.borrow_mut().options.whole_word = enabled;
        sync_toggle_class(&self.word_btn, enabled);
    }

    pub fn set_highlight_all(&self, enabled: bool) {
        self.state.borrow_mut().options.highlight_all = enabled;
        sync_toggle_class(&self.highlight_btn, enabled);
        let icon = if enabled { IconId::Eye } else { IconId::EyeOff };
        icons::set_image_icon(&self.highlight_img, icon, icons::is_dark_theme());
    }

    pub fn open_and_focus(&self) {
        self.revealer.set_reveal_child(true);
        self.entry.grab_focus();
        self.entry.select_all();
        if !self.entry.text().is_empty() {
            self.trigger_search(true, false);
        }
    }

    pub fn close(&self) {
        if !self.is_open() {
            return;
        }
        self.revealer.set_reveal_child(false);
        self.count_label.set_text("");
        self.count_label.remove_css_class("no-matches");
        self.state.borrow_mut().last_query.clear();
        let on_close = self.state.borrow().on_close.clone();
        if let Some(cb) = on_close {
            cb();
        }
    }

    pub fn find_next(&self) {
        if !self.is_open() {
            self.open_and_focus();
            return;
        }
        self.trigger_search(true, true);
    }

    pub fn find_previous(&self) {
        if !self.is_open() {
            self.open_and_focus();
            return;
        }
        self.trigger_search(false, true);
    }

    pub fn set_match_status(&self, active_ordinal: i32, total_count: i32) {
        if self.entry.text().is_empty() {
            self.count_label.set_text("");
            self.count_label.remove_css_class("no-matches");
            self.state.borrow_mut().last_status_text.clear();
            return;
        }
        if total_count <= 0 {
            let msg = "Sin coincidencias";
            self.count_label.set_text(msg);
            self.count_label.add_css_class("no-matches");
            self.state.borrow_mut().last_status_text = msg.to_string();
        } else {
            let shown_ord = active_ordinal.max(1);
            let msg = format!("{shown_ord} / {total_count}");
            self.count_label.set_text(&msg);
            self.count_label.remove_css_class("no-matches");
            self.state.borrow_mut().last_status_text = msg;
        }
    }

    pub fn connect_find(&self, cb: impl Fn(String, bool, bool) + 'static) {
        self.state.borrow_mut().on_find = Some(Rc::new(cb));
    }

    #[allow(dead_code)]
    pub fn connect_find_with_options(
        &self,
        cb: impl Fn(String, bool, bool, FindOptions) + 'static,
    ) {
        self.state.borrow_mut().on_find_with_options = Some(Rc::new(cb));
    }

    pub fn connect_close(&self, cb: impl Fn() + 'static) {
        self.state.borrow_mut().on_close = Some(Rc::new(cb));
    }

    fn trigger_search(&self, forward: bool, explicit_step: bool) {
        let query = self.entry.text();
        let (find_next, options, on_find, on_find_with_options) = {
            let mut st = self.state.borrow_mut();
            let same = st.last_query == query && st.last_options == st.options;
            st.last_query = query.clone();
            st.last_options = st.options;
            (
                explicit_step && same,
                st.options,
                st.on_find.clone(),
                st.on_find_with_options.clone(),
            )
        };
        if query.is_empty() {
            self.count_label.set_text("");
            self.count_label.remove_css_class("no-matches");
        }
        if let Some(cb) = on_find_with_options {
            cb(query.clone(), forward, find_next, options);
        }
        if let Some(cb) = on_find {
            cb(query, forward, find_next);
        }
    }
}

fn sync_toggle_class(btn: &Button, active: bool) {
    if active {
        btn.add_css_class("active");
    } else {
        btn.remove_css_class("active");
    }
}

impl Default for FindBar {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_bar_match_status_and_toggles() {
        let _guard = crate::GTK_TEST_MUTEX
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if !gnome_platform::init() || gnome_platform::gdk::Display::default_display().is_none() {
            return;
        }

        let bar = FindBar::new();
        assert!(!bar.is_open());
        assert!(!bar.is_case_sensitive());
        assert!(!bar.is_whole_word());
        assert!(bar.is_highlight_all());

        bar.set_case_sensitive(true);
        bar.set_whole_word(true);
        bar.set_highlight_all(false);
        assert!(bar.is_case_sensitive());
        assert!(bar.is_whole_word());
        assert!(!bar.is_highlight_all());

        bar.entry.set_text("rust");
        bar.set_match_status(0, 0);
        assert_eq!(bar.status_text(), "Sin coincidencias");

        bar.set_match_status(3, 12);
        assert_eq!(bar.status_text(), "3 / 12");

        bar.entry.set_text("");
        bar.set_match_status(1, 5);
        assert_eq!(bar.status_text(), "");
    }
}
