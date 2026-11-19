//! Integración del modo lectura (`Reader.swift`) sobre `third_party/readability/readability.js`,
//! saneamiento y plantilla HTML/CSS en `search_core::reader`, y píldora flotante discreta
//! para controlar tema (Papel / Sepia / Carbón / AMOLED), tipografía, medida, interlineado
//! y tamaño de fuente (`reader.fontSize`).

use crate::icons::{self, IconId};
use crate::settings::SettingsStore;
use gnome_platform::gtk::{
    Align, AsWidget, BoxWidget, Button, Label, MenuButton, Orientation, Popover, PositionType,
};
use search_core::reader::{
    ReaderFont, ReaderLineHeight, ReaderMeasure, ReaderOptions, ReaderTheme,
};
use search_core::settings::ColorLook;
use search_core::shield::js_string_literal;
use std::cell::Cell;
use std::rc::Rc;

/// Extractor de artículos (`third_party/readability/readability.js`, licencia MIT del proyecto) incluido en el binario.
const READABILITY_EXTRACT_JS: &str =
    include_str!("../../../third_party/readability/readability.js");

/// Construye el script inyectable para extraer el artículo de la página actual y enviarlo a Rust.
pub fn build_reader_extract_script(ipc_nonce: &str) -> String {
    let prefix = js_string_literal(&format!("__search_ipc_{ipc_nonce}:"));
    READABILITY_EXTRACT_JS.replace("__SEARCH_IPC_PREFIX__", &prefix)
}

type OptionsChangedCb = Rc<dyn Fn(ReaderOptions)>;
type ExitCb = Rc<dyn Fn()>;

/// Píldora flotante discreta en la esquina superior derecha visible únicamente en modo lectura:
/// tamaño de fuente A−/A+, menú de apariencia (tema, tipografía, medida e interlineado) y salir.
#[derive(Clone)]
pub struct ReaderToolbar {
    root: BoxWidget,
    size_label: Label,
    theme_buttons: [(ReaderTheme, Button); 4],
    font_buttons: [(ReaderFont, Button); 3],
    measure_buttons: [(ReaderMeasure, Button); 3],
    line_height_buttons: [(ReaderLineHeight, Button); 3],
    look: Rc<Cell<ColorLook>>,
    theme: Rc<Cell<Option<ReaderTheme>>>,
    font: Rc<Cell<ReaderFont>>,
    measure: Rc<Cell<ReaderMeasure>>,
    line_height: Rc<Cell<ReaderLineHeight>>,
    font_size: Rc<Cell<i32>>,
    settings: SettingsStore,
    on_options_changed: OptionsChangedCb,
}

impl ReaderToolbar {
    pub fn new(
        settings: SettingsStore,
        on_options_changed: impl Fn(ReaderOptions) + 'static,
        on_exit: impl Fn() + 'static,
    ) -> Self {
        let snap = settings.snapshot();
        let look = Rc::new(Cell::new(snap.look));
        let initial_theme = match snap.look {
            ColorLook::Light => Some(ReaderTheme::Paper),
            ColorLook::Dark => Some(ReaderTheme::Charcoal),
            ColorLook::System => None,
        };
        let theme = Rc::new(Cell::new(initial_theme));
        let font = Rc::new(Cell::new(ReaderFont::Serif));
        let measure = Rc::new(Cell::new(ReaderMeasure::Medium));
        let line_height = Rc::new(Cell::new(ReaderLineHeight::Normal));
        let font_size = Rc::new(Cell::new(snap.reader_font_size.clamp(12, 36)));
        let on_options_changed: OptionsChangedCb = Rc::new(on_options_changed);
        let on_exit: ExitCb = Rc::new(on_exit);

        let root = BoxWidget::new(Orientation::Horizontal, 4);
        root.add_css_class("search-reader-pill");
        root.set_halign(Align::End);
        root.set_valign(Align::Start);
        root.set_margins(12, 0, 0, 14);
        root.set_visible(false);

        let btn_smaller = Button::with_label("A−");
        btn_smaller.add_css_class("flat");
        btn_smaller.add_css_class("search-reader-btn");
        btn_smaller.set_tooltip_text(Some("Reducir tamaño de letra"));
        root.append(&btn_smaller);

        let size_label = Label::new(Some(&format!("{}px", font_size.get())));
        size_label.add_css_class("search-reader-size");
        root.append(&size_label);

        let btn_larger = Button::with_label("A+");
        btn_larger.add_css_class("flat");
        btn_larger.add_css_class("search-reader-btn");
        btn_larger.set_tooltip_text(Some("Aumentar tamaño de letra"));
        root.append(&btn_larger);

        // Popover de apariencia y tipografía.
        let appearance_pop = Popover::new();
        appearance_pop.set_position(PositionType::Bottom);
        let pop_box = BoxWidget::new(Orientation::Vertical, 10);
        pop_box.set_margins(10, 10, 12, 12);

        let make_section = |title: &str| -> BoxWidget {
            let sec = BoxWidget::new(Orientation::Vertical, 4);
            let lbl = Label::new(Some(title));
            lbl.add_css_class("search-panel-section");
            lbl.set_xalign(0.0);
            sec.append(&lbl);
            sec
        };

        let make_choice_btn = |label: &str, tooltip: &str| -> Button {
            let btn = Button::with_label(label);
            btn.add_css_class("flat");
            btn.add_css_class("search-reader-btn");
            btn.set_hexpand(true);
            btn.set_tooltip_text(Some(tooltip));
            btn
        };

        // Tema (Papel / Sepia / Carbón / AMOLED).
        let theme_sec = make_section("Tema");
        let theme_row = BoxWidget::new(Orientation::Horizontal, 4);
        let btn_paper = make_choice_btn("Papel", "Tema claro papel");
        let btn_sepia = make_choice_btn("Sepia", "Tema cálido sepia");
        let btn_charcoal = make_choice_btn("Carbón", "Tema oscuro carbón");
        let btn_amoled = make_choice_btn("AMOLED", "Tema negro puro AMOLED");
        for b in [&btn_paper, &btn_sepia, &btn_charcoal, &btn_amoled] {
            theme_row.append(b);
        }
        theme_sec.append(&theme_row);
        pop_box.append(&theme_sec);

        // Tipografía (Sans / Serif / Mono).
        let font_sec = make_section("Tipografía");
        let font_row = BoxWidget::new(Orientation::Horizontal, 4);
        let btn_sans = make_choice_btn("Sans", "Tipografía sans-serif");
        let btn_serif = make_choice_btn("Serif", "Tipografía serif");
        let btn_mono = make_choice_btn("Mono", "Tipografía monoespaciada");
        for b in [&btn_sans, &btn_serif, &btn_mono] {
            font_row.append(b);
        }
        font_sec.append(&font_row);
        pop_box.append(&font_sec);

        // Medida (58ch / 68ch / 78ch).
        let measure_sec = make_section("Ancho de línea");
        let measure_row = BoxWidget::new(Orientation::Horizontal, 4);
        let btn_narrow = make_choice_btn("58ch", "Columna estrecha (58 caracteres)");
        let btn_medium = make_choice_btn("68ch", "Columna equilibrada (68 caracteres)");
        let btn_wide = make_choice_btn("78ch", "Columna ancha (78 caracteres)");
        for b in [&btn_narrow, &btn_medium, &btn_wide] {
            measure_row.append(b);
        }
        measure_sec.append(&measure_row);
        pop_box.append(&measure_sec);

        // Interlineado (1.5 / 1.7 / 1.9).
        let lh_sec = make_section("Interlineado");
        let lh_row = BoxWidget::new(Orientation::Horizontal, 4);
        let btn_lh_compact = make_choice_btn("1.5", "Interlineado compacto (1.5)");
        let btn_lh_normal = make_choice_btn("1.7", "Interlineado normal (1.7)");
        let btn_lh_relaxed = make_choice_btn("1.9", "Interlineado amplio (1.9)");
        for b in [&btn_lh_compact, &btn_lh_normal, &btn_lh_relaxed] {
            lh_row.append(b);
        }
        lh_sec.append(&lh_row);
        pop_box.append(&lh_sec);

        appearance_pop.set_child(Some(&pop_box));

        let appearance_btn = MenuButton::new();
        appearance_btn.add_css_class("flat");
        appearance_btn.add_css_class("search-reader-btn");
        appearance_btn.set_child(Some(&icons::image(IconId::Palette, 14)));
        appearance_btn.set_tooltip_text(Some("Apariencia y tipografía"));
        appearance_btn.set_popover(Some(&appearance_pop));
        root.append(&appearance_btn);

        let btn_exit = icons::icon_button(IconId::Close, 14);
        btn_exit.add_css_class("search-reader-btn");
        btn_exit.set_tooltip_text(Some("Salir del modo lectura (F9 / Ctrl+Alt+R)"));
        {
            let cb = on_exit.clone();
            btn_exit.connect_clicked(move || cb());
        }
        root.append(&btn_exit);

        let bar = Self {
            root,
            size_label,
            theme_buttons: [
                (ReaderTheme::Paper, btn_paper.clone()),
                (ReaderTheme::Sepia, btn_sepia.clone()),
                (ReaderTheme::Charcoal, btn_charcoal.clone()),
                (ReaderTheme::Amoled, btn_amoled.clone()),
            ],
            font_buttons: [
                (ReaderFont::Sans, btn_sans.clone()),
                (ReaderFont::Serif, btn_serif.clone()),
                (ReaderFont::Mono, btn_mono.clone()),
            ],
            measure_buttons: [
                (ReaderMeasure::Narrow, btn_narrow.clone()),
                (ReaderMeasure::Medium, btn_medium.clone()),
                (ReaderMeasure::Wide, btn_wide.clone()),
            ],
            line_height_buttons: [
                (ReaderLineHeight::Compact, btn_lh_compact.clone()),
                (ReaderLineHeight::Normal, btn_lh_normal.clone()),
                (ReaderLineHeight::Relaxed, btn_lh_relaxed.clone()),
            ],
            look,
            theme,
            font,
            measure,
            line_height,
            font_size,
            settings,
            on_options_changed,
        };
        bar.sync_active_styles();

        {
            let b = bar.clone();
            btn_smaller.connect_clicked(move || b.step_font_size(-2));
        }
        {
            let b = bar.clone();
            btn_larger.connect_clicked(move || b.step_font_size(2));
        }
        for (target, btn) in &bar.theme_buttons {
            let b = bar.clone();
            let t = *target;
            btn.connect_clicked(move || b.set_theme(t));
        }
        for (target, btn) in &bar.font_buttons {
            let b = bar.clone();
            let f = *target;
            btn.connect_clicked(move || b.set_font(f));
        }
        for (target, btn) in &bar.measure_buttons {
            let b = bar.clone();
            let m = *target;
            btn.connect_clicked(move || b.set_measure(m));
        }
        for (target, btn) in &bar.line_height_buttons {
            let b = bar.clone();
            let lh = *target;
            btn.connect_clicked(move || b.set_line_height(lh));
        }

        bar
    }

    pub fn widget(&self) -> &BoxWidget {
        &self.root
    }

    pub fn set_visible(&self, visible: bool) {
        self.root.set_visible(visible);
    }

    pub fn options(&self) -> ReaderOptions {
        ReaderOptions {
            look: self.look.get(),
            theme: self.theme.get(),
            font: self.font.get(),
            measure: self.measure.get(),
            line_height: self.line_height.get(),
            font_size_px: self.font_size.get(),
        }
    }

    fn step_font_size(&self, delta: i32) {
        let next = (self.font_size.get() + delta).clamp(12, 36);
        self.font_size.set(next);
        self.size_label.set_text(&format!("{next}px"));
        self.settings.update(|s| s.reader_font_size = next);
        (self.on_options_changed)(self.options());
    }

    fn set_theme(&self, theme: ReaderTheme) {
        self.theme.set(Some(theme));
        self.look.set(match theme {
            ReaderTheme::Paper | ReaderTheme::Sepia => ColorLook::Light,
            ReaderTheme::Charcoal | ReaderTheme::Amoled => ColorLook::Dark,
        });
        self.sync_active_styles();
        (self.on_options_changed)(self.options());
    }

    fn set_font(&self, font: ReaderFont) {
        self.font.set(font);
        self.sync_active_styles();
        (self.on_options_changed)(self.options());
    }

    fn set_measure(&self, measure: ReaderMeasure) {
        self.measure.set(measure);
        self.sync_active_styles();
        (self.on_options_changed)(self.options());
    }

    fn set_line_height(&self, line_height: ReaderLineHeight) {
        self.line_height.set(line_height);
        self.sync_active_styles();
        (self.on_options_changed)(self.options());
    }

    fn sync_active_styles(&self) {
        let cur_theme = self.theme.get();
        for (target, btn) in &self.theme_buttons {
            if cur_theme == Some(*target) {
                btn.add_css_class("active");
            } else {
                btn.remove_css_class("active");
            }
        }
        let cur_font = self.font.get();
        for (target, btn) in &self.font_buttons {
            if cur_font == *target {
                btn.add_css_class("active");
            } else {
                btn.remove_css_class("active");
            }
        }
        let cur_measure = self.measure.get();
        for (target, btn) in &self.measure_buttons {
            if cur_measure == *target {
                btn.add_css_class("active");
            } else {
                btn.remove_css_class("active");
            }
        }
        let cur_lh = self.line_height.get();
        for (target, btn) in &self.line_height_buttons {
            if cur_lh == *target {
                btn.add_css_class("active");
            } else {
                btn.remove_css_class("active");
            }
        }
    }
}
