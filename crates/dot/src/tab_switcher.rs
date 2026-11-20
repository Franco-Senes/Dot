//! Paleta de comandos y selector difuso de pestañas abiertas (`Ctrl+K`) sobre **todas**
//! las ventanas (`TabSwitcher.swift` + Command Palette).
//!
//! Busca pestañas abiertas, páginas internas `plasma://` y más de 25 comandos del navegador
//! (con prefijo `>` para filtrar solo comandos, o en modo mixto), mostrando chips `<kbd>`
//! con atajos, navegación por teclado (`↑`/`↓`, `Enter`, `Esc`, `Ctrl+W`/`Supr`) e iconos vectoriales.

use crate::icons::{self, IconId};
use dot_core::address::sanitize_url_for_ui;
use dot_core::fuzzy::{fuzzy_match_tab, fuzzy_score};
use gnome_platform::adw::{ApplicationWindow, Dialog};
use gnome_platform::gdk::{self, ModifierType, Texture};
use gnome_platform::gtk::{
    Align, AsController, AsEditable, AsWidget, BoxWidget, EllipsizeMode, EventControllerKey, Image,
    Label, ListBox, ListBoxRow, Orientation, PolicyType, PropagationPhase, ScrolledWindow,
    SearchEntry, SelectionMode,
};
use gnome_platform::object::ObjectSubclass;
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

/// Página interna `plasma://` accesible desde la paleta de comandos.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct PalettePage {
    pub id: &'static str,
    pub title: &'static str,
    pub url: &'static str,
    pub subtitle: &'static str,
    pub icon: IconId,
    pub shortcut: Option<&'static str>,
}

/// Acción ejecutable del navegador disponible en la paleta de comandos (`>` o mixta).
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct PaletteCommand {
    pub id: &'static str,
    pub action: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub keywords: &'static str,
    pub shortcut: Option<&'static str>,
    pub icon: IconId,
}

/// Entrada unificada mostrada en la lista de resultados de `Ctrl+K`.
#[derive(Clone)]
pub enum PaletteEntry {
    Tab(SwitcherTabItem),
    PlasmaPage(PalettePage),
    Command(PaletteCommand),
}

pub type UrlOpenCallback = Box<dyn Fn(&str)>;
pub type CommandExecCallback = Box<dyn Fn(&str)>;

pub struct TabSwitcherCallbacks {
    pub list_tabs: Box<dyn Fn() -> Vec<SwitcherTabItem>>,
    pub on_activate: Box<dyn Fn(u64, u64)>,
    pub on_close_tab: Box<dyn Fn(u64, u64) -> bool>,
}

/// Callbacks extendidos para la paleta de comandos completa (pestañas + páginas `plasma://` + comandos).
pub struct CommandPaletteCallbacks {
    pub tabs: TabSwitcherCallbacks,
    pub on_open_url: Option<UrlOpenCallback>,
    pub on_execute_command: Option<CommandExecCallback>,
}

impl From<TabSwitcherCallbacks> for CommandPaletteCallbacks {
    fn from(tabs: TabSwitcherCallbacks) -> Self {
        Self {
            tabs,
            on_open_url: None,
            on_execute_command: None,
        }
    }
}

/// Catálogo de páginas internas `dot://` del navegador.
pub fn plasma_pages() -> Vec<PalettePage> {
    vec![
        PalettePage {
            id: "plasma-settings",
            title: "Ajustes",
            url: "dot://settings",
            subtitle: "Preferencias generales, apariencia, privacidad y motor",
            icon: IconId::Settings,
            shortcut: Some("Ctrl+,"),
        },
        PalettePage {
            id: "plasma-history",
            title: "Historial",
            url: "dot://history",
            subtitle: "Páginas visitadas recientemente",
            icon: IconId::Clock,
            shortcut: Some("Ctrl+H"),
        },
        PalettePage {
            id: "plasma-bookmarks",
            title: "Marcadores",
            url: "dot://bookmarks",
            subtitle: "Sitios guardados y carpetas",
            icon: IconId::BookmarkFilled,
            shortcut: Some("Ctrl+Shift+O"),
        },
        PalettePage {
            id: "plasma-downloads",
            title: "Descargas",
            url: "dot://downloads",
            subtitle: "Archivos descargados y transferencias activas",
            icon: IconId::Download,
            shortcut: Some("Ctrl+Shift+Y"),
        },
        PalettePage {
            id: "plasma-passwords",
            title: "Contraseñas (Vault)",
            url: "dot://passwords",
            subtitle: "Gestor de credenciales guardadas",
            icon: IconId::Key,
            shortcut: Some("Ctrl+Alt+L"),
        },
        PalettePage {
            id: "plasma-shortcuts",
            title: "Atajos de teclado",
            url: "dot://shortcuts",
            subtitle: "Referencia rápida de combinaciones de teclas",
            icon: IconId::Keyboard,
            shortcut: Some("Ctrl+?"),
        },
        PalettePage {
            id: "plasma-newtab",
            title: "Nueva pestaña",
            url: "dot://newtab",
            subtitle: "Página de inicio y accesos rápidos",
            icon: IconId::Home,
            shortcut: Some("Ctrl+T"),
        },
        PalettePage {
            id: "plasma-about",
            title: "Acerca de Dot",
            url: "dot://about",
            subtitle: "Versión del navegador, motor CEF y diagnósticos",
            icon: IconId::Info,
            shortcut: None,
        },
    ]
}

/// Catálogo de 30+ comandos del navegador disponibles en la paleta (`>` o mixto).
pub fn default_browser_commands() -> Vec<PaletteCommand> {
    vec![
        PaletteCommand {
            id: "cmd-new-tab",
            action: "win.new-tab",
            title: "Nueva pestaña",
            category: "Pestañas",
            keywords: "new tab abrir crear pestaña",
            shortcut: Some("Ctrl+T"),
            icon: IconId::Plus,
        },
        PaletteCommand {
            id: "cmd-close-tab",
            action: "win.close-tab",
            title: "Cerrar pestaña actual",
            category: "Pestañas",
            keywords: "close tab cerrar pestaña",
            shortcut: Some("Ctrl+W"),
            icon: IconId::Close,
        },
        PaletteCommand {
            id: "cmd-reopen-tab",
            action: "win.reopen-tab",
            title: "Reabrir pestaña cerrada",
            category: "Pestañas",
            keywords: "reopen restore tab restaurar deshacer",
            shortcut: Some("Ctrl+Shift+T"),
            icon: IconId::RotateCcw,
        },
        PaletteCommand {
            id: "cmd-duplicate-tab",
            action: "win.duplicate-tab",
            title: "Duplicar pestaña actual",
            category: "Pestañas",
            keywords: "duplicate clone tab clonar",
            shortcut: Some("Ctrl+Alt+D"),
            icon: IconId::Copy,
        },
        PaletteCommand {
            id: "cmd-next-tab",
            action: "win.next-tab",
            title: "Pestaña siguiente",
            category: "Pestañas",
            keywords: "next tab avanzar",
            shortcut: Some("Ctrl+Tab"),
            icon: IconId::Forward,
        },
        PaletteCommand {
            id: "cmd-prev-tab",
            action: "win.prev-tab",
            title: "Pestaña anterior",
            category: "Pestañas",
            keywords: "previous tab retroceder",
            shortcut: Some("Ctrl+Shift+Tab"),
            icon: IconId::Back,
        },
        PaletteCommand {
            id: "cmd-toggle-mute",
            action: "win.toggle-mute",
            title: "Silenciar o reactivar audio de pestaña",
            category: "Pestañas",
            keywords: "mute unmute audio sonido",
            shortcut: Some("Ctrl+Shift+M"),
            icon: IconId::VolumeMuted,
        },
        PaletteCommand {
            id: "cmd-new-window",
            action: "win.new-window",
            title: "Nueva ventana",
            category: "Ventanas",
            keywords: "new window ventana",
            shortcut: Some("Ctrl+N"),
            icon: IconId::Layers,
        },
        PaletteCommand {
            id: "cmd-new-private-window",
            action: "win.new-private-window",
            title: "Nueva ventana privada",
            category: "Privacidad",
            keywords: "incognito private window privada",
            shortcut: Some("Ctrl+Shift+N"),
            icon: IconId::Shield,
        },
        PaletteCommand {
            id: "cmd-focus-address",
            action: "win.focus-address",
            title: "Enfocar barra de direcciones",
            category: "Navegación",
            keywords: "omnibox url address bar direcciones",
            shortcut: Some("Ctrl+L"),
            icon: IconId::Search,
        },
        PaletteCommand {
            id: "cmd-reload",
            action: "win.reload",
            title: "Recargar página",
            category: "Navegación",
            keywords: "reload refresh actualizar",
            shortcut: Some("Ctrl+R"),
            icon: IconId::Refresh,
        },
        PaletteCommand {
            id: "cmd-reload-hard",
            action: "win.reload-ignore-cache",
            title: "Recargar ignorando caché",
            category: "Navegación",
            keywords: "hard reload cache limpiar recargar",
            shortcut: Some("Ctrl+Shift+R"),
            icon: IconId::RotateCcw,
        },
        PaletteCommand {
            id: "cmd-go-back",
            action: "win.go-back",
            title: "Ir atrás en el historial",
            category: "Navegación",
            keywords: "back atras volver",
            shortcut: Some("Alt+Left"),
            icon: IconId::Back,
        },
        PaletteCommand {
            id: "cmd-go-forward",
            action: "win.go-forward",
            title: "Ir adelante en el historial",
            category: "Navegación",
            keywords: "forward adelante avanzar",
            shortcut: Some("Alt+Right"),
            icon: IconId::Forward,
        },
        PaletteCommand {
            id: "cmd-find",
            action: "win.find",
            title: "Buscar en la página",
            category: "Página",
            keywords: "find search texto buscar",
            shortcut: Some("Ctrl+F"),
            icon: IconId::Search,
        },
        PaletteCommand {
            id: "cmd-zoom-in",
            action: "win.zoom-in",
            title: "Acercar zoom",
            category: "Vista",
            keywords: "zoom in ampliar aumentar",
            shortcut: Some("Ctrl++"),
            icon: IconId::ZoomIn,
        },
        PaletteCommand {
            id: "cmd-zoom-out",
            action: "win.zoom-out",
            title: "Alejar zoom",
            category: "Vista",
            keywords: "zoom out reducir disminuir",
            shortcut: Some("Ctrl+-"),
            icon: IconId::ZoomOut,
        },
        PaletteCommand {
            id: "cmd-zoom-reset",
            action: "win.zoom-reset",
            title: "Restablecer zoom al 100%",
            category: "Vista",
            keywords: "zoom reset 100 normal",
            shortcut: Some("Ctrl+0"),
            icon: IconId::Sliders,
        },
        PaletteCommand {
            id: "cmd-copy-clean-url",
            action: "win.copy-clean-url",
            title: "Copiar URL limpia",
            category: "Compartir",
            keywords: "copy url link enlace limpio",
            shortcut: Some("Ctrl+Shift+C"),
            icon: IconId::Copy,
        },
        PaletteCommand {
            id: "cmd-paste-and-go",
            action: "win.paste-and-go",
            title: "Pegar dirección e ir",
            category: "Navegación",
            keywords: "paste go portapapeles",
            shortcut: Some("Ctrl+Shift+V"),
            icon: IconId::ExternalLink,
        },
        PaletteCommand {
            id: "cmd-bookmark-page",
            action: "win.bookmark-page",
            title: "Guardar página en marcadores",
            category: "Marcadores",
            keywords: "bookmark star favorito marcador",
            shortcut: Some("Ctrl+D"),
            icon: IconId::BookmarkOutline,
        },
        PaletteCommand {
            id: "cmd-toggle-bookmarks",
            action: "win.toggle-bookmarks",
            title: "Mostrar u ocultar panel de marcadores",
            category: "Paneles",
            keywords: "bookmarks panel sidebar",
            shortcut: Some("Ctrl+Shift+O"),
            icon: IconId::BookmarkFilled,
        },
        PaletteCommand {
            id: "cmd-toggle-history",
            action: "win.toggle-history",
            title: "Mostrar u ocultar historial",
            category: "Paneles",
            keywords: "history panel historial",
            shortcut: Some("Ctrl+H"),
            icon: IconId::Clock,
        },
        PaletteCommand {
            id: "cmd-toggle-downloads",
            action: "win.toggle-downloads",
            title: "Mostrar u ocultar descargas",
            category: "Paneles",
            keywords: "downloads descargas panel",
            shortcut: Some("Ctrl+Shift+Y"),
            icon: IconId::Download,
        },
        PaletteCommand {
            id: "cmd-open-passwords",
            action: "win.open-passwords",
            title: "Abrir gestor de contraseñas",
            category: "Privacidad",
            keywords: "passwords vault credenciales",
            shortcut: Some("Ctrl+Alt+L"),
            icon: IconId::Key,
        },
        PaletteCommand {
            id: "cmd-toggle-reader",
            action: "win.toggle-reader",
            title: "Alternar modo lectura",
            category: "Vista",
            keywords: "reader mode lectura articulo",
            shortcut: Some("F9"),
            icon: IconId::BookOpen,
        },
        PaletteCommand {
            id: "cmd-toggle-curtain",
            action: "win.toggle-curtain",
            title: "Alternar filtro de elementos (Curtain)",
            category: "Privacidad",
            keywords: "curtain shield adblock ocultar elementos",
            shortcut: Some("Ctrl+Shift+E"),
            icon: IconId::EyeOff,
        },
        PaletteCommand {
            id: "cmd-toggle-tab-layout",
            action: "win.toggle-tab-layout",
            title: "Cambiar entre barra lateral y pestañas arriba",
            category: "Diseño",
            keywords: "sidebar horizontal vertical layout tabs",
            shortcut: Some("Ctrl+Shift+S"),
            icon: IconId::LayoutTabs,
        },
        PaletteCommand {
            id: "cmd-toggle-fold-tabs",
            action: "win.toggle-fold-tabs",
            title: "Plegar o desplegar barra lateral",
            category: "Diseño",
            keywords: "fold collapse sidebar plegar",
            shortcut: Some("Ctrl+S"),
            icon: IconId::Spaces,
        },
        PaletteCommand {
            id: "cmd-split-tab",
            action: "win.split-tab",
            title: "Dividir pestaña en vista paralela",
            category: "Diseño",
            keywords: "split view dividir pantalla",
            shortcut: Some("Ctrl+Alt+N"),
            icon: IconId::SplitHorizontal,
        },
        PaletteCommand {
            id: "cmd-toggle-pip",
            action: "win.toggle-pip",
            title: "Alternar Picture-in-Picture",
            category: "Vista",
            keywords: "pip video flotante",
            shortcut: Some("Ctrl+Shift+P"),
            icon: IconId::Pip,
        },
        PaletteCommand {
            id: "cmd-toggle-fullscreen",
            action: "win.toggle-fullscreen",
            title: "Alternar pantalla completa",
            category: "Vista",
            keywords: "fullscreen pantalla completa",
            shortcut: Some("F11"),
            icon: IconId::ExternalLink,
        },
        PaletteCommand {
            id: "cmd-dev-tools",
            action: "win.dev-tools",
            title: "Abrir herramientas de desarrollo",
            category: "Desarrollador",
            keywords: "devtools inspector console codigo",
            shortcut: Some("F12"),
            icon: IconId::Code,
        },
        PaletteCommand {
            id: "cmd-open-settings",
            action: "win.open-settings",
            title: "Abrir ajustes del navegador",
            category: "Sistema",
            keywords: "settings preferences configuracion ajustes",
            shortcut: Some("Ctrl+,"),
            icon: IconId::Settings,
        },
        PaletteCommand {
            id: "cmd-open-shortcuts",
            action: "win.open-shortcuts",
            title: "Ver todos los atajos de teclado",
            category: "Sistema",
            keywords: "shortcuts keybindings ayuda atajos",
            shortcut: Some("Ctrl+?"),
            icon: IconId::Keyboard,
        },
        PaletteCommand {
            id: "cmd-quit",
            action: "app.quit",
            title: "Salir de Dot",
            category: "Aplicación",
            keywords: "quit exit cerrar salir",
            shortcut: Some("Ctrl+Q"),
            icon: IconId::Ban,
        },
    ]
}

/// Filtra y ordena pestañas, páginas `plasma://` y comandos del navegador según `raw_query`.
///
/// - Con prefijo `>` filtra exclusivamente comandos.
/// - Sin prefijo combina pestañas abiertas, páginas `plasma://` y comandos.
pub fn filter_palette_entries(
    raw_query: &str,
    tabs: Vec<SwitcherTabItem>,
    pages: &[PalettePage],
    commands: &[PaletteCommand],
) -> Vec<PaletteEntry> {
    let trimmed = raw_query.trim();
    let command_only = trimmed.starts_with('>');
    let query = if command_only {
        trimmed.trim_start_matches('>').trim()
    } else {
        trimmed
    };

    if command_only {
        let mut scored_cmds: Vec<(i64, usize, PaletteEntry)> = commands
            .iter()
            .enumerate()
            .filter_map(|(ord, cmd)| {
                let candidate = format!("{} {} {}", cmd.title, cmd.category, cmd.keywords);
                let score = fuzzy_score(query, &candidate)?;
                Some((score, ord, PaletteEntry::Command(cmd.clone())))
            })
            .collect();
        if !query.is_empty() {
            scored_cmds.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        }
        return scored_cmds.into_iter().map(|(_, _, e)| e).collect();
    }

    if query.is_empty() {
        let mut out: Vec<PaletteEntry> = tabs.into_iter().map(PaletteEntry::Tab).collect();
        out.extend(pages.iter().cloned().map(PaletteEntry::PlasmaPage));
        out.extend(commands.iter().cloned().map(PaletteEntry::Command));
        return out;
    }

    let mut scored: Vec<(i64, u8, usize, PaletteEntry)> = Vec::new();

    for (ord, tab) in tabs.into_iter().enumerate() {
        if let Some(score) = fuzzy_match_tab(query, &tab.title, &tab.url) {
            // Priorizar ligeramente las pestañas abiertas en modo mixto.
            scored.push((score + 18, 0, ord, PaletteEntry::Tab(tab)));
        }
    }

    for (ord, page) in pages.iter().enumerate() {
        let candidate = format!("{} {} {}", page.title, page.url, page.subtitle);
        if let Some(score) = fuzzy_score(query, &candidate) {
            scored.push((score + 8, 1, ord, PaletteEntry::PlasmaPage(page.clone())));
        }
    }

    for (ord, cmd) in commands.iter().enumerate() {
        let candidate = format!("{} {} {}", cmd.title, cmd.category, cmd.keywords);
        if let Some(score) = fuzzy_score(query, &candidate) {
            scored.push((score, 2, ord, PaletteEntry::Command(cmd.clone())));
        }
    }

    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
    });
    scored.into_iter().map(|(_, _, _, entry)| entry).collect()
}

/// Abre el diálogo centrado `Ctrl+K` (paleta de comandos + selector de pestañas) sobre `parent`.
pub fn present_tab_switcher(parent: &ApplicationWindow, callbacks: TabSwitcherCallbacks) {
    present_command_palette(parent, CommandPaletteCallbacks::from(callbacks));
}

/// Abre la paleta de comandos unificada `Ctrl+K` con soporte de URLs `plasma://` y comandos.
pub fn present_command_palette(parent: &ApplicationWindow, callbacks: CommandPaletteCallbacks) {
    let (dialog, search) = build_command_palette_dialog(callbacks);
    dialog.present(Some(parent));
    search.grab_focus();
}

fn build_command_palette_dialog(callbacks: CommandPaletteCallbacks) -> (Dialog, SearchEntry) {
    let dialog = Dialog::new();
    dialog.set_title("Paleta de comandos y pestañas");
    dialog.set_content_width(620);
    dialog.set_content_height(460);
    dialog.set_follows_content_size(false);

    let root = BoxWidget::new(Orientation::Vertical, 10);
    root.add_css_class("search-switcher-root");

    let search = SearchEntry::new();
    search.set_placeholder_text("Buscar pestañas, páginas dot:// o > comandos…");
    search.set_hexpand(true);

    let list_box = ListBox::new();
    list_box.add_css_class("search-switcher-list");
    list_box.set_selection_mode(SelectionMode::Single);
    list_box.set_activate_on_single_click(true);

    let scrolled = ScrolledWindow::new();
    scrolled.set_policy(PolicyType::Never, PolicyType::Automatic);
    scrolled.set_vexpand(true);
    scrolled.set_child(Some(&list_box));

    let hint_bar = build_footer_hints();

    root.append(&search);
    root.append(&scrolled);
    root.append(&hint_bar);

    dialog.set_child(Some(&root));
    dialog.set_focus_widget(Some(&search));

    let cbs = Rc::new(callbacks);
    let pages = Rc::new(plasma_pages());
    let commands = Rc::new(default_browser_commands());
    let filtered: Rc<RefCell<Vec<PaletteEntry>>> = Rc::new(RefCell::new(Vec::new()));
    let selected_idx: Rc<Cell<usize>> = Rc::new(Cell::new(0));

    let refresh = {
        let cbs_ref = cbs.clone();
        let pages_ref = pages.clone();
        let cmds_ref = commands.clone();
        let list_weak = list_box.downgrade();
        let filt_ref = filtered.clone();
        let sel_ref = selected_idx.clone();
        let search_weak = search.downgrade();
        Rc::new(move |preferred_index: Option<usize>| {
            let (Some(list_ref), Some(search_ref)) = (list_weak.upgrade(), search_weak.upgrade())
            else {
                return;
            };
            let query = search_ref.text();
            let all_tabs = (cbs_ref.tabs.list_tabs)();
            let entries = filter_palette_entries(&query, all_tabs, &pages_ref, &cmds_ref);

            list_ref.remove_all();

            if entries.is_empty() {
                list_ref.append(&build_empty_row(&query));
                filt_ref.borrow_mut().clear();
                sel_ref.set(0);
                return;
            }

            for entry in &entries {
                list_ref.append(&build_palette_row(entry));
            }

            let count = entries.len();
            *filt_ref.borrow_mut() = entries;

            let target = preferred_index.unwrap_or(0).min(count - 1);
            sel_ref.set(target);
            if let Some(row) = list_ref.row_at_index(target as i32) {
                list_ref.select_row(Some(&row));
            }
        })
    };

    refresh(None);

    let activate_entry = {
        let cbs_act = cbs.clone();
        move |entry: PaletteEntry| match entry {
            PaletteEntry::Tab(tab) => {
                (cbs_act.tabs.on_activate)(tab.window_id, tab.tab_id);
            }
            PaletteEntry::PlasmaPage(page) => {
                // Si ya hay una pestaña abierta con esa URL, cambiar a ella; si no, abrir URL o comando.
                let existing = (cbs_act.tabs.list_tabs)()
                    .into_iter()
                    .find(|t| t.url == page.url);
                if let Some(tab) = existing {
                    (cbs_act.tabs.on_activate)(tab.window_id, tab.tab_id);
                } else if let Some(ref open_url) = cbs_act.on_open_url {
                    open_url(page.url);
                }
            }
            PaletteEntry::Command(cmd) => {
                if let Some(ref exec_cmd) = cbs_act.on_execute_command {
                    exec_cmd(cmd.action);
                }
            }
        }
    };

    // Filtrado en vivo con cada pulsación.
    {
        let refresh_change = refresh.clone();
        search.connect_search_changed(move || {
            refresh_change(Some(0));
        });
    }

    // Enter desde el cuadro de búsqueda activa la fila seleccionada.
    {
        let dialog_weak = dialog.downgrade();
        let filt_act = filtered.clone();
        let sel_act = selected_idx.clone();
        let act_fn = activate_entry.clone();
        search.connect_activate(move || {
            let item = filt_act.borrow().get(sel_act.get()).cloned();
            if let Some(it) = item {
                if let Some(dialog_act) = dialog_weak.upgrade() {
                    dialog_act.force_close();
                }
                act_fn(it);
            }
        });
    }

    // Esc desde el cuadro de búsqueda cierra el selector.
    {
        let dialog_weak = dialog.downgrade();
        search.connect_stop_search(move || {
            if let Some(dialog_stop) = dialog_weak.upgrade() {
                dialog_stop.force_close();
            }
        });
    }

    // Clic sobre una fila activa esa entrada.
    {
        let dialog_weak = dialog.downgrade();
        let filt_row = filtered.clone();
        let act_fn = activate_entry.clone();
        list_box.connect_row_activated(move |row| {
            let idx = row.index();
            if idx >= 0 {
                let item = filt_row.borrow().get(idx as usize).cloned();
                if let Some(it) = item {
                    if let Some(dialog_row) = dialog_weak.upgrade() {
                        dialog_row.force_close();
                    }
                    act_fn(it);
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
        let dialog_weak = dialog.downgrade();
        let list_weak = list_box.downgrade();
        let filt_key = filtered.clone();
        let sel_key = selected_idx.clone();
        let cbs_key = cbs.clone();
        let refresh_key = refresh.clone();
        let act_fn = activate_entry;

        key.connect_key_pressed(move |keyval, _keycode, raw_state| {
            let mods = ModifierType::from_bits_truncate(raw_state);
            let ctrl = mods.contains(ModifierType::CONTROL_MASK);
            let lower = gdk::keyval_to_lower(keyval);
            let count = filt_key.borrow().len();

            if keyval == gdk::keys::ESCAPE {
                if let Some(dialog_key) = dialog_weak.upgrade() {
                    dialog_key.force_close();
                }
                return true;
            }

            if keyval == gdk::keys::DOWN || (ctrl && lower == 0x6e) {
                if count > 0 {
                    let next = (sel_key.get() + 1) % count;
                    sel_key.set(next);
                    if let Some(list_key) = list_weak.upgrade() {
                        if let Some(row) = list_key.row_at_index(next as i32) {
                            list_key.select_row(Some(&row));
                        }
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
                    if let Some(list_key) = list_weak.upgrade() {
                        if let Some(row) = list_key.row_at_index(prev as i32) {
                            list_key.select_row(Some(&row));
                        }
                    }
                }
                return true;
            }

            if keyval == gdk::keys::RETURN || keyval == gdk::keys::KP_ENTER {
                let item = filt_key.borrow().get(sel_key.get()).cloned();
                if let Some(it) = item {
                    if let Some(dialog_key) = dialog_weak.upgrade() {
                        dialog_key.force_close();
                    }
                    act_fn(it);
                }
                return true;
            }

            if (ctrl && lower == 0x77) || keyval == gdk::keys::DELETE {
                let cur = sel_key.get();
                let item = filt_key.borrow().get(cur).cloned();
                if let Some(PaletteEntry::Tab(it)) = item {
                    if !it.pinned && (cbs_key.tabs.on_close_tab)(it.window_id, it.tab_id) {
                        refresh_key(Some(cur));
                    }
                }
                return true;
            }

            false
        });
        root.add_controller(&key);
    }

    (dialog, search)
}

fn build_footer_hints() -> BoxWidget {
    let row = BoxWidget::new(Orientation::Horizontal, 12);
    row.set_halign(Align::Center);
    row.set_valign(Align::Center);

    append_hint_pair(&row, &["↑", "↓"], "Navegar");
    append_hint_pair(&row, &["Enter"], "Ejecutar");
    append_hint_pair(&row, &[">"], "Solo comandos");
    append_hint_pair(&row, &["Ctrl+W"], "Cerrar pestaña");
    append_hint_pair(&row, &["Esc"], "Salir");
    row
}

fn append_hint_pair(parent: &BoxWidget, keys: &[&str], label: &str) {
    let pair = BoxWidget::new(Orientation::Horizontal, 4);
    pair.set_valign(Align::Center);
    for k in keys {
        let chip = Label::new(Some(k));
        chip.add_css_class("kbd-chip");
        pair.append(&chip);
    }
    let desc = Label::new(Some(label));
    desc.add_css_class("search-switcher-url");
    pair.append(&desc);
    parent.append(&pair);
}

fn build_kbd_chips(shortcut: &str) -> BoxWidget {
    let kbd_box = BoxWidget::new(Orientation::Horizontal, 3);
    kbd_box.set_valign(Align::Center);
    for part in shortcut.split('+') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        let chip = Label::new(Some(trimmed));
        chip.add_css_class("kbd-chip");
        kbd_box.append(&chip);
    }
    kbd_box
}

fn build_empty_row(query: &str) -> ListBoxRow {
    let row = ListBoxRow::new();
    row.set_activatable(false);
    row.set_selectable(false);

    let box_w = BoxWidget::new(Orientation::Vertical, 6);
    box_w.add_css_class("empty-state");
    box_w.set_halign(Align::Center);
    box_w.set_valign(Align::Center);

    let icon = icons::image(IconId::Search, 24);
    let msg = if query.trim().is_empty() {
        "No hay pestañas ni comandos disponibles".to_string()
    } else {
        format!("Sin resultados para «{}»", query.trim())
    };
    let lbl = Label::new(Some(&msg));
    lbl.add_css_class("search-switcher-url");

    box_w.append(&icon);
    box_w.append(&lbl);
    row.set_child(Some(&box_w));
    row
}

fn build_palette_row(entry: &PaletteEntry) -> ListBoxRow {
    match entry {
        PaletteEntry::Tab(item) => build_switcher_row(item),
        PaletteEntry::PlasmaPage(page) => build_plasma_page_row(page),
        PaletteEntry::Command(cmd) => build_command_row(cmd),
    }
}

fn build_plasma_page_row(page: &PalettePage) -> ListBoxRow {
    let row = ListBoxRow::new();
    row.add_css_class("search-switcher-row");

    let hbox = BoxWidget::new(Orientation::Horizontal, 10);
    hbox.set_valign(Align::Center);

    let icon = icons::image(page.icon, 17);
    hbox.append(&icon);

    let text_col = BoxWidget::new(Orientation::Vertical, 1);
    text_col.set_hexpand(true);

    let title_lbl = Label::new(Some(page.title));
    title_lbl.set_ellipsize(EllipsizeMode::End);
    title_lbl.set_xalign(0.0);

    let sub_lbl = Label::new(Some(&format!("{}  ·  {}", page.url, page.subtitle)));
    sub_lbl.add_css_class("search-switcher-url");
    sub_lbl.set_ellipsize(EllipsizeMode::End);
    sub_lbl.set_xalign(0.0);

    text_col.append(&title_lbl);
    text_col.append(&sub_lbl);
    hbox.append(&text_col);

    let badge = Label::new(Some("Página"));
    badge.add_css_class("search-switcher-badge");
    hbox.append(&badge);

    if let Some(shortcut) = page.shortcut {
        hbox.append(&build_kbd_chips(shortcut));
    }

    row.set_child(Some(&hbox));
    row
}

fn build_command_row(cmd: &PaletteCommand) -> ListBoxRow {
    let row = ListBoxRow::new();
    row.add_css_class("search-switcher-row");

    let hbox = BoxWidget::new(Orientation::Horizontal, 10);
    hbox.set_valign(Align::Center);

    let icon = icons::image(cmd.icon, 17);
    hbox.append(&icon);

    let text_col = BoxWidget::new(Orientation::Vertical, 1);
    text_col.set_hexpand(true);

    let title_lbl = Label::new(Some(cmd.title));
    title_lbl.set_ellipsize(EllipsizeMode::End);
    title_lbl.set_xalign(0.0);

    let cat_lbl = Label::new(Some(cmd.category));
    cat_lbl.add_css_class("search-switcher-url");
    cat_lbl.set_ellipsize(EllipsizeMode::End);
    cat_lbl.set_xalign(0.0);

    text_col.append(&title_lbl);
    text_col.append(&cat_lbl);
    hbox.append(&text_col);

    let badge = Label::new(Some("Comando"));
    badge.add_css_class("search-switcher-badge");
    badge.add_css_class("command");
    hbox.append(&badge);

    if let Some(shortcut) = cmd.shortcut {
        hbox.append(&build_kbd_chips(shortcut));
    }

    row.set_child(Some(&hbox));
    row
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
    } else if item.url.starts_with("dot://settings") || item.url.starts_with("plasma://settings") {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tab(id: u64, title: &str, url: &str) -> SwitcherTabItem {
        SwitcherTabItem {
            window_id: 1,
            window_label: None,
            is_private: false,
            tab_id: id,
            title: title.to_string(),
            url: url.to_string(),
            pinned: false,
            active: false,
            audible: false,
            muted: false,
            favicon: None,
        }
    }

    #[test]
    fn command_catalog_has_over_25_commands() {
        let cmds = default_browser_commands();
        assert!(
            cmds.len() >= 25,
            "expected >= 25 commands, got {}",
            cmds.len()
        );
        let pages = plasma_pages();
        assert!(pages.len() >= 6);
    }

    #[test]
    fn command_prefix_filters_only_commands() {
        let tabs = vec![sample_tab(1, "Zoom Docs", "https://zoom.us")];
        let pages = plasma_pages();
        let cmds = default_browser_commands();
        let results = filter_palette_entries("> zoom", tabs, &pages, &cmds);
        assert!(!results.is_empty());
        assert!(results
            .iter()
            .all(|entry| matches!(entry, PaletteEntry::Command(_))));
    }

    #[test]
    fn mixed_mode_finds_tabs_pages_and_commands() {
        let tabs = vec![sample_tab(1, "Rust Lang", "https://www.rust-lang.org")];
        let pages = plasma_pages();
        let cmds = default_browser_commands();

        let tab_hits = filter_palette_entries("rust", tabs.clone(), &pages, &cmds);
        assert!(matches!(tab_hits.first(), Some(PaletteEntry::Tab(_))));

        let page_hits = filter_palette_entries("dot://settings", tabs, &pages, &cmds);
        assert!(page_hits
            .iter()
            .any(|e| matches!(e, PaletteEntry::PlasmaPage(p) if p.url == "dot://settings")));
    }

    #[test]
    fn tab_switcher_dialog_has_no_reference_cycle() {
        let _guard = crate::GTK_TEST_MUTEX
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if !gnome_platform::init() || gnome_platform::gdk::Display::default_display().is_none() {
            return;
        }
        let (dialog, _search) =
            build_command_palette_dialog(CommandPaletteCallbacks::from(TabSwitcherCallbacks {
                list_tabs: Box::new(Vec::new),
                on_activate: Box::new(|_, _| {}),
                on_close_tab: Box::new(|_, _| false),
            }));
        let weak = dialog.downgrade();
        assert!(weak.upgrade().is_some());
        drop(dialog);
        assert!(weak.upgrade().is_none());
    }
}
