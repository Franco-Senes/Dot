//! Envoltorio seguro e idiomático en Rust sobre las bibliotecas nativas del SDK de GNOME
//! (`gtk-4`, `libadwaita-1`, `gio-2.0`, `gobject-2.0`, `glib-2.0`, `libsecret-1`,
//! `libsqlite3` y `libcrypto`).
//!
//! Cada familia de bibliotecas va detrás de una feature de Cargo para que `search-core`
//! pueda usar `sqlite` y `crypto` sin arrastrar GTK.

#[cfg(feature = "gtk")]
pub mod adw;
#[cfg(feature = "crypto")]
pub mod crypto;
#[cfg(feature = "gtk")]
pub mod gdk;
#[cfg(feature = "gtk")]
pub mod gio;
#[cfg(feature = "gtk")]
pub mod glib;
#[cfg(feature = "gtk")]
pub mod gtk;
#[cfg(feature = "gtk")]
pub mod object;
#[cfg(feature = "secret")]
pub mod secret;
#[cfg(feature = "sqlite")]
pub mod sqlite;
#[cfg(feature = "gtk")]
pub mod sys;

/// Inicializa GTK4 y libadwaita si aún no están inicializados en el proceso actual.
#[cfg(feature = "gtk")]
pub fn init() -> bool {
    unsafe {
        if sys::gtk_is_initialized() == 0 {
            // Evita que `gtk_init` pise `LC_NUMERIC` para que CEF/Chromium no emita advertencia
            // cuando invoca internamente `gtk_disable_setlocale()`.
            sys::gtk_disable_setlocale();

            let disable_portals = std::env::var("GTK_USE_PORTALS").as_deref() == Ok("0")
                || std::env::var("ADW_DISABLE_PORTAL").as_deref() == Ok("1");
            if disable_portals {
                sys::gtk_disable_portals();
            }
        }
        if sys::gtk_init_check() == 0 {
            return false;
        }
        if sys::adw_is_initialized() == 0 {
            sys::adw_init();
        }
        true
    }
}
