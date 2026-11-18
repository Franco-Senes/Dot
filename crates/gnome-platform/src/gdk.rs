use crate::glib::Bytes;
use crate::object::{to_cstring, ObjectRef};
use crate::sys;
use bitflags::bitflags;
use std::ffi::CStr;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ModifierType: u32 {
        const NONE = 0;
        const SHIFT_MASK = 1 << 0;
        const LOCK_MASK = 1 << 1;
        const CONTROL_MASK = 1 << 2;
        const ALT_MASK = 1 << 3;
        const BUTTON1_MASK = 1 << 8;
        const BUTTON2_MASK = 1 << 9;
        const BUTTON3_MASK = 1 << 10;
        const SUPER_MASK = 1 << 26;
        const HYPER_MASK = 1 << 27;
        const META_MASK = 1 << 28;
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryFormat {
    B8g8r8a8Premultiplied = 0,
    B8g8r8a8 = 3,
    R8g8b8a8 = 5,
}

#[derive(Clone)]
pub struct MemoryTexture {
    obj: ObjectRef,
}

impl MemoryTexture {
    pub fn new(
        width: i32,
        height: i32,
        format: MemoryFormat,
        bytes: &Bytes,
        stride: usize,
    ) -> Self {
        let ptr = unsafe {
            sys::gdk_memory_texture_new(width, height, format as i32, bytes.as_ptr(), stride)
        };
        let obj = unsafe { ObjectRef::from_new_ptr(ptr.cast()) };
        Self { obj }
    }

    pub fn as_paintable_ptr(&self) -> *mut sys::GdkPaintable {
        self.obj.as_ptr().cast()
    }
}

pub struct Display {
    ptr: *mut sys::GdkDisplay,
}

impl Display {
    pub fn default_display() -> Option<Self> {
        let ptr = unsafe { sys::gdk_display_get_default() };
        if ptr.is_null() {
            None
        } else {
            Some(Self { ptr })
        }
    }

    pub fn as_ptr(&self) -> *mut sys::GdkDisplay {
        self.ptr
    }

    pub fn set_clipboard_text(&self, text: &str) {
        let c_text = to_cstring(text);
        unsafe {
            let clipboard = sys::gdk_display_get_clipboard(self.ptr);
            if !clipboard.is_null() {
                sys::gdk_clipboard_set_text(clipboard, c_text.as_ptr());
            }
        }
    }

    pub fn read_clipboard_text_async<F: FnOnce(Option<String>) + 'static>(&self, callback: F) {
        unsafe extern "C" fn on_ready<F: FnOnce(Option<String>) + 'static>(
            source_object: *mut sys::GObject,
            res: *mut libc::c_void,
            user_data: sys::gpointer,
        ) {
            let cb = Box::from_raw(user_data.cast::<F>());
            let mut err: *mut sys::GError = std::ptr::null_mut();
            let text_ptr = sys::gdk_clipboard_read_text_finish(source_object.cast(), res, &mut err);
            if !err.is_null() {
                sys::g_error_free(err);
            }
            let text = if text_ptr.is_null() {
                None
            } else {
                let s = CStr::from_ptr(text_ptr).to_string_lossy().into_owned();
                sys::g_free(text_ptr.cast());
                Some(s)
            };
            cb(text);
        }

        unsafe {
            let clipboard = sys::gdk_display_get_clipboard(self.ptr);
            if clipboard.is_null() {
                callback(None);
                return;
            }
            let boxed = Box::new(callback);
            sys::gdk_clipboard_read_text_async(
                clipboard,
                std::ptr::null_mut(),
                Some(on_ready::<F>),
                Box::into_raw(boxed).cast(),
            );
        }
    }
}

pub fn keyval_to_unicode(keyval: u32) -> Option<char> {
    let cp = unsafe { sys::gdk_keyval_to_unicode(keyval) };
    if cp == 0 {
        None
    } else {
        char::from_u32(cp)
    }
}

pub fn keyval_to_lower(keyval: u32) -> u32 {
    unsafe { sys::gdk_keyval_to_lower(keyval) }
}

pub fn keyval_name(keyval: u32) -> Option<String> {
    unsafe {
        let ptr = sys::gdk_keyval_name(keyval);
        if ptr.is_null() {
            None
        } else {
            Some(CStr::from_ptr(ptr).to_string_lossy().into_owned())
        }
    }
}

/// Constantes estándar de teclas X11/GDK (`gdkkeysyms.h`).
pub mod keys {
    pub const BACKSPACE: u32 = 0xff08;
    pub const TAB: u32 = 0xff09;
    pub const ISO_LEFT_TAB: u32 = 0xfe20;
    pub const RETURN: u32 = 0xff0d;
    pub const KP_ENTER: u32 = 0xff8d;
    pub const ESCAPE: u32 = 0xff1b;
    pub const DELETE: u32 = 0xffff;
    pub const HOME: u32 = 0xff50;
    pub const LEFT: u32 = 0xff51;
    pub const UP: u32 = 0xff52;
    pub const RIGHT: u32 = 0xff53;
    pub const DOWN: u32 = 0xff54;
    pub const PAGE_UP: u32 = 0xff55;
    pub const PAGE_DOWN: u32 = 0xff56;
    pub const END: u32 = 0xff57;
    pub const INSERT: u32 = 0xff63;
    pub const F1: u32 = 0xffbe;
    pub const F2: u32 = 0xffbf;
    pub const F3: u32 = 0xffc0;
    pub const F4: u32 = 0xffc1;
    pub const F5: u32 = 0xffc2;
    pub const F6: u32 = 0xffc3;
    pub const F7: u32 = 0xffc4;
    pub const F8: u32 = 0xffc5;
    pub const F9: u32 = 0xffc6;
    pub const F10: u32 = 0xffc7;
    pub const F11: u32 = 0xffc8;
    pub const F12: u32 = 0xffc9;
    pub const SHIFT_L: u32 = 0xffe1;
    pub const SHIFT_R: u32 = 0xffe2;
    pub const CONTROL_L: u32 = 0xffe3;
    pub const CONTROL_R: u32 = 0xffe4;
    pub const ALT_L: u32 = 0xffe9;
    pub const ALT_R: u32 = 0xffea;
    pub const SUPER_L: u32 = 0xffeb;
    pub const SUPER_R: u32 = 0xffec;
    pub const SPACE: u32 = 0x0020;
    pub const PLUS: u32 = 0x002b;
    pub const MINUS: u32 = 0x002d;
    pub const EQUAL: u32 = 0x003d;
    pub const ZERO: u32 = 0x0030;
    pub const BRACKET_LEFT: u32 = 0x005b;
    pub const BRACKET_RIGHT: u32 = 0x005d;
    pub const KP_ADD: u32 = 0xffab;
    pub const KP_SUBTRACT: u32 = 0xffad;
    pub const KP_0: u32 = 0xffb0;
}
