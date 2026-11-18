use crate::object::{connect_signal_action_activate, to_cstring, ObjectRef, SignalHandlerId};
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

    pub fn as_action_ptr(&self) -> *mut sys::GAction {
        self.obj.as_ptr().cast()
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
