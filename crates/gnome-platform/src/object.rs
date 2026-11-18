use crate::sys;
use libc::{c_char, c_double, c_int, c_uint, c_void};
use std::ffi::CString;
use std::ptr::NonNull;

pub(crate) fn to_cstring(s: &str) -> CString {
    if !s.as_bytes().contains(&0) {
        // SAFETY: Verified above that `s` has no interior NUL bytes.
        unsafe { CString::from_vec_unchecked(s.as_bytes().to_vec()) }
    } else {
        let filtered: Vec<u8> = s.bytes().filter(|&b| b != 0).collect();
        // SAFETY: All NUL bytes were removed.
        unsafe { CString::from_vec_unchecked(filtered) }
    }
}

/// Referencia fuerte tipada sobre una instancia `GObject`.
#[derive(Debug, PartialEq, Eq)]
pub struct ObjectRef {
    ptr: NonNull<c_void>,
}

impl ObjectRef {
    /// Adopta un puntero recién construido: si es flotante (`GInitiallyUnowned`),
    /// hunde la referencia flotante con `g_object_ref_sink`; si ya es una referencia
    /// fuerte propia (`transfer full`), conserva ese recuento sin incrementarlo.
    ///
    /// # Safety
    /// `ptr` debe ser una instancia `GObject` válida recién creada.
    pub unsafe fn from_new_ptr(ptr: *mut c_void) -> Self {
        let nn = NonNull::new(ptr).expect("GObject constructor returned null");
        if sys::g_object_is_floating(nn.as_ptr()) != 0 {
            sys::g_object_ref_sink(nn.as_ptr());
        }
        Self { ptr: nn }
    }

    /// Toma una nueva referencia fuerte (`g_object_ref`) sobre un puntero prestado (`transfer none`).
    ///
    /// # Safety
    /// `ptr` debe ser una instancia `GObject` válida.
    pub unsafe fn from_borrowed_ptr(ptr: *mut c_void) -> Option<Self> {
        let nn = NonNull::new(ptr)?;
        sys::g_object_ref(nn.as_ptr());
        Some(Self { ptr: nn })
    }

    pub fn as_ptr(&self) -> *mut c_void {
        self.ptr.as_ptr()
    }
}

impl Clone for ObjectRef {
    fn clone(&self) -> Self {
        unsafe {
            sys::g_object_ref(self.ptr.as_ptr());
        }
        Self { ptr: self.ptr }
    }
}

impl Drop for ObjectRef {
    fn drop(&mut self) {
        unsafe {
            sys::g_object_unref(self.ptr.as_ptr());
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignalHandlerId(pub sys::gulong);

unsafe extern "C" fn drop_boxed_closure<F>(data: sys::gpointer, _closure: *mut c_void) {
    if !data.is_null() {
        drop(Box::from_raw(data.cast::<F>()));
    }
}

/// Conecta una señal sin argumentos adicionales: `void (*)(GObject*, gpointer)`.
pub(crate) fn connect_signal_void<F: Fn() + 'static>(
    instance: *mut c_void,
    signal: &str,
    after: bool,
    callback: F,
) -> SignalHandlerId {
    unsafe extern "C" fn trampoline<F: Fn() + 'static>(
        _instance: *mut c_void,
        user_data: sys::gpointer,
    ) {
        let cb = &*(user_data as *const F);
        cb();
    }

    let c_sig = to_cstring(signal);
    let data = Box::into_raw(Box::new(callback)) as sys::gpointer;
    let flags = if after { 1 } else { 0 };
    let cb_ptr: sys::GCallback = Some(unsafe {
        std::mem::transmute::<*const (), unsafe extern "C" fn()>(trampoline::<F> as *const ())
    });
    let id = unsafe {
        sys::g_signal_connect_data(
            instance,
            c_sig.as_ptr(),
            cb_ptr,
            data,
            Some(drop_boxed_closure::<F>),
            flags,
        )
    };
    SignalHandlerId(id)
}

/// Conecta una señal `notify::<prop>`: `void (*)(GObject*, GParamSpec*, gpointer)`.
pub(crate) fn connect_signal_notify<F: Fn() + 'static>(
    instance: *mut c_void,
    property: &str,
    callback: F,
) -> SignalHandlerId {
    unsafe extern "C" fn trampoline<F: Fn() + 'static>(
        _instance: *mut c_void,
        _pspec: *mut c_void,
        user_data: sys::gpointer,
    ) {
        let cb = &*(user_data as *const F);
        cb();
    }

    let detailed = format!("notify::{property}");
    let c_sig = to_cstring(&detailed);
    let data = Box::into_raw(Box::new(callback)) as sys::gpointer;
    let cb_ptr: sys::GCallback = Some(unsafe {
        std::mem::transmute::<*const (), unsafe extern "C" fn()>(trampoline::<F> as *const ())
    });
    let id = unsafe {
        sys::g_signal_connect_data(
            instance,
            c_sig.as_ptr(),
            cb_ptr,
            data,
            Some(drop_boxed_closure::<F>),
            0,
        )
    };
    SignalHandlerId(id)
}

/// Conecta una señal que devuelve `gboolean` (p. ej. `close-request`).
pub(crate) fn connect_signal_bool_ret<F: Fn() -> bool + 'static>(
    instance: *mut c_void,
    signal: &str,
    callback: F,
) -> SignalHandlerId {
    unsafe extern "C" fn trampoline<F: Fn() -> bool + 'static>(
        _instance: *mut c_void,
        user_data: sys::gpointer,
    ) -> sys::gboolean {
        let cb = &*(user_data as *const F);
        if cb() {
            sys::GTRUE
        } else {
            sys::GFALSE
        }
    }

    let c_sig = to_cstring(signal);
    let data = Box::into_raw(Box::new(callback)) as sys::gpointer;
    let cb_ptr: sys::GCallback = Some(unsafe {
        std::mem::transmute::<*const (), unsafe extern "C" fn()>(trampoline::<F> as *const ())
    });
    let id = unsafe {
        sys::g_signal_connect_data(
            instance,
            c_sig.as_ptr(),
            cb_ptr,
            data,
            Some(drop_boxed_closure::<F>),
            0,
        )
    };
    SignalHandlerId(id)
}

/// Conecta una señal `(instance, double, double, user_data) -> void` (p. ej. `motion`, `enter`, `swipe`).
pub(crate) fn connect_signal_f64_f64<F: Fn(f64, f64) + 'static>(
    instance: *mut c_void,
    signal: &str,
    callback: F,
) -> SignalHandlerId {
    unsafe extern "C" fn trampoline<F: Fn(f64, f64) + 'static>(
        _instance: *mut c_void,
        a: c_double,
        b: c_double,
        user_data: sys::gpointer,
    ) {
        let cb = &*(user_data as *const F);
        cb(a, b);
    }

    let c_sig = to_cstring(signal);
    let data = Box::into_raw(Box::new(callback)) as sys::gpointer;
    let cb_ptr: sys::GCallback = Some(unsafe {
        std::mem::transmute::<*const (), unsafe extern "C" fn()>(trampoline::<F> as *const ())
    });
    let id = unsafe {
        sys::g_signal_connect_data(
            instance,
            c_sig.as_ptr(),
            cb_ptr,
            data,
            Some(drop_boxed_closure::<F>),
            0,
        )
    };
    SignalHandlerId(id)
}

/// Conecta una señal `(instance, double, double, user_data) -> gboolean` (p. ej. `scroll`).
pub(crate) fn connect_signal_f64_f64_bool<F: Fn(f64, f64) -> bool + 'static>(
    instance: *mut c_void,
    signal: &str,
    callback: F,
) -> SignalHandlerId {
    unsafe extern "C" fn trampoline<F: Fn(f64, f64) -> bool + 'static>(
        _instance: *mut c_void,
        a: c_double,
        b: c_double,
        user_data: sys::gpointer,
    ) -> sys::gboolean {
        let cb = &*(user_data as *const F);
        if cb(a, b) {
            sys::GTRUE
        } else {
            sys::GFALSE
        }
    }

    let c_sig = to_cstring(signal);
    let data = Box::into_raw(Box::new(callback)) as sys::gpointer;
    let cb_ptr: sys::GCallback = Some(unsafe {
        std::mem::transmute::<*const (), unsafe extern "C" fn()>(trampoline::<F> as *const ())
    });
    let id = unsafe {
        sys::g_signal_connect_data(
            instance,
            c_sig.as_ptr(),
            cb_ptr,
            data,
            Some(drop_boxed_closure::<F>),
            0,
        )
    };
    SignalHandlerId(id)
}

/// Conecta una señal `(instance, int, double, double, user_data) -> void` (p. ej. `pressed`/`released`).
pub(crate) fn connect_signal_i32_f64_f64<F: Fn(i32, f64, f64) + 'static>(
    instance: *mut c_void,
    signal: &str,
    callback: F,
) -> SignalHandlerId {
    unsafe extern "C" fn trampoline<F: Fn(i32, f64, f64) + 'static>(
        _instance: *mut c_void,
        n: c_int,
        x: c_double,
        y: c_double,
        user_data: sys::gpointer,
    ) {
        let cb = &*(user_data as *const F);
        cb(n, x, y);
    }

    let c_sig = to_cstring(signal);
    let data = Box::into_raw(Box::new(callback)) as sys::gpointer;
    let cb_ptr: sys::GCallback = Some(unsafe {
        std::mem::transmute::<*const (), unsafe extern "C" fn()>(trampoline::<F> as *const ())
    });
    let id = unsafe {
        sys::g_signal_connect_data(
            instance,
            c_sig.as_ptr(),
            cb_ptr,
            data,
            Some(drop_boxed_closure::<F>),
            0,
        )
    };
    SignalHandlerId(id)
}

/// Conecta una señal `(instance, int, int, user_data) -> void` (p. ej. `GtkDrawingArea::resize`).
pub(crate) fn connect_signal_i32_i32<F: Fn(i32, i32) + 'static>(
    instance: *mut c_void,
    signal: &str,
    callback: F,
) -> SignalHandlerId {
    unsafe extern "C" fn trampoline<F: Fn(i32, i32) + 'static>(
        _instance: *mut c_void,
        w: c_int,
        h: c_int,
        user_data: sys::gpointer,
    ) {
        let cb = &*(user_data as *const F);
        cb(w, h);
    }

    let c_sig = to_cstring(signal);
    let data = Box::into_raw(Box::new(callback)) as sys::gpointer;
    let cb_ptr: sys::GCallback = Some(unsafe {
        std::mem::transmute::<*const (), unsafe extern "C" fn()>(trampoline::<F> as *const ())
    });
    let id = unsafe {
        sys::g_signal_connect_data(
            instance,
            c_sig.as_ptr(),
            cb_ptr,
            data,
            Some(drop_boxed_closure::<F>),
            0,
        )
    };
    SignalHandlerId(id)
}

/// Conecta `key-pressed`: `(controller, keyval, keycode, state, user_data) -> gboolean`.
pub(crate) fn connect_signal_key_pressed<F: Fn(u32, u32, u32) -> bool + 'static>(
    instance: *mut c_void,
    callback: F,
) -> SignalHandlerId {
    unsafe extern "C" fn trampoline<F: Fn(u32, u32, u32) -> bool + 'static>(
        _instance: *mut c_void,
        keyval: c_uint,
        keycode: c_uint,
        state: c_uint,
        user_data: sys::gpointer,
    ) -> sys::gboolean {
        let cb = &*(user_data as *const F);
        if cb(keyval, keycode, state) {
            sys::GTRUE
        } else {
            sys::GFALSE
        }
    }

    let c_sig = to_cstring("key-pressed");
    let data = Box::into_raw(Box::new(callback)) as sys::gpointer;
    let cb_ptr: sys::GCallback = Some(unsafe {
        std::mem::transmute::<*const (), unsafe extern "C" fn()>(trampoline::<F> as *const ())
    });
    let id = unsafe {
        sys::g_signal_connect_data(
            instance,
            c_sig.as_ptr(),
            cb_ptr,
            data,
            Some(drop_boxed_closure::<F>),
            0,
        )
    };
    SignalHandlerId(id)
}

/// Conecta `key-released`: `(controller, keyval, keycode, state, user_data) -> void`.
pub(crate) fn connect_signal_key_released<F: Fn(u32, u32, u32) + 'static>(
    instance: *mut c_void,
    callback: F,
) -> SignalHandlerId {
    unsafe extern "C" fn trampoline<F: Fn(u32, u32, u32) + 'static>(
        _instance: *mut c_void,
        keyval: c_uint,
        keycode: c_uint,
        state: c_uint,
        user_data: sys::gpointer,
    ) {
        let cb = &*(user_data as *const F);
        cb(keyval, keycode, state);
    }

    let c_sig = to_cstring("key-released");
    let data = Box::into_raw(Box::new(callback)) as sys::gpointer;
    let cb_ptr: sys::GCallback = Some(unsafe {
        std::mem::transmute::<*const (), unsafe extern "C" fn()>(trampoline::<F> as *const ())
    });
    let id = unsafe {
        sys::g_signal_connect_data(
            instance,
            c_sig.as_ptr(),
            cb_ptr,
            data,
            Some(drop_boxed_closure::<F>),
            0,
        )
    };
    SignalHandlerId(id)
}

/// Conecta `GSimpleAction::activate`: `(action, parameter, user_data) -> void`.
pub(crate) fn connect_signal_action_activate<F: Fn() + 'static>(
    instance: *mut c_void,
    callback: F,
) -> SignalHandlerId {
    unsafe extern "C" fn trampoline<F: Fn() + 'static>(
        _action: *mut c_void,
        _param: *mut c_void,
        user_data: sys::gpointer,
    ) {
        let cb = &*(user_data as *const F);
        cb();
    }

    let c_sig = to_cstring("activate");
    let data = Box::into_raw(Box::new(callback)) as sys::gpointer;
    let cb_ptr: sys::GCallback = Some(unsafe {
        std::mem::transmute::<*const (), unsafe extern "C" fn()>(trampoline::<F> as *const ())
    });
    let id = unsafe {
        sys::g_signal_connect_data(
            instance,
            c_sig.as_ptr(),
            cb_ptr,
            data,
            Some(drop_boxed_closure::<F>),
            0,
        )
    };
    SignalHandlerId(id)
}

/// Conecta `GApplication::open`: `(app, GFile** files, gint n_files, const char* hint, user_data)`.
pub(crate) fn connect_signal_app_open<F: Fn(Vec<String>) + 'static>(
    instance: *mut c_void,
    callback: F,
) -> SignalHandlerId {
    unsafe extern "C" fn trampoline<F: Fn(Vec<String>) + 'static>(
        _app: *mut c_void,
        files: *mut *mut sys::GFile,
        n_files: c_int,
        _hint: *const c_char,
        user_data: sys::gpointer,
    ) {
        let mut uris = Vec::new();
        if !files.is_null() && n_files > 0 {
            let slice = std::slice::from_raw_parts(files, n_files as usize);
            for &file in slice {
                if !file.is_null() {
                    let uri_ptr = sys::g_file_get_uri(file);
                    if !uri_ptr.is_null() {
                        let s = std::ffi::CStr::from_ptr(uri_ptr)
                            .to_string_lossy()
                            .into_owned();
                        sys::g_free(uri_ptr.cast());
                        uris.push(s);
                    }
                }
            }
        }
        let cb = &*(user_data as *const F);
        cb(uris);
    }

    let c_sig = to_cstring("open");
    let data = Box::into_raw(Box::new(callback)) as sys::gpointer;
    let cb_ptr: sys::GCallback = Some(unsafe {
        std::mem::transmute::<*const (), unsafe extern "C" fn()>(trampoline::<F> as *const ())
    });
    let id = unsafe {
        sys::g_signal_connect_data(
            instance,
            c_sig.as_ptr(),
            cb_ptr,
            data,
            Some(drop_boxed_closure::<F>),
            0,
        )
    };
    SignalHandlerId(id)
}
