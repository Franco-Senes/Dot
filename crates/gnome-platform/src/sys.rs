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
pub const G_TYPE_STRING: GType = 16 << 2;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct GValue {
    pub g_type: GType,
    pub data: [u64; 2],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct GdkRectangle {
    pub x: c_int,
    pub y: c_int,
    pub width: c_int,
    pub height: c_int,
}

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
pub struct GNotification {
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
pub struct GActionGroup {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GSimpleActionGroup {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GIcon {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GMenu {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GMenuModel {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GListModel {
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
pub struct GdkContentProvider {
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
pub struct GtkPaned {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkWindowHandle {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkAspectFrame {
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
pub struct GtkToggleButton {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkListBox {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkListBoxRow {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkScrolledWindow {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkPopover {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkMenuButton {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkDropDown {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkFileDialog {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkFileLauncher {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkPopoverMenu {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkImage {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkSpinner {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkSeparator {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkStringList {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkDragSource {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkDropTarget {
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
pub struct GtkStack {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkRange {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkScale {
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

#[repr(C)]
pub struct AdwTabView {
    _private: [u8; 0],
}

#[repr(C)]
pub struct AdwTabPage {
    _private: [u8; 0],
}

#[repr(C)]
pub struct AdwTabBar {
    _private: [u8; 0],
}

#[repr(C)]
pub struct AdwOverlaySplitView {
    _private: [u8; 0],
}

#[repr(C)]
pub struct AdwDialog {
    _private: [u8; 0],
}

#[repr(C)]
pub struct AdwPreferencesDialog {
    _private: [u8; 0],
}

#[repr(C)]
pub struct AdwPreferencesPage {
    _private: [u8; 0],
}

#[repr(C)]
pub struct AdwPreferencesGroup {
    _private: [u8; 0],
}

#[repr(C)]
pub struct AdwPreferencesRow {
    _private: [u8; 0],
}

#[repr(C)]
pub struct AdwActionRow {
    _private: [u8; 0],
}

#[repr(C)]
pub struct AdwSwitchRow {
    _private: [u8; 0],
}

#[repr(C)]
pub struct AdwComboRow {
    _private: [u8; 0],
}

#[repr(C)]
pub struct AdwEntryRow {
    _private: [u8; 0],
}

#[repr(C)]
pub struct AdwSpinRow {
    _private: [u8; 0],
}

#[repr(C)]
pub struct AdwShortcutsDialog {
    _private: [u8; 0],
}

#[repr(C)]
pub struct AdwShortcutsSection {
    _private: [u8; 0],
}

#[repr(C)]
pub struct AdwShortcutsItem {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GDBusConnection {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GDBusNodeInfo {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GDBusInterfaceInfo {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GDBusMethodInvocation {
    _private: [u8; 0],
}

pub type GDBusInterfaceMethodCallFunc = Option<
    unsafe extern "C" fn(
        connection: *mut GDBusConnection,
        sender: *const c_char,
        object_path: *const c_char,
        interface_name: *const c_char,
        method_name: *const c_char,
        parameters: *mut GVariant,
        invocation: *mut GDBusMethodInvocation,
        user_data: gpointer,
    ),
>;

pub type GDBusInterfaceGetPropertyFunc = Option<
    unsafe extern "C" fn(
        connection: *mut GDBusConnection,
        sender: *const c_char,
        object_path: *const c_char,
        interface_name: *const c_char,
        property_name: *const c_char,
        error: *mut *mut GError,
        user_data: gpointer,
    ) -> *mut GVariant,
>;

pub type GDBusInterfaceSetPropertyFunc = Option<
    unsafe extern "C" fn(
        connection: *mut GDBusConnection,
        sender: *const c_char,
        object_path: *const c_char,
        interface_name: *const c_char,
        property_name: *const c_char,
        value: *mut GVariant,
        error: *mut *mut GError,
        user_data: gpointer,
    ) -> gboolean,
>;

#[repr(C)]
pub struct GDBusInterfaceVTable {
    pub method_call: GDBusInterfaceMethodCallFunc,
    pub get_property: GDBusInterfaceGetPropertyFunc,
    pub set_property: GDBusInterfaceSetPropertyFunc,
    pub padding: [gpointer; 8],
}

#[link(name = "glib-2.0")]
extern "C" {
    pub fn g_free(mem: gpointer);
    pub fn g_error_free(error: *mut GError);
    pub fn g_bytes_new(data: *const c_void, size: gsize) -> *mut GBytes;
    pub fn g_bytes_get_data(bytes: *mut GBytes, size: *mut gsize) -> *const c_void;
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
    pub fn g_get_user_special_dir(directory: c_int) -> *const c_char;
    pub fn g_variant_type_new(type_string: *const c_char) -> *mut GVariantType;
    pub fn g_variant_type_free(type_: *mut GVariantType);
    pub fn g_variant_get_string(value: *mut GVariant, length: *mut gsize) -> *const c_char;
    pub fn g_variant_get_child_value(value: *mut GVariant, index_: gsize) -> *mut GVariant;
    pub fn g_variant_get_uint64(value: *mut GVariant) -> u64;
    pub fn g_variant_get_boolean(value: *mut GVariant) -> gboolean;
    pub fn g_variant_new_string(string: *const c_char) -> *mut GVariant;
    pub fn g_variant_new_uint64(value: u64) -> *mut GVariant;
    pub fn g_variant_new_boolean(value: gboolean) -> *mut GVariant;
    pub fn g_variant_new_tuple(children: *const *mut GVariant, n_children: gsize) -> *mut GVariant;
    pub fn g_variant_new_array(
        child_type: *const GVariantType,
        children: *const *mut GVariant,
        n_children: gsize,
    ) -> *mut GVariant;
    pub fn g_variant_unref(value: *mut GVariant);
}

#[repr(C)]
pub struct GWeakRef {
    priv_ptr: gpointer,
}

#[link(name = "gobject-2.0")]
extern "C" {
    pub fn g_object_ref(object: gpointer) -> gpointer;
    pub fn g_object_ref_sink(object: gpointer) -> gpointer;
    pub fn g_object_unref(object: gpointer);
    pub fn g_object_is_floating(object: gpointer) -> gboolean;
    pub fn g_weak_ref_init(weak_ref: *mut GWeakRef, object: gpointer);
    pub fn g_weak_ref_clear(weak_ref: *mut GWeakRef);
    pub fn g_weak_ref_get(weak_ref: *mut GWeakRef) -> gpointer;
    pub fn g_signal_connect_data(
        instance: gpointer,
        detailed_signal: *const c_char,
        c_handler: GCallback,
        data: gpointer,
        destroy_data: GClosureNotify,
        connect_flags: c_uint,
    ) -> gulong;
    pub fn g_signal_handler_disconnect(instance: gpointer, handler_id: gulong);
    pub fn g_value_init(value: *mut GValue, g_type: GType) -> *mut GValue;
    pub fn g_value_set_string(value: *mut GValue, v_string: *const c_char);
    pub fn g_value_get_string(value: *const GValue) -> *const c_char;
    pub fn g_value_unset(value: *mut GValue);
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
    pub fn g_application_get_dbus_connection(
        application: *mut GApplication,
    ) -> *mut GDBusConnection;
    pub fn g_application_get_dbus_object_path(application: *mut GApplication) -> *const c_char;
    pub fn g_application_send_notification(
        application: *mut GApplication,
        id: *const c_char,
        notification: *mut GNotification,
    );

    pub fn g_dbus_node_info_new_for_xml(
        xml_data: *const c_char,
        error: *mut *mut GError,
    ) -> *mut GDBusNodeInfo;
    pub fn g_dbus_node_info_unref(info: *mut GDBusNodeInfo);
    pub fn g_dbus_node_info_lookup_interface(
        info: *mut GDBusNodeInfo,
        name: *const c_char,
    ) -> *mut GDBusInterfaceInfo;
    pub fn g_dbus_connection_register_object(
        connection: *mut GDBusConnection,
        object_path: *const c_char,
        interface_info: *mut GDBusInterfaceInfo,
        vtable: *const GDBusInterfaceVTable,
        user_data: gpointer,
        user_data_free_func: GDestroyNotify,
        error: *mut *mut GError,
    ) -> guint;
    pub fn g_dbus_connection_unregister_object(
        connection: *mut GDBusConnection,
        registration_id: guint,
    ) -> gboolean;
    pub fn g_dbus_method_invocation_return_value(
        invocation: *mut GDBusMethodInvocation,
        parameters: *mut GVariant,
    );
    pub fn g_dbus_method_invocation_return_dbus_error(
        invocation: *mut GDBusMethodInvocation,
        error_name: *const c_char,
        error_message: *const c_char,
    );

    pub fn g_notification_new(title: *const c_char) -> *mut GNotification;
    pub fn g_notification_set_body(notification: *mut GNotification, body: *const c_char);
    pub fn g_notification_set_icon(notification: *mut GNotification, icon: *mut GIcon);

    pub fn g_file_new_for_path(path: *const c_char) -> *mut GFile;
    pub fn g_file_get_uri(file: *mut GFile) -> *mut c_char;
    pub fn g_file_get_path(file: *mut GFile) -> *mut c_char;

    pub fn g_simple_action_new(
        name: *const c_char,
        parameter_type: *const GVariantType,
    ) -> *mut GSimpleAction;
    pub fn g_simple_action_set_enabled(simple: *mut GSimpleAction, enabled: gboolean);
    pub fn g_action_map_add_action(action_map: *mut GActionMap, action: *mut GAction);
    pub fn g_simple_action_group_new() -> *mut GSimpleActionGroup;

    pub fn g_themed_icon_new(iconname: *const c_char) -> *mut GIcon;
    pub fn g_bytes_icon_new(bytes: *mut GBytes) -> *mut GIcon;

    pub fn g_menu_new() -> *mut GMenu;
    pub fn g_menu_append(menu: *mut GMenu, label: *const c_char, detailed_action: *const c_char);
    pub fn g_menu_append_section(menu: *mut GMenu, label: *const c_char, section: *mut GMenuModel);
    pub fn g_menu_append_submenu(menu: *mut GMenu, label: *const c_char, submenu: *mut GMenuModel);
    pub fn g_menu_remove_all(menu: *mut GMenu);

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
    pub fn gtk_get_major_version() -> guint;
    pub fn gtk_get_minor_version() -> guint;
    pub fn gtk_get_micro_version() -> guint;

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
    pub fn gdk_texture_new_from_bytes(
        bytes: *mut GBytes,
        error: *mut *mut GError,
    ) -> *mut GdkTexture;
    pub fn gdk_texture_get_width(texture: *mut GdkTexture) -> c_int;
    pub fn gdk_texture_get_height(texture: *mut GdkTexture) -> c_int;
    pub fn gdk_texture_save_to_png_bytes(texture: *mut GdkTexture) -> *mut GBytes;
    pub fn gdk_content_provider_new_for_value(value: *const GValue) -> *mut GdkContentProvider;
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
    pub fn gtk_widget_has_css_class(widget: *mut GtkWidget, css_class: *const c_char) -> gboolean;
    pub fn gtk_widget_set_parent(widget: *mut GtkWidget, parent: *mut GtkWidget);
    pub fn gtk_widget_unparent(widget: *mut GtkWidget);
    pub fn gtk_widget_insert_action_group(
        widget: *mut GtkWidget,
        name: *const c_char,
        group: *mut GActionGroup,
    );
    pub fn gtk_widget_add_controller(widget: *mut GtkWidget, controller: *mut GtkEventController);
    pub fn gtk_widget_set_cursor_from_name(widget: *mut GtkWidget, name: *const c_char);
    pub fn gtk_widget_set_tooltip_text(widget: *mut GtkWidget, text: *const c_char);
    pub fn gtk_widget_set_overflow(widget: *mut GtkWidget, overflow: c_int);
    pub fn gtk_widget_queue_draw(widget: *mut GtkWidget);

    pub fn gtk_window_new() -> *mut GtkWidget;
    pub fn gtk_window_set_title(window: *mut GtkWindow, title: *const c_char);
    pub fn gtk_window_get_title(window: *mut GtkWindow) -> *const c_char;
    pub fn gtk_window_set_default_size(window: *mut GtkWindow, width: c_int, height: c_int);
    pub fn gtk_window_get_default_size(
        window: *mut GtkWindow,
        width: *mut c_int,
        height: *mut c_int,
    );
    pub fn gtk_window_set_decorated(window: *mut GtkWindow, setting: gboolean);
    pub fn gtk_window_set_resizable(window: *mut GtkWindow, resizable: gboolean);
    pub fn gtk_window_set_child(window: *mut GtkWindow, child: *mut GtkWidget);
    pub fn gtk_window_set_transient_for(window: *mut GtkWindow, parent: *mut GtkWindow);
    pub fn gtk_window_set_hide_on_close(window: *mut GtkWindow, setting: gboolean);
    pub fn gtk_window_present(window: *mut GtkWindow);
    pub fn gtk_window_close(window: *mut GtkWindow);
    pub fn gtk_window_destroy(window: *mut GtkWindow);
    pub fn gtk_window_fullscreen(window: *mut GtkWindow);
    pub fn gtk_window_unfullscreen(window: *mut GtkWindow);
    pub fn gtk_window_is_fullscreen(window: *mut GtkWindow) -> gboolean;
    pub fn gtk_window_is_active(window: *mut GtkWindow) -> gboolean;
    pub fn gtk_window_set_focus(window: *mut GtkWindow, focus: *mut GtkWidget);

    pub fn gtk_paned_new(orientation: c_int) -> *mut GtkWidget;
    pub fn gtk_paned_set_start_child(paned: *mut GtkPaned, child: *mut GtkWidget);
    pub fn gtk_paned_set_end_child(paned: *mut GtkPaned, child: *mut GtkWidget);
    pub fn gtk_paned_set_resize_start_child(paned: *mut GtkPaned, resize: gboolean);
    pub fn gtk_paned_set_resize_end_child(paned: *mut GtkPaned, resize: gboolean);
    pub fn gtk_paned_set_shrink_start_child(paned: *mut GtkPaned, shrink: gboolean);
    pub fn gtk_paned_set_shrink_end_child(paned: *mut GtkPaned, shrink: gboolean);
    pub fn gtk_paned_set_position(paned: *mut GtkPaned, position: c_int);
    pub fn gtk_paned_get_position(paned: *mut GtkPaned) -> c_int;
    pub fn gtk_paned_set_wide_handle(paned: *mut GtkPaned, wide: gboolean);

    pub fn gtk_window_handle_new() -> *mut GtkWidget;
    pub fn gtk_window_handle_set_child(handle: *mut GtkWindowHandle, child: *mut GtkWidget);

    pub fn gtk_aspect_frame_new(
        xalign: gfloat,
        yalign: gfloat,
        ratio: gfloat,
        obey_child: gboolean,
    ) -> *mut GtkWidget;
    pub fn gtk_aspect_frame_set_child(self_: *mut GtkAspectFrame, child: *mut GtkWidget);

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
    pub fn gtk_editable_get_selection_bounds(
        editable: *mut GtkEditable,
        start_pos: *mut c_int,
        end_pos: *mut c_int,
    ) -> gboolean;
    pub fn gtk_editable_delete_selection(editable: *mut GtkEditable);
    pub fn gtk_editable_set_position(editable: *mut GtkEditable, position: c_int);
    pub fn gtk_editable_get_position(editable: *mut GtkEditable) -> c_int;

    pub fn gtk_label_new(str_: *const c_char) -> *mut GtkWidget;
    pub fn gtk_label_set_text(self_: *mut GtkLabel, str_: *const c_char);
    pub fn gtk_label_get_text(self_: *mut GtkLabel) -> *const c_char;
    pub fn gtk_label_set_ellipsize(self_: *mut GtkLabel, mode: c_int);
    pub fn gtk_label_set_xalign(self_: *mut GtkLabel, xalign: gfloat);
    pub fn gtk_label_set_max_width_chars(self_: *mut GtkLabel, n_chars: c_int);
    pub fn gtk_label_set_wrap(self_: *mut GtkLabel, wrap: gboolean);

    pub fn gtk_button_new() -> *mut GtkWidget;
    pub fn gtk_button_new_with_label(label: *const c_char) -> *mut GtkWidget;
    pub fn gtk_button_new_from_icon_name(icon_name: *const c_char) -> *mut GtkWidget;
    pub fn gtk_button_set_label(button: *mut GtkButton, label: *const c_char);
    pub fn gtk_button_set_icon_name(button: *mut GtkButton, icon_name: *const c_char);
    pub fn gtk_button_set_child(button: *mut GtkButton, child: *mut GtkWidget);

    pub fn gtk_toggle_button_new() -> *mut GtkWidget;
    pub fn gtk_toggle_button_set_active(toggle_button: *mut GtkToggleButton, is_active: gboolean);
    pub fn gtk_toggle_button_get_active(toggle_button: *mut GtkToggleButton) -> gboolean;

    pub fn gtk_list_box_new() -> *mut GtkWidget;
    pub fn gtk_list_box_append(box_: *mut GtkListBox, child: *mut GtkWidget);
    pub fn gtk_list_box_remove(box_: *mut GtkListBox, child: *mut GtkWidget);
    pub fn gtk_list_box_remove_all(box_: *mut GtkListBox);
    pub fn gtk_list_box_select_row(box_: *mut GtkListBox, row: *mut GtkListBoxRow);
    pub fn gtk_list_box_get_selected_row(box_: *mut GtkListBox) -> *mut GtkListBoxRow;
    pub fn gtk_list_box_get_row_at_index(
        box_: *mut GtkListBox,
        index_: c_int,
    ) -> *mut GtkListBoxRow;
    pub fn gtk_list_box_set_selection_mode(box_: *mut GtkListBox, mode: c_int);
    pub fn gtk_list_box_set_activate_on_single_click(box_: *mut GtkListBox, single: gboolean);

    pub fn gtk_list_box_row_new() -> *mut GtkWidget;
    pub fn gtk_list_box_row_set_child(row: *mut GtkListBoxRow, child: *mut GtkWidget);
    pub fn gtk_list_box_row_get_index(row: *mut GtkListBoxRow) -> c_int;
    pub fn gtk_list_box_row_set_selectable(row: *mut GtkListBoxRow, selectable: gboolean);
    pub fn gtk_list_box_row_set_activatable(row: *mut GtkListBoxRow, activatable: gboolean);

    pub fn gtk_scrolled_window_new() -> *mut GtkWidget;
    pub fn gtk_scrolled_window_set_child(
        scrolled_window: *mut GtkScrolledWindow,
        child: *mut GtkWidget,
    );
    pub fn gtk_scrolled_window_set_policy(
        scrolled_window: *mut GtkScrolledWindow,
        hscrollbar_policy: c_int,
        vscrollbar_policy: c_int,
    );
    pub fn gtk_scrolled_window_set_min_content_width(
        scrolled_window: *mut GtkScrolledWindow,
        width: c_int,
    );
    pub fn gtk_scrolled_window_set_min_content_height(
        scrolled_window: *mut GtkScrolledWindow,
        height: c_int,
    );
    pub fn gtk_scrolled_window_set_max_content_height(
        scrolled_window: *mut GtkScrolledWindow,
        height: c_int,
    );
    pub fn gtk_scrolled_window_set_propagate_natural_height(
        scrolled_window: *mut GtkScrolledWindow,
        propagate: gboolean,
    );

    pub fn gtk_popover_new() -> *mut GtkWidget;
    pub fn gtk_popover_set_child(popover: *mut GtkPopover, child: *mut GtkWidget);
    pub fn gtk_popover_set_autohide(popover: *mut GtkPopover, autohide: gboolean);
    pub fn gtk_popover_set_offset(popover: *mut GtkPopover, x_offset: c_int, y_offset: c_int);
    pub fn gtk_popover_menu_new_from_model(model: *mut GMenuModel) -> *mut GtkWidget;
    pub fn gtk_popover_menu_set_menu_model(popover: *mut GtkPopoverMenu, model: *mut GMenuModel);
    pub fn gtk_popover_set_pointing_to(popover: *mut GtkPopover, rect: *const GdkRectangle);
    pub fn gtk_popover_set_has_arrow(popover: *mut GtkPopover, has_arrow: gboolean);
    pub fn gtk_popover_set_position(popover: *mut GtkPopover, position: c_int);
    pub fn gtk_popover_popup(popover: *mut GtkPopover);
    pub fn gtk_popover_popdown(popover: *mut GtkPopover);

    pub fn gtk_menu_button_new() -> *mut GtkWidget;
    pub fn gtk_menu_button_set_icon_name(menu_button: *mut GtkMenuButton, icon_name: *const c_char);
    pub fn gtk_menu_button_set_label(menu_button: *mut GtkMenuButton, label: *const c_char);
    pub fn gtk_menu_button_set_menu_model(
        menu_button: *mut GtkMenuButton,
        menu_model: *mut GMenuModel,
    );
    pub fn gtk_menu_button_set_popover(menu_button: *mut GtkMenuButton, popover: *mut GtkWidget);
    pub fn gtk_menu_button_set_child(menu_button: *mut GtkMenuButton, child: *mut GtkWidget);

    pub fn gtk_drop_down_new_from_strings(strings: *const *const c_char) -> *mut GtkWidget;
    pub fn gtk_drop_down_get_selected(self_: *mut GtkDropDown) -> guint;
    pub fn gtk_drop_down_set_selected(self_: *mut GtkDropDown, position: guint);

    pub fn gtk_file_dialog_new() -> *mut GtkFileDialog;
    pub fn gtk_file_dialog_set_title(self_: *mut GtkFileDialog, title: *const c_char);
    pub fn gtk_file_dialog_set_initial_name(self_: *mut GtkFileDialog, name: *const c_char);
    pub fn gtk_file_dialog_set_initial_folder(self_: *mut GtkFileDialog, folder: *mut GFile);
    pub fn gtk_file_dialog_open(
        self_: *mut GtkFileDialog,
        parent: *mut GtkWindow,
        cancellable: *mut c_void,
        callback: GAsyncReadyCallback,
        user_data: gpointer,
    );
    pub fn gtk_file_dialog_open_finish(
        self_: *mut GtkFileDialog,
        result: *mut c_void,
        error: *mut *mut GError,
    ) -> *mut GFile;
    pub fn gtk_file_dialog_save(
        self_: *mut GtkFileDialog,
        parent: *mut GtkWindow,
        cancellable: *mut c_void,
        callback: GAsyncReadyCallback,
        user_data: gpointer,
    );
    pub fn gtk_file_dialog_save_finish(
        self_: *mut GtkFileDialog,
        result: *mut c_void,
        error: *mut *mut GError,
    ) -> *mut GFile;
    pub fn gtk_file_dialog_select_folder(
        self_: *mut GtkFileDialog,
        parent: *mut GtkWindow,
        cancellable: *mut c_void,
        callback: GAsyncReadyCallback,
        user_data: gpointer,
    );
    pub fn gtk_file_dialog_select_folder_finish(
        self_: *mut GtkFileDialog,
        result: *mut c_void,
        error: *mut *mut GError,
    ) -> *mut GFile;

    pub fn gtk_file_launcher_new(file: *mut GFile) -> *mut GtkFileLauncher;
    pub fn gtk_file_launcher_launch(
        self_: *mut GtkFileLauncher,
        parent: *mut GtkWindow,
        cancellable: *mut c_void,
        callback: GAsyncReadyCallback,
        user_data: gpointer,
    );
    pub fn gtk_file_launcher_launch_finish(
        self_: *mut GtkFileLauncher,
        result: *mut c_void,
        error: *mut *mut GError,
    ) -> gboolean;
    pub fn gtk_file_launcher_open_containing_folder(
        self_: *mut GtkFileLauncher,
        parent: *mut GtkWindow,
        cancellable: *mut c_void,
        callback: GAsyncReadyCallback,
        user_data: gpointer,
    );
    pub fn gtk_file_launcher_open_containing_folder_finish(
        self_: *mut GtkFileLauncher,
        result: *mut c_void,
        error: *mut *mut GError,
    ) -> gboolean;

    pub fn gtk_image_new() -> *mut GtkWidget;
    pub fn gtk_image_new_from_icon_name(icon_name: *const c_char) -> *mut GtkWidget;
    pub fn gtk_image_set_from_icon_name(image: *mut GtkImage, icon_name: *const c_char);
    pub fn gtk_image_set_from_gicon(image: *mut GtkImage, icon: *mut GIcon);
    pub fn gtk_image_set_from_paintable(image: *mut GtkImage, paintable: *mut GdkPaintable);
    pub fn gtk_image_set_pixel_size(image: *mut GtkImage, pixel_size: c_int);
    pub fn gtk_image_clear(image: *mut GtkImage);

    pub fn gtk_spinner_new() -> *mut GtkWidget;
    pub fn gtk_spinner_set_spinning(spinner: *mut GtkSpinner, spinning: gboolean);

    pub fn gtk_separator_new(orientation: c_int) -> *mut GtkWidget;

    pub fn gtk_string_list_new(strings: *const *const c_char) -> *mut GtkStringList;

    pub fn gtk_drag_source_new() -> *mut GtkDragSource;
    pub fn gtk_drag_source_set_actions(source: *mut GtkDragSource, actions: c_uint);
    pub fn gtk_drag_source_set_content(
        source: *mut GtkDragSource,
        content: *mut GdkContentProvider,
    );

    pub fn gtk_drop_target_new(type_: GType, actions: c_uint) -> *mut GtkDropTarget;
    pub fn gtk_drop_target_set_actions(self_: *mut GtkDropTarget, actions: c_uint);

    pub fn gtk_progress_bar_new() -> *mut GtkWidget;
    pub fn gtk_progress_bar_set_fraction(pbar: *mut GtkProgressBar, fraction: gdouble);
    pub fn gtk_progress_bar_pulse(pbar: *mut GtkProgressBar);

    pub fn gtk_revealer_new() -> *mut GtkWidget;
    pub fn gtk_revealer_set_child(revealer: *mut GtkRevealer, child: *mut GtkWidget);
    pub fn gtk_revealer_set_reveal_child(revealer: *mut GtkRevealer, reveal_child: gboolean);
    pub fn gtk_revealer_get_reveal_child(revealer: *mut GtkRevealer) -> gboolean;
    pub fn gtk_revealer_set_transition_type(revealer: *mut GtkRevealer, transition: c_int);
    pub fn gtk_revealer_set_transition_duration(revealer: *mut GtkRevealer, duration: guint);

    pub fn gtk_stack_new() -> *mut GtkWidget;
    pub fn gtk_stack_add_named(
        stack: *mut GtkStack,
        child: *mut GtkWidget,
        name: *const c_char,
    ) -> *mut c_void;
    pub fn gtk_stack_set_visible_child_name(stack: *mut GtkStack, name: *const c_char);
    pub fn gtk_stack_get_visible_child_name(stack: *mut GtkStack) -> *const c_char;
    pub fn gtk_stack_set_transition_type(stack: *mut GtkStack, transition: c_int);
    pub fn gtk_stack_set_transition_duration(stack: *mut GtkStack, duration: guint);

    pub fn gtk_scale_new_with_range(
        orientation: c_int,
        min: gdouble,
        max: gdouble,
        step: gdouble,
    ) -> *mut GtkWidget;
    pub fn gtk_scale_set_digits(scale: *mut GtkScale, digits: c_int);
    pub fn gtk_scale_set_draw_value(scale: *mut GtkScale, draw_value: gboolean);
    pub fn gtk_range_get_value(range: *mut GtkRange) -> gdouble;
    pub fn gtk_range_set_value(range: *mut GtkRange, value: gdouble);

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
    pub fn gtk_gesture_drag_new() -> *mut GtkGesture;
    pub fn gtk_gesture_single_set_button(gesture: *mut GtkGestureSingle, button: guint);
    pub fn gtk_gesture_single_get_current_button(gesture: *mut GtkGestureSingle) -> guint;
    pub fn gtk_gesture_swipe_new() -> *mut GtkGesture;
}

#[link(name = "adwaita-1")]
extern "C" {
    pub fn adw_init();
    pub fn adw_is_initialized() -> gboolean;
    pub fn adw_get_major_version() -> guint;
    pub fn adw_get_minor_version() -> guint;
    pub fn adw_get_micro_version() -> guint;

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

    pub fn adw_tab_view_new() -> *mut AdwTabView;
    pub fn adw_tab_view_get_n_pages(self_: *mut AdwTabView) -> c_int;
    pub fn adw_tab_view_get_n_pinned_pages(self_: *mut AdwTabView) -> c_int;
    pub fn adw_tab_view_get_selected_page(self_: *mut AdwTabView) -> *mut AdwTabPage;
    pub fn adw_tab_view_set_selected_page(self_: *mut AdwTabView, selected_page: *mut AdwTabPage);
    pub fn adw_tab_view_select_previous_page(self_: *mut AdwTabView) -> gboolean;
    pub fn adw_tab_view_select_next_page(self_: *mut AdwTabView) -> gboolean;
    pub fn adw_tab_view_set_default_icon(self_: *mut AdwTabView, default_icon: *mut GIcon);
    pub fn adw_tab_view_set_menu_model(self_: *mut AdwTabView, menu_model: *mut GMenuModel);
    pub fn adw_tab_view_set_shortcuts(self_: *mut AdwTabView, shortcuts: c_uint);
    pub fn adw_tab_view_set_page_pinned(
        self_: *mut AdwTabView,
        page: *mut AdwTabPage,
        pinned: gboolean,
    );
    pub fn adw_tab_view_get_page(self_: *mut AdwTabView, child: *mut GtkWidget) -> *mut AdwTabPage;
    pub fn adw_tab_view_get_nth_page(self_: *mut AdwTabView, position: c_int) -> *mut AdwTabPage;
    pub fn adw_tab_view_get_page_position(self_: *mut AdwTabView, page: *mut AdwTabPage) -> c_int;
    pub fn adw_tab_view_append(self_: *mut AdwTabView, child: *mut GtkWidget) -> *mut AdwTabPage;
    pub fn adw_tab_view_insert(
        self_: *mut AdwTabView,
        child: *mut GtkWidget,
        position: c_int,
    ) -> *mut AdwTabPage;
    pub fn adw_tab_view_append_pinned(
        self_: *mut AdwTabView,
        child: *mut GtkWidget,
    ) -> *mut AdwTabPage;
    pub fn adw_tab_view_insert_pinned(
        self_: *mut AdwTabView,
        child: *mut GtkWidget,
        position: c_int,
    ) -> *mut AdwTabPage;
    pub fn adw_tab_view_close_page(self_: *mut AdwTabView, page: *mut AdwTabPage);
    pub fn adw_tab_view_close_page_finish(
        self_: *mut AdwTabView,
        page: *mut AdwTabPage,
        confirm: gboolean,
    );
    pub fn adw_tab_view_close_other_pages(self_: *mut AdwTabView, page: *mut AdwTabPage);
    pub fn adw_tab_view_reorder_page(
        self_: *mut AdwTabView,
        page: *mut AdwTabPage,
        position: c_int,
    ) -> gboolean;

    pub fn adw_tab_page_get_child(self_: *mut AdwTabPage) -> *mut GtkWidget;
    pub fn adw_tab_page_get_selected(self_: *mut AdwTabPage) -> gboolean;
    pub fn adw_tab_page_get_pinned(self_: *mut AdwTabPage) -> gboolean;
    pub fn adw_tab_page_get_title(self_: *mut AdwTabPage) -> *const c_char;
    pub fn adw_tab_page_set_title(self_: *mut AdwTabPage, title: *const c_char);
    pub fn adw_tab_page_get_tooltip(self_: *mut AdwTabPage) -> *const c_char;
    pub fn adw_tab_page_set_tooltip(self_: *mut AdwTabPage, tooltip: *const c_char);
    pub fn adw_tab_page_set_icon(self_: *mut AdwTabPage, icon: *mut GIcon);
    pub fn adw_tab_page_get_loading(self_: *mut AdwTabPage) -> gboolean;
    pub fn adw_tab_page_set_loading(self_: *mut AdwTabPage, loading: gboolean);
    pub fn adw_tab_page_set_indicator_icon(self_: *mut AdwTabPage, indicator_icon: *mut GIcon);
    pub fn adw_tab_page_set_indicator_tooltip(self_: *mut AdwTabPage, tooltip: *const c_char);
    pub fn adw_tab_page_set_indicator_activatable(self_: *mut AdwTabPage, activatable: gboolean);

    pub fn adw_tab_bar_new() -> *mut AdwTabBar;
    pub fn adw_tab_bar_set_view(self_: *mut AdwTabBar, view: *mut AdwTabView);
    pub fn adw_tab_bar_set_autohide(self_: *mut AdwTabBar, autohide: gboolean);
    pub fn adw_tab_bar_set_expand_tabs(self_: *mut AdwTabBar, expand_tabs: gboolean);

    pub fn adw_overlay_split_view_new() -> *mut GtkWidget;
    pub fn adw_overlay_split_view_set_sidebar(
        self_: *mut AdwOverlaySplitView,
        sidebar: *mut GtkWidget,
    );
    pub fn adw_overlay_split_view_set_content(
        self_: *mut AdwOverlaySplitView,
        content: *mut GtkWidget,
    );
    pub fn adw_overlay_split_view_set_collapsed(
        self_: *mut AdwOverlaySplitView,
        collapsed: gboolean,
    );
    pub fn adw_overlay_split_view_set_sidebar_position(
        self_: *mut AdwOverlaySplitView,
        position: c_int,
    );
    pub fn adw_overlay_split_view_set_enable_show_gesture(
        self_: *mut AdwOverlaySplitView,
        enable_show_gesture: gboolean,
    );
    pub fn adw_overlay_split_view_set_enable_hide_gesture(
        self_: *mut AdwOverlaySplitView,
        enable_hide_gesture: gboolean,
    );
    pub fn adw_overlay_split_view_set_sidebar_width_fraction(
        self_: *mut AdwOverlaySplitView,
        fraction: gdouble,
    );
    pub fn adw_overlay_split_view_get_show_sidebar(self_: *mut AdwOverlaySplitView) -> gboolean;
    pub fn adw_overlay_split_view_set_show_sidebar(
        self_: *mut AdwOverlaySplitView,
        show_sidebar: gboolean,
    );
    pub fn adw_overlay_split_view_set_pin_sidebar(
        self_: *mut AdwOverlaySplitView,
        pin_sidebar: gboolean,
    );
    pub fn adw_overlay_split_view_set_min_sidebar_width(
        self_: *mut AdwOverlaySplitView,
        width: gdouble,
    );
    pub fn adw_overlay_split_view_set_max_sidebar_width(
        self_: *mut AdwOverlaySplitView,
        width: gdouble,
    );

    pub fn adw_dialog_new() -> *mut AdwDialog;
    pub fn adw_dialog_set_child(self_: *mut AdwDialog, child: *mut GtkWidget);
    pub fn adw_dialog_set_title(self_: *mut AdwDialog, title: *const c_char);
    pub fn adw_dialog_set_content_width(self_: *mut AdwDialog, content_width: c_int);
    pub fn adw_dialog_set_content_height(self_: *mut AdwDialog, content_height: c_int);
    pub fn adw_dialog_set_follows_content_size(
        self_: *mut AdwDialog,
        follows_content_size: gboolean,
    );
    pub fn adw_dialog_set_focus(self_: *mut AdwDialog, focus: *mut GtkWidget);
    pub fn adw_dialog_close(self_: *mut AdwDialog) -> gboolean;
    pub fn adw_dialog_force_close(self_: *mut AdwDialog);
    pub fn adw_dialog_present(self_: *mut AdwDialog, parent: *mut GtkWidget);

    pub fn adw_preferences_dialog_new() -> *mut AdwDialog;
    pub fn adw_preferences_dialog_add(
        self_: *mut AdwPreferencesDialog,
        page: *mut AdwPreferencesPage,
    );
    pub fn adw_preferences_dialog_set_search_enabled(
        self_: *mut AdwPreferencesDialog,
        search_enabled: gboolean,
    );

    pub fn adw_preferences_page_new() -> *mut GtkWidget;
    pub fn adw_preferences_page_add(
        self_: *mut AdwPreferencesPage,
        group: *mut AdwPreferencesGroup,
    );
    pub fn adw_preferences_page_set_title(self_: *mut AdwPreferencesPage, title: *const c_char);
    pub fn adw_preferences_page_set_icon_name(
        self_: *mut AdwPreferencesPage,
        icon_name: *const c_char,
    );

    pub fn adw_preferences_group_new() -> *mut GtkWidget;
    pub fn adw_preferences_group_add(self_: *mut AdwPreferencesGroup, child: *mut GtkWidget);
    pub fn adw_preferences_group_set_title(self_: *mut AdwPreferencesGroup, title: *const c_char);
    pub fn adw_preferences_group_set_description(
        self_: *mut AdwPreferencesGroup,
        description: *const c_char,
    );

    pub fn adw_preferences_row_set_title(self_: *mut AdwPreferencesRow, title: *const c_char);

    pub fn adw_action_row_new() -> *mut GtkWidget;
    pub fn adw_action_row_set_subtitle(self_: *mut AdwActionRow, subtitle: *const c_char);
    pub fn adw_action_row_add_prefix(self_: *mut AdwActionRow, widget: *mut GtkWidget);
    pub fn adw_action_row_add_suffix(self_: *mut AdwActionRow, widget: *mut GtkWidget);

    pub fn adw_switch_row_new() -> *mut GtkWidget;
    pub fn adw_switch_row_get_active(self_: *mut AdwSwitchRow) -> gboolean;
    pub fn adw_switch_row_set_active(self_: *mut AdwSwitchRow, is_active: gboolean);

    pub fn adw_combo_row_new() -> *mut GtkWidget;
    pub fn adw_combo_row_set_model(self_: *mut AdwComboRow, model: *mut GListModel);
    pub fn adw_combo_row_get_selected(self_: *mut AdwComboRow) -> guint;
    pub fn adw_combo_row_set_selected(self_: *mut AdwComboRow, position: guint);

    pub fn adw_entry_row_new() -> *mut GtkWidget;
    pub fn adw_entry_row_set_show_apply_button(self_: *mut AdwEntryRow, show: gboolean);

    pub fn adw_spin_row_new_with_range(min: gdouble, max: gdouble, step: gdouble)
        -> *mut GtkWidget;
    pub fn adw_spin_row_get_value(self_: *mut AdwSpinRow) -> gdouble;
    pub fn adw_spin_row_set_value(self_: *mut AdwSpinRow, value: gdouble);
    pub fn adw_spin_row_set_digits(self_: *mut AdwSpinRow, digits: guint);

    pub fn adw_shortcuts_dialog_new() -> *mut AdwDialog;
    pub fn adw_shortcuts_dialog_add(
        self_: *mut AdwShortcutsDialog,
        section: *mut AdwShortcutsSection,
    );
    pub fn adw_shortcuts_section_new(title: *const c_char) -> *mut AdwShortcutsSection;
    pub fn adw_shortcuts_section_add(self_: *mut AdwShortcutsSection, item: *mut AdwShortcutsItem);
    pub fn adw_shortcuts_item_new(
        title: *const c_char,
        accelerator: *const c_char,
    ) -> *mut AdwShortcutsItem;
}

// --- Fase 5 (Vault): diálogos de alerta, entrada de contraseña y portapapeles propio. ---

#[repr(C)]
pub struct AdwAlertDialog {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GtkPasswordEntry {
    _private: [u8; 0],
}

#[link(name = "gtk-4")]
extern "C" {
    pub fn gdk_clipboard_set_content(
        clipboard: *mut GdkClipboard,
        provider: *mut GdkContentProvider,
    ) -> gboolean;
    pub fn gdk_clipboard_get_content(clipboard: *mut GdkClipboard) -> *mut GdkContentProvider;

    pub fn gtk_widget_set_focus_on_click(widget: *mut GtkWidget, focus_on_click: gboolean);
    pub fn gtk_label_set_selectable(self_: *mut GtkLabel, setting: gboolean);

    pub fn gtk_password_entry_new() -> *mut GtkWidget;
    pub fn gtk_password_entry_set_show_peek_icon(
        entry: *mut GtkPasswordEntry,
        show_peek_icon: gboolean,
    );
}

#[link(name = "adwaita-1")]
extern "C" {
    pub fn adw_alert_dialog_new(heading: *const c_char, body: *const c_char) -> *mut AdwDialog;
    pub fn adw_alert_dialog_set_extra_child(self_: *mut AdwAlertDialog, child: *mut GtkWidget);
    pub fn adw_alert_dialog_add_response(
        self_: *mut AdwAlertDialog,
        id: *const c_char,
        label: *const c_char,
    );
    pub fn adw_alert_dialog_set_response_appearance(
        self_: *mut AdwAlertDialog,
        response: *const c_char,
        appearance: c_int,
    );
    pub fn adw_alert_dialog_set_response_enabled(
        self_: *mut AdwAlertDialog,
        response: *const c_char,
        enabled: gboolean,
    );
    pub fn adw_alert_dialog_set_default_response(
        self_: *mut AdwAlertDialog,
        response: *const c_char,
    );
    pub fn adw_alert_dialog_set_close_response(self_: *mut AdwAlertDialog, response: *const c_char);
    pub fn adw_preferences_row_set_use_markup(self_: *mut AdwPreferencesRow, use_markup: gboolean);
}
