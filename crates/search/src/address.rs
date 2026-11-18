//! Barra única minimalista de dirección y búsqueda centrada en la `AdwHeaderBar`
//! (`Omnibox.swift` + `AtRestLine.swift`).
//!
//! Cuando está en reposo muestra el dominio limpio (`display_at_rest`, con indicador si es HTTP
//! no cifrado); al recibir foco muestra la URL completa saneada (sin `user:pass@`) seleccionada
//! para poder escribir de inmediato, y con `Escape` restaura la URL real y devuelve el foco.

use gnome_platform::adw::Clamp;
use gnome_platform::gdk;
use gnome_platform::gtk::{
    AsEditable, AsWidget, Entry, EventControllerFocus, EventControllerKey, InputHints, InputPurpose,
};
use search_core::address::{display_at_rest, sanitize_url_for_ui};
use std::cell::RefCell;
use std::rc::Rc;

type SubmitCallback = Box<dyn Fn(String)>;
type EscapeCallback = Box<dyn Fn()>;

#[derive(Clone)]
pub struct AddressBar {
    clamp: Clamp,
    entry: Entry,
    state: Rc<RefCell<AddressState>>,
}

#[derive(Default)]
struct AddressState {
    committed_url: String,
    editing: bool,
    suppress_change: bool,
    on_submit: Option<SubmitCallback>,
    on_escape: Option<EscapeCallback>,
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

        let state = Rc::new(RefCell::new(AddressState::default()));

        // Al entrar el foco: pasar a alineación izquierda y mostrar la URL completa seleccionada.
        {
            let focus_ctrl = EventControllerFocus::new();
            let entry_enter = entry.clone();
            let state_enter = state.clone();
            focus_ctrl.connect_enter(move || {
                let full = {
                    let mut st = state_enter.borrow_mut();
                    st.editing = true;
                    st.suppress_change = true;
                    if st.committed_url.is_empty() || st.committed_url == "about:blank" {
                        String::new()
                    } else {
                        sanitize_url_for_ui(&st.committed_url)
                    }
                };
                entry_enter.set_alignment(0.0);
                entry_enter.set_text(&full);
                state_enter.borrow_mut().suppress_change = false;

                let entry_select = entry_enter.clone();
                gnome_platform::glib::idle_add_local_once(move || {
                    entry_select.select_all();
                });
            });

            let entry_leave = entry.clone();
            let state_leave = state.clone();
            focus_ctrl.connect_leave(move || {
                let rest_text = {
                    let mut st = state_leave.borrow_mut();
                    st.editing = false;
                    st.suppress_change = true;
                    display_at_rest(&st.committed_url)
                };
                entry_leave.set_alignment(0.5);
                entry_leave.set_text(&rest_text);
                state_leave.borrow_mut().suppress_change = false;
            });

            entry.add_controller(&focus_ctrl);
        }

        // Enter confirma navegación.
        {
            let entry_act = entry.clone();
            let state_act = state.clone();
            entry.connect_activate(move || {
                let typed = entry_act.text();
                let cb = state_act.borrow().on_submit.as_ref().map(|_| ());
                if cb.is_some() {
                    if let Some(ref submit) = state_act.borrow().on_submit {
                        submit(typed);
                    }
                }
            });
        }

        // Escape restaura la URL de la página actual y devuelve el foco a la vista web.
        {
            let key_ctrl = EventControllerKey::new();
            let entry_esc = entry.clone();
            let state_esc = state.clone();
            key_ctrl.connect_key_pressed(move |keyval, _keycode, _mods| {
                if keyval == gdk::keys::ESCAPE {
                    let rest = {
                        let mut st = state_esc.borrow_mut();
                        st.editing = false;
                        st.suppress_change = true;
                        display_at_rest(&st.committed_url)
                    };
                    entry_esc.set_alignment(0.5);
                    entry_esc.set_text(&rest);
                    state_esc.borrow_mut().suppress_change = false;
                    if let Some(ref on_esc) = state_esc.borrow().on_escape {
                        on_esc();
                    }
                    return true;
                }
                false
            });
            entry.add_controller(&key_ctrl);
        }

        Self {
            clamp,
            entry,
            state,
        }
    }

    pub fn widget(&self) -> &Clamp {
        &self.clamp
    }

    #[allow(dead_code)]
    pub fn entry(&self) -> &Entry {
        &self.entry
    }

    pub fn has_focus(&self) -> bool {
        self.entry.has_focus()
    }

    /// Enfoca la barra de direcciones y selecciona todo su contenido (`Ctrl+L` / `F6` / `Alt+D`).
    pub fn focus_and_select(&self) {
        let full = {
            let mut st = self.state.borrow_mut();
            st.editing = true;
            st.suppress_change = true;
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
        self.state.borrow_mut().on_submit = Some(Box::new(cb));
    }

    pub fn connect_escape(&self, cb: impl Fn() + 'static) {
        self.state.borrow_mut().on_escape = Some(Box::new(cb));
    }
}

impl Default for AddressBar {
    fn default() -> Self {
        Self::new()
    }
}
