//! Barra lateral izquierda vertical de pestañas estilo Arc (`Side.swift` + `PinFolders.swift` + `PinDrop.swift`).
//!
//! Estructura auténtica Arc / Search enlazada al mismo `AdwTabView` de la ventana:
//! 1. Cuadrícula superior de Favoritos (hasta 4 cuadrados redondeados por fila con favicon/monograma).
//! 2. Sección de filas fijadas y carpetas de pines plegables (hasta 3 niveles de anidación con
//!    animación `GtkRevealer` `SlideDown`, creación/renombrado inline, menú contextual y DnD).
//! 3. Separador sutil + lista desplazable de pestañas regulares.
//! 4. Pie discreto con `+ Nueva pestaña` y botón compacto de `Ajustes (plasma://settings)`.

use crate::icons::{self, IconId};
use gnome_platform::gdk::{self, ContentProvider, DragAction, Rectangle, Texture};
use gnome_platform::gtk::{
    Align, AsEditable, AsWidget, BoxWidget, Button, DragSource, DropTarget, EllipsizeMode, Entry,
    EventControllerMotion, GestureClick, Image, Label, ListBox, ListBoxRow, Orientation,
    PolicyType, Popover, PositionType, Revealer, RevealerTransitionType, ScrolledWindow,
    SearchEntry, SelectionMode, Separator, Spinner,
};
use search_core::address::clean_link;
use search_core::pin_folders::{PinFolder, PinFolderStore, MAX_PIN_FOLDER_DEPTH};
use search_core::session::PinStyle;
use search_core::settings::{AppSettings, TabGlyph};
use search_core::workspaces::WorkspaceStore;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::time::Duration;

const PIN_FOLDERS_DEBOUNCE_MS: u64 = 1000;

/// Identificador de un espacio de trabajo estilo Arc.
pub type WorkspaceId = String;

/// Paleta curada de colores de acento para espacios (`color_id`, etiqueta, `#RRGGBB`).
pub const SPACE_ACCENT_COLORS: &[(&str, &str, &str)] = &[
    ("blue", "Azul", "#3584e4"),
    ("purple", "Violeta", "#9141ac"),
    ("green", "Verde", "#2ec27e"),
    ("amber", "Ámbar", "#e5a50a"),
    ("rose", "Rosa", "#e01b24"),
    ("teal", "Turquesa", "#26a269"),
    ("slate", "Pizarra", "#5e5c64"),
];

/// Devuelve el color hexadecimal de acento para un `color_id` de espacio.
pub fn space_accent_hex(color_id: &str) -> &'static str {
    let normalized = color_id.trim().to_ascii_lowercase();
    for (id, _label, hex) in SPACE_ACCENT_COLORS {
        if *id == normalized {
            return hex;
        }
    }
    "#3584e4"
}

fn space_icon_id(icon_id: &str) -> IconId {
    match icon_id.trim().to_ascii_lowercase().as_str() {
        "home" => IconId::Home,
        "layers" => IconId::Layers,
        "code" => IconId::Code,
        "star" => IconId::Star,
        "globe" => IconId::Globe,
        "folder" => IconId::Folder,
        _ => IconId::Spaces,
    }
}

fn default_workspace_store() -> WorkspaceStore {
    let mut store = WorkspaceStore::new();
    let personal_id = store.create("Personal", "blue", "home").id.clone();
    store.create("Trabajo", "purple", "layers");
    store.create("Dev", "green", "code");
    let _ = store.switch(&personal_id);
    store
}

/// Extrae el dominio normalizado de una URL para agrupar/ordenar pestañas.
pub fn extract_domain(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return "local".to_string();
    }
    if let Some(rest) = trimmed.strip_prefix("plasma://") {
        let host = rest.split(['/', '?', '#']).next().unwrap_or("plasma");
        return format!("plasma://{host}");
    }
    let after_scheme = match trimmed.split_once("://") {
        Some((_scheme, rest)) => rest,
        None => trimmed,
    };
    let authority = after_scheme.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority
        .rsplit_once('@')
        .map(|(_, h)| h)
        .unwrap_or(authority);
    let host = if host_port.starts_with('[') {
        host_port
            .split(']')
            .next()
            .unwrap_or(host_port)
            .trim_start_matches('[')
    } else {
        host_port.split(':').next().unwrap_or(host_port)
    };
    let lower = host.trim().to_ascii_lowercase();
    let bare = lower.strip_prefix("www.").unwrap_or(&lower);
    if bare.is_empty() {
        "local".to_string()
    } else {
        bare.to_string()
    }
}

fn tab_belongs_to_workspace(
    item: &SidebarTabItem,
    active_workspace_id: Option<&str>,
    tab_workspaces: &HashMap<u64, WorkspaceId>,
) -> bool {
    let Some(active_id) = active_workspace_id else {
        return true;
    };
    match tab_workspaces.get(&item.id) {
        Some(ws_id) => ws_id == active_id,
        None => true,
    }
}

/// Ordena de forma estable las pestañas no fijadas del espacio activo por dominio.
pub fn tidy_items_by_domain(
    items: &mut [SidebarTabItem],
    active_workspace_id: Option<&str>,
    tab_workspaces: &HashMap<u64, WorkspaceId>,
) {
    let mut positions = Vec::new();
    let mut unpinned = Vec::new();
    for (idx, item) in items.iter().enumerate() {
        if !item.is_pinned() && tab_belongs_to_workspace(item, active_workspace_id, tab_workspaces)
        {
            positions.push(idx);
            unpinned.push((extract_domain(&item.url), idx, item.clone()));
        }
    }
    unpinned.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    for (slot, (_domain, _orig_idx, sorted_item)) in positions.into_iter().zip(unpinned) {
        items[slot] = sorted_item;
    }
}

/// Recopila los IDs de pestañas no fijadas y no activas del espacio activo para "Limpiar".
pub fn collect_clearable_tab_ids(
    items: &[SidebarTabItem],
    active_workspace_id: Option<&str>,
    tab_workspaces: &HashMap<u64, WorkspaceId>,
) -> Vec<u64> {
    items
        .iter()
        .filter(|item| {
            !item.is_pinned()
                && !item.selected
                && !item.split_hidden
                && tab_belongs_to_workspace(item, active_workspace_id, tab_workspaces)
        })
        .map(|item| item.id)
        .collect()
}

// ============================================================================
// Gestor reactivo compartido de carpetas de pines (`pin-folders.json`)
// ============================================================================

type Listener = Rc<dyn Fn()>;

struct PinFoldersManagerInner {
    file_path: PathBuf,
    store: PinFolderStore,
    save_debounce: Cell<Option<gnome_platform::glib::SourceId>>,
    next_listener_id: u64,
    listeners: Vec<(u64, Listener)>,
}

/// Almacén compartido de carpetas de pines estilo Arc con persistencia atómica y debounce (~1 s).
#[derive(Clone)]
pub struct PinFoldersManager {
    inner: Rc<RefCell<PinFoldersManagerInner>>,
}

impl PinFoldersManager {
    pub fn load(file_path: PathBuf) -> Self {
        let store = PinFolderStore::load(&file_path);
        Self {
            inner: Rc::new(RefCell::new(PinFoldersManagerInner {
                file_path,
                store,
                save_debounce: Cell::new(None),
                next_listener_id: 1,
                listeners: Vec::new(),
            })),
        }
    }

    pub fn snapshot(&self) -> Vec<PinFolder> {
        self.inner.borrow().store.folders.clone()
    }

    pub fn flattened(&self) -> Vec<(PinFolder, usize)> {
        self.inner.borrow().store.flattened()
    }

    pub fn contains(&self, id: &str) -> bool {
        self.inner.borrow().store.contains(id)
    }

    pub fn create_folder(&self, name: &str, parent_id: Option<&str>) -> Option<PinFolder> {
        let created = self
            .inner
            .borrow_mut()
            .store
            .create_folder(name, parent_id)?;
        self.schedule_save();
        self.notify_listeners();
        Some(created)
    }

    pub fn rename_folder(&self, id: &str, new_name: &str) -> bool {
        let changed = self.inner.borrow_mut().store.rename_folder(id, new_name);
        if changed {
            self.schedule_save();
            self.notify_listeners();
        }
        changed
    }

    /// Actualiza el estado plegado sin emitir notificación destructiva para permitir
    /// que la animación `GtkRevealer` termine suavemente en la barra lateral activa.
    pub fn set_collapsed_silent(&self, id: &str, collapsed: bool) {
        if self.inner.borrow_mut().store.set_collapsed(id, collapsed) {
            self.schedule_save();
        }
    }

    pub fn remove_folder(&self, id: &str) -> Option<Option<String>> {
        let promoted_parent = self.inner.borrow_mut().store.remove_folder(id)?;
        self.schedule_save();
        self.notify_listeners();
        Some(promoted_parent)
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> u64 {
        let mut inner = self.inner.borrow_mut();
        let id = inner.next_listener_id;
        inner.next_listener_id += 1;
        inner.listeners.push((id, Rc::new(listener)));
        id
    }

    pub fn unsubscribe(&self, id: u64) {
        self.inner
            .borrow_mut()
            .listeners
            .retain(|(lid, _)| *lid != id);
    }

    pub fn flush_now(&self) {
        let (path, store) = {
            let inner = self.inner.borrow();
            if let Some(prev) = inner.save_debounce.take() {
                prev.remove();
            }
            (inner.file_path.clone(), inner.store.clone())
        };
        if let Err(err) = store.save(&path) {
            search_core::warn!(
                "No se pudieron guardar las carpetas de pines en '{}': {err:#}",
                path.display()
            );
        }
    }

    fn schedule_save(&self) {
        {
            let inner = self.inner.borrow();
            if let Some(prev) = inner.save_debounce.take() {
                prev.remove();
            }
        }
        let mgr = self.clone();
        let src = gnome_platform::glib::timeout_add_local_once(
            Duration::from_millis(PIN_FOLDERS_DEBOUNCE_MS),
            move || {
                mgr.inner.borrow().save_debounce.set(None);
                mgr.flush_now();
            },
        );
        self.inner.borrow().save_debounce.set(Some(src));
    }

    fn notify_listeners(&self) {
        let listeners: Vec<Listener> = self
            .inner
            .borrow()
            .listeners
            .iter()
            .map(|(_, l)| l.clone())
            .collect();
        for cb in listeners {
            cb();
        }
    }
}

// ============================================================================
// Widget SidebarTabs estilo Arc
// ============================================================================

/// Instantánea ligera de una pestaña para renderizar la barra lateral vertical.
#[derive(Clone)]
pub struct SidebarTabItem {
    pub id: u64,
    pub url: String,
    pub title: String,
    pub tooltip: String,
    pub pin_label: Option<String>,
    pub pin_style: Option<PinStyle>,
    pub folder_id: Option<String>,
    pub selected: bool,
    pub loading: bool,
    pub audible: bool,
    pub muted: bool,
    pub favicon: Option<Texture>,
    pub split_partner_favicon: Option<Texture>,
    pub split_partner_title: Option<String>,
    pub split_partner_url: Option<String>,
    pub split_hidden: bool,
}

impl SidebarTabItem {
    pub fn is_pinned(&self) -> bool {
        self.pin_style.is_some()
    }
}

/// Opciones visuales en vivo para la barra lateral.
#[derive(Debug, Clone, Copy)]
pub struct SidebarRenderOptions {
    pub glyph: TabGlyph,
    pub roomy: bool,
    pub show_audio_indicators: bool,
    pub motion_ms: u32,
}

impl Default for SidebarRenderOptions {
    fn default() -> Self {
        Self {
            glyph: TabGlyph::Favicon,
            roomy: false,
            show_audio_indicators: true,
            motion_ms: 180,
        }
    }
}

type ContextMenuCallback = Box<dyn Fn(u64, BoxWidget, f64, f64)>;
type CreateFolderCallback = Box<dyn Fn(String, Option<String>, Option<u64>)>;

#[derive(Default)]
pub struct SidebarCallbacks {
    pub on_select: Option<Box<dyn Fn(u64)>>,
    pub on_close: Option<Box<dyn Fn(u64)>>,
    pub on_toggle_mute: Option<Box<dyn Fn(u64)>>,
    #[allow(dead_code)]
    pub on_duplicate_tab: Option<Box<dyn Fn(u64)>>,
    #[allow(dead_code)]
    pub on_copy_clean_url: Option<Box<dyn Fn(u64, String)>>,
    #[allow(dead_code)]
    pub on_move_to_new_window: Option<Box<dyn Fn(u64)>>,
    #[allow(dead_code)]
    pub on_close_tabs_below: Option<Box<dyn Fn(u64)>>,
    #[allow(dead_code)]
    pub on_close_other_tabs: Option<Box<dyn Fn(u64)>>,
    #[allow(dead_code)]
    pub on_reopen_closed_tab: Option<Box<dyn Fn()>>,
    pub on_context_menu: Option<ContextMenuCallback>,
    pub on_reorder: Option<Box<dyn Fn(u64, u64)>>,
    pub on_drop_tab_into_folder: Option<Box<dyn Fn(u64, String)>>,
    pub on_new_tab: Option<Box<dyn Fn()>>,
    pub on_open_settings: Option<Box<dyn Fn()>>,
    pub on_create_folder: Option<CreateFolderCallback>,
    pub on_rename_folder: Option<Box<dyn Fn(String, String)>>,
    pub on_delete_folder: Option<Box<dyn Fn(String)>>,
    pub on_toggle_folder_collapsed: Option<Box<dyn Fn(String, bool)>>,
    pub on_workspace_changed: Option<Box<dyn Fn(WorkspaceId, String)>>,
    pub on_clear_today: Option<Box<dyn Fn(Vec<u64>)>>,
    pub on_tidy_tabs: Option<Box<dyn Fn()>>,
}

#[derive(Clone)]
enum FolderEditorMode {
    Create {
        parent_id: Option<String>,
        move_tab_id: Option<u64>,
    },
    Rename {
        folder_id: String,
    },
}

#[derive(Clone)]
pub struct SidebarTabs {
    inner: Rc<SidebarTabsInner>,
}

struct SidebarTabsInner {
    weak: Weak<SidebarTabsInner>,
    root: BoxWidget,
    filter_entry: SearchEntry,
    filter_empty_box: BoxWidget,
    favorites_box: BoxWidget,
    pinned_header: BoxWidget,
    folder_editor_box: BoxWidget,
    folder_editor_entry: Entry,
    folder_editor_mode: Rc<RefCell<Option<FolderEditorMode>>>,
    pinned_tree_box: BoxWidget,
    pinned_sep: Separator,
    today_header: BoxWidget,
    tidy_btn: Button,
    tidy_icon: Image,
    clear_today_btn: Button,
    clear_today_icon: Image,
    list_box: ListBox,
    spaces_bar: BoxWidget,
    spaces_pills_box: BoxWidget,
    add_space_icon: Image,
    new_tab_icon: Image,
    settings_icon: Image,
    add_folder_icon: Image,
    folder_popover: Rc<RefCell<Option<Popover>>>,
    space_popover: Rc<RefCell<Option<Popover>>>,
    current_width: Rc<Cell<i32>>,
    motion_ms: Rc<Cell<u32>>,
    last_items: Rc<RefCell<Vec<SidebarTabItem>>>,
    last_folders: Rc<RefCell<Vec<PinFolder>>>,
    last_opts: Rc<Cell<SidebarRenderOptions>>,
    workspace_store: Rc<RefCell<WorkspaceStore>>,
    tab_workspaces: Rc<RefCell<HashMap<u64, WorkspaceId>>>,
    callbacks: Rc<RefCell<SidebarCallbacks>>,
}

impl SidebarTabs {
    pub fn new(width: i32) -> Self {
        Self {
            inner: SidebarTabsInner::new(width),
        }
    }

    pub fn widget(&self) -> &BoxWidget {
        &self.inner.root
    }

    pub fn set_callbacks(&self, cbs: SidebarCallbacks) {
        self.inner.set_callbacks(cbs);
    }

    pub fn set_width(&self, width: i32) {
        self.inner.set_width(width);
    }

    pub fn width(&self) -> i32 {
        self.inner.width()
    }

    pub fn set_motion_duration(&self, ms: u32) {
        self.inner.set_motion_duration(ms);
    }

    pub fn set_roomy(&self, roomy: bool) {
        self.inner.set_roomy(roomy);
    }

    pub fn refresh_theme_icons(&self, dark: bool) {
        self.inner.refresh_theme_icons(dark);
    }

    pub fn cleanup(&self) {
        self.inner.cleanup();
    }

    #[allow(dead_code)]
    pub fn set_workspace_store(&self, store: WorkspaceStore) {
        self.inner.set_workspace_store(store);
    }

    #[allow(dead_code)]
    pub fn workspace_store(&self) -> WorkspaceStore {
        self.inner.workspace_store()
    }

    #[allow(dead_code)]
    pub fn active_workspace_id(&self) -> Option<WorkspaceId> {
        self.inner.active_workspace_id()
    }

    #[allow(dead_code)]
    pub fn set_tab_workspace(&self, tab_id: u64, workspace_id: &str) {
        self.inner.set_tab_workspace(tab_id, workspace_id);
    }

    #[allow(dead_code)]
    pub fn switch_workspace(&self, workspace_id: &str) -> bool {
        self.inner.switch_workspace(workspace_id)
    }

    pub fn begin_create_folder(&self, parent_id: Option<String>, move_tab_id: Option<u64>) {
        self.inner.begin_create_folder(parent_id, move_tab_id);
    }

    #[allow(dead_code)]
    pub fn begin_rename_folder(&self, folder_id: String, current_name: &str) {
        self.inner.begin_rename_folder(folder_id, current_name);
    }

    pub fn sync(
        &self,
        items: &[SidebarTabItem],
        folders: &[PinFolder],
        opts: SidebarRenderOptions,
    ) {
        self.inner.sync(items, folders, opts);
    }

    #[allow(clippy::too_many_arguments, dead_code)]
    pub fn open_tab_context_menu(
        &self,
        tab_id: u64,
        tab_url: &str,
        is_muted: bool,
        is_pinned: bool,
        anchor: &BoxWidget,
        x: f64,
        y: f64,
    ) {
        self.inner
            .open_tab_context_menu(tab_id, tab_url, is_muted, is_pinned, anchor, x, y);
    }
}

impl SidebarTabsInner {
    fn new(width: i32) -> Rc<Self> {
        let dark = icons::is_dark_theme();
        let clamped_width = width.clamp(
            AppSettings::SIDEBAR_WIDTH_MIN as i32,
            AppSettings::SIDEBAR_WIDTH_MAX as i32,
        );

        let root = BoxWidget::new(Orientation::Vertical, 4);
        root.add_css_class("search-sidebar");
        root.set_size_request(clamped_width, -1);
        root.set_vexpand(true);

        // Marca minimalista de cabecera ("Dot").
        let brand_header = BoxWidget::new(Orientation::Horizontal, 8);
        brand_header.add_css_class("dot-brand-header");
        brand_header.set_halign(Align::Start);
        brand_header.set_valign(Align::Center);

        let brand_dot = BoxWidget::new(Orientation::Horizontal, 0);
        brand_dot.add_css_class("dot-brand-mark");
        brand_dot.set_valign(Align::Center);

        let brand_wordmark = Label::new(Some("Dot"));
        brand_wordmark.add_css_class("dot-brand-wordmark");
        brand_wordmark.set_xalign(0.0);

        brand_header.append(&brand_dot);
        brand_header.append(&brand_wordmark);

        // 0. Filtro en vivo de pestañas.
        let filter_entry = SearchEntry::new();
        filter_entry.set_placeholder_text("Filtrar pestañas…");
        filter_entry.set_tooltip_text(Some("Filtrar pestañas abiertas por título o dirección"));
        filter_entry.set_margins(4, 4, 2, 2);

        let filter_empty_box = BoxWidget::new(Orientation::Vertical, 6);
        filter_empty_box.set_margins(12, 12, 24, 12);
        filter_empty_box.set_halign(Align::Center);
        filter_empty_box.set_visible(false);
        let filter_empty_lbl = Label::new(Some("Sin pestañas coincidentes"));
        filter_empty_lbl.add_css_class("search-panel-empty");
        filter_empty_lbl.set_halign(Align::Center);
        let clear_filter_btn = Button::with_label("Limpiar filtro");
        clear_filter_btn.add_css_class("flat");
        clear_filter_btn.set_halign(Align::Center);
        filter_empty_box.append(&filter_empty_lbl);
        filter_empty_box.append(&clear_filter_btn);

        // 1. Cuadrícula superior de Favoritos (hasta 4 cuadrados por fila).
        let favorites_box = BoxWidget::new(Orientation::Vertical, 5);
        favorites_box.add_css_class("search-sidebar-favorites");
        favorites_box.set_visible(false);

        // 2. Cabecera sutil de sección fijada + botón `+ Carpeta`.
        let pinned_header = BoxWidget::new(Orientation::Horizontal, 4);
        pinned_header.add_css_class("search-sidebar-section-header");
        let pinned_title = Label::new(Some("Fijadas"));
        pinned_title.add_css_class("search-sidebar-section-label");
        pinned_title.set_xalign(0.0);
        pinned_title.set_hexpand(true);
        pinned_header.append(&pinned_title);

        let add_folder_btn = Button::new();
        add_folder_btn.add_css_class("flat");
        add_folder_btn.add_css_class("search-tab-mini-btn");
        add_folder_btn.set_tooltip_text(Some("Nueva carpeta de pines"));
        let add_folder_icon = icons::image_for_theme(IconId::FolderPlus, 14, dark);
        add_folder_btn.set_child(Some(&add_folder_icon));
        pinned_header.append(&add_folder_btn);

        // Editor inline discreto para crear o renombrar carpetas de pines.
        let folder_editor_box = BoxWidget::new(Orientation::Horizontal, 4);
        folder_editor_box.add_css_class("search-folder-editor");
        folder_editor_box.set_visible(false);

        let folder_editor_entry = Entry::new();
        folder_editor_entry.set_placeholder_text("Nombre de la carpeta…");
        folder_editor_entry.set_hexpand(true);
        folder_editor_box.append(&folder_editor_entry);

        let folder_ok_btn = icons::icon_button_for_theme(IconId::Check, 14, dark);
        folder_ok_btn.add_css_class("search-tab-mini-btn");
        folder_ok_btn.set_tooltip_text(Some("Confirmar"));
        folder_editor_box.append(&folder_ok_btn);

        let folder_cancel_btn = icons::icon_button_for_theme(IconId::Close, 13, dark);
        folder_cancel_btn.add_css_class("search-tab-mini-btn");
        folder_cancel_btn.set_tooltip_text(Some("Cancelar"));
        folder_editor_box.append(&folder_cancel_btn);

        // Árbol de carpetas de pines + filas fijadas.
        let pinned_tree_box = BoxWidget::new(Orientation::Vertical, 2);
        pinned_tree_box.add_css_class("search-sidebar-pinned-tree");
        pinned_tree_box.set_visible(false);

        let pinned_sep = Separator::new(Orientation::Horizontal);
        pinned_sep.add_css_class("search-sidebar-divider");
        pinned_sep.set_visible(false);

        // 3. Cabecera de pestañas de Hoy (Limpiar + Ordenar) y lista desplazable.
        let today_header = BoxWidget::new(Orientation::Horizontal, 4);
        today_header.add_css_class("search-sidebar-section-header");
        today_header.set_visible(false);
        let today_title = Label::new(Some("Hoy"));
        today_title.add_css_class("search-sidebar-section-label");
        today_title.set_xalign(0.0);
        today_title.set_hexpand(true);
        today_header.append(&today_title);

        let tidy_btn = Button::new();
        tidy_btn.add_css_class("flat");
        tidy_btn.add_css_class("search-tab-mini-btn");
        tidy_btn.add_css_class("tidy-btn");
        tidy_btn.set_tooltip_text(Some("Ordenar pestañas por dominio"));
        let tidy_row = BoxWidget::new(Orientation::Horizontal, 3);
        tidy_row.set_valign(Align::Center);
        let tidy_icon = icons::image_for_theme(IconId::Layers, 12, dark);
        let tidy_lbl = Label::new(Some("Ordenar"));
        tidy_row.append(&tidy_icon);
        tidy_row.append(&tidy_lbl);
        tidy_btn.set_child(Some(&tidy_row));
        today_header.append(&tidy_btn);

        let clear_today_btn = Button::new();
        clear_today_btn.add_css_class("flat");
        clear_today_btn.add_css_class("search-tab-mini-btn");
        clear_today_btn.add_css_class("tidy-btn");
        clear_today_btn.set_tooltip_text(Some("Limpiar pestañas no activas de hoy"));
        let clear_today_row = BoxWidget::new(Orientation::Horizontal, 3);
        clear_today_row.set_valign(Align::Center);
        let clear_today_icon = icons::image_for_theme(IconId::Broom, 12, dark);
        let clear_today_lbl = Label::new(Some("Limpiar"));
        clear_today_row.append(&clear_today_icon);
        clear_today_row.append(&clear_today_lbl);
        clear_today_btn.set_child(Some(&clear_today_row));
        today_header.append(&clear_today_btn);

        let list_box = ListBox::new();
        list_box.add_css_class("search-sidebar-list");
        list_box.set_selection_mode(SelectionMode::None);
        list_box.set_vexpand(true);

        // Scrollable combinando filas fijadas y pestañas regulares si crecen mucho.
        let scroll_inner = BoxWidget::new(Orientation::Vertical, 3);
        scroll_inner.append(&filter_empty_box);
        scroll_inner.append(&pinned_header);
        scroll_inner.append(&folder_editor_box);
        scroll_inner.append(&pinned_tree_box);
        scroll_inner.append(&pinned_sep);
        scroll_inner.append(&today_header);
        scroll_inner.append(&list_box);

        let scrolled = ScrolledWindow::new();
        scrolled.set_policy(PolicyType::Never, PolicyType::Automatic);
        scrolled.set_vexpand(true);
        scrolled.set_child(Some(&scroll_inner));

        // 3b. Barra de Espacios estilo Arc (`Personal`, `Trabajo`, `Dev`, `+`).
        let spaces_bar = BoxWidget::new(Orientation::Horizontal, 4);
        spaces_bar.add_css_class("search-sidebar-spaces-bar");
        let spaces_scroll = ScrolledWindow::new();
        spaces_scroll.set_policy(PolicyType::Automatic, PolicyType::Never);
        spaces_scroll.set_propagate_natural_height(true);
        spaces_scroll.set_hexpand(true);
        let spaces_pills_box = BoxWidget::new(Orientation::Horizontal, 4);
        spaces_pills_box.set_valign(Align::Center);
        spaces_scroll.set_child(Some(&spaces_pills_box));
        spaces_bar.append(&spaces_scroll);

        let add_space_btn = Button::new();
        add_space_btn.add_css_class("flat");
        add_space_btn.add_css_class("search-tab-mini-btn");
        add_space_btn.add_css_class("space-pill");
        add_space_btn.set_tooltip_text(Some("Nuevo espacio"));
        let add_space_icon = icons::image_for_theme(IconId::Plus, 13, dark);
        add_space_btn.set_child(Some(&add_space_icon));
        spaces_bar.append(&add_space_btn);

        // 4. Pie discreto: `+ Nueva pestaña` + botón compacto de Ajustes (`plasma://settings`).
        let footer = BoxWidget::new(Orientation::Horizontal, 4);
        footer.add_css_class("search-sidebar-footer");

        let new_tab_btn = Button::new();
        new_tab_btn.add_css_class("flat");
        new_tab_btn.add_css_class("search-sidebar-new-tab");
        new_tab_btn.set_hexpand(true);
        new_tab_btn.set_tooltip_text(Some("Nueva pestaña (Ctrl+T)"));
        let new_tab_row = BoxWidget::new(Orientation::Horizontal, 7);
        new_tab_row.set_valign(Align::Center);
        let new_tab_icon = icons::image_for_theme(IconId::Plus, 14, dark);
        let new_tab_lbl = Label::new(Some("Nueva pestaña"));
        new_tab_lbl.set_xalign(0.0);
        new_tab_row.append(&new_tab_icon);
        new_tab_row.append(&new_tab_lbl);
        new_tab_btn.set_child(Some(&new_tab_row));

        let settings_btn = Button::new();
        settings_btn.add_css_class("flat");
        settings_btn.add_css_class("search-sidebar-settings-btn");
        settings_btn.set_tooltip_text(Some("Ajustes (plasma://settings — Ctrl+,)"));
        let settings_icon = icons::image_for_theme(IconId::Settings, 15, dark);
        settings_btn.set_child(Some(&settings_icon));

        footer.append(&new_tab_btn);
        footer.append(&settings_btn);

        root.append(&brand_header);
        root.append(&filter_entry);
        root.append(&favorites_box);
        root.append(&scrolled);
        root.append(&spaces_bar);
        root.append(&footer);

        let callbacks = Rc::new(RefCell::new(SidebarCallbacks::default()));
        let folder_editor_mode = Rc::new(RefCell::new(None));
        let folder_popover = Rc::new(RefCell::new(None));
        let space_popover = Rc::new(RefCell::new(None));
        let motion_ms = Rc::new(Cell::new(180u32));
        let workspace_store = Rc::new(RefCell::new(default_workspace_store()));
        let tab_workspaces = Rc::new(RefCell::new(HashMap::new()));

        let sidebar = Rc::new_cyclic(|weak| Self {
            weak: weak.clone(),
            root,
            filter_entry: filter_entry.clone(),
            filter_empty_box,
            favorites_box,
            pinned_header,
            folder_editor_box,
            folder_editor_entry: folder_editor_entry.clone(),
            folder_editor_mode,
            pinned_tree_box,
            pinned_sep,
            today_header,
            tidy_btn: tidy_btn.clone(),
            tidy_icon,
            clear_today_btn: clear_today_btn.clone(),
            clear_today_icon,
            list_box,
            spaces_bar,
            spaces_pills_box,
            add_space_icon,
            new_tab_icon,
            settings_icon,
            add_folder_icon,
            folder_popover,
            space_popover,
            current_width: Rc::new(Cell::new(clamped_width)),
            motion_ms,
            last_items: Rc::new(RefCell::new(Vec::new())),
            last_folders: Rc::new(RefCell::new(Vec::new())),
            last_opts: Rc::new(Cell::new(SidebarRenderOptions::default())),
            workspace_store,
            tab_workspaces,
            callbacks: callbacks.clone(),
        });

        {
            let weak = sidebar.weak.clone();
            filter_entry.connect_search_changed(move || {
                if let Some(s) = weak.upgrade() {
                    s.render_filtered();
                }
            });
        }
        {
            let fe = filter_entry.clone();
            clear_filter_btn.connect_clicked(move || {
                fe.set_text("");
                fe.grab_focus();
            });
        }

        // Botones del pie.
        {
            let cbs = callbacks.clone();
            new_tab_btn.connect_clicked(move || {
                if let Some(ref cb) = cbs.borrow().on_new_tab {
                    cb();
                }
            });
        }
        {
            let cbs = callbacks.clone();
            settings_btn.connect_clicked(move || {
                if let Some(ref cb) = cbs.borrow().on_open_settings {
                    cb();
                }
            });
        }

        // Botón `+ Carpeta` y editor inline.
        {
            let weak = sidebar.weak.clone();
            add_folder_btn.connect_clicked(move || {
                if let Some(s) = weak.upgrade() {
                    s.begin_create_folder(None, None);
                }
            });
        }
        {
            let weak = sidebar.weak.clone();
            folder_ok_btn.connect_clicked(move || {
                if let Some(s) = weak.upgrade() {
                    s.commit_folder_editor();
                }
            });
        }
        {
            let weak = sidebar.weak.clone();
            folder_editor_entry.connect_activate(move || {
                if let Some(s) = weak.upgrade() {
                    s.commit_folder_editor();
                }
            });
        }
        {
            let weak = sidebar.weak.clone();
            folder_cancel_btn.connect_clicked(move || {
                if let Some(s) = weak.upgrade() {
                    s.cancel_folder_editor();
                }
            });
        }

        // Clic derecho en el fondo vacío de la barra lateral -> Nueva carpeta / Nueva pestaña.
        {
            let bg_right = GestureClick::new();
            bg_right.set_button(3);
            let weak = sidebar.weak.clone();
            bg_right.connect_pressed(move |_n, x, y| {
                if let Some(s) = weak.upgrade() {
                    s.open_sidebar_background_menu(&s.root, x, y);
                }
            });
            sidebar.list_box.add_controller(&bg_right);
        }

        // Botones "Ordenar" y "Limpiar" de la sección Hoy.
        {
            let weak = sidebar.weak.clone();
            tidy_btn.connect_clicked(move || {
                if let Some(s) = weak.upgrade() {
                    s.tidy_tabs_now();
                }
            });
        }
        {
            let weak = sidebar.weak.clone();
            clear_today_btn.connect_clicked(move || {
                if let Some(s) = weak.upgrade() {
                    s.clear_today_tabs();
                }
            });
        }

        // Botón `+` para crear un nuevo Espacio.
        {
            let weak = sidebar.weak.clone();
            add_space_btn.connect_clicked(move || {
                if let Some(s) = weak.upgrade() {
                    s.open_space_editor_popover(&s.spaces_bar, None);
                }
            });
        }

        sidebar.render_spaces_bar();
        sidebar
    }

    fn set_callbacks(&self, cbs: SidebarCallbacks) {
        *self.callbacks.borrow_mut() = cbs;
    }

    pub fn set_width(&self, width: i32) {
        let clamped = width.clamp(
            AppSettings::SIDEBAR_WIDTH_MIN as i32,
            AppSettings::SIDEBAR_WIDTH_MAX as i32,
        );
        self.current_width.set(clamped);
        self.root.set_size_request(clamped, -1);
    }

    pub fn width(&self) -> i32 {
        let allocated = self.root.width();
        if allocated > 0 {
            allocated
        } else {
            self.current_width.get()
        }
    }

    pub fn set_motion_duration(&self, ms: u32) {
        self.motion_ms.set(ms);
    }

    pub fn set_roomy(&self, roomy: bool) {
        if roomy {
            self.root.add_css_class("search-roomy-sidebar");
        } else {
            self.root.remove_css_class("search-roomy-sidebar");
        }
    }

    pub fn refresh_theme_icons(&self, dark: bool) {
        icons::set_image_icon(&self.new_tab_icon, IconId::Plus, dark);
        icons::set_image_icon(&self.settings_icon, IconId::Settings, dark);
        icons::set_image_icon(&self.add_folder_icon, IconId::FolderPlus, dark);
        icons::set_image_icon(&self.add_space_icon, IconId::Plus, dark);
        icons::set_image_icon(&self.tidy_icon, IconId::Layers, dark);
        icons::set_image_icon(&self.clear_today_icon, IconId::Broom, dark);
    }

    pub fn cleanup(&self) {
        if let Some(pop) = self.folder_popover.borrow_mut().take() {
            pop.popdown();
            pop.unparent();
        }
        if let Some(pop) = self.space_popover.borrow_mut().take() {
            pop.popdown();
            pop.unparent();
        }
    }

    /// Reemplaza el almacén de Espacios y actualiza la barra de píldoras y pestañas visibles.
    #[allow(dead_code)]
    pub fn set_workspace_store(&self, mut store: WorkspaceStore) {
        if store.workspaces.is_empty() {
            store = default_workspace_store();
        } else {
            store.sanitize();
        }
        *self.workspace_store.borrow_mut() = store;
        self.render_spaces_bar();
        self.render_filtered();
    }

    /// Devuelve una instantánea del almacén de Espacios actual.
    #[allow(dead_code)]
    pub fn workspace_store(&self) -> WorkspaceStore {
        self.workspace_store.borrow().clone()
    }

    /// Devuelve el ID del Espacio activo actualmente.
    #[allow(dead_code)]
    pub fn active_workspace_id(&self) -> Option<WorkspaceId> {
        self.workspace_store.borrow().active_workspace_id.clone()
    }

    /// Asigna una pestaña no fijada a un Espacio concreto.
    #[allow(dead_code)]
    pub fn set_tab_workspace(&self, tab_id: u64, workspace_id: &str) {
        self.tab_workspaces
            .borrow_mut()
            .insert(tab_id, workspace_id.to_string());
        self.render_filtered();
    }

    /// Cambia al Espacio `workspace_id`, filtra las pestañas no fijadas y dispara `on_workspace_changed`.
    pub fn switch_workspace(&self, workspace_id: &str) -> bool {
        let switched = {
            let mut store = self.workspace_store.borrow_mut();
            store
                .switch(workspace_id)
                .ok()
                .map(|ws| (ws.id.clone(), space_accent_hex(&ws.color_id).to_string()))
        };
        let Some((ws_id, accent_hex)) = switched else {
            return false;
        };
        self.render_spaces_bar();
        self.render_filtered();
        if let Some(ref cb) = self.callbacks.borrow().on_workspace_changed {
            cb(ws_id, accent_hex);
        }
        true
    }

    fn clear_today_tabs(&self) {
        let active_ws = self.active_workspace_id();
        let ids = {
            let items = self.last_items.borrow();
            let tab_ws = self.tab_workspaces.borrow();
            collect_clearable_tab_ids(&items, active_ws.as_deref(), &tab_ws)
        };
        if ids.is_empty() {
            return;
        }
        let cbs = self.callbacks.borrow();
        if let Some(ref cb) = cbs.on_clear_today {
            cb(ids);
        } else if let Some(ref close_cb) = cbs.on_close {
            for id in ids {
                close_cb(id);
            }
        }
    }

    fn tidy_tabs_now(&self) {
        let active_ws = self.active_workspace_id();
        {
            let mut items = self.last_items.borrow_mut();
            let tab_ws = self.tab_workspaces.borrow();
            tidy_items_by_domain(&mut items, active_ws.as_deref(), &tab_ws);
        }
        self.render_filtered();
        if let Some(ref cb) = self.callbacks.borrow().on_tidy_tabs {
            cb();
        }
    }

    fn render_spaces_bar(&self) {
        let dark = icons::is_dark_theme();
        let (workspaces, active_id) = {
            let store = self.workspace_store.borrow();
            (store.workspaces.clone(), store.active_workspace_id.clone())
        };

        let weak = self.weak.clone();
        rebuild_box_children(&self.spaces_pills_box, || {
            let mut pills = Vec::new();
            for ws in &workspaces {
                let is_active = active_id.as_deref() == Some(ws.id.as_str());
                let pill_holder = BoxWidget::new(Orientation::Horizontal, 0);

                let btn = Button::new();
                btn.add_css_class("flat");
                btn.add_css_class("space-pill");
                if is_active {
                    btn.add_css_class("active");
                }
                let accent = space_accent_hex(&ws.color_id);
                btn.set_tooltip_text(Some(&format!(
                    "Espacio {} ({accent}) — clic derecho para editar",
                    ws.name
                )));

                let row = BoxWidget::new(Orientation::Horizontal, 4);
                row.set_valign(Align::Center);
                let icon = icons::image_for_theme(space_icon_id(&ws.icon_id), 13, dark);
                let lbl = Label::new(Some(&ws.name));
                lbl.set_ellipsize(EllipsizeMode::End);
                row.append(&icon);
                row.append(&lbl);
                btn.set_child(Some(&row));

                {
                    let weak_click = weak.clone();
                    let ws_id = ws.id.clone();
                    btn.connect_clicked(move || {
                        if let Some(s) = weak_click.upgrade() {
                            s.switch_workspace(&ws_id);
                        }
                    });
                }

                {
                    let right = GestureClick::new();
                    right.set_button(3);
                    let weak_right = weak.clone();
                    let ws_id = ws.id.clone();
                    right.connect_pressed(move |_n, _x, _y| {
                        if let Some(s) = weak_right.upgrade() {
                            s.open_space_editor_popover(&s.spaces_bar, Some(ws_id.clone()));
                        }
                    });
                    btn.add_controller(&right);
                }

                pill_holder.append(&btn);
                pills.push(pill_holder);
            }
            pills
        });
    }

    fn open_space_editor_popover(&self, anchor: &impl AsWidget, edit_ws_id: Option<String>) {
        if let Some(old) = self.space_popover.borrow_mut().take() {
            old.popdown();
            old.unparent();
        }
        let pop = Popover::new();
        pop.set_parent(anchor);
        pop.set_has_arrow(true);
        pop.set_position(PositionType::Top);

        let (initial_name, initial_color, can_delete) = if let Some(ref ws_id) = edit_ws_id {
            let store = self.workspace_store.borrow();
            let ws = store.workspaces.iter().find(|w| &w.id == ws_id);
            let name = ws.map(|w| w.name.clone()).unwrap_or_default();
            let color = ws
                .map(|w| w.color_id.clone())
                .unwrap_or_else(|| "blue".to_string());
            (name, color, store.workspaces.len() > 1)
        } else {
            (String::new(), "blue".to_string(), false)
        };

        let container = BoxWidget::new(Orientation::Vertical, 6);
        container.set_margins(8, 8, 8, 8);

        let heading = Label::new(Some(if edit_ws_id.is_some() {
            "Editar espacio"
        } else {
            "Nuevo espacio"
        }));
        heading.add_css_class("search-sidebar-section-label");
        heading.set_xalign(0.0);
        container.append(&heading);

        let name_entry = Entry::new();
        name_entry.set_placeholder_text("Nombre del espacio…");
        name_entry.set_text(&initial_name);
        container.append(&name_entry);

        let selected_color = Rc::new(RefCell::new(initial_color));
        let color_row = BoxWidget::new(Orientation::Horizontal, 4);
        let swatch_buttons: Rc<RefCell<Vec<(String, Button)>>> = Rc::new(RefCell::new(Vec::new()));
        for (color_id, color_label, hex) in SPACE_ACCENT_COLORS {
            let swatch = Button::with_label(color_label);
            swatch.add_css_class("flat");
            swatch.add_css_class("space-pill");
            if *color_id == selected_color.borrow().as_str() {
                swatch.add_css_class("active");
            }
            swatch.set_tooltip_text(Some(&format!("{color_label} ({hex})")));

            let cid = (*color_id).to_string();
            let sel_ref = selected_color.clone();
            let btns_weak = Rc::downgrade(&swatch_buttons);
            swatch.connect_clicked(move || {
                *sel_ref.borrow_mut() = cid.clone();
                if let Some(btns) = btns_weak.upgrade() {
                    for (id, b) in btns.borrow().iter() {
                        if id == &cid {
                            b.add_css_class("active");
                        } else {
                            b.remove_css_class("active");
                        }
                    }
                }
            });
            swatch_buttons
                .borrow_mut()
                .push(((*color_id).to_string(), swatch.clone()));
            color_row.append(&swatch);
        }
        container.append(&color_row);

        let actions_row = BoxWidget::new(Orientation::Horizontal, 4);
        let save_btn = Button::with_label(if edit_ws_id.is_some() {
            "Guardar"
        } else {
            "Crear"
        });
        save_btn.add_css_class("suggested-action");
        save_btn.set_hexpand(true);
        actions_row.append(&save_btn);

        if can_delete {
            if let Some(ref ws_id) = edit_ws_id {
                let del_btn = Button::with_label("Eliminar");
                del_btn.add_css_class("flat");
                del_btn.add_css_class("destructive-action");
                let weak = self.weak.clone();
                let del_id = ws_id.clone();
                del_btn.connect_clicked(move || {
                    if let Some(s) = weak.upgrade() {
                        if let Some(p) = s.space_popover.borrow_mut().take() {
                            p.popdown();
                            p.unparent();
                        }
                        let fallback_ws = {
                            let mut store = s.workspace_store.borrow_mut();
                            let _ = store.delete(&del_id);
                            store.active().map(|w| w.id.clone())
                        };
                        if let Some(next_id) = fallback_ws {
                            let mut map = s.tab_workspaces.borrow_mut();
                            for ws in map.values_mut() {
                                if ws == &del_id {
                                    *ws = next_id.clone();
                                }
                            }
                            drop(map);
                            s.switch_workspace(&next_id);
                        } else {
                            s.render_spaces_bar();
                            s.render_filtered();
                        }
                    }
                });
                actions_row.append(&del_btn);
            }
        }
        container.append(&actions_row);

        let entry_slot = Rc::new(RefCell::new(Some(name_entry.clone())));
        let commit_action = {
            let weak = self.weak.clone();
            let entry_slot = entry_slot.clone();
            let sel_color = selected_color.clone();
            let edit_id = edit_ws_id.clone();
            Rc::new(move || {
                let Some(entry) = entry_slot.borrow_mut().take() else {
                    return;
                };
                let name = entry.text().trim().to_string();
                let Some(s) = weak.upgrade() else {
                    return;
                };
                if let Some(p) = s.space_popover.borrow_mut().take() {
                    p.popdown();
                    p.unparent();
                }
                if name.is_empty() {
                    return;
                }
                let color_id = sel_color.borrow().clone();
                if let Some(ref ws_id) = edit_id {
                    let is_active = {
                        let mut store = s.workspace_store.borrow_mut();
                        let _ = store.rename(ws_id, &name);
                        let _ = store.set_appearance(ws_id, &color_id, "");
                        store.active_workspace_id.as_deref() == Some(ws_id.as_str())
                    };
                    s.render_spaces_bar();
                    if is_active {
                        if let Some(ref cb) = s.callbacks.borrow().on_workspace_changed {
                            cb(ws_id.clone(), space_accent_hex(&color_id).to_string());
                        }
                    }
                } else {
                    let new_id = {
                        let mut store = s.workspace_store.borrow_mut();
                        store.create(&name, &color_id, "spaces").id.clone()
                    };
                    s.switch_workspace(&new_id);
                }
            })
        };

        {
            let commit = commit_action.clone();
            save_btn.connect_clicked(move || commit());
        }
        {
            let commit = commit_action;
            name_entry.connect_activate(move || commit());
        }

        pop.set_child(Some(&container));
        pop.popup();
        name_entry.grab_focus();
        *self.space_popover.borrow_mut() = Some(pop);
    }

    /// Abre el editor inline de carpetas para crear una carpeta (raíz o subcarpeta),
    /// moviendo opcionalmente `move_tab_id` dentro de ella al confirmar.
    pub fn begin_create_folder(&self, parent_id: Option<String>, move_tab_id: Option<u64>) {
        *self.folder_editor_mode.borrow_mut() = Some(FolderEditorMode::Create {
            parent_id,
            move_tab_id,
        });
        self.folder_editor_entry
            .set_placeholder_text("Nombre de la carpeta…");
        self.folder_editor_entry.set_text("");
        self.folder_editor_box.set_visible(true);
        self.folder_editor_entry.grab_focus();
    }

    /// Abre el editor inline para renombrar una carpeta existente.
    pub fn begin_rename_folder(&self, folder_id: String, current_name: &str) {
        *self.folder_editor_mode.borrow_mut() = Some(FolderEditorMode::Rename { folder_id });
        self.folder_editor_entry
            .set_placeholder_text("Nuevo nombre…");
        self.folder_editor_entry.set_text(current_name);
        self.folder_editor_box.set_visible(true);
        self.folder_editor_entry.grab_focus();
        self.folder_editor_entry.select_all();
    }

    fn commit_folder_editor(&self) {
        let name = self.folder_editor_entry.text().trim().to_string();
        let mode = self.folder_editor_mode.borrow_mut().take();
        self.folder_editor_box.set_visible(false);
        self.folder_editor_entry.set_text("");
        if name.is_empty() {
            return;
        }
        if let Some(mode) = mode {
            match mode {
                FolderEditorMode::Create {
                    parent_id,
                    move_tab_id,
                } => {
                    if let Some(ref cb) = self.callbacks.borrow().on_create_folder {
                        cb(name, parent_id, move_tab_id);
                    }
                }
                FolderEditorMode::Rename { folder_id } => {
                    if let Some(ref cb) = self.callbacks.borrow().on_rename_folder {
                        cb(folder_id, name);
                    }
                }
            }
        }
    }

    fn cancel_folder_editor(&self) {
        *self.folder_editor_mode.borrow_mut() = None;
        self.folder_editor_box.set_visible(false);
        self.folder_editor_entry.set_text("");
    }

    fn open_sidebar_background_menu(&self, anchor: &BoxWidget, x: f64, y: f64) {
        if let Some(old) = self.folder_popover.borrow_mut().take() {
            old.popdown();
            old.unparent();
        }
        let pop = Popover::new();
        pop.set_parent(anchor);
        pop.set_has_arrow(false);
        pop.set_position(PositionType::Right);
        pop.set_pointing_to(Some(Rectangle {
            x: x.round() as i32,
            y: y.round() as i32,
            width: 1,
            height: 1,
        }));

        let menu_box = BoxWidget::new(Orientation::Vertical, 2);
        menu_box.set_margins(4, 4, 4, 4);

        let btn_folder = Button::with_label("Nueva carpeta");
        btn_folder.add_css_class("flat");
        {
            let weak = self.weak.clone();
            let p = pop.clone();
            btn_folder.connect_clicked(move || {
                p.popdown();
                if let Some(s) = weak.upgrade() {
                    s.begin_create_folder(None, None);
                }
            });
        }
        menu_box.append(&btn_folder);

        let btn_tab = Button::with_label("Nueva pestaña");
        btn_tab.add_css_class("flat");
        {
            let cbs = self.callbacks.clone();
            let p = pop.clone();
            btn_tab.connect_clicked(move || {
                p.popdown();
                if let Some(ref cb) = cbs.borrow().on_new_tab {
                    cb();
                }
            });
        }
        menu_box.append(&btn_tab);

        pop.set_child(Some(&menu_box));
        pop.popup();
        *self.folder_popover.borrow_mut() = Some(pop);
    }

    fn open_folder_context_menu(
        &self,
        folder: &PinFolder,
        depth: usize,
        anchor: &BoxWidget,
        x: f64,
        y: f64,
    ) {
        if let Some(old) = self.folder_popover.borrow_mut().take() {
            old.popdown();
            old.unparent();
        }
        let pop = Popover::new();
        pop.set_parent(anchor);
        pop.set_has_arrow(false);
        pop.set_position(PositionType::Right);
        pop.set_pointing_to(Some(Rectangle {
            x: x.round() as i32,
            y: y.round() as i32,
            width: 1,
            height: 1,
        }));

        let menu_box = BoxWidget::new(Orientation::Vertical, 2);
        menu_box.set_margins(4, 4, 4, 4);

        if depth + 1 < MAX_PIN_FOLDER_DEPTH {
            let sub_btn = Button::with_label("Nueva subcarpeta");
            sub_btn.add_css_class("flat");
            let weak = self.weak.clone();
            let fid = folder.id.clone();
            let p = pop.clone();
            sub_btn.connect_clicked(move || {
                p.popdown();
                if let Some(s) = weak.upgrade() {
                    s.begin_create_folder(Some(fid.clone()), None);
                }
            });
            menu_box.append(&sub_btn);
        }

        let ren_btn = Button::with_label("Renombrar carpeta");
        ren_btn.add_css_class("flat");
        {
            let weak = self.weak.clone();
            let fid = folder.id.clone();
            let fname = folder.name.clone();
            let p = pop.clone();
            ren_btn.connect_clicked(move || {
                p.popdown();
                if let Some(s) = weak.upgrade() {
                    s.begin_rename_folder(fid.clone(), &fname);
                }
            });
        }
        menu_box.append(&ren_btn);

        menu_box.append(&Separator::new(Orientation::Horizontal));

        let del_btn = Button::with_label("Eliminar carpeta");
        del_btn.add_css_class("flat");
        del_btn.add_css_class("destructive-action");
        {
            let cbs = self.callbacks.clone();
            let fid = folder.id.clone();
            let p = pop.clone();
            del_btn.connect_clicked(move || {
                p.popdown();
                if let Some(ref cb) = cbs.borrow().on_delete_folder {
                    cb(fid.clone());
                }
            });
        }
        menu_box.append(&del_btn);

        pop.set_child(Some(&menu_box));
        pop.popup();
        *self.folder_popover.borrow_mut() = Some(pop);
    }

    /// Reconstruye la vista completa de la barra lateral (favoritos cuadrados, carpetas y filas fijadas, pestañas regulares).
    pub fn sync(
        &self,
        items: &[SidebarTabItem],
        folders: &[PinFolder],
        opts: SidebarRenderOptions,
    ) {
        {
            let store = self.workspace_store.borrow();
            let active_id = store.active_workspace_id.as_deref();
            let mut tab_ws = self.tab_workspaces.borrow_mut();
            let live_ids: std::collections::HashSet<u64> = items.iter().map(|i| i.id).collect();
            tab_ws.retain(|id, _| live_ids.contains(id));
            for item in items {
                if !item.is_pinned() && !tab_ws.contains_key(&item.id) {
                    let matched = store
                        .workspaces
                        .iter()
                        .find(|w| w.tab_urls.iter().any(|u| u == &item.url))
                        .map(|w| w.id.clone())
                        .or_else(|| active_id.map(|id| id.to_string()));
                    if let Some(ws_id) = matched {
                        tab_ws.insert(item.id, ws_id);
                    }
                }
            }
        }
        *self.last_items.borrow_mut() = items.to_vec();
        *self.last_folders.borrow_mut() = folders.to_vec();
        self.last_opts.set(opts);
        self.render_spaces_bar();
        self.render_filtered();
    }

    fn render_filtered(&self) {
        let all_items = self.last_items.borrow();
        let folders = self.last_folders.borrow();
        let opts = self.last_opts.get();
        let active_ws = self.active_workspace_id();
        let tab_ws = self.tab_workspaces.borrow();

        self.set_roomy(opts.roomy);
        self.set_motion_duration(opts.motion_ms);
        let dark = icons::is_dark_theme();
        self.refresh_theme_icons(dark);

        let query = self.filter_entry.text().trim().to_lowercase();
        let filtered_items: Vec<&SidebarTabItem> = if query.is_empty() {
            all_items.iter().collect()
        } else {
            all_items
                .iter()
                .filter(|i| {
                    i.title.to_lowercase().contains(&query) || i.url.to_lowercase().contains(&query)
                })
                .collect()
        };

        if !query.is_empty() && filtered_items.is_empty() {
            self.favorites_box.set_visible(false);
            self.pinned_header.set_visible(false);
            self.pinned_tree_box.set_visible(false);
            self.pinned_sep.set_visible(false);
            self.today_header.set_visible(false);
            self.list_box.remove_all();
            self.filter_empty_box.set_visible(true);
            return;
        }
        self.filter_empty_box.set_visible(false);

        // Separar favoritos cuadrados, filas fijadas y pestañas regulares del espacio activo.
        let squares: Vec<&SidebarTabItem> = filtered_items
            .iter()
            .copied()
            .filter(|i| i.pin_style == Some(PinStyle::Square) && i.folder_id.is_none())
            .collect();
        let pinned_rows: Vec<&SidebarTabItem> = filtered_items
            .iter()
            .copied()
            .filter(|i| i.pin_style == Some(PinStyle::Row) || i.folder_id.is_some())
            .collect();
        let regular: Vec<&SidebarTabItem> = filtered_items
            .iter()
            .copied()
            .filter(|i| {
                !i.is_pinned() && tab_belongs_to_workspace(i, active_ws.as_deref(), &tab_ws)
            })
            .collect();

        // 1. Reconstruir cuadrícula superior de Favoritos (hasta 4 por fila).
        rebuild_box_children(&self.favorites_box, || {
            let mut rows = Vec::new();
            for chunk in squares.chunks(4) {
                let row_box = BoxWidget::new(Orientation::Horizontal, 5);
                row_box.set_hexpand(true);
                for item in chunk {
                    let tile = build_favorite_tile(item, opts, dark, &self.callbacks);
                    row_box.append(&tile);
                }
                rows.push(row_box);
            }
            rows
        });
        self.favorites_box.set_visible(!squares.is_empty());

        // 2. Reconstruir árbol de carpetas de pines y filas fijadas.
        let has_pinned_tree = !folders.is_empty() || !pinned_rows.is_empty();
        self.pinned_header
            .set_visible(query.is_empty() || has_pinned_tree);
        rebuild_box_children(&self.pinned_tree_box, || {
            if !has_pinned_tree {
                return Vec::new();
            }
            let mut out = Vec::new();
            self.build_folder_level(None, 0, &folders, &pinned_rows, opts, dark, &mut out);
            out
        });
        self.pinned_tree_box.set_visible(has_pinned_tree);
        self.pinned_sep
            .set_visible((!squares.is_empty() || has_pinned_tree) && !regular.is_empty());

        // 3. Cabecera de Hoy (Ordenar + Limpiar) y lista de pestañas regulares.
        let visible_regular_count = regular.iter().filter(|i| !i.split_hidden).count();
        let clearable_count = regular
            .iter()
            .filter(|i| !i.selected && !i.split_hidden)
            .count();
        self.today_header.set_visible(visible_regular_count > 0);
        self.tidy_btn.set_sensitive(visible_regular_count >= 2);
        self.clear_today_btn.set_sensitive(clearable_count > 0);

        self.list_box.remove_all();
        for item in regular {
            if item.split_hidden {
                continue;
            }
            let row = build_tab_list_row(item, false, 0, opts, dark, &self.callbacks);
            self.list_box.append(&row);
        }
    }

    /// Abre el menú contextual enriquecido de una pestaña cuando no se delega externamente.
    #[allow(clippy::too_many_arguments, dead_code)]
    pub fn open_tab_context_menu(
        &self,
        tab_id: u64,
        tab_url: &str,
        is_muted: bool,
        is_pinned: bool,
        anchor: &BoxWidget,
        x: f64,
        y: f64,
    ) {
        if let Some(old) = self.folder_popover.borrow_mut().take() {
            old.popdown();
            old.unparent();
        }
        let pop = Popover::new();
        pop.set_parent(anchor);
        pop.set_has_arrow(false);
        pop.set_position(PositionType::Right);
        pop.set_pointing_to(Some(Rectangle {
            x: x.round() as i32,
            y: y.round() as i32,
            width: 1,
            height: 1,
        }));

        let menu_box = BoxWidget::new(Orientation::Vertical, 2);
        menu_box.set_margins(4, 4, 4, 4);

        let add_item = |label: &str, destructive: bool, action: Box<dyn Fn()>| {
            let btn = Button::with_label(label);
            btn.add_css_class("flat");
            if destructive {
                btn.add_css_class("destructive-action");
            }
            let p = pop.clone();
            btn.connect_clicked(move || {
                p.popdown();
                action();
            });
            menu_box.append(&btn);
        };

        {
            let cbs = self.callbacks.clone();
            add_item(
                "Duplicar pestaña",
                false,
                Box::new(move || {
                    if let Some(ref cb) = cbs.borrow().on_duplicate_tab {
                        cb(tab_id);
                    }
                }),
            );
        }
        {
            let cbs = self.callbacks.clone();
            let mute_lbl = if is_muted {
                "Activar sonido"
            } else {
                "Silenciar pestaña"
            };
            add_item(
                mute_lbl,
                false,
                Box::new(move || {
                    if let Some(ref cb) = cbs.borrow().on_toggle_mute {
                        cb(tab_id);
                    }
                }),
            );
        }
        {
            let cbs = self.callbacks.clone();
            let clean_url = clean_link(tab_url);
            add_item(
                "Copiar URL limpia",
                false,
                Box::new(move || {
                    if let Some(ref cb) = cbs.borrow().on_copy_clean_url {
                        cb(tab_id, clean_url.clone());
                    } else if let Some(disp) = gdk::Display::default_display() {
                        disp.set_clipboard_text(&clean_url);
                    }
                }),
            );
        }
        {
            let cbs = self.callbacks.clone();
            add_item(
                "Mover a ventana nueva",
                false,
                Box::new(move || {
                    if let Some(ref cb) = cbs.borrow().on_move_to_new_window {
                        cb(tab_id);
                    }
                }),
            );
        }

        menu_box.append(&Separator::new(Orientation::Horizontal));

        {
            let cbs = self.callbacks.clone();
            add_item(
                "Cerrar pestañas debajo",
                false,
                Box::new(move || {
                    if let Some(ref cb) = cbs.borrow().on_close_tabs_below {
                        cb(tab_id);
                    }
                }),
            );
        }
        {
            let cbs = self.callbacks.clone();
            add_item(
                "Cerrar otras pestañas",
                false,
                Box::new(move || {
                    if let Some(ref cb) = cbs.borrow().on_close_other_tabs {
                        cb(tab_id);
                    }
                }),
            );
        }
        {
            let cbs = self.callbacks.clone();
            add_item(
                "Reabrir pestaña cerrada",
                false,
                Box::new(move || {
                    if let Some(ref cb) = cbs.borrow().on_reopen_closed_tab {
                        cb();
                    }
                }),
            );
        }

        if !is_pinned {
            menu_box.append(&Separator::new(Orientation::Horizontal));
            let cbs = self.callbacks.clone();
            add_item(
                "Cerrar pestaña",
                true,
                Box::new(move || {
                    if let Some(ref cb) = cbs.borrow().on_close {
                        cb(tab_id);
                    }
                }),
            );
        }

        pop.set_child(Some(&menu_box));
        pop.popup();
        *self.folder_popover.borrow_mut() = Some(pop);
    }

    #[allow(clippy::too_many_arguments)]
    fn build_folder_level(
        &self,
        parent_id: Option<&str>,
        depth: usize,
        folders: &[PinFolder],
        pinned_rows: &[&SidebarTabItem],
        opts: SidebarRenderOptions,
        dark: bool,
        out: &mut Vec<BoxWidget>,
    ) {
        if depth >= MAX_PIN_FOLDER_DEPTH {
            return;
        }
        let valid_ids: std::collections::HashSet<&str> =
            folders.iter().map(|f| f.id.as_str()).collect();

        // Primero las carpetas de este nivel (igual que `PinFolders.swift`).
        for folder in folders
            .iter()
            .filter(|f| f.parent_id.as_deref() == parent_id)
        {
            let count = count_items_in_folder(&folder.id, folders, pinned_rows);
            let folder_block = BoxWidget::new(Orientation::Vertical, 1);
            folder_block.set_hexpand(true);

            // Cabecera de carpeta.
            let header_holder = BoxWidget::new(Orientation::Horizontal, 0);
            header_holder.set_hexpand(true);
            let indent_px = (depth as i32) * 14;
            header_holder.set_margins(0, 0, indent_px, 0);

            let header_btn = Button::new();
            header_btn.add_css_class("flat");
            header_btn.add_css_class("search-folder-row");
            header_btn.set_hexpand(true);

            let hbox = BoxWidget::new(Orientation::Horizontal, 6);
            hbox.set_valign(Align::Center);

            let is_open = !folder.collapsed;
            let chevron_img = icons::image_for_theme(
                if is_open {
                    IconId::ChevronDown
                } else {
                    IconId::ChevronRight
                },
                12,
                dark,
            );
            let folder_img = icons::image_for_theme(
                if is_open {
                    IconId::FolderOpen
                } else {
                    IconId::Folder
                },
                15,
                dark,
            );
            let name_lbl = Label::new(Some(&folder.name));
            name_lbl.add_css_class("search-folder-name");
            name_lbl.set_xalign(0.0);
            name_lbl.set_hexpand(true);
            name_lbl.set_ellipsize(EllipsizeMode::End);

            hbox.append(&chevron_img);
            hbox.append(&folder_img);
            hbox.append(&name_lbl);

            if count > 0 {
                let count_lbl = Label::new(Some(&count.to_string()));
                count_lbl.add_css_class("search-folder-count");
                hbox.append(&count_lbl);
            }

            header_btn.set_child(Some(&hbox));
            header_holder.append(&header_btn);

            // Contenido plegable con animación `GtkRevealer` (`SlideDown`).
            let revealer = Revealer::new();
            revealer.set_transition_type(RevealerTransitionType::SlideDown);
            revealer.set_transition_duration(opts.motion_ms);
            revealer.set_reveal_child(is_open);

            let children_box = BoxWidget::new(Orientation::Vertical, 1);
            let mut nested = Vec::new();
            self.build_folder_level(
                Some(&folder.id),
                depth + 1,
                folders,
                pinned_rows,
                opts,
                dark,
                &mut nested,
            );
            for child_widget in nested {
                children_box.append(&child_widget);
            }
            revealer.set_child(Some(&children_box));

            // Clic izquierdo: alternar plegado suavemente in-place sin reconstruir la barra.
            {
                let rev_ref = revealer.clone();
                let chev_ref = chevron_img.clone();
                let fimg_ref = folder_img.clone();
                let fid = folder.id.clone();
                let cbs = self.callbacks.clone();
                header_btn.connect_clicked(move || {
                    let next_open = !rev_ref.reveals_child();
                    rev_ref.set_reveal_child(next_open);
                    let cur_dark = icons::is_dark_theme();
                    icons::set_image_icon(
                        &chev_ref,
                        if next_open {
                            IconId::ChevronDown
                        } else {
                            IconId::ChevronRight
                        },
                        cur_dark,
                    );
                    icons::set_image_icon(
                        &fimg_ref,
                        if next_open {
                            IconId::FolderOpen
                        } else {
                            IconId::Folder
                        },
                        cur_dark,
                    );
                    if let Some(ref cb) = cbs.borrow().on_toggle_folder_collapsed {
                        cb(fid.clone(), !next_open);
                    }
                });
            }

            // Clic derecho: menú contextual de carpeta.
            {
                let right = GestureClick::new();
                right.set_button(3);
                let weak = self.weak.clone();
                let f_clone = folder.clone();
                let anchor = header_holder.clone();
                right.connect_pressed(move |_n, x, y| {
                    if let Some(s) = weak.upgrade() {
                        s.open_folder_context_menu(&f_clone, depth, &anchor, x, y);
                    }
                });
                header_btn.add_controller(&right);
            }

            // DropTarget: soltar una pestaña sobre una carpeta la fija dentro de esa carpeta.
            {
                let drop = DropTarget::for_string(DragAction::MOVE);
                let cbs = self.callbacks.clone();
                let fid = folder.id.clone();
                drop.connect_drop_string(move |payload, _x, _y| {
                    let Ok(tab_id) = payload.trim().parse::<u64>() else {
                        return false;
                    };
                    if let Some(ref cb) = cbs.borrow().on_drop_tab_into_folder {
                        cb(tab_id, fid.clone());
                        return true;
                    }
                    false
                });
                header_btn.add_controller(&drop);
            }

            folder_block.append(&header_holder);
            folder_block.append(&revealer);
            out.push(folder_block);
        }

        // Luego las filas fijadas que pertenecen directamente a `parent_id`
        // (las que apuntan a una carpeta borrada se muestran en la raíz).
        for item in pinned_rows {
            let effective_parent = item
                .folder_id
                .as_deref()
                .filter(|fid| valid_ids.contains(fid));
            if effective_parent == parent_id {
                let row_box = build_pinned_row_widget(item, depth, opts, dark, &self.callbacks);
                out.push(row_box);
            }
        }
    }
}

fn count_items_in_folder(
    folder_id: &str,
    folders: &[PinFolder],
    pinned_rows: &[&SidebarTabItem],
) -> usize {
    let direct = pinned_rows
        .iter()
        .filter(|t| t.folder_id.as_deref() == Some(folder_id))
        .count();
    let sub: usize = folders
        .iter()
        .filter(|f| f.parent_id.as_deref() == Some(folder_id))
        .map(|child| count_items_in_folder(&child.id, folders, pinned_rows))
        .sum();
    direct + sub
}

fn rebuild_box_children(container: &BoxWidget, build: impl FnOnce() -> Vec<BoxWidget>) {
    use std::collections::HashMap;
    thread_local! {
        static CHILD_TRACKER: RefCell<HashMap<usize, Vec<BoxWidget>>> =
            RefCell::new(HashMap::new());
    }
    let key = container.as_widget_ptr() as usize;
    CHILD_TRACKER.with(|map| {
        if let Some(old) = map.borrow_mut().remove(&key) {
            for w in old {
                container.remove(&w);
            }
        }
    });
    let created = build();
    for w in &created {
        container.append(w);
    }
    if !created.is_empty() {
        CHILD_TRACKER.with(|map| {
            map.borrow_mut().insert(key, created);
        });
    }
}

fn build_favorite_tile(
    item: &SidebarTabItem,
    opts: SidebarRenderOptions,
    dark: bool,
    callbacks: &Rc<RefCell<SidebarCallbacks>>,
) -> BoxWidget {
    let holder = BoxWidget::new(Orientation::Horizontal, 0);
    holder.set_hexpand(true);

    let btn = Button::new();
    btn.add_css_class("search-fav-tile");
    btn.set_hexpand(true);
    btn.set_tooltip_text(Some(&item.tooltip));
    if item.selected {
        btn.add_css_class("active");
    }

    let content = BoxWidget::new(Orientation::Horizontal, 3);
    content.set_halign(Align::Center);
    content.set_valign(Align::Center);

    if item.loading {
        let spin = Spinner::new();
        spin.set_spinning(true);
        spin.set_size_request(16, 16);
        content.append(&spin);
    } else if item.url.starts_with("plasma://settings") {
        content.append(&icons::image_for_theme(IconId::Settings, 18, dark));
    } else if opts.glyph == TabGlyph::Favicon {
        if let Some(ref tex) = item.favicon {
            let img = Image::new();
            img.set_pixel_size(18);
            img.set_from_texture(Some(tex));
            content.append(&img);
        } else {
            let letter = item
                .pin_label
                .clone()
                .unwrap_or_else(|| icons::monogram_letter(&item.title, &item.url));
            content.append(&icons::monogram_badge(&letter, &item.url, true));
        }
    } else {
        let letter = item
            .pin_label
            .clone()
            .unwrap_or_else(|| icons::monogram_letter(&item.title, &item.url));
        content.append(&icons::monogram_badge(&letter, &item.url, true));
    }

    if opts.show_audio_indicators && (item.muted || item.audible) {
        let audio_icon = icons::image_for_theme(
            if item.muted {
                IconId::VolumeMuted
            } else {
                IconId::VolumeHigh
            },
            11,
            dark,
        );
        content.append(&audio_icon);
    }

    btn.set_child(Some(&content));

    let id = item.id;
    {
        let cbs = callbacks.clone();
        btn.connect_clicked(move || {
            if let Some(ref cb) = cbs.borrow().on_select {
                cb(id);
            }
        });
    }
    {
        let right = GestureClick::new();
        right.set_button(3);
        let cbs = callbacks.clone();
        let anchor = holder.clone();
        right.connect_pressed(move |_n, x, y| {
            if let Some(ref cb) = cbs.borrow().on_context_menu {
                cb(id, anchor.clone(), x, y);
            }
        });
        btn.add_controller(&right);
    }

    attach_drag_and_drop(&btn, id, callbacks);
    holder.append(&btn);
    holder
}

fn build_pinned_row_widget(
    item: &SidebarTabItem,
    depth: usize,
    opts: SidebarRenderOptions,
    dark: bool,
    callbacks: &Rc<RefCell<SidebarCallbacks>>,
) -> BoxWidget {
    let outer = BoxWidget::new(Orientation::Horizontal, 0);
    outer.add_css_class("search-tab-row");
    outer.add_css_class("pinned-row");
    if item.selected {
        outer.add_css_class("active");
    }
    outer.set_hexpand(true);
    outer.set_tooltip_text(Some(&item.tooltip));
    let indent_px = (depth as i32) * 14;
    outer.set_margins(0, 0, indent_px, 0);

    let inner = build_tab_row_content(item, true, opts, dark, callbacks, &outer);
    outer.append(&inner);
    outer
}

fn build_tab_list_row(
    item: &SidebarTabItem,
    is_pinned: bool,
    depth: usize,
    opts: SidebarRenderOptions,
    dark: bool,
    callbacks: &Rc<RefCell<SidebarCallbacks>>,
) -> ListBoxRow {
    let row = ListBoxRow::new();
    row.add_css_class("search-tab-row");
    if is_pinned {
        row.add_css_class("pinned-row");
    }
    row.set_selectable(false);
    row.set_activatable(false);
    row.set_tooltip_text(Some(&item.tooltip));
    if item.selected {
        row.add_css_class("active");
    }

    let holder = BoxWidget::new(Orientation::Horizontal, 0);
    holder.set_hexpand(true);
    if depth > 0 {
        holder.set_margins(0, 0, (depth as i32) * 14, 0);
    }
    let inner = build_tab_row_content(item, is_pinned, opts, dark, callbacks, &holder);
    holder.append(&inner);
    row.set_child(Some(&holder));
    row
}

fn build_tab_row_content(
    item: &SidebarTabItem,
    is_pinned: bool,
    opts: SidebarRenderOptions,
    dark: bool,
    callbacks: &Rc<RefCell<SidebarCallbacks>>,
    anchor_box: &BoxWidget,
) -> BoxWidget {
    let hbox = BoxWidget::new(Orientation::Horizontal, 7);
    hbox.set_hexpand(true);
    hbox.set_valign(Align::Center);

    if item.loading {
        let spinner = Spinner::new();
        spinner.set_spinning(true);
        spinner.set_size_request(16, 16);
        hbox.append(&spinner);
    } else if item.url.starts_with("plasma://settings") {
        hbox.append(&icons::image_for_theme(IconId::Settings, 16, dark));
    } else if opts.glyph == TabGlyph::Favicon {
        if let Some(ref tex) = item.favicon {
            let img = Image::new();
            img.set_pixel_size(16);
            img.set_from_texture(Some(tex));
            hbox.append(&img);
        } else {
            let letter = icons::monogram_letter(&item.title, &item.url);
            hbox.append(&icons::monogram_badge(&letter, &item.url, false));
        }
    } else {
        let letter = icons::monogram_letter(&item.title, &item.url);
        hbox.append(&icons::monogram_badge(&letter, &item.url, false));
    }

    // Segundo favicon si la pestaña forma parte de un Split View (`SplitTabItem.swift`).
    if let Some(ref partner_url) = item.split_partner_url {
        let partner_title = item.split_partner_title.as_deref().unwrap_or("");
        if opts.glyph == TabGlyph::Favicon {
            if let Some(ref ptex) = item.split_partner_favicon {
                let img = Image::new();
                img.set_pixel_size(14);
                img.set_from_texture(Some(ptex));
                img.add_css_class("search-split-tab-favicon");
                hbox.append(&img);
            } else {
                let letter = icons::monogram_letter(partner_title, partner_url);
                hbox.append(&icons::monogram_badge(&letter, partner_url, false));
            }
        } else {
            let letter = icons::monogram_letter(partner_title, partner_url);
            hbox.append(&icons::monogram_badge(&letter, partner_url, false));
        }
    }

    let display_label = match item.split_partner_title.as_deref() {
        Some(partner) if !partner.is_empty() => format!("{}  |  {partner}", item.title),
        _ => item.title.clone(),
    };
    let title_label = Label::new(Some(&display_label));
    title_label.set_ellipsize(EllipsizeMode::End);
    title_label.set_xalign(0.0);
    title_label.set_hexpand(true);
    hbox.append(&title_label);

    let id = item.id;

    if opts.show_audio_indicators && (item.audible || item.muted) {
        let audio_btn = icons::icon_button_for_theme(
            if item.muted {
                IconId::VolumeMuted
            } else {
                IconId::VolumeHigh
            },
            13,
            dark,
        );
        audio_btn.add_css_class("search-tab-mini-btn");
        audio_btn.set_tooltip_text(Some(if item.muted {
            "Activar sonido"
        } else {
            "Silenciar pestaña"
        }));
        let cbs = callbacks.clone();
        audio_btn.connect_clicked(move || {
            if let Some(ref cb) = cbs.borrow().on_toggle_mute {
                cb(id);
            }
        });
        hbox.append(&audio_btn);
    }

    if !is_pinned {
        let close_btn = icons::icon_button_for_theme(IconId::Close, 12, dark);
        close_btn.add_css_class("search-tab-mini-btn");
        close_btn.set_tooltip_text(Some("Cerrar pestaña"));
        close_btn.set_visible(false);
        {
            let cbs = callbacks.clone();
            close_btn.connect_clicked(move || {
                if let Some(ref cb) = cbs.borrow().on_close {
                    cb(id);
                }
            });
        }
        hbox.append(&close_btn);

        let motion = EventControllerMotion::new();
        {
            let cb = close_btn.clone();
            motion.connect_enter(move |_x, _y| {
                cb.set_visible(true);
            });
        }
        {
            let cb = close_btn;
            motion.connect_leave(move || {
                cb.set_visible(false);
            });
        }
        hbox.add_controller(&motion);
    }

    // Clic izquierdo selecciona; clic central cierra (si no está fijada); clic derecho abre menú contextual.
    {
        let click = GestureClick::new();
        click.set_button(0);
        let click_ref = click.clone();
        let cbs = callbacks.clone();
        let anchor = anchor_box.clone();
        click.connect_pressed(move |_n, x, y| match click_ref.current_button() {
            1 => {
                if let Some(ref cb) = cbs.borrow().on_select {
                    cb(id);
                }
            }
            2 => {
                if let Some(ref cb) = cbs.borrow().on_close {
                    cb(id);
                }
            }
            3 => {
                if let Some(ref cb) = cbs.borrow().on_context_menu {
                    cb(id, anchor.clone(), x, y);
                }
            }
            _ => {}
        });
        hbox.add_controller(&click);
    }

    attach_drag_and_drop(&hbox, id, callbacks);
    hbox
}

fn attach_drag_and_drop(
    widget: &impl AsWidget,
    tab_id: u64,
    callbacks: &Rc<RefCell<SidebarCallbacks>>,
) {
    let drag = DragSource::new();
    drag.set_actions(DragAction::MOVE);
    drag.connect_prepare(move |_x, _y| Some(ContentProvider::for_string(&tab_id.to_string())));
    widget.add_controller(&drag);

    let drop = DropTarget::for_string(DragAction::MOVE);
    let cbs = callbacks.clone();
    drop.connect_drop_string(move |payload, _x, _y| {
        let Ok(src_id) = payload.trim().parse::<u64>() else {
            return false;
        };
        if src_id == tab_id {
            return false;
        }
        if let Some(ref cb) = cbs.borrow().on_reorder {
            cb(src_id, tab_id);
            return true;
        }
        false
    });
    widget.add_controller(&drop);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Weak;

    #[test]
    fn pin_folders_subscribe_unsubscribe_drops_listener_rc() {
        let dir = std::env::temp_dir().join(format!(
            "search-pin-folders-unsub-test-{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let mgr = PinFoldersManager::load(dir.join("pin-folders.json"));

        let sentinel = Rc::new(Cell::new(0usize));
        let weak = Rc::downgrade(&sentinel);
        let sub_id = mgr.subscribe(move || {
            sentinel.set(sentinel.get() + 1);
        });
        assert_eq!(Weak::strong_count(&weak), 1);

        mgr.unsubscribe(sub_id);
        assert_eq!(Weak::strong_count(&weak), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn make_test_item(
        id: u64,
        url: &str,
        selected: bool,
        pin_style: Option<PinStyle>,
    ) -> SidebarTabItem {
        SidebarTabItem {
            id,
            url: url.to_string(),
            title: format!("Tab {id}"),
            tooltip: String::new(),
            pin_label: None,
            pin_style,
            folder_id: None,
            selected,
            loading: false,
            audible: false,
            muted: false,
            favicon: None,
            split_partner_favicon: None,
            split_partner_title: None,
            split_partner_url: None,
            split_hidden: false,
        }
    }

    #[test]
    fn extract_domain_normalizes_urls() {
        assert_eq!(
            extract_domain("https://www.github.com/rust-lang"),
            "github.com"
        );
        assert_eq!(extract_domain("http://docs.rs:8080/crate"), "docs.rs");
        assert_eq!(extract_domain("plasma://settings"), "plasma://settings");
        assert_eq!(extract_domain(""), "local");
    }

    #[test]
    fn tidy_items_by_domain_groups_active_workspace_unpinned_only() {
        let mut map = HashMap::new();
        map.insert(1, "ws-a".to_string());
        map.insert(2, "ws-a".to_string());
        map.insert(3, "ws-a".to_string());
        map.insert(4, "ws-b".to_string());

        let mut items = vec![
            make_test_item(10, "https://zeta.org", false, Some(PinStyle::Row)),
            make_test_item(1, "https://www.youtube.com/watch?v=1", false, None),
            make_test_item(2, "https://docs.rs/gtk", false, None),
            make_test_item(4, "https://alpha.example.com", false, None),
            make_test_item(3, "https://youtube.com/feed", true, None),
        ];

        tidy_items_by_domain(&mut items, Some("ws-a"), &map);
        let ids: Vec<u64> = items.iter().map(|i| i.id).collect();
        // Pinned (10) stays at 0; ws-b (4) stays at 3; ws-a unpinned sorted: docs.rs (2), youtube (1), youtube (3)
        assert_eq!(ids, vec![10, 2, 1, 4, 3]);
    }

    #[test]
    fn collect_clearable_tab_ids_skips_pinned_selected_and_other_workspaces() {
        let mut map = HashMap::new();
        map.insert(1, "ws-a".to_string());
        map.insert(2, "ws-a".to_string());
        map.insert(3, "ws-b".to_string());

        let items = vec![
            make_test_item(10, "https://pinned.example", false, Some(PinStyle::Square)),
            make_test_item(1, "https://a.example", true, None),
            make_test_item(2, "https://b.example", false, None),
            make_test_item(3, "https://c.example", false, None),
        ];

        let clearable = collect_clearable_tab_ids(&items, Some("ws-a"), &map);
        assert_eq!(clearable, vec![2]);
    }

    #[test]
    fn default_workspace_store_seeds_personal_trabajo_dev() {
        let store = default_workspace_store();
        let names: Vec<&str> = store.workspaces.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, vec!["Personal", "Trabajo", "Dev"]);
        assert_eq!(store.active().map(|w| w.name.as_str()), Some("Personal"));
        assert_eq!(space_accent_hex("purple"), "#9141ac");
        assert_eq!(space_accent_hex("unknown"), "#3584e4");
    }
}
