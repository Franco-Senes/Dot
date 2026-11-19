//! Interfaz de Arc Boosts por sitio (`AdwDialog`) con vista previa en vivo.
#![allow(dead_code)]

use crate::icons::{self, IconId};
use gnome_platform::adw::{
    ActionRow, ApplicationWindow, ComboRow, Dialog, EntryRow, HeaderBar, PreferencesGroup,
    SwitchRow, ToolbarView,
};
use gnome_platform::gtk::{
    Align, AsEditable, AsWidget, BoxWidget, Button, Label, Orientation, PolicyType, Scale,
    ScrolledWindow, StringList,
};
use search_core::boosts::{
    self, BoostFont, BoostStore, ContrastPreset, SiteBoost, DEFAULT_FONT_SCALE_PCT,
    MAX_FONT_SCALE_PCT, MIN_FONT_SCALE_PCT,
};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

const FONT_OPTIONS: [Option<BoostFont>; 4] = [
    None,
    Some(BoostFont::Sans),
    Some(BoostFont::Serif),
    Some(BoostFont::Mono),
];
const FONT_LABELS: [&str; 4] = ["Predeterminada", "Sans-serif", "Serif", "Monoespaciada"];

const CONTRAST_OPTIONS: [ContrastPreset; 4] = [
    ContrastPreset::Normal,
    ContrastPreset::HighContrast,
    ContrastPreset::Sepia,
    ContrastPreset::InvertDark,
];
const CONTRAST_LABELS: [&str; 4] = ["Normal", "Alto contraste", "Sepia", "Oscuro invertido"];

struct BoostCtx {
    origin: String,
    store: Rc<RefCell<BoostStore>>,
    store_path: PathBuf,
    current: RefCell<SiteBoost>,
    updating: Cell<bool>,
    on_apply: Rc<dyn Fn(String, String)>,
}

impl BoostCtx {
    fn persist_and_apply(&self) {
        let boost = self.current.borrow().clone();
        {
            let mut store = self.store.borrow_mut();
            if let Err(err) = store.upsert(boost.clone()) {
                search_core::warn!("No se pudo actualizar Boost para '{}': {err}", self.origin);
                return;
            }
            if let Err(err) = store.save(&self.store_path) {
                search_core::warn!("No se pudo guardar '{}' : {err}", self.store_path.display());
            }
        }
        let css = boosts::compile_boost_css(&boost);
        (self.on_apply)(self.origin.clone(), css);
    }
}

/// Abre el diálogo de **Arc Boosts** para `origin_or_url` con aplicación en vivo.
pub fn open_boost_dialog(
    parent: &ApplicationWindow,
    origin_or_url: &str,
    store: Rc<RefCell<BoostStore>>,
    store_path: PathBuf,
    on_apply: Rc<dyn Fn(String, String)>,
) {
    let origin =
        boosts::normalize_origin(origin_or_url).unwrap_or_else(|| origin_or_url.trim().to_string());
    let initial = store
        .borrow()
        .get_or_default(&origin)
        .unwrap_or_else(|| SiteBoost::new(origin.clone()));

    let ctx = Rc::new(BoostCtx {
        origin: origin.clone(),
        store,
        store_path,
        current: RefCell::new(initial.clone()),
        updating: Cell::new(false),
        on_apply,
    });

    let dialog = Dialog::new();
    dialog.set_title("Boost del sitio");
    dialog.set_content_width(440);
    dialog.set_content_height(520);

    let header = HeaderBar::new();
    let title_box = BoxWidget::new(Orientation::Horizontal, 8);
    title_box.set_halign(Align::Center);
    let title_icon = icons::image(IconId::Palette, 16);
    let title_lbl = Label::new(Some("Boost del sitio"));
    title_lbl.add_css_class("heading");
    title_box.append(&title_icon);
    title_box.append(&title_lbl);
    header.set_title_widget(Some(&title_box));

    let origin_lbl = Label::new(Some(&origin));
    origin_lbl.add_css_class("dim-label");
    origin_lbl.add_css_class("caption");
    origin_lbl.set_xalign(0.0);

    // Grupo 1: Activación
    let general_group = PreferencesGroup::new();
    general_group.set_title("Estado");

    let enable_row = SwitchRow::new();
    enable_row.set_title("Activar Boost");
    enable_row.set_subtitle("Aplica estilos visuales en tiempo real para este sitio");
    enable_row.set_active(initial.enabled);
    enable_row.add_prefix(&icons::image(IconId::Sliders, 16));
    general_group.add(&enable_row);

    // Grupo 2: Tipografía y tamaño
    let typography_group = PreferencesGroup::new();
    typography_group.set_title("Tipografía y lectura");

    let font_row = ComboRow::new();
    font_row.set_title("Cambiar tipografía");
    font_row.set_subtitle("Sustituye la familia de fuente de la página");
    let font_model = StringList::new(&FONT_LABELS);
    font_row.set_string_model(&font_model);
    let font_idx = FONT_OPTIONS
        .iter()
        .position(|&f| f == initial.font_override)
        .unwrap_or(0) as u32;
    font_row.set_selected(font_idx);
    typography_group.add(&font_row);

    let size_row = ActionRow::new();
    size_row.set_title("Ajustar tamaño de texto");
    size_row.set_subtitle("Escala porcentual (80% – 150%)");
    let size_value_lbl = Label::new(Some(&format!("{}%", initial.font_scale_pct)));
    size_value_lbl.add_css_class("numeric");
    size_value_lbl.add_css_class("dim-label");
    let size_scale = Scale::with_range(
        Orientation::Horizontal,
        f64::from(MIN_FONT_SCALE_PCT),
        f64::from(MAX_FONT_SCALE_PCT),
        5.0,
    );
    size_scale.set_digits(0);
    size_scale.set_draw_value(false);
    size_scale.set_size_request(140, -1);
    size_scale.set_valign(Align::Center);
    size_scale.set_value(f64::from(initial.font_scale_pct));
    let size_suffix = BoxWidget::new(Orientation::Horizontal, 8);
    size_suffix.set_valign(Align::Center);
    size_suffix.append(&size_scale);
    size_suffix.append(&size_value_lbl);
    size_row.add_suffix(&size_suffix);
    typography_group.add(&size_row);

    // Grupo 3: Color e imágenes
    let visual_group = PreferencesGroup::new();
    visual_group.set_title("Color y elementos");

    let contrast_row = ComboRow::new();
    contrast_row.set_title("Elegir contraste");
    contrast_row.set_subtitle("Filtro de color y legibilidad");
    let contrast_model = StringList::new(&CONTRAST_LABELS);
    contrast_row.set_string_model(&contrast_model);
    let contrast_idx = CONTRAST_OPTIONS
        .iter()
        .position(|&c| c == initial.contrast_preset)
        .unwrap_or(0) as u32;
    contrast_row.set_selected(contrast_idx);
    visual_group.add(&contrast_row);

    let hide_images_row = SwitchRow::new();
    hide_images_row.set_title("Ocultar imágenes");
    hide_images_row.set_subtitle("Esconde imágenes, vídeos y gráficos decorativos");
    hide_images_row.set_active(initial.hide_images);
    hide_images_row.add_prefix(&icons::image(IconId::EyeOff, 16));
    visual_group.add(&hide_images_row);

    // Grupo 4: CSS personalizado
    let css_group = PreferencesGroup::new();
    css_group.set_title("CSS personalizado");

    let css_entry = EntryRow::new();
    css_entry.set_title("Añadir CSS personalizado");
    css_entry.set_text(&initial.custom_css);
    css_entry.set_show_apply_button(true);
    css_group.add(&css_entry);

    // Pie con botón Restablecer Boost
    let reset_btn = Button::new();
    let reset_box = BoxWidget::new(Orientation::Horizontal, 6);
    reset_box.set_halign(Align::Center);
    reset_box.append(&icons::image(IconId::RotateCcw, 16));
    reset_box.append(&Label::new(Some("Restablecer Boost")));
    reset_btn.set_child(Some(&reset_box));
    reset_btn.add_css_class("destructive-action");
    reset_btn.set_halign(Align::End);

    // Conexión de señales (BoostCtx no almacena ningún GObject)
    {
        let ctx_ref = Rc::clone(&ctx);
        let row_ref = enable_row.clone();
        enable_row.connect_active_notify(move || {
            if ctx_ref.updating.get() {
                return;
            }
            ctx_ref.current.borrow_mut().enabled = row_ref.is_active();
            ctx_ref.persist_and_apply();
        });
    }

    {
        let ctx_ref = Rc::clone(&ctx);
        let row_ref = font_row.clone();
        font_row.connect_selected_notify(move || {
            if ctx_ref.updating.get() {
                return;
            }
            let idx = row_ref.selected() as usize;
            let font = FONT_OPTIONS.get(idx).copied().flatten();
            ctx_ref.current.borrow_mut().font_override = font;
            ctx_ref.persist_and_apply();
        });
    }

    {
        let ctx_ref = Rc::clone(&ctx);
        let scale_ref = size_scale.clone();
        let lbl_ref = size_value_lbl.clone();
        size_scale.connect_value_changed(move || {
            if ctx_ref.updating.get() {
                return;
            }
            let pct =
                (scale_ref.value().round() as u16).clamp(MIN_FONT_SCALE_PCT, MAX_FONT_SCALE_PCT);
            lbl_ref.set_text(&format!("{pct}%"));
            ctx_ref.current.borrow_mut().font_scale_pct = pct;
            ctx_ref.persist_and_apply();
        });
    }

    {
        let ctx_ref = Rc::clone(&ctx);
        let row_ref = contrast_row.clone();
        contrast_row.connect_selected_notify(move || {
            if ctx_ref.updating.get() {
                return;
            }
            let idx = row_ref.selected() as usize;
            let preset = CONTRAST_OPTIONS
                .get(idx)
                .copied()
                .unwrap_or(ContrastPreset::Normal);
            ctx_ref.current.borrow_mut().contrast_preset = preset;
            ctx_ref.persist_and_apply();
        });
    }

    {
        let ctx_ref = Rc::clone(&ctx);
        let row_ref = hide_images_row.clone();
        hide_images_row.connect_active_notify(move || {
            if ctx_ref.updating.get() {
                return;
            }
            ctx_ref.current.borrow_mut().hide_images = row_ref.is_active();
            ctx_ref.persist_and_apply();
        });
    }

    {
        let ctx_ref = Rc::clone(&ctx);
        let entry_ref = css_entry.clone();
        css_entry.connect_apply(move || {
            if ctx_ref.updating.get() {
                return;
            }
            let sanitized = boosts::sanitize_custom_css(&entry_ref.text());
            ctx_ref.current.borrow_mut().custom_css = sanitized;
            ctx_ref.persist_and_apply();
        });
    }

    {
        let ctx_ref = Rc::clone(&ctx);
        let enable_ref = enable_row.clone();
        let font_ref = font_row.clone();
        let scale_ref = size_scale.clone();
        let scale_lbl_ref = size_value_lbl.clone();
        let contrast_ref = contrast_row.clone();
        let hide_ref = hide_images_row.clone();
        let css_ref = css_entry.clone();
        reset_btn.connect_clicked(move || {
            ctx_ref.updating.set(true);
            *ctx_ref.current.borrow_mut() = SiteBoost::new(ctx_ref.origin.clone());

            enable_ref.set_active(false);
            font_ref.set_selected(0);
            scale_ref.set_value(f64::from(DEFAULT_FONT_SCALE_PCT));
            scale_lbl_ref.set_text(&format!("{DEFAULT_FONT_SCALE_PCT}%"));
            contrast_ref.set_selected(0);
            hide_ref.set_active(false);
            css_ref.set_text("");

            ctx_ref.updating.set(false);
            ctx_ref.persist_and_apply();
        });
    }

    let body = BoxWidget::new(Orientation::Vertical, 14);
    body.set_margins(16, 16, 12, 16);
    body.append(&origin_lbl);
    body.append(&general_group);
    body.append(&typography_group);
    body.append(&visual_group);
    body.append(&css_group);
    body.append(&reset_btn);

    let scroller = ScrolledWindow::new();
    scroller.set_policy(PolicyType::Never, PolicyType::Automatic);
    scroller.set_vexpand(true);
    scroller.set_child(Some(&body));

    let toolbar = ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&scroller));

    dialog.set_child(Some(&toolbar));
    dialog.present(Some(parent));
}
