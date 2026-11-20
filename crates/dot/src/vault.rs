//! Integración de Vault en la aplicación (parte GTK/CEF de `Vault.swift` + `Forms.swift`).
//!
//! - [`VaultService`]: un único hilo de trabajo dueño de [`Vault`] (las llamadas a libsecret son
//!   bloqueantes y pueden abrir el diálogo de desbloqueo del llavero). La interfaz encola trabajos
//!   y recibe el resultado en el hilo principal de GLib.
//! - [`parse_ipc`]: decodifica los mensajes de `resources/vault.js` que llegan por el puente IPC
//!   autenticado de `browser_view.rs` (token aleatorio por marco).
//! - [`attach`]: máquina de estados por pestaña: captura de envíos → oferta de guardar/actualizar
//!   cuando el inicio de sesión ha funcionado, y oferta de cuentas al enfocar un campo de login.
//!
//! El origin sale siempre de las URLs de CEF (`VaultFrameMessage::{page_url, frame_url}`) y se
//! revalida en `dot_core::vault`; nunca de lo que diga el JavaScript. Ninguna contraseña se
//! escribe en logs: [`Password`] oculta su `Debug` y se pone a cero al liberarse.

use crate::browser_view::{BrowserCallbacks, BrowserView, VaultEvent, VaultFrameMessage};
use crate::settings::SettingsStore;
use crate::vault_ui::{AccountsRequest, OfferKind, OfferRequest, VaultWindowUi};
use dot_core::paths::DotPaths;
use dot_core::vault::{CaptureOffer, FillOffer, Password, Vault, VaultStatus};
use gnome_platform::glib::{self, SourceId};
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Tras navegar con una captura pendiente, cuánto esperar a que la página nueva diga si vuelve a
/// tener un formulario de login (inicio de sesión rechazado) antes de ofrecer guardar.
const SETTLE_AFTER_NAVIGATION: Duration = Duration::from_millis(1200);
/// Una captura más antigua que esto ya no se ofrece.
const CAPTURE_MAX_AGE: Duration = Duration::from_secs(120);
/// Reutiliza la última lista de cuentas de un marco durante este tiempo (evita consultar el
/// llavero en cada desplazamiento de la página).
const FILL_OFFER_CACHE: Duration = Duration::from_secs(15);
/// Longitud máxima aceptada de un usuario enviado por la página.
const MAX_USERNAME_CHARS: usize = 512;

type Job = Box<dyn FnOnce(&Vault) + Send>;
type Done = Box<dyn FnOnce(Box<dyn Any + Send>)>;

thread_local! {
    static PENDING: RefCell<HashMap<u64, Done>> = RefCell::new(HashMap::new());
    static NEXT_JOB_ID: Cell<u64> = const { Cell::new(1) };
}

struct ServiceInner {
    tx: Option<mpsc::Sender<Job>>,
    status: RefCell<Option<VaultStatus>>,
    paths: DotPaths,
}

/// Acceso compartido (hilo principal) al Vault de la aplicación.
#[derive(Clone)]
pub struct VaultService {
    inner: Rc<ServiceInner>,
}

impl VaultService {
    /// Arranca el hilo de Vault. Abrir el Secret Service ocurre allí, sin retrasar el arranque.
    pub fn start(paths: DotPaths) -> Self {
        let (tx, rx) = mpsc::channel::<Job>();
        let worker_paths = paths.clone();
        let spawned = std::thread::Builder::new()
            .name("search-vault".into())
            .spawn(move || {
                let vault = Vault::open_default(&worker_paths);
                while let Ok(job) = rx.recv() {
                    job(&vault);
                }
            });
        let (tx, status) = match spawned {
            Ok(_) => (Some(tx), None),
            Err(err) => {
                dot_core::error!("Vault: no se pudo crear el hilo de trabajo: {err}");
                (
                    None,
                    Some(VaultStatus::Disabled {
                        reason: "no se pudo iniciar el hilo de Vault".into(),
                    }),
                )
            }
        };
        let service = Self {
            inner: Rc::new(ServiceInner {
                tx,
                status: RefCell::new(status),
                paths,
            }),
        };
        let weak = Rc::downgrade(&service.inner);
        service.run(
            |vault| vault.status(),
            move |status| {
                if let Some(inner) = weak.upgrade() {
                    *inner.status.borrow_mut() = Some(status);
                }
            },
        );
        service
    }

    /// Ejecuta `job` en el hilo de Vault y después `done` con su resultado en el hilo principal.
    pub fn run<T, J, D>(&self, job: J, done: D)
    where
        T: Send + 'static,
        J: FnOnce(&Vault) -> T + Send + 'static,
        D: FnOnce(T) + 'static,
    {
        let Some(tx) = self.inner.tx.as_ref() else {
            return;
        };
        let id = NEXT_JOB_ID.with(|n| {
            let id = n.get();
            n.set(id.wrapping_add(1));
            id
        });
        let done: Done = Box::new(move |any| {
            if let Ok(value) = any.downcast::<T>() {
                done(*value);
            }
        });
        PENDING.with(|p| p.borrow_mut().insert(id, done));
        let job: Job = Box::new(move |vault| {
            let out: Box<dyn Any + Send> = Box::new(job(vault));
            glib::idle_add_once(move || {
                let cb = PENDING.with(|p| p.borrow_mut().remove(&id));
                if let Some(cb) = cb {
                    cb(out);
                }
            });
        });
        if tx.send(job).is_err() {
            PENDING.with(|p| p.borrow_mut().remove(&id));
        }
    }

    /// `false` solo si ya sabemos que Vault está desactivado.
    pub fn may_be_enabled(&self) -> bool {
        !matches!(
            *self.inner.status.borrow(),
            Some(VaultStatus::Disabled { .. })
        )
    }

    pub fn paths(&self) -> &DotPaths {
        &self.inner.paths
    }
}

// ---------------------------------------------------------------------------------------------
// Mensajes de `vault.js`
// ---------------------------------------------------------------------------------------------

/// Rectángulo de un campo en píxeles CSS del marco principal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Mensaje decodificado de `vault.js`. `Debug` no revela la contraseña.
#[derive(Debug)]
pub enum VaultIpc {
    /// Hay un formulario de login; `focused` si el cursor está en su usuario o contraseña.
    Forms {
        focused: bool,
        rect: Option<FieldRect>,
    },
    /// El cursor salió del campo de login.
    Blur,
    /// Se acaba de enviar un formulario de login.
    Capture {
        username: String,
        password: Password,
    },
    /// El formulario desapareció sin navegar tras un envío (inicio de sesión dentro de la página).
    Settled,
}

fn parse_rect(value: Option<&serde_json::Value>) -> Option<FieldRect> {
    let obj = value?.as_object()?;
    let num = |k: &str| obj.get(k).and_then(serde_json::Value::as_f64);
    let rect = FieldRect {
        x: num("x")?,
        y: num("y")?,
        w: num("w")?,
        h: num("h")?,
    };
    let sane = |v: f64| v.is_finite() && v.abs() < 100_000.0;
    if sane(rect.x) && sane(rect.y) && sane(rect.w) && sane(rect.h) && rect.w > 0.0 && rect.h > 0.0
    {
        Some(rect)
    } else {
        None
    }
}

/// Decodifica un mensaje JSON de `vault.js`. Devuelve `None` si no es válido.
pub fn parse_ipc(json: &str) -> Option<VaultIpc> {
    let mut value: serde_json::Value = serde_json::from_str(json).ok()?;
    let obj = value.as_object_mut()?;
    let kind = obj.get("kind")?.as_str()?.to_owned();
    match kind.as_str() {
        "vault-capture" => {
            let password = match obj.remove("password") {
                Some(serde_json::Value::String(s)) => Password::new(s),
                _ => return None,
            };
            if password.is_empty() {
                return None;
            }
            let username = match obj.remove("username") {
                Some(serde_json::Value::String(s)) => {
                    s.trim().chars().take(MAX_USERNAME_CHARS).collect()
                }
                _ => String::new(),
            };
            Some(VaultIpc::Capture { username, password })
        }
        "vault-forms" => {
            let rect = parse_rect(obj.get("rect"));
            let focused = obj
                .get("focused")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
                && rect.is_some();
            Some(VaultIpc::Forms { focused, rect })
        }
        "vault-blur" => Some(VaultIpc::Blur),
        "vault-settled" => Some(VaultIpc::Settled),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------
// Estado por pestaña
// ---------------------------------------------------------------------------------------------

struct PendingCapture {
    page_url: String,
    frame_url: String,
    username: String,
    password: Password,
    at: Instant,
    navigated: bool,
}

struct CachedOffer {
    frame_id: String,
    page_url: String,
    frame_url: String,
    at: Instant,
    offer: FillOffer,
}

struct TabVault {
    tab_id: u64,
    service: VaultService,
    ui: VaultWindowUi,
    settings: SettingsStore,
    view: BrowserView,
    pending: RefCell<Option<PendingCapture>>,
    settle_timer: Cell<Option<SourceId>>,
    fill_seq: Cell<u64>,
    cached_offer: RefCell<Option<CachedOffer>>,
}

/// Conecta Vault a una pestaña: activa la inyección de `vault.js` y enruta sus mensajes.
pub fn attach(
    view: &BrowserView,
    callbacks: &mut BrowserCallbacks,
    tab_id: u64,
    service: &VaultService,
    ui: &VaultWindowUi,
    settings: &SettingsStore,
) {
    view.set_vault_enabled(service.may_be_enabled());
    let tab = Rc::new(TabVault {
        tab_id,
        service: service.clone(),
        ui: ui.clone(),
        settings: settings.clone(),
        view: view.clone(),
        pending: RefCell::new(None),
        settle_timer: Cell::new(None),
        fill_seq: Cell::new(0),
        cached_offer: RefCell::new(None),
    });
    callbacks.on_vault_event = Some(Box::new(move |event| tab.handle(event)));
}

impl TabVault {
    fn handle(self: &Rc<Self>, event: VaultEvent) {
        match event {
            VaultEvent::Message(message) => self.handle_message(message),
            VaultEvent::MainFrameLoaded => self.handle_navigation(),
        }
    }

    fn handle_message(self: &Rc<Self>, message: VaultFrameMessage) {
        if !self.service.may_be_enabled() {
            self.view.set_vault_enabled(false);
            return;
        }
        let Some(ipc) = parse_ipc(&message.payload) else {
            return;
        };
        match ipc {
            VaultIpc::Capture { username, password } => {
                if !self.settings.with(|s| s.vault_save) {
                    return;
                }
                self.cancel_settle_timer();
                *self.pending.borrow_mut() = Some(PendingCapture {
                    page_url: message.page_url,
                    frame_url: message.frame_url,
                    username,
                    password,
                    at: Instant::now(),
                    navigated: false,
                });
            }
            VaultIpc::Settled => self.offer_pending(),
            VaultIpc::Forms { focused, rect } => {
                // Tras navegar, la página nueva vuelve a pedir la contraseña: el inicio de
                // sesión no funcionó y no se ofrece guardar lo que se tecleó.
                if message.is_main {
                    let refused = self.pending.borrow().as_ref().is_some_and(|p| {
                        p.navigated && same_origin(&p.frame_url, &message.frame_url)
                    });
                    if refused {
                        self.cancel_settle_timer();
                        self.pending.borrow_mut().take();
                    }
                }
                match (focused, rect) {
                    (true, Some(rect)) => self.offer_accounts(&message, rect),
                    _ => self.ui.hide_accounts(self.tab_id),
                }
            }
            VaultIpc::Blur => self.ui.hide_accounts(self.tab_id),
        }
    }

    fn handle_navigation(self: &Rc<Self>) {
        self.ui.hide_accounts(self.tab_id);
        self.cached_offer.borrow_mut().take();
        let schedule = match self.pending.borrow_mut().as_mut() {
            Some(p) if !p.navigated => {
                p.navigated = true;
                true
            }
            _ => false,
        };
        if schedule {
            self.cancel_settle_timer();
            let weak = Rc::downgrade(self);
            let id = glib::timeout_add_local_once(SETTLE_AFTER_NAVIGATION, move || {
                if let Some(tab) = weak.upgrade() {
                    tab.settle_timer.set(None);
                    tab.offer_pending();
                }
            });
            self.settle_timer.set(Some(id));
        }
    }

    fn cancel_settle_timer(&self) {
        if let Some(id) = self.settle_timer.take() {
            id.remove();
        }
    }

    /// Pregunta a Vault qué ofrecer para la captura pendiente y, si procede, muestra la barra.
    fn offer_pending(self: &Rc<Self>) {
        self.cancel_settle_timer();
        let Some(pending) = self.pending.borrow_mut().take() else {
            return;
        };
        if pending.at.elapsed() > CAPTURE_MAX_AGE || !self.settings.with(|s| s.vault_save) {
            return;
        }
        let PendingCapture {
            page_url,
            frame_url,
            username,
            password,
            ..
        } = pending;
        let ui = self.ui.clone();
        let service = self.service.clone();
        let tab_id = self.tab_id;
        self.service.run(
            move |vault| {
                let offer = vault.capture_offer(&page_url, &frame_url, &username, &password);
                (offer, username, password)
            },
            move |(offer, username, password)| {
                let (kind, origin, insecure) = match offer {
                    Ok(CaptureOffer::Save {
                        origin, insecure, ..
                    }) => (OfferKind::Save, origin, insecure),
                    Ok(CaptureOffer::Update {
                        origin, insecure, ..
                    }) => (OfferKind::Update, origin, insecure),
                    Ok(CaptureOffer::Nothing) => return,
                    Err(err) => {
                        dot_core::warn!("Vault: no se pudo preparar la oferta de guardado: {err}");
                        return;
                    }
                };
                ui.show_offer(
                    &service,
                    OfferRequest {
                        tab_id,
                        kind,
                        origin,
                        username: username.trim().to_owned(),
                        password,
                        insecure,
                    },
                );
            },
        );
    }

    /// Muestra las cuentas del origin exacto del marco junto al campo enfocado.
    fn offer_accounts(self: &Rc<Self>, message: &VaultFrameMessage, rect: FieldRect) {
        if !self.settings.with(|s| s.vault_fill) {
            return;
        }
        let seq = self.fill_seq.get().wrapping_add(1);
        self.fill_seq.set(seq);

        let cached = self.cached_offer.borrow().as_ref().and_then(|c| {
            (c.frame_id == message.frame_id
                && c.page_url == message.page_url
                && c.frame_url == message.frame_url
                && c.at.elapsed() < FILL_OFFER_CACHE)
                .then(|| c.offer.clone())
        });
        if let Some(offer) = cached {
            self.present_accounts(message.frame_id.clone(), rect, offer);
            return;
        }

        let page_url = message.page_url.clone();
        let frame_url = message.frame_url.clone();
        let frame_id = message.frame_id.clone();
        let weak = Rc::downgrade(self);
        self.service.run(
            {
                let page_url = page_url.clone();
                let frame_url = frame_url.clone();
                move |vault| vault.fill_offer(&page_url, &frame_url)
            },
            move |result| {
                let Some(tab) = weak.upgrade() else {
                    return;
                };
                if tab.fill_seq.get() != seq {
                    return;
                }
                let offer = match result {
                    Ok(offer) => offer,
                    Err(err) => {
                        dot_core::debug!("Vault: sin cuentas que ofrecer: {err}");
                        return;
                    }
                };
                *tab.cached_offer.borrow_mut() = Some(CachedOffer {
                    frame_id: frame_id.clone(),
                    page_url,
                    frame_url,
                    at: Instant::now(),
                    offer: offer.clone(),
                });
                tab.present_accounts(frame_id, rect, offer);
            },
        );
    }

    fn present_accounts(self: &Rc<Self>, frame_id: String, rect: FieldRect, offer: FillOffer) {
        if offer.accounts.is_empty() {
            self.ui.hide_accounts(self.tab_id);
            return;
        }
        let weak = Rc::downgrade(self);
        let pick_frame = frame_id.clone();
        self.ui.show_accounts(
            &self.view,
            AccountsRequest {
                tab_id: self.tab_id,
                rect,
                origin: offer.origin.clone(),
                insecure: offer.insecure,
                accounts: offer.accounts,
            },
            Box::new(move |username: String| {
                if let Some(tab) = weak.upgrade() {
                    tab.fill(&pick_frame, username);
                }
            }),
        );
    }

    /// Rellena la cuenta elegida, revalidando el origin del marco antes y después de leer el
    /// secreto del llavero.
    fn fill(self: &Rc<Self>, frame_id: &str, username: String) {
        self.ui.hide_accounts(self.tab_id);
        self.cached_offer.borrow_mut().take();
        let Some((page_url, frame_url)) = self.view.vault_frame_urls(frame_id) else {
            return;
        };
        let view = self.view.clone();
        let frame_id = frame_id.to_owned();
        self.service.run(
            move |vault| {
                let origin =
                    dot_core::vault::frame_origin(&page_url, &frame_url).map(|o| o.to_string());
                let password = vault.fill_password(&page_url, &frame_url, &username);
                (origin, username, password)
            },
            move |(origin, username, password)| match (origin, password) {
                (Ok(origin), Ok(Some(password))) => {
                    if !view.vault_fill(&frame_id, &origin, &username, password.expose()) {
                        dot_core::debug!("Vault: el marco cambió antes de rellenar");
                    }
                }
                (Ok(_), Ok(None)) => {}
                (Err(err), _) | (_, Err(err)) => {
                    dot_core::warn!("Vault: no se pudo rellenar: {err}");
                }
            },
        );
    }
}

fn same_origin(a: &str, b: &str) -> bool {
    match (
        dot_core::vault::Origin::parse(a),
        dot_core::vault::Origin::parse(b),
    ) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_capture_and_trims_username() {
        let msg = r#"{"kind":"vault-capture","username":"  ana@example.com ","password":"s3cr3t"}"#;
        match parse_ipc(msg) {
            Some(VaultIpc::Capture { username, password }) => {
                assert_eq!(username, "ana@example.com");
                assert_eq!(password.expose(), "s3cr3t");
            }
            other => panic!("inesperado: {other:?}"),
        }
    }

    #[test]
    fn rejects_capture_without_password() {
        assert!(parse_ipc(r#"{"kind":"vault-capture","username":"ana","password":""}"#).is_none());
        assert!(parse_ipc(r#"{"kind":"vault-capture","username":"ana"}"#).is_none());
        assert!(parse_ipc(r#"{"kind":"vault-capture","password":42}"#).is_none());
    }

    #[test]
    fn debug_never_reveals_password() {
        let msg = r#"{"kind":"vault-capture","username":"ana","password":"hunter2-unico"}"#;
        let parsed = parse_ipc(msg);
        let printed = format!("{parsed:?}");
        assert!(!printed.contains("hunter2-unico"));
    }

    #[test]
    fn parses_forms_with_and_without_rect() {
        match parse_ipc(
            r#"{"kind":"vault-forms","focused":true,"rect":{"x":10,"y":20.5,"w":200,"h":30}}"#,
        ) {
            Some(VaultIpc::Forms { focused, rect }) => {
                assert!(focused);
                assert_eq!(
                    rect,
                    Some(FieldRect {
                        x: 10.0,
                        y: 20.5,
                        w: 200.0,
                        h: 30.0
                    })
                );
            }
            other => panic!("inesperado: {other:?}"),
        }
        match parse_ipc(r#"{"kind":"vault-forms","focused":true,"rect":null}"#) {
            Some(VaultIpc::Forms { focused, rect }) => {
                assert!(!focused, "sin rectángulo no hay foco utilizable");
                assert!(rect.is_none());
            }
            other => panic!("inesperado: {other:?}"),
        }
    }

    #[test]
    fn rejects_absurd_rects_and_unknown_kinds() {
        let huge = r#"{"kind":"vault-forms","focused":true,"rect":{"x":1e12,"y":0,"w":10,"h":10}}"#;
        assert!(matches!(
            parse_ipc(huge),
            Some(VaultIpc::Forms {
                focused: false,
                rect: None
            })
        ));
        assert!(parse_ipc(r#"{"kind":"vault-origin","origin":"https://evil.example"}"#).is_none());
        assert!(parse_ipc("no es json").is_none());
        assert!(parse_ipc("[]").is_none());
    }

    #[test]
    fn parses_blur_and_settled() {
        assert!(matches!(
            parse_ipc(r#"{"kind":"vault-blur"}"#),
            Some(VaultIpc::Blur)
        ));
        assert!(matches!(
            parse_ipc(r#"{"kind":"vault-settled"}"#),
            Some(VaultIpc::Settled)
        ));
    }

    #[test]
    fn same_origin_is_exact() {
        assert!(same_origin(
            "https://a.example/login",
            "https://a.example/home"
        ));
        assert!(!same_origin("https://a.example/", "http://a.example/"));
        assert!(!same_origin("https://a.example/", "https://b.a.example/"));
        assert!(!same_origin(
            "https://a.example/",
            "https://a.example:8443/"
        ));
    }

    #[test]
    fn vault_js_keeps_token_placeholder_and_bridge_prefix() {
        let js = include_str!("../resources/vault.js");
        assert!(js.contains("'__search_ipc___SEARCH_VAULT_TOKEN__:'"));
        // El script nunca debe registrar nada por su cuenta.
        assert!(!js.contains("console.log"));
        assert!(!js.contains("console.error"));
    }
}
