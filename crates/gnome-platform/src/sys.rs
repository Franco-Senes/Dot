#![allow(dead_code, non_camel_case_types)]

use libc::{c_char, c_double, c_float, c_int, c_uint, c_ulong, c_void, size_t};

pub type gboolean = c_int;
pub type gint = c_int;
pub type guint = c_uint;
pub type gulong = c_ulong;
pub type gdouble = c_double;
pub type gfloat = c_float;
pub type gsize = size_t;
pub type gpointer = *mut c_void;
pub type GType = gsize;

pub const GFALSE: gboolean = 0;
pub const GTRUE: gboolean = 1;
pub const G_PRIORITY_HIGH_IDLE: gint = 100;
pub const G_PRIORITY_DEFAULT: gint = 0;
pub const G_PRIORITY_DEFAULT_IDLE: gint = 200;

pub type GSourceFunc = Option<unsafe extern "C" fn(user_data: gpointer) -> gboolean>;
pub type GDestroyNotify = Option<unsafe extern "C" fn(data: gpointer)>;
pub type GCallback = Option<unsafe extern "C" fn()>;
pub type GClosureNotify = Option<unsafe extern "C" fn(data: gpointer, closure: *mut c_void)>;
pub type GAsyncReadyCallback = Option<
    unsafe extern "C" fn(source_object: *mut GObject, res: *mut c_void, user_data: gpointer),
>;

#[repr(C)]
pub struct GError {
    pub domain: u32,
    pub code: c_int,
    pub message: *mut c_char,
}

#[repr(C)]
pub struct GBytes {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GVariant {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GVariantType {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GObject {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GApplication {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GFile {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GAction {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GSimpleAction {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GActionMap {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GSettingsSchemaSource {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GSettingsSchema {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GSettings {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GdkDisplay {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GdkClipboard {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GdkPaintable {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GdkTexture {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkWidget {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkWindow {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkApplication {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkBox {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkOverlay {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkPicture {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkDrawingArea {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkEditable {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkEntry {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkSearchEntry {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkLabel {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkButton {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkProgressBar {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkRevealer {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkCssProvider {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkStyleProvider {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkEventController {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkEventControllerMotion {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkEventControllerScroll {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkEventControllerKey {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkEventControllerFocus {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkGesture {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkGestureSingle {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkGestureClick {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkGestureSwipe {
    _private: [u8; 0],
}

#[repr(C)]
pub struct AdwApplication {
    _private: [u8; 0],
}

#[repr(C)]
pub struct AdwStyleManager {
    _private: [u8; 0],
}

#[repr(C)]
pub struct AdwApplicationWindow {
    _private: [u8; 0],
}

#[repr(C)]
pub struct AdwToolbarView {
    _private: [u8; 0],
}

#[repr(C)]
pub struct AdwHeaderBar {
    _private: [u8; 0],
}

#[repr(C)]
pub struct AdwClamp {
    _private: [u8; 0],
}

#[link(name = "glib-2.0")]
extern "C" {
    pub fn g_free(mem: gpointer);
    pub fn g_error_free(error: *mut GError);
    pub fn g_bytes_new(data: *const c_void, size: gsize) -> *mut GBytes;
    pub fn g_bytes_unref(bytes: *mut GBytes);
    pub fn g_idle_add_full(
        priority: gint,
        function: GSourceFunc,
        data: gpointer,
        notify: GDestroyNotify,
    ) -> guint;
    pub fn g_timeout_add_full(
        priority: gint,
        interval: guint,
        function: GSourceFunc,
        data: gpointer,
        notify: GDestroyNotify,
    ) -> guint;
    pub fn g_source_remove(tag: guint) -> gboolean;
}

#[link(name = "gobject-2.0")]
extern "C" {
    pub fn g_object_ref(object: gpointer) -> gpointer;
    pub fn g_object_ref_sink(object: gpointer) -> gpointer;
    pub fn g_object_unref(object: gpointer);
    pub fn g_object_is_floating(object: gpointer) -> gboolean;
    pub fn g_signal_connect_data(
        instance: gpointer,
        detailed_signal: *const c_char,
        c_handler: GCallback,
        data: gpointer,
        destroy_data: GClosureNotify,
        connect_flags: c_uint,
    ) -> gulong;
    pub fn g_signal_handler_disconnect(instance: gpointer, handler_id: gulong);
}

#[link(name = "gio-2.0")]
extern "C" {
    pub fn g_application_run(
        application: *mut GApplication,
        argc: c_int,
        argv: *mut *mut c_char,
    ) -> c_int;
    pub fn g_application_quit(application: *mut GApplication);
    pub fn g_application_hold(application: *mut GApplication);
    pub fn g_application_release(application: *mut GApplication);

    pub fn g_file_get_uri(file: *mut GFile) -> *mut c_char;
    pub fn g_file_get_path(file: *mut GFile) -> *mut c_char;

    pub fn g_simple_action_new(
        name: *const c_char,
        parameter_type: *const GVariantType,
    ) -> *mut GSimpleAction;
    pub fn g_simple_action_set_enabled(simple: *mut GSimpleAction, enabled: gboolean);
    pub fn g_action_map_add_action(action_map: *mut GActionMap, action: *mut GAction);

    pub fn g_settings_schema_source_get_default() -> *mut GSettingsSchemaSource;
    pub fn g_settings_schema_source_new_from_directory(
        directory: *const c_char,
        parent: *mut GSettingsSchemaSource,
        trusted: gboolean,
        error: *mut *mut GError,
    ) -> *mut GSettingsSchemaSource;
    pub fn g_settings_schema_source_lookup(
        source: *mut GSettingsSchemaSource,
        schema_id: *const c_char,
        recursive: gboolean,
    ) -> *mut GSettingsSchema;
    pub fn g_settings_schema_source_unref(source: *mut GSettingsSchemaSource);
    pub fn g_settings_schema_has_key(schema: *mut GSettingsSchema, name: *const c_char)
        -> gboolean;
    pub fn g_settings_schema_unref(schema: *mut GSettingsSchema);

    pub fn g_settings_new_full(
        schema: *mut GSettingsSchema,
        backend: *mut c_void,
        path: *const c_char,
    ) -> *mut GSettings;
    pub fn g_settings_get_string(settings: *mut GSettings, key: *const c_char) -> *mut c_char;
    pub fn g_settings_set_string(
        settings: *mut GSettings,
        key: *const c_char,
        value: *const c_char,
    ) -> gboolean;
    pub fn g_settings_get_boolean(settings: *mut GSettings, key: *const c_char) -> gboolean;
    pub fn g_settings_set_boolean(
        settings: *mut GSettings,
        key: *const c_char,
        value: gboolean,
    ) -> gboolean;
    pub fn g_settings_get_double(settings: *mut GSettings, key: *const c_char) -> gdouble;
    pub fn g_settings_set_double(
        settings: *mut GSettings,
        key: *const c_char,
        value: gdouble,
    ) -> gboolean;
    pub fn g_settings_get_int(settings: *mut GSettings, key: *const c_char) -> gint;
    pub fn g_settings_set_int(
        settings: *mut GSettings,
        key: *const c_char,
        value: gint,
    ) -> gboolean;
    pub fn g_settings_get_uint(settings: *mut GSettings, key: *const c_char) -> guint;
    pub fn g_settings_set_uint(
        settings: *mut GSettings,
        key: *const c_char,
        value: guint,
    ) -> gboolean;
    pub fn g_settings_sync();
}

#[link(name = "gtk-4")]
extern "C" {
    pub fn gtk_init();
    pub fn gtk_init_check() -> gboolean;
    pub fn gtk_is_initialized() -> gboolean;
    pub fn gtk_disable_setlocale();
    pub fn gtk_disable_portals();

    pub fn gdk_display_get_default() -> *mut GdkDisplay;
    pub fn gdk_display_get_clipboard(display: *mut GdkDisplay) -> *mut GdkClipboard;
    pub fn gdk_clipboard_set_text(clipboard: *mut GdkClipboard, text: *const c_char);
    pub fn gdk_clipboard_read_text_async(
        clipboard: *mut GdkClipboard,
        cancellable: *mut c_void,
        callback: GAsyncReadyCallback,
        user_data: gpointer,
    );
    pub fn gdk_clipboard_read_text_finish(
        clipboard: *mut GdkClipboard,
        result: *mut c_void,
        error: *mut *mut GError,
    ) -> *mut c_char;
    pub fn gdk_memory_texture_new(
        width: c_int,
        height: c_int,
        format: c_int,
        bytes: *mut GBytes,
        stride: gsize,
    ) -> *mut GdkTexture;
    pub fn gdk_keyval_name(keyval: guint) -> *const c_char;
    pub fn gdk_keyval_to_lower(keyval: guint) -> guint;
    pub fn gdk_keyval_to_unicode(keyval: guint) -> u32;

    pub fn gtk_style_context_add_provider_for_display(
        display: *mut GdkDisplay,
        provider: *mut GtkStyleProvider,
        priority: guint,
    );
    pub fn gtk_css_provider_new() -> *mut GtkCssProvider;
    pub fn gtk_css_provider_load_from_string(
        css_provider: *mut GtkCssProvider,
        string: *const c_char,
    );

    pub fn gtk_application_add_window(application: *mut GtkApplication, window: *mut GtkWindow);
    pub fn gtk_application_get_active_window(application: *mut GtkApplication) -> *mut GtkWindow;
    pub fn gtk_application_set_accels_for_action(
        application: *mut GtkApplication,
        detailed_action_name: *const c_char,
        accels: *const *const c_char,
    );

    pub fn gtk_widget_set_visible(widget: *mut GtkWidget, visible: gboolean);
    pub fn gtk_widget_get_visible(widget: *mut GtkWidget) -> gboolean;
    pub fn gtk_widget_set_sensitive(widget: *mut GtkWidget, sensitive: gboolean);
    pub fn gtk_widget_set_hexpand(widget: *mut GtkWidget, expand: gboolean);
    pub fn gtk_widget_set_vexpand(widget: *mut GtkWidget, expand: gboolean);
    pub fn gtk_widget_set_halign(widget: *mut GtkWidget, align: c_int);
    pub fn gtk_widget_set_valign(widget: *mut GtkWidget, align: c_int);
    pub fn gtk_widget_set_margin_start(widget: *mut GtkWidget, margin: c_int);
    pub fn gtk_widget_set_margin_end(widget: *mut GtkWidget, margin: c_int);
    pub fn gtk_widget_set_margin_top(widget: *mut GtkWidget, margin: c_int);
    pub fn gtk_widget_set_margin_bottom(widget: *mut GtkWidget, margin: c_int);
    pub fn gtk_widget_set_size_request(widget: *mut GtkWidget, width: c_int, height: c_int);
    pub fn gtk_widget_get_width(widget: *mut GtkWidget) -> c_int;
    pub fn gtk_widget_get_height(widget: *mut GtkWidget) -> c_int;
    pub fn gtk_widget_get_scale_factor(widget: *mut GtkWidget) -> c_int;
    pub fn gtk_widget_set_focusable(widget: *mut GtkWidget, focusable: gboolean);
    pub fn gtk_widget_set_can_focus(widget: *mut GtkWidget, can_focus: gboolean);
    pub fn gtk_widget_grab_focus(widget: *mut GtkWidget) -> gboolean;
    pub fn gtk_widget_has_focus(widget: *mut GtkWidget) -> gboolean;
    pub fn gtk_widget_set_can_target(widget: *mut GtkWidget, can_target: gboolean);
    pub fn gtk_widget_add_css_class(widget: *mut GtkWidget, css_class: *const c_char);
    pub fn gtk_widget_remove_css_class(widget: *mut GtkWidget, css_class: *const c_char);
    pub fn gtk_widget_add_controller(widget: *mut GtkWidget, controller: *mut GtkEventController);
    pub fn gtk_widget_set_cursor_from_name(widget: *mut GtkWidget, name: *const c_char);
    pub fn gtk_widget_set_tooltip_text(widget: *mut GtkWidget, text: *const c_char);
    pub fn gtk_widget_queue_draw(widget: *mut GtkWidget);

    pub fn gtk_window_set_title(window: *mut GtkWindow, title: *const c_char);
    pub fn gtk_window_get_title(window: *mut GtkWindow) -> *const c_char;
    pub fn gtk_window_set_default_size(window: *mut GtkWindow, width: c_int, height: c_int);
    pub fn gtk_window_present(window: *mut GtkWindow);
    pub fn gtk_window_close(window: *mut GtkWindow);
    pub fn gtk_window_destroy(window: *mut GtkWindow);
    pub fn gtk_window_fullscreen(window: *mut GtkWindow);
    pub fn gtk_window_unfullscreen(window: *mut GtkWindow);
    pub fn gtk_window_is_fullscreen(window: *mut GtkWindow) -> gboolean;
    pub fn gtk_window_set_focus(window: *mut GtkWindow, focus: *mut GtkWidget);

    pub fn gtk_box_new(orientation: c_int, spacing: c_int) -> *mut GtkWidget;
    pub fn gtk_box_append(box_: *mut GtkBox, child: *mut GtkWidget);
    pub fn gtk_box_remove(box_: *mut GtkBox, child: *mut GtkWidget);

    pub fn gtk_overlay_new() -> *mut GtkWidget;
    pub fn gtk_overlay_set_child(overlay: *mut GtkOverlay, child: *mut GtkWidget);
    pub fn gtk_overlay_add_overlay(overlay: *mut GtkOverlay, widget: *mut GtkWidget);
    pub fn gtk_overlay_set_measure_overlay(
        overlay: *mut GtkOverlay,
        widget: *mut GtkWidget,
        measure: gboolean,
    );
    pub fn gtk_overlay_set_clip_overlay(
        overlay: *mut GtkOverlay,
        widget: *mut GtkWidget,
        clip_overlay: gboolean,
    );

    pub fn gtk_picture_new() -> *mut GtkWidget;
    pub fn gtk_picture_set_paintable(self_: *mut GtkPicture, paintable: *mut GdkPaintable);
    pub fn gtk_picture_set_can_shrink(self_: *mut GtkPicture, can_shrink: gboolean);
    pub fn gtk_picture_set_content_fit(self_: *mut GtkPicture, content_fit: c_int);

    pub fn gtk_drawing_area_new() -> *mut GtkWidget;
    pub fn gtk_drawing_area_set_content_width(self_: *mut GtkDrawingArea, width: c_int);
    pub fn gtk_drawing_area_set_content_height(self_: *mut GtkDrawingArea, height: c_int);

    pub fn gtk_entry_new() -> *mut GtkWidget;
    pub fn gtk_entry_set_placeholder_text(entry: *mut GtkEntry, text: *const c_char);
    pub fn gtk_entry_set_alignment(entry: *mut GtkEntry, xalign: gfloat);
    pub fn gtk_entry_set_input_purpose(entry: *mut GtkEntry, purpose: c_int);
    pub fn gtk_entry_set_input_hints(entry: *mut GtkEntry, hints: c_uint);
    pub fn gtk_entry_grab_focus_without_selecting(entry: *mut GtkEntry) -> gboolean;

    pub fn gtk_search_entry_new() -> *mut GtkWidget;
    pub fn gtk_search_entry_set_placeholder_text(entry: *mut GtkSearchEntry, text: *const c_char);

    pub fn gtk_editable_get_text(editable: *mut GtkEditable) -> *const c_char;
    pub fn gtk_editable_set_text(editable: *mut GtkEditable, text: *const c_char);
    pub fn gtk_editable_select_region(editable: *mut GtkEditable, start_pos: c_int, end_pos: c_int);
    pub fn gtk_editable_set_position(editable: *mut GtkEditable, position: c_int);

    pub fn gtk_label_new(str_: *const c_char) -> *mut GtkWidget;
    pub fn gtk_label_set_text(self_: *mut GtkLabel, str_: *const c_char);
    pub fn gtk_label_get_text(self_: *mut GtkLabel) -> *const c_char;
    pub fn gtk_label_set_ellipsize(self_: *mut GtkLabel, mode: c_int);
    pub fn gtk_label_set_xalign(self_: *mut GtkLabel, xalign: gfloat);
    pub fn gtk_label_set_max_width_chars(self_: *mut GtkLabel, n_chars: c_int);
    pub fn gtk_label_set_wrap(self_: *mut GtkLabel, wrap: gboolean);

    pub fn gtk_button_new_with_label(label: *const c_char) -> *mut GtkWidget;
    pub fn gtk_button_new_from_icon_name(icon_name: *const c_char) -> *mut GtkWidget;
    pub fn gtk_button_set_label(button: *mut GtkButton, label: *const c_char);

    pub fn gtk_progress_bar_new() -> *mut GtkWidget;
    pub fn gtk_progress_bar_set_fraction(pbar: *mut GtkProgressBar, fraction: gdouble);
    pub fn gtk_progress_bar_pulse(pbar: *mut GtkProgressBar);

    pub fn gtk_revealer_new() -> *mut GtkWidget;
    pub fn gtk_revealer_set_child(revealer: *mut GtkRevealer, child: *mut GtkWidget);
    pub fn gtk_revealer_set_reveal_child(revealer: *mut GtkRevealer, reveal_child: gboolean);
    pub fn gtk_revealer_get_reveal_child(revealer: *mut GtkRevealer) -> gboolean;
    pub fn gtk_revealer_set_transition_type(revealer: *mut GtkRevealer, transition: c_int);
    pub fn gtk_revealer_set_transition_duration(revealer: *mut GtkRevealer, duration: guint);

    pub fn gtk_event_controller_set_propagation_phase(
        controller: *mut GtkEventController,
        phase: c_int,
    );
    pub fn gtk_event_controller_get_current_event_state(
        controller: *mut GtkEventController,
    ) -> c_uint;
    pub fn gtk_event_controller_motion_new() -> *mut GtkEventController;
    pub fn gtk_event_controller_scroll_new(flags: c_int) -> *mut GtkEventController;
    pub fn gtk_event_controller_key_new() -> *mut GtkEventController;
    pub fn gtk_event_controller_focus_new() -> *mut GtkEventController;

    pub fn gtk_gesture_click_new() -> *mut GtkGesture;
    pub fn gtk_gesture_single_set_button(gesture: *mut GtkGestureSingle, button: guint);
    pub fn gtk_gesture_single_get_current_button(gesture: *mut GtkGestureSingle) -> guint;
    pub fn gtk_gesture_swipe_new() -> *mut GtkGesture;
}

#[link(name = "adwaita-1")]
extern "C" {
    pub fn adw_init();
    pub fn adw_is_initialized() -> gboolean;

    pub fn adw_application_new(application_id: *const c_char, flags: c_uint)
        -> *mut AdwApplication;
    pub fn adw_application_get_style_manager(self_: *mut AdwApplication) -> *mut AdwStyleManager;

    pub fn adw_style_manager_get_default() -> *mut AdwStyleManager;
    pub fn adw_style_manager_set_color_scheme(self_: *mut AdwStyleManager, color_scheme: c_int);
    pub fn adw_style_manager_get_dark(self_: *mut AdwStyleManager) -> gboolean;

    pub fn adw_application_window_new(app: *mut GtkApplication) -> *mut GtkWidget;
    pub fn adw_application_window_set_content(
        self_: *mut AdwApplicationWindow,
        content: *mut GtkWidget,
    );

    pub fn adw_toolbar_view_new() -> *mut GtkWidget;
    pub fn adw_toolbar_view_add_top_bar(self_: *mut AdwToolbarView, widget: *mut GtkWidget);
    pub fn adw_toolbar_view_set_content(self_: *mut AdwToolbarView, content: *mut GtkWidget);
    pub fn adw_toolbar_view_set_top_bar_style(self_: *mut AdwToolbarView, style: c_int);
    pub fn adw_toolbar_view_set_reveal_top_bars(self_: *mut AdwToolbarView, reveal: gboolean);

    pub fn adw_header_bar_new() -> *mut GtkWidget;
    pub fn adw_header_bar_set_title_widget(self_: *mut AdwHeaderBar, title_widget: *mut GtkWidget);
    pub fn adw_header_bar_pack_start(self_: *mut AdwHeaderBar, child: *mut GtkWidget);
    pub fn adw_header_bar_pack_end(self_: *mut AdwHeaderBar, child: *mut GtkWidget);
    pub fn adw_header_bar_set_centering_policy(self_: *mut AdwHeaderBar, policy: c_int);
    pub fn adw_header_bar_set_show_start_title_buttons(self_: *mut AdwHeaderBar, setting: gboolean);
    pub fn adw_header_bar_set_show_end_title_buttons(self_: *mut AdwHeaderBar, setting: gboolean);
    pub fn adw_header_bar_set_show_back_button(
        self_: *mut AdwHeaderBar,
        show_back_button: gboolean,
    );

    pub fn adw_clamp_new() -> *mut GtkWidget;
    pub fn adw_clamp_set_child(self_: *mut AdwClamp, child: *mut GtkWidget);
    pub fn adw_clamp_set_maximum_size(self_: *mut AdwClamp, maximum_size: c_int);
    pub fn adw_clamp_set_tightening_threshold(self_: *mut AdwClamp, tightening_threshold: c_int);
}
