use crate::glib::Bytes;
use crate::object::{
    connect_signal_action_activate, connect_signal_action_activate_string, to_cstring, ObjectRef,
    SignalHandlerId,
};
use crate::sys;
use bitflags::bitflags;
use std::ffi::CStr;
use std::path::Path;
use std::ptr;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ApplicationFlags: u32 {
        const DEFAULT_FLAGS = 0;
        const HANDLES_OPEN = 1 << 2;
        const HANDLES_COMMAND_LINE = 1 << 3;
        const NON_UNIQUE = 1 << 5;
    }
}

#[derive(Clone)]
pub struct SimpleAction {
    obj: ObjectRef,
}

impl SimpleAction {
    pub fn new(name: &str) -> Self {
        let cname = to_cstring(name);
        let ptr = unsafe { sys::g_simple_action_new(cname.as_ptr(), ptr::null()) };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn new_string_param(name: &str) -> Self {
        let cname = to_cstring(name);
        let ctype = to_cstring("s");
        unsafe {
            let vtype = sys::g_variant_type_new(ctype.as_ptr());
            let ptr = sys::g_simple_action_new(cname.as_ptr(), vtype);
            sys::g_variant_type_free(vtype);
            let obj = ObjectRef::from_new_ptr(ptr.cast());
            Self { obj }
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        unsafe {
            sys::g_simple_action_set_enabled(
                self.obj.as_ptr().cast(),
                if enabled { sys::GTRUE } else { sys::GFALSE },
            );
        }
    }

    pub fn connect_activate<F: Fn() + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_action_activate(self.obj.as_ptr(), callback)
    }

    pub fn connect_activate_string<F: Fn(String) + 'static>(&self, callback: F) -> SignalHandlerId {
        connect_signal_action_activate_string(self.obj.as_ptr(), callback)
    }

    pub fn as_action_ptr(&self) -> *mut sys::GAction {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct SimpleActionGroup {
    obj: ObjectRef,
}

impl SimpleActionGroup {
    pub fn new() -> Self {
        let ptr = unsafe { sys::g_simple_action_group_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn add_action(&self, action: &SimpleAction) {
        unsafe {
            sys::g_action_map_add_action(self.obj.as_ptr().cast(), action.as_action_ptr());
        }
    }

    pub fn as_action_group_ptr(&self) -> *mut sys::GActionGroup {
        self.obj.as_ptr().cast()
    }
}

impl Default for SimpleActionGroup {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct Icon {
    obj: ObjectRef,
}

impl Icon {
    pub fn themed(icon_name: &str) -> Self {
        let c_name = to_cstring(icon_name);
        let ptr = unsafe { sys::g_themed_icon_new(c_name.as_ptr()) };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn from_bytes(bytes: &Bytes) -> Self {
        let ptr = unsafe { sys::g_bytes_icon_new(bytes.as_ptr()) };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub(crate) fn from_object_ref(obj: ObjectRef) -> Self {
        Self { obj }
    }

    pub fn as_gicon_ptr(&self) -> *mut sys::GIcon {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct Notification {
    obj: ObjectRef,
}

impl Notification {
    pub fn new(title: &str) -> Self {
        let c_title = to_cstring(title);
        let ptr = unsafe { sys::g_notification_new(c_title.as_ptr()) };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn set_body(&self, body: Option<&str>) {
        let c_body = body.map(to_cstring);
        unsafe {
            sys::g_notification_set_body(
                self.obj.as_ptr().cast(),
                c_body.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
            );
        }
    }

    pub fn set_icon(&self, icon: &Icon) {
        unsafe {
            sys::g_notification_set_icon(self.obj.as_ptr().cast(), icon.as_gicon_ptr());
        }
    }

    pub fn as_notification_ptr(&self) -> *mut sys::GNotification {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct Menu {
    obj: ObjectRef,
}

impl Menu {
    pub fn new() -> Self {
        let ptr = unsafe { sys::g_menu_new() };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn append(&self, label: &str, detailed_action: Option<&str>) {
        let c_label = to_cstring(label);
        let c_action = detailed_action.map(to_cstring);
        unsafe {
            sys::g_menu_append(
                self.obj.as_ptr().cast(),
                c_label.as_ptr(),
                c_action.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
            );
        }
    }

    pub fn append_section(&self, label: Option<&str>, section: &Menu) {
        let c_label = label.map(to_cstring);
        unsafe {
            sys::g_menu_append_section(
                self.obj.as_ptr().cast(),
                c_label.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
                section.as_menu_model_ptr(),
            );
        }
    }

    pub fn append_submenu(&self, label: &str, submenu: &Menu) {
        let c_label = to_cstring(label);
        unsafe {
            sys::g_menu_append_submenu(
                self.obj.as_ptr().cast(),
                c_label.as_ptr(),
                submenu.as_menu_model_ptr(),
            );
        }
    }

    pub fn remove_all(&self) {
        unsafe {
            sys::g_menu_remove_all(self.obj.as_ptr().cast());
        }
    }

    pub fn as_menu_model_ptr(&self) -> *mut sys::GMenuModel {
        self.obj.as_ptr().cast()
    }
}

impl Default for Menu {
    fn default() -> Self {
        Self::new()
    }
}

/// Envoltorio seguro sobre `GSettings` con búsqueda de esquema en rutas del sistema
/// y respaldo opcional a un directorio local (`data/` con `gschemas.compiled`).
#[derive(Clone)]
pub struct Settings {
    obj: ObjectRef,
    schema: *mut sys::GSettingsSchema,
}

impl Settings {
    /// Intenta abrir `schema_id` desde las fuentes estándar de `GSettings` o desde
    /// un directorio local opcional donde exista `gschemas.compiled`.
    pub fn try_new(schema_id: &str, local_schema_dir: Option<&Path>) -> Option<Self> {
        let c_schema_id = to_cstring(schema_id);
        unsafe {
            let default_src = sys::g_settings_schema_source_get_default();
            if !default_src.is_null() {
                let schema = sys::g_settings_schema_source_lookup(
                    default_src,
                    c_schema_id.as_ptr(),
                    sys::GTRUE,
                );
                if !schema.is_null() {
                    return Self::from_schema(schema);
                }
            }

            if let Some(dir) = local_schema_dir {
                if dir.join("gschemas.compiled").is_file() {
                    let c_dir = to_cstring(&dir.to_string_lossy());
                    let mut err: *mut sys::GError = ptr::null_mut();
                    let local_src = sys::g_settings_schema_source_new_from_directory(
                        c_dir.as_ptr(),
                        default_src,
                        sys::GFALSE,
                        &mut err,
                    );
                    if !err.is_null() {
                        sys::g_error_free(err);
                    }
                    if !local_src.is_null() {
                        let schema = sys::g_settings_schema_source_lookup(
                            local_src,
                            c_schema_id.as_ptr(),
                            sys::GTRUE,
                        );
                        sys::g_settings_schema_source_unref(local_src);
                        if !schema.is_null() {
                            return Self::from_schema(schema);
                        }
                    }
                }
            }
            None
        }
    }

    unsafe fn from_schema(schema: *mut sys::GSettingsSchema) -> Option<Self> {
        let ptr = sys::g_settings_new_full(schema, ptr::null_mut(), ptr::null());
        if ptr.is_null() {
            sys::g_settings_schema_unref(schema);
            return None;
        }
        let obj = ObjectRef::from_new_ptr(ptr.cast());
        // `g_settings_new_full` incrementa internamente la referencia del esquema;
        // conservamos una referencia propia para `has_key` y la liberamos en `Drop`.
        Some(Self { obj, schema })
    }

    pub fn has_key(&self, key: &str) -> bool {
        let c_key = to_cstring(key);
        unsafe { sys::g_settings_schema_has_key(self.schema, c_key.as_ptr()) != 0 }
    }

    pub fn string(&self, key: &str) -> Option<String> {
        if !self.has_key(key) {
            return None;
        }
        let c_key = to_cstring(key);
        unsafe {
            let ptr = sys::g_settings_get_string(self.obj.as_ptr().cast(), c_key.as_ptr());
            if ptr.is_null() {
                return None;
            }
            let val = CStr::from_ptr(ptr).to_string_lossy().into_owned();
            sys::g_free(ptr.cast());
            Some(val)
        }
    }

    pub fn set_string(&self, key: &str, value: &str) -> bool {
        if !self.has_key(key) {
            return false;
        }
        let c_key = to_cstring(key);
        let c_val = to_cstring(value);
        unsafe {
            sys::g_settings_set_string(self.obj.as_ptr().cast(), c_key.as_ptr(), c_val.as_ptr())
                != 0
        }
    }

    pub fn boolean(&self, key: &str) -> Option<bool> {
        if !self.has_key(key) {
            return None;
        }
        let c_key = to_cstring(key);
        unsafe { Some(sys::g_settings_get_boolean(self.obj.as_ptr().cast(), c_key.as_ptr()) != 0) }
    }

    pub fn set_boolean(&self, key: &str, value: bool) -> bool {
        if !self.has_key(key) {
            return false;
        }
        let c_key = to_cstring(key);
        unsafe {
            sys::g_settings_set_boolean(
                self.obj.as_ptr().cast(),
                c_key.as_ptr(),
                if value { sys::GTRUE } else { sys::GFALSE },
            ) != 0
        }
    }

    pub fn double(&self, key: &str) -> Option<f64> {
        if !self.has_key(key) {
            return None;
        }
        let c_key = to_cstring(key);
        unsafe {
            Some(sys::g_settings_get_double(
                self.obj.as_ptr().cast(),
                c_key.as_ptr(),
            ))
        }
    }

    pub fn set_double(&self, key: &str, value: f64) -> bool {
        if !self.has_key(key) {
            return false;
        }
        let c_key = to_cstring(key);
        unsafe { sys::g_settings_set_double(self.obj.as_ptr().cast(), c_key.as_ptr(), value) != 0 }
    }

    pub fn uint(&self, key: &str) -> Option<u32> {
        if !self.has_key(key) {
            return None;
        }
        let c_key = to_cstring(key);
        unsafe {
            Some(sys::g_settings_get_uint(
                self.obj.as_ptr().cast(),
                c_key.as_ptr(),
            ))
        }
    }

    pub fn set_uint(&self, key: &str, value: u32) -> bool {
        if !self.has_key(key) {
            return false;
        }
        let c_key = to_cstring(key);
        unsafe { sys::g_settings_set_uint(self.obj.as_ptr().cast(), c_key.as_ptr(), value) != 0 }
    }

    pub fn int(&self, key: &str) -> Option<i32> {
        if !self.has_key(key) {
            return None;
        }
        let c_key = to_cstring(key);
        unsafe {
            Some(sys::g_settings_get_int(
                self.obj.as_ptr().cast(),
                c_key.as_ptr(),
            ))
        }
    }

    pub fn set_int(&self, key: &str, value: i32) -> bool {
        if !self.has_key(key) {
            return false;
        }
        let c_key = to_cstring(key);
        unsafe { sys::g_settings_set_int(self.obj.as_ptr().cast(), c_key.as_ptr(), value) != 0 }
    }

    pub fn sync() {
        unsafe {
            sys::g_settings_sync();
        }
    }
}

impl Drop for Settings {
    fn drop(&mut self) {
        if !self.schema.is_null() {
            unsafe {
                sys::g_settings_schema_unref(self.schema);
            }
        }
    }
}

/// Conexión D-Bus de sesión obtenida desde `GApplication` (`g_application_get_dbus_connection`).
#[derive(Clone)]
pub struct DBusConnection {
    obj: ObjectRef,
}

impl DBusConnection {
    /// Envuelve un puntero prestado `*mut sys::GDBusConnection` sin transferir propiedad.
    ///
    /// # Safety
    /// `ptr` debe ser un puntero válido a `GDBusConnection` o nulo.
    pub unsafe fn from_borrowed_ptr(ptr: *mut sys::GDBusConnection) -> Option<Self> {
        if ptr.is_null() {
            None
        } else {
            Some(Self {
                obj: ObjectRef::from_borrowed_ptr(ptr.cast())?,
            })
        }
    }

    /// Registra una interfaz D-Bus descrita por XML de introspección en `object_path`.
    pub fn register_object<F>(
        &self,
        object_path: &str,
        introspection_xml: &str,
        interface_name: &str,
        on_method_call: F,
    ) -> Result<DBusRegistration, String>
    where
        F: Fn(&str, DBusParams, DBusInvocation) + 'static,
    {
        let c_xml = to_cstring(introspection_xml);
        let c_iface = to_cstring(interface_name);
        let c_path = to_cstring(object_path);

        unsafe {
            let mut err: *mut sys::GError = ptr::null_mut();
            let node_info = sys::g_dbus_node_info_new_for_xml(c_xml.as_ptr(), &mut err);
            if node_info.is_null() {
                let msg = take_gerror_message(err, "g_dbus_node_info_new_for_xml failed");
                return Err(msg);
            }

            let iface_info = sys::g_dbus_node_info_lookup_interface(node_info, c_iface.as_ptr());
            if iface_info.is_null() {
                sys::g_dbus_node_info_unref(node_info);
                return Err(format!("interface '{interface_name}' not found in XML"));
            }

            let state = Box::new(DBusMethodState {
                handler: Box::new(on_method_call),
            });
            let state_ptr = Box::into_raw(state);

            let vtable = sys::GDBusInterfaceVTable {
                method_call: Some(dbus_method_trampoline),
                get_property: None,
                set_property: None,
                padding: [ptr::null_mut(); 8],
            };

            let reg_id = sys::g_dbus_connection_register_object(
                self.obj.as_ptr().cast(),
                c_path.as_ptr(),
                iface_info,
                &vtable,
                state_ptr.cast(),
                Some(dbus_method_state_destroy),
                &mut err,
            );
            sys::g_dbus_node_info_unref(node_info);

            if reg_id == 0 {
                let msg = take_gerror_message(err, "g_dbus_connection_register_object failed");
                return Err(msg);
            }

            Ok(DBusRegistration {
                connection: self.clone(),
                registration_id: reg_id,
            })
        }
    }
}

pub struct DBusRegistration {
    connection: DBusConnection,
    registration_id: u32,
}

impl DBusRegistration {
    pub fn unregister(mut self) {
        if self.registration_id != 0 {
            unsafe {
                sys::g_dbus_connection_unregister_object(
                    self.connection.obj.as_ptr().cast(),
                    self.registration_id,
                );
            }
            self.registration_id = 0;
        }
    }
}

impl Drop for DBusRegistration {
    fn drop(&mut self) {
        if self.registration_id != 0 {
            unsafe {
                sys::g_dbus_connection_unregister_object(
                    self.connection.obj.as_ptr().cast(),
                    self.registration_id,
                );
            }
            self.registration_id = 0;
        }
    }
}

/// Parámetros recibidos en una llamada a método D-Bus (`GVariant` tupla).
pub struct DBusParams {
    ptr: *mut sys::GVariant,
}

impl DBusParams {
    pub fn string(&self, index: usize) -> Option<String> {
        if self.ptr.is_null() {
            return None;
        }
        unsafe {
            let child = sys::g_variant_get_child_value(self.ptr, index);
            if child.is_null() {
                return None;
            }
            let cstr = sys::g_variant_get_string(child, ptr::null_mut());
            let out = if cstr.is_null() {
                None
            } else {
                Some(CStr::from_ptr(cstr).to_string_lossy().into_owned())
            };
            sys::g_variant_unref(child);
            out
        }
    }

    pub fn uint64(&self, index: usize) -> Option<u64> {
        if self.ptr.is_null() {
            return None;
        }
        unsafe {
            let child = sys::g_variant_get_child_value(self.ptr, index);
            if child.is_null() {
                return None;
            }
            let val = sys::g_variant_get_uint64(child);
            sys::g_variant_unref(child);
            Some(val)
        }
    }

    pub fn boolean(&self, index: usize) -> Option<bool> {
        if self.ptr.is_null() {
            return None;
        }
        unsafe {
            let child = sys::g_variant_get_child_value(self.ptr, index);
            if child.is_null() {
                return None;
            }
            let val = sys::g_variant_get_boolean(child) != 0;
            sys::g_variant_unref(child);
            Some(val)
        }
    }
}

/// Invocación D-Bus pendiente de respuesta.
pub struct DBusInvocation {
    ptr: *mut sys::GDBusMethodInvocation,
}

impl DBusInvocation {
    pub fn return_unit(self) {
        unsafe {
            sys::g_dbus_method_invocation_return_value(self.ptr, ptr::null_mut());
        }
    }

    pub fn return_string(self, value: &str) {
        let c_val = to_cstring(value);
        unsafe {
            let v_str = sys::g_variant_new_string(c_val.as_ptr());
            let tuple = sys::g_variant_new_tuple(&v_str, 1);
            sys::g_dbus_method_invocation_return_value(self.ptr, tuple);
        }
    }

    pub fn return_bool(self, value: bool) {
        unsafe {
            let v_bool = sys::g_variant_new_boolean(if value { sys::GTRUE } else { sys::GFALSE });
            let tuple = sys::g_variant_new_tuple(&v_bool, 1);
            sys::g_dbus_method_invocation_return_value(self.ptr, tuple);
        }
    }

    /// Devuelve `(a(tss))`: array de tuplas `(tab_id, title, url)`.
    pub fn return_tab_list(self, tabs: &[(u64, String, String)]) {
        let c_elem_type = to_cstring("(tss)");
        unsafe {
            let elem_type = sys::g_variant_type_new(c_elem_type.as_ptr());
            let mut items: Vec<*mut sys::GVariant> = Vec::with_capacity(tabs.len());
            for (id, title, url) in tabs {
                let c_title = to_cstring(title);
                let c_url = to_cstring(url);
                let fields = [
                    sys::g_variant_new_uint64(*id),
                    sys::g_variant_new_string(c_title.as_ptr()),
                    sys::g_variant_new_string(c_url.as_ptr()),
                ];
                items.push(sys::g_variant_new_tuple(fields.as_ptr(), fields.len()));
            }
            let arr = sys::g_variant_new_array(
                elem_type,
                if items.is_empty() {
                    ptr::null()
                } else {
                    items.as_ptr()
                },
                items.len(),
            );
            sys::g_variant_type_free(elem_type);
            let tuple = sys::g_variant_new_tuple(&arr, 1);
            sys::g_dbus_method_invocation_return_value(self.ptr, tuple);
        }
    }

    pub fn return_error(self, error_name: &str, message: &str) {
        let c_name = to_cstring(error_name);
        let c_msg = to_cstring(message);
        unsafe {
            sys::g_dbus_method_invocation_return_dbus_error(
                self.ptr,
                c_name.as_ptr(),
                c_msg.as_ptr(),
            );
        }
    }
}

type DBusMethodHandler = Box<dyn Fn(&str, DBusParams, DBusInvocation)>;

struct DBusMethodState {
    handler: DBusMethodHandler,
}

unsafe extern "C" fn dbus_method_state_destroy(data: sys::gpointer) {
    if !data.is_null() {
        drop(Box::from_raw(data.cast::<DBusMethodState>()));
    }
}

unsafe extern "C" fn dbus_method_trampoline(
    _connection: *mut sys::GDBusConnection,
    _sender: *const libc::c_char,
    _object_path: *const libc::c_char,
    _interface_name: *const libc::c_char,
    method_name: *const libc::c_char,
    parameters: *mut sys::GVariant,
    invocation: *mut sys::GDBusMethodInvocation,
    user_data: sys::gpointer,
) {
    if user_data.is_null() || method_name.is_null() || invocation.is_null() {
        return;
    }
    let state = &*(user_data as *const DBusMethodState);
    let method = CStr::from_ptr(method_name).to_string_lossy();
    (state.handler)(
        &method,
        DBusParams { ptr: parameters },
        DBusInvocation { ptr: invocation },
    );
}

unsafe fn take_gerror_message(err: *mut sys::GError, fallback: &str) -> String {
    if err.is_null() {
        return fallback.to_string();
    }
    // GError layout: domain (u32), code (i32), message (*mut c_char)
    #[repr(C)]
    struct GErrorView {
        _domain: u32,
        _code: i32,
        message: *mut libc::c_char,
    }
    let view = &*(err as *const GErrorView);
    let msg = if view.message.is_null() {
        fallback.to_string()
    } else {
        CStr::from_ptr(view.message).to_string_lossy().into_owned()
    };
    sys::g_error_free(err);
    msg
}
