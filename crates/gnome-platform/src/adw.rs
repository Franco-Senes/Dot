use crate::gio::{ApplicationFlags, SimpleAction};
use crate::gtk::AsWidget;
use crate::object::{
    connect_signal_app_open, connect_signal_bool_ret, connect_signal_notify, connect_signal_void,
    to_cstring, ObjectRef, SignalHandlerId,
};
use crate::sys;
use libc::c_char;
use std::ffi::{CStr, CString};
use std::ptr;

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorScheme {
    Default = 0,
    ForceLight = 1,
    PreferLight = 2,
    PreferDark = 3,
    ForceDark = 4,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarStyle {
    Flat = 0,
    Raised = 1,
    RaisedBorder = 2,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CenteringPolicy {
    Loose = 0,
    Strict = 1,
}

pub struct StyleManager {
    ptr: *mut sys::AdwStyleManager,
}

impl Default for StyleManager {
    fn default() -> Self {
        let ptr = unsafe { sys::adw_style_manager_get_default() };
        Self { ptr }
    }
}

impl StyleManager {
    pub fn set_color_scheme(&self, scheme: ColorScheme) {
        if !self.ptr.is_null() {
            unsafe {
                sys::adw_style_manager_set_color_scheme(self.ptr, scheme as i32);
            }
        }
    }

    pub fn is_dark(&self) -> bool {
        if self.ptr.is_null() {
            false
        } else {
            unsafe { sys::adw_style_manager_get_dark(self.ptr) != 0 }
        }
    }

    pub fn connect_dark_notify<F: Fn() + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_notify(self.ptr.cast(), "dark", callback)
    }
}

#[derive(Clone)]
pub struct Application {
    obj: ObjectRef,
}

impl Application {
    pub fn new(app_id: &str, flags: ApplicationFlags) -> Self {
        let c_id = to_cstring(app_id);
        let ptr = unsafe { sys::adw_application_new(c_id.as_ptr(), flags.bits()) };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn style_manager(&self) -> StyleManager {
        let ptr = unsafe { sys::adw_application_get_style_manager(self.obj.as_ptr().cast()) };
        StyleManager { ptr }
    }

    pub fn add_action(&self, action: &SimpleAction) {
        unsafe {
            sys::g_action_map_add_action(self.obj.as_ptr().cast(), action.as_action_ptr());
        }
    }

    pub fn set_accels_for_action(&self, detailed_action_name: &str, accels: &[&str]) {
        let c_name = to_cstring(detailed_action_name);
        let c_accels: Vec<CString> = accels.iter().map(|s| to_cstring(s)).collect();
        let mut ptrs: Vec<*const c_char> = c_accels.iter().map(|s| s.as_ptr()).collect();
        ptrs.push(ptr::null());
        unsafe {
            sys::gtk_application_set_accels_for_action(
                self.obj.as_ptr().cast(),
                c_name.as_ptr(),
                ptrs.as_ptr(),
            );
        }
    }

    pub fn connect_startup<F: Fn() + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_void(self.obj.as_ptr(), "startup", false, callback)
    }

    pub fn connect_activate<F: Fn() + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_void(self.obj.as_ptr(), "activate", false, callback)
    }

    pub fn connect_open<F: Fn(Vec<String>) + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_app_open(self.obj.as_ptr(), callback)
    }

    pub fn connect_shutdown<F: Fn() + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_void(self.obj.as_ptr(), "shutdown", false, callback)
    }

    pub fn quit(&self) {
        unsafe {
            sys::g_application_quit(self.obj.as_ptr().cast());
        }
    }

    pub fn run_with_args(&self, args: &[String]) -> i32 {
        let c_args: Vec<CString> = args.iter().map(|a| to_cstring(a)).collect();
        let mut argv: Vec<*mut c_char> = c_args.iter().map(|a| a.as_ptr() as *mut c_char).collect();
        unsafe {
            sys::g_application_run(
                self.obj.as_ptr().cast(),
                argv.len() as i32,
                argv.as_mut_ptr(),
            )
        }
    }

    pub(crate) fn as_gtk_app_ptr(&self) -> *mut sys::GtkApplication {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct ApplicationWindow {
    obj: ObjectRef,
}

impl ApplicationWindow {
    pub fn new(app: &Application) -> Self {
        let ptr = unsafe { sys::adw_application_window_new(app.as_gtk_app_ptr()) };
        // En GTK4 la lista global de ventanas toplevel ya posee la referencia inicial;
        // tomamos una referencia adicional prestada para nuestro handle en Rust.
        let obj = unsafe {
            ObjectRef::from_borrowed_ptr(ptr.cast())
                .expect("adw_application_window_new returned null")
        };
        Self { obj }
    }

    pub fn set_content(&self, content: Option<&impl AsWidget>) {
        unsafe {
            let ptr = content.map_or(ptr::null_mut(), AsWidget::as_widget_ptr);
            sys::adw_application_window_set_content(self.obj.as_ptr().cast(), ptr);
        }
    }

    pub fn set_title(&self, title: &str) {
        let c_title = to_cstring(title);
        unsafe {
            sys::gtk_window_set_title(self.obj.as_ptr().cast(), c_title.as_ptr());
        }
    }

    pub fn title(&self) -> String {
        unsafe {
            let ptr = sys::gtk_window_get_title(self.obj.as_ptr().cast());
            if ptr.is_null() {
                String::new()
            } else {
                CStr::from_ptr(ptr).to_string_lossy().into_owned()
            }
        }
    }

    pub fn set_default_size(&self, width: i32, height: i32) {
        unsafe {
            sys::gtk_window_set_default_size(self.obj.as_ptr().cast(), width, height);
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

    pub fn set_fullscreen(&self, fullscreen: bool) {
        unsafe {
            if fullscreen {
                sys::gtk_window_fullscreen(self.obj.as_ptr().cast());
            } else {
                sys::gtk_window_unfullscreen(self.obj.as_ptr().cast());
            }
        }
    }

    pub fn is_fullscreen(&self) -> bool {
        unsafe { sys::gtk_window_is_fullscreen(self.obj.as_ptr().cast()) != 0 }
    }

    pub fn set_focus_widget(&self, widget: Option<&impl AsWidget>) {
        unsafe {
            let ptr = widget.map_or(ptr::null_mut(), AsWidget::as_widget_ptr);
            sys::gtk_window_set_focus(self.obj.as_ptr().cast(), ptr);
        }
    }

    pub fn add_action(&self, action: &SimpleAction) {
        unsafe {
            sys::g_action_map_add_action(self.obj.as_ptr().cast(), action.as_action_ptr());
        }
    }

    pub fn connect_close_request<F: Fn() -> bool + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_bool_ret(self.obj.as_ptr(), "close-request", callback)
    }

    pub fn connect_destroy<F: Fn() + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_void(self.obj.as_ptr(), "destroy", false, callback)
    }
}

impl AsWidget for ApplicationWindow {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct ToolbarView {
    obj: ObjectRef,
}

impl ToolbarView {
    pub fn new() -> Self {
        let ptr = unsafe { sys::adw_toolbar_view_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn add_top_bar(&self, widget: &impl AsWidget) {
        unsafe {
            sys::adw_toolbar_view_add_top_bar(self.obj.as_ptr().cast(), widget.as_widget_ptr());
        }
    }

    pub fn set_content(&self, content: Option<&impl AsWidget>) {
        unsafe {
            let ptr = content.map_or(ptr::null_mut(), AsWidget::as_widget_ptr);
            sys::adw_toolbar_view_set_content(self.obj.as_ptr().cast(), ptr);
        }
    }

    pub fn set_top_bar_style(&self, style: ToolbarStyle) {
        unsafe {
            sys::adw_toolbar_view_set_top_bar_style(self.obj.as_ptr().cast(), style as i32);
        }
    }

    pub fn set_reveal_top_bars(&self, reveal: bool) {
        unsafe {
            sys::adw_toolbar_view_set_reveal_top_bars(
                self.obj.as_ptr().cast(),
                if reveal { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }
}

impl Default for ToolbarView {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for ToolbarView {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct HeaderBar {
    obj: ObjectRef,
}

impl HeaderBar {
    pub fn new() -> Self {
        let ptr = unsafe { sys::adw_header_bar_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_title_widget(&self, widget: Option<&impl AsWidget>) {
        unsafe {
            let ptr = widget.map_or(ptr::null_mut(), AsWidget::as_widget_ptr);
            sys::adw_header_bar_set_title_widget(self.obj.as_ptr().cast(), ptr);
        }
    }

    pub fn pack_start(&self, child: &impl AsWidget) {
        unsafe {
            sys::adw_header_bar_pack_start(self.obj.as_ptr().cast(), child.as_widget_ptr());
        }
    }

    pub fn pack_end(&self, child: &impl AsWidget) {
        unsafe {
            sys::adw_header_bar_pack_end(self.obj.as_ptr().cast(), child.as_widget_ptr());
        }
    }

    pub fn set_centering_policy(&self, policy: CenteringPolicy) {
        unsafe {
            sys::adw_header_bar_set_centering_policy(self.obj.as_ptr().cast(), policy as i32);
        }
    }

    pub fn set_show_back_button(&self, show: bool) {
        unsafe {
            sys::adw_header_bar_set_show_back_button(
                self.obj.as_ptr().cast(),
                if show { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }
}

impl Default for HeaderBar {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for HeaderBar {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct Clamp {
    obj: ObjectRef,
}

impl Clamp {
    pub fn new() -> Self {
        let ptr = unsafe { sys::adw_clamp_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_child(&self, child: Option<&impl AsWidget>) {
        unsafe {
            let ptr = child.map_or(ptr::null_mut(), AsWidget::as_widget_ptr);
            sys::adw_clamp_set_child(self.obj.as_ptr().cast(), ptr);
        }
    }

    pub fn set_maximum_size(&self, max_size: i32) {
        unsafe {
            sys::adw_clamp_set_maximum_size(self.obj.as_ptr().cast(), max_size);
        }
    }

    pub fn set_tightening_threshold(&self, threshold: i32) {
        unsafe {
            sys::adw_clamp_set_tightening_threshold(self.obj.as_ptr().cast(), threshold);
        }
    }
}

impl Default for Clamp {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for Clamp {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}
