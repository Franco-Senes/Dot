//! Envoltorio mínimo y seguro sobre `libsecret-1` (API de contraseñas simple del
//! Secret Service: GNOME Keyring, KWallet ≥ 5.97 o el portal de secretos de Flatpak).
//!
//! - Los secretos entran como `&[u8]` y salen en `Zeroizing<Vec<u8>>`; la copia que devuelve
//!   libsecret se libera con `secret_password_free`, que la pone a cero.
//! - Las búsquedas devuelven **solo atributos**, nunca secretos (igual que `Vault.swift`, que lee
//!   cada secreto por separado y solo cuando se muestra, copia o rellena).
//! - Los mensajes de error vienen de `GError` y nunca contienen el secreto.

use std::collections::BTreeMap;
use std::ffi::{CStr, CString};
use std::fmt;

use libc::{c_char, c_int, c_uint, c_void};
use zeroize::Zeroizing;

type Gboolean = c_int;

#[repr(C)]
struct GError {
    domain: u32,
    code: c_int,
    message: *mut c_char,
}

#[repr(C)]
struct GHashTable {
    _private: [u8; 0],
}

#[repr(C)]
struct GList {
    data: *mut c_void,
    next: *mut GList,
    prev: *mut GList,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RawSchemaAttribute {
    name: *const c_char,
    kind: c_int,
}

#[repr(C)]
struct RawSchema {
    name: *const c_char,
    flags: c_int,
    attributes: [RawSchemaAttribute; 32],
    reserved: c_int,
    reserved1: *mut c_void,
    reserved2: *mut c_void,
    reserved3: *mut c_void,
    reserved4: *mut c_void,
    reserved5: *mut c_void,
    reserved6: *mut c_void,
    reserved7: *mut c_void,
}

type GHashFunc = unsafe extern "C" fn(*const c_void) -> c_uint;
type GEqualFunc = unsafe extern "C" fn(*const c_void, *const c_void) -> Gboolean;
type GDestroyNotify = unsafe extern "C" fn(*mut c_void);

#[link(name = "glib-2.0")]
extern "C" {
    fn g_str_hash(v: *const c_void) -> c_uint;
    fn g_str_equal(a: *const c_void, b: *const c_void) -> Gboolean;
    fn g_free(mem: *mut c_void);
    fn g_strdup(s: *const c_char) -> *mut c_char;
    fn g_hash_table_new_full(
        hash: GHashFunc,
        equal: GEqualFunc,
        key_destroy: Option<GDestroyNotify>,
        value_destroy: Option<GDestroyNotify>,
    ) -> *mut GHashTable;
    fn g_hash_table_insert(
        table: *mut GHashTable,
        key: *mut c_void,
        value: *mut c_void,
    ) -> Gboolean;
    fn g_hash_table_lookup(table: *mut GHashTable, key: *const c_void) -> *mut c_void;
    fn g_hash_table_unref(table: *mut GHashTable);
    fn g_list_free_full(list: *mut GList, free: GDestroyNotify);
    fn g_error_free(error: *mut GError);
}

#[link(name = "gobject-2.0")]
extern "C" {
    fn g_object_unref(object: *mut c_void);
}

#[link(name = "secret-1")]
extern "C" {
    fn secret_service_get_sync(
        flags: c_int,
        cancellable: *mut c_void,
        error: *mut *mut GError,
    ) -> *mut c_void;
    fn secret_password_storev_sync(
        schema: *const RawSchema,
        attributes: *mut GHashTable,
        collection: *const c_char,
        label: *const c_char,
        password: *const c_char,
        cancellable: *mut c_void,
        error: *mut *mut GError,
    ) -> Gboolean;
    fn secret_password_lookupv_sync(
        schema: *const RawSchema,
        attributes: *mut GHashTable,
        cancellable: *mut c_void,
        error: *mut *mut GError,
    ) -> *mut c_char;
    fn secret_password_clearv_sync(
        schema: *const RawSchema,
        attributes: *mut GHashTable,
        cancellable: *mut c_void,
        error: *mut *mut GError,
    ) -> Gboolean;
    fn secret_password_searchv_sync(
        schema: *const RawSchema,
        attributes: *mut GHashTable,
        flags: c_int,
        cancellable: *mut c_void,
        error: *mut *mut GError,
    ) -> *mut GList;
    fn secret_retrievable_get_attributes(retrievable: *mut c_void) -> *mut GHashTable;
    fn secret_password_free(password: *mut c_char);
}

const SECRET_SCHEMA_NONE: c_int = 0;
const SECRET_SCHEMA_DONT_MATCH_NAME: c_int = 1 << 1;
const SECRET_SCHEMA_ATTRIBUTE_STRING: c_int = 0;
const SECRET_SEARCH_ALL: c_int = 1 << 1;
const SECRET_SEARCH_UNLOCK: c_int = 1 << 2;

/// Error del Secret Service. Nunca incluye el secreto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretError {
    pub message: String,
}

impl fmt::Display for SecretError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Secret Service: {}", self.message)
    }
}

impl std::error::Error for SecretError {}

fn err(message: &str) -> SecretError {
    SecretError {
        message: message.to_owned(),
    }
}

/// Convierte (y libera) un `GError`.
unsafe fn take_error(error: *mut GError, fallback: &str) -> SecretError {
    if error.is_null() {
        return err(fallback);
    }
    let message = if (*error).message.is_null() {
        fallback.to_owned()
    } else {
        CStr::from_ptr((*error).message)
            .to_string_lossy()
            .into_owned()
    };
    g_error_free(error);
    SecretError { message }
}

/// Esquema de libsecret con atributos de tipo cadena.
pub struct Schema {
    // Los `CString` se conservan para que los punteros de `raw` sigan siendo válidos.
    _name: CString,
    attributes: Vec<CString>,
    raw: Box<RawSchema>,
}

// SAFETY: el esquema es inmutable tras su creación y solo contiene punteros a sus propios
// `CString`, que viven tanto como él.
unsafe impl Send for Schema {}
unsafe impl Sync for Schema {}

impl Schema {
    /// Crea un esquema. Con `match_name = false` se usa `SECRET_SCHEMA_DONT_MATCH_NAME`
    /// (necesario para leer secretos de otras aplicaciones, p. ej. "Chrome Safe Storage").
    pub fn new(name: &str, match_name: bool, attributes: &[&str]) -> Result<Self, SecretError> {
        if attributes.is_empty() || attributes.len() > 31 {
            return Err(err("un esquema necesita entre 1 y 31 atributos"));
        }
        let c_name = CString::new(name).map_err(|_| err("nombre de esquema con NUL"))?;
        let c_attrs = attributes
            .iter()
            .map(|a| CString::new(*a).map_err(|_| err("atributo con NUL")))
            .collect::<Result<Vec<_>, _>>()?;
        let mut slots = [RawSchemaAttribute {
            name: std::ptr::null(),
            kind: 0,
        }; 32];
        for (slot, attr) in slots.iter_mut().zip(c_attrs.iter()) {
            slot.name = attr.as_ptr();
            slot.kind = SECRET_SCHEMA_ATTRIBUTE_STRING;
        }
        let raw = Box::new(RawSchema {
            name: c_name.as_ptr(),
            flags: if match_name {
                SECRET_SCHEMA_NONE
            } else {
                SECRET_SCHEMA_DONT_MATCH_NAME
            },
            attributes: slots,
            reserved: 0,
            reserved1: std::ptr::null_mut(),
            reserved2: std::ptr::null_mut(),
            reserved3: std::ptr::null_mut(),
            reserved4: std::ptr::null_mut(),
            reserved5: std::ptr::null_mut(),
            reserved6: std::ptr::null_mut(),
            reserved7: std::ptr::null_mut(),
        });
        Ok(Schema {
            _name: c_name,
            attributes: c_attrs,
            raw,
        })
    }

    /// Nombres de los atributos del esquema.
    pub fn attribute_names(&self) -> Vec<String> {
        self.attributes
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    fn has_attribute(&self, name: &str) -> bool {
        self.attributes
            .iter()
            .any(|a| a.as_bytes() == name.as_bytes())
    }

    /// Construye la tabla `GHashTable<gchar*, gchar*>` validando que cada clave pertenece al
    /// esquema (libsecret aborta con un `g_critical` si no).
    fn table(&self, attrs: &[(&str, &str)]) -> Result<AttrTable, SecretError> {
        for (key, _) in attrs {
            if !self.has_attribute(key) {
                return Err(err("atributo fuera del esquema"));
            }
        }
        let keys = attrs
            .iter()
            .map(|(k, v)| {
                Ok((
                    CString::new(*k).map_err(|_| err("atributo con NUL"))?,
                    CString::new(*v).map_err(|_| err("valor de atributo con NUL"))?,
                ))
            })
            .collect::<Result<Vec<_>, SecretError>>()?;
        unsafe {
            let table = g_hash_table_new_full(g_str_hash, g_str_equal, Some(g_free), Some(g_free));
            if table.is_null() {
                return Err(err("sin memoria"));
            }
            for (k, v) in &keys {
                g_hash_table_insert(
                    table,
                    g_strdup(k.as_ptr()).cast(),
                    g_strdup(v.as_ptr()).cast(),
                );
            }
            Ok(AttrTable(table))
        }
    }
}

struct AttrTable(*mut GHashTable);

impl Drop for AttrTable {
    fn drop(&mut self) {
        unsafe { g_hash_table_unref(self.0) }
    }
}

/// ¿Hay un Secret Service accesible en el bus de sesión? (Bloquea brevemente.)
pub fn is_available() -> bool {
    unsafe {
        let mut error: *mut GError = std::ptr::null_mut();
        let service = secret_service_get_sync(0, std::ptr::null_mut(), &mut error);
        if !error.is_null() {
            g_error_free(error);
        }
        if service.is_null() {
            return false;
        }
        g_object_unref(service);
        true
    }
}

/// Guarda (o sustituye, si todos los atributos coinciden) un secreto en la colección por defecto.
pub fn store(
    schema: &Schema,
    attrs: &[(&str, &str)],
    label: &str,
    secret: &[u8],
) -> Result<(), SecretError> {
    if secret.contains(&0) {
        return Err(err("el secreto contiene un byte NUL"));
    }
    let table = schema.table(attrs)?;
    let c_label = CString::new(label).map_err(|_| err("etiqueta con NUL"))?;
    let mut c_secret = Zeroizing::new(Vec::with_capacity(secret.len() + 1));
    c_secret.extend_from_slice(secret);
    c_secret.push(0);
    unsafe {
        let mut error: *mut GError = std::ptr::null_mut();
        let ok = secret_password_storev_sync(
            &*schema.raw,
            table.0,
            std::ptr::null(),
            c_label.as_ptr(),
            c_secret.as_ptr().cast(),
            std::ptr::null_mut(),
            &mut error,
        );
        if ok == 0 || !error.is_null() {
            return Err(take_error(error, "no se pudo guardar el secreto"));
        }
    }
    Ok(())
}

/// Lee un secreto. `Ok(None)` si no existe.
pub fn lookup(
    schema: &Schema,
    attrs: &[(&str, &str)],
) -> Result<Option<Zeroizing<Vec<u8>>>, SecretError> {
    let table = schema.table(attrs)?;
    unsafe {
        let mut error: *mut GError = std::ptr::null_mut();
        let ptr =
            secret_password_lookupv_sync(&*schema.raw, table.0, std::ptr::null_mut(), &mut error);
        if !error.is_null() {
            if !ptr.is_null() {
                secret_password_free(ptr);
            }
            return Err(take_error(error, "no se pudo leer el secreto"));
        }
        if ptr.is_null() {
            return Ok(None);
        }
        let out = Zeroizing::new(CStr::from_ptr(ptr).to_bytes().to_vec());
        // `secret_password_free` pone a cero la copia de libsecret antes de liberarla.
        secret_password_free(ptr);
        Ok(Some(out))
    }
}

/// Borra todos los secretos que coinciden con los atributos. Devuelve si se borró alguno.
pub fn clear(schema: &Schema, attrs: &[(&str, &str)]) -> Result<bool, SecretError> {
    let table = schema.table(attrs)?;
    unsafe {
        let mut error: *mut GError = std::ptr::null_mut();
        let ok =
            secret_password_clearv_sync(&*schema.raw, table.0, std::ptr::null_mut(), &mut error);
        if !error.is_null() {
            return Err(take_error(error, "no se pudo borrar el secreto"));
        }
        Ok(ok != 0)
    }
}

unsafe extern "C" fn unref_object(object: *mut c_void) {
    g_object_unref(object);
}

/// Lista los atributos (sin secretos) de todos los elementos que coinciden. Con `unlock`, pide
/// al llavero que se desbloquee si hace falta.
pub fn search(
    schema: &Schema,
    attrs: &[(&str, &str)],
    unlock: bool,
) -> Result<Vec<BTreeMap<String, String>>, SecretError> {
    let table = schema.table(attrs)?;
    let mut flags = SECRET_SEARCH_ALL;
    if unlock {
        flags |= SECRET_SEARCH_UNLOCK;
    }
    let names = schema
        .attributes
        .iter()
        .map(|a| (a.to_string_lossy().into_owned(), a.as_ptr()))
        .collect::<Vec<_>>();
    unsafe {
        let mut error: *mut GError = std::ptr::null_mut();
        let list = secret_password_searchv_sync(
            &*schema.raw,
            table.0,
            flags,
            std::ptr::null_mut(),
            &mut error,
        );
        if !error.is_null() {
            if !list.is_null() {
                g_list_free_full(list, unref_object);
            }
            return Err(take_error(error, "no se pudo buscar en el llavero"));
        }
        let mut out = Vec::new();
        let mut node = list;
        while !node.is_null() {
            let item = (*node).data;
            if !item.is_null() {
                let found = secret_retrievable_get_attributes(item);
                if !found.is_null() {
                    let mut map = BTreeMap::new();
                    for (name, c_name) in &names {
                        let value = g_hash_table_lookup(found, (*c_name).cast());
                        if !value.is_null() {
                            map.insert(
                                name.clone(),
                                CStr::from_ptr(value.cast::<c_char>())
                                    .to_string_lossy()
                                    .into_owned(),
                            );
                        }
                    }
                    g_hash_table_unref(found);
                    out.push(map);
                }
            }
            node = (*node).next;
        }
        if !list.is_null() {
            g_list_free_full(list, unref_object);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_layout_matches_libsecret_abi() {
        // SecretSchema en libsecret: puntero + enum + 32 * (puntero + enum) + gint + 7 punteros.
        assert_eq!(std::mem::size_of::<RawSchemaAttribute>(), 16);
        assert_eq!(
            std::mem::size_of::<RawSchema>(),
            8 + 8 + 32 * 16 + 8 + 7 * 8
        );
    }

    #[test]
    fn schema_validates_attributes() {
        let schema = Schema::new("io.example.Test", true, &["origin", "username"]).unwrap();
        assert_eq!(schema.attribute_names(), vec!["origin", "username"]);
        assert!(schema.table(&[("origin", "https://a.example")]).is_ok());
        assert!(schema.table(&[("other", "x")]).is_err());
        assert!(schema.table(&[("origin", "a\0b")]).is_err());
        assert!(Schema::new("x", true, &[]).is_err());
    }

    #[test]
    fn store_rejects_nul_in_secret_before_touching_dbus() {
        let schema = Schema::new("io.example.Test", true, &["origin"]).unwrap();
        let res = store(&schema, &[("origin", "o")], "l", b"a\0b");
        assert!(res.is_err());
        assert!(!res.unwrap_err().message.contains("a\0b"));
    }
}
