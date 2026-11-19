//! Gestor reactivo de `Curtain` (`curtain.json` con persistencia atómica y debounce),
//! inyección del colector mínimo `resources/curtain.js` y panel lateral discreto (`Ctrl+Shift+E`)
//! para inspeccionar, previsualizar al pasar el ratón y restaurar reglas por sitio o globales
//! (`Curtain.swift` + `Hidden.swift`).

use gnome_platform::gtk::{
    Align, AsWidget, BoxWidget, Button, EllipsizeMode, EventControllerMotion, Label, ListBox,
    ListBoxRow, Orientation, PolicyType, ScrolledWindow, SelectionMode, Separator,
};
use search_core::curtain::{generate_selector, CurtainStore, ElementPickSnapshot, Veil};
use search_core::session::now_unix_secs;
use search_core::shield::js_string_literal;
use std::cell::{Cell, RefCell};
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
            search_core::warn!(
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
    btn_site: Button,
    btn_all: Button,
    btn_undo: Button,
    btn_restore_all: Button,
    list_box: ListBox,
    empty_label: Label,
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

        // Cabecera compacta.
        let header = BoxWidget::new(Orientation::Horizontal, 6);
        header.set_margins(8, 8, 10, 4);

        let title = Label::new(Some("Elementos ocultos"));
        title.add_css_class("search-panel-title");
        title.set_xalign(0.0);
        title.set_hexpand(true);
        header.append(&title);

        let close_btn = Button::from_icon_name("window-close-symbolic");
        close_btn.add_css_class("flat");
        close_btn.set_tooltip_text(Some("Cerrar panel (Esc)"));
        {
            let cbs = callbacks.clone();
            close_btn.connect_clicked(move || (cbs.on_close)());
        }
        header.append(&close_btn);
        root.append(&header);

        // Controles de acción rápida: Ocultar elemento + Deshacer.
        let action_bar = BoxWidget::new(Orientation::Horizontal, 6);
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

        let btn_undo = Button::from_icon_name("edit-undo-symbolic");
        btn_undo.add_css_class("flat");
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

        let site_caption = Label::new(None);
        site_caption.add_css_class("search-panel-subtitle");
        site_caption.set_xalign(0.0);
        site_caption.set_ellipsize(EllipsizeMode::End);
        site_caption.set_margins(0, 4, 10, 10);
        root.append(&site_caption);

        root.append(&Separator::new(Orientation::Horizontal));

        let list_box = ListBox::new();
        list_box.add_css_class("search-panel-list");
        list_box.set_selection_mode(SelectionMode::None);
        list_box.set_activate_on_single_click(false);

        let empty_label = Label::new(Some("No hay elementos ocultos en este sitio."));
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
            btn_site: btn_site.clone(),
            btn_all: btn_all.clone(),
            btn_undo: btn_undo.clone(),
            btn_restore_all: btn_restore_all.clone(),
            list_box,
            empty_label,
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
                    self.empty_label
                        .set_text("No hay elementos ocultos en este sitio.");
                    self.empty_label.set_visible(true);
                    self.btn_restore_all.set_visible(false);
                    return;
                }
                self.empty_label.set_visible(false);
                self.btn_restore_all.set_visible(true);

                if let Some(host) = current_host {
                    for veil in current_veils {
                        let row = self.build_veil_row(&host, &veil, true);
                        self.list_box.append(&row);
                    }
                }
            }
            CurtainScope::AllSites => {
                let all = self.manager.all_entries();
                let total: usize = all.iter().map(|(_, v)| v.len()).sum();
                self.site_caption
                    .set_text(&format!("{total} regla(s) en {} sitio(s)", all.len()));

                if all.is_empty() {
                    self.empty_label
                        .set_text("Todavía no has ocultado elementos en ningún sitio.");
                    self.empty_label.set_visible(true);
                    self.btn_restore_all.set_visible(false);
                    return;
                }
                self.empty_label.set_visible(false);
                self.btn_restore_all.set_visible(true);

                for (host, veils) in all {
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

                    let clear_host = Button::from_icon_name("edit-clear-all-symbolic");
                    clear_host.add_css_class("flat");
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
                    for veil in veils {
                        let row = self.build_veil_row(&host, &veil, is_active_host);
                        self.list_box.append(&row);
                    }
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

        let text_col = BoxWidget::new(Orientation::Vertical, 1);
        text_col.set_hexpand(true);

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

        let restore_btn = Button::from_icon_name("edit-undo-symbolic");
        restore_btn.add_css_class("flat");
        restore_btn.add_css_class("search-tab-mini-btn");
        restore_btn.set_tooltip_text(Some("Restaurar este elemento"));
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
