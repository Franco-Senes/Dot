//! Vault: almacén seguro de contraseñas (port de `Vault.swift` + reglas de `Forms.swift`).
//!
//! Principios (ver PORTING.md §14, «Seguridad de Vault»):
//! - Las contraseñas viven **solo** en el Secret Service (libsecret: GNOME Keyring, KWallet o el
//!   portal de secretos de Flatpak). Nunca se escriben en disco, en la sesión, en el historial ni
//!   en los logs. En memoria van en [`Password`] (`Zeroizing<String>`), que se pone a cero al
//!   liberarse y cuyo `Debug` está ocultado.
//! - Si no hay Secret Service, Vault queda **desactivado** con un motivo visible en la interfaz.
//!   No existe alternativa en texto plano.
//! - Coincidencia **exacta de origin** (esquema + host + puerto): una credencial guardada en
//!   `https://example.com` nunca se ofrece en `http://example.com`, ni en `https://a.example.com`,
//!   ni en un iframe de otro origin. Las páginas HTTP sin cifrar se marcan como inseguras.
//! - Las listas solo leen atributos; cada secreto se lee por separado y solo cuando se rellena,
//!   se muestra o se copia (igual que `Vault.swift`).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use url::Url;
use zeroize::{Zeroize, Zeroizing};

use crate::paths::{self, SearchPaths};

/// Errores de Vault. Ninguna variante transporta contraseñas.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VaultError {
    #[error("Vault está desactivado: {0}")]
    Disabled(String),
    #[error("la dirección no es un origin http(s) válido")]
    InvalidOrigin,
    #[error("el marco no pertenece al mismo origin que la página")]
    CrossOrigin,
    #[error("la contraseña está vacía o no es UTF-8 válido")]
    InvalidPassword,
    #[error("error del llavero: {0}")]
    Store(String),
    #[error("error de E/S: {0}")]
    Io(String),
}

pub type Result<T> = std::result::Result<T, VaultError>;

// ---------------------------------------------------------------------------------------------
// Password
// ---------------------------------------------------------------------------------------------

/// Una contraseña en memoria: se pone a cero al liberarse y nunca se imprime.
#[derive(Clone, Default)]
pub struct Password(Zeroizing<String>);

impl Password {
    /// Toma posesión de la cadena (sin copias intermedias).
    pub fn new(value: String) -> Self {
        Password(Zeroizing::new(value))
    }

    /// Convierte bytes UTF-8 (p. ej. del llavero o de un descifrado) sin dejar copias sin borrar.
    pub fn from_utf8(mut bytes: Zeroizing<Vec<u8>>) -> Result<Self> {
        let raw = std::mem::take(&mut *bytes);
        match String::from_utf8(raw) {
            Ok(text) => Ok(Password::new(text)),
            Err(e) => {
                let mut back = e.into_bytes();
                back.zeroize();
                Err(VaultError::InvalidPassword)
            }
        }
    }

    /// Acceso explícito al texto. Úsalo solo para rellenar, copiar, mostrar o exportar.
    pub fn expose(&self) -> &str {
        self.0.as_str()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Comparación en tiempo constante respecto al contenido (no a la longitud).
    pub fn same_as(&self, other: &Password) -> bool {
        let a = self.0.as_bytes();
        let b = other.0.as_bytes();
        if a.len() != b.len() {
            return false;
        }
        a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
    }
}

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Password(<oculta>)")
    }
}

// ---------------------------------------------------------------------------------------------
// Origin
// ---------------------------------------------------------------------------------------------

/// Origin web normalizado: esquema (`http`/`https`) + host (minúsculas, IDNA en Punycode) +
/// puerto efectivo.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Origin {
    scheme: String,
    host: String,
    port: u16,
}

impl Origin {
    /// Origin de una URL completa (la de un marco de CEF). Solo `http`/`https`.
    pub fn parse(raw: &str) -> Result<Origin> {
        let url = Url::parse(raw.trim()).map_err(|_| VaultError::InvalidOrigin)?;
        Self::from_url(&url)
    }

    /// Origin de un texto «suelto» de un CSV o de otro navegador: añade `https://` si falta el
    /// esquema. Rechaza todo lo que no sea un sitio web (p. ej. `android://…@com.vendor.app/`,
    /// que podría leerse como un dominio de otra persona; ver `Vault.host(of:)` del original).
    pub fn parse_loose(raw: &str) -> Result<Origin> {
        let text = raw.trim();
        if text.is_empty() {
            return Err(VaultError::InvalidOrigin);
        }
        if text.contains("://") {
            Self::parse(text)
        } else {
            Self::parse(&format!("https://{text}"))
        }
    }

    fn from_url(url: &Url) -> Result<Origin> {
        let scheme = url.scheme().to_ascii_lowercase();
        if scheme != "http" && scheme != "https" {
            return Err(VaultError::InvalidOrigin);
        }
        let host = url
            .host_str()
            .filter(|h| !h.is_empty())
            .ok_or(VaultError::InvalidOrigin)?
            .to_ascii_lowercase();
        let port = url
            .port_or_known_default()
            .ok_or(VaultError::InvalidOrigin)?;
        Ok(Origin { scheme, host, port })
    }

    pub fn scheme(&self) -> &str {
        &self.scheme
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// `true` solo para `https`.
    pub fn is_https(&self) -> bool {
        self.scheme == "https"
    }

    /// `https`, o `http` en loopback (`localhost`, `127.0.0.0/8`, `[::1]`). Fuera de esto se
    /// avisa siempre antes de guardar o rellenar.
    pub fn is_potentially_trustworthy(&self) -> bool {
        if self.is_https() {
            return true;
        }
        let h = self.host.as_str();
        h == "localhost"
            || h.ends_with(".localhost")
            || h == "[::1]"
            || h.parse::<std::net::Ipv4Addr>()
                .map(|ip| ip.is_loopback())
                .unwrap_or(false)
    }

    fn default_port(&self) -> u16 {
        if self.is_https() {
            443
        } else {
            80
        }
    }
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.port == self.default_port() {
            write!(f, "{}://{}", self.scheme, self.host)
        } else {
            write!(f, "{}://{}:{}", self.scheme, self.host, self.port)
        }
    }
}

impl std::str::FromStr for Origin {
    type Err = VaultError;
    fn from_str(s: &str) -> Result<Self> {
        Origin::parse(s)
    }
}

/// Origin efectivo para capturar o rellenar en un marco: el del marco, y solo si es el marco
/// principal o un iframe del **mismo** origin que la página.
pub fn frame_origin(page_url: &str, frame_url: &str) -> Result<Origin> {
    let page = Origin::parse(page_url)?;
    let frame = Origin::parse(frame_url)?;
    if page != frame {
        return Err(VaultError::CrossOrigin);
    }
    Ok(frame)
}

// ---------------------------------------------------------------------------------------------
// Credenciales y almacén
// ---------------------------------------------------------------------------------------------

/// Lo que una lista necesita de una credencial: sin el secreto (como `Kept` del original).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialMeta {
    pub origin: Origin,
    pub username: String,
    /// Segundos Unix de creación.
    pub created: i64,
    /// Segundos Unix del último uso para iniciar sesión.
    pub last_used: Option<i64>,
}

/// Una credencial importada de otro navegador o de un CSV.
#[derive(Debug, Clone)]
pub struct ImportedLogin {
    pub origin: Origin,
    pub username: String,
    pub password: Password,
    pub created: Option<i64>,
    pub last_used: Option<i64>,
}

/// Backend de secretos. La implementación real es [`SecretServiceStore`]; los tests usan
/// [`MemoryStore`].
pub trait SecretStore: Send + Sync {
    /// Guarda la credencial, sustituyendo cualquier otra con el mismo `(origin, username)`.
    fn put(&self, meta: &CredentialMeta, password: &Password) -> Result<()>;
    /// Lee un secreto concreto.
    fn get(&self, origin: &Origin, username: &str) -> Result<Option<Password>>;
    /// Borra una credencial. Devuelve si existía.
    fn remove(&self, origin: &Origin, username: &str) -> Result<bool>;
    /// Lista credenciales (sin secretos), opcionalmente de un origin.
    fn list(&self, origin: Option<&Origin>) -> Result<Vec<CredentialMeta>>;
}

type MemoryItems = BTreeMap<(Origin, String), (CredentialMeta, Password)>;

/// Almacén en memoria para tests y pruebas headless. Nunca toca disco.
#[derive(Default)]
pub struct MemoryStore {
    items: Mutex<MemoryItems>,
}

impl MemoryStore {
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, MemoryItems>> {
        self.items
            .lock()
            .map_err(|_| VaultError::Store("almacén en memoria envenenado".into()))
    }
}

impl SecretStore for MemoryStore {
    fn put(&self, meta: &CredentialMeta, password: &Password) -> Result<()> {
        self.lock()?.insert(
            (meta.origin.clone(), meta.username.clone()),
            (meta.clone(), password.clone()),
        );
        Ok(())
    }

    fn get(&self, origin: &Origin, username: &str) -> Result<Option<Password>> {
        Ok(self
            .lock()?
            .get(&(origin.clone(), username.to_owned()))
            .map(|(_, p)| p.clone()))
    }

    fn remove(&self, origin: &Origin, username: &str) -> Result<bool> {
        Ok(self
            .lock()?
            .remove(&(origin.clone(), username.to_owned()))
            .is_some())
    }

    fn list(&self, origin: Option<&Origin>) -> Result<Vec<CredentialMeta>> {
        Ok(self
            .lock()?
            .values()
            .filter(|(m, _)| origin.is_none_or(|o| &m.origin == o))
            .map(|(m, _)| m.clone())
            .collect())
    }
}

/// Nombre del esquema de libsecret de Vault.
pub const SECRET_SCHEMA: &str = "io.github.searchlinux.Search.Password";
const ATTR_APP: &str = "app";
const ATTR_ORIGIN: &str = "origin";
const ATTR_USERNAME: &str = "username";
const ATTR_CREATED: &str = "created";
const ATTR_LAST_USED: &str = "last_used";

/// Almacén real en el Secret Service (libsecret). Cada credencial es un secreto con atributos
/// `app`, `origin` (esquema+host+puerto), `username`, `created` y `last_used`.
pub struct SecretServiceStore {
    schema: gnome_platform::secret::Schema,
    app: String,
}

impl SecretServiceStore {
    /// Conecta con el Secret Service. `app` separa las credenciales reales (`search`) de las de
    /// pruebas (`search-test`), como el `label` de `Vault.swift`.
    pub fn connect(app: &str) -> Result<Self> {
        let schema = gnome_platform::secret::Schema::new(
            SECRET_SCHEMA,
            true,
            &[
                ATTR_APP,
                ATTR_ORIGIN,
                ATTR_USERNAME,
                ATTR_CREATED,
                ATTR_LAST_USED,
            ],
        )
        .map_err(|e| VaultError::Store(e.message))?;
        if !gnome_platform::secret::is_available() {
            return Err(VaultError::Disabled(
                "no hay ningún Secret Service (GNOME Keyring, KWallet o portal de secretos) \
                 disponible en el bus de sesión"
                    .into(),
            ));
        }
        Ok(SecretServiceStore {
            schema,
            app: app.to_owned(),
        })
    }
}

fn store_err(e: gnome_platform::secret::SecretError) -> VaultError {
    VaultError::Store(e.message)
}

impl SecretStore for SecretServiceStore {
    fn put(&self, meta: &CredentialMeta, password: &Password) -> Result<()> {
        if password.is_empty() {
            return Err(VaultError::InvalidPassword);
        }
        let origin = meta.origin.to_string();
        // Los atributos de fecha forman parte de la identidad en libsecret, así que primero se
        // borra cualquier versión anterior de (app, origin, username).
        gnome_platform::secret::clear(
            &self.schema,
            &[
                (ATTR_APP, &self.app),
                (ATTR_ORIGIN, &origin),
                (ATTR_USERNAME, &meta.username),
            ],
        )
        .map_err(store_err)?;
        let created = meta.created.to_string();
        let last_used = meta.last_used.map(|t| t.to_string()).unwrap_or_default();
        let label = if meta.username.is_empty() {
            format!("Search: {origin}")
        } else {
            format!("Search: {} en {origin}", meta.username)
        };
        gnome_platform::secret::store(
            &self.schema,
            &[
                (ATTR_APP, &self.app),
                (ATTR_ORIGIN, &origin),
                (ATTR_USERNAME, &meta.username),
                (ATTR_CREATED, &created),
                (ATTR_LAST_USED, &last_used),
            ],
            &label,
            password.expose().as_bytes(),
        )
        .map_err(store_err)
    }

    fn get(&self, origin: &Origin, username: &str) -> Result<Option<Password>> {
        let origin = origin.to_string();
        let found = gnome_platform::secret::lookup(
            &self.schema,
            &[
                (ATTR_APP, &self.app),
                (ATTR_ORIGIN, &origin),
                (ATTR_USERNAME, username),
            ],
        )
        .map_err(store_err)?;
        found.map(Password::from_utf8).transpose()
    }

    fn remove(&self, origin: &Origin, username: &str) -> Result<bool> {
        let origin = origin.to_string();
        gnome_platform::secret::clear(
            &self.schema,
            &[
                (ATTR_APP, &self.app),
                (ATTR_ORIGIN, &origin),
                (ATTR_USERNAME, username),
            ],
        )
        .map_err(store_err)
    }

    fn list(&self, origin: Option<&Origin>) -> Result<Vec<CredentialMeta>> {
        let origin_text = origin.map(|o| o.to_string());
        let mut attrs: Vec<(&str, &str)> = vec![(ATTR_APP, &self.app)];
        if let Some(o) = origin_text.as_deref() {
            attrs.push((ATTR_ORIGIN, o));
        }
        let rows =
            gnome_platform::secret::search(&self.schema, &attrs, false).map_err(store_err)?;
        Ok(rows.iter().filter_map(meta_from_attributes).collect())
    }
}

fn meta_from_attributes(row: &BTreeMap<String, String>) -> Option<CredentialMeta> {
    let origin = Origin::parse(row.get(ATTR_ORIGIN)?).ok()?;
    Some(CredentialMeta {
        origin,
        username: row.get(ATTR_USERNAME).cloned().unwrap_or_default(),
        created: row
            .get(ATTR_CREATED)
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
        last_used: row.get(ATTR_LAST_USED).and_then(|v| v.parse().ok()),
    })
}

// ---------------------------------------------------------------------------------------------
// Sitios marcados como «Nunca»
// ---------------------------------------------------------------------------------------------

/// Origins en los que el usuario pidió no volver a ofrecer guardar (`passwords.never` del
/// original). Se guarda en `$XDG_DATA_HOME/search/vault_never.json`; no contiene secretos.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct NeverFile {
    never: BTreeSet<String>,
}

struct NeverList {
    path: Option<PathBuf>,
    origins: BTreeSet<String>,
}

impl NeverList {
    fn in_memory() -> Self {
        NeverList {
            path: None,
            origins: BTreeSet::new(),
        }
    }

    fn load(path: &Path) -> Self {
        let origins = match std::fs::read(path) {
            Ok(bytes) => match serde_json::from_slice::<NeverFile>(&bytes) {
                Ok(file) => file.never,
                Err(e) => {
                    crate::warn!("vault_never.json corrupto ({e}); se empieza de cero");
                    let _ = paths::backup_corrupt_file(path);
                    BTreeSet::new()
                }
            },
            Err(_) => BTreeSet::new(),
        };
        NeverList {
            path: Some(path.to_path_buf()),
            origins,
        }
    }

    fn contains(&self, origin: &Origin) -> bool {
        self.origins.contains(&origin.to_string())
    }

    fn insert(&mut self, origin: &Origin) -> Result<()> {
        if self.origins.insert(origin.to_string()) {
            self.save()?;
        }
        Ok(())
    }

    fn remove(&mut self, origin: &Origin) -> Result<()> {
        if self.origins.remove(&origin.to_string()) {
            self.save()?;
        }
        Ok(())
    }

    fn save(&self) -> Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let file = NeverFile {
            never: self.origins.clone(),
        };
        let bytes = serde_json::to_vec_pretty(&file).map_err(|e| VaultError::Io(e.to_string()))?;
        paths::write_atomic(path, &bytes).map_err(|e| VaultError::Io(e.to_string()))
    }
}

// ---------------------------------------------------------------------------------------------
// Vault
// ---------------------------------------------------------------------------------------------

/// Qué ofrecer tras un envío de formulario de login.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureOffer {
    /// Nada que ofrecer (Vault desactivado, sitio en «Nunca», contraseña igual a la guardada…).
    Nothing,
    /// «¿Guardar la contraseña de usuario@sitio?»
    Save {
        origin: Origin,
        username: String,
        insecure: bool,
    },
    /// La contraseña cambió: «¿Actualizar la contraseña de usuario@sitio?»
    Update {
        origin: Origin,
        username: String,
        insecure: bool,
    },
}

/// Credenciales que se pueden ofrecer en un formulario (solo metadatos).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FillOffer {
    pub origin: Origin,
    /// Página HTTP sin cifrar: la interfaz debe avisar antes de rellenar.
    pub insecure: bool,
    /// Las del origin exacto, la usada más recientemente primero.
    pub accounts: Vec<CredentialMeta>,
}

/// Resultado de guardar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveOutcome {
    Created,
    Updated,
    Unchanged,
}

/// Resumen de una importación.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportSummary {
    pub imported: usize,
    pub duplicates: usize,
    pub failed: usize,
}

/// Estado visible en la interfaz.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultStatus {
    Enabled,
    Disabled { reason: String },
}

/// Fachada de Vault: reglas de origin, «Nunca», captura, relleno, panel, importación y
/// exportación sobre un [`SecretStore`].
pub struct Vault {
    store: Option<Box<dyn SecretStore>>,
    disabled_reason: Option<String>,
    never: Mutex<NeverList>,
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

impl Vault {
    /// Vault sobre un almacén concreto, con la lista «Nunca» solo en memoria.
    pub fn new(store: Box<dyn SecretStore>) -> Self {
        Vault {
            store: Some(store),
            disabled_reason: None,
            never: Mutex::new(NeverList::in_memory()),
        }
    }

    /// Vault sobre un almacén concreto, persistiendo la lista «Nunca» en `never_file`.
    pub fn with_never_file(store: Box<dyn SecretStore>, never_file: &Path) -> Self {
        Vault {
            store: Some(store),
            disabled_reason: None,
            never: Mutex::new(NeverList::load(never_file)),
        }
    }

    /// Vault desactivado con un motivo que se enseña al usuario.
    pub fn disabled(reason: impl Into<String>) -> Self {
        Vault {
            store: None,
            disabled_reason: Some(reason.into()),
            never: Mutex::new(NeverList::in_memory()),
        }
    }

    /// Abre el Vault de la aplicación sobre el Secret Service. Si no está disponible devuelve un
    /// Vault desactivado (nunca hay alternativa en texto plano).
    pub fn open_default(paths: &SearchPaths) -> Self {
        let app = if std::env::var_os("SEARCH_PROFILE_DIR").is_some() {
            "search-test"
        } else {
            "search"
        };
        match SecretServiceStore::connect(app) {
            Ok(store) => {
                Self::with_never_file(Box::new(store), &paths.data_dir.join("vault_never.json"))
            }
            Err(e) => {
                let reason = match e {
                    VaultError::Disabled(r) => r,
                    other => other.to_string(),
                };
                crate::warn!("Vault desactivado: {reason}");
                Vault::disabled(reason)
            }
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.store.is_some()
    }

    pub fn status(&self) -> VaultStatus {
        match &self.disabled_reason {
            None => VaultStatus::Enabled,
            Some(reason) => VaultStatus::Disabled {
                reason: reason.clone(),
            },
        }
    }

    pub fn disabled_reason(&self) -> Option<&str> {
        self.disabled_reason.as_deref()
    }

    fn store(&self) -> Result<&dyn SecretStore> {
        self.store.as_deref().ok_or_else(|| {
            VaultError::Disabled(
                self.disabled_reason
                    .clone()
                    .unwrap_or_else(|| "sin Secret Service".into()),
            )
        })
    }

    fn never_lock(&self) -> Result<std::sync::MutexGuard<'_, NeverList>> {
        self.never
            .lock()
            .map_err(|_| VaultError::Io("lista «Nunca» envenenada".into()))
    }

    fn find(&self, origin: &Origin, username: &str) -> Result<Option<CredentialMeta>> {
        Ok(self
            .store()?
            .list(Some(origin))?
            .into_iter()
            .find(|m| m.username == username))
    }

    /// Decide qué ofrecer tras un envío de login en `frame_url` (dentro de `page_url`).
    /// El origin sale siempre de las URLs de CEF, nunca de lo que diga el JavaScript.
    pub fn capture_offer(
        &self,
        page_url: &str,
        frame_url: &str,
        username: &str,
        password: &Password,
    ) -> Result<CaptureOffer> {
        if !self.is_enabled() || password.is_empty() {
            return Ok(CaptureOffer::Nothing);
        }
        let origin = match frame_origin(page_url, frame_url) {
            Ok(o) => o,
            Err(VaultError::CrossOrigin) | Err(VaultError::InvalidOrigin) => {
                return Ok(CaptureOffer::Nothing)
            }
            Err(e) => return Err(e),
        };
        if self.never_lock()?.contains(&origin) {
            return Ok(CaptureOffer::Nothing);
        }
        let insecure = !origin.is_potentially_trustworthy();
        let username = username.trim().to_owned();
        match self.find(&origin, &username)? {
            None => Ok(CaptureOffer::Save {
                origin,
                username,
                insecure,
            }),
            Some(_) => match self.store()?.get(&origin, &username)? {
                Some(saved) if saved.same_as(password) => Ok(CaptureOffer::Nothing),
                _ => Ok(CaptureOffer::Update {
                    origin,
                    username,
                    insecure,
                }),
            },
        }
    }

    /// Guarda (o actualiza) una credencial conservando su fecha de creación.
    pub fn save(
        &self,
        origin: &Origin,
        username: &str,
        password: &Password,
    ) -> Result<SaveOutcome> {
        if password.is_empty() {
            return Err(VaultError::InvalidPassword);
        }
        let store = self.store()?;
        let now = now_secs();
        let existing = self.find(origin, username)?;
        if existing.is_some() {
            if let Some(saved) = store.get(origin, username)? {
                if saved.same_as(password) {
                    return Ok(SaveOutcome::Unchanged);
                }
            }
        }
        let meta = CredentialMeta {
            origin: origin.clone(),
            username: username.to_owned(),
            created: existing.as_ref().map(|m| m.created).unwrap_or(now),
            last_used: Some(now),
        };
        store.put(&meta, password)?;
        Ok(if existing.is_some() {
            SaveOutcome::Updated
        } else {
            SaveOutcome::Created
        })
    }

    /// «Nunca en este sitio».
    pub fn never(&self, origin: &Origin) -> Result<()> {
        self.never_lock()?.insert(origin)
    }

    /// Quita un sitio de «Nunca».
    pub fn allow(&self, origin: &Origin) -> Result<()> {
        self.never_lock()?.remove(origin)
    }

    /// ¿Está el origin en «Nunca»?
    pub fn is_never(&self, origin: &Origin) -> bool {
        self.never_lock()
            .map(|n| n.contains(origin))
            .unwrap_or(false)
    }

    /// Lista de origins en «Nunca».
    pub fn never_list(&self) -> Vec<String> {
        self.never_lock()
            .map(|n| n.origins.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Credenciales que se pueden ofrecer en `frame_url` dentro de `page_url`: solo del mismo
    /// origin exacto y solo en el marco principal o en iframes del mismo origin.
    pub fn fill_offer(&self, page_url: &str, frame_url: &str) -> Result<FillOffer> {
        let origin = frame_origin(page_url, frame_url)?;
        let mut accounts = self.store()?.list(Some(&origin))?;
        accounts.retain(|m| m.origin == origin);
        accounts.sort_by(|a, b| {
            b.last_used
                .unwrap_or(0)
                .cmp(&a.last_used.unwrap_or(0))
                .then_with(|| a.username.cmp(&b.username))
        });
        Ok(FillOffer {
            insecure: !origin.is_potentially_trustworthy(),
            origin,
            accounts,
        })
    }

    /// Secreto para rellenar `username` en `frame_url`, revalidando las reglas de origin
    /// (defensa en profundidad: la interfaz no puede pedir el secreto de otro origin). Marca la
    /// credencial como usada.
    pub fn fill_password(
        &self,
        page_url: &str,
        frame_url: &str,
        username: &str,
    ) -> Result<Option<Password>> {
        let origin = frame_origin(page_url, frame_url)?;
        let found = self.store()?.get(&origin, username)?;
        if let Some(password) = &found {
            if let Err(e) = self.touch_with(&origin, username, password) {
                crate::debug!("Vault: no se pudo actualizar la fecha de uso: {e}");
            }
        }
        Ok(found)
    }

    /// Secreto de una credencial para mostrarlo o copiarlo desde el panel.
    pub fn password(&self, origin: &Origin, username: &str) -> Result<Option<Password>> {
        self.store()?.get(origin, username)
    }

    fn touch_with(&self, origin: &Origin, username: &str, password: &Password) -> Result<()> {
        let store = self.store()?;
        let created = self
            .find(origin, username)?
            .map(|m| m.created)
            .unwrap_or_else(now_secs);
        store.put(
            &CredentialMeta {
                origin: origin.clone(),
                username: username.to_owned(),
                created,
                last_used: Some(now_secs()),
            },
            password,
        )
    }

    /// Se acaba de usar para iniciar sesión: las listas la ponen primero.
    pub fn touch(&self, origin: &Origin, username: &str) -> Result<()> {
        match self.store()?.get(origin, username)? {
            Some(password) => self.touch_with(origin, username, &password),
            None => Ok(()),
        }
    }

    /// Edita una credencial (usuario y/o contraseña). Si cambia el usuario se borra la antigua.
    pub fn edit(
        &self,
        origin: &Origin,
        old_username: &str,
        new_username: &str,
        password: &Password,
    ) -> Result<()> {
        if password.is_empty() {
            return Err(VaultError::InvalidPassword);
        }
        let store = self.store()?;
        let created = self
            .find(origin, old_username)?
            .map(|m| m.created)
            .unwrap_or_else(now_secs);
        store.put(
            &CredentialMeta {
                origin: origin.clone(),
                username: new_username.to_owned(),
                created,
                last_used: Some(now_secs()),
            },
            password,
        )?;
        if old_username != new_username {
            store.remove(origin, old_username)?;
        }
        Ok(())
    }

    /// Borra una credencial.
    pub fn delete(&self, origin: &Origin, username: &str) -> Result<bool> {
        self.store()?.remove(origin, username)
    }

    /// Todas las credenciales (sin secretos) que coinciden con `query` en origin o usuario,
    /// ordenadas por origin y usuario.
    pub fn list(&self, query: &str) -> Result<Vec<CredentialMeta>> {
        let needle = query.trim().to_lowercase();
        let mut all = self.store()?.list(None)?;
        if !needle.is_empty() {
            all.retain(|m| {
                m.origin.to_string().to_lowercase().contains(&needle)
                    || m.username.to_lowercase().contains(&needle)
            });
        }
        all.sort_by(|a, b| {
            a.origin
                .host()
                .cmp(b.origin.host())
                .then_with(|| a.origin.cmp(&b.origin))
                .then_with(|| a.username.cmp(&b.username))
        });
        Ok(all)
    }

    /// Importa credenciales. Nunca sobrescribe una que ya exista con el mismo
    /// `(origin, usuario)`: cuenta como duplicada.
    pub fn import(&self, logins: &[ImportedLogin]) -> Result<ImportSummary> {
        let store = self.store()?;
        let mut summary = ImportSummary::default();
        let mut existing: BTreeSet<(Origin, String)> = store
            .list(None)?
            .into_iter()
            .map(|m| (m.origin, m.username))
            .collect();
        let now = now_secs();
        for login in logins {
            let key = (login.origin.clone(), login.username.clone());
            if existing.contains(&key) {
                summary.duplicates += 1;
                continue;
            }
            if login.password.is_empty() {
                summary.failed += 1;
                continue;
            }
            let meta = CredentialMeta {
                origin: login.origin.clone(),
                username: login.username.clone(),
                created: login.created.unwrap_or(now),
                last_used: login.last_used,
            };
            match store.put(&meta, &login.password) {
                Ok(()) => {
                    summary.imported += 1;
                    existing.insert(key);
                }
                Err(e) => {
                    crate::warn!("Vault: no se pudo importar una credencial: {e}");
                    summary.failed += 1;
                }
            }
        }
        Ok(summary)
    }

    /// Exporta todo a CSV en el formato de Chrome (`name,url,username,password,note`).
    /// **El resultado contiene contraseñas en claro**: la interfaz debe avisar antes y el
    /// llamador escribirlo solo donde el usuario elija.
    pub fn export_csv(&self) -> Result<Zeroizing<String>> {
        let store = self.store()?;
        let mut out = Zeroizing::new(String::from("name,url,username,password,note\n"));
        for meta in self.list("")? {
            let Some(password) = store.get(&meta.origin, &meta.username)? else {
                continue;
            };
            let url = meta.origin.to_string();
            push_csv_field(&mut out, meta.origin.host());
            out.push(',');
            push_csv_field(&mut out, &url);
            out.push(',');
            push_csv_field(&mut out, &meta.username);
            out.push(',');
            push_csv_field(&mut out, password.expose());
            out.push_str(",\n");
        }
        Ok(out)
    }
}

/// Añade un campo CSV (RFC 4180): entre comillas si contiene `,`, `"` o saltos de línea.
fn push_csv_field(out: &mut String, field: &str) {
    if field.contains([',', '"', '\n', '\r']) {
        out.push('"');
        for c in field.chars() {
            if c == '"' {
                out.push('"');
            }
            out.push(c);
        }
        out.push('"');
    } else {
        out.push_str(field);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pw(s: &str) -> Password {
        Password::new(s.to_owned())
    }

    fn vault() -> Vault {
        Vault::new(Box::<MemoryStore>::default())
    }

    #[test]
    fn origin_normalization() {
        let o = Origin::parse("HTTPS://Example.COM/login?x=1#y").unwrap();
        assert_eq!(o.to_string(), "https://example.com");
        assert_eq!(o.port(), 443);
        let o = Origin::parse("https://example.com:8443/a").unwrap();
        assert_eq!(o.to_string(), "https://example.com:8443");
        let o = Origin::parse("http://bücher.example/").unwrap();
        assert_eq!(o.host(), "xn--bcher-kva.example");
        assert!(Origin::parse("file:///etc/passwd").is_err());
        assert!(Origin::parse("javascript:alert(1)").is_err());
        assert!(Origin::parse("data:text/html,hi").is_err());
        assert!(Origin::parse_loose("android://abc@com.vendor.app/").is_err());
        assert_eq!(
            Origin::parse_loose("accounts.example.org")
                .unwrap()
                .to_string(),
            "https://accounts.example.org"
        );
        assert!(Origin::parse_loose("   ").is_err());
    }

    #[test]
    fn origin_matching_is_exact() {
        let https = Origin::parse("https://example.com/").unwrap();
        assert_ne!(https, Origin::parse("http://example.com/").unwrap());
        assert_ne!(https, Origin::parse("https://www.example.com/").unwrap());
        assert_ne!(https, Origin::parse("https://example.com:444/").unwrap());
        assert_eq!(
            https,
            Origin::parse("https://example.com:443/other").unwrap()
        );
        assert_ne!(
            https,
            Origin::parse("https://example.com.evil.test/").unwrap()
        );
    }

    #[test]
    fn trustworthiness() {
        assert!(Origin::parse("https://a.test")
            .unwrap()
            .is_potentially_trustworthy());
        assert!(!Origin::parse("http://a.test")
            .unwrap()
            .is_potentially_trustworthy());
        assert!(Origin::parse("http://localhost:8080")
            .unwrap()
            .is_potentially_trustworthy());
        assert!(Origin::parse("http://127.0.0.1/")
            .unwrap()
            .is_potentially_trustworthy());
        assert!(Origin::parse("http://[::1]/")
            .unwrap()
            .is_potentially_trustworthy());
    }

    #[test]
    fn frame_rules() {
        assert!(frame_origin("https://a.test/x", "https://a.test/frame").is_ok());
        assert_eq!(
            frame_origin("https://a.test/x", "https://evil.test/frame").unwrap_err(),
            VaultError::CrossOrigin
        );
        assert_eq!(
            frame_origin("https://a.test/x", "http://a.test/frame").unwrap_err(),
            VaultError::CrossOrigin
        );
    }

    #[test]
    fn capture_save_update_and_unchanged() {
        let v = vault();
        let page = "https://login.example.test/signin";
        let offer = v.capture_offer(page, page, "ana", &pw("uno")).unwrap();
        let origin = Origin::parse(page).unwrap();
        assert_eq!(
            offer,
            CaptureOffer::Save {
                origin: origin.clone(),
                username: "ana".into(),
                insecure: false
            }
        );
        assert_eq!(
            v.save(&origin, "ana", &pw("uno")).unwrap(),
            SaveOutcome::Created
        );
        assert_eq!(
            v.capture_offer(page, page, "ana", &pw("uno")).unwrap(),
            CaptureOffer::Nothing
        );
        assert!(matches!(
            v.capture_offer(page, page, "ana", &pw("dos")).unwrap(),
            CaptureOffer::Update { .. }
        ));
        assert_eq!(
            v.save(&origin, "ana", &pw("dos")).unwrap(),
            SaveOutcome::Updated
        );
        assert_eq!(
            v.save(&origin, "ana", &pw("dos")).unwrap(),
            SaveOutcome::Unchanged
        );
        assert_eq!(v.password(&origin, "ana").unwrap().unwrap().expose(), "dos");
    }

    #[test]
    fn capture_ignores_cross_origin_frames_never_sites_and_empty() {
        let v = vault();
        assert_eq!(
            v.capture_offer("https://a.test/", "https://ads.test/", "u", &pw("p"))
                .unwrap(),
            CaptureOffer::Nothing
        );
        assert_eq!(
            v.capture_offer("https://a.test/", "https://a.test/", "u", &pw(""))
                .unwrap(),
            CaptureOffer::Nothing
        );
        let origin = Origin::parse("https://a.test/").unwrap();
        v.never(&origin).unwrap();
        assert!(v.is_never(&origin));
        assert_eq!(
            v.capture_offer("https://a.test/", "https://a.test/", "u", &pw("p"))
                .unwrap(),
            CaptureOffer::Nothing
        );
        v.allow(&origin).unwrap();
        assert!(matches!(
            v.capture_offer("https://a.test/", "https://a.test/", "u", &pw("p"))
                .unwrap(),
            CaptureOffer::Save { .. }
        ));
    }

    #[test]
    fn capture_flags_http_as_insecure() {
        let v = vault();
        match v
            .capture_offer("http://plain.test/", "http://plain.test/", "u", &pw("p"))
            .unwrap()
        {
            CaptureOffer::Save { insecure, .. } => assert!(insecure),
            other => panic!("oferta inesperada: {other:?}"),
        }
    }

    #[test]
    fn fill_only_offers_exact_origin_and_never_downgrades() {
        let v = vault();
        let https = Origin::parse("https://site.test/").unwrap();
        v.save(&https, "ana", &pw("secreto-https")).unwrap();
        v.save(
            &Origin::parse("https://sub.site.test/").unwrap(),
            "luis",
            &pw("x"),
        )
        .unwrap();

        let offer = v
            .fill_offer("https://site.test/login", "https://site.test/login")
            .unwrap();
        assert_eq!(offer.accounts.len(), 1);
        assert_eq!(offer.accounts[0].username, "ana");
        assert!(!offer.insecure);

        // Nunca de HTTPS a HTTP.
        let offer = v
            .fill_offer("http://site.test/login", "http://site.test/login")
            .unwrap();
        assert!(offer.accounts.is_empty());
        assert!(offer.insecure);

        // Nunca a un iframe de otro origin.
        assert_eq!(
            v.fill_offer("https://site.test/", "https://evil.test/")
                .unwrap_err(),
            VaultError::CrossOrigin
        );
        assert_eq!(
            v.fill_password("https://site.test/", "https://evil.test/", "ana")
                .unwrap_err(),
            VaultError::CrossOrigin
        );
        let got = v
            .fill_password("https://site.test/", "https://site.test/", "ana")
            .unwrap()
            .unwrap();
        assert_eq!(got.expose(), "secreto-https");
    }

    #[test]
    fn edit_delete_list_and_search() {
        let v = vault();
        let a = Origin::parse("https://alpha.test").unwrap();
        let b = Origin::parse("https://beta.test").unwrap();
        v.save(&b, "zoe", &pw("1")).unwrap();
        v.save(&a, "ana", &pw("2")).unwrap();
        let all = v.list("").unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].origin, a);
        assert_eq!(v.list("ZOE").unwrap().len(), 1);
        assert_eq!(v.list("beta").unwrap().len(), 1);
        v.edit(&a, "ana", "ana2", &pw("3")).unwrap();
        assert!(v.password(&a, "ana").unwrap().is_none());
        assert_eq!(v.password(&a, "ana2").unwrap().unwrap().expose(), "3");
        assert!(v.delete(&a, "ana2").unwrap());
        assert!(!v.delete(&a, "ana2").unwrap());
        assert_eq!(v.list("").unwrap().len(), 1);
    }

    #[test]
    fn import_counts_duplicates_and_never_overwrites() {
        let v = vault();
        let o = Origin::parse("https://dup.test").unwrap();
        v.save(&o, "ana", &pw("actual")).unwrap();
        let logins = vec![
            ImportedLogin {
                origin: o.clone(),
                username: "ana".into(),
                password: pw("vieja"),
                created: None,
                last_used: None,
            },
            ImportedLogin {
                origin: o.clone(),
                username: "bea".into(),
                password: pw("b"),
                created: Some(10),
                last_used: Some(20),
            },
            ImportedLogin {
                origin: o.clone(),
                username: "bea".into(),
                password: pw("b2"),
                created: None,
                last_used: None,
            },
            ImportedLogin {
                origin: o.clone(),
                username: "cris".into(),
                password: pw(""),
                created: None,
                last_used: None,
            },
        ];
        let s = v.import(&logins).unwrap();
        assert_eq!(
            s,
            ImportSummary {
                imported: 1,
                duplicates: 2,
                failed: 1
            }
        );
        assert_eq!(v.password(&o, "ana").unwrap().unwrap().expose(), "actual");
        let bea = v.list("bea").unwrap();
        assert_eq!(bea[0].created, 10);
        assert_eq!(bea[0].last_used, Some(20));
    }

    #[test]
    fn export_csv_quotes_fields() {
        let v = vault();
        let o = Origin::parse("https://csv.test").unwrap();
        v.save(&o, "ana", &pw("a,b\"c")).unwrap();
        let csv = v.export_csv().unwrap();
        assert_eq!(
            csv.as_str(),
            "name,url,username,password,note\ncsv.test,https://csv.test,ana,\"a,b\"\"c\",\n"
        );
    }

    #[test]
    fn disabled_vault_refuses_everything_without_plaintext_fallback() {
        let v = Vault::disabled("sin Secret Service");
        assert!(!v.is_enabled());
        assert_eq!(
            v.status(),
            VaultStatus::Disabled {
                reason: "sin Secret Service".into()
            }
        );
        let o = Origin::parse("https://a.test").unwrap();
        assert!(matches!(
            v.save(&o, "u", &pw("p")),
            Err(VaultError::Disabled(_))
        ));
        assert_eq!(
            v.capture_offer("https://a.test", "https://a.test", "u", &pw("p"))
                .unwrap(),
            CaptureOffer::Nothing
        );
        assert!(v.fill_offer("https://a.test", "https://a.test").is_err());
        assert!(v.export_csv().is_err());
    }

    #[test]
    fn passwords_never_appear_in_debug_or_errors() {
        let secret = "Contraseña-Ultra-Secreta-42";
        let p = pw(secret);
        assert!(!format!("{p:?}").contains(secret));
        let login = ImportedLogin {
            origin: Origin::parse("https://a.test").unwrap(),
            username: "u".into(),
            password: p.clone(),
            created: None,
            last_used: None,
        };
        assert!(!format!("{login:?}").contains(secret));
        let v = vault();
        let o = Origin::parse("https://a.test").unwrap();
        v.save(&o, "u", &p).unwrap();
        // Ningún resultado de lista ni error expone el secreto.
        assert!(!format!("{:?}", v.list("").unwrap()).contains(secret));
        assert!(
            !format!("{:?}", v.fill_offer("https://a.test", "https://a.test")).contains(secret)
        );
        assert!(!format!(
            "{:?}",
            v.capture_offer("https://a.test", "https://a.test", "u", &pw("otra"))
        )
        .contains(secret));
        let err = Password::from_utf8(Zeroizing::new(vec![0xff, 0xfe])).unwrap_err();
        assert!(!err.to_string().contains(secret));
    }

    #[test]
    fn password_compare_and_utf8() {
        assert!(pw("abc").same_as(&pw("abc")));
        assert!(!pw("abc").same_as(&pw("abd")));
        assert!(!pw("abc").same_as(&pw("abcd")));
        let p = Password::from_utf8(Zeroizing::new(b"hola".to_vec())).unwrap();
        assert_eq!(p.expose(), "hola");
    }

    #[test]
    fn never_list_persists_atomically() {
        let base = std::env::temp_dir().join(format!(
            "search-vault-never-{}-{}",
            std::process::id(),
            now_secs()
        ));
        std::fs::create_dir_all(&base).unwrap();
        let file = base.join("vault_never.json");
        let o = Origin::parse("https://never.test").unwrap();
        {
            let v = Vault::with_never_file(Box::<MemoryStore>::default(), &file);
            v.never(&o).unwrap();
        }
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("https://never.test"));
        let v = Vault::with_never_file(Box::<MemoryStore>::default(), &file);
        assert!(v.is_never(&o));
        assert_eq!(v.never_list(), vec!["https://never.test".to_string()]);
        std::fs::remove_dir_all(&base).unwrap();
    }
}
