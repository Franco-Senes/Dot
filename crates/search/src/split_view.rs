//! Contenedor visual de Split View (`PaneStage.swift` / `SplitView.swift`).
//!
//! Envuelve un `GtkPaned` horizontal con dos paneles (`BrowserView`) independientes,
//! borde sutil en el panel activo, foco al hacer clic, doble clic en el divisor
//! para restablecer 50/50 y sincronización de proporciones con `TabSplit`.

use gnome_platform::glib;
use gnome_platform::gtk::{
    AsController, AsWidget, BoxWidget, GestureClick, Orientation, Paned, PropagationPhase,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Callbacks emitidos por `SplitViewContainer` hacia `BrowserWindow`.
#[derive(Default)]
pub struct SplitViewCallbacks {
    pub on_focus_pane: Option<Box<dyn Fn(u64)>>,
    pub on_ratio_changed: Option<Box<dyn Fn(f64)>>,
}

struct SplitViewInner {
    paned: Paned,
    left_box: BoxWidget,
    right_box: BoxWidget,
    left_tab_id: Cell<Option<u64>>,
    right_tab_id: Cell<Option<u64>>,
    ratio: Cell<f64>,
    applying_position: Cell<bool>,
    callbacks: RefCell<SplitViewCallbacks>,
}

/// Contenedor reutilizable de `GtkPaned` para mostrar dos pestañas lado a lado.
#[derive(Clone)]
pub struct SplitViewContainer {
    inner: Rc<SplitViewInner>,
}

impl SplitViewContainer {
    pub fn new() -> Self {
        let paned = Paned::new(Orientation::Horizontal);
        paned.add_css_class("search-split-paned");
        paned.set_hexpand(true);
        paned.set_vexpand(true);
        paned.set_wide_handle(false);
        paned.set_resize_start_child(true);
        paned.set_resize_end_child(true);
        paned.set_shrink_start_child(true);
        paned.set_shrink_end_child(true);

        let left_box = BoxWidget::new(Orientation::Vertical, 0);
        left_box.add_css_class("search-split-pane");
        left_box.set_hexpand(true);
        left_box.set_vexpand(true);

        let right_box = BoxWidget::new(Orientation::Vertical, 0);
        right_box.add_css_class("search-split-pane");
        right_box.set_hexpand(true);
        right_box.set_vexpand(true);

        paned.set_start_child(Some(&left_box));
        paned.set_end_child(Some(&right_box));

        let inner = Rc::new(SplitViewInner {
            paned: paned.clone(),
            left_box: left_box.clone(),
            right_box: right_box.clone(),
            left_tab_id: Cell::new(None),
            right_tab_id: Cell::new(None),
            ratio: Cell::new(0.5),
            applying_position: Cell::new(false),
            callbacks: RefCell::new(SplitViewCallbacks::default()),
        });

        // Foco al hacer clic sobre el panel izquierdo.
        {
            let click = GestureClick::new();
            click.set_button(0);
            click.set_propagation_phase(PropagationPhase::Capture);
            let inner_weak = Rc::downgrade(&inner);
            click.connect_pressed(move |_, _, _| {
                if let Some(st) = inner_weak.upgrade() {
                    if let Some(id) = st.left_tab_id.get() {
                        if let Some(cb) = st.callbacks.borrow().on_focus_pane.as_ref() {
                            cb(id);
                        }
                    }
                }
            });
            left_box.add_controller(&click);
        }

        // Foco al hacer clic sobre el panel derecho.
        {
            let click = GestureClick::new();
            click.set_button(0);
            click.set_propagation_phase(PropagationPhase::Capture);
            let inner_weak = Rc::downgrade(&inner);
            click.connect_pressed(move |_, _, _| {
                if let Some(st) = inner_weak.upgrade() {
                    if let Some(id) = st.right_tab_id.get() {
                        if let Some(cb) = st.callbacks.borrow().on_focus_pane.as_ref() {
                            cb(id);
                        }
                    }
                }
            });
            right_box.add_controller(&click);
        }

        // Doble clic en el divisor de GtkPaned -> restablecer 50/50.
        {
            let click = GestureClick::new();
            click.set_button(1);
            click.set_propagation_phase(PropagationPhase::Capture);
            let inner_weak = Rc::downgrade(&inner);
            click.connect_pressed(move |n_press, x, _| {
                if n_press != 2 {
                    return;
                }
                let Some(st) = inner_weak.upgrade() else {
                    return;
                };
                let pos = st.paned.position() as f64;
                if (x - pos).abs() <= 14.0 {
                    st.ratio.set(0.5);
                    let width = st.paned.width();
                    if width > 0 {
                        st.applying_position.set(true);
                        st.paned.set_position(width / 2);
                        st.applying_position.set(false);
                    }
                    if let Some(cb) = st.callbacks.borrow().on_ratio_changed.as_ref() {
                        cb(0.5);
                    }
                }
            });
            paned.add_controller(&click);
        }

        // Arrastrar el divisor -> actualizar proporción en TabSplit.
        {
            let inner_weak = Rc::downgrade(&inner);
            paned.connect_position_notify(move || {
                let Some(st) = inner_weak.upgrade() else {
                    return;
                };
                if st.applying_position.get() {
                    return;
                }
                let width = st.paned.width();
                if width <= 40 {
                    return;
                }
                let pos = st.paned.position();
                let ratio = (pos as f64 / width as f64).clamp(0.2, 0.8);
                if (ratio - st.ratio.get()).abs() > 0.002 {
                    st.ratio.set(ratio);
                    if let Some(cb) = st.callbacks.borrow().on_ratio_changed.as_ref() {
                        cb(ratio);
                    }
                }
            });
        }

        Self { inner }
    }

    pub fn widget(&self) -> &Paned {
        &self.inner.paned
    }

    pub fn set_callbacks(&self, callbacks: SplitViewCallbacks) {
        *self.inner.callbacks.borrow_mut() = callbacks;
    }

    /// Monta dos widgets (`BrowserView`) en los paneles izquierdo y derecho del `GtkPaned`.
    pub fn set_panes(
        &self,
        left_id: u64,
        left_widget: &impl AsWidget,
        right_id: u64,
        right_widget: &impl AsWidget,
        ratio: f64,
        focused_id: u64,
    ) {
        self.inner.left_tab_id.set(Some(left_id));
        self.inner.right_tab_id.set(Some(right_id));

        left_widget.unparent();
        right_widget.unparent();

        self.inner.left_box.append(left_widget);
        self.inner.right_box.append(right_widget);

        self.set_focused_pane(focused_id);
        self.apply_ratio(ratio);
    }

    /// Actualiza el indicador visual de foco entre el panel izquierdo y el derecho.
    pub fn set_focused_pane(&self, focused_id: u64) {
        let left_focused = self.inner.left_tab_id.get() == Some(focused_id);
        let right_focused = self.inner.right_tab_id.get() == Some(focused_id);

        if left_focused {
            self.inner.left_box.add_css_class("focused");
        } else {
            self.inner.left_box.remove_css_class("focused");
        }

        if right_focused {
            self.inner.right_box.add_css_class("focused");
        } else {
            self.inner.right_box.remove_css_class("focused");
        }
    }

    /// Aplica la proporción `[0.2..0.8]` al `GtkPaned` (diferido un tick si aún no tiene ancho).
    pub fn apply_ratio(&self, ratio: f64) {
        let clamped = if ratio.is_finite() {
            ratio.clamp(0.2, 0.8)
        } else {
            0.5
        };
        self.inner.ratio.set(clamped);

        let width = self.inner.paned.width();
        if width > 40 {
            self.inner.applying_position.set(true);
            self.inner
                .paned
                .set_position((width as f64 * clamped).round() as i32);
            self.inner.applying_position.set(false);
        } else {
            let inner_weak = Rc::downgrade(&self.inner);
            glib::idle_add_local_once(move || {
                let Some(st) = inner_weak.upgrade() else {
                    return;
                };
                let w = st.paned.width();
                if w > 40 {
                    let r = st.ratio.get();
                    st.applying_position.set(true);
                    st.paned.set_position((w as f64 * r).round() as i32);
                    st.applying_position.set(false);
                }
            });
        }
    }

    /// Desmonta el contenedor de su pestaña actual.
    pub fn detach(&self) {
        self.inner.left_tab_id.set(None);
        self.inner.right_tab_id.set(None);
        self.inner.left_box.remove_css_class("focused");
        self.inner.right_box.remove_css_class("focused");
        self.inner.paned.unparent();
    }
}

impl Default for SplitViewContainer {
    fn default() -> Self {
        Self::new()
    }
}
