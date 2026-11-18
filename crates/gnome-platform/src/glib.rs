use crate::sys;
use std::ptr::NonNull;
use std::time::Duration;

pub struct Bytes {
    ptr: NonNull<sys::GBytes>,
}

impl Bytes {
    pub fn from_slice(data: &[u8]) -> Self {
        let ptr = unsafe { sys::g_bytes_new(data.as_ptr().cast(), data.len()) };
        Self {
            ptr: NonNull::new(ptr).expect("g_bytes_new returned null"),
        }
    }

    pub fn as_ptr(&self) -> *mut sys::GBytes {
        self.ptr.as_ptr()
    }
}

impl Drop for Bytes {
    fn drop(&mut self) {
        unsafe {
            sys::g_bytes_unref(self.ptr.as_ptr());
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceId(pub u32);

impl SourceId {
    pub fn remove(self) {
        if self.0 != 0 {
            unsafe {
                sys::g_source_remove(self.0);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlFlow {
    Continue,
    Break,
}

unsafe extern "C" fn destroy_source_box<T>(data: sys::gpointer) {
    if !data.is_null() {
        drop(Box::from_raw(data.cast::<T>()));
    }
}

/// Programa una tarea de una sola ejecución en el bucle principal de GLib (seguro desde cualquier hilo).
pub fn idle_add_once<F: FnOnce() + Send + 'static>(func: F) -> SourceId {
    unsafe extern "C" fn trampoline<F: FnOnce() + Send + 'static>(
        data: sys::gpointer,
    ) -> sys::gboolean {
        let slot = &mut *(data as *mut Option<F>);
        if let Some(cb) = slot.take() {
            cb();
        }
        sys::GFALSE
    }

    let boxed: Box<Option<F>> = Box::new(Some(func));
    let id = unsafe {
        sys::g_idle_add_full(
            sys::G_PRIORITY_DEFAULT_IDLE,
            Some(trampoline::<F>),
            Box::into_raw(boxed).cast(),
            Some(destroy_source_box::<Option<F>>),
        )
    };
    SourceId(id)
}

/// Programa una tarea diferida de una sola ejecución en el bucle principal de GLib (seguro entre hilos).
pub fn timeout_add_once<F: FnOnce() + Send + 'static>(delay: Duration, func: F) -> SourceId {
    unsafe extern "C" fn trampoline<F: FnOnce() + Send + 'static>(
        data: sys::gpointer,
    ) -> sys::gboolean {
        let slot = &mut *(data as *mut Option<F>);
        if let Some(cb) = slot.take() {
            cb();
        }
        sys::GFALSE
    }

    let ms = delay.as_millis().min(u128::from(u32::MAX)) as u32;
    let boxed: Box<Option<F>> = Box::new(Some(func));
    let id = unsafe {
        sys::g_timeout_add_full(
            sys::G_PRIORITY_DEFAULT,
            ms,
            Some(trampoline::<F>),
            Box::into_raw(boxed).cast(),
            Some(destroy_source_box::<Option<F>>),
        )
    };
    SourceId(id)
}

/// Programa un temporizador recurrente en el hilo principal de GLib.
pub fn timeout_add_local<F: FnMut() -> ControlFlow + 'static>(
    interval: Duration,
    func: F,
) -> SourceId {
    unsafe extern "C" fn trampoline<F: FnMut() -> ControlFlow + 'static>(
        data: sys::gpointer,
    ) -> sys::gboolean {
        let cb = &mut *(data as *mut F);
        match cb() {
            ControlFlow::Continue => sys::GTRUE,
            ControlFlow::Break => sys::GFALSE,
        }
    }

    let ms = interval.as_millis().min(u128::from(u32::MAX)) as u32;
    let boxed = Box::new(func);
    let id = unsafe {
        sys::g_timeout_add_full(
            sys::G_PRIORITY_DEFAULT,
            ms,
            Some(trampoline::<F>),
            Box::into_raw(boxed).cast(),
            Some(destroy_source_box::<F>),
        )
    };
    SourceId(id)
}

/// Programa una tarea de una sola ejecución en el hilo principal actual (permite capturar `Rc` y widgets GTK).
pub fn idle_add_local_once<F: FnOnce() + 'static>(func: F) -> SourceId {
    unsafe extern "C" fn trampoline<F: FnOnce() + 'static>(data: sys::gpointer) -> sys::gboolean {
        let slot = &mut *(data as *mut Option<F>);
        if let Some(cb) = slot.take() {
            cb();
        }
        sys::GFALSE
    }

    let boxed: Box<Option<F>> = Box::new(Some(func));
    let id = unsafe {
        sys::g_idle_add_full(
            sys::G_PRIORITY_DEFAULT_IDLE,
            Some(trampoline::<F>),
            Box::into_raw(boxed).cast(),
            Some(destroy_source_box::<Option<F>>),
        )
    };
    SourceId(id)
}

/// Programa un temporizador de una sola ejecución en el hilo principal actual (permite capturar `Rc` y widgets GTK).
pub fn timeout_add_local_once<F: FnOnce() + 'static>(delay: Duration, func: F) -> SourceId {
    unsafe extern "C" fn trampoline<F: FnOnce() + 'static>(data: sys::gpointer) -> sys::gboolean {
        let slot = &mut *(data as *mut Option<F>);
        if let Some(cb) = slot.take() {
            cb();
        }
        sys::GFALSE
    }

    let ms = delay.as_millis().min(u128::from(u32::MAX)) as u32;
    let boxed: Box<Option<F>> = Box::new(Some(func));
    let id = unsafe {
        sys::g_timeout_add_full(
            sys::G_PRIORITY_DEFAULT,
            ms,
            Some(trampoline::<F>),
            Box::into_raw(boxed).cast(),
            Some(destroy_source_box::<Option<F>>),
        )
    };
    SourceId(id)
}
