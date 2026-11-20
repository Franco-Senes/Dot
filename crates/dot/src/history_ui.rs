//! Gestor reactivo del historial (`HistoryManager`) con persistencia atómica + debounce (~1 s)
//! y panel lateral discreto (`Ctrl+H` / `Ctrl+Y`) agrupado por día (`Recall.swift`).

use crate::icons::{self, IconId};
use dot_core::address::display_at_rest;
use dot_core::history::{format_time_hm, group_by_day, HistoryEntry, HistoryStore};
use dot_core::session::now_unix_secs;
use gnome_platform::gtk::{
    Align, AsEditable, AsWidget, BoxWidget, Button, EllipsizeMode, Image, Label, ListBox,
    ListBoxRow, MenuButton, Orientation, PolicyType, Popover, ScrolledWindow, SearchEntry,
    SelectionMode, Separator,
};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

const HISTORY_DEBOUNCE_MS: u64 = 1000;
const PANEL_MAX_ROWS: usize = 300;

type Listener = Rc<dyn Fn()>;
type OpenUrlCallback = Rc<dyn Fn(String, bool)>;
type CloseCallback = Rc<dyn Fn()>;

struct HistoryManagerInner {
    file_path: PathBuf,
    store: HistoryStore,
    save_debounce: Cell<Option<gnome_platform::glib::SourceId>>,
    listeners: Vec<Listener>,
}

/// Almacén compartido de historial para todas las ventanas normales de la aplicación.
#[derive(Clone)]
pub struct HistoryManager {
    inner: Rc<RefCell<HistoryManagerInner>>,
}

impl HistoryManager {
    pub fn load(file_path: PathBuf) -> Self {
        let store = HistoryStore::load(&file_path);
        Self {
            inner: Rc::new(RefCell::new(HistoryManagerInner {
                file_path,
                store,
                save_debounce: Cell::new(None),
                listeners: Vec::new(),
            })),
        }
    }

    pub fn with_store<R>(&self, f: impl FnOnce(&HistoryStore) -> R) -> R {
        let borrow = self.inner.borrow();
        f(&borrow.store)
    }

    pub fn record_visit(&self, url: &str, title: &str) {
        let recorded = self
            .inner
            .borrow_mut()
            .store
            .record(url, title, now_unix_secs());
        if recorded {
            self.schedule_save();
            self.notify_listeners();
        }
    }

    pub fn retitle(&self, url: &str, title: &str) {
        let changed = self.inner.borrow_mut().store.retitle(url, title);
        if changed {
            self.schedule_save();
            self.notify_listeners();
        }
    }

    pub fn remove_by_key(&self, key: &str) -> bool {
        let removed = self.inner.borrow_mut().store.remove_by_key(key);
        if removed {
            self.schedule_save();
            self.notify_listeners();
        }
        removed
    }

    pub fn remove_host(&self, host: &str) -> usize {
        let removed = self.inner.borrow_mut().store.remove_host(host);
        if removed > 0 {
            self.schedule_save();
            self.notify_listeners();
        }
        removed
    }

    pub fn clear_last_hour(&self) {
        let cutoff = now_unix_secs() - 3600.0;
        if self.inner.borrow_mut().store.clear_since(cutoff) > 0 {
            self.schedule_save();
            self.notify_listeners();
        }
    }

    pub fn clear_last_day(&self) {
        let cutoff = now_unix_secs() - 86_400.0;
        if self.inner.borrow_mut().store.clear_since(cutoff) > 0 {
            self.schedule_save();
            self.notify_listeners();
        }
    }

    pub fn clear_last_7_days(&self) {
        let cutoff = now_unix_secs() - 7.0 * 86_400.0;
        if self.inner.borrow_mut().store.clear_since(cutoff) > 0 {
            self.schedule_save();
            self.notify_listeners();
        }
    }

    pub fn clear_all(&self) {
        self.inner.borrow_mut().store.clear_all();
        self.schedule_save();
        self.notify_listeners();
    }

    pub fn filter_chronological(&self, query: &str, limit: usize) -> Vec<HistoryEntry> {
        self.inner.borrow().store.filter_chronological(query, limit)
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
                "No se pudo guardar el historial en '{}': {err:#}",
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
            Duration::from_millis(HISTORY_DEBOUNCE_MS),
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

/// Panel lateral discreto de historial (`Ctrl+H` / `Ctrl+Y`).
#[derive(Clone)]
pub struct HistoryPanel {
    root: BoxWidget,
    search_entry: SearchEntry,
    list_box: ListBox,
    empty_box: BoxWidget,
    empty_icon: Image,
    empty_title: Label,
    empty_desc: Label,
    empty_cta: Button,
    manager: HistoryManager,
    on_open_url: OpenUrlCallback,
    visible: Rc<Cell<bool>>,
}

impl HistoryPanel {
    pub fn new(
        manager: HistoryManager,
        on_open_url: impl Fn(String, bool) + 'static,
        on_close: impl Fn() + 'static,
    ) -> Self {
        let on_open_url: OpenUrlCallback = Rc::new(on_open_url);
        let on_close: CloseCallback = Rc::new(on_close);
        let visible = Rc::new(Cell::new(false));

        let root = BoxWidget::new(Orientation::Vertical, 6);
        root.add_css_class("search-side-panel");
        root.set_hexpand(true);
        root.set_vexpand(true);

        // Cabecera compacta.
        let header = BoxWidget::new(Orientation::Horizontal, 6);
        header.set_margins(8, 8, 10, 4);

        let title = Label::new(Some("Historial"));
        title.add_css_class("search-panel-title");
        title.set_xalign(0.0);
        title.set_hexpand(true);
        header.append(&title);

        // Botón "Borrar…" con menú discreto (última hora / último día / todo).
        let clear_pop = Popover::new();
        let clear_box = BoxWidget::new(Orientation::Vertical, 2);
        clear_box.set_margins(4, 4, 4, 4);

        let btn_hour = Button::with_label("Borrar última hora");
        btn_hour.add_css_class("flat");
        {
            let mgr = manager.clone();
            let pop = clear_pop.clone();
            btn_hour.connect_clicked(move || {
                pop.popdown();
                mgr.clear_last_hour();
            });
        }
        clear_box.append(&btn_hour);

        let btn_day = Button::with_label("Borrar últimas 24 h");
        btn_day.add_css_class("flat");
        {
            let mgr = manager.clone();
            let pop = clear_pop.clone();
            btn_day.connect_clicked(move || {
                pop.popdown();
                mgr.clear_last_day();
            });
        }
        clear_box.append(&btn_day);

        let btn_week = Button::with_label("Borrar últimos 7 días");
        btn_week.add_css_class("flat");
        {
            let mgr = manager.clone();
            let pop = clear_pop.clone();
            btn_week.connect_clicked(move || {
                pop.popdown();
                mgr.clear_last_7_days();
            });
        }
        clear_box.append(&btn_week);

        clear_box.append(&Separator::new(Orientation::Horizontal));

        let btn_all = Button::with_label("Borrar todo el historial");
        btn_all.add_css_class("flat");
        btn_all.add_css_class("destructive-action");
        {
            let mgr = manager.clone();
            let pop = clear_pop.clone();
            btn_all.connect_clicked(move || {
                pop.popdown();
                mgr.clear_all();
            });
        }
        clear_box.append(&btn_all);
        clear_pop.set_child(Some(&clear_box));

        let clear_menu_btn = MenuButton::new();
        clear_menu_btn.set_label("Borrar");
        clear_menu_btn.add_css_class("flat");
        clear_menu_btn.set_tooltip_text(Some("Borrar historial por rango de tiempo"));
        clear_menu_btn.set_popover(Some(&clear_pop));
        header.append(&clear_menu_btn);

        let close_btn = icons::icon_button(IconId::Close, 15);
        close_btn.set_tooltip_text(Some("Cerrar panel de historial (Esc)"));
        {
            let cb = on_close.clone();
            close_btn.connect_clicked(move || cb());
        }
        header.append(&close_btn);

        root.append(&header);

        let search_entry = SearchEntry::new();
        search_entry.set_placeholder_text("Buscar en el historial…");
        search_entry.set_margins(8, 8, 0, 4);
        root.append(&search_entry);

        root.append(&Separator::new(Orientation::Horizontal));

        let list_box = ListBox::new();
        list_box.add_css_class("search-panel-list");
        list_box.set_selection_mode(SelectionMode::None);
        list_box.set_activate_on_single_click(false);

        let empty_box = BoxWidget::new(Orientation::Vertical, 8);
        empty_box.set_margins(20, 20, 32, 20);
        empty_box.set_halign(Align::Center);
        empty_box.set_valign(Align::Start);

        let empty_icon = icons::image(IconId::History, 28);
        empty_icon.set_halign(Align::Center);
        empty_box.append(&empty_icon);

        let empty_title = Label::new(Some("Sin visitas en el historial"));
        empty_title.add_css_class("search-panel-title");
        empty_title.set_halign(Align::Center);
        empty_box.append(&empty_title);

        let empty_desc = Label::new(Some(
            "Las páginas que visites aparecerán aquí organizadas por día.",
        ));
        empty_desc.add_css_class("search-panel-empty");
        empty_desc.set_wrap(true);
        empty_desc.set_halign(Align::Center);
        empty_box.append(&empty_desc);

        let empty_cta = Button::with_label("Abrir nueva pestaña");
        empty_cta.add_css_class("suggested-action");
        empty_cta.set_halign(Align::Center);
        empty_box.append(&empty_cta);

        let list_container = BoxWidget::new(Orientation::Vertical, 0);
        list_container.append(&empty_box);
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
            list_box,
            empty_box,
            empty_icon,
            empty_title,
            empty_desc,
            empty_cta: empty_cta.clone(),
            manager: manager.clone(),
            on_open_url: on_open_url.clone(),
            visible,
        };

        {
            let se = search_entry.clone();
            let cb = on_open_url;
            empty_cta.connect_clicked(move || {
                if !se.text().trim().is_empty() {
                    se.set_text("");
                    se.grab_focus();
                } else {
                    cb("dot://newtab".to_string(), false);
                }
            });
        }

        {
            let p = panel.clone();
            search_entry.connect_search_changed(move || {
                p.refresh();
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

    pub fn set_panel_visible(&self, shown: bool) {
        self.visible.set(shown);
        if shown {
            self.refresh();
            self.search_entry.grab_focus();
        }
    }

    pub fn refresh(&self) {
        self.list_box.remove_all();
        let query = self.search_entry.text();
        let entries = self.manager.filter_chronological(&query, PANEL_MAX_ROWS);

        if entries.is_empty() {
            let trimmed = query.trim();
            let dark = icons::is_dark_theme();
            if trimmed.is_empty() {
                icons::set_image_icon(&self.empty_icon, IconId::History, dark);
                self.empty_title.set_text("Sin visitas en el historial");
                self.empty_desc
                    .set_text("Las páginas que visites aparecerán aquí organizadas por día.");
                self.empty_cta.set_label("Abrir nueva pestaña");
            } else {
                icons::set_image_icon(&self.empty_icon, IconId::Search, dark);
                self.empty_title
                    .set_text(&format!("Sin resultados para «{trimmed}»"));
                self.empty_desc
                    .set_text("Prueba con otro término o limpia el filtro de búsqueda.");
                self.empty_cta.set_label("Limpiar filtro");
            }
            self.empty_box.set_visible(true);
            return;
        }
        self.empty_box.set_visible(false);

        let groups = group_by_day(&entries, now_unix_secs());
        for group in groups {
            let header_row = ListBoxRow::new();
            header_row.set_selectable(false);
            header_row.set_activatable(false);
            let lbl = Label::new(Some(&group.label));
            lbl.add_css_class("search-panel-section");
            lbl.set_xalign(0.0);
            lbl.set_margins(10, 10, 8, 2);
            header_row.set_child(Some(&lbl));
            self.list_box.append(&header_row);

            for item in group.entries {
                let row = self.build_visit_row(&item);
                self.list_box.append(&row);
            }
        }
    }

    fn build_visit_row(&self, entry: &HistoryEntry) -> ListBoxRow {
        let row = ListBoxRow::new();
        row.add_css_class("search-panel-row");
        row.set_selectable(false);
        row.set_activatable(false);

        let hbox = BoxWidget::new(Orientation::Horizontal, 6);
        hbox.set_margins(8, 8, 5, 5);

        let open_btn = Button::with_label("");
        open_btn.add_css_class("flat");
        open_btn.add_css_class("search-panel-item-btn");
        open_btn.set_hexpand(true);

        let text_col = BoxWidget::new(Orientation::Vertical, 1);
        text_col.set_hexpand(true);

        let display_title = if entry.title.trim().is_empty() {
            display_at_rest(&entry.url)
        } else {
            entry.title.clone()
        };
        let title_lbl = Label::new(Some(&display_title));
        title_lbl.set_xalign(0.0);
        title_lbl.set_ellipsize(EllipsizeMode::End);
        text_col.append(&title_lbl);

        let rest = display_at_rest(&entry.url);
        let time_str = format_time_hm(entry.last);
        let sub_text = format!("{time_str} · {rest}");
        let sub_lbl = Label::new(Some(&sub_text));
        sub_lbl.add_css_class("search-panel-subtitle");
        sub_lbl.set_xalign(0.0);
        sub_lbl.set_ellipsize(EllipsizeMode::End);
        text_col.append(&sub_lbl);

        open_btn.set_child(Some(&text_col));
        open_btn.set_tooltip_text(Some(&entry.url));
        {
            let cb = self.on_open_url.clone();
            let url = entry.url.clone();
            open_btn.connect_clicked(move || {
                cb(url.clone(), false);
            });
        }
        hbox.append(&open_btn);

        // Botón para borrar todo el sitio (host).
        if let Ok(parsed) = url::Url::parse(&entry.url) {
            if let Some(host) = parsed.host_str() {
                let bare = host.strip_prefix("www.").unwrap_or(host).to_string();
                let del_site = icons::icon_button(IconId::ClearAll, 14);
                del_site.add_css_class("search-tab-mini-btn");
                del_site.set_tooltip_text(Some(&format!("Borrar todo de {bare}")));
                let mgr = self.manager.clone();
                del_site.connect_clicked(move || {
                    mgr.remove_host(&bare);
                });
                hbox.append(&del_site);
            }
        }

        // Botón para borrar esta entrada individual.
        let del_one = icons::icon_button(IconId::Close, 13);
        del_one.add_css_class("search-tab-mini-btn");
        del_one.set_tooltip_text(Some("Borrar entrada"));
        {
            let mgr = self.manager.clone();
            let key = entry.key.clone();
            del_one.connect_clicked(move || {
                mgr.remove_by_key(&key);
            });
        }
        hbox.append(&del_one);

        row.set_child(Some(&hbox));
        row
    }
}
