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

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct DragAction: u32 {
        const NONE = 0;
        const COPY = 1 << 0;
        const MOVE = 1 << 1;
        const LINK = 1 << 2;
        const ASK = 1 << 3;
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Rectangle {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rectangle {
    pub(crate) fn to_raw(self) -> sys::GdkRectangle {
        sys::GdkRectangle {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        }
    }
}

#[derive(Clone)]
pub struct Texture {
    obj: ObjectRef,
}

impl Texture {
    pub fn from_bytes(bytes: &Bytes) -> Option<Self> {
        unsafe {
            let mut err: *mut sys::GError = std::ptr::null_mut();
            let ptr = sys::gdk_texture_new_from_bytes(bytes.as_ptr(), &mut err);
            if !err.is_null() {
                sys::g_error_free(err);
            }
            if ptr.is_null() {
                None
            } else {
                Some(Self {
                    obj: ObjectRef::from_new_ptr(ptr.cast()),
                })
            }
        }
    }

    pub fn width(&self) -> i32 {
        unsafe { sys::gdk_texture_get_width(self.obj.as_ptr().cast()) }
    }

    pub fn height(&self) -> i32 {
        unsafe { sys::gdk_texture_get_height(self.obj.as_ptr().cast()) }
    }

    pub fn save_to_png_bytes(&self) -> Option<Bytes> {
        unsafe {
            let ptr = sys::gdk_texture_save_to_png_bytes(self.obj.as_ptr().cast());
            Bytes::from_new_ptr(ptr)
        }
    }

    /// `GdkTexture` implementa directamente `GIcon` y `GLoadableIcon` en GTK4,
    /// evitando pasar por `gdk-pixbuf`/`glycin` al usarse en `AdwTabPage::set_icon`.
    pub fn to_gicon(&self) -> crate::gio::Icon {
        crate::gio::Icon::from_object_ref(self.obj.clone())
    }

    pub fn as_paintable_ptr(&self) -> *mut sys::GdkPaintable {
        self.obj.as_ptr().cast()
    }
}

#[derive(Clone)]
pub struct ContentProvider {
    pub(crate) obj: ObjectRef,
}

impl ContentProvider {
    pub fn for_string(value: &str) -> Self {
        let c_str = to_cstring(value);
        unsafe {
            let mut gval = sys::GValue::default();
            sys::g_value_init(&mut gval, sys::G_TYPE_STRING);
            sys::g_value_set_string(&mut gval, c_str.as_ptr());
            let ptr = sys::gdk_content_provider_new_for_value(&gval);
            sys::g_value_unset(&mut gval);
            Self {
                obj: ObjectRef::from_new_ptr(ptr.cast()),
            }
        }
    }

    pub fn as_ptr(&self) -> *mut sys::GdkContentProvider {
        self.obj.as_ptr().cast()
    }
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

    pub fn to_texture(&self) -> Texture {
        Texture {
            obj: self.obj.clone(),
        }
    }

    pub fn to_gicon(&self) -> crate::gio::Icon {
        crate::gio::Icon::from_object_ref(self.obj.clone())
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
    pub const KEY_1: u32 = 0x0031;
    pub const KEY_9: u32 = 0x0039;
    pub const COMMA: u32 = 0x002c;
    pub const SLASH: u32 = 0x002f;
    pub const QUESTION: u32 = 0x003f;
    pub const BRACKET_LEFT: u32 = 0x005b;
    pub const BRACKET_RIGHT: u32 = 0x005d;
    pub const KP_ADD: u32 = 0xffab;
    pub const KP_SUBTRACT: u32 = 0xffad;
    pub const KP_0: u32 = 0xffb0;
}

// --- Fase 5 (Vault): portapapeles con proveedor propio para poder limpiarlo con seguridad. ---

impl ContentProvider {
    /// Proveedor de texto para un secreto (contraseña): la copia temporal en C se pone a cero
    /// en cuanto GLib ha hecho la suya.
    pub fn for_secret_string(value: &str) -> Self {
        use zeroize::Zeroize;
        let mut bytes: Vec<u8> = value.bytes().filter(|&b| b != 0).collect();
        bytes.push(0);
        unsafe {
            let mut gval = sys::GValue::default();
            sys::g_value_init(&mut gval, sys::G_TYPE_STRING);
            sys::g_value_set_string(&mut gval, bytes.as_ptr().cast());
            bytes.zeroize();
            let ptr = sys::gdk_content_provider_new_for_value(&gval);
            sys::g_value_unset(&mut gval);
            Self {
                obj: ObjectRef::from_new_ptr(ptr.cast()),
            }
        }
    }
}

impl Display {
    /// Publica `provider` como contenido del portapapeles de esta pantalla.
    pub fn set_clipboard_content(&self, provider: &ContentProvider) -> bool {
        unsafe {
            let clipboard = sys::gdk_display_get_clipboard(self.ptr);
            !clipboard.is_null()
                && sys::gdk_clipboard_set_content(clipboard, provider.as_ptr()) != 0
        }
    }

    /// `true` si el portapapeles sigue conteniendo exactamente `provider` (es decir, nadie ha
    /// copiado otra cosa desde entonces, ni en esta aplicación ni en otra).
    pub fn clipboard_holds(&self, provider: &ContentProvider) -> bool {
        unsafe {
            let clipboard = sys::gdk_display_get_clipboard(self.ptr);
            !clipboard.is_null() && sys::gdk_clipboard_get_content(clipboard) == provider.as_ptr()
        }
    }

    /// Vacía el portapapeles. Llamar solo tras comprobar `clipboard_holds`.
    pub fn clear_clipboard(&self) {
        unsafe {
            let clipboard = sys::gdk_display_get_clipboard(self.ptr);
            if !clipboard.is_null() {
                sys::gdk_clipboard_set_content(clipboard, std::ptr::null_mut());
            }
        }
    }
}
