//! Contenedor visual de Split View (`PaneStage.swift` / `SplitView.swift`).
//!
//! Envuelve un `GtkPaned` horizontal dentro de un `GtkOverlay` con dos paneles
//! (`BrowserView`) independientes, indicador de foco nítido con transición suave,
//! micro-barra flotante en hover (intercambiar izquierda/derecha, presets `50/50`,
//! `65/35`, `35/65` y estado de sync-scroll), doble clic en el divisor para 50/50
//! y sincronización de proporciones con `TabSplit`.

use crate::icons::{self, IconId};
use dot_core::split::SplitRatioPreset;
use gnome_platform::gdk::Display;
use gnome_platform::glib;
use gnome_platform::gtk::{
    Align, AsController, AsWidget, BoxWidget, Button, CssProvider, EventControllerMotion,
    GestureClick, Label, Orientation, Overlay, Paned, PropagationPhase, Revealer,
    RevealerTransitionType, STYLE_PROVIDER_PRIORITY_APPLICATION,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Once;

const SPLIT_VIEW_CSS: &str = r#"
box.search-split-pane {
    border: 2px solid transparent;
    border-radius: 8px;
    transition-property: border-color, box-shadow, opacity;
    transition-duration: 150ms;
    transition-timing-function: cubic-bezier(0.2, 0, 0, 1);
}

box.search-split-pane.focused {
    border-color: alpha(@accent_bg_color, 0.72);
    box-shadow: inset 0 0 0 1px alpha(@accent_bg_color, 0.28);
    opacity: 1.0;
}

box.search-split-pane.unfocused {
    opacity: 0.96;
}

box.search-split-toolbar {
    margin-top: 8px;
    padding: 4px;
    border-radius: 12px;
    background-color: alpha(@window_bg_color, 0.90);
    border: 1px solid alpha(currentColor, 0.12);
    box-shadow: 0 4px 14px alpha(#000000, 0.16);
}

button.search-split-tool-btn,
togglebutton.search-split-tool-btn {
    min-height: 26px;
    min-width: 26px;
    padding: 3px 8px;
    border-radius: 8px;
    font-size: 0.78rem;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    transition-property: background-color, color, transform;
    transition-duration: 150ms;
    transition-timing-function: cubic-bezier(0.2, 0, 0, 1);
}

button.search-split-tool-btn:hover,
togglebutton.search-split-tool-btn:hover {
    background-color: alpha(currentColor, 0.08);
}

button.search-split-tool-btn:active,
togglebutton.search-split-tool-btn:active {
    transform: scale(0.96);
}

button.search-split-tool-btn.active,
togglebutton.search-split-tool-btn:checked,
togglebutton.search-split-tool-btn.active {
    background-color: alpha(@accent_bg_color, 0.20);
    color: @accent_color;
}
"#;

fn ensure_split_css() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(display) = Display::default_display() {
            let provider = CssProvider::new();
            provider.load_from_string(SPLIT_VIEW_CSS);
            provider.add_to_display(&display, STYLE_PROVIDER_PRIORITY_APPLICATION + 1);
        }
    });
}

/// Callbacks emitidos por `SplitViewContainer` hacia `BrowserWindow`.
#[derive(Default)]
pub struct SplitViewCallbacks {
    pub on_focus_pane: Option<Box<dyn Fn(u64)>>,
    pub on_ratio_changed: Option<Box<dyn Fn(f64)>>,
}

pub type SwapPanesCallback = Box<dyn Fn(u64, u64)>;
pub type SyncScrollCallback = Box<dyn Fn(bool)>;

struct SplitViewInner {
    overlay: Overlay,
    paned: Paned,
    left_box: RefCell<BoxWidget>,
    right_box: RefCell<BoxWidget>,
    left_tab_id: Cell<Option<u64>>,
    right_tab_id: Cell<Option<u64>>,
    focused_tab_id: Cell<Option<u64>>,
    ratio: Cell<f64>,
    sync_scroll: Cell<bool>,
    applying_position: Cell<bool>,
    toolbar_revealer: Revealer,
    preset_buttons: [(SplitRatioPreset, Button); 3],
    sync_btn: Button,
    callbacks: RefCell<SplitViewCallbacks>,
    on_swap_panes: RefCell<Option<SwapPanesCallback>>,
    on_sync_scroll_changed: RefCell<Option<SyncScrollCallback>>,
}

/// Contenedor reutilizable de `GtkPaned` para mostrar dos pestañas lado a lado.
#[derive(Clone)]
pub struct SplitViewContainer {
    inner: Rc<SplitViewInner>,
}

impl SplitViewContainer {
    pub fn new() -> Self {
        ensure_split_css();

        let overlay = Overlay::new();
        overlay.set_hexpand(true);
        overlay.set_vexpand(true);

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
        overlay.set_child(Some(&paned));

        // Micro-barra flotante centrada en la parte superior.
        let toolbar_revealer = Revealer::new();
        toolbar_revealer.set_transition_type(RevealerTransitionType::Crossfade);
        toolbar_revealer.set_transition_duration(150);
        toolbar_revealer.set_reveal_child(false);
        toolbar_revealer.set_halign(Align::Center);
        toolbar_revealer.set_valign(Align::Start);

        let toolbar_box = BoxWidget::new(Orientation::Horizontal, 10);
        toolbar_box.add_css_class("search-split-toolbar");

        let swap_btn = Button::new();
        swap_btn.add_css_class("flat");
        swap_btn.add_css_class("search-split-tool-btn");
        swap_btn.set_tooltip_text(Some("Intercambiar paneles (Izquierda / Derecha)"));
        let swap_content = BoxWidget::new(Orientation::Horizontal, 4);
        swap_content.append(&icons::image(IconId::Refresh, 13));
        swap_content.append(&Label::new(Some("Swap")));
        swap_btn.set_child(Some(&swap_content));

        let preset_group = BoxWidget::new(Orientation::Horizontal, 4);
        let btn_even = Self::build_preset_button(SplitRatioPreset::Even);
        let btn_left = Self::build_preset_button(SplitRatioPreset::LeftWide);
        let btn_right = Self::build_preset_button(SplitRatioPreset::RightWide);
        btn_even.add_css_class("active");
        preset_group.append(&btn_even);
        preset_group.append(&btn_left);
        preset_group.append(&btn_right);

        let sync_btn = Button::new();
        sync_btn.add_css_class("flat");
        sync_btn.add_css_class("search-split-tool-btn");
        sync_btn.set_tooltip_text(Some("Sincronizar desplazamiento entre paneles"));
        let sync_content = BoxWidget::new(Orientation::Horizontal, 4);
        sync_content.append(&icons::image(IconId::ArrowDown, 13));
        sync_content.append(&Label::new(Some("Sync")));
        sync_btn.set_child(Some(&sync_content));

        toolbar_box.append(&swap_btn);
        toolbar_box.append(&preset_group);
        toolbar_box.append(&sync_btn);
        toolbar_revealer.set_child(Some(&toolbar_box));

        overlay.add_overlay(&toolbar_revealer);
        overlay.set_measure_overlay(&toolbar_revealer, false);

        let inner = Rc::new(SplitViewInner {
            overlay: overlay.clone(),
            paned: paned.clone(),
            left_box: RefCell::new(left_box.clone()),
            right_box: RefCell::new(right_box.clone()),
            left_tab_id: Cell::new(None),
            right_tab_id: Cell::new(None),
            focused_tab_id: Cell::new(None),
            ratio: Cell::new(0.5),
            sync_scroll: Cell::new(false),
            applying_position: Cell::new(false),
            toolbar_revealer: toolbar_revealer.clone(),
            preset_buttons: [
                (SplitRatioPreset::Even, btn_even.clone()),
                (SplitRatioPreset::LeftWide, btn_left.clone()),
                (SplitRatioPreset::RightWide, btn_right.clone()),
            ],
            sync_btn: sync_btn.clone(),
            callbacks: RefCell::new(SplitViewCallbacks::default()),
            on_swap_panes: RefCell::new(None),
            on_sync_scroll_changed: RefCell::new(None),
        });

        // Mostrar/ocultar micro-barra al pasar el cursor sobre el Split View.
        {
            let motion = EventControllerMotion::new();
            let rev_enter = toolbar_revealer.clone();
            motion.connect_enter(move |_, _| {
                rev_enter.set_reveal_child(true);
            });
            let rev_leave = toolbar_revealer;
            motion.connect_leave(move || {
                rev_leave.set_reveal_child(false);
            });
            overlay.add_controller(&motion);
        }

        // Foco al hacer clic sobre cualquiera de los dos paneles.
        Self::attach_pane_focus_click(&inner, &left_box);
        Self::attach_pane_focus_click(&inner, &right_box);

        // Botón Swap Panes.
        {
            let container = Self {
                inner: inner.clone(),
            };
            swap_btn.connect_clicked(move || {
                container.swap_panes();
            });
        }

        // Botones de presets de proporción.
        for (preset, btn) in [
            (SplitRatioPreset::Even, btn_even),
            (SplitRatioPreset::LeftWide, btn_left),
            (SplitRatioPreset::RightWide, btn_right),
        ] {
            let container = Self {
                inner: inner.clone(),
            };
            btn.connect_clicked(move || {
                container.apply_ratio_preset(preset);
            });
        }

        // Botón Sync-scroll.
        {
            let container = Self {
                inner: inner.clone(),
            };
            sync_btn.connect_clicked(move || {
                container.toggle_sync_scroll();
            });
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
                    Self::sync_preset_highlight(&st, 0.5);
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
                    Self::sync_preset_highlight(&st, ratio);
                    if let Some(cb) = st.callbacks.borrow().on_ratio_changed.as_ref() {
                        cb(ratio);
                    }
                }
            });
        }

        Self { inner }
    }

    fn build_preset_button(preset: SplitRatioPreset) -> Button {
        let btn = Button::with_label(preset.label());
        btn.add_css_class("flat");
        btn.add_css_class("search-split-tool-btn");
        btn.set_tooltip_text(Some(&format!("Proporción {}", preset.label())));
        btn
    }

    fn attach_pane_focus_click(inner: &Rc<SplitViewInner>, pane: &BoxWidget) {
        let click = GestureClick::new();
        click.set_button(0);
        click.set_propagation_phase(PropagationPhase::Capture);
        let inner_weak = Rc::downgrade(inner);
        let pane_ptr = pane.as_widget_ptr() as usize;
        click.connect_pressed(move |_, _, _| {
            let Some(st) = inner_weak.upgrade() else {
                return;
            };
            let target_id = if st.left_box.borrow().as_widget_ptr() as usize == pane_ptr {
                st.left_tab_id.get()
            } else {
                st.right_tab_id.get()
            };
            if let Some(id) = target_id {
                if let Some(cb) = st.callbacks.borrow().on_focus_pane.as_ref() {
                    cb(id);
                }
            }
        });
        pane.add_controller(&click);
    }

    fn sync_preset_highlight(st: &SplitViewInner, ratio: f64) {
        let active = SplitRatioPreset::from_fraction(ratio, 0.025);
        for (preset, btn) in &st.preset_buttons {
            if Some(*preset) == active {
                btn.add_css_class("active");
            } else {
                btn.remove_css_class("active");
            }
        }
    }

    pub fn widget(&self) -> &Overlay {
        &self.inner.overlay
    }

    #[allow(dead_code)]
    pub fn paned(&self) -> &Paned {
        &self.inner.paned
    }

    pub fn set_callbacks(&self, callbacks: SplitViewCallbacks) {
        *self.inner.callbacks.borrow_mut() = callbacks;
    }

    #[allow(dead_code)]
    pub fn set_on_swap_panes(&self, cb: Option<SwapPanesCallback>) {
        *self.inner.on_swap_panes.borrow_mut() = cb;
    }

    #[allow(dead_code)]
    pub fn set_on_sync_scroll_changed(&self, cb: Option<SyncScrollCallback>) {
        *self.inner.on_sync_scroll_changed.borrow_mut() = cb;
    }

    #[allow(dead_code)]
    pub fn set_toolbar_visible(&self, visible: bool) {
        self.inner.toolbar_revealer.set_reveal_child(visible);
    }

    #[allow(dead_code)]
    pub fn is_toolbar_visible(&self) -> bool {
        self.inner.toolbar_revealer.reveals_child()
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

        self.inner.left_box.borrow().append(left_widget);
        self.inner.right_box.borrow().append(right_widget);

        self.set_focused_pane(focused_id);
        self.apply_ratio(ratio);
    }

    /// Intercambia visualmente el panel izquierdo y derecho, invirtiendo la proporción.
    pub fn swap_panes(&self) {
        let Some(old_left) = self.inner.left_tab_id.get() else {
            return;
        };
        let Some(old_right) = self.inner.right_tab_id.get() else {
            return;
        };

        let new_left_box = self.inner.right_box.borrow().clone();
        let new_right_box = self.inner.left_box.borrow().clone();

        new_left_box.unparent();
        new_right_box.unparent();
        self.inner.paned.set_start_child(Some(&new_left_box));
        self.inner.paned.set_end_child(Some(&new_right_box));

        *self.inner.left_box.borrow_mut() = new_left_box;
        *self.inner.right_box.borrow_mut() = new_right_box;
        self.inner.left_tab_id.set(Some(old_right));
        self.inner.right_tab_id.set(Some(old_left));

        let inverted_ratio = (1.0 - self.inner.ratio.get()).clamp(0.2, 0.8);
        self.apply_ratio(inverted_ratio);

        if let Some(focused) = self.inner.focused_tab_id.get() {
            self.set_focused_pane(focused);
        }

        if let Some(cb) = self.inner.callbacks.borrow().on_ratio_changed.as_ref() {
            cb(inverted_ratio);
        }
        if let Some(cb) = self.inner.on_swap_panes.borrow().as_ref() {
            cb(old_right, old_left);
        }
    }

    /// Aplica uno de los presets de proporción (`50/50`, `65/35`, `35/65`).
    pub fn apply_ratio_preset(&self, preset: SplitRatioPreset) {
        let fraction = preset.fraction();
        self.apply_ratio(fraction);
        if let Some(cb) = self.inner.callbacks.borrow().on_ratio_changed.as_ref() {
            cb(fraction);
        }
    }

    /// Devuelve el preset activo si la proporción actual coincide dentro de `0.025`.
    #[allow(dead_code)]
    pub fn active_ratio_preset(&self) -> Option<SplitRatioPreset> {
        SplitRatioPreset::from_fraction(self.inner.ratio.get(), 0.025)
    }

    /// Activa o desactiva la sincronización de desplazamiento entre paneles.
    pub fn set_sync_scroll(&self, enabled: bool) {
        let changed = self.inner.sync_scroll.replace(enabled) != enabled;
        if enabled {
            self.inner.sync_btn.add_css_class("active");
        } else {
            self.inner.sync_btn.remove_css_class("active");
        }
        if changed {
            if let Some(cb) = self.inner.on_sync_scroll_changed.borrow().as_ref() {
                cb(enabled);
            }
        }
    }

    /// Alterna el estado de sincronización de desplazamiento y devuelve el nuevo valor.
    pub fn toggle_sync_scroll(&self) -> bool {
        let next = !self.inner.sync_scroll.get();
        self.set_sync_scroll(next);
        next
    }

    #[allow(dead_code)]
    pub fn is_sync_scroll(&self) -> bool {
        self.inner.sync_scroll.get()
    }

    /// Actualiza el indicador visual de foco entre el panel izquierdo y el derecho.
    pub fn set_focused_pane(&self, focused_id: u64) {
        self.inner.focused_tab_id.set(Some(focused_id));
        let left_focused = self.inner.left_tab_id.get() == Some(focused_id);
        let right_focused = self.inner.right_tab_id.get() == Some(focused_id);

        let left_box = self.inner.left_box.borrow();
        if left_focused {
            left_box.add_css_class("focused");
            left_box.remove_css_class("unfocused");
        } else {
            left_box.remove_css_class("focused");
            left_box.add_css_class("unfocused");
        }

        let right_box = self.inner.right_box.borrow();
        if right_focused {
            right_box.add_css_class("focused");
            right_box.remove_css_class("unfocused");
        } else {
            right_box.remove_css_class("focused");
            right_box.add_css_class("unfocused");
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
        Self::sync_preset_highlight(&self.inner, clamped);

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
        self.inner.focused_tab_id.set(None);
        self.inner.toolbar_revealer.set_reveal_child(false);
        {
            let left_box = self.inner.left_box.borrow();
            left_box.remove_css_class("focused");
            left_box.remove_css_class("unfocused");
        }
        {
            let right_box = self.inner.right_box.borrow();
            right_box.remove_css_class("focused");
            right_box.remove_css_class("unfocused");
        }
        self.inner.overlay.unparent();
    }
}

impl Default for SplitViewContainer {
    fn default() -> Self {
        Self::new()
    }
}
