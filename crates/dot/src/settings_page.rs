//! Página nativa completa de configuración como pestaña real del navegador: `dot://settings`.
//!
//! Inspirada en `Settings.swift` y la estética limpia de GNOME Adwaita + Arc:
//! - Riel izquierdo de 8 categorías con búsqueda en vivo sobre todos los ajustes y atajos.
//! - Panel derecho desplazable con tarjetas `AdwPreferencesGroup`, controles reactivos con
//!   efecto inmediato, resumen en vivo de flags de Chromium 156 (con Site Isolation protegido)
//!   y cero sobrecarga de CEF (100 % GTK4/libadwaita nativo).

use crate::downloads::{default_system_downloads_dir, DownloadManager};
use crate::extensions;
use crate::history_ui::HistoryManager;
use crate::icons::{self, IconId};
use crate::settings::SettingsStore;
use dot_core::address::SearchEngine;
use dot_core::chromium_flags::{
    ResolvedChromiumFlags, BASE_DISABLED_FEATURES, BASE_PRIVACY_SWITCHES, VAAPI_LINUX_FEATURES,
};
use dot_core::paths::DotPaths;
use dot_core::settings::{AppSettings, ColorLook, NewTabPosition, TabGlyph, UiMotion};
use gnome_platform::adw::{
    ActionRow, ApplicationWindow, Clamp, ComboRow, EntryRow, PreferencesGroup, SpinRow, SwitchRow,
};
use gnome_platform::gtk::{
    Align, AsEditable, AsWidget, BoxWidget, Button, EllipsizeMode, FileDialog, Image, Label,
    ListBox, ListBoxRow, Orientation, PolicyType, ScrolledWindow, SearchEntry, SelectionMode,
    Separator, Stack, StackTransitionType, StringList,
};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

const ENGINE_ORDER: [SearchEngine; 9] = [
    SearchEngine::DuckDuckGo,
    SearchEngine::Google,
    SearchEngine::Brave,
    SearchEngine::Kagi,
    SearchEngine::Startpage,
    SearchEngine::Ecosia,
    SearchEngine::Qwant,
    SearchEngine::Bing,
    SearchEngine::Custom,
];

const ENGINE_LABELS: [&str; 9] = [
    "DuckDuckGo",
    "Google",
    "Brave Search",
    "Kagi",
    "Startpage",
    "Ecosia",
    "Qwant",
    "Bing",
    "Personalizado",
];

const LOOK_ORDER: [ColorLook; 3] = [ColorLook::System, ColorLook::Light, ColorLook::Dark];
const LOOK_LABELS: [&str; 3] = ["Sistema", "Claro", "Oscuro"];

const MOTION_ORDER: [UiMotion; 3] = [UiMotion::Smooth, UiMotion::Reduced, UiMotion::Instant];
const MOTION_LABELS: [&str; 3] = ["Suaves", "Reducidas", "Instantáneas"];

const GLYPH_ORDER: [TabGlyph; 2] = [TabGlyph::Favicon, TabGlyph::Monogram];
const GLYPH_LABELS: [&str; 2] = ["Favicons del sitio", "Monogramas tipográficos"];

const NEW_TAB_POS_ORDER: [NewTabPosition; 2] =
    [NewTabPosition::NextToCurrent, NewTabPosition::AtEnd];
const NEW_TAB_POS_LABELS: [&str; 2] = ["Junto a la pestaña actual", "Al final de la barra"];

const RENDERER_LIMIT_VALUES: [u32; 4] = [0, 4, 6, 8];
const RENDERER_LIMIT_LABELS: [&str; 4] = [
    "Predeterminado (sin tope manual)",
    "4 procesos renderer",
    "6 procesos renderer (recomendado)",
    "8 procesos renderer",
];

const ZOOM_PRESETS: [f64; 10] = [0.5, 0.67, 0.75, 0.85, 0.9, 1.0, 1.1, 1.25, 1.5, 2.0];
const ZOOM_LABELS: [&str; 10] = [
    "50 %", "67 %", "75 %", "85 %", "90 %", "100 %", "110 %", "125 %", "150 %", "200 %",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CategoryDef {
    pub id: &'static str,
    pub title: &'static str,
    pub icon: IconId,
}

const CATEGORIES: &[CategoryDef] = &[
    CategoryDef {
        id: "general",
        title: "General",
        icon: IconId::Settings,
    },
    CategoryDef {
        id: "appearance",
        title: "Apariencia y movimiento",
        icon: IconId::Palette,
    },
    CategoryDef {
        id: "tabs",
        title: "Pestañas y fijación",
        icon: IconId::LayoutTabs,
    },
    CategoryDef {
        id: "privacy",
        title: "Privacidad y bloqueo",
        icon: IconId::Shield,
    },
    CategoryDef {
        id: "performance",
        title: "Rendimiento y motor",
        icon: IconId::Cpu,
    },
    CategoryDef {
        id: "passwords",
        title: "Contraseñas y bóveda",
        icon: IconId::Key,
    },
    CategoryDef {
        id: "shortcuts",
        title: "Atajos de teclado",
        icon: IconId::Keyboard,
    },
    CategoryDef {
        id: "about",
        title: "Acerca de Dot",
        icon: IconId::Info,
    },
];

struct SearchEntryItem {
    section_id: &'static str,
    section_title: &'static str,
    title: String,
    subtitle: String,
    keywords: String,
}

struct RailItem {
    id: &'static str,
    button: Button,
    icon_img: Image,
    icon_id: IconId,
    badge: Label,
}

/// Widget completo de la pestaña interna `dot://settings`.
#[derive(Clone)]
pub struct SettingsPage {
    root: BoxWidget,
    stack: Stack,
    search_entry: SearchEntry,
    search_results_list: ListBox,
    search_empty_label: Label,
    rail_items: Rc<RefCell<Vec<RailItem>>>,
    active_section: Rc<RefCell<String>>,
    index: Rc<Vec<SearchEntryItem>>,
    themed_icons: Rc<RefCell<Vec<(Image, IconId)>>>,
}

impl SettingsPage {
    pub fn new(
        parent_window: &ApplicationWindow,
        settings: &SettingsStore,
        history: &HistoryManager,
        downloads: &DownloadManager,
        paths: &DotPaths,
        initial_section: Option<&str>,
    ) -> Self {
        let dark = icons::is_dark_theme();
        let snap = settings.snapshot();

        let root = BoxWidget::new(Orientation::Horizontal, 0);
        root.add_css_class("search-settings-root");
        root.set_hexpand(true);
        root.set_vexpand(true);

        // ====================================================================
        // Riel izquierdo de categorías + buscador global
        // ====================================================================
        let rail = BoxWidget::new(Orientation::Vertical, 6);
        rail.add_css_class("search-settings-rail");
        rail.set_size_request(238, -1);
        rail.set_vexpand(true);

        let rail_head = BoxWidget::new(Orientation::Vertical, 2);
        rail_head.set_margins(14, 6, 12, 12);
        let head_title = Label::new(Some("Ajustes"));
        head_title.add_css_class("search-settings-heading");
        head_title.set_xalign(0.0);
        let head_sub = Label::new(Some("dot://settings"));
        head_sub.add_css_class("search-panel-subtitle");
        head_sub.set_xalign(0.0);
        rail_head.append(&head_title);
        rail_head.append(&head_sub);
        rail.append(&rail_head);

        let search_entry = SearchEntry::new();
        search_entry.set_placeholder_text("Buscar un ajuste…");
        search_entry.set_margins(0, 6, 10, 10);
        rail.append(&search_entry);

        let nav_box = BoxWidget::new(Orientation::Vertical, 2);
        nav_box.set_margins(0, 10, 8, 8);
        nav_box.set_vexpand(true);

        let rail_items: Rc<RefCell<Vec<RailItem>>> = Rc::new(RefCell::new(Vec::new()));
        let themed_icons: Rc<RefCell<Vec<(Image, IconId)>>> = Rc::new(RefCell::new(Vec::new()));

        let start_section = initial_section
            .and_then(resolve_section_alias)
            .unwrap_or("general");
        let active_section = Rc::new(RefCell::new(start_section.to_string()));

        // Stack derecho de secciones + vista de resultados de búsqueda.
        let stack = Stack::new();
        stack.set_hexpand(true);
        stack.set_vexpand(true);
        stack.set_transition_type(StackTransitionType::Crossfade);
        stack.set_transition_duration(snap.ui_motion.duration_ms(160));

        // Construir los botones del riel izquierdo.
        for (idx, cat) in CATEGORIES.iter().enumerate() {
            if idx == 3 || idx == 7 {
                let sep = Separator::new(Orientation::Horizontal);
                sep.set_margins(6, 6, 6, 6);
                nav_box.append(&sep);
            }
            let btn = Button::new();
            btn.add_css_class("flat");
            btn.add_css_class("search-settings-rail-btn");
            if cat.id == start_section {
                btn.add_css_class("active");
            }

            let hbox = BoxWidget::new(Orientation::Horizontal, 9);
            hbox.set_valign(Align::Center);
            let icon_img = icons::image_for_theme(cat.icon, 15, dark);
            hbox.append(&icon_img);

            let lbl = Label::new(Some(cat.title));
            lbl.set_xalign(0.0);
            lbl.set_hexpand(true);
            lbl.set_ellipsize(EllipsizeMode::End);
            hbox.append(&lbl);

            let badge = Label::new(None);
            badge.add_css_class("search-settings-match-badge");
            badge.set_visible(false);
            hbox.append(&badge);

            btn.set_child(Some(&hbox));
            nav_box.append(&btn);

            rail_items.borrow_mut().push(RailItem {
                id: cat.id,
                button: btn,
                icon_img,
                icon_id: cat.icon,
                badge,
            });
        }

        let nav_scroll = ScrolledWindow::new();
        nav_scroll.set_policy(PolicyType::Never, PolicyType::Automatic);
        nav_scroll.set_vexpand(true);
        nav_scroll.set_child(Some(&nav_box));
        rail.append(&nav_scroll);

        root.append(&rail);
        root.append(&Separator::new(Orientation::Vertical));

        // ====================================================================
        // Construcción de las 8 secciones + índice de búsqueda
        // ====================================================================
        let mut index = Vec::new();
        let mut register_icon = |img: &Image, id: IconId| {
            themed_icons.borrow_mut().push((img.clone(), id));
        };

        let p_general = build_general_section(
            parent_window,
            settings,
            &snap,
            dark,
            &mut index,
            &mut register_icon,
        );
        let p_appearance =
            build_appearance_section(settings, &snap, dark, &mut index, &mut register_icon);
        let p_tabs = build_tabs_section(settings, &snap, dark, &mut index, &mut register_icon);
        let p_privacy = build_privacy_section(
            settings,
            history,
            downloads,
            &snap,
            dark,
            &mut index,
            &mut register_icon,
        );
        let p_perf = build_performance_section(
            parent_window,
            paths,
            settings,
            &snap,
            dark,
            &mut index,
            &mut register_icon,
        );
        let p_passwords =
            build_passwords_section(settings, &snap, dark, &mut index, &mut register_icon);
        let p_shortcuts = build_shortcuts_section(&mut index);
        let p_about =
            build_about_section(paths, settings, &snap, dark, &mut index, &mut register_icon);

        stack.add_named(&p_general, "general");
        stack.add_named(&p_appearance, "appearance");
        stack.add_named(&p_tabs, "tabs");
        stack.add_named(&p_privacy, "privacy");
        stack.add_named(&p_perf, "performance");
        stack.add_named(&p_passwords, "passwords");
        stack.add_named(&p_shortcuts, "shortcuts");
        stack.add_named(&p_about, "about");

        // Vista dinámica de resultados de búsqueda.
        let search_results_list = ListBox::new();
        search_results_list.add_css_class("search-settings-results");
        search_results_list.set_selection_mode(SelectionMode::None);

        let search_empty_label = Label::new(Some("No se encontraron ajustes coincidentes"));
        search_empty_label.add_css_class("search-panel-empty");
        search_empty_label.set_margins(24, 16, 16, 16);
        search_empty_label.set_halign(Align::Center);
        search_empty_label.set_visible(false);

        let search_col = BoxWidget::new(Orientation::Vertical, 10);
        search_col.set_margins(26, 32, 20, 20);
        let search_heading = Label::new(Some("Resultados de búsqueda"));
        search_heading.add_css_class("search-settings-section-title");
        search_heading.set_xalign(0.0);
        search_col.append(&search_heading);
        search_col.append(&search_empty_label);
        search_col.append(&search_results_list);

        let search_page = wrap_scrollable_section(&search_col);
        stack.add_named(&search_page, "search-results");
        stack.set_visible_child_name(start_section);

        root.append(&stack);

        let page = Self {
            root,
            stack,
            search_entry: search_entry.clone(),
            search_results_list,
            search_empty_label,
            rail_items: rail_items.clone(),
            active_section,
            index: Rc::new(index),
            themed_icons,
        };

        // Cablear clics en categorías del riel.
        for item in rail_items.borrow().iter() {
            let p = page.clone();
            let target_id = item.id;
            item.button.connect_clicked(move || {
                p.search_entry.set_text("");
                p.select_section(target_id);
            });
        }

        // Cablear búsqueda en vivo.
        {
            let p = page.clone();
            search_entry.connect_search_changed(move || {
                p.update_search();
            });
        }
        {
            let p = page.clone();
            search_entry.connect_stop_search(move || {
                p.search_entry.set_text("");
                p.update_search();
            });
        }

        page
    }

    pub fn widget(&self) -> &BoxWidget {
        &self.root
    }

    pub fn set_motion_duration(&self, ms: u32) {
        self.stack.set_transition_duration(ms);
    }

    pub fn focus_search(&self) {
        self.search_entry.grab_focus();
    }

    pub fn select_section(&self, raw_section: &str) {
        let target = resolve_section_alias(raw_section).unwrap_or("general");
        *self.active_section.borrow_mut() = target.to_string();
        self.stack.set_visible_child_name(target);
        for item in self.rail_items.borrow().iter() {
            if item.id == target {
                item.button.add_css_class("active");
            } else {
                item.button.remove_css_class("active");
            }
        }
    }

    /// Re-renderiza todos los iconos propios de la página al cambiar entre tema claro y oscuro.
    pub fn refresh_theme_icons(&self, dark: bool) {
        for item in self.rail_items.borrow().iter() {
            icons::set_image_icon(&item.icon_img, item.icon_id, dark);
        }
        for (img, id) in self.themed_icons.borrow().iter() {
            icons::set_image_icon(img, *id, dark);
        }
    }

    fn update_search(&self) {
        let query = self.search_entry.text();
        let trimmed = query.trim().to_lowercase();
        if trimmed.is_empty() {
            for item in self.rail_items.borrow().iter() {
                item.badge.set_visible(false);
            }
            let cur = self.active_section.borrow().clone();
            self.stack.set_visible_child_name(&cur);
            return;
        }

        let terms: Vec<&str> = trimmed.split_whitespace().collect();
        self.search_results_list.remove_all();

        let mut counts: std::collections::HashMap<&'static str, usize> =
            std::collections::HashMap::new();
        let mut matches = 0usize;

        for entry in self.index.iter() {
            let haystack = format!(
                "{} {} {} {}",
                entry.section_title, entry.title, entry.subtitle, entry.keywords
            )
            .to_lowercase();
            if terms.iter().all(|t| haystack.contains(t)) {
                matches += 1;
                *counts.entry(entry.section_id).or_insert(0) += 1;

                let row = ListBoxRow::new();
                row.add_css_class("search-settings-result-row");
                row.set_selectable(false);
                row.set_activatable(false);

                let btn = Button::new();
                btn.add_css_class("flat");
                btn.add_css_class("search-settings-result-btn");
                btn.set_hexpand(true);

                let hbox = BoxWidget::new(Orientation::Horizontal, 10);
                hbox.set_margins(8, 8, 10, 10);

                let col = BoxWidget::new(Orientation::Vertical, 2);
                col.set_hexpand(true);

                let title_lbl = Label::new(Some(&entry.title));
                title_lbl.set_xalign(0.0);
                title_lbl.set_ellipsize(EllipsizeMode::End);
                col.append(&title_lbl);

                if !entry.subtitle.is_empty() {
                    let sub_lbl = Label::new(Some(&entry.subtitle));
                    sub_lbl.add_css_class("search-panel-subtitle");
                    sub_lbl.set_xalign(0.0);
                    sub_lbl.set_ellipsize(EllipsizeMode::End);
                    col.append(&sub_lbl);
                }
                hbox.append(&col);

                let sec_pill = Label::new(Some(entry.section_title));
                sec_pill.add_css_class("search-switcher-badge");
                sec_pill.set_valign(Align::Center);
                hbox.append(&sec_pill);

                btn.set_child(Some(&hbox));
                {
                    let p = self.clone();
                    let target_sec = entry.section_id;
                    btn.connect_clicked(move || {
                        p.search_entry.set_text("");
                        p.select_section(target_sec);
                    });
                }

                row.set_child(Some(&btn));
                self.search_results_list.append(&row);
            }
        }

        for item in self.rail_items.borrow().iter() {
            let n = counts.get(item.id).copied().unwrap_or(0);
            if n > 0 {
                item.badge.set_text(&n.to_string());
                item.badge.set_visible(true);
            } else {
                item.badge.set_visible(false);
            }
        }

        self.search_empty_label.set_visible(matches == 0);
        self.stack.set_visible_child_name("search-results");
    }
}

fn resolve_section_alias(raw: &str) -> Option<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "general" | "downloads" | "descargas" => Some("general"),
        "appearance" | "apariencia" | "theme" | "motion" => Some("appearance"),
        "tabs" | "pestanas" | "pins" | "pinning" | "folders" => Some("tabs"),
        "privacy" | "privacidad" | "shield" | "blocking" => Some("privacy"),
        "performance" | "rendimiento" | "engine" | "chromium" => Some("performance"),
        "passwords" | "vault" | "contrasenas" => Some("passwords"),
        "shortcuts" | "keyboard" | "atajos" => Some("shortcuts"),
        "about" | "acerca" => Some("about"),
        _ => None,
    }
}

fn wrap_scrollable_section(content: &BoxWidget) -> ScrolledWindow {
    let clamp = Clamp::new();
    clamp.set_maximum_size(700);
    clamp.set_tightening_threshold(520);
    clamp.set_child(Some(content));

    let scroller = ScrolledWindow::new();
    scroller.set_policy(PolicyType::Never, PolicyType::Automatic);
    scroller.set_hexpand(true);
    scroller.set_vexpand(true);
    scroller.set_child(Some(&clamp));
    scroller
}

fn section_container(title: &str, subtitle: &str) -> BoxWidget {
    let col = BoxWidget::new(Orientation::Vertical, 18);
    col.set_margins(26, 38, 22, 22);

    let head = BoxWidget::new(Orientation::Vertical, 4);
    let t = Label::new(Some(title));
    t.add_css_class("search-settings-section-title");
    t.set_xalign(0.0);
    let s = Label::new(Some(subtitle));
    s.add_css_class("search-panel-subtitle");
    s.set_xalign(0.0);
    s.set_wrap(true);
    head.append(&t);
    head.append(&s);
    col.append(&head);
    col
}

fn push_index(
    index: &mut Vec<SearchEntryItem>,
    section_id: &'static str,
    section_title: &'static str,
    title: &str,
    subtitle: &str,
    keywords: &str,
) {
    index.push(SearchEntryItem {
        section_id,
        section_title,
        title: title.to_string(),
        subtitle: subtitle.to_string(),
        keywords: keywords.to_string(),
    });
}

fn make_prefix_icon(id: IconId, dark: bool, register: &mut impl FnMut(&Image, IconId)) -> Image {
    let img = icons::image_for_theme(id, 16, dark);
    register(&img, id);
    img
}

// ============================================================================
// 1. General
// ============================================================================
fn build_general_section(
    parent_window: &ApplicationWindow,
    settings: &SettingsStore,
    snap: &dot_core::settings::AppSettings,
    dark: bool,
    index: &mut Vec<SearchEntryItem>,
    register: &mut impl FnMut(&Image, IconId),
) -> ScrolledWindow {
    let col = section_container(
        "General",
        "Motor de búsqueda, arranque, descargas y comportamiento predeterminado.",
    );

    // Búsqueda
    let search_group = PreferencesGroup::new();
    search_group.set_title("Búsqueda");

    let engine_row = ComboRow::new();
    engine_row.set_title("Motor de búsqueda predeterminado");
    engine_row.set_subtitle("Consultas escritas en la barra de direcciones (sin telemetría)");
    engine_row.add_prefix(&make_prefix_icon(IconId::Search, dark, register));
    let engine_model = StringList::new(&ENGINE_LABELS);
    engine_row.set_string_model(&engine_model);
    let engine_idx = ENGINE_ORDER
        .iter()
        .position(|&e| e == snap.search_engine)
        .unwrap_or(0) as u32;
    engine_row.set_selected(engine_idx);
    push_index(
        index,
        "general",
        "General",
        "Motor de búsqueda predeterminado",
        "DuckDuckGo, Google, Brave, Kagi, Startpage, Ecosia, Qwant, Bing o personalizado",
        "search engine buscador duckduckgo google brave kagi startpage ecosia qwant bing custom",
    );

    let custom_url_row = EntryRow::new();
    custom_url_row.set_title("Plantilla de búsqueda personalizada (%s)");
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
    search_group.add(&engine_row);
    search_group.add(&custom_url_row);
    col.append(&search_group);

    // Arranque y salida
    let startup_group = PreferencesGroup::new();
    startup_group.set_title("Arranque y cierre");

    let startup_row = ComboRow::new();
    startup_row.set_title("Al abrir Dot");
    startup_row.set_subtitle("Elige si reanudar tus pestañas o empezar limpio");
    startup_row.add_prefix(&make_prefix_icon(IconId::History, dark, register));
    let startup_model = StringList::new(&[
        "Restaurar la sesión anterior (carga diferida)",
        "Empezar con una pestaña en blanco",
    ]);
    startup_row.set_string_model(&startup_model);
    startup_row.set_selected(if snap.restore_session { 0 } else { 1 });
    {
        let st = settings.clone();
        let row_ref = startup_row.clone();
        startup_row.connect_selected_notify(move || {
            let restore = row_ref.selected() == 0;
            st.update(|s| s.restore_session = restore);
        });
    }
    startup_group.add(&startup_row);
    push_index(
        index,
        "general",
        "General",
        "Al abrir Dot",
        "Restaurar sesión anterior o empezar en blanco",
        "startup arranque sesion restore blank",
    );

    let confirm_quit_row = SwitchRow::new();
    confirm_quit_row.set_title("Confirmar antes de salir (Ctrl+Q)");
    confirm_quit_row
        .set_subtitle("Exige pulsar Ctrl+Q una segunda vez cuando hay pestañas abiertas");
    confirm_quit_row.set_active(snap.confirm_quit);
    {
        let st = settings.clone();
        let row_ref = confirm_quit_row.clone();
        confirm_quit_row.connect_active_notify(move || {
            let confirm = row_ref.is_active();
            st.update(|s| s.confirm_quit = confirm);
        });
    }
    startup_group.add(&confirm_quit_row);
    push_index(
        index,
        "general",
        "General",
        "Confirmar antes de salir (Ctrl+Q)",
        "Evita cerrar el navegador por accidente",
        "quit salir confirm ctrl+q",
    );

    thread_local! {
        static START_PAGE_NEWTAB: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
    }
    let start_page_row = SwitchRow::new();
    start_page_row.set_title("Abrir página de inicio (dot://newtab) al crear pestaña");
    start_page_row
        .set_subtitle("Muestra la página de inicio Dot con accesos rápidos en cada nueva pestaña");
    start_page_row.set_active(START_PAGE_NEWTAB.with(|c| c.get()));
    {
        let row_ref = start_page_row.clone();
        start_page_row.connect_active_notify(move || {
            let active = row_ref.is_active();
            START_PAGE_NEWTAB.with(|c| c.set(active));
        });
    }
    startup_group.add(&start_page_row);
    push_index(
        index,
        "general",
        "General",
        "Página de inicio (dot://newtab)",
        "Mostrar página de inicio Dot en cada nueva pestaña",
        "start page inicio newtab plasma",
    );
    col.append(&startup_group);

    // Descargas
    let downloads_group = PreferencesGroup::new();
    downloads_group.set_title("Descargas");

    let dir_row = ActionRow::new();
    dir_row.set_title("Carpeta de descargas");
    dir_row.add_prefix(&make_prefix_icon(IconId::Download, dark, register));
    let default_dir_display = default_system_downloads_dir()
        .to_string_lossy()
        .into_owned();
    let current_dir_subtitle = if snap.downloads_dir.trim().is_empty() {
        format!("Predeterminada ({default_dir_display})")
    } else {
        snap.downloads_dir.clone()
    };
    dir_row.set_subtitle(&current_dir_subtitle);

    let dir_actions = BoxWidget::new(Orientation::Horizontal, 6);
    dir_actions.set_valign(Align::Center);
    let choose_btn = Button::with_label("Elegir…");
    {
        let st = settings.clone();
        let row_ref = dir_row.clone();
        let parent_ref = parent_window.clone();
        choose_btn.connect_clicked(move || {
            let dlg = FileDialog::new();
            dlg.set_title("Elegir carpeta de descargas");
            let current = st.with(|s| s.downloads_dir.trim().to_string());
            if !current.is_empty() && Path::new(&current).is_dir() {
                dlg.set_initial_folder(Path::new(&current));
            } else {
                let sys_dl = default_system_downloads_dir();
                if sys_dl.is_dir() {
                    dlg.set_initial_folder(&sys_dl);
                }
            }
            let st_done = st.clone();
            let row_done = row_ref.clone();
            dlg.select_folder(Some(&parent_ref), move |opt_dir| {
                if let Some(chosen) = opt_dir {
                    let s_path = chosen.to_string_lossy().into_owned();
                    row_done.set_subtitle(&s_path);
                    st_done.update(|s| s.downloads_dir = s_path);
                }
            });
        });
    }
    dir_actions.append(&choose_btn);

    let reset_btn = icons::icon_button_for_theme(IconId::Undo, 15, dark);
    reset_btn.set_tooltip_text(Some("Restablecer a la carpeta XDG de Descargas"));
    {
        let st = settings.clone();
        let row_ref = dir_row.clone();
        let def_label = format!("Predeterminada ({default_dir_display})");
        reset_btn.connect_clicked(move || {
            row_ref.set_subtitle(&def_label);
            st.update(|s| s.downloads_dir.clear());
        });
    }
    dir_actions.append(&reset_btn);
    dir_row.add_suffix(&dir_actions);
    downloads_group.add(&dir_row);
    push_index(
        index,
        "general",
        "General",
        "Carpeta de descargas",
        "Ubicación donde se guardan los archivos descargados",
        "downloads descargas folder carpeta guardar",
    );

    let ask_row = SwitchRow::new();
    ask_row.set_title("Preguntar siempre dónde guardar");
    ask_row.set_subtitle("Abre el selector de archivos antes de iniciar cada descarga");
    ask_row.set_active(snap.downloads_ask);
    {
        let st = settings.clone();
        let row_ref = ask_row.clone();
        ask_row.connect_active_notify(move || {
            let ask = row_ref.is_active();
            st.update(|s| s.downloads_ask = ask);
        });
    }
    downloads_group.add(&ask_row);
    push_index(
        index,
        "general",
        "General",
        "Preguntar siempre dónde guardar",
        "Selector de archivos al descargar",
        "downloads ask preguntar guardar",
    );
    col.append(&downloads_group);

    // Navegador predeterminado
    let def_group = PreferencesGroup::new();
    def_group.set_title("Integración de escritorio");
    let def_row = ActionRow::new();
    def_row.set_title("Navegador web predeterminado");
    def_row.add_prefix(&make_prefix_icon(IconId::Globe, dark, register));

    let initial_default_status = match std::process::Command::new("xdg-settings")
        .args(["get", "default-web-browser"])
        .output()
    {
        Ok(out) if out.status.success() => {
            let current = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if current == "dev.senes.Dot.desktop" {
                "Dot es actualmente tu navegador predeterminado.".to_string()
            } else if current.is_empty() {
                "No hay ningún navegador predeterminado configurado.".to_string()
            } else {
                format!("Actual: {current}")
            }
        }
        Ok(_) => "No se pudo consultar el navegador predeterminado con xdg-settings.".to_string(),
        Err(_) => {
            "xdg-settings no está disponible; configúralo desde las preferencias de tu escritorio."
                .to_string()
        }
    };
    def_row.set_subtitle(&initial_default_status);

    let set_default_btn = Button::with_label("Hacer navegador predeterminado");
    set_default_btn.set_valign(Align::Center);
    {
        let row_ref = def_row.clone();
        set_default_btn.connect_clicked(move || {
            match std::process::Command::new("xdg-settings")
                .args([
                    "set",
                    "default-web-browser",
                    "dev.senes.Dot.desktop",
                ])
                .output()
            {
                Ok(out) if out.status.success() => {
                    row_ref.set_subtitle(
                        "Dot configurado como navegador predeterminado (dev.senes.Dot.desktop).",
                    );
                }
                Ok(out) => {
                    let err_txt = String::from_utf8_lossy(&out.stderr).trim().to_string();
                    let msg = if err_txt.is_empty() {
                        format!(
                            "xdg-settings devolvió código {}. Configúralo desde tu escritorio.",
                            out.status.code().unwrap_or(-1)
                        )
                    } else {
                        format!("No se pudo establecer: {err_txt}")
                    };
                    row_ref.set_subtitle(&msg);
                }
                Err(_) => {
                    row_ref.set_subtitle(
                        "xdg-settings no está disponible en este entorno; configúralo en tu escritorio.",
                    );
                }
            }
        });
    }
    def_row.add_suffix(&set_default_btn);
    def_group.add(&def_row);
    push_index(
        index,
        "general",
        "General",
        "Navegador web predeterminado",
        "xdg-settings default-web-browser",
        "default browser predeterminado xdg",
    );
    col.append(&def_group);

    wrap_scrollable_section(&col)
}

// ============================================================================
// 2. Apariencia y movimiento
// ============================================================================
fn build_appearance_section(
    settings: &SettingsStore,
    snap: &dot_core::settings::AppSettings,
    dark: bool,
    index: &mut Vec<SearchEntryItem>,
    register: &mut impl FnMut(&Image, IconId),
) -> ScrolledWindow {
    let col = section_container(
        "Apariencia y movimiento",
        "Tema, disposición de pestañas, marco de tarjeta estilo Arc, animaciones y escala.",
    );

    let theme_group = PreferencesGroup::new();
    theme_group.set_title("Tema y diseño");

    let look_row = ComboRow::new();
    look_row.set_title("Esquema de color");
    look_row.set_subtitle("Sigue el estilo del escritorio o fija modo claro/oscuro");
    look_row.add_prefix(&make_prefix_icon(IconId::Palette, dark, register));
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
    theme_group.add(&look_row);
    push_index(
        index,
        "appearance",
        "Apariencia y movimiento",
        "Esquema de color",
        "Sistema, Claro u Oscuro",
        "theme tema look dark light oscuro claro",
    );

    let force_dark_row = SwitchRow::new();
    force_dark_row.set_title("Forzar modo oscuro en la interfaz");
    force_dark_row.set_subtitle("Mantiene la interfaz oscura aunque el sistema use tema claro");
    force_dark_row.set_active(snap.look == ColorLook::Dark);
    {
        let st = settings.clone();
        let row_ref = force_dark_row.clone();
        let look_ref = look_row.clone();
        force_dark_row.connect_active_notify(move || {
            let dark = row_ref.is_active();
            let next_look = if dark {
                ColorLook::Dark
            } else {
                ColorLook::System
            };
            if let Some(idx) = LOOK_ORDER.iter().position(|&l| l == next_look) {
                look_ref.set_selected(idx as u32);
            }
            st.update(|s| s.look = next_look);
        });
    }
    theme_group.add(&force_dark_row);
    push_index(
        index,
        "appearance",
        "Apariencia y movimiento",
        "Forzar modo oscuro",
        "Mantiene la interfaz oscura de forma permanente",
        "force dark mode oscuro forzar",
    );

    const ZOOM_VALUES: [f64; 6] = [0.75, 0.90, 1.0, 1.10, 1.25, 1.50];
    let zoom_row = ComboRow::new();
    zoom_row.set_title("Zoom predeterminado de página");
    zoom_row.set_subtitle("Escala inicial aplicada a todas las páginas web");
    let zoom_model = StringList::new(&[
        "75 %",
        "90 %",
        "100 % (Predeterminado)",
        "110 %",
        "125 %",
        "150 %",
    ]);
    zoom_row.set_string_model(&zoom_model);
    let zoom_idx = ZOOM_VALUES
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            ((*a) - snap.page_zoom)
                .abs()
                .total_cmp(&((*b) - snap.page_zoom).abs())
        })
        .map(|(i, _)| i as u32)
        .unwrap_or(2);
    zoom_row.set_selected(zoom_idx);
    {
        let st = settings.clone();
        let row_ref = zoom_row.clone();
        zoom_row.connect_selected_notify(move || {
            if let Some(&val) = ZOOM_VALUES.get(row_ref.selected() as usize) {
                st.update(|s| s.page_zoom = val);
            }
        });
    }
    theme_group.add(&zoom_row);
    push_index(
        index,
        "appearance",
        "Apariencia y movimiento",
        "Zoom predeterminado",
        "Escala inicial de página (75 % a 150 %)",
        "zoom escala predeterminado default",
    );

    let layout_row = ComboRow::new();
    layout_row.set_title("Disposición de pestañas");
    layout_row.set_subtitle("Alternable en vivo en cualquier momento con Ctrl+Shift+S");
    layout_row.add_prefix(&make_prefix_icon(IconId::LayoutTabs, dark, register));
    let layout_model = StringList::new(&[
        "Barra superior horizontal",
        "Barra lateral izquierda vertical",
    ]);
    layout_row.set_string_model(&layout_model);
    layout_row.set_selected(if snap.vertical_tabs { 1 } else { 0 });
    {
        let st = settings.clone();
        let row_ref = layout_row.clone();
        layout_row.connect_selected_notify(move || {
            let vertical = row_ref.selected() == 1;
            st.update(|s| s.vertical_tabs = vertical);
        });
    }
    theme_group.add(&layout_row);
    push_index(
        index,
        "appearance",
        "Apariencia y movimiento",
        "Disposición de pestañas",
        "Barra superior o barra lateral izquierda vertical",
        "sidebar vertical tabs layout barra lateral superior",
    );

    let width_row = SpinRow::with_range(
        AppSettings::SIDEBAR_WIDTH_MIN,
        AppSettings::SIDEBAR_WIDTH_MAX,
        10.0,
    );
    width_row.set_title("Anchura de la barra lateral (px)");
    width_row.set_subtitle("Ajustable entre 180 px y 440 px con efecto inmediato");
    width_row.set_digits(0);
    width_row.set_value(snap.sidebar_width.clamp(
        AppSettings::SIDEBAR_WIDTH_MIN,
        AppSettings::SIDEBAR_WIDTH_MAX,
    ));
    {
        let st = settings.clone();
        let row_ref = width_row.clone();
        width_row.connect_value_notify(move || {
            let w = row_ref.value().clamp(
                AppSettings::SIDEBAR_WIDTH_MIN,
                AppSettings::SIDEBAR_WIDTH_MAX,
            );
            st.update(|s| s.sidebar_width = w);
        });
    }
    theme_group.add(&width_row);
    push_index(
        index,
        "appearance",
        "Apariencia y movimiento",
        "Anchura de la barra lateral",
        "180–440 px",
        "sidebar width anchura barra lateral",
    );

    let cards_row = SwitchRow::new();
    cards_row.set_title("Marco de tarjeta redondeado en modo lateral");
    cards_row.set_subtitle("Envuelve la página activa en una tarjeta elevada estilo Arc/Dot");
    cards_row.set_active(snap.layout_cards);
    {
        let st = settings.clone();
        let row_ref = cards_row.clone();
        cards_row.connect_active_notify(move || {
            let cards = row_ref.is_active();
            st.update(|s| s.layout_cards = cards);
        });
    }
    theme_group.add(&cards_row);
    push_index(
        index,
        "appearance",
        "Apariencia y movimiento",
        "Marco de tarjeta redondeado",
        "Estilo tarjeta elevada en modo barra lateral",
        "cards layout_cards rounded arc marco tarjeta",
    );
    col.append(&theme_group);

    // Densidad e iconos de pestañas
    let density_group = PreferencesGroup::new();
    density_group.set_title("Densidad y aspecto de pestañas");

    let compact_row = SwitchRow::new();
    compact_row.set_title("Modo Zen / pestañas compactas");
    compact_row.set_subtitle("Reduce la altura de las pestañas para maximizar el área de lectura");
    compact_row.set_active(snap.compact_tabs);
    {
        let st = settings.clone();
        let row_ref = compact_row.clone();
        compact_row.connect_active_notify(move || {
            let v = row_ref.is_active();
            st.update(|s| s.compact_tabs = v);
        });
    }
    density_group.add(&compact_row);
    push_index(
        index,
        "appearance",
        "Apariencia y movimiento",
        "Barra superior de pestañas compacta",
        "Altura reducida en modo barra superior",
        "compact tabs compacta",
    );

    let roomy_row = SwitchRow::new();
    roomy_row.set_title("Filas amplias en la barra lateral");
    roomy_row.set_subtitle("Mayor espaciado vertical entre pestañas y carpetas");
    roomy_row.set_active(snap.roomy_sidebar);
    {
        let st = settings.clone();
        let row_ref = roomy_row.clone();
        roomy_row.connect_active_notify(move || {
            let v = row_ref.is_active();
            st.update(|s| s.roomy_sidebar = v);
        });
    }
    density_group.add(&roomy_row);
    push_index(
        index,
        "appearance",
        "Apariencia y movimiento",
        "Filas amplias en la barra lateral",
        "Mayor aire vertical entre pestañas",
        "roomy sidebar amplias espaciado",
    );

    let domain_row = SwitchRow::new();
    domain_row.set_title("Mostrar dominio en lugar del título completo");
    domain_row.set_subtitle("Pestañas más limpias mostrando el host (ej. docs.rs)");
    domain_row.set_active(snap.tab_show_domain);
    {
        let st = settings.clone();
        let row_ref = domain_row.clone();
        domain_row.connect_active_notify(move || {
            let v = row_ref.is_active();
            st.update(|s| s.tab_show_domain = v);
        });
    }
    density_group.add(&domain_row);
    push_index(
        index,
        "appearance",
        "Apariencia y movimiento",
        "Mostrar dominio en las pestañas",
        "Dominio limpio frente al título completo",
        "domain host title dominio pestana",
    );

    let glyph_row = ComboRow::new();
    glyph_row.set_title("Icono de pestañas");
    glyph_row.set_subtitle("Usa el favicon del sitio o monogramas minimalistas con la inicial");
    let glyph_model = StringList::new(&GLYPH_LABELS);
    glyph_row.set_string_model(&glyph_model);
    let glyph_idx = GLYPH_ORDER
        .iter()
        .position(|&g| g == snap.tab_glyph)
        .unwrap_or(0) as u32;
    glyph_row.set_selected(glyph_idx);
    {
        let st = settings.clone();
        let row_ref = glyph_row.clone();
        glyph_row.connect_selected_notify(move || {
            let idx = row_ref.selected() as usize;
            if let Some(&glyph) = GLYPH_ORDER.get(idx) {
                st.update(|s| s.tab_glyph = glyph);
            }
        });
    }
    density_group.add(&glyph_row);
    push_index(
        index,
        "appearance",
        "Apariencia y movimiento",
        "Icono de pestañas (Favicons / Monogramas)",
        "Favicons o monogramas con inicial",
        "glyph favicon monogram iconos",
    );
    col.append(&density_group);

    // Movimiento y Zoom
    let motion_group = PreferencesGroup::new();
    motion_group.set_title("Animaciones y zoom");

    let motion_row = ComboRow::new();
    motion_row.set_title("Animaciones de interfaz");
    motion_row.set_subtitle("Controla las transiciones de barra lateral, carpetas y paneles");
    let motion_model = StringList::new(&MOTION_LABELS);
    motion_row.set_string_model(&motion_model);
    let motion_idx = MOTION_ORDER
        .iter()
        .position(|&m| m == snap.ui_motion)
        .unwrap_or(0) as u32;
    motion_row.set_selected(motion_idx);
    {
        let st = settings.clone();
        let row_ref = motion_row.clone();
        motion_row.connect_selected_notify(move || {
            let idx = row_ref.selected() as usize;
            if let Some(&m) = MOTION_ORDER.get(idx) {
                st.update(|s| s.ui_motion = m);
            }
        });
    }
    motion_group.add(&motion_row);
    push_index(
        index,
        "appearance",
        "Apariencia y movimiento",
        "Animaciones de interfaz",
        "Suaves, Reducidas o Instantáneas",
        "motion animaciones smooth reduced instant",
    );

    let zoom_row = ComboRow::new();
    zoom_row.set_title("Zoom predeterminado de páginas");
    zoom_row.set_subtitle("Escala base aplicada a las páginas web (50 % – 200 %)");
    let zoom_model = StringList::new(&ZOOM_LABELS);
    zoom_row.set_string_model(&zoom_model);
    let zoom_idx = ZOOM_PRESETS
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            (*a - snap.page_zoom)
                .abs()
                .partial_cmp(&(*b - snap.page_zoom).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| i as u32)
        .unwrap_or(5);
    zoom_row.set_selected(zoom_idx);
    {
        let st = settings.clone();
        let row_ref = zoom_row.clone();
        zoom_row.connect_selected_notify(move || {
            let idx = row_ref.selected() as usize;
            if let Some(&z) = ZOOM_PRESETS.get(idx) {
                st.update(|s| s.page_zoom = z);
            }
        });
    }
    motion_group.add(&zoom_row);
    push_index(
        index,
        "appearance",
        "Apariencia y movimiento",
        "Zoom predeterminado de páginas",
        "50 % – 200 %",
        "zoom escala pageZoom",
    );
    col.append(&motion_group);

    wrap_scrollable_section(&col)
}

// ============================================================================
// 3. Pestañas y fijación (estilo Arc)
// ============================================================================
fn build_tabs_section(
    settings: &SettingsStore,
    snap: &dot_core::settings::AppSettings,
    dark: bool,
    index: &mut Vec<SearchEntryItem>,
    register: &mut impl FnMut(&Image, IconId),
) -> ScrolledWindow {
    let col = section_container(
        "Pestañas y fijación (estilo Arc)",
        "Favoritos en cuadrícula, filas fijadas, carpetas anidadas, carga diferida y comportamiento de cierre.",
    );

    let behavior_group = PreferencesGroup::new();
    behavior_group.set_title("Comportamiento de pestañas");

    let pos_row = ComboRow::new();
    pos_row.set_title("Posición de nuevas pestañas");
    pos_row.set_subtitle("Dónde insertar una pestaña al pulsar Ctrl+T o abrir un enlace");
    pos_row.add_prefix(&make_prefix_icon(IconId::Plus, dark, register));
    let pos_model = StringList::new(&NEW_TAB_POS_LABELS);
    pos_row.set_string_model(&pos_model);
    let pos_idx = NEW_TAB_POS_ORDER
        .iter()
        .position(|&p| p == snap.new_tab_position)
        .unwrap_or(0) as u32;
    pos_row.set_selected(pos_idx);
    {
        let st = settings.clone();
        let row_ref = pos_row.clone();
        pos_row.connect_selected_notify(move || {
            let idx = row_ref.selected() as usize;
            if let Some(&pos) = NEW_TAB_POS_ORDER.get(idx) {
                st.update(|s| s.new_tab_position = pos);
            }
        });
    }
    behavior_group.add(&pos_row);
    push_index(
        index,
        "tabs",
        "Pestañas y fijación",
        "Posición de nuevas pestañas",
        "Junto a la actual o al final",
        "new tab position posicion nueva pestana",
    );

    let bg_links_row = SwitchRow::new();
    bg_links_row.set_title("Abrir enlaces de clic central en segundo plano");
    bg_links_row
        .set_subtitle("Mantiene el foco en la página actual al abrir enlaces en pestañas nuevas");
    bg_links_row.set_active(snap.open_links_in_background);
    {
        let st = settings.clone();
        let row_ref = bg_links_row.clone();
        bg_links_row.connect_active_notify(move || {
            let v = row_ref.is_active();
            st.update(|s| s.open_links_in_background = v);
        });
    }
    behavior_group.add(&bg_links_row);
    push_index(
        index,
        "tabs",
        "Pestañas y fijación",
        "Abrir enlaces en segundo plano",
        "Clic central y enlaces externos",
        "background links enlaces segundo plano",
    );

    let audio_row = SwitchRow::new();
    audio_row.set_title("Mostrar indicadores de audio y silencio");
    audio_row.set_subtitle("Permite silenciar pestañas ruidosas con un clic o Ctrl+Shift+M");
    audio_row.add_prefix(&make_prefix_icon(IconId::VolumeHigh, dark, register));
    audio_row.set_active(snap.show_audio_indicators);
    {
        let st = settings.clone();
        let row_ref = audio_row.clone();
        audio_row.connect_active_notify(move || {
            let v = row_ref.is_active();
            st.update(|s| s.show_audio_indicators = v);
        });
    }
    behavior_group.add(&audio_row);
    push_index(
        index,
        "tabs",
        "Pestañas y fijación",
        "Indicadores de audio y silencio",
        "Botón de silencio en pestañas con sonido",
        "audio mute sonido silenciar",
    );
    col.append(&behavior_group);

    let pins_group = PreferencesGroup::new();
    pins_group.set_title("Fijación y carpetas estilo Arc");

    let protect_row = SwitchRow::new();
    protect_row.set_title("Proteger pestañas fijadas frente a Ctrl+W");
    protect_row
        .set_subtitle("Ctrl+W duerme la pestaña fijada para ahorrar RAM en lugar de cerrarla");
    protect_row.add_prefix(&make_prefix_icon(IconId::Pin, dark, register));
    protect_row.set_active(snap.protect_pinned);
    {
        let st = settings.clone();
        let row_ref = protect_row.clone();
        protect_row.connect_active_notify(move || {
            let v = row_ref.is_active();
            st.update(|s| s.protect_pinned = v);
        });
    }
    pins_group.add(&protect_row);
    push_index(
        index,
        "tabs",
        "Pestañas y fijación",
        "Proteger pestañas fijadas frente a Ctrl+W",
        "Las pone en reposo en vez de cerrarlas",
        "pin pinned protect fijadas ctrl+w arc",
    );

    let home_row = SwitchRow::new();
    home_row.set_title("Devolver pestañas fijadas a su URL inicial con Ctrl+W");
    home_row.set_subtitle(
        "Si navegaste lejos dentro de una fijada, Ctrl+W restaura su dirección original",
    );
    home_row.set_active(snap.pins_go_home);
    {
        let st = settings.clone();
        let row_ref = home_row.clone();
        home_row.connect_active_notify(move || {
            let v = row_ref.is_active();
            st.update(|s| s.pins_go_home = v);
        });
    }
    pins_group.add(&home_row);
    push_index(
        index,
        "tabs",
        "Pestañas y fijación",
        "Devolver fijadas a su URL inicial",
        "pins_go_home estilo Arc",
        "pins home url inicial fijadas",
    );

    let lazy_row = SwitchRow::new();
    lazy_row.set_title("Carga diferida (lazy) de pestañas restauradas");
    lazy_row
        .set_subtitle("No crea procesos CEF para pestañas en segundo plano hasta que las visitas");
    lazy_row.set_active(snap.lazy_restore);
    {
        let st = settings.clone();
        let row_ref = lazy_row.clone();
        lazy_row.connect_active_notify(move || {
            let v = row_ref.is_active();
            st.update(|s| s.lazy_restore = v);
        });
    }
    pins_group.add(&lazy_row);
    push_index(
        index,
        "tabs",
        "Pestañas y fijación",
        "Carga diferida de pestañas restauradas",
        "Ahorra memoria al restaurar sesiones grandes",
        "lazy sleep restaurar ram memoria",
    );

    let closed_stack_row = SpinRow::with_range(20.0, 100.0, 4.0);
    closed_stack_row.set_title("Capacidad de pestañas cerradas (Ctrl+Shift+T)");
    closed_stack_row.set_subtitle("Número máximo de pestañas recordadas para reabrir en la sesión");
    closed_stack_row.set_digits(0);
    closed_stack_row.set_value(snap.closed_tab_stack_size.clamp(20, 100) as f64);
    {
        let st = settings.clone();
        let row_ref = closed_stack_row.clone();
        closed_stack_row.connect_value_notify(move || {
            let v = (row_ref.value().round() as usize).clamp(20, 100);
            st.update(|s| s.closed_tab_stack_size = v);
        });
    }
    pins_group.add(&closed_stack_row);
    push_index(
        index,
        "tabs",
        "Pestañas y fijación",
        "Capacidad de pestañas cerradas",
        "Pila de Ctrl+Shift+T (20–100)",
        "reopen closed stack ctrl+shift+t cerradas",
    );
    col.append(&pins_group);

    wrap_scrollable_section(&col)
}

// ============================================================================
// 4. Privacidad y bloqueo
// ============================================================================
fn build_privacy_section(
    settings: &SettingsStore,
    history: &HistoryManager,
    downloads: &DownloadManager,
    snap: &dot_core::settings::AppSettings,
    dark: bool,
    index: &mut Vec<SearchEntryItem>,
    register: &mut impl FnMut(&Image, IconId),
) -> ScrolledWindow {
    let col = section_container(
        "Privacidad y bloqueo",
        "Cero telemetría, cero cuentas, cero servicios de Google y protección contra rastreadores.",
    );

    let shield_group = PreferencesGroup::new();
    shield_group.set_title("Protección contra rastreo");

    let shield_row = SwitchRow::new();
    shield_row.set_title("Escudo bloqueador de anuncios y rastreadores (Shield)");
    shield_row.set_subtitle("Interruptor maestro de bloqueo de red y reglas cosméticas");
    shield_row.add_prefix(&make_prefix_icon(IconId::Shield, dark, register));
    shield_row.set_active(snap.shield_enabled);
    {
        let st = settings.clone();
        let row_ref = shield_row.clone();
        shield_row.connect_active_notify(move || {
            let v = row_ref.is_active();
            st.update(|s| s.shield_enabled = v);
        });
    }
    shield_group.add(&shield_row);
    push_index(
        index,
        "privacy",
        "Privacidad y bloqueo",
        "Escudo bloqueador (Shield)",
        "Bloqueo de anuncios y rastreadores",
        "shield adblock bloqueador anuncios trackers",
    );

    let cookies_row = SwitchRow::new();
    cookies_row.set_title("Bloquear cookies de terceros");
    cookies_row
        .set_subtitle("Impide el seguimiento entre dominios distintos (CefCookieAccessFilter)");
    cookies_row.set_active(snap.block_third_party_cookies());
    {
        let st = settings.clone();
        let row_ref = cookies_row.clone();
        cookies_row.connect_active_notify(move || {
            let v = row_ref.is_active();
            st.update(|s| s.set_block_third_party_cookies(v));
        });
    }
    shield_group.add(&cookies_row);
    push_index(
        index,
        "privacy",
        "Privacidad y bloqueo",
        "Bloquear cookies de terceros",
        "Aislamiento de cookies cross-site",
        "cookies terceros third party",
    );

    let gpc_row = SwitchRow::new();
    gpc_row.set_title("Enviar señal Global Privacy Control (Sec-GPC: 1)");
    gpc_row.set_subtitle("Comunica a los sitios tu rechazo a la venta o cesión de datos");
    gpc_row.set_active(snap.send_gpc);
    {
        let st = settings.clone();
        let row_ref = gpc_row.clone();
        gpc_row.connect_active_notify(move || {
            let v = row_ref.is_active();
            st.update(|s| s.send_gpc = v);
        });
    }
    shield_group.add(&gpc_row);
    push_index(
        index,
        "privacy",
        "Privacidad y bloqueo",
        "Global Privacy Control (Sec-GPC: 1)",
        "Cabecera Sec-GPC y navigator.globalPrivacyControl",
        "gpc privacy privacidad",
    );

    let click_ids_row = SwitchRow::new();
    click_ids_row.set_title("Limpiar identificadores de rastreo en enlaces");
    click_ids_row
        .set_subtitle("Elimina automáticamente gclid, fbclid, msclkid y similares al navegar");
    click_ids_row.set_active(snap.strip_click_ids);
    {
        let st = settings.clone();
        let row_ref = click_ids_row.clone();
        click_ids_row.connect_active_notify(move || {
            let v = row_ref.is_active();
            st.update(|s| s.strip_click_ids = v);
        });
    }
    shield_group.add(&click_ids_row);
    push_index(
        index,
        "privacy",
        "Privacidad y bloqueo",
        "Limpiar identificadores de rastreo",
        "gclid, fbclid, msclkid, utm_*",
        "click ids tracking utm fbclid gclid msclkid",
    );
    col.append(&shield_group);

    // Acciones rápidas de borrado de datos
    let clear_group = PreferencesGroup::new();
    clear_group.set_title("Datos de navegación");

    let status_lbl = Label::new(None);
    status_lbl.add_css_class("search-panel-subtitle");
    status_lbl.set_xalign(0.0);
    status_lbl.set_visible(false);

    let clear_hist_row = ActionRow::new();
    clear_hist_row.set_title("Borrar historial de navegación");
    clear_hist_row.set_subtitle("Elimina visitas recientes o todo el historial guardado en disco");
    clear_hist_row.add_prefix(&make_prefix_icon(IconId::Trash, dark, register));

    let hist_btns = BoxWidget::new(Orientation::Horizontal, 6);
    hist_btns.set_valign(Align::Center);

    let btn_1h = Button::with_label("1 hora");
    {
        let h = history.clone();
        let s = status_lbl.clone();
        btn_1h.connect_clicked(move || {
            h.clear_last_hour();
            s.set_text("Historial de la última hora borrado.");
            s.set_visible(true);
        });
    }
    let btn_24h = Button::with_label("24 h");
    {
        let h = history.clone();
        let s = status_lbl.clone();
        btn_24h.connect_clicked(move || {
            h.clear_last_day();
            s.set_text("Historial de las últimas 24 horas borrado.");
            s.set_visible(true);
        });
    }
    let btn_all = Button::with_label("Todo");
    btn_all.add_css_class("destructive-action");
    {
        let h = history.clone();
        let s = status_lbl.clone();
        btn_all.connect_clicked(move || {
            h.clear_all();
            s.set_text("Todo el historial de navegación fue eliminado.");
            s.set_visible(true);
        });
    }
    hist_btns.append(&btn_1h);
    hist_btns.append(&btn_24h);
    hist_btns.append(&btn_all);
    clear_hist_row.add_suffix(&hist_btns);
    clear_group.add(&clear_hist_row);
    push_index(
        index,
        "privacy",
        "Privacidad y bloqueo",
        "Borrar historial de navegación",
        "Última hora, 24 h o todo el historial",
        "clear history borrar historial datos",
    );

    let clear_dl_row = ActionRow::new();
    clear_dl_row.set_title("Limpiar lista de descargas");
    clear_dl_row.set_subtitle("Borra el registro de descargas completadas sin tocar tus archivos");
    let clear_dl_btn = Button::with_label("Limpiar lista");
    {
        let dl = downloads.clone();
        let s = status_lbl.clone();
        clear_dl_btn.connect_clicked(move || {
            dl.clear_history();
            s.set_text("Lista de descargas limpiada.");
            s.set_visible(true);
        });
    }
    clear_dl_row.add_suffix(&clear_dl_btn);
    clear_group.add(&clear_dl_row);
    push_index(
        index,
        "privacy",
        "Privacidad y bloqueo",
        "Limpiar lista de descargas",
        "Limpia el registro downloads.json",
        "clear downloads descargas",
    );

    col.append(&clear_group);
    col.append(&status_lbl);

    wrap_scrollable_section(&col)
}

// ============================================================================
// 5. Rendimiento y motor Chromium 156
// ============================================================================
fn build_performance_section(
    parent_window: &ApplicationWindow,
    paths: &DotPaths,
    settings: &SettingsStore,
    snap: &dot_core::settings::AppSettings,
    dark: bool,
    index: &mut Vec<SearchEntryItem>,
    register: &mut impl FnMut(&Image, IconId),
) -> ScrolledWindow {
    let col = section_container(
        "Rendimiento y motor Chromium 156",
        "Control directo de procesos, memoria y aceleración VA-API sobre chromium_flags.rs.",
    );

    let engine_group = PreferencesGroup::new();
    engine_group.set_title("Memoria y procesos (reinicio necesario)");

    let limit_row = ComboRow::new();
    limit_row.set_title("Límite de procesos renderer (--renderer-process-limit)");
    limit_row.set_subtitle("Acota la RAM usada con muchas pestañas abiertas (reinicio necesario)");
    limit_row.add_prefix(&make_prefix_icon(IconId::Cpu, dark, register));
    let limit_model = StringList::new(&RENDERER_LIMIT_LABELS);
    limit_row.set_string_model(&limit_model);
    let limit_idx = RENDERER_LIMIT_VALUES
        .iter()
        .position(|&v| v == snap.renderer_process_limit)
        .unwrap_or(2) as u32;
    limit_row.set_selected(limit_idx);
    {
        let st = settings.clone();
        let row_ref = limit_row.clone();
        limit_row.connect_selected_notify(move || {
            let idx = row_ref.selected() as usize;
            if let Some(&val) = RENDERER_LIMIT_VALUES.get(idx) {
                st.update(|s| s.renderer_process_limit = val);
            }
        });
    }
    engine_group.add(&limit_row);
    push_index(
        index,
        "performance",
        "Rendimiento y motor",
        "Límite de procesos renderer",
        "--renderer-process-limit (Predeterminado / 4 / 6 / 8)",
        "renderer process limit procesos ram memoria",
    );

    let pps_row = SwitchRow::new();
    pps_row.set_title("Agrupar procesos por sitio (--process-per-site)");
    pps_row.set_subtitle("Comparte proceso entre pestañas del mismo dominio (reinicio necesario)");
    pps_row.set_active(snap.process_per_site);
    {
        let st = settings.clone();
        let row_ref = pps_row.clone();
        pps_row.connect_active_notify(move || {
            let v = row_ref.is_active();
            st.update(|s| s.process_per_site = v);
        });
    }
    engine_group.add(&pps_row);
    push_index(
        index,
        "performance",
        "Rendimiento y motor",
        "Agrupar procesos por sitio",
        "--process-per-site",
        "process per site proceso sitio memoria",
    );

    let mem_saver_row = SwitchRow::new();
    mem_saver_row.set_title("Ahorro de memoria / High Efficiency Mode");
    mem_saver_row.set_subtitle(
        "Activa performance_tuning.high_efficiency_mode.state en RequestContext (reinicio necesario)",
    );
    mem_saver_row.set_active(snap.memory_saver);
    {
        let st = settings.clone();
        let row_ref = mem_saver_row.clone();
        mem_saver_row.connect_active_notify(move || {
            let v = row_ref.is_active();
            st.update(|s| s.memory_saver = v);
        });
    }
    engine_group.add(&mem_saver_row);
    push_index(
        index,
        "performance",
        "Rendimiento y motor",
        "Ahorro de memoria (High Efficiency Mode)",
        "performance_tuning.high_efficiency_mode.state",
        "memory saver high efficiency ahorro memoria",
    );

    let lite_row = SwitchRow::new();
    lite_row.set_title("Modo ligero de JavaScript V8 (--js-flags=--lite-mode)");
    lite_row.set_subtitle(
        "Reduce consumo de memoria JIT a cambio de menor velocidad pico JS (reinicio necesario)",
    );
    lite_row.set_active(snap.v8_lite_mode);
    {
        let st = settings.clone();
        let row_ref = lite_row.clone();
        lite_row.connect_active_notify(move || {
            let v = row_ref.is_active();
            st.update(|s| s.v8_lite_mode = v);
        });
    }
    engine_group.add(&lite_row);
    push_index(
        index,
        "performance",
        "Rendimiento y motor",
        "Modo ligero V8",
        "--js-flags=--lite-mode",
        "v8 lite mode javascript memoria",
    );

    let vaapi_row = SwitchRow::new();
    vaapi_row.set_title("Decodificación de vídeo por hardware (VA-API)");
    vaapi_row.set_subtitle(
        "Activa AcceleratedVideoDecodeLinuxGL, ZeroCopyGL y VaapiIgnoreDriverChecks (AMD Radeon Vega / Mesa; reinicio necesario)",
    );
    vaapi_row.set_active(snap.hardware_video_decode);
    {
        let st = settings.clone();
        let row_ref = vaapi_row.clone();
        vaapi_row.connect_active_notify(move || {
            let v = row_ref.is_active();
            st.update(|s| s.hardware_video_decode = v);
        });
    }
    engine_group.add(&vaapi_row);
    push_index(
        index,
        "performance",
        "Rendimiento y motor",
        "Decodificación de vídeo por hardware (VA-API)",
        "AcceleratedVideoDecodeLinuxGL, ZeroCopyGL, AMD Radeon Vega",
        "vaapi gpu hardware video decode radeon vega",
    );
    col.append(&engine_group);

    // Resumen en vivo de flags y seguridad
    let summary_group = PreferencesGroup::new();
    summary_group.set_title("Estado del motor y aislamiento");

    let iso_row = ActionRow::new();
    iso_row.set_title("Site Isolation (site-per-process / IsolateOrigins)");
    iso_row.set_subtitle(
        "Protegido a nivel de código: jamás se desactiva ni mediante variables de entorno",
    );
    iso_row.add_prefix(&make_prefix_icon(IconId::Shield, dark, register));
    let iso_badge = Label::new(Some("Siempre ACTIVO (protegido)"));
    iso_badge.add_css_class("search-badge-protected");
    iso_badge.set_valign(Align::Center);
    iso_row.add_suffix(&iso_badge);
    summary_group.add(&iso_row);
    push_index(
        index,
        "performance",
        "Rendimiento y motor",
        "Site Isolation",
        "Siempre ACTIVO (protegido)",
        "site isolation seguridad sandbox",
    );

    let privacy_flags_row = ActionRow::new();
    privacy_flags_row.set_title("Flags de privacidad cero-Google activos");
    let disabled_names: Vec<&str> = BASE_DISABLED_FEATURES.iter().map(|(n, _)| *n).collect();
    privacy_flags_row.set_subtitle(&format!(
        "{} switches base activos · Features desactivadas: {}",
        BASE_PRIVACY_SWITCHES.len(),
        disabled_names.join(", ")
    ));
    summary_group.add(&privacy_flags_row);

    let vaapi_names: Vec<&str> = VAAPI_LINUX_FEATURES.iter().map(|(n, _)| *n).collect();
    let cfg = snap.to_chromium_config(false, false, None);
    let resolved = ResolvedChromiumFlags::from_env(&cfg);
    let env_val =
        dot_core::env::var("DOT_CHROMIUM_FLAGS", "SEARCH_CHROMIUM_FLAGS").unwrap_or_default();
    let env_display = if env_val.trim().is_empty() {
        "(ninguno — usando configuración guardada)".to_string()
    } else {
        env_val
    };
    let env_row = ActionRow::new();
    env_row.set_title("Override en ejecución (DOT_CHROMIUM_FLAGS)");
    env_row.set_subtitle(&format!(
        "{env_display} · Switches totales: {} · VA-API: {}",
        resolved.switches().len(),
        vaapi_names.join(", ")
    ));
    summary_group.add(&env_row);
    push_index(
        index,
        "performance",
        "Rendimiento y motor",
        "DOT_CHROMIUM_FLAGS",
        "Override de flags de Chromium 156",
        "flags chromium override env",
    );
    col.append(&summary_group);

    // Extensiones (experimental — Paso 2B)
    let ext_group = PreferencesGroup::new();
    ext_group.set_title("Extensiones de Chrome (experimental · desactivado por defecto)");

    let ext_toggle_row = SwitchRow::new();
    ext_toggle_row.set_title("Habilitar almacén experimental de extensiones CRX3");
    ext_toggle_row.set_subtitle(extensions::runtime_limitation_summary());
    ext_toggle_row.set_active(snap.extensions_enabled);
    ext_group.add(&ext_toggle_row);

    let ext_storage_row = ActionRow::new();
    ext_storage_row.set_title("Almacén verificado de paquetes CRX3");
    let ext_count = extensions::load_store(paths)
        .map(|s| s.list().len())
        .unwrap_or(0);
    ext_storage_row.set_subtitle(&format!(
        "{} · {ext_count} paquete(s) verificado(s) en disco (solo almacenamiento; ejecución MV3 no disponible en Alloy/OSR)",
        extensions::storage_dir(paths).display()
    ));
    ext_storage_row.set_visible(snap.extensions_enabled);

    let install_crx_btn = Button::with_label("Verificar .crx…");
    install_crx_btn.set_valign(Align::Center);
    {
        let parent_ref = parent_window.clone();
        let paths_ref = paths.clone();
        let row_ref = ext_storage_row.clone();
        install_crx_btn.connect_clicked(move || {
            let dlg = FileDialog::new();
            dlg.set_title("Seleccionar paquete de extensión (.crx)");
            let p_done = paths_ref.clone();
            let r_done = row_ref.clone();
            dlg.open(Some(&parent_ref), move |opt_path| {
                let Some(path) = opt_path else {
                    return;
                };
                match extensions::install_crx_file(&p_done, &path, None) {
                    Ok(ext) => {
                        let count = extensions::load_store(&p_done)
                            .map(|s| s.list().len())
                            .unwrap_or(1);
                        r_done.set_subtitle(&format!(
                            "Verificada '{}' v{} (ID {}) · {count} paquete(s) en {}",
                            ext.name,
                            ext.version,
                            ext.id,
                            extensions::storage_dir(&p_done).display()
                        ));
                    }
                    Err(err) => {
                        r_done.set_subtitle(&format!("Paquete CRX3 rechazado: {err}"));
                    }
                }
            });
        });
    }
    ext_storage_row.add_suffix(&install_crx_btn);
    ext_group.add(&ext_storage_row);

    {
        let st = settings.clone();
        let row_ref = ext_toggle_row.clone();
        let storage_ref = ext_storage_row.clone();
        ext_toggle_row.connect_active_notify(move || {
            let enabled = row_ref.is_active();
            storage_ref.set_visible(enabled);
            st.update(|s| s.extensions_enabled = enabled);
        });
    }

    push_index(
        index,
        "performance",
        "Rendimiento y motor",
        "Extensiones de Chrome (CRX3 experimental)",
        "Almacén CRX3 desactivado por defecto en CEF Alloy/OSR",
        "extensiones extensions crx crx3 mv3 chrome experimental",
    );
    col.append(&ext_group);

    wrap_scrollable_section(&col)
}

// ============================================================================
// 6. Contraseñas y bóveda
// ============================================================================
fn build_passwords_section(
    settings: &SettingsStore,
    snap: &dot_core::settings::AppSettings,
    dark: bool,
    index: &mut Vec<SearchEntryItem>,
    register: &mut impl FnMut(&Image, IconId),
) -> ScrolledWindow {
    let col = section_container(
        "Contraseñas y bóveda (Vault)",
        "Gestión privada de credenciales sin almacenar jamás secretos en texto plano.",
    );

    let vault_group = PreferencesGroup::new();
    vault_group.set_title("Preferencias de Vault");

    let save_row = SwitchRow::new();
    save_row.set_title("Ofrecer guardar contraseñas");
    save_row.set_subtitle("Muestra un aviso discreto al iniciar sesión en un sitio");
    save_row.add_prefix(&make_prefix_icon(IconId::Key, dark, register));
    save_row.set_active(snap.vault_save);
    {
        let st = settings.clone();
        let row_ref = save_row.clone();
        save_row.connect_active_notify(move || {
            let v = row_ref.is_active();
            st.update(|s| s.vault_save = v);
        });
    }
    vault_group.add(&save_row);
    push_index(
        index,
        "passwords",
        "Contraseñas y bóveda",
        "Ofrecer guardar contraseñas",
        "Guardado en Vault",
        "passwords save contraseñas guardar vault",
    );

    let fill_row = SwitchRow::new();
    fill_row.set_title("Ofrecer autocompletado de credenciales");
    fill_row.set_subtitle("Sugiere cuentas guardadas en los formularios de acceso");
    fill_row.set_active(snap.vault_fill);
    {
        let st = settings.clone();
        let row_ref = fill_row.clone();
        fill_row.connect_active_notify(move || {
            let v = row_ref.is_active();
            st.update(|s| s.vault_fill = v);
        });
    }
    vault_group.add(&fill_row);
    push_index(
        index,
        "passwords",
        "Contraseñas y bóveda",
        "Ofrecer autocompletado de credenciales",
        "Autocompletado de formularios de login",
        "passwords fill autofill autocompletar vault",
    );

    let status_row = ActionRow::new();
    status_row.set_title("Estado de la integración con el llavero del sistema");
    status_row.set_subtitle(
        "El gestor interno de Chromium está completamente desactivado. La bóveda cifrada mediante Secret Service / Portal (oo7) se integra en la Fase 5.",
    );
    let badge = Label::new(Some("Fase 5 · oo7"));
    badge.add_css_class("search-switcher-badge");
    badge.set_valign(Align::Center);
    status_row.add_suffix(&badge);
    vault_group.add(&status_row);
    col.append(&vault_group);

    wrap_scrollable_section(&col)
}

// ============================================================================
// 7. Atajos de teclado
// ============================================================================
fn build_shortcuts_section(index: &mut Vec<SearchEntryItem>) -> ScrolledWindow {
    let col = section_container(
        "Atajos de teclado",
        "Referencia completa de atajos rápidos con Ctrl (también accesible con Ctrl+?).",
    );

    let groups: &[(&str, &[(&str, &str)])] = &[
        (
            "Pestañas y ventanas",
            &[
                ("Nueva pestaña", "Ctrl+T"),
                ("Cerrar pestaña (protege fijadas)", "Ctrl+W"),
                ("Reabrir última pestaña cerrada", "Ctrl+Shift+T"),
                ("Selector difuso de pestañas", "Ctrl+K"),
                ("Pestaña siguiente / anterior", "Ctrl+Tab / Ctrl+Shift+Tab"),
                ("Ir a pestaña 1…8 / última", "Ctrl+1…8 / Ctrl+9"),
                ("Duplicar pestaña", "Ctrl+Alt+D"),
                ("Silenciar o activar sonido", "Ctrl+Shift+M"),
                ("Alternar pestañas arriba / barra lateral", "Ctrl+Shift+S"),
                ("Plegar o mostrar barra de pestañas", "Ctrl+S"),
                ("Nueva ventana / ventana privada", "Ctrl+N / Ctrl+Shift+N"),
            ],
        ),
        (
            "Navegación",
            &[
                ("Enfocar barra de direcciones", "Ctrl+L · F6 · Alt+D"),
                ("Atrás / Adelante", "Alt+← / Alt+→"),
                ("Recargar / Recargar sin caché", "Ctrl+R / Ctrl+Shift+R"),
                ("Detener carga o cerrar paneles", "Escape"),
                ("Copiar URL limpia", "Ctrl+Shift+C"),
                ("Pegar e ir", "Ctrl+Shift+V"),
            ],
        ),
        (
            "Historial, marcadores y descargas",
            &[
                ("Panel de historial", "Ctrl+H · Ctrl+Y"),
                ("Añadir o editar marcador", "Ctrl+D · Ctrl+Shift+B"),
                ("Panel de marcadores", "Ctrl+Shift+O"),
                ("Panel de descargas", "Ctrl+Shift+Y · Ctrl+Shift+J"),
            ],
        ),
        (
            "Página y aplicación",
            &[
                ("Buscar en la página", "Ctrl+F"),
                ("Coincidencia siguiente / anterior", "Ctrl+G / Ctrl+Shift+G"),
                (
                    "Acercar / Alejar / Restablecer zoom",
                    "Ctrl++ / Ctrl+- / Ctrl+0",
                ),
                ("Pantalla completa", "F11"),
                ("Inspector web (DevTools)", "Ctrl+Shift+I · F12"),
                ("Abrir ajustes (dot://settings)", "Ctrl+,"),
                ("Diálogo de atajos", "Ctrl+?"),
                ("Salir de Dot", "Ctrl+Q"),
            ],
        ),
    ];

    for &(group_title, items) in groups {
        let grp = PreferencesGroup::new();
        grp.set_title(group_title);
        for &(label, keys) in items {
            let row = ActionRow::new();
            row.set_title(label);
            let kbd = Label::new(Some(keys));
            kbd.add_css_class("search-keycap");
            kbd.set_valign(Align::Center);
            row.add_suffix(&kbd);
            grp.add(&row);
            push_index(
                index,
                "shortcuts",
                "Atajos de teclado",
                label,
                keys,
                &format!("shortcut atajo teclado {keys}"),
            );
        }
        col.append(&grp);
    }

    wrap_scrollable_section(&col)
}

// ============================================================================
// 8. Acerca de Dot
// ============================================================================
fn build_about_section(
    paths: &DotPaths,
    settings: &SettingsStore,
    snap: &dot_core::settings::AppSettings,
    dark: bool,
    index: &mut Vec<SearchEntryItem>,
    register: &mut impl FnMut(&Image, IconId),
) -> ScrolledWindow {
    let col = section_container(
        "Acerca de Dot",
        "Navegador web minimalista, rápido y privado para Linux escrito en Rust.",
    );

    let info_group = PreferencesGroup::new();
    info_group.set_title("Información de versión");

    let app_row = ActionRow::new();
    app_row.set_title("Dot");
    app_row
        .set_subtitle("Based on Search by Office Commun (MIT) · Copyright (c) 2026 Office Commun");
    let ver_badge = Label::new(Some(&format!("v{}", env!("CARGO_PKG_VERSION"))));
    ver_badge.add_css_class("search-switcher-badge");
    ver_badge.set_valign(Align::Center);
    app_row.add_suffix(&ver_badge);
    info_group.add(&app_row);

    let cef_row = ActionRow::new();
    cef_row.set_title("Motor web");
    cef_row.set_subtitle("CEF 156.0.30 · Chromium 156.0.8078.96 (modo Alloy + OSR Windowless)");
    info_group.add(&cef_row);

    let (gmaj, gmin, gmic) = gnome_platform::gtk::version();
    let (amaj, amin, amic) = gnome_platform::adw::version();
    let toolkit_row = ActionRow::new();
    toolkit_row.set_title("Interfaz nativa");
    toolkit_row.set_subtitle(&format!(
        "GTK {gmaj}.{gmin}.{gmic} · libadwaita {amaj}.{amin}.{amic} · Rust 100 %"
    ));
    info_group.add(&toolkit_row);
    col.append(&info_group);

    // Actualizaciones firmadas (dot_core::updater)
    let updater_group = PreferencesGroup::new();
    updater_group.set_title("Actualizaciones");
    let in_flatpak = dot_core::updater::is_flatpak();
    let current_ver = dot_core::updater::Version::parse(env!("CARGO_PKG_VERSION"))
        .unwrap_or_else(|_| dot_core::updater::Version::new(0, 1, 0));

    if in_flatpak {
        let fp_row = SwitchRow::new();
        fp_row.set_title("Comprobar actualizaciones firmadas");
        fp_row.set_subtitle(
            "Desactivado automáticamente en Flatpak (/.flatpak-info): actualiza desde Flathub o tu tienda de software.",
        );
        fp_row.set_active(false);
        fp_row.set_sensitive(false);
        updater_group.add(&fp_row);
    } else {
        let upd_switch = SwitchRow::new();
        upd_switch.set_title("Comprobar actualizaciones firmadas (Ed25519)");
        upd_switch.set_subtitle(
            "Comprobación diaria de manifiesto firmado; nunca descarga ni instala sin tu confirmación.",
        );
        upd_switch.add_prefix(&make_prefix_icon(IconId::Refresh, dark, register));
        upd_switch.set_active(snap.update_check);
        {
            let st = settings.clone();
            let row_ref = upd_switch.clone();
            upd_switch.connect_active_notify(move || {
                let enabled = row_ref.is_active();
                st.update(|s| s.update_check = enabled);
            });
        }
        updater_group.add(&upd_switch);

        let upd_status_row = ActionRow::new();
        upd_status_row.set_title("Estado de actualización");
        let policy =
            dot_core::updater::UpdatePolicy::new(current_ver.clone(), 1, snap.update_check);
        let init_msg = if !policy.check_enabled {
            format!("v{current_ver} · Comprobación automática desactivada.")
        } else {
            format!("v{current_ver} · Verificación Ed25519 habilitada (sin descargas automáticas).")
        };
        upd_status_row.set_subtitle(&init_msg);

        let check_btn = Button::with_label("Comprobar ahora");
        check_btn.set_valign(Align::Center);
        {
            let st = settings.clone();
            let row_ref = upd_status_row.clone();
            let ver_clone = current_ver.clone();
            check_btn.connect_clicked(move || {
                let enabled = st.with(|s| s.update_check);
                let pol = dot_core::updater::UpdatePolicy::new(ver_clone.clone(), 1, enabled);
                if pol.is_flatpak() {
                    row_ref.set_subtitle("Desactivado en Flatpak: actualiza mediante Flathub.");
                } else if !pol.check_enabled {
                    row_ref.set_subtitle("Comprobación desactivada por el usuario.");
                } else {
                    row_ref.set_subtitle(&format!(
                        "Dot v{} · Verificación Ed25519 activa (sin manifiesto remoto configurado en modo local).",
                        pol.current_version
                    ));
                }
            });
        }
        upd_status_row.add_suffix(&check_btn);
        updater_group.add(&upd_status_row);
    }
    col.append(&updater_group);

    let paths_group = PreferencesGroup::new();
    paths_group.set_title("Rutas de datos XDG (permisos 0700)");
    for (title, path) in [
        (
            "Datos (sesión, historial, marcadores, carpetas)",
            &paths.data_dir,
        ),
        ("Configuración (settings.json)", &paths.config_dir),
        ("Caché (perfil CEF)", &paths.cache_dir),
        ("Runtime (sockets)", &paths.runtime_dir),
    ] {
        let row = ActionRow::new();
        row.set_title(title);
        row.set_subtitle(&path.to_string_lossy());
        paths_group.add(&row);
    }
    col.append(&paths_group);

    push_index(
        index,
        "about",
        "Acerca de Dot",
        "Dot · Versiones, actualizaciones y rutas XDG",
        "Based on Search by Office Commun (MIT), CEF 156, Chromium 156, GTK4, libadwaita, updater, Flatpak",
        "about acerca dot version licencia mit cef chromium gtk adwaita xdg rutas actualizaciones updater flatpak ed25519",
    );

    wrap_scrollable_section(&col)
}
