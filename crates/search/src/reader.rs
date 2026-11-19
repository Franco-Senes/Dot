//! Integración del modo lectura (`Reader.swift`) sobre `third_party/readability/readability.js`,
//! saneamiento y plantilla HTML/CSS en `search_core::reader`, y píldora flotante discreta
//! para controlar tema (sistema / claro / oscuro) y tamaño de fuente (`reader.fontSize`).

use crate::settings::SettingsStore;
use gnome_platform::gtk::{Align, AsWidget, BoxWidget, Button, Label, Orientation};
use search_core::reader::ReaderOptions;
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

/// Píldora flotante discreta en la esquina superior derecha visible únicamente en modo lectura
/// (`ReaderLook` de `Reader.swift`): tamaño de fuente A−/A+, tema sistema/claro/oscuro y salir.
#[derive(Clone)]
pub struct ReaderToolbar {
    root: BoxWidget,
    size_label: Label,
    btn_system: Button,
    btn_light: Button,
    btn_dark: Button,
    look: Rc<Cell<ColorLook>>,
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

        let btn_system = Button::with_label("Auto");
        btn_system.add_css_class("flat");
        btn_system.add_css_class("search-reader-btn");
        btn_system.set_tooltip_text(Some("Seguir el tema del sistema"));
        root.append(&btn_system);

        let btn_light = Button::with_label("Claro");
        btn_light.add_css_class("flat");
        btn_light.add_css_class("search-reader-btn");
        btn_light.set_tooltip_text(Some("Tema claro en lectura"));
        root.append(&btn_light);

        let btn_dark = Button::with_label("Oscuro");
        btn_dark.add_css_class("flat");
        btn_dark.add_css_class("search-reader-btn");
        btn_dark.set_tooltip_text(Some("Tema oscuro en lectura"));
        root.append(&btn_dark);

        let btn_exit = Button::from_icon_name("window-close-symbolic");
        btn_exit.add_css_class("flat");
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
            btn_system: btn_system.clone(),
            btn_light: btn_light.clone(),
            btn_dark: btn_dark.clone(),
            look,
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
        {
            let b = bar.clone();
            btn_system.connect_clicked(move || b.set_look(ColorLook::System));
        }
        {
            let b = bar.clone();
            btn_light.connect_clicked(move || b.set_look(ColorLook::Light));
        }
        {
            let b = bar.clone();
            btn_dark.connect_clicked(move || b.set_look(ColorLook::Dark));
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

    fn set_look(&self, look: ColorLook) {
        self.look.set(look);
        self.sync_active_styles();
        (self.on_options_changed)(self.options());
    }

    fn sync_active_styles(&self) {
        let current = self.look.get();
        for (btn, target) in [
            (&self.btn_system, ColorLook::System),
            (&self.btn_light, ColorLook::Light),
            (&self.btn_dark, ColorLook::Dark),
        ] {
            if current == target {
                btn.add_css_class("active");
            } else {
                btn.remove_css_class("active");
            }
        }
    }
}
