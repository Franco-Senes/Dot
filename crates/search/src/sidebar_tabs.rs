//! Barra lateral izquierda vertical de pestañas estilo Arc (`Side.swift` + `PinFolders.swift` + `PinDrop.swift`).
//!
//! Estructura auténtica Arc / Search enlazada al mismo `AdwTabView` de la ventana:
//! 1. Cuadrícula superior de Favoritos (hasta 4 cuadrados redondeados por fila con favicon/monograma).
//! 2. Sección de filas fijadas y carpetas de pines plegables (hasta 3 niveles de anidación con
//!    animación `GtkRevealer` `SlideDown`, creación/renombrado inline, menú contextual y DnD).
//! 3. Separador sutil + lista desplazable de pestañas regulares.
//! 4. Pie discreto con `+ Nueva pestaña` y botón compacto de `Ajustes (plasma://settings)`.

use crate::icons::{self, IconId};
use gnome_platform::gdk::{ContentProvider, DragAction, Rectangle, Texture};
use gnome_platform::gtk::{
    Align, AsEditable, AsWidget, BoxWidget, Button, DragSource, DropTarget, EllipsizeMode, Entry,
    GestureClick, Image, Label, ListBox, ListBoxRow, Orientation, PolicyType, Popover,
    PositionType, Revealer, RevealerTransitionType, ScrolledWindow, SelectionMode, Separator,
    Spinner,
};
use search_core::pin_folders::{PinFolder, PinFolderStore, MAX_PIN_FOLDER_DEPTH};
use search_core::session::PinStyle;
use search_core::settings::{AppSettings, TabGlyph};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

const PIN_FOLDERS_DEBOUNCE_MS: u64 = 1000;

// ============================================================================
// Gestor reactivo compartido de carpetas de pines (`pin-folders.json`)
// ============================================================================

type Listener = Rc<dyn Fn()>;

struct PinFoldersManagerInner {
    file_path: PathBuf,
    store: PinFolderStore,
    save_debounce: Cell<Option<gnome_platform::glib::SourceId>>,
    listeners: Vec<Listener>,
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

    pub fn subscribe(&self, listener: impl Fn() + 'static) {
        self.inner.borrow_mut().listeners.push(Rc::new(listener));
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
        let listeners: Vec<Listener> = self.inner.borrow().listeners.clone();
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
    pub on_context_menu: Option<ContextMenuCallback>,
    pub on_reorder: Option<Box<dyn Fn(u64, u64)>>,
    pub on_drop_tab_into_folder: Option<Box<dyn Fn(u64, String)>>,
    pub on_new_tab: Option<Box<dyn Fn()>>,
    pub on_open_settings: Option<Box<dyn Fn()>>,
    pub on_create_folder: Option<CreateFolderCallback>,
    pub on_rename_folder: Option<Box<dyn Fn(String, String)>>,
    pub on_delete_folder: Option<Box<dyn Fn(String)>>,
    pub on_toggle_folder_collapsed: Option<Box<dyn Fn(String, bool)>>,
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
    root: BoxWidget,
    favorites_box: BoxWidget,
    pinned_header: BoxWidget,
    folder_editor_box: BoxWidget,
    folder_editor_entry: Entry,
    folder_editor_mode: Rc<RefCell<Option<FolderEditorMode>>>,
    pinned_tree_box: BoxWidget,
    pinned_sep: Separator,
    list_box: ListBox,
    new_tab_icon: Image,
    settings_icon: Image,
    add_folder_icon: Image,
    folder_popover: Rc<RefCell<Option<Popover>>>,
    current_width: Rc<Cell<i32>>,
    motion_ms: Rc<Cell<u32>>,
    callbacks: Rc<RefCell<SidebarCallbacks>>,
}

impl SidebarTabs {
    pub fn new(width: i32) -> Self {
        let dark = icons::is_dark_theme();
        let clamped_width = width.clamp(
            AppSettings::SIDEBAR_WIDTH_MIN as i32,
            AppSettings::SIDEBAR_WIDTH_MAX as i32,
        );

        let root = BoxWidget::new(Orientation::Vertical, 4);
        root.add_css_class("search-sidebar");
        root.set_size_request(clamped_width, -1);
        root.set_vexpand(true);

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

        // 3. Lista desplazable de pestañas regulares (Hoy).
        let list_box = ListBox::new();
        list_box.add_css_class("search-sidebar-list");
        list_box.set_selection_mode(SelectionMode::None);
        list_box.set_vexpand(true);

        // Scrollable combinando filas fijadas y pestañas regulares si crecen mucho.
        let scroll_inner = BoxWidget::new(Orientation::Vertical, 3);
        scroll_inner.append(&pinned_header);
        scroll_inner.append(&folder_editor_box);
        scroll_inner.append(&pinned_tree_box);
        scroll_inner.append(&pinned_sep);
        scroll_inner.append(&list_box);

        let scrolled = ScrolledWindow::new();
        scrolled.set_policy(PolicyType::Never, PolicyType::Automatic);
        scrolled.set_vexpand(true);
        scrolled.set_child(Some(&scroll_inner));

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

        root.append(&favorites_box);
        root.append(&scrolled);
        root.append(&footer);

        let callbacks = Rc::new(RefCell::new(SidebarCallbacks::default()));
        let folder_editor_mode = Rc::new(RefCell::new(None));
        let folder_popover = Rc::new(RefCell::new(None));
        let motion_ms = Rc::new(Cell::new(180u32));

        let sidebar = Self {
            root,
            favorites_box,
            pinned_header,
            folder_editor_box,
            folder_editor_entry: folder_editor_entry.clone(),
            folder_editor_mode,
            pinned_tree_box,
            pinned_sep,
            list_box,
            new_tab_icon,
            settings_icon,
            add_folder_icon,
            folder_popover,
            current_width: Rc::new(Cell::new(clamped_width)),
            motion_ms,
            callbacks: callbacks.clone(),
        };

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
            let s = sidebar.clone();
            add_folder_btn.connect_clicked(move || {
                s.begin_create_folder(None, None);
            });
        }
        {
            let s = sidebar.clone();
            folder_ok_btn.connect_clicked(move || s.commit_folder_editor());
        }
        {
            let s = sidebar.clone();
            folder_editor_entry.connect_activate(move || s.commit_folder_editor());
        }
        {
            let s = sidebar.clone();
            folder_cancel_btn.connect_clicked(move || s.cancel_folder_editor());
        }

        // Clic derecho en el fondo vacío de la barra lateral -> Nueva carpeta / Nueva pestaña.
        {
            let bg_right = GestureClick::new();
            bg_right.set_button(3);
            let s = sidebar.clone();
            let anchor = sidebar.root.clone();
            bg_right.connect_pressed(move |_n, x, y| {
                s.open_sidebar_background_menu(&anchor, x, y);
            });
            sidebar.list_box.add_controller(&bg_right);
        }

        sidebar
    }

    pub fn widget(&self) -> &BoxWidget {
        &self.root
    }

    pub fn set_callbacks(&self, cbs: SidebarCallbacks) {
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
    }

    pub fn cleanup(&self) {
        if let Some(pop) = self.folder_popover.borrow_mut().take() {
            pop.popdown();
            pop.unparent();
        }
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
            let s = self.clone();
            let p = pop.clone();
            btn_folder.connect_clicked(move || {
                p.popdown();
                s.begin_create_folder(None, None);
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
            let s = self.clone();
            let fid = folder.id.clone();
            let p = pop.clone();
            sub_btn.connect_clicked(move || {
                p.popdown();
                s.begin_create_folder(Some(fid.clone()), None);
            });
            menu_box.append(&sub_btn);
        }

        let ren_btn = Button::with_label("Renombrar carpeta");
        ren_btn.add_css_class("flat");
        {
            let s = self.clone();
            let fid = folder.id.clone();
            let fname = folder.name.clone();
            let p = pop.clone();
            ren_btn.connect_clicked(move || {
                p.popdown();
                s.begin_rename_folder(fid.clone(), &fname);
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
        self.set_roomy(opts.roomy);
        self.set_motion_duration(opts.motion_ms);
        let dark = icons::is_dark_theme();
        self.refresh_theme_icons(dark);

        // Separar favoritos cuadrados, filas fijadas y pestañas regulares.
        let squares: Vec<&SidebarTabItem> = items
            .iter()
            .filter(|i| i.pin_style == Some(PinStyle::Square) && i.folder_id.is_none())
            .collect();
        let pinned_rows: Vec<&SidebarTabItem> = items
            .iter()
            .filter(|i| i.pin_style == Some(PinStyle::Row) || i.folder_id.is_some())
            .collect();
        let regular: Vec<&SidebarTabItem> = items.iter().filter(|i| !i.is_pinned()).collect();

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
        self.pinned_header.set_visible(true);
        rebuild_box_children(&self.pinned_tree_box, || {
            if !has_pinned_tree {
                return Vec::new();
            }
            let mut out = Vec::new();
            self.build_folder_level(None, 0, folders, &pinned_rows, opts, dark, &mut out);
            out
        });
        self.pinned_tree_box.set_visible(has_pinned_tree);
        self.pinned_sep
            .set_visible((!squares.is_empty() || has_pinned_tree) && !regular.is_empty());

        // 3. Reconstruir lista de pestañas regulares.
        self.list_box.remove_all();
        for item in regular {
            let row = build_tab_list_row(item, false, 0, opts, dark, &self.callbacks);
            self.list_box.append(&row);
        }
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
                let s = self.clone();
                let f_clone = folder.clone();
                let anchor = header_holder.clone();
                right.connect_pressed(move |_n, x, y| {
                    s.open_folder_context_menu(&f_clone, depth, &anchor, x, y);
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

    let title_label = Label::new(Some(&item.title));
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
        {
            let cbs = callbacks.clone();
            close_btn.connect_clicked(move || {
                if let Some(ref cb) = cbs.borrow().on_close {
                    cb(id);
                }
            });
        }
        hbox.append(&close_btn);
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
