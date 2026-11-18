//! Gestor reactivo de marcadores (`BookmarksManager`) con persistencia atómica + debounce (~1 s),
//! popover rápido (`Ctrl+D` / `Ctrl+Shift+B`) y panel lateral discreto (`Ctrl+Shift+O`) con
//! carpetas, búsqueda, edición, borrado, reordenación Drag & Drop e importación/exportación
//! HTML de marcadores Netscape (`Bookmarks.swift` + `BookmarksFile.swift`).

use gnome_platform::adw::ApplicationWindow;
use gnome_platform::gdk::{ContentProvider, DragAction};
use gnome_platform::gtk::{
    Align, AsEditable, AsWidget, BoxWidget, Button, DragSource, DropDown, DropTarget,
    EllipsizeMode, Entry, FileDialog, Image, Label, ListBox, ListBoxRow, MenuButton, Orientation,
    PolicyType, Popover, PositionType, ScrolledWindow, SearchEntry, SelectionMode, Separator,
};
use search_core::address::{display_at_rest, sanitize_url_for_ui};
use search_core::bookmarks::{BookmarkImportStats, BookmarkNode, BookmarkStore, FolderOption};
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

const BOOKMARKS_DEBOUNCE_MS: u64 = 1000;

type Listener = Rc<dyn Fn()>;
type OpenUrlCallback = Rc<dyn Fn(String, bool)>;
type CloseCallback = Rc<dyn Fn()>;

struct BookmarksManagerInner {
    file_path: PathBuf,
    store: BookmarkStore,
    save_debounce: Cell<Option<gnome_platform::glib::SourceId>>,
    listeners: Vec<Listener>,
}

/// Almacén compartido de marcadores para todas las ventanas de la aplicación.
#[derive(Clone)]
pub struct BookmarksManager {
    inner: Rc<RefCell<BookmarksManagerInner>>,
}

impl BookmarksManager {
    pub fn load(file_path: PathBuf) -> Self {
        let store = BookmarkStore::load(&file_path);
        Self {
            inner: Rc::new(RefCell::new(BookmarksManagerInner {
                file_path,
                store,
                save_debounce: Cell::new(None),
                listeners: Vec::new(),
            })),
        }
    }

    pub fn with_store<R>(&self, f: impl FnOnce(&BookmarkStore) -> R) -> R {
        let borrow = self.inner.borrow();
        f(&borrow.store)
    }

    pub fn find_by_url(&self, url: &str) -> Option<(BookmarkNode, Option<String>)> {
        self.inner.borrow().store.find_by_url(url)
    }

    pub fn folders(&self) -> Vec<FolderOption> {
        self.inner.borrow().store.folders()
    }

    pub fn add_bookmark(
        &self,
        title: &str,
        url: &str,
        parent_folder_id: Option<&str>,
    ) -> BookmarkNode {
        let node = self
            .inner
            .borrow_mut()
            .store
            .add_bookmark(title, url, parent_folder_id);
        self.schedule_save();
        self.notify_listeners();
        node
    }

    pub fn create_folder(&self, title: &str, parent_folder_id: Option<&str>) -> BookmarkNode {
        let node = self
            .inner
            .borrow_mut()
            .store
            .create_folder(title, parent_folder_id);
        self.schedule_save();
        self.notify_listeners();
        node
    }

    pub fn update_node(&self, id: &str, title: Option<String>, url: Option<String>) {
        if self.inner.borrow_mut().store.update_node(id, title, url) {
            self.schedule_save();
            self.notify_listeners();
        }
    }

    pub fn move_node(&self, id: &str, target_folder_id: Option<&str>, index: Option<usize>) {
        if self
            .inner
            .borrow_mut()
            .store
            .move_node(id, target_folder_id, index)
        {
            self.schedule_save();
            self.notify_listeners();
        }
    }

    pub fn reorder_before(&self, src_id: &str, dst_id: &str) {
        if self.inner.borrow_mut().store.reorder_before(src_id, dst_id) {
            self.schedule_save();
            self.notify_listeners();
        }
    }

    pub fn remove(&self, id: &str) {
        if self.inner.borrow_mut().store.remove(id) {
            self.schedule_save();
            self.notify_listeners();
        }
    }

    pub fn export_netscape_html(&self) -> String {
        self.inner.borrow().store.export_netscape_html()
    }

    pub fn import_netscape_html(&self, html: &str) -> BookmarkImportStats {
        let stats = self.inner.borrow_mut().store.import_netscape_html(html);
        if stats.added > 0 {
            self.schedule_save();
            self.notify_listeners();
        }
        stats
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
                "No se pudieron guardar los marcadores en '{}': {err:#}",
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
            Duration::from_millis(BOOKMARKS_DEBOUNCE_MS),
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

/// Abre el popover discreto de `Ctrl+D` / `Ctrl+Shift+B` anclado a `anchor` para añadir,
/// editar título/carpeta o quitar el marcador de la página actual.
pub fn open_bookmark_popover(
    anchor: &impl AsWidget,
    bookmarks: &BookmarksManager,
    url: &str,
    page_title: &str,
) -> Option<Popover> {
    let clean_url = sanitize_url_for_ui(url);
    if clean_url.is_empty()
        || clean_url == "about:blank"
        || clean_url.starts_with("data:text/html;charset=utf-8,<!--search-internal-->")
    {
        return None;
    }

    let (node, current_parent, header_text) = match bookmarks.find_by_url(&clean_url) {
        Some((existing, parent)) => (existing, parent, "Editar marcador"),
        None => {
            let added = bookmarks.add_bookmark(page_title, &clean_url, None);
            (added, None, "Marcador guardado")
        }
    };

    let pop = Popover::new();
    pop.set_parent(anchor);
    pop.set_position(PositionType::Bottom);
    pop.set_has_arrow(true);
    pop.set_autohide(true);

    let root = BoxWidget::new(Orientation::Vertical, 8);
    root.set_margins(12, 12, 10, 10);
    root.set_size_request(300, -1);

    let heading = Label::new(Some(header_text));
    heading.add_css_class("search-panel-title");
    heading.set_xalign(0.0);
    root.append(&heading);

    let title_lbl = Label::new(Some("Nombre"));
    title_lbl.add_css_class("search-panel-subtitle");
    title_lbl.set_xalign(0.0);
    root.append(&title_lbl);

    let title_entry = Entry::new();
    title_entry.set_text(&node.title);
    title_entry.set_hexpand(true);
    root.append(&title_entry);

    let folder_lbl = Label::new(Some("Carpeta"));
    folder_lbl.add_css_class("search-panel-subtitle");
    folder_lbl.set_xalign(0.0);
    root.append(&folder_lbl);

    let folders = bookmarks.folders();
    let mut folder_labels: Vec<String> = vec!["Barra principal (Raíz)".to_string()];
    let mut folder_ids: Vec<Option<String>> = vec![None];
    let mut selected_idx: u32 = 0;

    for f in &folders {
        let indent = "  ".repeat(f.depth);
        folder_labels.push(format!("{indent}{}", f.title));
        if current_parent.as_deref() == Some(f.id.as_str()) {
            selected_idx = (folder_ids.len()) as u32;
        }
        folder_ids.push(Some(f.id.clone()));
    }
    let label_refs: Vec<&str> = folder_labels.iter().map(String::as_str).collect();
    let folder_dd = DropDown::from_strings(&label_refs);
    folder_dd.set_selected(selected_idx);
    folder_dd.set_hexpand(true);
    root.append(&folder_dd);

    let btn_row = BoxWidget::new(Orientation::Horizontal, 8);
    btn_row.set_margins(0, 0, 4, 0);

    let remove_btn = Button::with_label("Eliminar");
    remove_btn.add_css_class("destructive-action");
    {
        let mgr = bookmarks.clone();
        let node_id = node.id.clone();
        let pop_ref = pop.clone();
        remove_btn.connect_clicked(move || {
            mgr.remove(&node_id);
            pop_ref.popdown();
        });
    }
    btn_row.append(&remove_btn);

    let spacer = BoxWidget::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    btn_row.append(&spacer);

    let save_and_close = Rc::new({
        let mgr = bookmarks.clone();
        let node_id = node.id.clone();
        let entry_ref = title_entry.clone();
        let dd_ref = folder_dd.clone();
        let ids_ref = folder_ids;
        let orig_parent = current_parent;
        let pop_ref = pop.clone();
        move || {
            let new_title = entry_ref.text();
            mgr.update_node(&node_id, Some(new_title), None);
            let idx = dd_ref.selected() as usize;
            if let Some(target_folder) = ids_ref.get(idx) {
                if target_folder != &orig_parent {
                    mgr.move_node(&node_id, target_folder.as_deref(), None);
                }
            }
            pop_ref.popdown();
        }
    });

    {
        let cb = save_and_close.clone();
        title_entry.connect_activate(move || cb());
    }

    let done_btn = Button::with_label("Listo");
    done_btn.add_css_class("suggested-action");
    {
        let cb = save_and_close.clone();
        done_btn.connect_clicked(move || cb());
    }
    btn_row.append(&done_btn);
    root.append(&btn_row);

    pop.set_child(Some(&root));
    pop.popup();
    title_entry.grab_focus();
    title_entry.select_all();
    Some(pop)
}

/// Panel lateral discreto de Marcadores (`Ctrl+Shift+O`).
#[derive(Clone)]
pub struct BookmarksPanel {
    root: BoxWidget,
    search_entry: SearchEntry,
    new_folder_box: BoxWidget,
    new_folder_entry: Entry,
    edit_box: BoxWidget,
    edit_title_entry: Entry,
    edit_url_entry: Entry,
    status_label: Label,
    list_box: ListBox,
    empty_label: Label,
    manager: BookmarksManager,
    parent_window: ApplicationWindow,
    on_open_url: OpenUrlCallback,
    expanded: Rc<RefCell<HashSet<String>>>,
    editing_id: Rc<RefCell<Option<String>>>,
    editing_is_folder: Rc<Cell<bool>>,
    visible: Rc<Cell<bool>>,
}

impl BookmarksPanel {
    pub fn new(
        parent_window: ApplicationWindow,
        manager: BookmarksManager,
        on_open_url: impl Fn(String, bool) + 'static,
        on_close: impl Fn() + 'static,
    ) -> Self {
        let on_open_url: OpenUrlCallback = Rc::new(on_open_url);
        let on_close: CloseCallback = Rc::new(on_close);
        let expanded = Rc::new(RefCell::new(HashSet::new()));
        let editing_id = Rc::new(RefCell::new(None));
        let editing_is_folder = Rc::new(Cell::new(false));
        let visible = Rc::new(Cell::new(false));

        let root = BoxWidget::new(Orientation::Vertical, 6);
        root.add_css_class("search-side-panel");
        root.set_hexpand(true);
        root.set_vexpand(true);

        // Cabecera del panel.
        let header = BoxWidget::new(Orientation::Horizontal, 4);
        header.set_margins(8, 8, 10, 4);

        let title = Label::new(Some("Marcadores"));
        title.add_css_class("search-panel-title");
        title.set_xalign(0.0);
        title.set_hexpand(true);
        header.append(&title);

        let add_folder_btn = Button::from_icon_name("folder-new-symbolic");
        add_folder_btn.add_css_class("flat");
        add_folder_btn.set_tooltip_text(Some("Nueva carpeta"));
        header.append(&add_folder_btn);

        // Menú Importar / Exportar Netscape HTML.
        let io_pop = Popover::new();
        let io_box = BoxWidget::new(Orientation::Vertical, 2);
        io_box.set_margins(4, 4, 4, 4);

        let import_btn = Button::with_label("Importar HTML (Firefox/Chrome)…");
        import_btn.add_css_class("flat");
        io_box.append(&import_btn);

        let export_btn = Button::with_label("Exportar HTML Netscape…");
        export_btn.add_css_class("flat");
        io_box.append(&export_btn);
        io_pop.set_child(Some(&io_box));

        let io_menu_btn = MenuButton::new();
        io_menu_btn.set_icon_name("view-more-symbolic");
        io_menu_btn.add_css_class("flat");
        io_menu_btn.set_tooltip_text(Some("Importar o exportar marcadores HTML"));
        io_menu_btn.set_popover(Some(&io_pop));
        header.append(&io_menu_btn);

        let close_btn = Button::from_icon_name("window-close-symbolic");
        close_btn.add_css_class("flat");
        close_btn.set_tooltip_text(Some("Cerrar panel (Esc)"));
        {
            let cb = on_close.clone();
            close_btn.connect_clicked(move || cb());
        }
        header.append(&close_btn);
        root.append(&header);

        // Barra inline para crear nueva carpeta (oculta por defecto).
        let new_folder_box = BoxWidget::new(Orientation::Horizontal, 4);
        new_folder_box.set_margins(8, 8, 0, 2);
        new_folder_box.set_visible(false);

        let new_folder_entry = Entry::new();
        new_folder_entry.set_placeholder_text("Nombre de la carpeta");
        new_folder_entry.set_hexpand(true);
        new_folder_box.append(&new_folder_entry);

        let confirm_folder_btn = Button::with_label("Crear");
        confirm_folder_btn.add_css_class("suggested-action");
        new_folder_box.append(&confirm_folder_btn);
        root.append(&new_folder_box);

        // Barra inline para editar marcador o carpeta (oculta por defecto).
        let edit_box = BoxWidget::new(Orientation::Vertical, 4);
        edit_box.add_css_class("search-panel-editor");
        edit_box.set_margins(8, 8, 2, 4);
        edit_box.set_visible(false);

        let edit_title_entry = Entry::new();
        edit_title_entry.set_placeholder_text("Nombre");
        edit_box.append(&edit_title_entry);

        let edit_url_entry = Entry::new();
        edit_url_entry.set_placeholder_text("https://…");
        edit_box.append(&edit_url_entry);

        let edit_actions = BoxWidget::new(Orientation::Horizontal, 6);
        let edit_cancel = Button::with_label("Cancelar");
        edit_cancel.add_css_class("flat");
        let edit_save = Button::with_label("Guardar");
        edit_save.add_css_class("suggested-action");
        edit_actions.append(&edit_cancel);
        edit_actions.append(&edit_save);
        edit_box.append(&edit_actions);
        root.append(&edit_box);

        let status_label = Label::new(None);
        status_label.add_css_class("search-panel-subtitle");
        status_label.set_xalign(0.0);
        status_label.set_margins(10, 10, 0, 0);
        status_label.set_visible(false);
        root.append(&status_label);

        let search_entry = SearchEntry::new();
        search_entry.set_placeholder_text("Buscar marcadores…");
        search_entry.set_margins(8, 8, 0, 4);
        root.append(&search_entry);

        root.append(&Separator::new(Orientation::Horizontal));

        let list_box = ListBox::new();
        list_box.add_css_class("search-panel-list");
        list_box.set_selection_mode(SelectionMode::None);
        list_box.set_activate_on_single_click(false);

        let empty_label = Label::new(Some(
            "No hay marcadores guardados\nUsa Ctrl+D para añadir esta página",
        ));
        empty_label.add_css_class("search-panel-empty");
        empty_label.set_margins(16, 16, 28, 16);
        empty_label.set_halign(Align::Center);
        empty_label.set_valign(Align::Start);

        let list_container = BoxWidget::new(Orientation::Vertical, 0);
        list_container.append(&empty_label);
        list_container.append(&list_box);

        let scroller = ScrolledWindow::new();
        scroller.set_policy(PolicyType::Never, PolicyType::Automatic);
        scroller.set_hexpand(true);
        scroller.set_vexpand(true);
        scroller.set_child(Some(&list_container));
        root.append(&scroller);

        let panel = Self {
            root,
            search_entry: search_entry.clone(),
            new_folder_box: new_folder_box.clone(),
            new_folder_entry: new_folder_entry.clone(),
            edit_box: edit_box.clone(),
            edit_title_entry: edit_title_entry.clone(),
            edit_url_entry: edit_url_entry.clone(),
            status_label: status_label.clone(),
            list_box,
            empty_label,
            manager: manager.clone(),
            parent_window: parent_window.clone(),
            on_open_url,
            expanded,
            editing_id,
            editing_is_folder,
            visible,
        };

        // Cableado: nueva carpeta.
        {
            let p = panel.clone();
            add_folder_btn.connect_clicked(move || {
                let show = !p.new_folder_box.is_visible();
                p.new_folder_box.set_visible(show);
                if show {
                    p.new_folder_entry.set_text("");
                    p.new_folder_entry.grab_focus();
                }
            });
        }
        let create_folder_action = Rc::new({
            let p = panel.clone();
            move || {
                let name = p.new_folder_entry.text().trim().to_string();
                if !name.is_empty() {
                    let created = p.manager.create_folder(&name, None);
                    p.expanded.borrow_mut().insert(created.id);
                    p.new_folder_entry.set_text("");
                    p.new_folder_box.set_visible(false);
                }
            }
        });
        {
            let cb = create_folder_action.clone();
            confirm_folder_btn.connect_clicked(move || cb());
        }
        {
            let cb = create_folder_action;
            new_folder_entry.connect_activate(move || cb());
        }

        // Cableado: editor inline.
        {
            let p = panel.clone();
            edit_cancel.connect_clicked(move || {
                *p.editing_id.borrow_mut() = None;
                p.edit_box.set_visible(false);
            });
        }
        let save_edit_action = Rc::new({
            let p = panel.clone();
            move || {
                let id_opt = p.editing_id.borrow_mut().take();
                if let Some(id) = id_opt {
                    let new_title = p.edit_title_entry.text();
                    let new_url = if p.editing_is_folder.get() {
                        None
                    } else {
                        Some(p.edit_url_entry.text())
                    };
                    p.manager.update_node(&id, Some(new_title), new_url);
                }
                p.edit_box.set_visible(false);
            }
        });
        {
            let cb = save_edit_action.clone();
            edit_save.connect_clicked(move || cb());
        }
        {
            let cb = save_edit_action.clone();
            edit_title_entry.connect_activate(move || cb());
        }
        {
            let cb = save_edit_action;
            edit_url_entry.connect_activate(move || cb());
        }

        // Cableado: importar HTML Netscape.
        {
            let p = panel.clone();
            let pop = io_pop.clone();
            import_btn.connect_clicked(move || {
                pop.popdown();
                let dlg = FileDialog::new();
                dlg.set_title("Importar marcadores HTML (Netscape)");
                let p_done = p.clone();
                dlg.open(Some(&p.parent_window), move |opt_path| {
                    let Some(path) = opt_path else { return };
                    match fs::read_to_string(&path) {
                        Ok(html) => {
                            let stats = p_done.manager.import_netscape_html(&html);
                            p_done.show_status(&format!(
                                "Importados {} marcadores ({} omitidos)",
                                stats.added, stats.skipped
                            ));
                        }
                        Err(err) => {
                            search_core::warn!("Error al leer archivo HTML de marcadores: {err}");
                            p_done.show_status("No se pudo leer el archivo seleccionado");
                        }
                    }
                });
            });
        }

        // Cableado: exportar HTML Netscape.
        {
            let p = panel.clone();
            let pop = io_pop.clone();
            export_btn.connect_clicked(move || {
                pop.popdown();
                let dlg = FileDialog::new();
                dlg.set_title("Exportar marcadores HTML (Netscape)");
                dlg.set_initial_name("bookmarks.html");
                let p_done = p.clone();
                dlg.save(Some(&p.parent_window), move |opt_path| {
                    let Some(path) = opt_path else { return };
                    let html = p_done.manager.export_netscape_html();
                    match search_core::paths::write_atomic(&path, html.as_bytes()) {
                        Ok(()) => {
                            p_done.show_status("Marcadores exportados correctamente");
                        }
                        Err(err) => {
                            search_core::warn!("Error al exportar marcadores HTML: {err:#}");
                            p_done.show_status("No se pudo guardar el archivo HTML");
                        }
                    }
                });
            });
        }

        // Búsqueda y refresco reactivo.
        {
            let p = panel.clone();
            search_entry.connect_search_changed(move || p.refresh());
        }
        {
            let p = panel.clone();
            manager.subscribe(move || {
                if p.visible.get() {
                    p.refresh();
                }
            });
        }

        panel
    }

    pub fn widget(&self) -> &BoxWidget {
        &self.root
    }

    pub fn set_panel_visible(&self, shown: bool) {
        self.visible.set(shown);
        if shown {
            self.refresh();
            self.search_entry.grab_focus();
        }
    }

    fn show_status(&self, msg: &str) {
        self.status_label.set_text(msg);
        self.status_label.set_visible(true);
    }

    fn start_editing(&self, node: &BookmarkNode) {
        *self.editing_id.borrow_mut() = Some(node.id.clone());
        self.editing_is_folder.set(node.is_folder());
        self.edit_title_entry.set_text(&node.title);
        if let Some(ref u) = node.url {
            self.edit_url_entry.set_text(u);
            self.edit_url_entry.set_visible(true);
        } else {
            self.edit_url_entry.set_text("");
            self.edit_url_entry.set_visible(false);
        }
        self.edit_box.set_visible(true);
        self.edit_title_entry.grab_focus();
        self.edit_title_entry.select_all();
    }

    pub fn refresh(&self) {
        self.list_box.remove_all();
        let query = self.search_entry.text();
        let trimmed = query.trim();

        if !trimmed.is_empty() {
            let matches = self.manager.with_store(|s| s.search(trimmed, true, 200));
            self.empty_label.set_visible(matches.is_empty());
            if matches.is_empty() {
                self.empty_label.set_text("Sin marcadores coincidentes");
            }
            for m in matches {
                let row = self.build_node_row(&m.node, 0, Some(&m.folder_path));
                self.list_box.append(&row);
            }
            return;
        }

        let roots = self.manager.with_store(|s| s.roots().to_vec());
        if roots.is_empty() {
            self.empty_label
                .set_text("No hay marcadores guardados\nUsa Ctrl+D para añadir esta página");
            self.empty_label.set_visible(true);
            return;
        }
        self.empty_label.set_visible(false);
        self.append_tree_nodes(&roots, 0);
    }

    fn append_tree_nodes(&self, nodes: &[BookmarkNode], depth: usize) {
        for node in nodes {
            let row = self.build_node_row(node, depth, None);
            self.list_box.append(&row);
            if node.is_folder() && self.expanded.borrow().contains(&node.id) {
                if let Some(ref kids) = node.children {
                    self.append_tree_nodes(kids, depth + 1);
                }
            }
        }
    }

    fn build_node_row(
        &self,
        node: &BookmarkNode,
        depth: usize,
        search_path: Option<&[String]>,
    ) -> ListBoxRow {
        let row = ListBoxRow::new();
        row.add_css_class("search-panel-row");
        row.set_selectable(false);
        row.set_activatable(false);

        let hbox = BoxWidget::new(Orientation::Horizontal, 6);
        let left_margin = 8 + (depth.min(6) as i32) * 14;
        hbox.set_margins(left_margin, 8, 4, 4);

        let is_folder = node.is_folder();
        let is_open = is_folder && self.expanded.borrow().contains(&node.id);

        let main_btn = Button::with_label("");
        main_btn.add_css_class("flat");
        main_btn.add_css_class("search-panel-item-btn");
        main_btn.set_hexpand(true);

        let btn_inner = BoxWidget::new(Orientation::Horizontal, 6);
        let icon = Image::from_icon_name(if is_folder {
            if is_open {
                "folder-open-symbolic"
            } else {
                "folder-symbolic"
            }
        } else {
            "starred-symbolic"
        });
        icon.set_pixel_size(15);
        btn_inner.append(&icon);

        let text_col = BoxWidget::new(Orientation::Vertical, 1);
        text_col.set_hexpand(true);

        let label_text = node.display_label();
        let title_lbl = Label::new(Some(&label_text));
        title_lbl.set_xalign(0.0);
        title_lbl.set_ellipsize(EllipsizeMode::End);
        text_col.append(&title_lbl);

        if let Some(ref url) = node.url {
            let mut sub = display_at_rest(url);
            if let Some(path) = search_path {
                if !path.is_empty() {
                    sub = format!("{} · {sub}", path.join(" / "));
                }
            }
            let sub_lbl = Label::new(Some(&sub));
            sub_lbl.add_css_class("search-panel-subtitle");
            sub_lbl.set_xalign(0.0);
            sub_lbl.set_ellipsize(EllipsizeMode::End);
            text_col.append(&sub_lbl);
        } else if let Some(ref kids) = node.children {
            let sub_lbl = Label::new(Some(&format!("{} elementos", kids.len())));
            sub_lbl.add_css_class("search-panel-subtitle");
            sub_lbl.set_xalign(0.0);
            text_col.append(&sub_lbl);
        }

        btn_inner.append(&text_col);
        main_btn.set_child(Some(&btn_inner));

        if is_folder {
            let p = self.clone();
            let fid = node.id.clone();
            main_btn.connect_clicked(move || {
                {
                    let mut exp = p.expanded.borrow_mut();
                    if !exp.remove(&fid) {
                        exp.insert(fid.clone());
                    }
                }
                p.refresh();
            });
        } else if let Some(ref url) = node.url {
            main_btn.set_tooltip_text(Some(url));
            let cb = self.on_open_url.clone();
            let u = url.clone();
            main_btn.connect_clicked(move || {
                cb(u.clone(), false);
            });
        }
        hbox.append(&main_btn);

        // Editar título/URL.
        let edit_btn = Button::from_icon_name("document-edit-symbolic");
        edit_btn.add_css_class("flat");
        edit_btn.add_css_class("search-tab-mini-btn");
        edit_btn.set_tooltip_text(Some("Editar"));
        {
            let p = self.clone();
            let node_clone = node.clone();
            edit_btn.connect_clicked(move || p.start_editing(&node_clone));
        }
        hbox.append(&edit_btn);

        // Borrar marcador o carpeta.
        let del_btn = Button::from_icon_name("user-trash-symbolic");
        del_btn.add_css_class("flat");
        del_btn.add_css_class("search-tab-mini-btn");
        del_btn.set_tooltip_text(Some("Eliminar"));
        {
            let mgr = self.manager.clone();
            let nid = node.id.clone();
            del_btn.connect_clicked(move || mgr.remove(&nid));
        }
        hbox.append(&del_btn);

        // Drag & Drop para reordenar o mover a una carpeta.
        let drag_src = DragSource::new();
        drag_src.set_actions(DragAction::MOVE);
        {
            let drag_id = node.id.clone();
            drag_src.connect_prepare(move |_x, _y| Some(ContentProvider::for_string(&drag_id)));
        }
        row.add_controller(&drag_src);

        let drop_target = DropTarget::for_string(DragAction::MOVE);
        {
            let mgr = self.manager.clone();
            let dst_id = node.id.clone();
            let dst_is_folder = is_folder;
            let expanded = self.expanded.clone();
            drop_target.connect_drop_string(move |src_id, _x, _y| {
                if src_id.is_empty() || src_id == dst_id {
                    return false;
                }
                if dst_is_folder {
                    mgr.move_node(&src_id, Some(&dst_id), None);
                    expanded.borrow_mut().insert(dst_id.clone());
                } else {
                    mgr.reorder_before(&src_id, &dst_id);
                }
                true
            });
        }
        row.add_controller(&drop_target);

        row.set_child(Some(&hbox));
        row
    }
}
