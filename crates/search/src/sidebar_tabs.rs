//! Barra lateral izquierda vertical de pestañas (`Side.swift` + `SideIcons.swift` + `PinDrop.swift`).
//!
//! Sincronizada en tiempo real con el mismo `AdwTabView` de la ventana:
//! - Sección compacta superior para pestañas fijadas (favicon/monograma, tooltip, DnD).
//! - Lista desplazable inferior para pestañas regulares con favicon, indicador de carga,
//!   botón de silencio de audio, cierre con clic central, menú contextual con clic derecho
//!   y reordenación por arrastrar y soltar (`GtkDragSource` / `GtkDropTarget`).

use gnome_platform::gdk::{ContentProvider, DragAction, Texture};
use gnome_platform::gtk::{
    Align, AsWidget, BoxWidget, Button, DragSource, DropTarget, EllipsizeMode, GestureClick, Image,
    Label, ListBox, ListBoxRow, Orientation, PolicyType, ScrolledWindow, SelectionMode, Separator,
    Spinner,
};
use std::cell::RefCell;
use std::rc::Rc;

/// Instantánea ligera de una pestaña para renderizar la barra lateral vertical.
#[derive(Clone)]
pub struct SidebarTabItem {
    pub id: u64,
    pub title: String,
    pub tooltip: String,
    pub pin_label: Option<String>,
    pub pinned: bool,
    pub selected: bool,
    pub loading: bool,
    pub audible: bool,
    pub muted: bool,
    pub favicon: Option<Texture>,
}

type ContextMenuCallback = Box<dyn Fn(u64, BoxWidget, f64, f64)>;

#[derive(Default)]
pub struct SidebarCallbacks {
    pub on_select: Option<Box<dyn Fn(u64)>>,
    pub on_close: Option<Box<dyn Fn(u64)>>,
    pub on_toggle_mute: Option<Box<dyn Fn(u64)>>,
    pub on_context_menu: Option<ContextMenuCallback>,
    pub on_reorder: Option<Box<dyn Fn(u64, u64)>>,
    pub on_new_tab: Option<Box<dyn Fn()>>,
}

#[derive(Clone)]
pub struct SidebarTabs {
    root: BoxWidget,
    pinned_box: BoxWidget,
    pinned_sep: Separator,
    list_box: ListBox,
    callbacks: Rc<RefCell<SidebarCallbacks>>,
}

impl SidebarTabs {
    pub fn new(width: i32) -> Self {
        let root = BoxWidget::new(Orientation::Vertical, 4);
        root.add_css_class("search-sidebar");
        root.set_size_request(width.clamp(160, 420), -1);
        root.set_vexpand(true);

        // Fila compacta de pestañas fijadas en la parte superior.
        let pinned_box = BoxWidget::new(Orientation::Vertical, 4);
        pinned_box.add_css_class("search-sidebar-pinned");
        pinned_box.set_visible(false);

        let pinned_sep = Separator::new(Orientation::Horizontal);
        pinned_sep.set_visible(false);

        let list_box = ListBox::new();
        list_box.add_css_class("search-sidebar-list");
        list_box.set_selection_mode(SelectionMode::None);
        list_box.set_vexpand(true);

        let scrolled = ScrolledWindow::new();
        scrolled.set_policy(PolicyType::Never, PolicyType::Automatic);
        scrolled.set_vexpand(true);
        scrolled.set_child(Some(&list_box));

        let new_tab_btn = Button::from_icon_name("tab-new-symbolic");
        new_tab_btn.add_css_class("flat");
        new_tab_btn.set_tooltip_text(Some("Nueva pestaña (Ctrl+T)"));
        new_tab_btn.set_halign(Align::Fill);

        root.append(&pinned_box);
        root.append(&pinned_sep);
        root.append(&scrolled);
        root.append(&new_tab_btn);

        let callbacks = Rc::new(RefCell::new(SidebarCallbacks::default()));
        {
            let cbs = callbacks.clone();
            new_tab_btn.connect_clicked(move || {
                if let Some(ref cb) = cbs.borrow().on_new_tab {
                    cb();
                }
            });
        }

        Self {
            root,
            pinned_box,
            pinned_sep,
            list_box,
            callbacks,
        }
    }

    pub fn widget(&self) -> &BoxWidget {
        &self.root
    }

    pub fn set_callbacks(&self, cbs: SidebarCallbacks) {
        *self.callbacks.borrow_mut() = cbs;
    }

    pub fn set_width(&self, width: i32) {
        self.root.set_size_request(width.clamp(160, 420), -1);
    }

    /// Reconstruye la vista de la barra lateral a partir del estado actual de `AdwTabView`.
    pub fn sync(&self, items: &[SidebarTabItem]) {
        // Limpiar contenedor de fijadas recreando sus filas de píldoras.
        clear_box_children_via_rebuild(
            &self.root,
            &self.pinned_box,
            &self.pinned_sep,
            items,
            &self.callbacks,
        );
        self.list_box.remove_all();

        let regular: Vec<&SidebarTabItem> = items.iter().filter(|i| !i.pinned).collect();
        let has_pinned = items.iter().any(|i| i.pinned);
        self.pinned_box.set_visible(has_pinned);
        self.pinned_sep
            .set_visible(has_pinned && !regular.is_empty());

        for item in regular {
            let row = build_regular_row(item, &self.callbacks);
            self.list_box.append(&row);
        }
    }
}

fn clear_box_children_via_rebuild(
    _root: &BoxWidget,
    pinned_box: &BoxWidget,
    _sep: &Separator,
    items: &[SidebarTabItem],
    callbacks: &Rc<RefCell<SidebarCallbacks>>,
) {
    // Reemplazamos los hijos previos guardando la referencia de filas en un Vec thread-local
    // o usando un ListBox sin selección para las fijadas. Wait: GtkBox no tiene remove_all;
    // mejor usar un contenedor dedicado con seguimiento seguro en una lista estática por widget.
    // Como `pinned_box` solo tiene pocas filas de botones (4 por fila), las rastreamos en
    // un mapa por dirección de puntero.
    use std::collections::HashMap;
    thread_local! {
        static PINNED_ROWS: RefCell<HashMap<usize, Vec<BoxWidget>>> = RefCell::new(HashMap::new());
    }

    let key = pinned_box.as_widget_ptr() as usize;
    PINNED_ROWS.with(|map| {
        if let Some(old_rows) = map.borrow_mut().remove(&key) {
            for r in old_rows {
                pinned_box.remove(&r);
            }
        }
    });

    let pinned: Vec<&SidebarTabItem> = items.iter().filter(|i| i.pinned).collect();
    if pinned.is_empty() {
        return;
    }

    let mut created_rows = Vec::new();
    for chunk in pinned.chunks(4) {
        let row_box = BoxWidget::new(Orientation::Horizontal, 4);
        row_box.set_hexpand(true);
        for item in chunk {
            let pill = build_pinned_pill(item, callbacks);
            row_box.append(&pill);
        }
        pinned_box.append(&row_box);
        created_rows.push(row_box);
    }

    PINNED_ROWS.with(|map| {
        map.borrow_mut().insert(key, created_rows);
    });
}

fn build_pinned_pill(
    item: &SidebarTabItem,
    callbacks: &Rc<RefCell<SidebarCallbacks>>,
) -> BoxWidget {
    let holder = BoxWidget::new(Orientation::Horizontal, 0);
    holder.set_hexpand(true);

    let btn = Button::with_label("");
    btn.add_css_class("search-pinned-pill");
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
        spin.set_size_request(14, 14);
        content.append(&spin);
    } else if let Some(ref tex) = item.favicon {
        let img = Image::new();
        img.set_pixel_size(16);
        img.set_from_texture(Some(tex));
        content.append(&img);
    } else {
        let letter = item.pin_label.as_deref().unwrap_or("•");
        let lbl = Label::new(Some(letter));
        content.append(&lbl);
    }

    if item.muted || item.audible {
        let icon_name = if item.muted {
            "audio-volume-muted-symbolic"
        } else {
            "audio-volume-high-symbolic"
        };
        let audio_icon = Image::from_icon_name(icon_name);
        audio_icon.set_pixel_size(11);
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

    // Clic derecho abre el menú contextual de la pestaña fijada (para desfijar, silenciar, etc.).
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

fn build_regular_row(
    item: &SidebarTabItem,
    callbacks: &Rc<RefCell<SidebarCallbacks>>,
) -> ListBoxRow {
    let row = ListBoxRow::new();
    row.add_css_class("search-tab-row");
    row.set_selectable(false);
    row.set_activatable(false);
    row.set_tooltip_text(Some(&item.tooltip));
    if item.selected {
        row.add_css_class("active");
    }

    let hbox = BoxWidget::new(Orientation::Horizontal, 6);
    hbox.set_hexpand(true);
    hbox.set_valign(Align::Center);

    if item.loading {
        let spinner = Spinner::new();
        spinner.set_spinning(true);
        spinner.set_size_request(16, 16);
        hbox.append(&spinner);
    } else if let Some(ref tex) = item.favicon {
        let img = Image::new();
        img.set_pixel_size(16);
        img.set_from_texture(Some(tex));
        hbox.append(&img);
    } else {
        let img = Image::from_icon_name("web-browser-symbolic");
        img.set_pixel_size(16);
        hbox.append(&img);
    }

    let title_label = Label::new(Some(&item.title));
    title_label.set_ellipsize(EllipsizeMode::End);
    title_label.set_xalign(0.0);
    title_label.set_hexpand(true);
    hbox.append(&title_label);

    let id = item.id;

    if item.audible || item.muted {
        let icon_name = if item.muted {
            "audio-volume-muted-symbolic"
        } else {
            "audio-volume-high-symbolic"
        };
        let audio_btn = Button::from_icon_name(icon_name);
        audio_btn.add_css_class("flat");
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

    let close_btn = Button::from_icon_name("window-close-symbolic");
    close_btn.add_css_class("flat");
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

    // Clic izquierdo selecciona; clic central cierra; clic derecho abre el menú contextual.
    {
        let click = GestureClick::new();
        click.set_button(0);
        let click_ref = click.clone();
        let cbs = callbacks.clone();
        let anchor = hbox.clone();
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
    row.set_child(Some(&hbox));
    row
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
