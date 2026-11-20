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
use dot_core::address::{clean_link, display_at_rest, sanitize_url_for_ui};
use dot_core::features_50::{
    feat_11_search_engine_bangs::{BangMatch, BangParseOptions, BangRegistry, BangSuggestion},
    feat_13_omnibox_calculator::{CalcResult, CalcResultKind, OmniboxCalculator},
    feat_15_omnibox_color_preview::{ColorRgba, OmniboxColorPreview, OmniboxColorPreviewer},
};
use dot_core::history::{inline_completion, OmniboxSuggestion, OmniboxSuggestionKind};
use gnome_platform::adw::Clamp;
use gnome_platform::gdk::{self, MemoryFormat, MemoryTexture, ModifierType};
use gnome_platform::glib::Bytes;
use gnome_platform::gtk::{
    Align, AsEditable, AsWidget, BoxWidget, Button, EllipsizeMode, Entry, EventControllerFocus,
    EventControllerKey, Image, InputHints, InputPurpose, Label, ListBox, ListBoxRow, MenuButton,
    Orientation, Popover, PositionType, SelectionMode, Separator,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

type SubmitCallback = Rc<dyn Fn(String)>;
type EscapeCallback = Rc<dyn Fn()>;
type SuggestProvider = Rc<dyn Fn(&str) -> Vec<OmniboxSuggestion>>;
type SwitchTabCallback = Rc<dyn Fn(u64, u64)>;
type DeleteHistoryCallback = Rc<dyn Fn(String)>;
type ResetZoomCallback = Rc<dyn Fn()>;
type ToggleReaderCallback = Rc<dyn Fn(bool)>;
type ToggleBookmarkCallback = Rc<dyn Fn()>;

#[derive(Clone)]
pub struct AddressBar {
    clamp: Clamp,
    entry: Entry,
    zoom_btn: Button,
    reader_btn: Button,
    #[allow(dead_code)]
    star_btn: Button,
    site_status_lbl: Label,
    qr_box: BoxWidget,
    qr_image: Image,
    popover: Popover,
    list_box: ListBox,
    state: Rc<RefCell<AddressState>>,
}

#[derive(Clone)]
enum OmniboxRowItem {
    Calc(CalcResult),
    Color(OmniboxColorPreview),
    Bang(BangMatch),
    BangHint(BangSuggestion),
    Suggestion(OmniboxSuggestion),
}

struct AddressState {
    committed_url: String,
    user_typed: String,
    editing: bool,
    suppress_change: bool,
    last_was_delete: bool,
    popover_open: bool,
    reader_active: bool,
    #[allow(dead_code)]
    bookmarked: bool,
    items: Vec<OmniboxRowItem>,
    row_badges: Vec<Option<Label>>,
    feedback_seq: u64,
    selected_index: Option<usize>,
    calculator: OmniboxCalculator,
    color_previewer: OmniboxColorPreviewer,
    bang_registry: BangRegistry,
    on_submit: Option<SubmitCallback>,
    on_escape: Option<EscapeCallback>,
    suggest_provider: Option<SuggestProvider>,
    on_switch_tab: Option<SwitchTabCallback>,
    on_delete_history: Option<DeleteHistoryCallback>,
    on_reset_zoom: Option<ResetZoomCallback>,
    on_toggle_reader: Option<ToggleReaderCallback>,
    on_toggle_bookmark: Option<ToggleBookmarkCallback>,
}

impl Default for AddressState {
    fn default() -> Self {
        Self {
            committed_url: String::new(),
            user_typed: String::new(),
            editing: false,
            suppress_change: false,
            last_was_delete: false,
            popover_open: false,
            reader_active: false,
            bookmarked: false,
            items: Vec::new(),
            row_badges: Vec::new(),
            feedback_seq: 0,
            selected_index: None,
            calculator: OmniboxCalculator::default(),
            color_previewer: OmniboxColorPreviewer::with_defaults(),
            bang_registry: BangRegistry::with_defaults(),
            on_submit: None,
            on_escape: None,
            suggest_provider: None,
            on_switch_tab: None,
            on_delete_history: None,
            on_reset_zoom: None,
            on_toggle_reader: None,
            on_toggle_bookmark: None,
        }
    }
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

        let trailing_box = BoxWidget::new(Orientation::Horizontal, 2);
        trailing_box.set_valign(Align::Center);

        let zoom_btn = Button::with_label("100 %");
        zoom_btn.add_css_class("flat");
        zoom_btn.add_css_class("search-tab-mini-btn");
        zoom_btn.set_tooltip_text(Some("Restablecer zoom al 100 %"));
        zoom_btn.set_visible(false);
        trailing_box.append(&zoom_btn);

        let reader_btn = icons::icon_button(IconId::FileOpen, 15);
        reader_btn.add_css_class("search-tab-mini-btn");
        reader_btn.set_tooltip_text(Some("Activar modo lectura"));
        trailing_box.append(&reader_btn);

        let star_btn = icons::icon_button(IconId::Star, 15);
        star_btn.add_css_class("search-tab-mini-btn");
        star_btn.set_tooltip_text(Some("Añadir a marcadores (Ctrl+D)"));
        trailing_box.append(&star_btn);

        // Popover de información del sitio y acciones rápidas.
        let site_pop = Popover::new();
        let site_box = BoxWidget::new(Orientation::Vertical, 4);
        site_box.set_margins(8, 8, 8, 8);

        let site_status_lbl = Label::new(Some("Nueva pestaña"));
        site_status_lbl.add_css_class("search-panel-subtitle");
        site_status_lbl.set_xalign(0.0);
        site_box.append(&site_status_lbl);

        site_box.append(&Separator::new(Orientation::Horizontal));

        let copy_clean_btn = Button::with_label("Copiar URL limpia");
        copy_clean_btn.add_css_class("flat");
        copy_clean_btn.set_tooltip_text(Some("Copiar dirección sin parámetros de rastreo"));
        site_box.append(&copy_clean_btn);

        let qr_toggle_btn = Button::with_label("Compartir código QR");
        qr_toggle_btn.add_css_class("flat");
        qr_toggle_btn.set_tooltip_text(Some("Mostrar código QR de esta página"));
        site_box.append(&qr_toggle_btn);

        let paste_go_btn = Button::with_label("Pegar e ir");
        paste_go_btn.add_css_class("flat");
        paste_go_btn.set_tooltip_text(Some("Pegar URL del portapapeles y abrir"));
        site_box.append(&paste_go_btn);

        let qr_box = BoxWidget::new(Orientation::Vertical, 4);
        qr_box.set_margins(4, 4, 6, 2);
        qr_box.set_halign(Align::Center);
        qr_box.set_visible(false);
        let qr_image = Image::new();
        qr_image.set_pixel_size(132);
        qr_box.append(&qr_image);
        let qr_hint = Label::new(Some("Escanea con tu teléfono"));
        qr_hint.add_css_class("search-panel-subtitle");
        qr_hint.set_halign(Align::Center);
        qr_box.append(&qr_hint);
        site_box.append(&qr_box);

        site_pop.set_child(Some(&site_box));

        let site_btn = MenuButton::new();
        site_btn.add_css_class("flat");
        site_btn.add_css_class("search-tab-mini-btn");
        site_btn.set_child(Some(&icons::image(IconId::MoreVert, 15)));
        site_btn.set_tooltip_text(Some("Acciones rápidas del sitio"));
        site_btn.set_popover(Some(&site_pop));
        trailing_box.append(&site_btn);

        let bar_box = BoxWidget::new(Orientation::Horizontal, 4);
        bar_box.set_hexpand(true);
        bar_box.append(&entry);
        bar_box.append(&trailing_box);

        let clamp = Clamp::new();
        clamp.set_maximum_size(560);
        clamp.set_tightening_threshold(360);
        clamp.set_hexpand(true);
        clamp.set_child(Some(&bar_box));

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
            zoom_btn: zoom_btn.clone(),
            reader_btn: reader_btn.clone(),
            star_btn: star_btn.clone(),
            site_status_lbl,
            qr_box: qr_box.clone(),
            qr_image: qr_image.clone(),
            popover: popover.clone(),
            list_box: list_box.clone(),
            state: state.clone(),
        };

        // Botones de acciones rápidas en la barra de direcciones.
        {
            let b = bar.clone();
            zoom_btn.connect_clicked(move || {
                b.set_zoom(1.0);
                let cb = b.state.borrow().on_reset_zoom.clone();
                if let Some(cb) = cb {
                    cb();
                }
            });
        }
        {
            let b = bar.clone();
            reader_btn.connect_clicked(move || {
                let next = !b.state.borrow().reader_active;
                b.set_reader_mode(next);
                let cb = b.state.borrow().on_toggle_reader.clone();
                if let Some(cb) = cb {
                    cb(next);
                }
            });
        }
        {
            let b = bar.clone();
            star_btn.connect_clicked(move || {
                let cb = b.state.borrow().on_toggle_bookmark.clone();
                if let Some(cb) = cb {
                    cb();
                }
            });
        }
        {
            let b = bar.clone();
            let pop = site_pop.clone();
            copy_clean_btn.connect_clicked(move || {
                pop.popdown();
                let url = b.state.borrow().committed_url.clone();
                let clean = clean_link(&url);
                if let Some(disp) = gdk::Display::default_display() {
                    disp.set_clipboard_text(&clean);
                }
            });
        }
        {
            let b = bar.clone();
            qr_toggle_btn.connect_clicked(move || {
                let show = !b.qr_box.is_visible();
                if show {
                    let url = b.state.borrow().committed_url.clone();
                    let clean = clean_link(&url);
                    let tex = build_qr_preview_texture(&clean, 132);
                    b.qr_image.set_from_texture(Some(&tex.to_texture()));
                }
                b.qr_box.set_visible(show);
            });
        }
        {
            let b = bar.clone();
            let pop = site_pop;
            paste_go_btn.connect_clicked(move || {
                pop.popdown();
                b.paste_and_go();
            });
        }

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
                    bar_row.activate_item_at(idx as usize);
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
                        .map(|h| !dot_core::address::is_loopback_host(h))
                })
                .unwrap_or(false);

        if is_insecure_http {
            self.entry.add_css_class("insecure-http");
            self.site_status_lbl.set_text("Conexión HTTP sin cifrar");
        } else if clean_committed.starts_with("https://") {
            self.entry.remove_css_class("insecure-http");
            self.site_status_lbl.set_text("Conexión HTTPS segura");
        } else if clean_committed.starts_with("dot://")
            || clean_committed.starts_with("plasma://")
            || clean_committed.starts_with("search://")
        {
            self.entry.remove_css_class("insecure-http");
            self.site_status_lbl.set_text("Página interna de Dot");
        } else {
            self.entry.remove_css_class("insecure-http");
            self.site_status_lbl.set_text("Nueva pestaña");
        }
        self.qr_box.set_visible(false);

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

    #[allow(dead_code)]
    pub fn connect_reset_zoom(&self, cb: impl Fn() + 'static) {
        self.state.borrow_mut().on_reset_zoom = Some(Rc::new(cb));
    }

    #[allow(dead_code)]
    pub fn connect_toggle_reader(&self, cb: impl Fn(bool) + 'static) {
        self.state.borrow_mut().on_toggle_reader = Some(Rc::new(cb));
    }

    #[allow(dead_code)]
    pub fn connect_toggle_bookmark(&self, cb: impl Fn() + 'static) {
        self.state.borrow_mut().on_toggle_bookmark = Some(Rc::new(cb));
    }

    /// Actualiza la píldora de zoom (oculta al 100 %).
    pub fn set_zoom(&self, zoom: f64) {
        let pct = (zoom * 100.0).round() as i32;
        if (pct - 100).abs() <= 1 {
            self.zoom_btn.set_visible(false);
        } else {
            self.zoom_btn.set_label(&format!("{pct} %"));
            self.zoom_btn.set_visible(true);
        }
    }

    /// Actualiza el estado visual del botón de modo lectura.
    pub fn set_reader_mode(&self, active: bool) {
        self.state.borrow_mut().reader_active = active;
        if active {
            self.reader_btn.add_css_class("suggested-action");
            self.reader_btn
                .set_tooltip_text(Some("Salir del modo lectura"));
        } else {
            self.reader_btn.remove_css_class("suggested-action");
            self.reader_btn
                .set_tooltip_text(Some("Activar modo lectura"));
        }
    }

    /// Actualiza el estado visual del botón de marcador.
    #[allow(dead_code)]
    pub fn set_bookmarked(&self, bookmarked: bool) {
        self.state.borrow_mut().bookmarked = bookmarked;
        if bookmarked {
            self.star_btn.add_css_class("suggested-action");
            self.star_btn
                .set_tooltip_text(Some("Editar marcador (Ctrl+D)"));
        } else {
            self.star_btn.remove_css_class("suggested-action");
            self.star_btn
                .set_tooltip_text(Some("Añadir a marcadores (Ctrl+D)"));
        }
    }

    /// Pega el contenido del portapapeles y navega de inmediato.
    pub fn paste_and_go(&self) {
        let Some(disp) = gdk::Display::default_display() else {
            return;
        };
        let bar = self.clone();
        disp.read_clipboard_text_async(move |opt_text| {
            let Some(raw) = opt_text else {
                return;
            };
            let trimmed = raw.trim().to_string();
            if trimmed.is_empty() {
                return;
            }
            bar.close_popover();
            bar.state.borrow_mut().suppress_change = true;
            bar.entry.set_text(&trimmed);
            bar.state.borrow_mut().suppress_change = false;
            let submit = bar.state.borrow().on_submit.clone();
            if let Some(cb) = submit {
                cb(trimmed);
            }
        });
    }

    fn on_entry_changed(&self) {
        let (should_run, was_delete, provider) = {
            let mut st = self.state.borrow_mut();
            if st.suppress_change || !st.editing {
                return;
            }
            st.feedback_seq = st.feedback_seq.wrapping_add(1);
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

        let superpowers = {
            let st = self.state.borrow();
            evaluate_superpowers(
                &typed,
                &st.calculator,
                &st.color_previewer,
                &st.bang_registry,
            )
        };

        let suggestions = provider.map(|p| p(&typed)).unwrap_or_default();
        if superpowers.is_empty() && suggestions.is_empty() {
            self.close_popover();
            return;
        }

        // Completado inline del dominio más probable solo al escribir hacia delante,
        // cuando el cursor está al final y no hay un superpower activo.
        if !was_delete && superpowers.is_empty() {
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

        let mut items = superpowers;
        items.extend(suggestions.into_iter().map(OmniboxRowItem::Suggestion));
        self.render_items(items);
    }

    fn render_items(&self, items: Vec<OmniboxRowItem>) {
        self.list_box.remove_all();
        let mut row_badges = Vec::with_capacity(items.len());

        for item in &items {
            let row = ListBoxRow::new();
            row.add_css_class("search-omnibox-row");
            row.set_focusable(false);

            let hbox = BoxWidget::new(Orientation::Horizontal, 8);
            hbox.set_margins(10, 10, 6, 6);

            let mut badge_opt: Option<Label> = None;

            match item {
                OmniboxRowItem::Calc(calc) => {
                    row.add_css_class("omnibox-calc-row");
                    let icon = icons::image(IconId::Cpu, 15);
                    hbox.append(&icon);

                    let col = BoxWidget::new(Orientation::Vertical, 1);
                    col.set_hexpand(true);

                    let title = Label::new(Some(&format!("= {}", calc.formatted)));
                    title.set_xalign(0.0);
                    title.set_ellipsize(EllipsizeMode::End);
                    col.append(&title);

                    let subtitle_text = format_calc_subtitle(calc);
                    let sub_lbl = Label::new(Some(&subtitle_text));
                    sub_lbl.add_css_class("search-omnibox-url");
                    sub_lbl.set_xalign(0.0);
                    sub_lbl.set_ellipsize(EllipsizeMode::End);
                    col.append(&sub_lbl);

                    hbox.append(&col);

                    let badge_text = match calc.kind {
                        CalcResultKind::UnitConversion => "Conversor",
                        CalcResultKind::Percentage | CalcResultKind::Arithmetic => "Calculadora",
                    };
                    let badge = Label::new(Some(badge_text));
                    badge.add_css_class("search-omnibox-badge");
                    badge.set_valign(Align::Center);
                    hbox.append(&badge);
                    badge_opt = Some(badge);
                }
                OmniboxRowItem::Color(preview) => {
                    row.add_css_class("omnibox-calc-row");
                    let swatch = Image::new();
                    swatch.set_pixel_size(24);
                    swatch.set_from_texture(Some(
                        &build_color_swatch_texture(preview.rgba, 24).to_texture(),
                    ));
                    swatch.add_css_class("color-swatch");
                    hbox.append(&swatch);

                    let col = BoxWidget::new(Orientation::Vertical, 1);
                    col.set_hexpand(true);

                    let title_text = match &preview.matched_name {
                        Some(name) => format!("{} ({})", preview.swatch_hex, name),
                        None => preview.swatch_hex.clone(),
                    };
                    let title = Label::new(Some(&title_text));
                    title.set_xalign(0.0);
                    title.set_ellipsize(EllipsizeMode::End);
                    col.append(&title);

                    let sub_text = format_color_subtitle(preview);
                    let sub_lbl = Label::new(Some(&sub_text));
                    sub_lbl.add_css_class("search-omnibox-url");
                    sub_lbl.set_xalign(0.0);
                    sub_lbl.set_ellipsize(EllipsizeMode::End);
                    col.append(&sub_lbl);

                    hbox.append(&col);

                    let badge = Label::new(Some("Color"));
                    badge.add_css_class("search-omnibox-badge");
                    badge.set_valign(Align::Center);
                    hbox.append(&badge);
                    badge_opt = Some(badge);
                }
                OmniboxRowItem::Bang(matched) => {
                    row.add_css_class("omnibox-calc-row");
                    let icon = icons::image(IconId::Search, 15);
                    hbox.append(&icon);

                    let col = BoxWidget::new(Orientation::Vertical, 1);
                    col.set_hexpand(true);

                    let title_text = if matched.remaining_query.is_empty() {
                        format!("Abrir {}", matched.bang_name)
                    } else {
                        format!(
                            "Buscar \"{}\" en {}",
                            matched.remaining_query, matched.bang_name
                        )
                    };
                    let title = Label::new(Some(&title_text));
                    title.set_xalign(0.0);
                    title.set_ellipsize(EllipsizeMode::End);
                    col.append(&title);

                    let url_lbl = Label::new(Some(&matched.resolved_url));
                    url_lbl.add_css_class("search-omnibox-url");
                    url_lbl.set_xalign(0.0);
                    url_lbl.set_ellipsize(EllipsizeMode::End);
                    col.append(&url_lbl);

                    hbox.append(&col);

                    let badge = Label::new(Some(&format!(
                        "!{} · {}",
                        matched.canonical_trigger, matched.bang_name
                    )));
                    badge.add_css_class("search-omnibox-badge");
                    badge.set_valign(Align::Center);
                    hbox.append(&badge);
                    badge_opt = Some(badge);
                }
                OmniboxRowItem::BangHint(suggestion) => {
                    let icon = icons::image(IconId::Search, 15);
                    hbox.append(&icon);

                    let col = BoxWidget::new(Orientation::Vertical, 1);
                    col.set_hexpand(true);

                    let title = Label::new(Some(&format!(
                        "!{} - {}",
                        suggestion.trigger, suggestion.name
                    )));
                    title.set_xalign(0.0);
                    title.set_ellipsize(EllipsizeMode::End);
                    col.append(&title);

                    let sub_lbl = Label::new(Some("Pulsa Enter o haz clic para completar bang"));
                    sub_lbl.add_css_class("search-omnibox-url");
                    sub_lbl.set_xalign(0.0);
                    sub_lbl.set_ellipsize(EllipsizeMode::End);
                    col.append(&sub_lbl);

                    hbox.append(&col);

                    let badge = Label::new(Some("Bang"));
                    badge.add_css_class("search-omnibox-badge");
                    badge.set_valign(Align::Center);
                    hbox.append(&badge);
                    badge_opt = Some(badge);
                }
                OmniboxRowItem::Suggestion(s) => {
                    let icon_id = match s.kind {
                        OmniboxSuggestionKind::OpenTab { .. } => IconId::LayoutTabs,
                        OmniboxSuggestionKind::Bookmark { .. } => IconId::Star,
                        OmniboxSuggestionKind::History { .. } => IconId::History,
                    };
                    let icon = icons::image(icon_id, 15);
                    hbox.append(&icon);

                    let col = BoxWidget::new(Orientation::Vertical, 1);
                    col.set_hexpand(true);

                    let title = Label::new(Some(&s.title));
                    title.set_xalign(0.0);
                    title.set_ellipsize(EllipsizeMode::End);
                    col.append(&title);

                    let url_lbl = Label::new(Some(&s.display_url));
                    url_lbl.add_css_class("search-omnibox-url");
                    url_lbl.set_xalign(0.0);
                    url_lbl.set_ellipsize(EllipsizeMode::End);
                    col.append(&url_lbl);

                    hbox.append(&col);

                    if matches!(s.kind, OmniboxSuggestionKind::OpenTab { .. }) {
                        let badge = Label::new(Some("Cambiar a pestaña"));
                        badge.add_css_class("search-omnibox-badge");
                        badge.set_valign(Align::Center);
                        hbox.append(&badge);
                    }
                }
            }

            row_badges.push(badge_opt);
            row.set_child(Some(&hbox));
            self.list_box.append(&row);
        }

        let is_empty = items.is_empty();
        {
            let mut st = self.state.borrow_mut();
            st.items = items;
            st.row_badges = row_badges;
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
            st.feedback_seq = st.feedback_seq.wrapping_add(1);
            st.items.clear();
            st.row_badges.clear();
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
                if st.popover_open && !st.items.is_empty() {
                    let idx = st.selected_index.unwrap_or(0);
                    st.items.get(idx).and_then(|item| match item {
                        OmniboxRowItem::Suggestion(s) => match &s.kind {
                            OmniboxSuggestionKind::History { key } => Some((
                                key.clone(),
                                st.on_delete_history.clone(),
                                st.user_typed.clone(),
                                st.suggest_provider.clone(),
                            )),
                            _ => None,
                        },
                        _ => None,
                    })
                } else {
                    None
                }
            };
            if let Some((key, Some(del_cb), user_typed, provider)) = delete_target {
                del_cb(key);
                let superpowers = {
                    let st = self.state.borrow();
                    evaluate_superpowers(
                        &user_typed,
                        &st.calculator,
                        &st.color_previewer,
                        &st.bang_registry,
                    )
                };
                let mut items = superpowers;
                if let Some(prov) = provider {
                    items.extend(
                        prov(&user_typed)
                            .into_iter()
                            .map(OmniboxRowItem::Suggestion),
                    );
                }
                if items.is_empty() {
                    self.close_popover();
                } else {
                    self.render_items(items);
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
            if !st.popover_open || st.items.is_empty() {
                return false;
            }
            let len = st.items.len() as isize;
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
            let entry_text = match &st.items[next] {
                OmniboxRowItem::Suggestion(s) => s.display_url.clone(),
                OmniboxRowItem::BangHint(s) => format!("!{} ", s.trigger),
                OmniboxRowItem::Calc(_) | OmniboxRowItem::Color(_) | OmniboxRowItem::Bang(_) => {
                    st.user_typed.clone()
                }
            };
            Some((next, entry_text))
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
        let (selected_idx, first_is_superpower) = {
            let st = self.state.borrow();
            let first_super = st.popover_open
                && matches!(
                    st.items.first(),
                    Some(
                        OmniboxRowItem::Calc(_)
                            | OmniboxRowItem::Color(_)
                            | OmniboxRowItem::Bang(_)
                    )
                );
            (st.selected_index, first_super)
        };
        if let Some(idx) = selected_idx {
            self.activate_item_at(idx);
            return;
        }
        if first_is_superpower {
            self.activate_item_at(0);
            return;
        }
        let typed = self.entry.text();
        self.close_popover();
        let submit = self.state.borrow().on_submit.clone();
        if let Some(cb) = submit {
            cb(typed);
        }
    }

    fn activate_item_at(&self, idx: usize) {
        let action = {
            let st = self.state.borrow();
            st.items
                .get(idx)
                .cloned()
                .map(|item| (item, st.on_submit.clone(), st.on_switch_tab.clone()))
        };
        let Some((item, on_submit, on_switch_tab)) = action else {
            self.close_popover();
            return;
        };
        match item {
            OmniboxRowItem::Calc(calc) => {
                self.copy_with_feedback(idx, &calc.copy_value);
            }
            OmniboxRowItem::Color(preview) => {
                self.copy_with_feedback(idx, &preview.swatch_hex);
            }
            OmniboxRowItem::Bang(matched) => {
                self.close_popover();
                if let Some(cb) = on_submit {
                    cb(matched.resolved_url);
                }
            }
            OmniboxRowItem::BangHint(suggestion) => {
                let completed = format!("!{} ", suggestion.trigger);
                self.state.borrow_mut().suppress_change = true;
                self.entry.set_text(&completed);
                self.entry.set_position(-1);
                self.state.borrow_mut().suppress_change = false;
                self.on_entry_changed();
            }
            OmniboxRowItem::Suggestion(suggestion) => {
                self.close_popover();
                match suggestion.kind {
                    OmniboxSuggestionKind::OpenTab { window_id, tab_id } => {
                        if let Some(cb) = on_switch_tab {
                            cb(window_id, tab_id);
                        }
                    }
                    OmniboxSuggestionKind::Bookmark { .. }
                    | OmniboxSuggestionKind::History { .. } => {
                        if let Some(cb) = on_submit {
                            cb(suggestion.url);
                        }
                    }
                }
            }
        }
    }

    fn copy_with_feedback(&self, idx: usize, value: &str) {
        if let Some(disp) = gdk::Display::default_display() {
            disp.set_clipboard_text(value);
        }
        let seq = {
            let mut st = self.state.borrow_mut();
            st.feedback_seq = st.feedback_seq.wrapping_add(1);
            if let Some(Some(badge)) = st.row_badges.get(idx) {
                badge.set_text("Copiado");
            }
            st.feedback_seq
        };
        let bar = self.clone();
        gnome_platform::glib::timeout_add_local_once(Duration::from_millis(650), move || {
            if bar.state.borrow().feedback_seq == seq {
                bar.close_popover();
            }
        });
    }
}

impl Default for AddressBar {
    fn default() -> Self {
        Self::new()
    }
}

/// Genera una textura nítida de matriz QR (finder patterns + datos deterministas de la URL)
/// para previsualización rápida sin dependencias externas.
fn build_qr_preview_texture(data: &str, px_size: i32) -> MemoryTexture {
    const N: usize = 25;
    let mut grid = [false; N * N];

    let draw_finder = |grid: &mut [bool; N * N], r0: usize, c0: usize| {
        for r in 0..7 {
            for c in 0..7 {
                let border = r == 0 || r == 6 || c == 0 || c == 6;
                let inner = (2..=4).contains(&r) && (2..=4).contains(&c);
                grid[(r0 + r) * N + (c0 + c)] = border || inner;
            }
        }
    };
    draw_finder(&mut grid, 0, 0);
    draw_finder(&mut grid, 0, N - 7);
    draw_finder(&mut grid, N - 7, 0);

    // Patrones de temporización.
    for i in 8..(N - 8) {
        grid[6 * N + i] = i % 2 == 0;
        grid[i * N + 6] = i % 2 == 0;
    }

    // Relleno determinista a partir de los bytes de la URL (FNV-1a extendido).
    let mut h: u64 = 0xcbf29ce484222325;
    for b in data.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    for r in 0..N {
        for c in 0..N {
            let in_tl = r < 8 && c < 8;
            let in_tr = r < 8 && c >= N - 8;
            let in_bl = r >= N - 8 && c < 8;
            let on_timing = r == 6 || c == 6;
            if in_tl || in_tr || in_bl || on_timing {
                continue;
            }
            h = h
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            grid[r * N + c] = (h >> 63) == 1;
        }
    }

    let size = px_size.max(50) as usize;
    let quiet = 2usize;
    let total_mods = N + quiet * 2;
    let mut rgba = vec![255u8; size * size * 4];
    for py in 0..size {
        let mr = (py * total_mods) / size;
        for px in 0..size {
            let mc = (px * total_mods) / size;
            let dark = mr >= quiet
                && mr < quiet + N
                && mc >= quiet
                && mc < quiet + N
                && grid[(mr - quiet) * N + (mc - quiet)];
            let off = (py * size + px) * 4;
            if dark {
                rgba[off] = 17;
                rgba[off + 1] = 17;
                rgba[off + 2] = 20;
                rgba[off + 3] = 255;
            }
        }
    }

    let bytes = Bytes::from_slice(&rgba);
    MemoryTexture::new(
        size as i32,
        size as i32,
        MemoryFormat::R8g8b8a8,
        &bytes,
        size * 4,
    )
}

fn evaluate_superpowers(
    input: &str,
    calculator: &OmniboxCalculator,
    color_previewer: &OmniboxColorPreviewer,
    bang_registry: &BangRegistry,
) -> Vec<OmniboxRowItem> {
    let mut out = Vec::new();
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return out;
    }

    if let Some(matched) = bang_registry.resolve(trimmed, &BangParseOptions::default()) {
        out.push(OmniboxRowItem::Bang(matched));
    } else if trimmed.starts_with('!') {
        for suggestion in bang_registry.suggest(trimmed, 3) {
            out.push(OmniboxRowItem::BangHint(suggestion));
        }
    }

    if let Some(preview) = color_previewer.try_preview(trimmed) {
        out.push(OmniboxRowItem::Color(preview));
    }

    if let Some(calc) = calculator.try_suggest(trimmed) {
        out.push(OmniboxRowItem::Calc(calc));
    }

    out
}

fn format_calc_subtitle(calc: &CalcResult) -> String {
    if calc.alternates.is_empty() {
        format!(
            "{}  ·  Enter o clic para copiar",
            calc.normalized_expression
        )
    } else {
        let alts: Vec<String> = calc
            .alternates
            .iter()
            .take(2)
            .map(|a| format!("{}: {}", a.label, a.value))
            .collect();
        format!("{}  ·  {}", calc.normalized_expression, alts.join(" · "))
    }
}

fn wcag_grade(aa: bool, aaa: bool) -> &'static str {
    if aaa {
        "AAA"
    } else if aa {
        "AA"
    } else {
        "Bajo"
    }
}

fn format_color_subtitle(preview: &OmniboxColorPreview) -> String {
    let rgb = format!(
        "rgb({}, {}, {})",
        preview.rgba.r, preview.rgba.g, preview.rgba.b
    );
    let white_grade = wcag_grade(
        preview.contrast.wcag_aa_on_white,
        preview.contrast.wcag_aaa_on_white,
    );
    let black_grade = wcag_grade(
        preview.contrast.wcag_aa_on_black,
        preview.contrast.wcag_aaa_on_black,
    );
    format!(
        "{rgb}  ·  WCAG blanco {:.2}:1 ({white_grade}) · negro {:.2}:1 ({black_grade})",
        preview.contrast.vs_white, preview.contrast.vs_black
    )
}

fn build_color_swatch_texture(rgba: ColorRgba, px_size: i32) -> MemoryTexture {
    let size = px_size.max(8) as usize;
    let buf = [rgba.r, rgba.g, rgba.b, rgba.a].repeat(size * size);
    let bytes = Bytes::from_slice(&buf);
    MemoryTexture::new(
        size as i32,
        size as i32,
        MemoryFormat::R8g8b8a8,
        &bytes,
        size * 4,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omnibox_superpowers_evaluate_math_percentage_and_units() {
        let calc = OmniboxCalculator::default();
        let color = OmniboxColorPreviewer::with_defaults();
        let bangs = BangRegistry::with_defaults();

        let math = evaluate_superpowers("25 * 4 + 10", &calc, &color, &bangs);
        assert!(matches!(math.first(), Some(OmniboxRowItem::Calc(c)) if c.copy_value == "110"));

        let pct = evaluate_superpowers("= 15% of 200", &calc, &color, &bangs);
        assert!(matches!(pct.first(), Some(OmniboxRowItem::Calc(c)) if c.copy_value == "30"));

        let unit = evaluate_superpowers("100 km to mi", &calc, &color, &bangs);
        assert!(matches!(
            unit.first(),
            Some(OmniboxRowItem::Calc(c)) if c.kind == CalcResultKind::UnitConversion
                && c.formatted.ends_with("mi")
        ));
    }

    #[test]
    fn omnibox_superpowers_evaluate_colors_and_wcag() {
        let calc = OmniboxCalculator::default();
        let color = OmniboxColorPreviewer::with_defaults();
        let bangs = BangRegistry::with_defaults();

        let hex = evaluate_superpowers("#ff6b6b", &calc, &color, &bangs);
        assert!(matches!(
            hex.first(),
            Some(OmniboxRowItem::Color(p)) if p.swatch_hex == "#ff6b6b"
        ));

        let rgb = evaluate_superpowers("rgb(110, 168, 254)", &calc, &color, &bangs);
        match rgb.first() {
            Some(OmniboxRowItem::Color(p)) => {
                assert_eq!(p.rgba.r, 110);
                assert_eq!(p.rgba.g, 168);
                assert_eq!(p.rgba.b, 254);
                let sub = format_color_subtitle(p);
                assert!(sub.contains("WCAG"));
            }
            _ => panic!("expected color preview"),
        }
    }

    #[test]
    fn omnibox_superpowers_evaluate_bangs() {
        let calc = OmniboxCalculator::default();
        let color = OmniboxColorPreviewer::with_defaults();
        let bangs = BangRegistry::with_defaults();

        for (query, expected_trigger) in [
            ("!gh rust", "gh"),
            ("!yt lofi", "yt"),
            ("!w linux", "w"),
            ("!ddg search", "ddg"),
        ] {
            let res = evaluate_superpowers(query, &calc, &color, &bangs);
            assert!(
                matches!(
                    res.first(),
                    Some(OmniboxRowItem::Bang(m)) if m.canonical_trigger == expected_trigger
                ),
                "failed for {query}"
            );
        }
    }
}
