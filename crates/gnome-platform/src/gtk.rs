use crate::gdk::{
    ContentProvider, Display, DragAction, MemoryTexture, ModifierType, Rectangle, Texture,
};
use crate::gio::{Icon, Menu, SimpleActionGroup};
use crate::object::{
    connect_signal_bool_ret, connect_signal_drag_prepare, connect_signal_drop_string,
    connect_signal_f64_f64, connect_signal_f64_f64_bool, connect_signal_i32_f64_f64,
    connect_signal_i32_i32, connect_signal_key_pressed, connect_signal_key_released,
    connect_signal_notify, connect_signal_ptr, connect_signal_void, to_cstring, ObjectRef,
    SignalHandlerId,
};
use crate::sys;
use bitflags::bitflags;
use libc::c_char;
use std::ffi::{CStr, CString};
use std::path::{Path, PathBuf};
use std::ptr;

pub const STYLE_PROVIDER_PRIORITY_APPLICATION: u32 = 600;

pub fn version() -> (u32, u32, u32) {
    unsafe {
        (
            sys::gtk_get_major_version(),
            sys::gtk_get_minor_version(),
            sys::gtk_get_micro_version(),
        )
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overflow {
    Visible = 0,
    Hidden = 1,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackTransitionType {
    None = 0,
    Crossfade = 1,
    SlideRight = 2,
    SlideLeft = 3,
    SlideUp = 4,
    SlideDown = 5,
    SlideLeftRight = 6,
    SlideUpDown = 7,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackType {
    Start = 0,
    End = 1,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Horizontal = 0,
    Vertical = 1,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Fill = 0,
    Start = 1,
    End = 2,
    Center = 3,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentFit {
    Fill = 0,
    Contain = 1,
    Cover = 2,
    ScaleDown = 3,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropagationPhase {
    None = 0,
    Capture = 1,
    Bubble = 2,
    Target = 3,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EllipsizeMode {
    None = 0,
    Start = 1,
    Middle = 2,
    End = 3,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputPurpose {
    FreeForm = 0,
    Url = 5,
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct InputHints: u32 {
        const NONE = 0;
        const NO_SPELLCHECK = 1 << 1;
        const NO_EMOJI = 1 << 10;
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ScrollFlags: i32 {
        const NONE = 0;
        const VERTICAL = 1 << 0;
        const HORIZONTAL = 1 << 1;
        const DISCRETE = 1 << 2;
        const KINETIC = 1 << 3;
        const BOTH_AXES = (1 << 0) | (1 << 1);
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevealerTransitionType {
    None = 0,
    Crossfade = 1,
    SlideRight = 2,
    SlideLeft = 3,
    SlideUp = 4,
    SlideDown = 5,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionMode {
    None = 0,
    Single = 1,
    Browse = 2,
    Multiple = 3,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyType {
    Always = 0,
    Automatic = 1,
    Never = 2,
    External = 3,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionType {
    Left = 0,
    Right = 1,
    Top = 2,
    Bottom = 3,
}

pub trait AsController {
    fn as_controller_ptr(&self) -> *mut sys::GtkEventController;

    fn set_propagation_phase(&self, phase: PropagationPhase) {
        unsafe {
            sys::gtk_event_controller_set_propagation_phase(self.as_controller_ptr(), phase as i32);
        }
    }

    fn current_event_state(&self) -> ModifierType {
        let raw =
            unsafe { sys::gtk_event_controller_get_current_event_state(self.as_controller_ptr()) };
        ModifierType::from_bits_truncate(raw)
    }
}

pub trait AsWidget {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget;

    fn set_visible(&self, visible: bool) {
        unsafe {
            sys::gtk_widget_set_visible(
                self.as_widget_ptr(),
                if visible { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    fn is_visible(&self) -> bool {
        unsafe { sys::gtk_widget_get_visible(self.as_widget_ptr()) != 0 }
    }

    fn set_sensitive(&self, sensitive: bool) {
        unsafe {
            sys::gtk_widget_set_sensitive(
                self.as_widget_ptr(),
                if sensitive { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    fn set_hexpand(&self, expand: bool) {
        unsafe {
            sys::gtk_widget_set_hexpand(
                self.as_widget_ptr(),
                if expand { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    fn set_vexpand(&self, expand: bool) {
        unsafe {
            sys::gtk_widget_set_vexpand(
                self.as_widget_ptr(),
                if expand { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    fn set_halign(&self, align: Align) {
        unsafe {
            sys::gtk_widget_set_halign(self.as_widget_ptr(), align as i32);
        }
    }

    fn set_valign(&self, align: Align) {
        unsafe {
            sys::gtk_widget_set_valign(self.as_widget_ptr(), align as i32);
        }
    }

    fn set_margins(&self, top: i32, bottom: i32, start: i32, end: i32) {
        unsafe {
            sys::gtk_widget_set_margin_top(self.as_widget_ptr(), top);
            sys::gtk_widget_set_margin_bottom(self.as_widget_ptr(), bottom);
            sys::gtk_widget_set_margin_start(self.as_widget_ptr(), start);
            sys::gtk_widget_set_margin_end(self.as_widget_ptr(), end);
        }
    }

    fn set_size_request(&self, width: i32, height: i32) {
        unsafe {
            sys::gtk_widget_set_size_request(self.as_widget_ptr(), width, height);
        }
    }

    fn width(&self) -> i32 {
        unsafe { sys::gtk_widget_get_width(self.as_widget_ptr()) }
    }

    fn height(&self) -> i32 {
        unsafe { sys::gtk_widget_get_height(self.as_widget_ptr()) }
    }

    fn scale_factor(&self) -> i32 {
        unsafe { sys::gtk_widget_get_scale_factor(self.as_widget_ptr()).max(1) }
    }

    fn set_focusable(&self, focusable: bool) {
        unsafe {
            sys::gtk_widget_set_focusable(
                self.as_widget_ptr(),
                if focusable { sys::GTRUE } else { sys::GFALSE },
            );
            sys::gtk_widget_set_can_focus(
                self.as_widget_ptr(),
                if focusable { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    fn grab_focus(&self) -> bool {
        unsafe { sys::gtk_widget_grab_focus(self.as_widget_ptr()) != 0 }
    }

    fn has_focus(&self) -> bool {
        unsafe { sys::gtk_widget_has_focus(self.as_widget_ptr()) != 0 }
    }

    fn set_can_target(&self, can_target: bool) {
        unsafe {
            sys::gtk_widget_set_can_target(
                self.as_widget_ptr(),
                if can_target { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    fn add_css_class(&self, css_class: &str) {
        let c_cls = to_cstring(css_class);
        unsafe {
            sys::gtk_widget_add_css_class(self.as_widget_ptr(), c_cls.as_ptr());
        }
    }

    fn remove_css_class(&self, css_class: &str) {
        let c_cls = to_cstring(css_class);
        unsafe {
            sys::gtk_widget_remove_css_class(self.as_widget_ptr(), c_cls.as_ptr());
        }
    }

    fn has_css_class(&self, css_class: &str) -> bool {
        let c_cls = to_cstring(css_class);
        unsafe { sys::gtk_widget_has_css_class(self.as_widget_ptr(), c_cls.as_ptr()) != 0 }
    }

    fn set_parent(&self, parent: &impl AsWidget) {
        unsafe {
            sys::gtk_widget_set_parent(self.as_widget_ptr(), parent.as_widget_ptr());
        }
    }

    fn unparent(&self) {
        unsafe {
            sys::gtk_widget_unparent(self.as_widget_ptr());
        }
    }

    fn insert_action_group(&self, name: &str, group: Option<&SimpleActionGroup>) {
        let c_name = to_cstring(name);
        let ptr = group.map_or(ptr::null_mut(), SimpleActionGroup::as_action_group_ptr);
        unsafe {
            sys::gtk_widget_insert_action_group(self.as_widget_ptr(), c_name.as_ptr(), ptr);
        }
    }

    fn add_controller(&self, controller: &impl AsController) {
        unsafe {
            // `gtk_widget_add_controller` adopta (`transfer full`) una referencia del controlador.
            let ctrl_ptr = controller.as_controller_ptr();
            sys::g_object_ref(ctrl_ptr.cast());
            sys::gtk_widget_add_controller(self.as_widget_ptr(), ctrl_ptr);
        }
    }

    fn set_cursor_from_name(&self, name: Option<&str>) {
        unsafe {
            if let Some(n) = name {
                let c_name = to_cstring(n);
                sys::gtk_widget_set_cursor_from_name(self.as_widget_ptr(), c_name.as_ptr());
            } else {
                sys::gtk_widget_set_cursor_from_name(self.as_widget_ptr(), ptr::null());
            }
        }
    }

    fn set_tooltip_text(&self, text: Option<&str>) {
        unsafe {
            if let Some(t) = text {
                let c_text = to_cstring(t);
                sys::gtk_widget_set_tooltip_text(self.as_widget_ptr(), c_text.as_ptr());
            } else {
                sys::gtk_widget_set_tooltip_text(self.as_widget_ptr(), ptr::null());
            }
        }
    }

    fn set_overflow(&self, overflow: Overflow) {
        unsafe {
            sys::gtk_widget_set_overflow(self.as_widget_ptr(), overflow as i32);
        }
    }

    fn queue_draw(&self) {
        unsafe {
            sys::gtk_widget_queue_draw(self.as_widget_ptr());
        }
    }

    fn connect_scale_factor_notify<F: Fn() + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_notify(self.as_widget_ptr().cast(), "scale-factor", callback)
    }
}

pub trait AsEditable: AsWidget {
    fn as_editable_ptr(&self) -> *mut sys::GtkEditable {
        self.as_widget_ptr().cast()
    }

    fn text(&self) -> String {
        unsafe {
            let ptr = sys::gtk_editable_get_text(self.as_editable_ptr());
            if ptr.is_null() {
                String::new()
            } else {
                CStr::from_ptr(ptr).to_string_lossy().into_owned()
            }
        }
    }

    fn set_text(&self, text: &str) {
        let c_text = to_cstring(text);
        unsafe {
            sys::gtk_editable_set_text(self.as_editable_ptr(), c_text.as_ptr());
        }
    }

    fn select_region(&self, start_pos: i32, end_pos: i32) {
        unsafe {
            sys::gtk_editable_select_region(self.as_editable_ptr(), start_pos, end_pos);
        }
    }

    fn select_all(&self) {
        self.select_region(0, -1);
    }

    fn selection_bounds(&self) -> Option<(i32, i32)> {
        let mut start: libc::c_int = 0;
        let mut end: libc::c_int = 0;
        let has_sel = unsafe {
            sys::gtk_editable_get_selection_bounds(self.as_editable_ptr(), &mut start, &mut end)
                != 0
        };
        if has_sel {
            Some((start, end))
        } else {
            None
        }
    }

    fn delete_selection(&self) {
        unsafe {
            sys::gtk_editable_delete_selection(self.as_editable_ptr());
        }
    }

    fn set_position(&self, position: i32) {
        unsafe {
            sys::gtk_editable_set_position(self.as_editable_ptr(), position);
        }
    }

    fn position(&self) -> i32 {
        unsafe { sys::gtk_editable_get_position(self.as_editable_ptr()) }
    }

    fn connect_changed<F: Fn() + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_void(self.as_widget_ptr().cast(), "changed", false, callback)
    }
}

#[derive(Clone)]
pub struct Widget {
    pub(crate) obj: ObjectRef,
}

impl AsWidget for Widget {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct BoxWidget {
    obj: ObjectRef,
}

impl BoxWidget {
    pub fn new(orientation: Orientation, spacing: i32) -> Self {
        let ptr = unsafe { sys::gtk_box_new(orientation as i32, spacing) };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn append(&self, child: &impl AsWidget) {
        unsafe {
            sys::gtk_box_append(self.obj.as_ptr().cast(), child.as_widget_ptr());
        }
    }

    pub fn remove(&self, child: &impl AsWidget) {
        unsafe {
            sys::gtk_box_remove(self.obj.as_ptr().cast(), child.as_widget_ptr());
        }
    }
}

impl AsWidget for BoxWidget {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct Overlay {
    obj: ObjectRef,
}

impl Overlay {
    pub fn new() -> Self {
        let ptr = unsafe { sys::gtk_overlay_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_child(&self, child: Option<&impl AsWidget>) {
        unsafe {
            let ptr = child.map_or(ptr::null_mut(), AsWidget::as_widget_ptr);
            sys::gtk_overlay_set_child(self.obj.as_ptr().cast(), ptr);
        }
    }

    pub fn add_overlay(&self, widget: &impl AsWidget) {
        unsafe {
            sys::gtk_overlay_add_overlay(self.obj.as_ptr().cast(), widget.as_widget_ptr());
        }
    }

    pub fn set_measure_overlay(&self, widget: &impl AsWidget, measure: bool) {
        unsafe {
            sys::gtk_overlay_set_measure_overlay(
                self.obj.as_ptr().cast(),
                widget.as_widget_ptr(),
                if measure { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn set_clip_overlay(&self, widget: &impl AsWidget, clip: bool) {
        unsafe {
            sys::gtk_overlay_set_clip_overlay(
                self.obj.as_ptr().cast(),
                widget.as_widget_ptr(),
                if clip { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }
}

impl Default for Overlay {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for Overlay {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct Picture {
    obj: ObjectRef,
}

impl Picture {
    pub fn new() -> Self {
        let ptr = unsafe { sys::gtk_picture_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_texture(&self, texture: Option<&MemoryTexture>) {
        unsafe {
            let paintable = texture.map_or(ptr::null_mut(), MemoryTexture::as_paintable_ptr);
            sys::gtk_picture_set_paintable(self.obj.as_ptr().cast(), paintable);
        }
    }

    pub fn set_can_shrink(&self, can_shrink: bool) {
        unsafe {
            sys::gtk_picture_set_can_shrink(
                self.obj.as_ptr().cast(),
                if can_shrink { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn set_content_fit(&self, fit: ContentFit) {
        unsafe {
            sys::gtk_picture_set_content_fit(self.obj.as_ptr().cast(), fit as i32);
        }
    }
}

impl Default for Picture {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for Picture {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct DrawingArea {
    obj: ObjectRef,
}

impl DrawingArea {
    pub fn new() -> Self {
        let ptr = unsafe { sys::gtk_drawing_area_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn connect_resize<F: Fn(i32, i32) + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_i32_i32(self.obj.as_ptr(), "resize", callback)
    }
}

impl Default for DrawingArea {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for DrawingArea {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct Entry {
    obj: ObjectRef,
}

impl Entry {
    pub fn new() -> Self {
        let ptr = unsafe { sys::gtk_entry_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_placeholder_text(&self, text: &str) {
        let c_text = to_cstring(text);
        unsafe {
            sys::gtk_entry_set_placeholder_text(self.obj.as_ptr().cast(), c_text.as_ptr());
        }
    }

    pub fn set_alignment(&self, xalign: f32) {
        unsafe {
            sys::gtk_entry_set_alignment(self.obj.as_ptr().cast(), xalign);
        }
    }

    pub fn set_input_purpose(&self, purpose: InputPurpose) {
        unsafe {
            sys::gtk_entry_set_input_purpose(self.obj.as_ptr().cast(), purpose as i32);
        }
    }

    pub fn set_input_hints(&self, hints: InputHints) {
        unsafe {
            sys::gtk_entry_set_input_hints(self.obj.as_ptr().cast(), hints.bits());
        }
    }

    pub fn grab_focus_without_selecting(&self) -> bool {
        unsafe { sys::gtk_entry_grab_focus_without_selecting(self.obj.as_ptr().cast()) != 0 }
    }

    pub fn connect_activate<F: Fn() + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_void(self.obj.as_ptr(), "activate", false, callback)
    }
}

impl Default for Entry {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for Entry {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

impl AsEditable for Entry {}

#[derive(Clone)]
pub struct SearchEntry {
    obj: ObjectRef,
}

impl SearchEntry {
    pub fn new() -> Self {
        let ptr = unsafe { sys::gtk_search_entry_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_placeholder_text(&self, text: &str) {
        let c_text = to_cstring(text);
        unsafe {
            sys::gtk_search_entry_set_placeholder_text(self.obj.as_ptr().cast(), c_text.as_ptr());
        }
    }

    pub fn connect_activate<F: Fn() + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_void(self.obj.as_ptr(), "activate", false, callback)
    }

    pub fn connect_search_changed<F: Fn() + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_void(self.obj.as_ptr(), "search-changed", false, callback)
    }

    pub fn connect_stop_search<F: Fn() + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_void(self.obj.as_ptr(), "stop-search", false, callback)
    }

    pub fn connect_next_match<F: Fn() + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_void(self.obj.as_ptr(), "next-match", false, callback)
    }

    pub fn connect_previous_match<F: Fn() + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_void(self.obj.as_ptr(), "previous-match", false, callback)
    }
}

impl Default for SearchEntry {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for SearchEntry {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

impl AsEditable for SearchEntry {}

#[derive(Clone)]
pub struct Label {
    obj: ObjectRef,
}

impl Label {
    pub fn new(text: Option<&str>) -> Self {
        let ptr = unsafe {
            if let Some(t) = text {
                let c_text = to_cstring(t);
                sys::gtk_label_new(c_text.as_ptr())
            } else {
                sys::gtk_label_new(ptr::null())
            }
        };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_text(&self, text: &str) {
        let c_text = to_cstring(text);
        unsafe {
            sys::gtk_label_set_text(self.obj.as_ptr().cast(), c_text.as_ptr());
        }
    }

    pub fn set_ellipsize(&self, mode: EllipsizeMode) {
        unsafe {
            sys::gtk_label_set_ellipsize(self.obj.as_ptr().cast(), mode as i32);
        }
    }

    pub fn set_xalign(&self, xalign: f32) {
        unsafe {
            sys::gtk_label_set_xalign(self.obj.as_ptr().cast(), xalign);
        }
    }

    pub fn set_max_width_chars(&self, chars: i32) {
        unsafe {
            sys::gtk_label_set_max_width_chars(self.obj.as_ptr().cast(), chars);
        }
    }

    pub fn set_wrap(&self, wrap: bool) {
        unsafe {
            sys::gtk_label_set_wrap(
                self.obj.as_ptr().cast(),
                if wrap { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }
}

impl AsWidget for Label {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct Button {
    obj: ObjectRef,
}

impl Button {
    pub fn new() -> Self {
        let ptr = unsafe { sys::gtk_button_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn with_label(label: &str) -> Self {
        let c_label = to_cstring(label);
        let ptr = unsafe { sys::gtk_button_new_with_label(c_label.as_ptr()) };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn from_icon_name(icon_name: &str) -> Self {
        let c_icon = to_cstring(icon_name);
        let ptr = unsafe { sys::gtk_button_new_from_icon_name(c_icon.as_ptr()) };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_label(&self, label: &str) {
        let c_label = to_cstring(label);
        unsafe {
            sys::gtk_button_set_label(self.obj.as_ptr().cast(), c_label.as_ptr());
        }
    }

    pub fn set_icon_name(&self, icon_name: &str) {
        let c_icon = to_cstring(icon_name);
        unsafe {
            sys::gtk_button_set_icon_name(self.obj.as_ptr().cast(), c_icon.as_ptr());
        }
    }

    pub fn set_child(&self, child: Option<&impl AsWidget>) {
        let ptr = child.map_or(ptr::null_mut(), AsWidget::as_widget_ptr);
        unsafe {
            sys::gtk_button_set_child(self.obj.as_ptr().cast(), ptr);
        }
    }

    pub fn connect_clicked<F: Fn() + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_void(self.obj.as_ptr(), "clicked", false, callback)
    }
}

impl Default for Button {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for Button {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct ProgressBar {
    obj: ObjectRef,
}

impl ProgressBar {
    pub fn new() -> Self {
        let ptr = unsafe { sys::gtk_progress_bar_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_fraction(&self, fraction: f64) {
        unsafe {
            sys::gtk_progress_bar_set_fraction(self.obj.as_ptr().cast(), fraction.clamp(0.0, 1.0));
        }
    }

    pub fn pulse(&self) {
        unsafe {
            sys::gtk_progress_bar_pulse(self.obj.as_ptr().cast());
        }
    }
}

impl Default for ProgressBar {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for ProgressBar {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct Revealer {
    obj: ObjectRef,
}

impl Revealer {
    pub fn new() -> Self {
        let ptr = unsafe { sys::gtk_revealer_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_child(&self, child: Option<&impl AsWidget>) {
        unsafe {
            let ptr = child.map_or(ptr::null_mut(), AsWidget::as_widget_ptr);
            sys::gtk_revealer_set_child(self.obj.as_ptr().cast(), ptr);
        }
    }

    pub fn set_reveal_child(&self, reveal: bool) {
        unsafe {
            sys::gtk_revealer_set_reveal_child(
                self.obj.as_ptr().cast(),
                if reveal { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn reveals_child(&self) -> bool {
        unsafe { sys::gtk_revealer_get_reveal_child(self.obj.as_ptr().cast()) != 0 }
    }

    pub fn set_transition_type(&self, transition: RevealerTransitionType) {
        unsafe {
            sys::gtk_revealer_set_transition_type(self.obj.as_ptr().cast(), transition as i32);
        }
    }

    pub fn set_transition_duration(&self, ms: u32) {
        unsafe {
            sys::gtk_revealer_set_transition_duration(self.obj.as_ptr().cast(), ms);
        }
    }
}

impl Default for Revealer {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for Revealer {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct Stack {
    obj: ObjectRef,
}

impl Stack {
    pub fn new() -> Self {
        let ptr = unsafe { sys::gtk_stack_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn add_named(&self, child: &impl AsWidget, name: &str) {
        let c = to_cstring(name);
        unsafe {
            let _ = sys::gtk_stack_add_named(
                self.obj.as_ptr().cast(),
                child.as_widget_ptr(),
                c.as_ptr(),
            );
        }
    }

    pub fn set_visible_child_name(&self, name: &str) {
        let c = to_cstring(name);
        unsafe {
            sys::gtk_stack_set_visible_child_name(self.obj.as_ptr().cast(), c.as_ptr());
        }
    }

    pub fn visible_child_name(&self) -> Option<String> {
        unsafe {
            let ptr = sys::gtk_stack_get_visible_child_name(self.obj.as_ptr().cast());
            if ptr.is_null() {
                None
            } else {
                Some(CStr::from_ptr(ptr).to_string_lossy().into_owned())
            }
        }
    }

    pub fn set_transition_type(&self, transition: StackTransitionType) {
        unsafe {
            sys::gtk_stack_set_transition_type(self.obj.as_ptr().cast(), transition as i32);
        }
    }

    pub fn set_transition_duration(&self, ms: u32) {
        unsafe {
            sys::gtk_stack_set_transition_duration(self.obj.as_ptr().cast(), ms);
        }
    }
}

impl Default for Stack {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for Stack {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct Scale {
    obj: ObjectRef,
}

impl Scale {
    pub fn with_range(orientation: Orientation, min: f64, max: f64, step: f64) -> Self {
        let ptr = unsafe { sys::gtk_scale_new_with_range(orientation as i32, min, max, step) };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_digits(&self, digits: i32) {
        unsafe {
            sys::gtk_scale_set_digits(self.obj.as_ptr().cast(), digits);
        }
    }

    pub fn set_draw_value(&self, draw: bool) {
        unsafe {
            sys::gtk_scale_set_draw_value(
                self.obj.as_ptr().cast(),
                if draw { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn value(&self) -> f64 {
        unsafe { sys::gtk_range_get_value(self.obj.as_ptr().cast()) }
    }

    pub fn set_value(&self, value: f64) {
        unsafe {
            sys::gtk_range_set_value(self.obj.as_ptr().cast(), value);
        }
    }

    pub fn connect_value_changed<F: Fn() + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_void(self.obj.as_ptr(), "value-changed", false, callback)
    }
}

impl AsWidget for Scale {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct CssProvider {
    obj: ObjectRef,
}

impl CssProvider {
    pub fn new() -> Self {
        let ptr = unsafe { sys::gtk_css_provider_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn load_from_string(&self, css: &str) {
        let c_css = to_cstring(css);
        unsafe {
            sys::gtk_css_provider_load_from_string(self.obj.as_ptr().cast(), c_css.as_ptr());
        }
    }

    pub fn add_to_display(&self, display: &Display, priority: u32) {
        unsafe {
            sys::gtk_style_context_add_provider_for_display(
                display.as_ptr(),
                self.obj.as_ptr().cast(),
                priority,
            );
        }
    }
}

impl Default for CssProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct EventControllerMotion {
    obj: ObjectRef,
}

impl EventControllerMotion {
    pub fn new() -> Self {
        let ptr = unsafe { sys::gtk_event_controller_motion_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn connect_enter<F: Fn(f64, f64) + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_f64_f64(self.obj.as_ptr(), "enter", callback)
    }

    pub fn connect_motion<F: Fn(f64, f64) + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_f64_f64(self.obj.as_ptr(), "motion", callback)
    }

    pub fn connect_leave<F: Fn() + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_void(self.obj.as_ptr(), "leave", false, callback)
    }
}

impl Default for EventControllerMotion {
    fn default() -> Self {
        Self::new()
    }
}

impl AsController for EventControllerMotion {
    fn as_controller_ptr(&self) -> *mut sys::GtkEventController {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct EventControllerScroll {
    obj: ObjectRef,
}

impl EventControllerScroll {
    pub fn new(flags: ScrollFlags) -> Self {
        let ptr = unsafe { sys::gtk_event_controller_scroll_new(flags.bits()) };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn connect_scroll<F: Fn(f64, f64) -> bool + 'static>(
        &self,
        callback: F,
    ) -> SignalHandlerId {
        connect_signal_f64_f64_bool(self.obj.as_ptr(), "scroll", callback)
    }
}

impl AsController for EventControllerScroll {
    fn as_controller_ptr(&self) -> *mut sys::GtkEventController {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct EventControllerKey {
    obj: ObjectRef,
}

impl EventControllerKey {
    pub fn new() -> Self {
        let ptr = unsafe { sys::gtk_event_controller_key_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn connect_key_pressed<F: Fn(u32, u32, u32) -> bool + 'static>(
        &self,
        callback: F,
    ) -> SignalHandlerId {
        connect_signal_key_pressed(self.obj.as_ptr(), callback)
    }

    pub fn connect_key_released<F: Fn(u32, u32, u32) + 'static>(
        &self,
        callback: F,
    ) -> SignalHandlerId {
        connect_signal_key_released(self.obj.as_ptr(), callback)
    }
}

impl Default for EventControllerKey {
    fn default() -> Self {
        Self::new()
    }
}

impl AsController for EventControllerKey {
    fn as_controller_ptr(&self) -> *mut sys::GtkEventController {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct EventControllerFocus {
    obj: ObjectRef,
}

impl EventControllerFocus {
    pub fn new() -> Self {
        let ptr = unsafe { sys::gtk_event_controller_focus_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn connect_enter<F: Fn() + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_void(self.obj.as_ptr(), "enter", false, callback)
    }

    pub fn connect_leave<F: Fn() + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_void(self.obj.as_ptr(), "leave", false, callback)
    }
}

impl Default for EventControllerFocus {
    fn default() -> Self {
        Self::new()
    }
}

impl AsController for EventControllerFocus {
    fn as_controller_ptr(&self) -> *mut sys::GtkEventController {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct GestureClick {
    obj: ObjectRef,
}

impl GestureClick {
    pub fn new() -> Self {
        let ptr = unsafe { sys::gtk_gesture_click_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    /// `button = 0` escucha cualquier botón del ratón (izquierdo, central, derecho).
    pub fn set_button(&self, button: u32) {
        unsafe {
            sys::gtk_gesture_single_set_button(self.obj.as_ptr().cast(), button);
        }
    }

    pub fn current_button(&self) -> u32 {
        unsafe { sys::gtk_gesture_single_get_current_button(self.obj.as_ptr().cast()) }
    }

    pub fn connect_pressed<F: Fn(i32, f64, f64) + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_i32_f64_f64(self.obj.as_ptr(), "pressed", callback)
    }

    pub fn connect_released<F: Fn(i32, f64, f64) + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_i32_f64_f64(self.obj.as_ptr(), "released", callback)
    }
}

impl Default for GestureClick {
    fn default() -> Self {
        Self::new()
    }
}

impl AsController for GestureClick {
    fn as_controller_ptr(&self) -> *mut sys::GtkEventController {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct GestureSwipe {
    obj: ObjectRef,
}

impl GestureSwipe {
    pub fn new() -> Self {
        let ptr = unsafe { sys::gtk_gesture_swipe_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn connect_swipe<F: Fn(f64, f64) + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_f64_f64(self.obj.as_ptr(), "swipe", callback)
    }
}

impl Default for GestureSwipe {
    fn default() -> Self {
        Self::new()
    }
}

impl AsController for GestureSwipe {
    fn as_controller_ptr(&self) -> *mut sys::GtkEventController {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct GestureDrag {
    obj: ObjectRef,
}

impl GestureDrag {
    pub fn new() -> Self {
        let ptr = unsafe { sys::gtk_gesture_drag_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_button(&self, button: u32) {
        unsafe {
            sys::gtk_gesture_single_set_button(self.obj.as_ptr().cast(), button);
        }
    }

    pub fn connect_drag_begin<F: Fn(f64, f64) + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_f64_f64(self.obj.as_ptr(), "drag-begin", callback)
    }

    pub fn connect_drag_update<F: Fn(f64, f64) + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_f64_f64(self.obj.as_ptr(), "drag-update", callback)
    }

    pub fn connect_drag_end<F: Fn(f64, f64) + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_f64_f64(self.obj.as_ptr(), "drag-end", callback)
    }
}

impl Default for GestureDrag {
    fn default() -> Self {
        Self::new()
    }
}

impl AsController for GestureDrag {
    fn as_controller_ptr(&self) -> *mut sys::GtkEventController {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct ToggleButton {
    obj: ObjectRef,
}

impl ToggleButton {
    pub fn new() -> Self {
        let ptr = unsafe { sys::gtk_toggle_button_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_active(&self, active: bool) {
        unsafe {
            sys::gtk_toggle_button_set_active(
                self.obj.as_ptr().cast(),
                if active { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn is_active(&self) -> bool {
        unsafe { sys::gtk_toggle_button_get_active(self.obj.as_ptr().cast()) != 0 }
    }

    pub fn connect_toggled<F: Fn() + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_void(self.obj.as_ptr(), "toggled", false, callback)
    }
}

impl Default for ToggleButton {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for ToggleButton {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct ListBoxRow {
    obj: ObjectRef,
}

impl ListBoxRow {
    pub fn new() -> Self {
        let ptr = unsafe { sys::gtk_list_box_row_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub(crate) unsafe fn from_borrowed(ptr: *mut sys::GtkListBoxRow) -> Option<Self> {
        ObjectRef::from_borrowed_ptr(ptr.cast()).map(|obj| Self { obj })
    }

    pub fn set_child(&self, child: Option<&impl AsWidget>) {
        let ptr = child.map_or(ptr::null_mut(), AsWidget::as_widget_ptr);
        unsafe {
            sys::gtk_list_box_row_set_child(self.obj.as_ptr().cast(), ptr);
        }
    }

    pub fn index(&self) -> i32 {
        unsafe { sys::gtk_list_box_row_get_index(self.obj.as_ptr().cast()) }
    }

    pub fn set_selectable(&self, selectable: bool) {
        unsafe {
            sys::gtk_list_box_row_set_selectable(
                self.obj.as_ptr().cast(),
                if selectable { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn set_activatable(&self, activatable: bool) {
        unsafe {
            sys::gtk_list_box_row_set_activatable(
                self.obj.as_ptr().cast(),
                if activatable { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub(crate) fn as_row_ptr(&self) -> *mut sys::GtkListBoxRow {
        self.obj.as_ptr().cast()
    }
}

impl Default for ListBoxRow {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for ListBoxRow {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct ListBox {
    obj: ObjectRef,
}

impl ListBox {
    pub fn new() -> Self {
        let ptr = unsafe { sys::gtk_list_box_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn append(&self, child: &impl AsWidget) {
        unsafe {
            sys::gtk_list_box_append(self.obj.as_ptr().cast(), child.as_widget_ptr());
        }
    }

    pub fn remove(&self, child: &impl AsWidget) {
        unsafe {
            sys::gtk_list_box_remove(self.obj.as_ptr().cast(), child.as_widget_ptr());
        }
    }

    pub fn remove_all(&self) {
        unsafe {
            sys::gtk_list_box_remove_all(self.obj.as_ptr().cast());
        }
    }

    pub fn select_row(&self, row: Option<&ListBoxRow>) {
        let ptr = row.map_or(ptr::null_mut(), ListBoxRow::as_row_ptr);
        unsafe {
            sys::gtk_list_box_select_row(self.obj.as_ptr().cast(), ptr);
        }
    }

    pub fn selected_row(&self) -> Option<ListBoxRow> {
        unsafe {
            let ptr = sys::gtk_list_box_get_selected_row(self.obj.as_ptr().cast());
            ListBoxRow::from_borrowed(ptr)
        }
    }

    pub fn row_at_index(&self, index: i32) -> Option<ListBoxRow> {
        unsafe {
            let ptr = sys::gtk_list_box_get_row_at_index(self.obj.as_ptr().cast(), index);
            ListBoxRow::from_borrowed(ptr)
        }
    }

    pub fn set_selection_mode(&self, mode: SelectionMode) {
        unsafe {
            sys::gtk_list_box_set_selection_mode(self.obj.as_ptr().cast(), mode as i32);
        }
    }

    pub fn set_activate_on_single_click(&self, single: bool) {
        unsafe {
            sys::gtk_list_box_set_activate_on_single_click(
                self.obj.as_ptr().cast(),
                if single { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn connect_row_activated<F: Fn(ListBoxRow) + 'static>(
        &self,
        callback: F,
    ) -> SignalHandlerId {
        connect_signal_ptr(self.obj.as_ptr(), "row-activated", move |row_ptr| {
            if let Some(row) = unsafe { ListBoxRow::from_borrowed(row_ptr.cast()) } {
                callback(row);
            }
        })
    }

    pub fn connect_row_selected<F: Fn(Option<ListBoxRow>) + 'static>(
        &self,
        callback: F,
    ) -> SignalHandlerId {
        connect_signal_ptr(self.obj.as_ptr(), "row-selected", move |row_ptr| {
            let row = unsafe { ListBoxRow::from_borrowed(row_ptr.cast()) };
            callback(row);
        })
    }
}

impl Default for ListBox {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for ListBox {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct ScrolledWindow {
    obj: ObjectRef,
}

impl ScrolledWindow {
    pub fn new() -> Self {
        let ptr = unsafe { sys::gtk_scrolled_window_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_child(&self, child: Option<&impl AsWidget>) {
        let ptr = child.map_or(ptr::null_mut(), AsWidget::as_widget_ptr);
        unsafe {
            sys::gtk_scrolled_window_set_child(self.obj.as_ptr().cast(), ptr);
        }
    }

    pub fn set_policy(&self, h: PolicyType, v: PolicyType) {
        unsafe {
            sys::gtk_scrolled_window_set_policy(self.obj.as_ptr().cast(), h as i32, v as i32);
        }
    }

    pub fn set_min_content_width(&self, width: i32) {
        unsafe {
            sys::gtk_scrolled_window_set_min_content_width(self.obj.as_ptr().cast(), width);
        }
    }

    pub fn set_min_content_height(&self, height: i32) {
        unsafe {
            sys::gtk_scrolled_window_set_min_content_height(self.obj.as_ptr().cast(), height);
        }
    }

    pub fn set_max_content_height(&self, height: i32) {
        unsafe {
            sys::gtk_scrolled_window_set_max_content_height(self.obj.as_ptr().cast(), height);
        }
    }

    pub fn set_propagate_natural_height(&self, propagate: bool) {
        unsafe {
            sys::gtk_scrolled_window_set_propagate_natural_height(
                self.obj.as_ptr().cast(),
                if propagate { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }
}

impl Default for ScrolledWindow {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for ScrolledWindow {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct PopoverMenu {
    obj: ObjectRef,
}

impl PopoverMenu {
    pub fn from_model(model: Option<&Menu>) -> Self {
        let ptr = model.map_or(ptr::null_mut(), Menu::as_menu_model_ptr);
        let raw = unsafe { sys::gtk_popover_menu_new_from_model(ptr) };
        let obj = unsafe { ObjectRef::from_new_ptr(raw.cast()) };
        Self { obj }
    }

    pub fn set_menu_model(&self, model: Option<&Menu>) {
        let ptr = model.map_or(ptr::null_mut(), Menu::as_menu_model_ptr);
        unsafe {
            sys::gtk_popover_menu_set_menu_model(self.obj.as_ptr().cast(), ptr);
        }
    }

    pub fn set_pointing_to(&self, rect: Option<Rectangle>) {
        unsafe {
            if let Some(r) = rect {
                let raw = r.to_raw();
                sys::gtk_popover_set_pointing_to(self.obj.as_ptr().cast(), &raw);
            } else {
                sys::gtk_popover_set_pointing_to(self.obj.as_ptr().cast(), ptr::null());
            }
        }
    }

    pub fn set_has_arrow(&self, has_arrow: bool) {
        unsafe {
            sys::gtk_popover_set_has_arrow(
                self.obj.as_ptr().cast(),
                if has_arrow { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn set_position(&self, pos: PositionType) {
        unsafe {
            sys::gtk_popover_set_position(self.obj.as_ptr().cast(), pos as i32);
        }
    }

    pub fn popup(&self) {
        unsafe {
            sys::gtk_popover_popup(self.obj.as_ptr().cast());
        }
    }

    pub fn popdown(&self) {
        unsafe {
            sys::gtk_popover_popdown(self.obj.as_ptr().cast());
        }
    }
}

impl AsWidget for PopoverMenu {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct Image {
    obj: ObjectRef,
}

impl Image {
    pub fn new() -> Self {
        let ptr = unsafe { sys::gtk_image_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn from_icon_name(icon_name: &str) -> Self {
        let c_name = to_cstring(icon_name);
        let ptr = unsafe { sys::gtk_image_new_from_icon_name(c_name.as_ptr()) };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_from_icon_name(&self, icon_name: Option<&str>) {
        unsafe {
            if let Some(name) = icon_name {
                let c_name = to_cstring(name);
                sys::gtk_image_set_from_icon_name(self.obj.as_ptr().cast(), c_name.as_ptr());
            } else {
                sys::gtk_image_set_from_icon_name(self.obj.as_ptr().cast(), ptr::null());
            }
        }
    }

    pub fn set_from_gicon(&self, icon: Option<&Icon>) {
        let ptr = icon.map_or(ptr::null_mut(), Icon::as_gicon_ptr);
        unsafe {
            sys::gtk_image_set_from_gicon(self.obj.as_ptr().cast(), ptr);
        }
    }

    pub fn set_from_texture(&self, texture: Option<&Texture>) {
        let ptr = texture.map_or(ptr::null_mut(), Texture::as_paintable_ptr);
        unsafe {
            sys::gtk_image_set_from_paintable(self.obj.as_ptr().cast(), ptr);
        }
    }

    pub fn set_pixel_size(&self, size: i32) {
        unsafe {
            sys::gtk_image_set_pixel_size(self.obj.as_ptr().cast(), size);
        }
    }

    pub fn clear(&self) {
        unsafe {
            sys::gtk_image_clear(self.obj.as_ptr().cast());
        }
    }
}

impl Default for Image {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for Image {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct Spinner {
    obj: ObjectRef,
}

impl Spinner {
    pub fn new() -> Self {
        let ptr = unsafe { sys::gtk_spinner_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_spinning(&self, spinning: bool) {
        unsafe {
            sys::gtk_spinner_set_spinning(
                self.obj.as_ptr().cast(),
                if spinning { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }
}

impl Default for Spinner {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for Spinner {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct Separator {
    obj: ObjectRef,
}

impl Separator {
    pub fn new(orientation: Orientation) -> Self {
        let ptr = unsafe { sys::gtk_separator_new(orientation as i32) };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }
}

impl AsWidget for Separator {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct StringList {
    obj: ObjectRef,
}

impl StringList {
    pub fn new(items: &[&str]) -> Self {
        let c_items: Vec<CString> = items.iter().map(|s| to_cstring(s)).collect();
        let mut ptrs: Vec<*const c_char> = c_items.iter().map(|s| s.as_ptr()).collect();
        ptrs.push(ptr::null());
        let ptr = unsafe { sys::gtk_string_list_new(ptrs.as_ptr()) };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn as_list_model_ptr(&self) -> *mut sys::GListModel {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct DragSource {
    obj: ObjectRef,
}

impl DragSource {
    pub fn new() -> Self {
        let ptr = unsafe { sys::gtk_drag_source_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_actions(&self, actions: DragAction) {
        unsafe {
            sys::gtk_drag_source_set_actions(self.obj.as_ptr().cast(), actions.bits());
        }
    }

    pub fn set_content(&self, provider: Option<&ContentProvider>) {
        let ptr = provider.map_or(ptr::null_mut(), ContentProvider::as_ptr);
        unsafe {
            sys::gtk_drag_source_set_content(self.obj.as_ptr().cast(), ptr);
        }
    }

    pub fn connect_prepare<F: Fn(f64, f64) -> Option<ContentProvider> + 'static>(
        &self,
        callback: F,
    ) -> SignalHandlerId {
        connect_signal_drag_prepare(self.obj.as_ptr(), move |x, y| callback(x, y).map(|p| p.obj))
    }
}

impl Default for DragSource {
    fn default() -> Self {
        Self::new()
    }
}

impl AsController for DragSource {
    fn as_controller_ptr(&self) -> *mut sys::GtkEventController {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct DropTarget {
    obj: ObjectRef,
}

impl DropTarget {
    pub fn for_string(actions: DragAction) -> Self {
        let ptr = unsafe { sys::gtk_drop_target_new(sys::G_TYPE_STRING, actions.bits()) };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn connect_drop_string<F: Fn(String, f64, f64) -> bool + 'static>(
        &self,
        callback: F,
    ) -> SignalHandlerId {
        connect_signal_drop_string(self.obj.as_ptr(), callback)
    }
}

impl AsController for DropTarget {
    fn as_controller_ptr(&self) -> *mut sys::GtkEventController {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct Popover {
    obj: ObjectRef,
}

impl Popover {
    pub fn new() -> Self {
        let ptr = unsafe { sys::gtk_popover_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_child(&self, child: Option<&impl AsWidget>) {
        let ptr = child.map_or(ptr::null_mut(), AsWidget::as_widget_ptr);
        unsafe {
            sys::gtk_popover_set_child(self.obj.as_ptr().cast(), ptr);
        }
    }

    pub fn set_autohide(&self, autohide: bool) {
        unsafe {
            sys::gtk_popover_set_autohide(
                self.obj.as_ptr().cast(),
                if autohide { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn set_has_arrow(&self, has_arrow: bool) {
        unsafe {
            sys::gtk_popover_set_has_arrow(
                self.obj.as_ptr().cast(),
                if has_arrow { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn set_position(&self, pos: PositionType) {
        unsafe {
            sys::gtk_popover_set_position(self.obj.as_ptr().cast(), pos as i32);
        }
    }

    pub fn set_pointing_to(&self, rect: Option<Rectangle>) {
        let raw = rect.map(Rectangle::to_raw);
        let ptr = raw.as_ref().map_or(ptr::null(), |r| r as *const _);
        unsafe {
            sys::gtk_popover_set_pointing_to(self.obj.as_ptr().cast(), ptr);
        }
    }

    pub fn set_offset(&self, x_offset: i32, y_offset: i32) {
        unsafe {
            sys::gtk_popover_set_offset(self.obj.as_ptr().cast(), x_offset, y_offset);
        }
    }

    pub fn popup(&self) {
        unsafe {
            sys::gtk_popover_popup(self.obj.as_ptr().cast());
        }
    }

    pub fn popdown(&self) {
        unsafe {
            sys::gtk_popover_popdown(self.obj.as_ptr().cast());
        }
    }

    pub fn connect_closed<F: Fn() + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_void(self.obj.as_ptr(), "closed", false, callback)
    }
}

impl Default for Popover {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for Popover {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct MenuButton {
    obj: ObjectRef,
}

impl MenuButton {
    pub fn new() -> Self {
        let ptr = unsafe { sys::gtk_menu_button_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_icon_name(&self, icon_name: &str) {
        let c = to_cstring(icon_name);
        unsafe {
            sys::gtk_menu_button_set_icon_name(self.obj.as_ptr().cast(), c.as_ptr());
        }
    }

    pub fn set_label(&self, label: &str) {
        let c = to_cstring(label);
        unsafe {
            sys::gtk_menu_button_set_label(self.obj.as_ptr().cast(), c.as_ptr());
        }
    }

    pub fn set_menu_model(&self, model: Option<&Menu>) {
        let ptr = model.map_or(ptr::null_mut(), Menu::as_menu_model_ptr);
        unsafe {
            sys::gtk_menu_button_set_menu_model(self.obj.as_ptr().cast(), ptr);
        }
    }

    pub fn set_popover(&self, popover: Option<&Popover>) {
        let ptr = popover.map_or(ptr::null_mut(), AsWidget::as_widget_ptr);
        unsafe {
            sys::gtk_menu_button_set_popover(self.obj.as_ptr().cast(), ptr);
        }
    }

    pub fn set_child(&self, child: Option<&impl AsWidget>) {
        let ptr = child.map_or(ptr::null_mut(), AsWidget::as_widget_ptr);
        unsafe {
            sys::gtk_menu_button_set_child(self.obj.as_ptr().cast(), ptr);
        }
    }
}

impl Default for MenuButton {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for MenuButton {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct DropDown {
    obj: ObjectRef,
}

impl DropDown {
    pub fn from_strings(items: &[&str]) -> Self {
        let c_items: Vec<CString> = items.iter().map(|s| to_cstring(s)).collect();
        let mut ptrs: Vec<*const c_char> = c_items.iter().map(|s| s.as_ptr()).collect();
        ptrs.push(ptr::null());
        let ptr = unsafe { sys::gtk_drop_down_new_from_strings(ptrs.as_ptr()) };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn selected(&self) -> u32 {
        unsafe { sys::gtk_drop_down_get_selected(self.obj.as_ptr().cast()) }
    }

    pub fn set_selected(&self, position: u32) {
        unsafe {
            sys::gtk_drop_down_set_selected(self.obj.as_ptr().cast(), position);
        }
    }

    pub fn connect_selected_notify<F: Fn() + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_notify(self.obj.as_ptr(), "selected", callback)
    }
}

impl AsWidget for DropDown {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct FileDialog {
    obj: ObjectRef,
}

impl FileDialog {
    pub fn new() -> Self {
        let ptr = unsafe { sys::gtk_file_dialog_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_title(&self, title: &str) {
        let c = to_cstring(title);
        unsafe {
            sys::gtk_file_dialog_set_title(self.obj.as_ptr().cast(), c.as_ptr());
        }
    }

    pub fn set_initial_name(&self, name: &str) {
        let c = to_cstring(name);
        unsafe {
            sys::gtk_file_dialog_set_initial_name(self.obj.as_ptr().cast(), c.as_ptr());
        }
    }

    pub fn set_initial_folder(&self, path: &Path) {
        let c = to_cstring(&path.to_string_lossy());
        unsafe {
            let gfile = sys::g_file_new_for_path(c.as_ptr());
            if !gfile.is_null() {
                sys::gtk_file_dialog_set_initial_folder(self.obj.as_ptr().cast(), gfile);
                sys::g_object_unref(gfile.cast());
            }
        }
    }

    pub fn open<F: FnOnce(Option<PathBuf>) + 'static>(
        &self,
        parent: Option<&crate::adw::ApplicationWindow>,
        callback: F,
    ) {
        unsafe extern "C" fn on_ready<F: FnOnce(Option<PathBuf>) + 'static>(
            source_object: *mut sys::GObject,
            res: *mut libc::c_void,
            user_data: sys::gpointer,
        ) {
            let cb = Box::from_raw(user_data.cast::<F>());
            let mut err: *mut sys::GError = ptr::null_mut();
            let gfile = sys::gtk_file_dialog_open_finish(source_object.cast(), res, &mut err);
            if !err.is_null() {
                sys::g_error_free(err);
            }
            cb(gfile_to_pathbuf_and_unref(gfile));
        }

        let parent_ptr = parent.map_or(ptr::null_mut(), |w| w.as_window_ptr());
        let boxed = Box::new(callback);
        unsafe {
            sys::gtk_file_dialog_open(
                self.obj.as_ptr().cast(),
                parent_ptr,
                ptr::null_mut(),
                Some(on_ready::<F>),
                Box::into_raw(boxed).cast(),
            );
        }
    }

    pub fn save<F: FnOnce(Option<PathBuf>) + 'static>(
        &self,
        parent: Option<&crate::adw::ApplicationWindow>,
        callback: F,
    ) {
        unsafe extern "C" fn on_ready<F: FnOnce(Option<PathBuf>) + 'static>(
            source_object: *mut sys::GObject,
            res: *mut libc::c_void,
            user_data: sys::gpointer,
        ) {
            let cb = Box::from_raw(user_data.cast::<F>());
            let mut err: *mut sys::GError = ptr::null_mut();
            let gfile = sys::gtk_file_dialog_save_finish(source_object.cast(), res, &mut err);
            if !err.is_null() {
                sys::g_error_free(err);
            }
            cb(gfile_to_pathbuf_and_unref(gfile));
        }

        let parent_ptr = parent.map_or(ptr::null_mut(), |w| w.as_window_ptr());
        let boxed = Box::new(callback);
        unsafe {
            sys::gtk_file_dialog_save(
                self.obj.as_ptr().cast(),
                parent_ptr,
                ptr::null_mut(),
                Some(on_ready::<F>),
                Box::into_raw(boxed).cast(),
            );
        }
    }

    pub fn select_folder<F: FnOnce(Option<PathBuf>) + 'static>(
        &self,
        parent: Option<&crate::adw::ApplicationWindow>,
        callback: F,
    ) {
        unsafe extern "C" fn on_ready<F: FnOnce(Option<PathBuf>) + 'static>(
            source_object: *mut sys::GObject,
            res: *mut libc::c_void,
            user_data: sys::gpointer,
        ) {
            let cb = Box::from_raw(user_data.cast::<F>());
            let mut err: *mut sys::GError = ptr::null_mut();
            let gfile =
                sys::gtk_file_dialog_select_folder_finish(source_object.cast(), res, &mut err);
            if !err.is_null() {
                sys::g_error_free(err);
            }
            cb(gfile_to_pathbuf_and_unref(gfile));
        }

        let parent_ptr = parent.map_or(ptr::null_mut(), |w| w.as_window_ptr());
        let boxed = Box::new(callback);
        unsafe {
            sys::gtk_file_dialog_select_folder(
                self.obj.as_ptr().cast(),
                parent_ptr,
                ptr::null_mut(),
                Some(on_ready::<F>),
                Box::into_raw(boxed).cast(),
            );
        }
    }
}

impl Default for FileDialog {
    fn default() -> Self {
        Self::new()
    }
}

unsafe fn gfile_to_pathbuf_and_unref(gfile: *mut sys::GFile) -> Option<PathBuf> {
    if gfile.is_null() {
        return None;
    }
    let path_ptr = sys::g_file_get_path(gfile);
    let out = if path_ptr.is_null() {
        None
    } else {
        let s = CStr::from_ptr(path_ptr).to_string_lossy().into_owned();
        sys::g_free(path_ptr.cast());
        Some(PathBuf::from(s))
    };
    sys::g_object_unref(gfile.cast());
    out
}

/// Abre archivos o carpetas contenedoras mediante `GtkFileLauncher` (compatible con portales XDG).
pub struct FileLauncher;

impl FileLauncher {
    pub fn launch_file(parent: Option<&crate::adw::ApplicationWindow>, path: &Path) {
        unsafe extern "C" fn on_done(
            source_object: *mut sys::GObject,
            res: *mut libc::c_void,
            _user_data: sys::gpointer,
        ) {
            let mut err: *mut sys::GError = ptr::null_mut();
            let _ = sys::gtk_file_launcher_launch_finish(source_object.cast(), res, &mut err);
            if !err.is_null() {
                sys::g_error_free(err);
            }
            sys::g_object_unref(source_object.cast());
        }

        let c_path = to_cstring(&path.to_string_lossy());
        let parent_ptr = parent.map_or(ptr::null_mut(), |w| w.as_window_ptr());
        unsafe {
            let gfile = sys::g_file_new_for_path(c_path.as_ptr());
            if gfile.is_null() {
                return;
            }
            let launcher = sys::gtk_file_launcher_new(gfile);
            sys::g_object_unref(gfile.cast());
            if launcher.is_null() {
                return;
            }
            sys::gtk_file_launcher_launch(
                launcher,
                parent_ptr,
                ptr::null_mut(),
                Some(on_done),
                ptr::null_mut(),
            );
        }
    }

    pub fn open_containing_folder(parent: Option<&crate::adw::ApplicationWindow>, path: &Path) {
        unsafe extern "C" fn on_done(
            source_object: *mut sys::GObject,
            res: *mut libc::c_void,
            _user_data: sys::gpointer,
        ) {
            let mut err: *mut sys::GError = ptr::null_mut();
            let _ = sys::gtk_file_launcher_open_containing_folder_finish(
                source_object.cast(),
                res,
                &mut err,
            );
            if !err.is_null() {
                sys::g_error_free(err);
            }
            sys::g_object_unref(source_object.cast());
        }

        let c_path = to_cstring(&path.to_string_lossy());
        let parent_ptr = parent.map_or(ptr::null_mut(), |w| w.as_window_ptr());
        unsafe {
            let gfile = sys::g_file_new_for_path(c_path.as_ptr());
            if gfile.is_null() {
                return;
            }
            let launcher = sys::gtk_file_launcher_new(gfile);
            sys::g_object_unref(gfile.cast());
            if launcher.is_null() {
                return;
            }
            sys::gtk_file_launcher_open_containing_folder(
                launcher,
                parent_ptr,
                ptr::null_mut(),
                Some(on_done),
                ptr::null_mut(),
            );
        }
    }
}

// --- Fase 5 (Vault): entrada de contraseña, etiquetas seleccionables y foco al pulsar. ---

/// `gtk_widget_set_focus_on_click`: evita que un clic robe el foco (p. ej. filas del popover de
/// cuentas que no deben quitar el foco a la página web).
pub fn set_focus_on_click(widget: &impl AsWidget, focus_on_click: bool) {
    unsafe {
        sys::gtk_widget_set_focus_on_click(
            widget.as_widget_ptr(),
            if focus_on_click {
                sys::GTRUE
            } else {
                sys::GFALSE
            },
        );
    }
}

impl Label {
    pub fn set_selectable(&self, selectable: bool) {
        unsafe {
            sys::gtk_label_set_selectable(
                self.obj.as_ptr().cast(),
                if selectable { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }
}

/// `GtkPasswordEntry`: entrada enmascarada (con icono opcional para mirar).
#[derive(Clone)]
pub struct PasswordEntry {
    obj: ObjectRef,
}

impl PasswordEntry {
    pub fn new() -> Self {
        let ptr = unsafe { sys::gtk_password_entry_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_show_peek_icon(&self, show: bool) {
        unsafe {
            sys::gtk_password_entry_set_show_peek_icon(
                self.obj.as_ptr().cast(),
                if show { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }
}

impl Default for PasswordEntry {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for PasswordEntry {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

impl AsEditable for PasswordEntry {}

#[derive(Clone)]
pub struct Paned {
    obj: ObjectRef,
}

impl Paned {
    pub fn new(orientation: Orientation) -> Self {
        let ptr = unsafe { sys::gtk_paned_new(orientation as i32) };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_start_child(&self, child: Option<&impl AsWidget>) {
        unsafe {
            let ptr = child.map_or(ptr::null_mut(), AsWidget::as_widget_ptr);
            sys::gtk_paned_set_start_child(self.obj.as_ptr().cast(), ptr);
        }
    }

    pub fn set_end_child(&self, child: Option<&impl AsWidget>) {
        unsafe {
            let ptr = child.map_or(ptr::null_mut(), AsWidget::as_widget_ptr);
            sys::gtk_paned_set_end_child(self.obj.as_ptr().cast(), ptr);
        }
    }

    pub fn set_resize_start_child(&self, resize: bool) {
        unsafe {
            sys::gtk_paned_set_resize_start_child(
                self.obj.as_ptr().cast(),
                if resize { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn set_resize_end_child(&self, resize: bool) {
        unsafe {
            sys::gtk_paned_set_resize_end_child(
                self.obj.as_ptr().cast(),
                if resize { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn set_shrink_start_child(&self, shrink: bool) {
        unsafe {
            sys::gtk_paned_set_shrink_start_child(
                self.obj.as_ptr().cast(),
                if shrink { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn set_shrink_end_child(&self, shrink: bool) {
        unsafe {
            sys::gtk_paned_set_shrink_end_child(
                self.obj.as_ptr().cast(),
                if shrink { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn set_position(&self, position: i32) {
        unsafe {
            sys::gtk_paned_set_position(self.obj.as_ptr().cast(), position);
        }
    }

    pub fn position(&self) -> i32 {
        unsafe { sys::gtk_paned_get_position(self.obj.as_ptr().cast()) }
    }

    pub fn set_wide_handle(&self, wide: bool) {
        unsafe {
            sys::gtk_paned_set_wide_handle(
                self.obj.as_ptr().cast(),
                if wide { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn connect_position_notify<F: Fn() + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_notify(self.obj.as_ptr(), "position", callback)
    }
}

impl AsWidget for Paned {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct Window {
    obj: ObjectRef,
}

impl Window {
    pub fn new() -> Self {
        let ptr = unsafe { sys::gtk_window_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_title(&self, title: &str) {
        let c_title = to_cstring(title);
        unsafe {
            sys::gtk_window_set_title(self.obj.as_ptr().cast(), c_title.as_ptr());
        }
    }

    pub fn set_decorated(&self, decorated: bool) {
        unsafe {
            sys::gtk_window_set_decorated(
                self.obj.as_ptr().cast(),
                if decorated { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn set_resizable(&self, resizable: bool) {
        unsafe {
            sys::gtk_window_set_resizable(
                self.obj.as_ptr().cast(),
                if resizable { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn set_default_size(&self, width: i32, height: i32) {
        unsafe {
            sys::gtk_window_set_default_size(self.obj.as_ptr().cast(), width, height);
        }
    }

    pub fn default_size(&self) -> (i32, i32) {
        let mut w = 0;
        let mut h = 0;
        unsafe {
            sys::gtk_window_get_default_size(self.obj.as_ptr().cast(), &mut w, &mut h);
        }
        (w, h)
    }

    pub fn set_child(&self, child: Option<&impl AsWidget>) {
        unsafe {
            let ptr = child.map_or(ptr::null_mut(), AsWidget::as_widget_ptr);
            sys::gtk_window_set_child(self.obj.as_ptr().cast(), ptr);
        }
    }

    pub fn set_transient_for(&self, parent: Option<&crate::adw::ApplicationWindow>) {
        unsafe {
            let ptr = parent.map_or(ptr::null_mut(), |w| w.as_window_ptr());
            sys::gtk_window_set_transient_for(self.obj.as_ptr().cast(), ptr);
        }
    }

    pub fn set_hide_on_close(&self, hide: bool) {
        unsafe {
            sys::gtk_window_set_hide_on_close(
                self.obj.as_ptr().cast(),
                if hide { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn present(&self) {
        unsafe {
            sys::gtk_window_present(self.obj.as_ptr().cast());
        }
    }

    pub fn close(&self) {
        unsafe {
            sys::gtk_window_close(self.obj.as_ptr().cast());
        }
    }

    pub fn destroy(&self) {
        unsafe {
            sys::gtk_window_destroy(self.obj.as_ptr().cast());
        }
    }

    pub fn connect_close_request<F: Fn() -> bool + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_bool_ret(self.obj.as_ptr(), "close-request", callback)
    }
}

impl Default for Window {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for Window {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct WindowHandle {
    obj: ObjectRef,
}

impl WindowHandle {
    pub fn new() -> Self {
        let ptr = unsafe { sys::gtk_window_handle_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_child(&self, child: Option<&impl AsWidget>) {
        unsafe {
            let ptr = child.map_or(ptr::null_mut(), AsWidget::as_widget_ptr);
            sys::gtk_window_handle_set_child(self.obj.as_ptr().cast(), ptr);
        }
    }
}

impl Default for WindowHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for WindowHandle {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct AspectFrame {
    obj: ObjectRef,
}

impl AspectFrame {
    pub fn new(xalign: f32, yalign: f32, ratio: f32, obey_child: bool) -> Self {
        let ptr = unsafe {
            sys::gtk_aspect_frame_new(
                xalign,
                yalign,
                ratio,
                if obey_child { sys::GTRUE } else { sys::GFALSE },
            )
        };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_child(&self, child: Option<&impl AsWidget>) {
        unsafe {
            let ptr = child.map_or(ptr::null_mut(), AsWidget::as_widget_ptr);
            sys::gtk_aspect_frame_set_child(self.obj.as_ptr().cast(), ptr);
        }
    }
}

impl AsWidget for AspectFrame {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}
