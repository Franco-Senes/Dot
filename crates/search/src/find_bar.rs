//! Barra discreta de búsqueda dentro de la página (`Ctrl+F`, `Ctrl+G`, `Ctrl+Shift+G`, `Escape`)
//! conectada a `BrowserHost::find` y `CefFindHandler` (`Find.swift` / `PageFind.swift`).

use crate::icons::{self, IconId};
use gnome_platform::gdk::{self, ModifierType};
use gnome_platform::gtk::{
    Align, AsController, AsEditable, AsWidget, BoxWidget, EventControllerKey, Label, Orientation,
    Revealer, RevealerTransitionType, SearchEntry,
};
use std::cell::RefCell;
use std::rc::Rc;

type QueryCallback = Rc<dyn Fn(String, bool, bool)>;
type CloseCallback = Rc<dyn Fn()>;

#[derive(Clone)]
pub struct FindBar {
    revealer: Revealer,
    entry: SearchEntry,
    count_label: Label,
    state: Rc<RefCell<FindState>>,
}

#[derive(Default)]
struct FindState {
    last_query: String,
    on_find: Option<QueryCallback>,
    on_close: Option<CloseCallback>,
}

impl FindBar {
    pub fn new() -> Self {
        let revealer = Revealer::new();
        revealer.set_transition_type(RevealerTransitionType::SlideDown);
        revealer.set_transition_duration(140);
        revealer.set_reveal_child(false);

        let row = BoxWidget::new(Orientation::Horizontal, 6);
        row.add_css_class("search-find-bar");
        row.set_hexpand(true);

        let entry = SearchEntry::new();
        entry.set_placeholder_text("Buscar en la página…");
        entry.set_hexpand(true);

        let count_label = Label::new(Some(""));
        count_label.add_css_class("search-find-count");
        count_label.set_halign(Align::End);

        let prev_btn = icons::icon_button(IconId::ArrowUp, 15);
        prev_btn.set_tooltip_text(Some("Coincidencia anterior (Ctrl+Shift+G)"));

        let next_btn = icons::icon_button(IconId::ArrowDown, 15);
        next_btn.set_tooltip_text(Some("Coincidencia siguiente (Ctrl+G)"));

        let close_btn = icons::icon_button(IconId::Close, 15);
        close_btn.set_tooltip_text(Some("Cerrar búsqueda (Escape)"));

        row.append(&entry);
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
            return;
        }
        if total_count <= 0 {
            self.count_label.set_text("0 de 0");
        } else {
            let shown_ord = active_ordinal.max(1);
            self.count_label
                .set_text(&format!("{shown_ord} de {total_count}"));
        }
    }

    pub fn connect_find(&self, cb: impl Fn(String, bool, bool) + 'static) {
        self.state.borrow_mut().on_find = Some(Rc::new(cb));
    }

    pub fn connect_close(&self, cb: impl Fn() + 'static) {
        self.state.borrow_mut().on_close = Some(Rc::new(cb));
    }

    fn trigger_search(&self, forward: bool, explicit_step: bool) {
        let query = self.entry.text();
        let (find_next, on_find) = {
            let mut st = self.state.borrow_mut();
            let same = st.last_query == query;
            st.last_query = query.clone();
            (explicit_step && same, st.on_find.clone())
        };
        if query.is_empty() {
            self.count_label.set_text("");
        }
        if let Some(cb) = on_find {
            cb(query, forward, find_next);
        }
    }
}

impl Default for FindBar {
    fn default() -> Self {
        Self::new()
    }
}
