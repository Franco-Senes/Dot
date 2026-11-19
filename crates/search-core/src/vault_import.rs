//! Importación de contraseñas (port de `Import.swift` + `ImportFile.swift`).
//!
//! Solo se ejecuta cuando el usuario lo pide y después de enseñarle los perfiles encontrados:
//! - **Firefox** (y derivados Gecko): `logins.json` descifrado con la clave de `key4.db` (NSS).
//!   La entrada de `key4.db` va protegida con PBES2 (PBKDF2-HMAC-SHA256 sobre
//!   `SHA-1(globalSalt ‖ contraseña principal)` + AES-256-CBC) o, en perfiles antiguos, con
//!   PBE-SHA1-3DES. Cada campo de `logins.json` es DER en base64 con AES-256-CBC o 3DES-CBC.
//!   Si hay contraseña principal se pide al usuario.
//! - **Chromium, Chrome, Brave, Edge, Vivaldi**: copia temporal de `Login Data` abierta en solo
//!   lectura. `v10` = clave PBKDF2-HMAC-SHA1(`"peanuts"`, `"saltysalt"`, 1 iteración, 16 bytes);
//!   `v11` = igual pero con la contraseña guardada en el Secret Service («Chrome Safe Storage»).
//!   AES-128-CBC con IV de 16 espacios. Las versiones desconocidas fallan con un mensaje claro
//!   que sugiere importar un CSV.
//! - **CSV** de Chrome, Firefox, Bitwarden o 1Password.
//!
//! Las copias temporales se crean con permisos `0700` (idealmente en `$XDG_RUNTIME_DIR`, que es
//! tmpfs) y se borran al terminar. Ninguna contraseña se escribe en disco ni en los logs.

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use gnome_platform::crypto::{self, Cipher, Digest};
use gnome_platform::sqlite::{Database, Value};
use zeroize::Zeroizing;

use crate::paths;
use crate::vault::{ImportSummary, ImportedLogin, Origin, Password, Vault};

/// Errores de importación. Nunca contienen contraseñas.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImportError {
    #[error("no se pudo leer el perfil: {0}")]
    Unreadable(String),
    #[error("este perfil de Firefox tiene contraseña principal: introdúcela para importar")]
    PrimaryPasswordRequired,
    #[error("la contraseña principal no es correcta")]
    WrongPrimaryPassword,
    #[error(
        "formato de cifrado no reconocido ({0}); exporta las contraseñas a CSV desde el otro \
         navegador e impórtalas desde el archivo"
    )]
    UnsupportedVersion(String),
    #[error("el CSV no tiene columnas de sitio, usuario y contraseña reconocibles")]
    CsvHeader,
}

/// Resultado de leer una fuente: credenciales listas para el llavero, más contadores.
#[derive(Debug, Default)]
pub struct ImportBatch {
    pub logins: Vec<ImportedLogin>,
    /// Entradas que no se pudieron descifrar o no tenían un sitio web válido.
    pub failed: usize,
    /// Sitios marcados como «nunca guardar» en el otro navegador.
    pub never: Vec<Origin>,
    /// Pista para el usuario (p. ej. «exporta a CSV») cuando algo no se pudo leer.
    pub hint: Option<String>,
}

/// Mete un lote en Vault y devuelve el resumen final (importadas, duplicadas, fallidas).
pub fn apply(vault: &Vault, batch: &ImportBatch) -> crate::vault::Result<ImportSummary> {
    let mut summary = vault.import(&batch.logins)?;
    summary.failed += batch.failed;
    for origin in &batch.never {
        vault.never(origin)?;
    }
    Ok(summary)
}

// ---------------------------------------------------------------------------------------------
// Descubrimiento de perfiles
// ---------------------------------------------------------------------------------------------

/// Familia de formato.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileKind {
    /// Directorio con `key4.db` + `logins.json`.
    Firefox,
    /// Archivo `Login Data`; `keyring_app` es el atributo `application` de su clave `v11`.
    Chromium { keyring_app: String },
}

/// Un perfil encontrado de otro navegador.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserProfile {
    /// Nombre del navegador (p. ej. «Firefox», «Brave (Flatpak)»).
    pub browser: String,
    /// Nombre del perfil.
    pub name: String,
    /// Directorio del perfil (Firefox) o ruta de `Login Data` (Chromium).
    pub path: PathBuf,
    pub kind: ProfileKind,
}

const FIREFOX_ROOTS: &[(&str, &str)] = &[
    ("Firefox", ".mozilla/firefox"),
    ("Firefox", ".config/mozilla/firefox"),
    (
        "Firefox (Flatpak)",
        ".var/app/org.mozilla.firefox/.mozilla/firefox",
    ),
    (
        "Firefox (Flatpak)",
        ".var/app/org.mozilla.firefox/config/mozilla/firefox",
    ),
    ("Firefox (Snap)", "snap/firefox/common/.mozilla/firefox"),
    ("LibreWolf", ".librewolf"),
    (
        "LibreWolf (Flatpak)",
        ".var/app/io.gitlab.librewolf-community/.librewolf",
    ),
];

/// (navegador, ruta relativa al `HOME`, atributo `application` de libsecret).
const CHROMIUM_ROOTS: &[(&str, &str, &str)] = &[
    ("Google Chrome", ".config/google-chrome", "chrome"),
    ("Google Chrome Beta", ".config/google-chrome-beta", "chrome"),
    ("Chromium", ".config/chromium", "chromium"),
    ("Brave", ".config/BraveSoftware/Brave-Browser", "brave"),
    ("Microsoft Edge", ".config/microsoft-edge", "chromium"),
    ("Vivaldi", ".config/vivaldi", "chrome"),
    (
        "Google Chrome (Flatpak)",
        ".var/app/com.google.Chrome/config/google-chrome",
        "chrome",
    ),
    (
        "Chromium (Flatpak)",
        ".var/app/org.chromium.Chromium/config/chromium",
        "chromium",
    ),
    (
        "Brave (Flatpak)",
        ".var/app/com.brave.Browser/config/BraveSoftware/Brave-Browser",
        "brave",
    ),
    (
        "Microsoft Edge (Flatpak)",
        ".var/app/com.microsoft.Edge/config/microsoft-edge",
        "chromium",
    ),
    (
        "Vivaldi (Flatpak)",
        ".var/app/com.vivaldi.Vivaldi/config/vivaldi",
        "chrome",
    ),
    (
        "Chromium (Snap)",
        "snap/chromium/common/chromium",
        "chromium",
    ),
];

/// Busca perfiles con contraseñas bajo `home`. Solo comprueba la existencia de archivos: no
/// abre ni descifra nada (eso ocurre únicamente cuando el usuario elige importar).
pub fn discover_profiles(home: &Path) -> Vec<BrowserProfile> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for (browser, rel) in FIREFOX_ROOTS {
        let root = home.join(rel);
        for (name, dir) in firefox_profiles(&root) {
            if dir.join("key4.db").is_file()
                && dir.join("logins.json").is_file()
                && seen.insert(dir.clone())
            {
                out.push(BrowserProfile {
                    browser: (*browser).to_owned(),
                    name,
                    path: dir,
                    kind: ProfileKind::Firefox,
                });
            }
        }
    }
    for (browser, rel, app) in CHROMIUM_ROOTS {
        let root = home.join(rel);
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        let mut dirs: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n == "Default" || n.starts_with("Profile "))
            })
            .collect();
        dirs.sort();
        for dir in dirs {
            let file = dir.join("Login Data");
            if file.is_file() && seen.insert(file.clone()) {
                out.push(BrowserProfile {
                    browser: (*browser).to_owned(),
                    name: dir
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    path: file,
                    kind: ProfileKind::Chromium {
                        keyring_app: (*app).to_owned(),
                    },
                });
            }
        }
    }
    out
}

/// Perfiles listados en `profiles.ini` (`[ProfileN]` con `Name`, `Path`, `IsRelative`).
fn firefox_profiles(root: &Path) -> Vec<(String, PathBuf)> {
    let Ok(text) = fs::read_to_string(root.join("profiles.ini")) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut in_profile = false;
    let (mut name, mut path, mut relative) = (String::new(), String::new(), true);
    let mut flush = |name: &mut String, path: &mut String, relative: &mut bool| {
        if !path.is_empty() {
            let p = Path::new(path.as_str());
            let full = if *relative || !p.is_absolute() {
                root.join(p)
            } else {
                p.to_path_buf()
            };
            let shown = if name.is_empty() {
                path.clone()
            } else {
                name.clone()
            };
            out.push((shown, full));
        }
        name.clear();
        path.clear();
        *relative = true;
    };
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            if in_profile {
                flush(&mut name, &mut path, &mut relative);
            }
            in_profile = line.starts_with("[Profile");
            continue;
        }
        if !in_profile {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            match k.trim() {
                "Name" => name = v.trim().to_owned(),
                "Path" => path = v.trim().to_owned(),
                "IsRelative" => relative = v.trim() != "0",
                _ => {}
            }
        }
    }
    if in_profile {
        flush(&mut name, &mut path, &mut relative);
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Copias temporales
// ---------------------------------------------------------------------------------------------

static TEMP_SEQ: AtomicU64 = AtomicU64::new(1);

/// Copia de una base de datos en un directorio `0700` que se borra al soltarse.
struct TempCopy {
    dir: PathBuf,
    file: PathBuf,
}

impl TempCopy {
    fn new(temp_root: &Path, source: &Path) -> Result<Self, ImportError> {
        let unique = format!(
            "vault-import-{}-{}",
            std::process::id(),
            TEMP_SEQ.fetch_add(1, Ordering::Relaxed)
        );
        let dir = temp_root.join(unique);
        paths::create_dir_0700(&dir).map_err(|e| ImportError::Unreadable(e.to_string()))?;
        let guard = TempCopy {
            file: dir.join("copy.db"),
            dir,
        };
        fs::copy(source, &guard.file).map_err(|e| {
            ImportError::Unreadable(format!("no se pudo copiar {}: {e}", source.display()))
        })?;
        // Un navegador abierto puede tener cambios aún en el WAL o en el journal.
        for suffix in ["-wal", "-journal"] {
            let mut side = source.as_os_str().to_owned();
            side.push(suffix);
            let side = PathBuf::from(side);
            if side.is_file() {
                let mut dest = guard.file.as_os_str().to_owned();
                dest.push(suffix);
                let _ = fs::copy(&side, PathBuf::from(dest));
            }
        }
        Ok(guard)
    }
}

impl Drop for TempCopy {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn db_err(e: gnome_platform::sqlite::SqliteError) -> ImportError {
    ImportError::Unreadable(e.to_string())
}

// ---------------------------------------------------------------------------------------------
// DER mínimo (lo justo para las estructuras de NSS)
// ---------------------------------------------------------------------------------------------

struct Tlv<'a> {
    tag: u8,
    body: &'a [u8],
    next: usize,
}

fn tlv(b: &[u8], i: usize) -> Option<Tlv<'_>> {
    let tag = *b.get(i)?;
    let mut j = i + 1;
    let first = *b.get(j)? as usize;
    j += 1;
    let len = if first & 0x80 != 0 {
        let n = first & 0x7f;
        // Como mucho cuatro bytes de longitud: nada en key4.db es mayor.
        if n == 0 || n > 4 || j + n > b.len() {
            return None;
        }
        let mut len = 0usize;
        for k in 0..n {
            len = (len << 8) | b[j + k] as usize;
        }
        j += n;
        len
    } else {
        first
    };
    let end = j.checked_add(len)?;
    if end > b.len() {
        return None;
    }
    Some(Tlv {
        tag,
        body: &b[j..end],
        next: end,
    })
}

fn items(body: &[u8]) -> Vec<Tlv<'_>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < body.len() {
        match tlv(body, i) {
            Some(t) => {
                i = t.next;
                out.push(t);
            }
            None => break,
        }
    }
    out
}

fn oid(body: &[u8]) -> String {
    let Some(first) = body.first() else {
        return String::new();
    };
    let mut parts = vec![u64::from(*first / 40), u64::from(*first % 40)];
    let mut value: u64 = 0;
    for byte in &body[1..] {
        value = (value << 7) | u64::from(byte & 0x7f);
        if byte & 0x80 == 0 {
            parts.push(value);
            value = 0;
        }
    }
    parts
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(".")
}

fn der_uint(body: &[u8]) -> Option<u64> {
    if body.is_empty() || body.len() > 8 {
        return None;
    }
    Some(body.iter().fold(0u64, |acc, b| (acc << 8) | u64::from(*b)))
}

const OID_PBES2: &str = "1.2.840.113549.1.5.13";
const OID_PBE_SHA1_3DES: &str = "1.2.840.113549.1.12.5.1.3";
const OID_AES256_CBC: &str = "2.16.840.1.101.3.4.1.42";
const OID_DES_EDE3_CBC: &str = "1.2.840.113549.3.7";
const PASSWORD_CHECK: &[u8] = b"password-check";

// ---------------------------------------------------------------------------------------------
// Base64 (estándar, con relleno)
// ---------------------------------------------------------------------------------------------

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_decode(text: &str) -> Option<Zeroizing<Vec<u8>>> {
    let mut out = Zeroizing::new(Vec::with_capacity(text.len() * 3 / 4));
    let mut acc: u32 = 0;
    let mut bits = 0;
    let mut padding = 0;
    for c in text.bytes() {
        if c.is_ascii_whitespace() {
            continue;
        }
        if c == b'=' {
            padding += 1;
            continue;
        }
        if padding > 0 {
            return None;
        }
        let v = B64.iter().position(|&x| x == c)? as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    if padding > 2 {
        return None;
    }
    Some(out)
}

#[cfg(test)]
fn base64_encode(data: &[u8]) -> String {
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(B64[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Firefox (NSS)
// ---------------------------------------------------------------------------------------------

/// Desenvuelve una entrada de `key4.db` (la comprobación o la clave). `prehash` es el hash con
/// el que NSS combina `globalSalt ‖ contraseña` antes de PBKDF2 (SHA-1 en NSS; se prueba también
/// SHA-256 por compatibilidad con la implementación del original).
fn nss_unwrap(
    blob: &[u8],
    global_salt: &[u8],
    password: &[u8],
    prehash: Digest,
) -> Option<Zeroizing<Vec<u8>>> {
    let outer = tlv(blob, 0)?;
    let top = items(outer.body);
    if top.len() != 2 {
        return None;
    }
    let algo = items(top[0].body);
    if algo.len() != 2 {
        return None;
    }
    let algo_oid = oid(algo[0].body);
    let ciphertext = top[1].body;
    let mut salted = Zeroizing::new(Vec::with_capacity(global_salt.len() + password.len()));
    salted.extend_from_slice(global_salt);
    salted.extend_from_slice(password);

    if algo_oid == OID_PBES2 {
        let params = items(algo[1].body);
        if params.len() != 2 {
            return None;
        }
        let kdf = items(params[0].body);
        if kdf.len() != 2 {
            return None;
        }
        let kp = items(kdf[1].body);
        if kp.len() < 2 {
            return None;
        }
        let entry_salt = kp[0].body;
        let rounds = der_uint(kp[1].body)?;
        // Un archivo corrupto podría pedir cualquier cosa; NSS escribe 10 000.
        if entry_salt.is_empty() || !(1..=10_000_000).contains(&rounds) {
            return None;
        }
        let key_len = kp
            .get(2)
            .filter(|t| t.tag == 0x02)
            .and_then(|t| der_uint(t.body))
            .unwrap_or(32);
        if key_len != 32 {
            return None;
        }
        let enc = items(params[1].body);
        if enc.len() != 2 || oid(enc[0].body) != OID_AES256_CBC {
            return None;
        }
        let ck = crypto::hash(prehash, &salted).ok()?;
        let key = crypto::pbkdf2(Digest::Sha256, &ck, entry_salt, rounds as u32, 32).ok()?;
        let iv = nss_iv(enc[1].body)?;
        return crypto::cbc_decrypt(Cipher::Aes256Cbc, &key, &iv, ciphertext).ok();
    }
    if algo_oid == OID_PBE_SHA1_3DES {
        let params = items(algo[1].body);
        let entry_salt = params.first()?.body;
        let hp = crypto::hash(Digest::Sha1, &salted).ok()?;
        let mut hp_salt = Zeroizing::new(hp.to_vec());
        hp_salt.extend_from_slice(entry_salt);
        let chp = crypto::hash(Digest::Sha1, &hp_salt).ok()?;
        let mut pes = entry_salt.to_vec();
        pes.resize(20, 0);
        let mut pes_salt = pes.clone();
        pes_salt.extend_from_slice(entry_salt);
        let k1 = crypto::hmac(Digest::Sha1, &chp, &pes_salt).ok()?;
        let tk = crypto::hmac(Digest::Sha1, &chp, &pes).ok()?;
        let mut tk_salt = Zeroizing::new(tk.to_vec());
        tk_salt.extend_from_slice(entry_salt);
        let k2 = crypto::hmac(Digest::Sha1, &chp, &tk_salt).ok()?;
        let mut k = Zeroizing::new(k1.to_vec());
        k.extend_from_slice(&k2);
        return crypto::cbc_decrypt(
            Cipher::TripleDesCbc,
            &k[..24],
            &k[k.len() - 8..],
            ciphertext,
        )
        .ok();
    }
    None
}

/// NSS guarda 14 bytes del IV de AES tras una cabecera DER de OCTET STRING (`04 0e`).
fn nss_iv(stored: &[u8]) -> Option<Vec<u8>> {
    match stored.len() {
        14 => {
            let mut iv = vec![0x04, 0x0e];
            iv.extend_from_slice(stored);
            Some(iv)
        }
        16 => Some(stored.to_vec()),
        _ => None,
    }
}

/// Clave maestra de un perfil (de `nssPrivate`), comprobando antes la contraseña principal con
/// el registro `password-check` de `metaData`.
fn firefox_master_key(
    key4: &Path,
    primary_password: Option<&str>,
    temp_root: &Path,
) -> Result<Zeroizing<Vec<u8>>, ImportError> {
    let copy = TempCopy::new(temp_root, key4)?;
    let db = Database::open_read_only(&copy.file).map_err(db_err)?;
    let meta = db
        .query(
            "SELECT item1, item2 FROM metaData WHERE id = 'password'",
            &[],
        )
        .map_err(db_err)?;
    let row = meta
        .first()
        .ok_or_else(|| ImportError::Unreadable("key4.db sin metaData".into()))?;
    let global_salt = row.first().map(Value::as_bytes).unwrap_or(&[]);
    let check = row.get(1).map(Value::as_bytes).unwrap_or(&[]);
    if global_salt.is_empty() || check.is_empty() {
        return Err(ImportError::Unreadable("key4.db incompleto".into()));
    }
    let password = primary_password.unwrap_or("").as_bytes();
    let prehash = [Digest::Sha1, Digest::Sha256]
        .into_iter()
        .find(|d| {
            nss_unwrap(check, global_salt, password, *d)
                .is_some_and(|plain| plain.starts_with(PASSWORD_CHECK))
        })
        .ok_or(if primary_password.is_some_and(|p| !p.is_empty()) {
            ImportError::WrongPrimaryPassword
        } else {
            ImportError::PrimaryPasswordRequired
        })?;

    let wanted_id: Vec<u8> = std::iter::once(0xf8)
        .chain(std::iter::repeat_n(0, 14))
        .chain(std::iter::once(0x01))
        .collect();
    let rows = db
        .query("SELECT a11, a102 FROM nssPrivate", &[])
        .map_err(db_err)?;
    let a11 = rows
        .iter()
        .find(|r| r.get(1).map(Value::as_bytes) == Some(&wanted_id[..]))
        .and_then(|r| r.first())
        .map(Value::as_bytes)
        .ok_or_else(|| ImportError::Unreadable("key4.db sin clave privada".into()))?;
    nss_unwrap(a11, global_salt, password, prehash)
        .ok_or_else(|| ImportError::UnsupportedVersion("clave de key4.db".into()))
}

/// Descifra un campo de `logins.json`.
fn firefox_decrypt_field(b64: &str, master: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
    let blob = base64_decode(b64)?;
    let outer = tlv(&blob, 0)?;
    let parts = items(outer.body);
    if parts.len() != 3 {
        return None;
    }
    let cipher = items(parts[1].body);
    if cipher.len() != 2 {
        return None;
    }
    let ct = parts[2].body;
    match oid(cipher[0].body).as_str() {
        OID_AES256_CBC => {
            let iv = nss_iv(cipher[1].body)?;
            crypto::cbc_decrypt(Cipher::Aes256Cbc, master.get(..32)?, &iv, ct).ok()
        }
        OID_DES_EDE3_CBC => {
            crypto::cbc_decrypt(Cipher::TripleDesCbc, master.get(..24)?, cipher[1].body, ct).ok()
        }
        _ => None,
    }
}

/// Importa un perfil de Firefox (directorio con `key4.db` y `logins.json`).
pub fn import_firefox(
    profile_dir: &Path,
    primary_password: Option<&str>,
    temp_root: &Path,
) -> Result<ImportBatch, ImportError> {
    let master = firefox_master_key(&profile_dir.join("key4.db"), primary_password, temp_root)?;
    let text = Zeroizing::new(
        fs::read_to_string(profile_dir.join("logins.json"))
            .map_err(|e| ImportError::Unreadable(format!("logins.json: {e}")))?,
    );
    let doc: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| ImportError::Unreadable(format!("logins.json: {e}")))?;
    let mut batch = ImportBatch::default();
    if let Some(hosts) = doc.get("disabledHosts").and_then(|v| v.as_array()) {
        batch.never = hosts
            .iter()
            .filter_map(|h| h.as_str())
            .filter_map(|h| Origin::parse_loose(h).ok())
            .collect();
    }
    let mut seen = BTreeSet::new();
    for entry in doc
        .get("logins")
        .and_then(|v| v.as_array())
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        let Some(origin) = entry
            .get("hostname")
            .and_then(|v| v.as_str())
            .and_then(|h| Origin::parse(h).ok())
        else {
            batch.failed += 1;
            continue;
        };
        let field = |name: &str| {
            entry
                .get(name)
                .and_then(|v| v.as_str())
                .and_then(|b64| firefox_decrypt_field(b64, &master))
        };
        let (Some(user), Some(pass)) = (field("encryptedUsername"), field("encryptedPassword"))
        else {
            batch.failed += 1;
            continue;
        };
        let username = String::from_utf8_lossy(&user).into_owned();
        let Ok(password) = Password::from_utf8(pass) else {
            batch.failed += 1;
            continue;
        };
        if password.is_empty() || !seen.insert((origin.clone(), username.clone())) {
            continue;
        }
        let millis = |name: &str| {
            entry
                .get(name)
                .and_then(|v| v.as_i64())
                .filter(|t| *t > 0)
                .map(|t| t / 1000)
        };
        batch.logins.push(ImportedLogin {
            origin,
            username,
            password,
            created: millis("timeCreated"),
            last_used: millis("timeLastUsed"),
        });
    }
    Ok(batch)
}

// ---------------------------------------------------------------------------------------------
// Chromium
// ---------------------------------------------------------------------------------------------

/// Claves para descifrar `Login Data`.
pub struct ChromiumKeys {
    /// Contraseña de «Safe Storage» del Secret Service para `v11` (si se encontró).
    pub v11_secret: Option<Password>,
}

impl fmt::Debug for ChromiumKeys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChromiumKeys")
            .field("v11_secret", &self.v11_secret.as_ref().map(|_| "<oculta>"))
            .finish()
    }
}

const CHROMIUM_SALT: &[u8] = b"saltysalt";
const CHROMIUM_V10_PASSWORD: &[u8] = b"peanuts";
const CHROMIUM_IV: [u8; 16] = [b' '; 16];
/// Microsegundos entre 1601-01-01 y 1970-01-01.
const CHROMIUM_EPOCH_OFFSET_SECS: i64 = 11_644_473_600;

/// Clave AES-128 de Chromium en Linux: PBKDF2-HMAC-SHA1(pass, "saltysalt", 1, 16).
pub fn chromium_key(passphrase: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
    crypto::pbkdf2(Digest::Sha1, passphrase, CHROMIUM_SALT, 1, 16).ok()
}

fn chromium_time(micros: i64) -> Option<i64> {
    (micros > 0).then(|| micros / 1_000_000 - CHROMIUM_EPOCH_OFFSET_SECS)
}

enum ChromiumPlain {
    Ok(Password),
    Undecryptable,
    Unsupported(String),
}

fn chromium_decrypt(blob: &[u8], v10: &[u8], v11: &[Zeroizing<Vec<u8>>]) -> ChromiumPlain {
    if blob.len() <= 3 {
        return ChromiumPlain::Undecryptable;
    }
    let (prefix, body) = blob.split_at(3);
    let keys: Vec<&[u8]> = match prefix {
        b"v10" => vec![v10],
        b"v11" => v11.iter().map(|k| k.as_slice()).collect(),
        // Un perfil puede tener texto en claro escrito por cualquier programa: no se acepta
        // (igual que el original). Otras versiones (p. ej. `v20` de app-bound encryption) no
        // existen en Linux a fecha de hoy; se informa con claridad.
        _ => {
            let shown: String = prefix
                .iter()
                .map(|b| {
                    if b.is_ascii_alphanumeric() {
                        *b as char
                    } else {
                        '?'
                    }
                })
                .collect();
            return ChromiumPlain::Unsupported(shown);
        }
    };
    for key in keys {
        let Ok(plain) = crypto::cbc_decrypt(Cipher::Aes128Cbc, key, &CHROMIUM_IV, body) else {
            continue;
        };
        if std::str::from_utf8(&plain).is_ok() {
            if let Ok(p) = Password::from_utf8(plain) {
                return ChromiumPlain::Ok(p);
            }
            continue;
        }
        // Las versiones nuevas anteponen SHA-256 del dominio (32 bytes) al valor.
        if plain.len() > 32 {
            if let Ok(p) = Password::from_utf8(Zeroizing::new(plain[32..].to_vec())) {
                return ChromiumPlain::Ok(p);
            }
        }
    }
    ChromiumPlain::Undecryptable
}

/// Importa un `Login Data` de Chromium/Chrome/Brave/Edge/Vivaldi.
pub fn import_chromium(
    login_data: &Path,
    keys: &ChromiumKeys,
    temp_root: &Path,
) -> Result<ImportBatch, ImportError> {
    let copy = TempCopy::new(temp_root, login_data)?;
    let db = Database::open_read_only(&copy.file).map_err(db_err)?;
    let rows = db
        .query(
            "SELECT origin_url, username_value, password_value, blacklisted_by_user, \
             date_created, date_last_used FROM logins",
            &[],
        )
        .or_else(|_| {
            // Esquemas antiguos sin `date_last_used`.
            db.query(
                "SELECT origin_url, username_value, password_value, blacklisted_by_user, \
                 date_created, 0 FROM logins",
                &[],
            )
        })
        .map_err(db_err)?;

    let v10 = chromium_key(CHROMIUM_V10_PASSWORD)
        .ok_or_else(|| ImportError::Unreadable("libcrypto".into()))?;
    let mut v11 = Vec::new();
    if let Some(secret) = &keys.v11_secret {
        if let Some(k) = chromium_key(secret.expose().as_bytes()) {
            v11.push(k);
        }
    }
    // Chromium también acepta `v11` cifrado con la contraseña vacía (llaveros que devolvieron "").
    if let Some(k) = chromium_key(b"") {
        v11.push(k);
    }

    let mut batch = ImportBatch::default();
    let mut seen = BTreeSet::new();
    let mut unsupported: Option<String> = None;
    let mut missing_v11 = false;
    for row in &rows {
        let origin_raw = row.first().map(Value::as_text).unwrap_or("");
        let blacklisted = row.get(3).map(Value::as_i64).unwrap_or(0) != 0;
        let origin = Origin::parse(origin_raw);
        if blacklisted {
            if let Ok(o) = origin {
                batch.never.push(o);
            }
            continue;
        }
        let blob = row.get(2).map(Value::as_bytes).unwrap_or(&[]);
        if blob.is_empty() {
            continue;
        }
        let Ok(origin) = origin else {
            batch.failed += 1;
            continue;
        };
        let username = row.get(1).map(Value::as_text).unwrap_or("").to_owned();
        match chromium_decrypt(blob, &v10, &v11) {
            ChromiumPlain::Ok(password) => {
                if password.is_empty() || !seen.insert((origin.clone(), username.clone())) {
                    continue;
                }
                batch.logins.push(ImportedLogin {
                    origin,
                    username,
                    password,
                    created: chromium_time(row.get(4).map(Value::as_i64).unwrap_or(0)),
                    last_used: chromium_time(row.get(5).map(Value::as_i64).unwrap_or(0)),
                });
            }
            ChromiumPlain::Undecryptable => {
                if blob.starts_with(b"v11") && keys.v11_secret.is_none() {
                    missing_v11 = true;
                }
                batch.failed += 1;
            }
            ChromiumPlain::Unsupported(v) => {
                unsupported.get_or_insert(v);
                batch.failed += 1;
            }
        }
    }
    batch.never.sort();
    batch.never.dedup();
    if let Some(v) = unsupported {
        if batch.logins.is_empty() {
            return Err(ImportError::UnsupportedVersion(format!("versión «{v}»")));
        }
        batch.hint = Some(format!(
            "Algunas contraseñas usan un formato no reconocido («{v}»). Exporta un CSV desde el \
             otro navegador para traerlas."
        ));
    } else if missing_v11 {
        batch.hint = Some(
            "Algunas contraseñas necesitan la clave «Safe Storage» del llavero, que no se \
             encontró. Abre el llavero o importa un CSV exportado desde el otro navegador."
                .into(),
        );
    }
    Ok(batch)
}

/// Busca en el Secret Service la contraseña «Safe Storage» con la que Chromium cifra `v11`
/// (esquemas `chrome_libsecret_os_crypt_password_v2` y `_v1`, atributo `application`).
/// Puede pedir desbloquear el llavero. Nunca se registra el valor.
pub fn chromium_safe_storage_secret(keyring_app: &str) -> Option<Password> {
    for schema_name in [
        "chrome_libsecret_os_crypt_password_v2",
        "chrome_libsecret_os_crypt_password_v1",
    ] {
        let Ok(schema) = gnome_platform::secret::Schema::new(schema_name, false, &["application"])
        else {
            continue;
        };
        match gnome_platform::secret::lookup(&schema, &[("application", keyring_app)]) {
            Ok(Some(bytes)) => {
                if let Ok(p) = Password::from_utf8(bytes) {
                    return Some(p);
                }
            }
            Ok(None) => {}
            Err(e) => crate::debug!("Vault: sin clave Safe Storage en {schema_name}: {e}"),
        }
    }
    None
}

// ---------------------------------------------------------------------------------------------
// CSV
// ---------------------------------------------------------------------------------------------

/// Campos entre comillas, comillas dobladas dentro y saltos de línea dentro: los tres aparecen
/// en exportaciones reales.
pub fn parse_csv(text: &str) -> Vec<Vec<Zeroizing<String>>> {
    let mut rows = Vec::new();
    let mut row: Vec<Zeroizing<String>> = Vec::new();
    let mut field = Zeroizing::new(String::new());
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    let finish_row = |row: &mut Vec<Zeroizing<String>>, rows: &mut Vec<Vec<Zeroizing<String>>>| {
        if row.iter().any(|f| !f.is_empty()) {
            rows.push(std::mem::take(row));
        } else {
            row.clear();
        }
    };
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' => quoted = true,
            ',' => row.push(std::mem::replace(&mut field, Zeroizing::new(String::new()))),
            '\r' | '\n' => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                row.push(std::mem::replace(&mut field, Zeroizing::new(String::new())));
                finish_row(&mut row, &mut rows);
            }
            _ => field.push(c),
        }
    }
    row.push(field);
    finish_row(&mut row, &mut rows);
    rows
}

/// Importa un CSV exportado de Chrome, Firefox, Bitwarden o 1Password.
pub fn import_csv(text: &str) -> Result<ImportBatch, ImportError> {
    // Quita la marca BOM que añaden algunos exportadores.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut rows = parse_csv(text).into_iter();
    let header: Vec<String> = rows
        .next()
        .ok_or(ImportError::CsvHeader)?
        .iter()
        .map(|h| h.trim().to_lowercase())
        .collect();
    let column = |names: &[&str]| header.iter().position(|h| names.contains(&h.as_str()));
    let url_at = column(&["url", "login_uri", "website", "site", "origin"]);
    let user_at = column(&["username", "login_username", "user", "email", "login"]);
    let pass_at = column(&["password", "login_password"]);
    let (Some(url_at), Some(user_at), Some(pass_at)) = (url_at, user_at, pass_at) else {
        return Err(ImportError::CsvHeader);
    };
    let mut batch = ImportBatch::default();
    let mut seen = BTreeSet::new();
    for row in rows {
        let (Some(url), Some(user), Some(pass)) =
            (row.get(url_at), row.get(user_at), row.get(pass_at))
        else {
            batch.failed += 1;
            continue;
        };
        let Ok(origin) = Origin::parse_loose(url) else {
            batch.failed += 1;
            continue;
        };
        if pass.is_empty() {
            batch.failed += 1;
            continue;
        }
        let username = user.trim().to_owned();
        if !seen.insert((origin.clone(), username.clone())) {
            continue;
        }
        batch.logins.push(ImportedLogin {
            origin,
            username,
            password: Password::new(pass.to_string()),
            created: None,
            last_used: None,
        });
    }
    Ok(batch)
}

#[cfg(test)]
mod tests {
    //! Todos los datos de estas pruebas son **sintéticos** y se generan en directorios
    //! temporales propios. Nunca se lee un perfil real (`~/.mozilla`, `~/.config`, …).

    use super::*;
    use crate::vault::MemoryStore;

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "search-vault-import-{tag}-{}-{}",
                std::process::id(),
                TEMP_SEQ.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            TestDir(dir)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn rand(n: usize) -> Vec<u8> {
        let mut v = vec![0u8; n];
        crypto::random_bytes(&mut v).unwrap();
        v
    }

    // --- Codificador DER mínimo para fixtures ---
    fn der(tag: u8, body: &[u8]) -> Vec<u8> {
        let mut out = vec![tag];
        let len = body.len();
        if len < 0x80 {
            out.push(len as u8);
        } else if len < 0x100 {
            out.extend([0x81, len as u8]);
        } else {
            out.extend([0x82, (len >> 8) as u8, len as u8]);
        }
        out.extend_from_slice(body);
        out
    }
    fn seq(parts: &[Vec<u8>]) -> Vec<u8> {
        der(0x30, &parts.concat())
    }
    fn octets(b: &[u8]) -> Vec<u8> {
        der(0x04, b)
    }
    fn int(n: u64) -> Vec<u8> {
        let mut bytes: Vec<u8> = n
            .to_be_bytes()
            .into_iter()
            .skip_while(|b| *b == 0)
            .collect();
        if bytes.is_empty() || bytes[0] & 0x80 != 0 {
            bytes.insert(0, 0);
        }
        der(0x02, &bytes)
    }
    fn oid_der(dotted: &str) -> Vec<u8> {
        let nums: Vec<u64> = dotted.split('.').map(|p| p.parse().unwrap()).collect();
        let mut body = vec![(nums[0] * 40 + nums[1]) as u8];
        for &n in &nums[2..] {
            let mut stack = vec![(n & 0x7f) as u8];
            let mut v = n >> 7;
            while v > 0 {
                stack.push(((v & 0x7f) as u8) | 0x80);
                v >>= 7;
            }
            stack.reverse();
            body.extend(stack);
        }
        der(0x06, &body)
    }

    /// Envuelve `plain` como lo hace NSS moderno (PBES2 + PBKDF2-SHA256 + AES-256-CBC).
    fn pbes2_wrap(plain: &[u8], global_salt: &[u8], password: &[u8]) -> Vec<u8> {
        let entry_salt = rand(32);
        let rounds = 1000;
        let mut salted = global_salt.to_vec();
        salted.extend_from_slice(password);
        let ck = crypto::hash(Digest::Sha1, &salted).unwrap();
        let key = crypto::pbkdf2(Digest::Sha256, &ck, &entry_salt, rounds, 32).unwrap();
        let iv14 = rand(14);
        let mut iv = vec![0x04, 0x0e];
        iv.extend_from_slice(&iv14);
        let ct = crypto::cbc_encrypt(Cipher::Aes256Cbc, &key, &iv, plain).unwrap();
        seq(&[
            seq(&[
                oid_der(OID_PBES2),
                seq(&[
                    seq(&[
                        oid_der("1.2.840.113549.1.5.12"),
                        seq(&[
                            octets(&entry_salt),
                            int(u64::from(rounds)),
                            int(32),
                            seq(&[oid_der("1.2.840.113549.2.9")]),
                        ]),
                    ]),
                    seq(&[oid_der(OID_AES256_CBC), octets(&iv14)]),
                ]),
            ]),
            octets(&ct),
        ])
    }

    /// Envuelve `plain` como NSS antiguo (PBE-SHA1-3DES).
    fn pbe_sha1_3des_wrap(plain: &[u8], global_salt: &[u8], password: &[u8]) -> Vec<u8> {
        let entry_salt = rand(20);
        let mut salted = global_salt.to_vec();
        salted.extend_from_slice(password);
        let hp = crypto::hash(Digest::Sha1, &salted).unwrap();
        let mut hs = hp.to_vec();
        hs.extend_from_slice(&entry_salt);
        let chp = crypto::hash(Digest::Sha1, &hs).unwrap();
        let mut pes = entry_salt.clone();
        pes.resize(20, 0);
        let k1 = crypto::hmac(
            Digest::Sha1,
            &chp,
            &[pes.clone(), entry_salt.clone()].concat(),
        )
        .unwrap();
        let tk = crypto::hmac(Digest::Sha1, &chp, &pes).unwrap();
        let k2 = crypto::hmac(
            Digest::Sha1,
            &chp,
            &[tk.to_vec(), entry_salt.clone()].concat(),
        )
        .unwrap();
        let k = [k1.to_vec(), k2.to_vec()].concat();
        let ct =
            crypto::cbc_encrypt(Cipher::TripleDesCbc, &k[..24], &k[k.len() - 8..], plain).unwrap();
        seq(&[
            seq(&[
                oid_der(OID_PBE_SHA1_3DES),
                seq(&[octets(&entry_salt), int(1)]),
            ]),
            octets(&ct),
        ])
    }

    fn login_field_aes(value: &str, master: &[u8]) -> String {
        let iv = rand(16);
        let ct =
            crypto::cbc_encrypt(Cipher::Aes256Cbc, &master[..32], &iv, value.as_bytes()).unwrap();
        base64_encode(&seq(&[
            octets(&rand(16)),
            seq(&[oid_der(OID_AES256_CBC), octets(&iv)]),
            octets(&ct),
        ]))
    }

    fn login_field_3des(value: &str, master: &[u8]) -> String {
        let iv = rand(8);
        let ct = crypto::cbc_encrypt(Cipher::TripleDesCbc, &master[..24], &iv, value.as_bytes())
            .unwrap();
        base64_encode(&seq(&[
            octets(&rand(16)),
            seq(&[oid_der(OID_DES_EDE3_CBC), octets(&iv)]),
            octets(&ct),
        ]))
    }

    #[derive(Clone, Copy)]
    enum Nss {
        Modern,
        Legacy,
    }

    /// Crea un perfil de Firefox sintético con `key4.db` + `logins.json`.
    fn make_firefox_profile(dir: &Path, primary: &str, nss: Nss) {
        fs::create_dir_all(dir).unwrap();
        let global_salt = rand(20);
        let mut master = rand(32);
        if let Nss::Legacy = nss {
            master.truncate(24);
        }
        let wrap = |plain: &[u8]| match nss {
            Nss::Modern => pbes2_wrap(plain, &global_salt, primary.as_bytes()),
            Nss::Legacy => pbe_sha1_3des_wrap(plain, &global_salt, primary.as_bytes()),
        };
        let db = Database::open_read_write(&dir.join("key4.db")).unwrap();
        db.execute(
            "CREATE TABLE metaData (id PRIMARY KEY UNIQUE ON CONFLICT REPLACE, item1, item2)",
            &[],
        )
        .unwrap();
        db.execute(
            "INSERT INTO metaData VALUES ('password', ?, ?)",
            &[
                Value::blob(&global_salt),
                Value::blob(&wrap(b"password-check\x02\x02")),
            ],
        )
        .unwrap();
        db.execute("CREATE TABLE nssPrivate (id PRIMARY KEY, a11, a102)", &[])
            .unwrap();
        db.execute(
            "INSERT INTO nssPrivate VALUES (1, ?, ?)",
            &[Value::blob(&wrap(b"otra-clave")), Value::blob(b"distinto")],
        )
        .unwrap();
        let mut id = vec![0xf8];
        id.extend([0u8; 14]);
        id.push(0x01);
        db.execute(
            "INSERT INTO nssPrivate VALUES (2, ?, ?)",
            &[Value::blob(&wrap(&master)), Value::blob(&id)],
        )
        .unwrap();
        drop(db);
        let field = |v: &str| match nss {
            Nss::Modern => login_field_aes(v, &master),
            Nss::Legacy => login_field_3des(v, &master),
        };
        let logins = serde_json::json!({
            "nextId": 4,
            "logins": [
                {
                    "id": 1,
                    "hostname": "https://synthetic-mail.test",
                    "encryptedUsername": field("ana@synthetic.test"),
                    "encryptedPassword": field("Sintética-1!"),
                    "timeCreated": 1_700_000_000_000_i64,
                    "timeLastUsed": 1_700_000_500_000_i64
                },
                {
                    "id": 2,
                    "hostname": "http://plain.synthetic.test:8080",
                    "encryptedUsername": field(""),
                    "encryptedPassword": field("solo-codigo")
                },
                {
                    "id": 3,
                    "hostname": "chrome://FirefoxAccounts",
                    "encryptedUsername": field("x"),
                    "encryptedPassword": field("y")
                },
                {
                    "id": 4,
                    "hostname": "https://broken.synthetic.test",
                    "encryptedUsername": "!!!no-base64!!!",
                    "encryptedPassword": field("z")
                }
            ],
            "disabledHosts": ["https://never.synthetic.test"]
        });
        fs::write(dir.join("logins.json"), logins.to_string()).unwrap();
    }

    fn check_firefox_batch(batch: &ImportBatch) {
        assert_eq!(batch.logins.len(), 2);
        let a = &batch.logins[0];
        assert_eq!(a.origin.to_string(), "https://synthetic-mail.test");
        assert_eq!(a.username, "ana@synthetic.test");
        assert_eq!(a.password.expose(), "Sintética-1!");
        assert_eq!(a.created, Some(1_700_000_000));
        assert_eq!(a.last_used, Some(1_700_000_500));
        let b = &batch.logins[1];
        assert_eq!(b.origin.to_string(), "http://plain.synthetic.test:8080");
        assert_eq!(b.username, "");
        assert_eq!(b.password.expose(), "solo-codigo");
        assert_eq!(batch.failed, 2);
        assert_eq!(batch.never.len(), 1);
    }

    #[test]
    fn firefox_modern_pbes2_without_primary_password() {
        let t = TestDir::new("ff-modern");
        let profile = t.0.join("profile");
        make_firefox_profile(&profile, "", Nss::Modern);
        let batch = import_firefox(&profile, None, &t.0).unwrap();
        check_firefox_batch(&batch);
        // La copia temporal se borró.
        let leftovers: Vec<_> = fs::read_dir(&t.0)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("vault-import-"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn firefox_legacy_3des() {
        let t = TestDir::new("ff-legacy");
        let profile = t.0.join("profile");
        make_firefox_profile(&profile, "", Nss::Legacy);
        let batch = import_firefox(&profile, None, &t.0).unwrap();
        check_firefox_batch(&batch);
    }

    #[test]
    fn firefox_primary_password_flow() {
        let t = TestDir::new("ff-primary");
        let profile = t.0.join("profile");
        make_firefox_profile(&profile, "principal-sintética", Nss::Modern);
        assert_eq!(
            import_firefox(&profile, None, &t.0).unwrap_err(),
            ImportError::PrimaryPasswordRequired
        );
        assert_eq!(
            import_firefox(&profile, Some("mala"), &t.0).unwrap_err(),
            ImportError::WrongPrimaryPassword
        );
        let batch = import_firefox(&profile, Some("principal-sintética"), &t.0).unwrap();
        check_firefox_batch(&batch);
    }

    #[test]
    fn firefox_missing_files_fail_cleanly() {
        let t = TestDir::new("ff-missing");
        assert!(matches!(
            import_firefox(&t.0.join("nope"), None, &t.0),
            Err(ImportError::Unreadable(_))
        ));
    }

    fn chromium_blob(version: &[u8], passphrase: &[u8], plain: &[u8]) -> Vec<u8> {
        let key = chromium_key(passphrase).unwrap();
        let mut out = version.to_vec();
        out.extend(crypto::cbc_encrypt(Cipher::Aes128Cbc, &key, &CHROMIUM_IV, plain).unwrap());
        out
    }

    fn make_login_data(path: &Path, rows: &[(&str, &str, Vec<u8>, i64)]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let db = Database::open_read_write(path).unwrap();
        db.execute(
            "CREATE TABLE logins (origin_url VARCHAR NOT NULL, action_url VARCHAR, \
             username_value VARCHAR, password_value BLOB, blacklisted_by_user INTEGER NOT NULL, \
             date_created INTEGER NOT NULL, date_last_used INTEGER NOT NULL DEFAULT 0)",
            &[],
        )
        .unwrap();
        for (origin, user, blob, black) in rows {
            db.execute(
                "INSERT INTO logins (origin_url, username_value, password_value, \
                 blacklisted_by_user, date_created, date_last_used) VALUES (?, ?, ?, ?, ?, ?)",
                &[
                    Value::text(origin),
                    Value::text(user),
                    Value::blob(blob),
                    Value::Integer(*black),
                    Value::Integer(13_350_000_000_000_000),
                    Value::Integer(0),
                ],
            )
            .unwrap();
        }
    }

    #[test]
    fn chromium_v10_v11_and_domain_hash_prefix() {
        let t = TestDir::new("chromium");
        let file = t.0.join("Default").join("Login Data");
        let keyring = b"synthetic-keyring-secret";
        let mut hashed = crypto::hash(Digest::Sha256, b"hash.synthetic.test")
            .unwrap()
            .to_vec();
        hashed.extend_from_slice(b"con-prefijo");
        make_login_data(
            &file,
            &[
                (
                    "https://v10.synthetic.test/login",
                    "ana",
                    chromium_blob(b"v10", b"peanuts", b"pass-v10"),
                    0,
                ),
                (
                    "https://v11.synthetic.test/",
                    "bea",
                    chromium_blob(b"v11", keyring, b"pass-v11"),
                    0,
                ),
                (
                    "https://hash.synthetic.test/",
                    "cris",
                    chromium_blob(b"v10", b"peanuts", &hashed),
                    0,
                ),
                ("https://never.synthetic.test/", "", Vec::new(), 1),
                (
                    "android://abc@com.vendor.app/",
                    "dan",
                    chromium_blob(b"v10", b"peanuts", b"x"),
                    0,
                ),
                (
                    "https://plain.synthetic.test/",
                    "eva",
                    b"texto-en-claro".to_vec(),
                    0,
                ),
            ],
        );
        let keys = ChromiumKeys {
            v11_secret: Some(Password::new(String::from_utf8(keyring.to_vec()).unwrap())),
        };
        let batch = import_chromium(&file, &keys, &t.0).unwrap();
        let got: Vec<(String, String, String)> = batch
            .logins
            .iter()
            .map(|l| {
                (
                    l.origin.to_string(),
                    l.username.clone(),
                    l.password.expose().to_owned(),
                )
            })
            .collect();
        assert_eq!(
            got,
            vec![
                (
                    "https://v10.synthetic.test".into(),
                    "ana".into(),
                    "pass-v10".into()
                ),
                (
                    "https://v11.synthetic.test".into(),
                    "bea".into(),
                    "pass-v11".into()
                ),
                (
                    "https://hash.synthetic.test".into(),
                    "cris".into(),
                    "con-prefijo".into()
                ),
            ]
        );
        assert_eq!(batch.never.len(), 1);
        // android:// (sin sitio web) + texto en claro sin versión reconocida.
        assert_eq!(batch.failed, 2);
        assert!(batch.hint.is_some());
        assert_eq!(
            batch.logins[0].created,
            Some(13_350_000_000 - CHROMIUM_EPOCH_OFFSET_SECS)
        );
        assert_eq!(batch.logins[0].last_used, None);
    }

    #[test]
    fn chromium_v11_without_keyring_key_reports_hint() {
        let t = TestDir::new("chromium-nokey");
        let file = t.0.join("Login Data");
        make_login_data(
            &file,
            &[
                (
                    "https://a.synthetic.test/",
                    "u",
                    chromium_blob(b"v11", b"desconocida", b"p"),
                    0,
                ),
                (
                    "https://b.synthetic.test/",
                    "u",
                    chromium_blob(b"v11", b"", b"vacia"),
                    0,
                ),
            ],
        );
        let batch = import_chromium(&file, &ChromiumKeys { v11_secret: None }, &t.0).unwrap();
        assert_eq!(batch.logins.len(), 1);
        assert_eq!(batch.logins[0].password.expose(), "vacia");
        assert_eq!(batch.failed, 1);
        assert!(batch.hint.unwrap().contains("Safe Storage"));
    }

    #[test]
    fn chromium_unknown_version_suggests_csv() {
        let t = TestDir::new("chromium-v20");
        let file = t.0.join("Login Data");
        make_login_data(
            &file,
            &[(
                "https://a.synthetic.test/",
                "u",
                chromium_blob(b"v20", b"peanuts", b"p"),
                0,
            )],
        );
        let err = import_chromium(&file, &ChromiumKeys { v11_secret: None }, &t.0).unwrap_err();
        assert!(matches!(err, ImportError::UnsupportedVersion(_)));
        assert!(err.to_string().contains("CSV"));
    }

    #[test]
    fn chromium_key_matches_reference_derivation() {
        // PBKDF2-HMAC-SHA1("peanuts", "saltysalt", 1, 16) calculado de forma independiente.
        let k = chromium_key(b"peanuts").unwrap();
        let hex: String = k.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "fd621fe5a2b402539dfa147ca9272778");
    }

    #[test]
    fn csv_parser_handles_quotes_newlines_and_crlf() {
        let rows =
            parse_csv("a,b,c\r\n\"x, y\",\"he said \"\"hi\"\"\",\"multi\nline\"\n\n,,\nlast,row,");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1][0].as_str(), "x, y");
        assert_eq!(rows[1][1].as_str(), "he said \"hi\"");
        assert_eq!(rows[1][2].as_str(), "multi\nline");
        assert_eq!(rows[2][2].as_str(), "");
    }

    #[test]
    fn csv_import_chrome_firefox_bitwarden_formats() {
        let chrome = "name,url,username,password,note\n\
                      synthetic.test,https://synthetic.test/login,ana,\"p,1\",\n\
                      app,android://x@com.vendor.app/,dan,p,\n\
                      empty,https://e.synthetic.test,eva,,\n";
        let b = import_csv(chrome).unwrap();
        assert_eq!(b.logins.len(), 1);
        assert_eq!(b.logins[0].password.expose(), "p,1");
        assert_eq!(b.failed, 2);

        let firefox = "\u{feff}\"url\",\"username\",\"password\",\"httpRealm\",\"formActionOrigin\",\"guid\",\"timeCreated\",\"timeLastUsed\",\"timePasswordChanged\"\n\
                       \"http://ff.synthetic.test:8080\",\"bob\",\"q\",,\"\",\"{1}\",\"1\",\"1\",\"1\"\n";
        let b = import_csv(firefox).unwrap();
        assert_eq!(
            b.logins[0].origin.to_string(),
            "http://ff.synthetic.test:8080"
        );

        let bitwarden = "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp\n\
                         ,,login,S,,,0,bw.synthetic.test,carl,r,\n\
                         ,,note,N,texto,,0,,,,\n";
        let b = import_csv(bitwarden).unwrap();
        assert_eq!(b.logins.len(), 1);
        assert_eq!(b.logins[0].origin.to_string(), "https://bw.synthetic.test");
        assert_eq!(b.failed, 1);

        assert_eq!(
            import_csv("foo,bar\n1,2").unwrap_err(),
            ImportError::CsvHeader
        );
        assert_eq!(import_csv("").unwrap_err(), ImportError::CsvHeader);
    }

    #[test]
    fn apply_merges_into_vault_with_summary() {
        let vault = Vault::new(Box::<MemoryStore>::default());
        let csv = "url,username,password\nhttps://a.synthetic.test,u,p\nhttps://a.synthetic.test,u,p2\nnot a url at all,x,y\n";
        let batch = import_csv(csv).unwrap();
        let summary = apply(&vault, &batch).unwrap();
        assert_eq!(summary.imported, 1);
        assert_eq!(summary.failed, 1);
        let again = apply(&vault, &batch).unwrap();
        assert_eq!(again.imported, 0);
        assert_eq!(again.duplicates, 1);
    }

    #[test]
    fn discover_profiles_in_synthetic_home_only() {
        let t = TestDir::new("discover");
        let home = &t.0;
        let ff_root = home.join(".mozilla/firefox");
        make_firefox_profile(&ff_root.join("abcd.default-release"), "", Nss::Modern);
        fs::create_dir_all(ff_root.join("empty.profile")).unwrap();
        fs::write(
            ff_root.join("profiles.ini"),
            "[Install4F96D1932A9F858E]\nDefault=abcd.default-release\n\n\
             [Profile0]\nName=default-release\nIsRelative=1\nPath=abcd.default-release\n\n\
             [Profile1]\nName=vacío\nIsRelative=1\nPath=empty.profile\n\n[General]\nVersion=2\n",
        )
        .unwrap();
        make_login_data(
            &home.join(".config/BraveSoftware/Brave-Browser/Default/Login Data"),
            &[],
        );
        make_login_data(&home.join(".config/chromium/Profile 1/Login Data"), &[]);
        fs::create_dir_all(home.join(".config/chromium/System Profile")).unwrap();

        let found = discover_profiles(home);
        assert_eq!(found.len(), 3, "{found:?}");
        assert_eq!(found[0].browser, "Firefox");
        assert_eq!(found[0].name, "default-release");
        assert_eq!(found[0].kind, ProfileKind::Firefox);
        assert!(found.iter().any(|p| p.browser == "Brave"
            && p.kind
                == ProfileKind::Chromium {
                    keyring_app: "brave".into()
                }));
        assert!(found
            .iter()
            .any(|p| p.browser == "Chromium" && p.name == "Profile 1"));
        assert!(found.iter().all(|p| p.path.starts_with(home)));
    }

    #[test]
    fn errors_and_debug_never_leak_passwords() {
        let keys = ChromiumKeys {
            v11_secret: Some(Password::new("clave-llavero-secreta".into())),
        };
        assert!(!format!("{keys:?}").contains("clave-llavero-secreta"));
        let batch = import_csv("url,username,password\nhttps://a.test,u,MuySecreta123\n").unwrap();
        assert!(!format!("{batch:?}").contains("MuySecreta123"));
        for e in [
            ImportError::PrimaryPasswordRequired,
            ImportError::WrongPrimaryPassword,
            ImportError::UnsupportedVersion("v20".into()),
            ImportError::CsvHeader,
        ] {
            assert!(!e.to_string().contains("MuySecreta123"));
        }
    }

    #[test]
    fn base64_roundtrip() {
        for n in 0..10 {
            let data = rand(n);
            let enc = base64_encode(&data);
            assert_eq!(base64_decode(&enc).unwrap().as_slice(), &data[..]);
        }
        assert!(base64_decode("@@@").is_none());
    }
}
