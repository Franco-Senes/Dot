use crate::gio::{ApplicationFlags, Icon, Menu, SimpleAction};
use crate::gtk::{AsEditable, AsWidget, StringList, Widget};
use crate::object::{
    connect_signal_app_open, connect_signal_bool_ret, connect_signal_notify, connect_signal_ptr,
    connect_signal_ptr_bool, connect_signal_ptr_i32, connect_signal_ret_ptr, connect_signal_void,
    to_cstring, ObjectRef, SignalHandlerId,
};
use crate::sys;
use bitflags::bitflags;
use libc::c_char;
use std::ffi::{CStr, CString};
use std::ptr;

pub fn version() -> (u32, u32, u32) {
    unsafe {
        (
            sys::adw_get_major_version(),
            sys::adw_get_minor_version(),
            sys::adw_get_micro_version(),
        )
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct TabViewShortcuts: u32 {
        const NONE = 0;
        const CONTROL_TAB = 1 << 0;
        const CONTROL_SHIFT_TAB = 1 << 1;
        const CONTROL_PAGE_UP = 1 << 2;
        const CONTROL_PAGE_DOWN = 1 << 3;
        const ALL = 0xFFF;
    }
}

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

    pub fn send_notification(&self, id: Option<&str>, notification: &crate::gio::Notification) {
        let c_id = id.map(to_cstring);
        unsafe {
            sys::g_application_send_notification(
                self.obj.as_ptr().cast(),
                c_id.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
                notification.as_notification_ptr(),
            );
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

    pub fn is_active(&self) -> bool {
        unsafe { sys::gtk_window_is_active(self.obj.as_ptr().cast()) != 0 }
    }

    pub fn as_window_ptr(&self) -> *mut sys::GtkWindow {
        self.obj.as_ptr().cast()
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

#[derive(Clone, PartialEq, Eq)]
pub struct TabPage {
    obj: ObjectRef,
}

impl TabPage {
    pub(crate) unsafe fn from_borrowed(ptr: *mut sys::AdwTabPage) -> Option<Self> {
        ObjectRef::from_borrowed_ptr(ptr.cast()).map(|obj| Self { obj })
    }

    pub fn as_page_ptr(&self) -> *mut sys::AdwTabPage {
        self.obj.as_ptr().cast()
    }

    pub fn child(&self) -> Option<Widget> {
        unsafe {
            let ptr = sys::adw_tab_page_get_child(self.as_page_ptr());
            ObjectRef::from_borrowed_ptr(ptr.cast()).map(|obj| Widget { obj })
        }
    }

    pub fn is_selected(&self) -> bool {
        unsafe { sys::adw_tab_page_get_selected(self.as_page_ptr()) != 0 }
    }

    pub fn is_pinned(&self) -> bool {
        unsafe { sys::adw_tab_page_get_pinned(self.as_page_ptr()) != 0 }
    }

    pub fn title(&self) -> String {
        unsafe {
            let ptr = sys::adw_tab_page_get_title(self.as_page_ptr());
            if ptr.is_null() {
                String::new()
            } else {
                CStr::from_ptr(ptr).to_string_lossy().into_owned()
            }
        }
    }

    pub fn set_title(&self, title: &str) {
        let c = to_cstring(title);
        unsafe {
            sys::adw_tab_page_set_title(self.as_page_ptr(), c.as_ptr());
        }
    }

    pub fn set_tooltip(&self, tooltip: Option<&str>) {
        let c = to_cstring(tooltip.unwrap_or(""));
        unsafe {
            sys::adw_tab_page_set_tooltip(self.as_page_ptr(), c.as_ptr());
        }
    }

    pub fn set_icon(&self, icon: Option<&Icon>) {
        let ptr = icon.map_or(ptr::null_mut(), Icon::as_gicon_ptr);
        unsafe {
            sys::adw_tab_page_set_icon(self.as_page_ptr(), ptr);
        }
    }

    pub fn is_loading(&self) -> bool {
        unsafe { sys::adw_tab_page_get_loading(self.as_page_ptr()) != 0 }
    }

    pub fn set_loading(&self, loading: bool) {
        unsafe {
            sys::adw_tab_page_set_loading(
                self.as_page_ptr(),
                if loading { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn set_indicator_icon(&self, icon: Option<&Icon>) {
        let ptr = icon.map_or(ptr::null_mut(), Icon::as_gicon_ptr);
        unsafe {
            sys::adw_tab_page_set_indicator_icon(self.as_page_ptr(), ptr);
        }
    }

    pub fn set_indicator_tooltip(&self, tooltip: Option<&str>) {
        let c = to_cstring(tooltip.unwrap_or(""));
        unsafe {
            sys::adw_tab_page_set_indicator_tooltip(self.as_page_ptr(), c.as_ptr());
        }
    }

    pub fn set_indicator_activatable(&self, activatable: bool) {
        unsafe {
            sys::adw_tab_page_set_indicator_activatable(
                self.as_page_ptr(),
                if activatable { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }
}

#[derive(Clone)]
pub struct TabView {
    obj: ObjectRef,
}

impl TabView {
    pub fn new() -> Self {
        let ptr = unsafe { sys::adw_tab_view_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn as_tab_view_ptr(&self) -> *mut sys::AdwTabView {
        self.obj.as_ptr().cast()
    }

    pub fn n_pages(&self) -> i32 {
        unsafe { sys::adw_tab_view_get_n_pages(self.as_tab_view_ptr()) }
    }

    pub fn n_pinned_pages(&self) -> i32 {
        unsafe { sys::adw_tab_view_get_n_pinned_pages(self.as_tab_view_ptr()) }
    }

    pub fn selected_page(&self) -> Option<TabPage> {
        unsafe {
            let ptr = sys::adw_tab_view_get_selected_page(self.as_tab_view_ptr());
            TabPage::from_borrowed(ptr)
        }
    }

    pub fn set_selected_page(&self, page: &TabPage) {
        unsafe {
            sys::adw_tab_view_set_selected_page(self.as_tab_view_ptr(), page.as_page_ptr());
        }
    }

    pub fn select_previous_page(&self) -> bool {
        unsafe { sys::adw_tab_view_select_previous_page(self.as_tab_view_ptr()) != 0 }
    }

    pub fn select_next_page(&self) -> bool {
        unsafe { sys::adw_tab_view_select_next_page(self.as_tab_view_ptr()) != 0 }
    }

    pub fn set_default_icon(&self, icon: &Icon) {
        unsafe {
            sys::adw_tab_view_set_default_icon(self.as_tab_view_ptr(), icon.as_gicon_ptr());
        }
    }

    pub fn set_menu_model(&self, menu: Option<&Menu>) {
        let ptr = menu.map_or(ptr::null_mut(), Menu::as_menu_model_ptr);
        unsafe {
            sys::adw_tab_view_set_menu_model(self.as_tab_view_ptr(), ptr);
        }
    }

    pub fn set_shortcuts(&self, shortcuts: TabViewShortcuts) {
        unsafe {
            sys::adw_tab_view_set_shortcuts(self.as_tab_view_ptr(), shortcuts.bits());
        }
    }

    pub fn set_page_pinned(&self, page: &TabPage, pinned: bool) {
        unsafe {
            sys::adw_tab_view_set_page_pinned(
                self.as_tab_view_ptr(),
                page.as_page_ptr(),
                if pinned { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn nth_page(&self, position: i32) -> Option<TabPage> {
        if position < 0 || position >= self.n_pages() {
            return None;
        }
        unsafe {
            let ptr = sys::adw_tab_view_get_nth_page(self.as_tab_view_ptr(), position);
            TabPage::from_borrowed(ptr)
        }
    }

    pub fn page_position(&self, page: &TabPage) -> i32 {
        unsafe { sys::adw_tab_view_get_page_position(self.as_tab_view_ptr(), page.as_page_ptr()) }
    }

    pub fn append(&self, child: &impl AsWidget) -> TabPage {
        unsafe {
            let ptr = sys::adw_tab_view_append(self.as_tab_view_ptr(), child.as_widget_ptr());
            TabPage::from_borrowed(ptr).expect("adw_tab_view_append returned null")
        }
    }

    pub fn insert(&self, child: &impl AsWidget, position: i32) -> TabPage {
        unsafe {
            let ptr =
                sys::adw_tab_view_insert(self.as_tab_view_ptr(), child.as_widget_ptr(), position);
            TabPage::from_borrowed(ptr).expect("adw_tab_view_insert returned null")
        }
    }

    pub fn append_pinned(&self, child: &impl AsWidget) -> TabPage {
        unsafe {
            let ptr =
                sys::adw_tab_view_append_pinned(self.as_tab_view_ptr(), child.as_widget_ptr());
            TabPage::from_borrowed(ptr).expect("adw_tab_view_append_pinned returned null")
        }
    }

    pub fn close_page(&self, page: &TabPage) {
        unsafe {
            sys::adw_tab_view_close_page(self.as_tab_view_ptr(), page.as_page_ptr());
        }
    }

    pub fn close_page_finish(&self, page: &TabPage, confirm: bool) {
        unsafe {
            sys::adw_tab_view_close_page_finish(
                self.as_tab_view_ptr(),
                page.as_page_ptr(),
                if confirm { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn close_other_pages(&self, page: &TabPage) {
        unsafe {
            sys::adw_tab_view_close_other_pages(self.as_tab_view_ptr(), page.as_page_ptr());
        }
    }

    pub fn reorder_page(&self, page: &TabPage, position: i32) -> bool {
        unsafe {
            sys::adw_tab_view_reorder_page(self.as_tab_view_ptr(), page.as_page_ptr(), position)
                != 0
        }
    }

    pub fn connect_selected_page_notify<F: Fn() + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_notify(self.obj.as_ptr(), "selected-page", callback)
    }

    pub fn connect_page_attached<F: Fn(TabPage, i32) + 'static>(
        &self,
        callback: F,
    ) -> SignalHandlerId {
        connect_signal_ptr_i32(self.obj.as_ptr(), "page-attached", move |page_ptr, pos| {
            if let Some(page) = unsafe { TabPage::from_borrowed(page_ptr.cast()) } {
                callback(page, pos);
            }
        })
    }

    pub fn connect_page_detached<F: Fn(TabPage, i32) + 'static>(
        &self,
        callback: F,
    ) -> SignalHandlerId {
        connect_signal_ptr_i32(self.obj.as_ptr(), "page-detached", move |page_ptr, pos| {
            if let Some(page) = unsafe { TabPage::from_borrowed(page_ptr.cast()) } {
                callback(page, pos);
            }
        })
    }

    pub fn connect_page_reordered<F: Fn(TabPage, i32) + 'static>(
        &self,
        callback: F,
    ) -> SignalHandlerId {
        connect_signal_ptr_i32(self.obj.as_ptr(), "page-reordered", move |page_ptr, pos| {
            if let Some(page) = unsafe { TabPage::from_borrowed(page_ptr.cast()) } {
                callback(page, pos);
            }
        })
    }

    pub fn connect_close_page<F: Fn(TabPage) -> bool + 'static>(
        &self,
        callback: F,
    ) -> SignalHandlerId {
        connect_signal_ptr_bool(self.obj.as_ptr(), "close-page", move |page_ptr| {
            if let Some(page) = unsafe { TabPage::from_borrowed(page_ptr.cast()) } {
                callback(page)
            } else {
                false
            }
        })
    }

    pub fn connect_setup_menu<F: Fn(Option<TabPage>) + 'static>(
        &self,
        callback: F,
    ) -> SignalHandlerId {
        connect_signal_ptr(self.obj.as_ptr(), "setup-menu", move |page_ptr| {
            let page = unsafe { TabPage::from_borrowed(page_ptr.cast()) };
            callback(page);
        })
    }

    pub fn connect_indicator_activated<F: Fn(TabPage) + 'static>(
        &self,
        callback: F,
    ) -> SignalHandlerId {
        connect_signal_ptr(self.obj.as_ptr(), "indicator-activated", move |page_ptr| {
            if let Some(page) = unsafe { TabPage::from_borrowed(page_ptr.cast()) } {
                callback(page);
            }
        })
    }

    pub fn connect_create_window<F: Fn() -> Option<TabView> + 'static>(
        &self,
        callback: F,
    ) -> SignalHandlerId {
        connect_signal_ret_ptr(self.obj.as_ptr(), "create-window", move || {
            callback()
                .map(|v| v.as_tab_view_ptr().cast())
                .unwrap_or(ptr::null_mut())
        })
    }
}

impl Default for TabView {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for TabView {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct TabBar {
    obj: ObjectRef,
}

impl TabBar {
    pub fn new() -> Self {
        let ptr = unsafe { sys::adw_tab_bar_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_view(&self, view: Option<&TabView>) {
        let ptr = view.map_or(ptr::null_mut(), TabView::as_tab_view_ptr);
        unsafe {
            sys::adw_tab_bar_set_view(self.obj.as_ptr().cast(), ptr);
        }
    }

    pub fn set_autohide(&self, autohide: bool) {
        unsafe {
            sys::adw_tab_bar_set_autohide(
                self.obj.as_ptr().cast(),
                if autohide { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn set_expand_tabs(&self, expand: bool) {
        unsafe {
            sys::adw_tab_bar_set_expand_tabs(
                self.obj.as_ptr().cast(),
                if expand { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }
}

impl Default for TabBar {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for TabBar {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct OverlaySplitView {
    obj: ObjectRef,
}

impl OverlaySplitView {
    pub fn new() -> Self {
        let ptr = unsafe { sys::adw_overlay_split_view_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_sidebar(&self, sidebar: Option<&impl AsWidget>) {
        let ptr = sidebar.map_or(ptr::null_mut(), AsWidget::as_widget_ptr);
        unsafe {
            sys::adw_overlay_split_view_set_sidebar(self.obj.as_ptr().cast(), ptr);
        }
    }

    pub fn set_content(&self, content: Option<&impl AsWidget>) {
        let ptr = content.map_or(ptr::null_mut(), AsWidget::as_widget_ptr);
        unsafe {
            sys::adw_overlay_split_view_set_content(self.obj.as_ptr().cast(), ptr);
        }
    }

    pub fn set_collapsed(&self, collapsed: bool) {
        unsafe {
            sys::adw_overlay_split_view_set_collapsed(
                self.obj.as_ptr().cast(),
                if collapsed { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn shows_sidebar(&self) -> bool {
        unsafe { sys::adw_overlay_split_view_get_show_sidebar(self.obj.as_ptr().cast()) != 0 }
    }

    pub fn set_show_sidebar(&self, show: bool) {
        unsafe {
            sys::adw_overlay_split_view_set_show_sidebar(
                self.obj.as_ptr().cast(),
                if show { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn set_pin_sidebar(&self, pin: bool) {
        unsafe {
            sys::adw_overlay_split_view_set_pin_sidebar(
                self.obj.as_ptr().cast(),
                if pin { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn set_min_sidebar_width(&self, width: f64) {
        unsafe {
            sys::adw_overlay_split_view_set_min_sidebar_width(self.obj.as_ptr().cast(), width);
        }
    }

    pub fn set_max_sidebar_width(&self, width: f64) {
        unsafe {
            sys::adw_overlay_split_view_set_max_sidebar_width(self.obj.as_ptr().cast(), width);
        }
    }

    pub fn set_sidebar_position(&self, position: crate::gtk::PackType) {
        unsafe {
            sys::adw_overlay_split_view_set_sidebar_position(
                self.obj.as_ptr().cast(),
                position as libc::c_int,
            );
        }
    }

    pub fn set_sidebar_width_fraction(&self, fraction: f64) {
        unsafe {
            sys::adw_overlay_split_view_set_sidebar_width_fraction(
                self.obj.as_ptr().cast(),
                fraction,
            );
        }
    }

    pub fn set_enable_show_gesture(&self, enable: bool) {
        unsafe {
            sys::adw_overlay_split_view_set_enable_show_gesture(
                self.obj.as_ptr().cast(),
                if enable { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn set_enable_hide_gesture(&self, enable: bool) {
        unsafe {
            sys::adw_overlay_split_view_set_enable_hide_gesture(
                self.obj.as_ptr().cast(),
                if enable { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn connect_show_sidebar_notify<F: Fn() + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_notify(self.obj.as_ptr(), "show-sidebar", callback)
    }
}

impl Default for OverlaySplitView {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for OverlaySplitView {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct Dialog {
    obj: ObjectRef,
}

impl Dialog {
    pub fn new() -> Self {
        let ptr = unsafe { sys::adw_dialog_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_child(&self, child: Option<&impl AsWidget>) {
        let ptr = child.map_or(ptr::null_mut(), AsWidget::as_widget_ptr);
        unsafe {
            sys::adw_dialog_set_child(self.obj.as_ptr().cast(), ptr);
        }
    }

    pub fn set_title(&self, title: &str) {
        let c = to_cstring(title);
        unsafe {
            sys::adw_dialog_set_title(self.obj.as_ptr().cast(), c.as_ptr());
        }
    }

    pub fn set_content_width(&self, width: i32) {
        unsafe {
            sys::adw_dialog_set_content_width(self.obj.as_ptr().cast(), width);
        }
    }

    pub fn set_content_height(&self, height: i32) {
        unsafe {
            sys::adw_dialog_set_content_height(self.obj.as_ptr().cast(), height);
        }
    }

    pub fn set_follows_content_size(&self, follows: bool) {
        unsafe {
            sys::adw_dialog_set_follows_content_size(
                self.obj.as_ptr().cast(),
                if follows { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn set_focus_widget(&self, widget: Option<&impl AsWidget>) {
        let ptr = widget.map_or(ptr::null_mut(), AsWidget::as_widget_ptr);
        unsafe {
            sys::adw_dialog_set_focus(self.obj.as_ptr().cast(), ptr);
        }
    }

    pub fn close(&self) -> bool {
        unsafe { sys::adw_dialog_close(self.obj.as_ptr().cast()) != 0 }
    }

    pub fn force_close(&self) {
        unsafe {
            sys::adw_dialog_force_close(self.obj.as_ptr().cast());
        }
    }

    pub fn present(&self, parent: Option<&impl AsWidget>) {
        let ptr = parent.map_or(ptr::null_mut(), AsWidget::as_widget_ptr);
        unsafe {
            sys::adw_dialog_present(self.obj.as_ptr().cast(), ptr);
        }
    }

    pub fn connect_closed<F: Fn() + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_void(self.obj.as_ptr(), "closed", false, callback)
    }
}

impl Default for Dialog {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for Dialog {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct PreferencesDialog {
    obj: ObjectRef,
}

impl PreferencesDialog {
    pub fn new() -> Self {
        let ptr = unsafe { sys::adw_preferences_dialog_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_title(&self, title: &str) {
        let c = to_cstring(title);
        unsafe {
            sys::adw_dialog_set_title(self.obj.as_ptr().cast(), c.as_ptr());
        }
    }

    pub fn set_search_enabled(&self, enabled: bool) {
        unsafe {
            sys::adw_preferences_dialog_set_search_enabled(
                self.obj.as_ptr().cast(),
                if enabled { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn add(&self, page: &PreferencesPage) {
        unsafe {
            sys::adw_preferences_dialog_add(self.obj.as_ptr().cast(), page.as_page_ptr());
        }
    }

    pub fn present(&self, parent: Option<&impl AsWidget>) {
        let ptr = parent.map_or(ptr::null_mut(), AsWidget::as_widget_ptr);
        unsafe {
            sys::adw_dialog_present(self.obj.as_ptr().cast(), ptr);
        }
    }
}

impl Default for PreferencesDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for PreferencesDialog {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct PreferencesPage {
    obj: ObjectRef,
}

impl PreferencesPage {
    pub fn new() -> Self {
        let ptr = unsafe { sys::adw_preferences_page_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_title(&self, title: &str) {
        let c = to_cstring(title);
        unsafe {
            sys::adw_preferences_page_set_title(self.obj.as_ptr().cast(), c.as_ptr());
        }
    }

    pub fn set_icon_name(&self, icon_name: &str) {
        let c = to_cstring(icon_name);
        unsafe {
            sys::adw_preferences_page_set_icon_name(self.obj.as_ptr().cast(), c.as_ptr());
        }
    }

    pub fn add(&self, group: &PreferencesGroup) {
        unsafe {
            sys::adw_preferences_page_add(self.obj.as_ptr().cast(), group.as_group_ptr());
        }
    }

    pub(crate) fn as_page_ptr(&self) -> *mut sys::AdwPreferencesPage {
        self.obj.as_ptr().cast()
    }
}

impl Default for PreferencesPage {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for PreferencesPage {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct PreferencesGroup {
    obj: ObjectRef,
}

impl PreferencesGroup {
    pub fn new() -> Self {
        let ptr = unsafe { sys::adw_preferences_group_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_title(&self, title: &str) {
        let c = to_cstring(title);
        unsafe {
            sys::adw_preferences_group_set_title(self.obj.as_ptr().cast(), c.as_ptr());
        }
    }

    pub fn set_description(&self, description: &str) {
        let c = to_cstring(description);
        unsafe {
            sys::adw_preferences_group_set_description(self.obj.as_ptr().cast(), c.as_ptr());
        }
    }

    pub fn add(&self, child: &impl AsWidget) {
        unsafe {
            sys::adw_preferences_group_add(self.obj.as_ptr().cast(), child.as_widget_ptr());
        }
    }

    pub(crate) fn as_group_ptr(&self) -> *mut sys::AdwPreferencesGroup {
        self.obj.as_ptr().cast()
    }
}

impl Default for PreferencesGroup {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for PreferencesGroup {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct ActionRow {
    obj: ObjectRef,
}

impl ActionRow {
    pub fn new() -> Self {
        let ptr = unsafe { sys::adw_action_row_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_title(&self, title: &str) {
        let c = to_cstring(title);
        unsafe {
            sys::adw_preferences_row_set_title(self.obj.as_ptr().cast(), c.as_ptr());
        }
    }

    pub fn set_subtitle(&self, subtitle: &str) {
        let c = to_cstring(subtitle);
        unsafe {
            sys::adw_action_row_set_subtitle(self.obj.as_ptr().cast(), c.as_ptr());
        }
    }

    pub fn add_prefix(&self, widget: &impl AsWidget) {
        unsafe {
            sys::adw_action_row_add_prefix(self.obj.as_ptr().cast(), widget.as_widget_ptr());
        }
    }

    pub fn add_suffix(&self, widget: &impl AsWidget) {
        unsafe {
            sys::adw_action_row_add_suffix(self.obj.as_ptr().cast(), widget.as_widget_ptr());
        }
    }
}

impl Default for ActionRow {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for ActionRow {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct SwitchRow {
    obj: ObjectRef,
}

impl SwitchRow {
    pub fn new() -> Self {
        let ptr = unsafe { sys::adw_switch_row_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_title(&self, title: &str) {
        let c = to_cstring(title);
        unsafe {
            sys::adw_preferences_row_set_title(self.obj.as_ptr().cast(), c.as_ptr());
        }
    }

    pub fn set_subtitle(&self, subtitle: &str) {
        let c = to_cstring(subtitle);
        unsafe {
            sys::adw_action_row_set_subtitle(self.obj.as_ptr().cast(), c.as_ptr());
        }
    }

    pub fn add_prefix(&self, widget: &impl AsWidget) {
        unsafe {
            sys::adw_action_row_add_prefix(self.obj.as_ptr().cast(), widget.as_widget_ptr());
        }
    }

    pub fn is_active(&self) -> bool {
        unsafe { sys::adw_switch_row_get_active(self.obj.as_ptr().cast()) != 0 }
    }

    pub fn set_active(&self, active: bool) {
        unsafe {
            sys::adw_switch_row_set_active(
                self.obj.as_ptr().cast(),
                if active { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn connect_active_notify<F: Fn() + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_notify(self.obj.as_ptr(), "active", callback)
    }
}

impl Default for SwitchRow {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for SwitchRow {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct ComboRow {
    obj: ObjectRef,
}

impl ComboRow {
    pub fn new() -> Self {
        let ptr = unsafe { sys::adw_combo_row_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_title(&self, title: &str) {
        let c = to_cstring(title);
        unsafe {
            sys::adw_preferences_row_set_title(self.obj.as_ptr().cast(), c.as_ptr());
        }
    }

    pub fn set_subtitle(&self, subtitle: &str) {
        let c = to_cstring(subtitle);
        unsafe {
            sys::adw_action_row_set_subtitle(self.obj.as_ptr().cast(), c.as_ptr());
        }
    }

    pub fn add_prefix(&self, widget: &impl AsWidget) {
        unsafe {
            sys::adw_action_row_add_prefix(self.obj.as_ptr().cast(), widget.as_widget_ptr());
        }
    }

    pub fn set_string_model(&self, model: &StringList) {
        unsafe {
            sys::adw_combo_row_set_model(self.obj.as_ptr().cast(), model.as_list_model_ptr());
        }
    }

    pub fn selected(&self) -> u32 {
        unsafe { sys::adw_combo_row_get_selected(self.obj.as_ptr().cast()) }
    }

    pub fn set_selected(&self, index: u32) {
        unsafe {
            sys::adw_combo_row_set_selected(self.obj.as_ptr().cast(), index);
        }
    }

    pub fn connect_selected_notify<F: Fn() + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_notify(self.obj.as_ptr(), "selected", callback)
    }
}

impl Default for ComboRow {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for ComboRow {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct EntryRow {
    obj: ObjectRef,
}

impl EntryRow {
    pub fn new() -> Self {
        let ptr = unsafe { sys::adw_entry_row_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_title(&self, title: &str) {
        let c = to_cstring(title);
        unsafe {
            sys::adw_preferences_row_set_title(self.obj.as_ptr().cast(), c.as_ptr());
        }
    }

    pub fn set_show_apply_button(&self, show: bool) {
        unsafe {
            sys::adw_entry_row_set_show_apply_button(
                self.obj.as_ptr().cast(),
                if show { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn connect_apply<F: Fn() + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_void(self.obj.as_ptr(), "apply", false, callback)
    }
}

impl Default for EntryRow {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for EntryRow {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

impl AsEditable for EntryRow {}

#[derive(Clone)]
pub struct SpinRow {
    obj: ObjectRef,
}

impl SpinRow {
    pub fn with_range(min: f64, max: f64, step: f64) -> Self {
        let ptr = unsafe { sys::adw_spin_row_new_with_range(min, max, step) };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_title(&self, title: &str) {
        let c = to_cstring(title);
        unsafe {
            sys::adw_preferences_row_set_title(self.obj.as_ptr().cast(), c.as_ptr());
        }
    }

    pub fn set_subtitle(&self, subtitle: &str) {
        let c = to_cstring(subtitle);
        unsafe {
            sys::adw_action_row_set_subtitle(self.obj.as_ptr().cast(), c.as_ptr());
        }
    }

    pub fn add_prefix(&self, widget: &impl AsWidget) {
        unsafe {
            sys::adw_action_row_add_prefix(self.obj.as_ptr().cast(), widget.as_widget_ptr());
        }
    }

    pub fn value(&self) -> f64 {
        unsafe { sys::adw_spin_row_get_value(self.obj.as_ptr().cast()) }
    }

    pub fn set_value(&self, value: f64) {
        unsafe {
            sys::adw_spin_row_set_value(self.obj.as_ptr().cast(), value);
        }
    }

    pub fn set_digits(&self, digits: u32) {
        unsafe {
            sys::adw_spin_row_set_digits(self.obj.as_ptr().cast(), digits);
        }
    }

    pub fn connect_value_notify<F: Fn() + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_notify(self.obj.as_ptr(), "value", callback)
    }
}

impl AsWidget for SpinRow {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct ShortcutsItem {
    obj: ObjectRef,
}

impl ShortcutsItem {
    pub fn new(title: &str, accelerator: &str) -> Self {
        let c_title = to_cstring(title);
        let c_accel = to_cstring(accelerator);
        let ptr = unsafe { sys::adw_shortcuts_item_new(c_title.as_ptr(), c_accel.as_ptr()) };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub(crate) fn as_item_ptr(&self) -> *mut sys::AdwShortcutsItem {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct ShortcutsSection {
    obj: ObjectRef,
}

impl ShortcutsSection {
    pub fn new(title: &str) -> Self {
        let c_title = to_cstring(title);
        let ptr = unsafe { sys::adw_shortcuts_section_new(c_title.as_ptr()) };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn add(&self, item: &ShortcutsItem) {
        unsafe {
            sys::adw_shortcuts_section_add(self.obj.as_ptr().cast(), item.as_item_ptr());
        }
    }

    pub(crate) fn as_section_ptr(&self) -> *mut sys::AdwShortcutsSection {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct ShortcutsDialog {
    obj: ObjectRef,
}

impl ShortcutsDialog {
    pub fn new() -> Self {
        let ptr = unsafe { sys::adw_shortcuts_dialog_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn add(&self, section: &ShortcutsSection) {
        unsafe {
            sys::adw_shortcuts_dialog_add(self.obj.as_ptr().cast(), section.as_section_ptr());
        }
    }

    pub fn present(&self, parent: Option<&impl AsWidget>) {
        let ptr = parent.map_or(ptr::null_mut(), AsWidget::as_widget_ptr);
        unsafe {
            sys::adw_dialog_present(self.obj.as_ptr().cast(), ptr);
        }
    }
}

impl Default for ShortcutsDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl AsWidget for ShortcutsDialog {
    fn as_widget_ptr(&self) -> *mut sys::GtkWidget {
        self.obj.as_ptr().cast()
    }
}
