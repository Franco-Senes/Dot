//! Gestor reactivo de `Curtain` (`curtain.json` con persistencia atómica y debounce),
//! inyección del colector mínimo `resources/curtain.js` y panel lateral discreto (`Ctrl+Shift+E`)
//! para inspeccionar, previsualizar al pasar el ratón y restaurar reglas por sitio o globales
//! (`Curtain.swift` + `Hidden.swift`).

use crate::icons::{self, IconId};
use dot_core::curtain::{
    generate_selector, CurtainImportStats, CurtainStore, ElementPickSnapshot, Veil,
};
use dot_core::session::now_unix_secs;
use dot_core::shield::js_string_literal;
use gnome_platform::gtk::{
    Align, AsEditable, AsWidget, BoxWidget, Button, EllipsizeMode, EventControllerMotion,
    FileDialog, Label, ListBox, ListBoxRow, MenuButton, Orientation, PolicyType, Popover,
    PositionType, ScrolledWindow, SearchEntry, SelectionMode, Separator,
};
use std::cell::{Cell, RefCell};
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

const CURTAIN_DEBOUNCE_MS: u64 = 400;

/// Colector DOM mínimo en JS incluido con `include_str!` (la generación de selectores vive en Rust).
const CURTAIN_PICKER_JS: &str = include_str!("../resources/curtain.js");

/// Construye el script inyectable de Curtain autenticando su canal IPC con `ipc_nonce`.
pub fn build_curtain_picker_script(ipc_nonce: &str) -> String {
    let prefix = js_string_literal(&format!("__search_ipc_{ipc_nonce}:"));
    CURTAIN_PICKER_JS.replace("__SEARCH_IPC_PREFIX__", &prefix)
}

/// Script breve para desactivar el modo selección en la página sin emitir evento de vuelta.
pub fn build_curtain_stop_script() -> &'static str {
    "try{if(window.__searchCurtain&&window.__searchCurtain.off){window.__searchCurtain.off();}}catch(_){}"
}

type Listener = Rc<dyn Fn()>;
type VoidCb = Rc<dyn Fn()>;
type HostSelectorCb = Rc<dyn Fn(String, String)>;
type HostCb = Rc<dyn Fn(String)>;

struct CurtainManagerInner {
    file_path: PathBuf,
    store: CurtainStore,
    save_debounce: Cell<Option<gnome_platform::glib::SourceId>>,
    listeners: Vec<Listener>,
}

/// Almacén compartido de reglas Curtain para todas las ventanas.
#[derive(Clone)]
pub struct CurtainManager {
    inner: Rc<RefCell<CurtainManagerInner>>,
}

impl CurtainManager {
    pub fn load(file_path: PathBuf) -> Self {
        let store = CurtainStore::load(&file_path);
        Self {
            inner: Rc::new(RefCell::new(CurtainManagerInner {
                file_path,
                store,
                save_debounce: Cell::new(None),
                listeners: Vec::new(),
            })),
        }
    }

    pub fn host_for_url(url: &str) -> Option<String> {
        CurtainStore::host_for_url(url)
    }

    /// Genera el selector CSS en Rust puro desde `snapshot` y lo persiste para `host`.
    pub fn hide_from_snapshot(&self, host: &str, snapshot: &ElementPickSnapshot) -> Option<String> {
        let selector = generate_selector(snapshot)?;
        let label = if snapshot.label.trim().is_empty() {
            selector.clone()
        } else {
            snapshot.label.trim().to_string()
        };
        let added = self.inner.borrow_mut().store.hide(
            host,
            selector.clone(),
            label,
            snapshot.note.clone(),
            now_unix_secs(),
        );
        if added {
            self.schedule_save();
            self.notify_listeners();
        }
        Some(selector)
    }

    pub fn restore(&self, host: &str, selector: &str) -> bool {
        let removed = self.inner.borrow_mut().store.restore(host, selector);
        if removed {
            self.schedule_save();
            self.notify_listeners();
        }
        removed
    }

    pub fn set_rule_enabled(&self, host: &str, selector: &str, enabled: bool) -> bool {
        let changed = self
            .inner
            .borrow_mut()
            .store
            .set_enabled(host, selector, enabled);
        if changed {
            self.schedule_save();
            self.notify_listeners();
        }
        changed
    }

    pub fn export_json(&self) -> anyhow::Result<String> {
        self.inner.borrow().store.export_json()
    }

    pub fn import_json(&self, raw: &str) -> anyhow::Result<CurtainImportStats> {
        let stats = self.inner.borrow_mut().store.import_json(raw)?;
        if stats.added > 0 {
            self.schedule_save();
            self.notify_listeners();
        }
        Ok(stats)
    }

    pub fn undo(&self, host: &str) -> Option<Veil> {
        let undone = self.inner.borrow_mut().store.undo(host);
        if undone.is_some() {
            self.schedule_save();
            self.notify_listeners();
        }
        undone
    }

    pub fn restore_all(&self, host: &str) -> usize {
        let n = self.inner.borrow_mut().store.restore_all(host);
        if n > 0 {
            self.schedule_save();
            self.notify_listeners();
        }
        n
    }

    pub fn clear_all(&self) -> usize {
        let n = self.inner.borrow_mut().store.clear_all();
        if n > 0 {
            self.schedule_save();
            self.notify_listeners();
        }
        n
    }

    pub fn veils_for_host(&self, host: &str) -> Vec<Veil> {
        self.inner.borrow().store.veils(host).to_vec()
    }

    pub fn all_entries(&self) -> Vec<(String, Vec<Veil>)> {
        self.inner
            .borrow()
            .store
            .by_host
            .iter()
            .map(|(h, v)| (h.clone(), v.clone()))
            .collect()
    }

    pub fn css_for_host(&self, host: &str, spared: Option<&str>) -> String {
        self.inner.borrow().store.css_for_host(host, spared)
    }

    pub fn css_for_url(&self, url: &str) -> Option<String> {
        let host = CurtainStore::host_for_url(url)?;
        let css = self.inner.borrow().store.css_for_host(&host, None);
        if css.is_empty() {
            None
        } else {
            Some(css)
        }
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
            dot_core::warn!(
                "No se pudo guardar Curtain en '{}': {err:#}",
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
            Duration::from_millis(CURTAIN_DEBOUNCE_MS),
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

/// Callbacks de interacción entre el panel Curtain y la ventana activa.
pub struct CurtainPanelCallbacks {
    pub on_start_pick: VoidCb,
    pub on_peek: HostSelectorCb,
    pub on_unpeek: HostCb,
    pub on_rules_changed: HostCb,
    pub on_close: VoidCb,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CurtainScope {
    CurrentSite,
    AllSites,
}

/// Panel lateral discreto de elementos ocultos (`Ctrl+Shift+E`).
#[derive(Clone)]
pub struct CurtainPanel {
    root: BoxWidget,
    site_caption: Label,
    status_label: Label,
    search_entry: SearchEntry,
    btn_site: Button,
    btn_all: Button,
    btn_undo: Button,
    btn_restore_all: Button,
    list_box: ListBox,
    empty_box: BoxWidget,
    empty_title: Label,
    empty_desc: Label,
    empty_pick_btn: Button,
    empty_clear_filter_btn: Button,
    manager: CurtainManager,
    callbacks: Rc<CurtainPanelCallbacks>,
    current_host: Rc<RefCell<Option<String>>>,
    scope: Rc<Cell<CurtainScope>>,
    visible: Rc<Cell<bool>>,
}

impl CurtainPanel {
    pub fn new(manager: CurtainManager, callbacks: CurtainPanelCallbacks) -> Self {
        let callbacks = Rc::new(callbacks);
        let current_host = Rc::new(RefCell::new(None));
        let scope = Rc::new(Cell::new(CurtainScope::CurrentSite));
        let visible = Rc::new(Cell::new(false));

        let root = BoxWidget::new(Orientation::Vertical, 6);
        root.add_css_class("search-side-panel");
        root.set_hexpand(true);
        root.set_vexpand(true);

        // Cabecera compacta con menú Exportar/Importar y botón cerrar.
        let header = BoxWidget::new(Orientation::Horizontal, 4);
        header.set_margins(8, 8, 10, 4);

        let title = Label::new(Some("Elementos ocultos"));
        title.add_css_class("search-panel-title");
        title.set_xalign(0.0);
        title.set_hexpand(true);
        header.append(&title);

        let io_pop = Popover::new();
        io_pop.set_position(PositionType::Bottom);
        let io_box = BoxWidget::new(Orientation::Vertical, 4);
        io_box.set_margins(6, 6, 6, 6);
        let export_btn = Button::with_label("Exportar reglas (JSON)…");
        export_btn.add_css_class("flat");
        let import_btn = Button::with_label("Importar reglas (JSON)…");
        import_btn.add_css_class("flat");
        io_box.append(&export_btn);
        io_box.append(&import_btn);
        io_pop.set_child(Some(&io_box));

        let io_menu_btn = MenuButton::new();
        io_menu_btn.add_css_class("flat");
        io_menu_btn.set_child(Some(&icons::image(IconId::MoreVert, 15)));
        io_menu_btn.set_tooltip_text(Some("Exportar o importar reglas JSON"));
        io_menu_btn.set_popover(Some(&io_pop));
        header.append(&io_menu_btn);

        let close_btn = icons::icon_button(IconId::Close, 15);
        close_btn.set_tooltip_text(Some("Cerrar panel (Esc)"));
        {
            let cbs = callbacks.clone();
            close_btn.connect_clicked(move || (cbs.on_close)());
        }
        header.append(&close_btn);
        root.append(&header);

        // Controles de acción rápida: Ocultar elemento + Deshacer último bloqueo.
        let action_bar = BoxWidget::new(Orientation::Vertical, 6);
        action_bar.set_margins(0, 4, 8, 8);

        let pick_btn = Button::with_label("Ocultar elemento…");
        pick_btn.add_css_class("suggested-action");
        pick_btn.set_hexpand(true);
        pick_btn.set_tooltip_text(Some("Seleccionar un elemento en la página (Ctrl+Shift+H)"));
        {
            let cbs = callbacks.clone();
            pick_btn.connect_clicked(move || (cbs.on_start_pick)());
        }
        action_bar.append(&pick_btn);

        let btn_undo = Button::new();
        btn_undo.add_css_class("flat");
        let undo_content = BoxWidget::new(Orientation::Horizontal, 6);
        undo_content.set_halign(Align::Center);
        undo_content.append(&icons::image(IconId::Undo, 14));
        undo_content.append(&Label::new(Some("Deshacer último bloqueo")));
        btn_undo.set_child(Some(&undo_content));
        btn_undo.set_hexpand(true);
        btn_undo.set_tooltip_text(Some("Deshacer el último elemento ocultado en este sitio"));
        action_bar.append(&btn_undo);
        root.append(&action_bar);

        // Conmutador de ámbito: Este sitio / Todos los sitios.
        let scope_bar = BoxWidget::new(Orientation::Horizontal, 4);
        scope_bar.set_margins(0, 4, 8, 8);

        let btn_site = Button::with_label("Este sitio");
        btn_site.set_hexpand(true);
        scope_bar.append(&btn_site);

        let btn_all = Button::with_label("Todos los sitios");
        btn_all.set_hexpand(true);
        scope_bar.append(&btn_all);
        root.append(&scope_bar);

        // Filtro de búsqueda de reglas.
        let search_entry = SearchEntry::new();
        search_entry.set_placeholder_text("Buscar por selector o etiqueta…");
        search_entry.set_margins(0, 4, 8, 8);
        root.append(&search_entry);

        let site_caption = Label::new(None);
        site_caption.add_css_class("search-panel-subtitle");
        site_caption.set_xalign(0.0);
        site_caption.set_ellipsize(EllipsizeMode::End);
        site_caption.set_margins(0, 4, 10, 10);
        root.append(&site_caption);

        let status_label = Label::new(None);
        status_label.add_css_class("search-panel-subtitle");
        status_label.set_xalign(0.0);
        status_label.set_wrap(true);
        status_label.set_margins(0, 4, 10, 10);
        status_label.set_visible(false);
        root.append(&status_label);

        root.append(&Separator::new(Orientation::Horizontal));

        let list_box = ListBox::new();
        list_box.add_css_class("search-panel-list");
        list_box.set_selection_mode(SelectionMode::None);
        list_box.set_activate_on_single_click(false);

        // Estado vacío orientado a la acción.
        let empty_box = BoxWidget::new(Orientation::Vertical, 8);
        empty_box.set_margins(24, 24, 18, 18);
        empty_box.set_halign(Align::Center);
        empty_box.set_valign(Align::Start);

        let empty_icon = icons::image(IconId::Shield, 28);
        empty_icon.set_halign(Align::Center);
        empty_box.append(&empty_icon);

        let empty_title = Label::new(Some("No hay elementos ocultos en este sitio"));
        empty_title.add_css_class("search-panel-title");
        empty_title.set_halign(Align::Center);
        empty_title.set_wrap(true);
        empty_box.append(&empty_title);

        let empty_desc = Label::new(Some(
            "Oculta banners, avisos o bloques molestos de forma permanente.",
        ));
        empty_desc.add_css_class("search-panel-empty");
        empty_desc.set_halign(Align::Center);
        empty_desc.set_wrap(true);
        empty_box.append(&empty_desc);

        let empty_pick_btn = Button::with_label("Iniciar selector (Ctrl+Shift+H)");
        empty_pick_btn.add_css_class("suggested-action");
        empty_pick_btn.set_halign(Align::Center);
        {
            let cbs = callbacks.clone();
            empty_pick_btn.connect_clicked(move || (cbs.on_start_pick)());
        }
        empty_box.append(&empty_pick_btn);

        let empty_clear_filter_btn = Button::with_label("Limpiar filtro");
        empty_clear_filter_btn.add_css_class("flat");
        empty_clear_filter_btn.set_halign(Align::Center);
        empty_clear_filter_btn.set_visible(false);
        empty_box.append(&empty_clear_filter_btn);

        let list_container = BoxWidget::new(Orientation::Vertical, 0);
        list_container.append(&empty_box);
        list_container.append(&list_box);

        let scroller = ScrolledWindow::new();
        scroller.set_policy(PolicyType::Never, PolicyType::Automatic);
        scroller.set_hexpand(true);
        scroller.set_vexpand(true);
        scroller.set_child(Some(&list_container));
        root.append(&scroller);

        // Pie de panel: Restaurar todo.
        let footer = BoxWidget::new(Orientation::Horizontal, 6);
        footer.set_margins(6, 8, 8, 8);
        let btn_restore_all = Button::with_label("Restaurar todo");
        btn_restore_all.add_css_class("flat");
        btn_restore_all.add_css_class("destructive-action");
        btn_restore_all.set_hexpand(true);
        footer.append(&btn_restore_all);
        root.append(&footer);

        let panel = Self {
            root,
            site_caption,
            status_label,
            search_entry: search_entry.clone(),
            btn_site: btn_site.clone(),
            btn_all: btn_all.clone(),
            btn_undo: btn_undo.clone(),
            btn_restore_all: btn_restore_all.clone(),
            list_box,
            empty_box,
            empty_title,
            empty_desc,
            empty_pick_btn,
            empty_clear_filter_btn: empty_clear_filter_btn.clone(),
            manager: manager.clone(),
            callbacks: callbacks.clone(),
            current_host,
            scope,
            visible,
        };

        // Al salir del panel entero, quitar cualquier previsualización activa (`Hidden.swift:44`).
        {
            let motion = EventControllerMotion::new();
            let p = panel.clone();
            motion.connect_leave(move || {
                if let Some(ref host) = *p.current_host.borrow() {
                    (p.callbacks.on_unpeek)(host.clone());
                }
            });
            panel.root.add_controller(&motion);
        }

        {
            let p = panel.clone();
            search_entry.connect_search_changed(move || p.refresh());
        }
        {
            let p = panel.clone();
            empty_clear_filter_btn.connect_clicked(move || {
                p.search_entry.set_text("");
                p.refresh();
            });
        }
        {
            let p = panel.clone();
            btn_site.connect_clicked(move || {
                p.scope.set(CurtainScope::CurrentSite);
                p.refresh();
            });
        }
        {
            let p = panel.clone();
            btn_all.connect_clicked(move || {
                p.scope.set(CurtainScope::AllSites);
                p.refresh();
            });
        }
        {
            let p = panel.clone();
            btn_undo.connect_clicked(move || {
                let host_opt = p.current_host.borrow().clone();
                if let Some(host) = host_opt {
                    if p.manager.undo(&host).is_some() {
                        (p.callbacks.on_rules_changed)(host);
                    }
                }
            });
        }
        {
            let p = panel.clone();
            btn_restore_all.connect_clicked(move || match p.scope.get() {
                CurtainScope::CurrentSite => {
                    let host_opt = p.current_host.borrow().clone();
                    if let Some(host) = host_opt {
                        if p.manager.restore_all(&host) > 0 {
                            (p.callbacks.on_rules_changed)(host);
                        }
                    }
                }
                CurtainScope::AllSites => {
                    let host_opt = p.current_host.borrow().clone();
                    if p.manager.clear_all() > 0 {
                        if let Some(host) = host_opt {
                            (p.callbacks.on_rules_changed)(host);
                        }
                    }
                }
            });
        }
        {
            let p = panel.clone();
            let pop = io_pop.clone();
            export_btn.connect_clicked(move || {
                pop.popdown();
                let dlg = FileDialog::new();
                dlg.set_title("Exportar reglas de Curtain (JSON)");
                dlg.set_initial_name("curtain-rules.json");
                let p_done = p.clone();
                dlg.save(None, move |opt_path| {
                    let Some(path) = opt_path else { return };
                    match p_done.manager.export_json() {
                        Ok(json) => match dot_core::paths::write_atomic(&path, json.as_bytes()) {
                            Ok(()) => p_done.show_status("Reglas exportadas."),
                            Err(err) => {
                                dot_core::warn!("Error al exportar reglas Curtain: {err:#}");
                                p_done.show_status("No se pudo guardar el archivo JSON.");
                            }
                        },
                        Err(err) => {
                            dot_core::warn!("Error al serializar reglas Curtain: {err:#}");
                            p_done.show_status("No se pudieron preparar las reglas.");
                        }
                    }
                });
            });
        }
        {
            let p = panel.clone();
            let pop = io_pop.clone();
            import_btn.connect_clicked(move || {
                pop.popdown();
                let dlg = FileDialog::new();
                dlg.set_title("Importar reglas de Curtain (JSON)");
                let p_done = p.clone();
                dlg.open(None, move |opt_path| {
                    let Some(path) = opt_path else { return };
                    match fs::read_to_string(&path) {
                        Ok(raw) => match p_done.manager.import_json(&raw) {
                            Ok(stats) => {
                                p_done.show_status(&format!(
                                    "Importadas {} regla(s) ({} omitida(s)).",
                                    stats.added, stats.skipped
                                ));
                                if stats.added > 0 {
                                    if let Some(host) = p_done.current_host.borrow().clone() {
                                        (p_done.callbacks.on_rules_changed)(host);
                                    }
                                }
                            }
                            Err(err) => {
                                dot_core::warn!("JSON inválido al importar Curtain: {err:#}");
                                p_done.show_status(
                                    "Archivo JSON no válido. Comprueba el formato e inténtalo de nuevo.",
                                );
                            }
                        },
                        Err(err) => {
                            dot_core::warn!("Error al leer archivo JSON de Curtain: {err}");
                            p_done.show_status("No se pudo leer el archivo seleccionado.");
                        }
                    }
                });
            });
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

    pub fn set_current_url(&self, url: &str) {
        let host = CurtainManager::host_for_url(url);
        *self.current_host.borrow_mut() = host;
        if self.visible.get() {
            self.refresh();
        }
    }

    pub fn set_panel_visible(&self, shown: bool) {
        self.visible.set(shown);
        if shown {
            self.refresh();
        } else if let Some(ref host) = *self.current_host.borrow() {
            (self.callbacks.on_unpeek)(host.clone());
        }
    }

    fn show_status(&self, msg: &str) {
        self.status_label.set_text(msg);
        self.status_label.set_visible(true);
    }

    fn matches_query(veil: &Veil, host: &str, query_lower: &str) -> bool {
        if query_lower.is_empty() {
            return true;
        }
        veil.label.to_lowercase().contains(query_lower)
            || veil.selector.to_lowercase().contains(query_lower)
            || veil
                .note
                .as_deref()
                .is_some_and(|n| n.to_lowercase().contains(query_lower))
            || host.to_lowercase().contains(query_lower)
    }

    pub fn refresh(&self) {
        self.list_box.remove_all();

        let scope = self.scope.get();
        if scope == CurtainScope::CurrentSite {
            self.btn_site.add_css_class("suggested-action");
            self.btn_site.remove_css_class("flat");
            self.btn_all.remove_css_class("suggested-action");
            self.btn_all.add_css_class("flat");
        } else {
            self.btn_all.add_css_class("suggested-action");
            self.btn_all.remove_css_class("flat");
            self.btn_site.remove_css_class("suggested-action");
            self.btn_site.add_css_class("flat");
        }

        let query = self.search_entry.text().trim().to_string();
        let query_lower = query.to_lowercase();
        let has_query = !query_lower.is_empty();

        let current_host = self.current_host.borrow().clone();
        let current_veils = current_host
            .as_deref()
            .map(|h| self.manager.veils_for_host(h))
            .unwrap_or_default();
        self.btn_undo.set_sensitive(!current_veils.is_empty());

        match scope {
            CurtainScope::CurrentSite => {
                let caption = match current_host.as_deref() {
                    Some(h) if !current_veils.is_empty() => {
                        format!("{h} — pasa el cursor por una fila para previsualizar")
                    }
                    Some(h) => h.to_string(),
                    None => "Esta página no tiene un dominio web".to_string(),
                };
                self.site_caption.set_text(&caption);

                if current_veils.is_empty() {
                    self.empty_title
                        .set_text("No hay elementos ocultos en este sitio");
                    if current_host.is_some() {
                        self.empty_desc.set_text(
                            "Oculta banners, avisos o bloques molestos de forma permanente.",
                        );
                        self.empty_pick_btn.set_visible(true);
                    } else {
                        self.empty_desc
                            .set_text("Abre una página web para ocultar elementos.");
                        self.empty_pick_btn.set_visible(false);
                    }
                    self.empty_clear_filter_btn.set_visible(false);
                    self.empty_box.set_visible(true);
                    self.btn_restore_all.set_visible(false);
                    return;
                }

                let host = current_host.unwrap_or_default();
                let filtered: Vec<Veil> = current_veils
                    .into_iter()
                    .filter(|v| Self::matches_query(v, &host, &query_lower))
                    .collect();

                if filtered.is_empty() && has_query {
                    self.empty_title
                        .set_text(&format!("Sin resultados para «{query}»"));
                    self.empty_desc
                        .set_text("Prueba otro selector o limpia el filtro.");
                    self.empty_pick_btn.set_visible(false);
                    self.empty_clear_filter_btn.set_visible(true);
                    self.empty_box.set_visible(true);
                    self.btn_restore_all.set_visible(true);
                    return;
                }

                self.empty_box.set_visible(false);
                self.btn_restore_all.set_visible(true);

                for veil in filtered {
                    let row = self.build_veil_row(&host, &veil, true);
                    self.list_box.append(&row);
                }
            }
            CurtainScope::AllSites => {
                let all = self.manager.all_entries();
                let total: usize = all.iter().map(|(_, v)| v.len()).sum();
                self.site_caption
                    .set_text(&format!("{total} regla(s) en {} sitio(s)", all.len()));

                if all.is_empty() {
                    self.empty_title
                        .set_text("Todavía no hay elementos ocultos");
                    self.empty_desc
                        .set_text("Usa el selector para ocultar partes de cualquier página web.");
                    self.empty_pick_btn.set_visible(current_host.is_some());
                    self.empty_clear_filter_btn.set_visible(false);
                    self.empty_box.set_visible(true);
                    self.btn_restore_all.set_visible(false);
                    return;
                }

                let mut matched_any = false;
                for (host, veils) in all {
                    let matching: Vec<Veil> = veils
                        .into_iter()
                        .filter(|v| Self::matches_query(v, &host, &query_lower))
                        .collect();
                    if matching.is_empty() {
                        continue;
                    }
                    matched_any = true;

                    let header_row = ListBoxRow::new();
                    header_row.set_selectable(false);
                    header_row.set_activatable(false);

                    let h_box = BoxWidget::new(Orientation::Horizontal, 6);
                    h_box.set_margins(8, 2, 10, 8);
                    let lbl = Label::new(Some(&host));
                    lbl.add_css_class("search-panel-section");
                    lbl.set_xalign(0.0);
                    lbl.set_hexpand(true);
                    h_box.append(&lbl);

                    let clear_host = icons::icon_button(IconId::ClearAll, 14);
                    clear_host.add_css_class("search-tab-mini-btn");
                    clear_host.set_tooltip_text(Some(&format!("Restaurar todo en {host}")));
                    {
                        let mgr = self.manager.clone();
                        let cbs = self.callbacks.clone();
                        let h_owned = host.clone();
                        clear_host.connect_clicked(move || {
                            if mgr.restore_all(&h_owned) > 0 {
                                (cbs.on_rules_changed)(h_owned.clone());
                            }
                        });
                    }
                    h_box.append(&clear_host);
                    header_row.set_child(Some(&h_box));
                    self.list_box.append(&header_row);

                    let is_active_host = current_host.as_deref() == Some(host.as_str());
                    for veil in matching {
                        let row = self.build_veil_row(&host, &veil, is_active_host);
                        self.list_box.append(&row);
                    }
                }

                if !matched_any && has_query {
                    self.empty_title
                        .set_text(&format!("Sin resultados para «{query}»"));
                    self.empty_desc
                        .set_text("Prueba otro selector, dominio o limpia el filtro.");
                    self.empty_pick_btn.set_visible(false);
                    self.empty_clear_filter_btn.set_visible(true);
                    self.empty_box.set_visible(true);
                    self.btn_restore_all.set_visible(true);
                } else {
                    self.empty_box.set_visible(false);
                    self.btn_restore_all.set_visible(true);
                }
            }
        }
    }

    fn build_veil_row(&self, host: &str, veil: &Veil, allow_peek: bool) -> ListBoxRow {
        let row = ListBoxRow::new();
        row.add_css_class("search-panel-row");
        row.set_selectable(false);
        row.set_activatable(false);

        let hbox = BoxWidget::new(Orientation::Horizontal, 6);
        hbox.set_margins(5, 5, 10, 8);

        // Conmutador por regla para activar/desactivar sin eliminar.
        let toggle_btn = icons::icon_button(IconId::Check, 13);
        toggle_btn.add_css_class("search-tab-mini-btn");
        if veil.enabled {
            toggle_btn.add_css_class("suggested-action");
            toggle_btn.remove_css_class("flat");
            toggle_btn.set_tooltip_text(Some("Desactivar regla temporalmente"));
        } else {
            toggle_btn.add_css_class("dim-label");
            toggle_btn.set_tooltip_text(Some("Activar regla"));
        }
        {
            let mgr = self.manager.clone();
            let cbs = self.callbacks.clone();
            let h_owned = host.to_string();
            let sel_owned = veil.selector.clone();
            let next_enabled = !veil.enabled;
            toggle_btn.connect_clicked(move || {
                if mgr.set_rule_enabled(&h_owned, &sel_owned, next_enabled) {
                    (cbs.on_rules_changed)(h_owned.clone());
                }
            });
        }
        hbox.append(&toggle_btn);

        let text_col = BoxWidget::new(Orientation::Vertical, 1);
        text_col.set_hexpand(true);
        if !veil.enabled {
            text_col.add_css_class("dim-label");
        }

        let label_lbl = Label::new(Some(&veil.label));
        label_lbl.set_xalign(0.0);
        label_lbl.set_ellipsize(EllipsizeMode::End);
        text_col.append(&label_lbl);

        let subtitle = match veil.note.as_deref() {
            Some(note) if !note.is_empty() => format!("{note} · {}", veil.selector),
            _ => veil.selector.clone(),
        };
        let sub_lbl = Label::new(Some(&subtitle));
        sub_lbl.add_css_class("search-panel-subtitle");
        sub_lbl.set_xalign(0.0);
        sub_lbl.set_ellipsize(EllipsizeMode::End);
        text_col.append(&sub_lbl);
        text_col.set_tooltip_text(Some(&veil.selector));
        hbox.append(&text_col);

        let restore_btn = icons::icon_button(IconId::Trash, 14);
        restore_btn.add_css_class("search-tab-mini-btn");
        restore_btn.set_tooltip_text(Some("Eliminar esta regla"));
        {
            let mgr = self.manager.clone();
            let cbs = self.callbacks.clone();
            let h_owned = host.to_string();
            let sel_owned = veil.selector.clone();
            restore_btn.connect_clicked(move || {
                if mgr.restore(&h_owned, &sel_owned) {
                    (cbs.on_rules_changed)(h_owned.clone());
                }
            });
        }
        hbox.append(&restore_btn);

        if allow_peek {
            let motion = EventControllerMotion::new();
            let cbs_in = self.callbacks.clone();
            let h_in = host.to_string();
            let sel_in = veil.selector.clone();
            motion.connect_enter(move |_x, _y| {
                (cbs_in.on_peek)(h_in.clone(), sel_in.clone());
            });
            let cbs_out = self.callbacks.clone();
            let h_out = host.to_string();
            motion.connect_leave(move || {
                (cbs_out.on_unpeek)(h_out.clone());
            });
            hbox.add_controller(&motion);
        }

        row.set_child(Some(&hbox));
        row
    }
}
