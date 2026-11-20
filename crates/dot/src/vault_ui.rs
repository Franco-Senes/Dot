//! Interfaz de Vault (`PageQuestions.swift::PasswordOffer`, `Accounts.swift`, `Passwords.swift`
//! e `ImportPanel.swift`):
//! - Barra discreta flotante «¿Guardar la contraseña de usuario@sitio?» → Guardar / Nunca en este
//!   sitio / Ahora no (y la variante «¿Actualizar…?» cuando cambia la contraseña).
//! - Popover de cuentas junto al campo de login, solo del origin exacto (con aviso en HTTP).
//! - Gestor de contraseñas (`Ctrl+Alt+L`): búsqueda, mostrar con confirmación, copiar (el
//!   portapapeles se vacía a los 30 s si sigue teniendo nuestra copia), editar, borrar y exportar
//!   a CSV con advertencia clara.
//! - Importación desde Firefox, Chromium/Chrome/Brave/Edge/Vivaldi o CSV, solo a petición del
//!   usuario y enseñando antes los perfiles encontrados.
//!
//! Nunca se escribe una contraseña en logs; solo se muestra en una etiqueta efímera (15 s) tras
//! confirmación explícita.

use crate::browser_view::BrowserView;
use crate::vault::{FieldRect, VaultService};
use dot_core::shield::sha256_hex;
use dot_core::vault::{CredentialMeta, Origin, Password, VaultError, VaultStatus};
use dot_core::vault_import::{self, BrowserProfile, ChromiumKeys, ImportError, ProfileKind};
use gnome_platform::adw::{
    ActionRow, AlertDialog, ApplicationWindow, Dialog, HeaderBar, ResponseAppearance, SwitchRow,
    ToolbarView,
};
use gnome_platform::gdk::{ContentProvider, Display, Rectangle};
use gnome_platform::glib;
use gnome_platform::gtk::{
    self as gtk, Align, AsEditable, AsWidget, BoxWidget, Button, EllipsizeMode, Entry, Label,
    ListBox, Orientation, PasswordEntry, PolicyType, Popover, PositionType, ProgressBar, Revealer,
    RevealerTransitionType, Scale, ScrolledWindow, SearchEntry, SelectionMode, Separator,
};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;
use zeroize::Zeroizing;

/// El portapapeles se vacía pasado este tiempo si aún contiene la contraseña copiada.
const CLIPBOARD_CLEAR_AFTER: Duration = Duration::from_secs(30);
/// Una contraseña mostrada vuelve a ocultarse pasado este tiempo (como `Passwords.swift`).
const REVEAL_FOR: Duration = Duration::from_secs(15);
/// Siempre el mismo número de puntos: no delatar la longitud.
const MASK: &str = "••••••••••";

/// Texto corto del sitio: el host si es HTTPS en el puerto estándar; si no, el origin entero.
pub fn site_label(origin: &Origin) -> String {
    if origin.is_https() && origin.port() == 443 {
        origin.host().to_owned()
    } else {
        origin.to_string()
    }
}

// ---------------------------------------------------------------------------------------------
// Barra de oferta y popover de cuentas (por ventana)
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferKind {
    Save,
    Update,
}

/// Petición de mostrar la barra «¿Guardar…?» para una pestaña.
pub struct OfferRequest {
    pub tab_id: u64,
    pub kind: OfferKind,
    pub origin: Origin,
    pub username: String,
    pub password: Password,
    pub insecure: bool,
}

/// Petición de mostrar las cuentas junto a un campo de login.
pub struct AccountsRequest {
    pub tab_id: u64,
    pub rect: FieldRect,
    pub origin: Origin,
    pub insecure: bool,
    pub accounts: Vec<CredentialMeta>,
}

pub type PickCallback = Box<dyn Fn(String)>;

struct OfferState {
    request: OfferRequest,
    service: VaultService,
}

struct AccountsState {
    tab_id: u64,
    key: String,
    popover: Popover,
}

struct UiInner {
    revealer: Revealer,
    label: Label,
    primary: Button,
    never: Button,
    offer: RefCell<Option<OfferState>>,
    accounts: RefCell<Option<AccountsState>>,
    selected_tab: Cell<Option<u64>>,
}

/// Interfaz de Vault de una ventana: barra de oferta (superpuesta sobre la página) y popover de
/// cuentas.
#[derive(Clone)]
pub struct VaultWindowUi {
    inner: Rc<UiInner>,
}

impl Default for VaultWindowUi {
    fn default() -> Self {
        Self::new()
    }
}

impl VaultWindowUi {
    pub fn new() -> Self {
        let label = Label::new(None);
        label.set_wrap(true);
        label.set_xalign(0.0);
        label.set_max_width_chars(60);

        let primary = Button::with_label("Guardar");
        primary.add_css_class("suggested-action");
        let never = Button::with_label("Nunca en este sitio");
        never.add_css_class("flat");
        let later = Button::with_label("Ahora no");
        later.add_css_class("flat");

        let bar = BoxWidget::new(Orientation::Horizontal, 8);
        bar.add_css_class("toolbar");
        bar.add_css_class("osd");
        bar.set_margins(8, 8, 8, 8);
        bar.append(&gtk::Image::from_icon_name("dialog-password-symbolic"));
        bar.append(&label);
        bar.append(&primary);
        bar.append(&never);
        bar.append(&later);

        let revealer = Revealer::new();
        revealer.set_transition_type(RevealerTransitionType::SlideDown);
        revealer.set_transition_duration(140);
        revealer.set_halign(Align::Center);
        revealer.set_valign(Align::Start);
        revealer.set_child(Some(&bar));
        revealer.set_reveal_child(false);

        let ui = Self {
            inner: Rc::new(UiInner {
                revealer,
                label,
                primary: primary.clone(),
                never: never.clone(),
                offer: RefCell::new(None),
                accounts: RefCell::new(None),
                selected_tab: Cell::new(None),
            }),
        };

        {
            let weak = Rc::downgrade(&ui.inner);
            primary.connect_clicked(move || {
                if let Some(inner) = weak.upgrade() {
                    VaultWindowUi { inner }.answer_offer(OfferAnswer::Keep);
                }
            });
        }
        {
            let weak = Rc::downgrade(&ui.inner);
            never.connect_clicked(move || {
                if let Some(inner) = weak.upgrade() {
                    VaultWindowUi { inner }.answer_offer(OfferAnswer::Never);
                }
            });
        }
        {
            let weak = Rc::downgrade(&ui.inner);
            later.connect_clicked(move || {
                if let Some(inner) = weak.upgrade() {
                    VaultWindowUi { inner }.answer_offer(OfferAnswer::NotNow);
                }
            });
        }
        ui
    }

    /// Widget de la barra para superponerlo sobre la página (`GtkOverlay`).
    pub fn offer_widget(&self) -> &Revealer {
        &self.inner.revealer
    }

    /// Muestra «¿Guardar/Actualizar la contraseña de usuario@sitio?».
    pub fn show_offer(&self, service: &VaultService, request: OfferRequest) {
        let site = site_label(&request.origin);
        let who = if request.username.is_empty() {
            site.clone()
        } else {
            format!("{}@{}", request.username, site)
        };
        let mut text = match request.kind {
            OfferKind::Save if request.username.is_empty() => {
                format!("¿Guardar la contraseña de {site}?")
            }
            OfferKind::Save => format!("¿Guardar la contraseña de {who}?"),
            OfferKind::Update => format!("¿Actualizar la contraseña de {who}?"),
        };
        if request.insecure {
            text.push_str(" Atención: este sitio no usa una conexión cifrada (HTTP).");
        }
        self.inner.label.set_text(&text);
        self.inner.primary.set_label(match request.kind {
            OfferKind::Save => "Guardar",
            OfferKind::Update => "Actualizar",
        });
        self.inner
            .never
            .set_visible(request.kind == OfferKind::Save);
        let visible = self
            .inner
            .selected_tab
            .get()
            .is_none_or(|t| t == request.tab_id);
        *self.inner.offer.borrow_mut() = Some(OfferState {
            request,
            service: service.clone(),
        });
        self.inner.revealer.set_reveal_child(visible);
    }

    fn answer_offer(&self, answer: OfferAnswer) {
        self.inner.revealer.set_reveal_child(false);
        let Some(OfferState { request, service }) = self.inner.offer.borrow_mut().take() else {
            return;
        };
        let OfferRequest {
            origin,
            username,
            password,
            ..
        } = request;
        match answer {
            OfferAnswer::Keep => service.run(
                move |vault| vault.save(&origin, &username, &password),
                |result| {
                    if let Err(err) = result {
                        dot_core::warn!("Vault: no se pudo guardar la contraseña: {err}");
                    }
                },
            ),
            OfferAnswer::Never => service.run(
                move |vault| vault.never(&origin),
                |result| {
                    if let Err(err) = result {
                        dot_core::warn!("Vault: no se pudo recordar «Nunca»: {err}");
                    }
                },
            ),
            OfferAnswer::NotNow => {}
        }
    }

    /// La pestaña activa cambió: la barra sigue a su pestaña y el popover se cierra.
    pub fn on_tab_selected(&self, tab_id: u64) {
        self.inner.selected_tab.set(Some(tab_id));
        let ours = self
            .inner
            .offer
            .borrow()
            .as_ref()
            .is_some_and(|o| o.request.tab_id == tab_id);
        self.inner.revealer.set_reveal_child(ours);
        self.close_accounts();
    }

    /// `Esc`: cierra primero el popover de cuentas y después la barra («Ahora no»).
    pub fn dismiss(&self) -> bool {
        if self.inner.accounts.borrow().is_some() {
            self.close_accounts();
            return true;
        }
        if self.inner.revealer.reveals_child() {
            self.answer_offer(OfferAnswer::NotNow);
            return true;
        }
        false
    }

    /// Muestra (o recoloca) el popover de cuentas bajo el campo enfocado.
    pub fn show_accounts(
        &self,
        view: &BrowserView,
        request: AccountsRequest,
        on_pick: PickCallback,
    ) {
        if self
            .inner
            .selected_tab
            .get()
            .is_some_and(|t| t != request.tab_id)
        {
            return;
        }
        let zoom = view.zoom_factor().clamp(0.25, 5.0);
        let rect = Rectangle {
            x: (request.rect.x * zoom).round() as i32,
            y: (request.rect.y * zoom).round() as i32,
            width: ((request.rect.w * zoom).round() as i32).max(1),
            height: ((request.rect.h * zoom).round() as i32).max(1),
        };
        let mut key = format!("{}|{}", request.origin, request.insecure);
        for account in &request.accounts {
            key.push('|');
            key.push_str(&account.username);
        }

        if let Some(state) = self.inner.accounts.borrow().as_ref() {
            if state.tab_id == request.tab_id && state.key == key {
                state.popover.set_pointing_to(Some(rect));
                return;
            }
        }
        self.close_accounts();

        let content = BoxWidget::new(Orientation::Vertical, 2);
        content.set_margins(4, 4, 4, 4);
        content.set_size_request(260, -1);
        if request.insecure {
            let warn = Label::new(Some(
                "Conexión sin cifrar (HTTP): la contraseña viajará sin protección.",
            ));
            warn.set_wrap(true);
            warn.set_xalign(0.0);
            warn.set_max_width_chars(36);
            warn.add_css_class("warning");
            warn.add_css_class("caption");
            warn.set_margins(4, 6, 8, 8);
            content.append(&warn);
        }
        let site = site_label(&request.origin);
        let on_pick: Rc<dyn Fn(String)> = Rc::from(on_pick);
        for account in &request.accounts {
            let row = BoxWidget::new(Orientation::Vertical, 0);
            let name = Label::new(Some(if account.username.is_empty() {
                "Sin usuario"
            } else {
                &account.username
            }));
            name.set_xalign(0.0);
            name.set_ellipsize(EllipsizeMode::Middle);
            let detail = Label::new(Some(&site));
            detail.set_xalign(0.0);
            detail.set_ellipsize(EllipsizeMode::End);
            detail.add_css_class("dim-label");
            detail.add_css_class("caption");
            row.append(&name);
            row.append(&detail);

            let button = Button::with_label("");
            button.set_child(Some(&row));
            button.add_css_class("flat");
            // Sin foco: el cursor debe seguir en el campo de la página.
            button.set_focusable(false);
            gtk::set_focus_on_click(&button, false);
            let pick = on_pick.clone();
            let username = account.username.clone();
            button.connect_clicked(move || pick(username.clone()));
            content.append(&button);
        }
        let footer = Label::new(Some("Desde tu llavero"));
        footer.set_xalign(0.0);
        footer.add_css_class("dim-label");
        footer.add_css_class("caption");
        footer.set_margins(4, 4, 8, 8);
        content.append(&footer);

        let popover = Popover::new();
        popover.set_autohide(false);
        popover.set_has_arrow(false);
        popover.set_position(PositionType::Bottom);
        popover.set_child(Some(&content));
        popover.set_parent(view.widget());
        popover.set_pointing_to(Some(rect));
        popover.popup();
        *self.inner.accounts.borrow_mut() = Some(AccountsState {
            tab_id: request.tab_id,
            key,
            popover,
        });
    }

    /// Cierra el popover de cuentas si pertenece a `tab_id`.
    pub fn hide_accounts(&self, tab_id: u64) {
        let ours = self
            .inner
            .accounts
            .borrow()
            .as_ref()
            .is_some_and(|s| s.tab_id == tab_id);
        if ours {
            self.close_accounts();
        }
    }

    fn close_accounts(&self) {
        let state = self.inner.accounts.borrow_mut().take();
        if let Some(state) = state {
            state.popover.popdown();
            state.popover.unparent();
        }
    }
}

enum OfferAnswer {
    Keep,
    Never,
    NotNow,
}

// ---------------------------------------------------------------------------------------------
// Utilidades comunes
// ---------------------------------------------------------------------------------------------

fn alert(parent: &ApplicationWindow, heading: &str, body: &str) {
    let dialog = AlertDialog::new(heading, body);
    dialog.add_response("ok", "Aceptar");
    dialog.set_default_response("ok");
    dialog.set_close_response("ok");
    dialog.present(Some(parent));
}

/// Confirmación con una respuesta destacada (`id = "go"`).
fn confirm(
    parent: &ApplicationWindow,
    heading: &str,
    body: &str,
    action: &str,
    appearance: ResponseAppearance,
    on_yes: impl Fn() + 'static,
) {
    let dialog = AlertDialog::new(heading, body);
    dialog.add_response("cancel", "Cancelar");
    dialog.add_response("go", action);
    dialog.set_response_appearance("go", appearance);
    dialog.set_default_response("cancel");
    dialog.set_close_response("cancel");
    dialog.connect_response(move |id| {
        if id == "go" {
            on_yes();
        }
    });
    dialog.present(Some(parent));
}

/// Escribe un archivo solo legible por el usuario (`0600`), truncándolo si existía.
fn write_private_file(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    file.write_all(data)?;
    file.sync_all()
}

/// Copia un secreto al portapapeles y lo borra pasado `CLIPBOARD_CLEAR_AFTER` solo si el
/// portapapeles aún contiene exactamente nuestra copia.
fn copy_secret_to_clipboard(password: &Password) -> bool {
    let Some(display) = Display::default_display() else {
        return false;
    };
    let provider = ContentProvider::for_secret_string(password.expose());
    if !display.set_clipboard_content(&provider) {
        return false;
    }
    glib::timeout_add_local_once(CLIPBOARD_CLEAR_AFTER, move || {
        if let Some(display) = Display::default_display() {
            if display.clipboard_holds(&provider) {
                display.clear_clipboard();
            }
        }
    });
    true
}

fn user_label(username: &str) -> &str {
    if username.is_empty() {
        "Sin usuario"
    } else {
        username
    }
}

// ---------------------------------------------------------------------------------------------
// Generador CSPRNG y salud del llavero
// ---------------------------------------------------------------------------------------------

const LOWERCASE_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
const UPPERCASE_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGIT_CHARS: &[u8] = b"0123456789";
const SYMBOL_CHARS: &[u8] = b"!@#$%^&*()-_=+[]{};:,.?/";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GeneratorOptions {
    length: usize,
    uppercase: bool,
    digits: bool,
    symbols: bool,
}

impl Default for GeneratorOptions {
    fn default() -> Self {
        Self {
            length: 20,
            uppercase: true,
            digits: true,
            symbols: true,
        }
    }
}

fn csprng_fill(buf: &mut [u8]) {
    let mut offset = 0;
    while offset < buf.len() {
        let rc =
            unsafe { libc::getrandom(buf[offset..].as_mut_ptr().cast(), buf.len() - offset, 0) };
        if rc > 0 {
            offset += rc as usize;
            continue;
        }
        let err = std::io::Error::last_os_error();
        if err.kind() == std::io::ErrorKind::Interrupted {
            continue;
        }
        // Respaldo con /dev/urandom si getrandom no estuviera disponible.
        if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
            use std::io::Read;
            if f.read_exact(&mut buf[offset..]).is_ok() {
                return;
            }
        }
        panic!("No se pudo obtener entropía CSPRNG del sistema: {err}");
    }
}

/// Índice uniforme sin sesgo en `0..bound` mediante muestreo por rechazo.
fn csprng_index(bound: usize) -> usize {
    debug_assert!(bound > 0);
    if bound <= 1 {
        return 0;
    }
    let bound_u64 = bound as u64;
    let limit = u64::MAX - (u64::MAX % bound_u64);
    let mut bytes = [0u8; 8];
    loop {
        csprng_fill(&mut bytes);
        let val = u64::from_le_bytes(bytes);
        if val < limit {
            return (val % bound_u64) as usize;
        }
    }
}

fn generate_password(opts: GeneratorOptions) -> Zeroizing<String> {
    let length = opts.length.clamp(12, 48);
    let mut classes: Vec<&[u8]> = vec![LOWERCASE_CHARS];
    if opts.uppercase {
        classes.push(UPPERCASE_CHARS);
    }
    if opts.digits {
        classes.push(DIGIT_CHARS);
    }
    if opts.symbols {
        classes.push(SYMBOL_CHARS);
    }

    let mut pool = Vec::with_capacity(96);
    for cls in &classes {
        pool.extend_from_slice(cls);
    }

    let mut chars = Vec::with_capacity(length);
    // Garantizar al menos un carácter de cada clase activada.
    for cls in &classes {
        chars.push(cls[csprng_index(cls.len())]);
    }
    while chars.len() < length {
        chars.push(pool[csprng_index(pool.len())]);
    }

    // Mezcla Fisher-Yates con CSPRNG.
    for i in (1..chars.len()).rev() {
        let j = csprng_index(i + 1);
        chars.swap(i, j);
    }

    let s = String::from_utf8(chars).unwrap_or_default();
    Zeroizing::new(s)
}

fn password_entropy_bits(password: &str) -> f64 {
    let mut has_lower = false;
    let mut has_upper = false;
    let mut has_digit = false;
    let mut has_symbol = false;
    let mut char_count = 0usize;

    for ch in password.chars() {
        char_count += 1;
        if ch.is_ascii_lowercase() {
            has_lower = true;
        } else if ch.is_ascii_uppercase() {
            has_upper = true;
        } else if ch.is_ascii_digit() {
            has_digit = true;
        } else {
            has_symbol = true;
        }
    }

    if char_count == 0 {
        return 0.0;
    }

    let mut pool = 0usize;
    if has_lower {
        pool += LOWERCASE_CHARS.len();
    }
    if has_upper {
        pool += UPPERCASE_CHARS.len();
    }
    if has_digit {
        pool += DIGIT_CHARS.len();
    }
    if has_symbol {
        pool += SYMBOL_CHARS.len();
    }
    if pool <= 1 {
        return 0.0;
    }

    (char_count as f64) * (pool as f64).log2()
}

fn password_strength_summary(password: &str) -> (&'static str, f64, f64) {
    let bits = password_entropy_bits(password);
    let (label, fraction) = if bits < 50.0 {
        ("Débil", 0.25)
    } else if bits < 75.0 {
        ("Aceptable", 0.55)
    } else if bits < 105.0 {
        ("Fuerte", 0.8)
    } else {
        ("Muy fuerte", 1.0)
    };
    (label, fraction, bits)
}

fn is_weak_password(password: &str) -> bool {
    let char_count = password.chars().count();
    if char_count < 12 {
        return true;
    }
    let unique: HashSet<char> = password.chars().collect();
    if unique.len() < 4 {
        return true;
    }
    password_entropy_bits(password) < 50.0
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct PasswordHealthSummary {
    total: usize,
    reused: usize,
    weak: usize,
}

fn compute_vault_health(vault: &dot_core::vault::Vault) -> PasswordHealthSummary {
    let Ok(all) = vault.list("") else {
        return PasswordHealthSummary::default();
    };
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut hashes: Vec<String> = Vec::with_capacity(all.len());
    let mut weak = 0usize;
    let mut total = 0usize;

    for meta in &all {
        if let Ok(Some(pw)) = vault.password(&meta.origin, &meta.username) {
            total += 1;
            if is_weak_password(pw.expose()) {
                weak += 1;
            }
            let digest = sha256_hex(pw.expose().as_bytes());
            *counts.entry(digest.clone()).or_insert(0) += 1;
            hashes.push(digest);
        }
    }

    let reused = hashes
        .iter()
        .filter(|h| counts.get(*h).copied().unwrap_or(0) > 1)
        .count();

    PasswordHealthSummary {
        total,
        reused,
        weak,
    }
}

// ---------------------------------------------------------------------------------------------
// Gestor de contraseñas (`Ctrl+Alt+L`)
// ---------------------------------------------------------------------------------------------

struct PanelInner {
    parent: ApplicationWindow,
    service: VaultService,
    dialog: Dialog,
    search: SearchEntry,
    health_box: BoxWidget,
    health_icon: gtk::Image,
    health_label: Label,
    list: ListBox,
    count: Label,
    status: Label,
    disabled: Label,
    body: BoxWidget,
    footer: BoxWidget,
    refresh_seq: Cell<u64>,
}

#[derive(Clone)]
struct PasswordsPanel {
    inner: Rc<PanelInner>,
}

/// Referencia débil para las señales de los widgets del propio diálogo (sin ciclos).
#[derive(Clone)]
struct WeakPanel(std::rc::Weak<PanelInner>);

impl WeakPanel {
    fn upgrade(&self) -> Option<PasswordsPanel> {
        self.0.upgrade().map(|inner| PasswordsPanel { inner })
    }
}

/// Abre el gestor de contraseñas como diálogo sobre `parent`.
pub fn present_passwords(parent: &ApplicationWindow, service: &VaultService) {
    let panel = PasswordsPanel::build(parent, service);
    panel.refresh();
    panel.inner.dialog.present(Some(parent));
    panel.inner.search.grab_focus();
}

impl PasswordsPanel {
    fn build(parent: &ApplicationWindow, service: &VaultService) -> Self {
        let dialog = Dialog::new();
        dialog.set_title("Contraseñas");
        dialog.set_content_width(600);
        dialog.set_content_height(560);

        let search = SearchEntry::new();
        search.set_placeholder_text("Buscar sitios y cuentas");
        search.set_hexpand(true);

        let health_icon = gtk::Image::from_icon_name("security-high-symbolic");
        let health_label = Label::new(None);
        health_label.set_xalign(0.0);
        health_label.set_wrap(true);
        health_label.add_css_class("caption");
        let health_box = BoxWidget::new(Orientation::Horizontal, 6);
        health_box.set_margins(2, 2, 4, 4);
        health_box.append(&health_icon);
        health_box.append(&health_label);
        health_box.set_visible(false);

        let list = ListBox::new();
        list.set_selection_mode(SelectionMode::None);
        list.add_css_class("boxed-list");

        let scroller = ScrolledWindow::new();
        scroller.set_policy(PolicyType::Never, PolicyType::Automatic);
        scroller.set_vexpand(true);
        scroller.set_child(Some(&list));

        let disabled = Label::new(None);
        disabled.set_wrap(true);
        disabled.set_xalign(0.0);
        disabled.set_visible(false);
        disabled.add_css_class("error");

        let status = Label::new(None);
        status.set_wrap(true);
        status.set_xalign(0.0);
        status.add_css_class("dim-label");
        status.add_css_class("caption");
        status.set_visible(false);

        let count = Label::new(None);
        count.add_css_class("dim-label");
        count.set_hexpand(true);
        count.set_xalign(1.0);

        let gen_btn = Button::with_label("Generar contraseña");
        gen_btn.add_css_class("suggested-action");
        let import_btn = Button::with_label("Importar…");
        let export_btn = Button::with_label("Exportar CSV…");
        let footer = BoxWidget::new(Orientation::Horizontal, 8);
        footer.append(&gen_btn);
        footer.append(&import_btn);
        footer.append(&export_btn);
        footer.append(&count);

        let body = BoxWidget::new(Orientation::Vertical, 12);
        body.append(&search);
        body.append(&health_box);
        body.append(&scroller);
        body.append(&status);
        body.append(&footer);

        let content = BoxWidget::new(Orientation::Vertical, 12);
        content.set_margins(12, 12, 12, 12);
        content.append(&disabled);
        content.append(&body);

        let toolbar = ToolbarView::new();
        toolbar.add_top_bar(&HeaderBar::new());
        toolbar.set_content(Some(&content));
        dialog.set_child(Some(&toolbar));

        let panel = Self {
            inner: Rc::new(PanelInner {
                parent: parent.clone(),
                service: service.clone(),
                dialog,
                search: search.clone(),
                health_box,
                health_icon,
                health_label,
                list,
                count,
                status,
                disabled,
                body,
                footer,
                refresh_seq: Cell::new(0),
            }),
        };

        {
            let w = panel.weak();
            let anchor = gen_btn.clone();
            gen_btn.connect_clicked(move || {
                if let Some(p) = w.upgrade() {
                    p.open_generator_popover(&anchor);
                }
            });
        }

        {
            let w = panel.weak();
            search.connect_search_changed(move || {
                if let Some(p) = w.upgrade() {
                    p.refresh();
                }
            });
        }
        {
            let w = panel.weak();
            import_btn.connect_clicked(move || {
                let Some(p) = w.upgrade() else {
                    return;
                };
                let after = p.weak();
                present_import(
                    &p.inner.parent,
                    &p.inner.service,
                    Rc::new(move || {
                        if let Some(p) = after.upgrade() {
                            p.refresh();
                        }
                    }),
                );
            });
        }
        {
            let w = panel.weak();
            export_btn.connect_clicked(move || {
                if let Some(p) = w.upgrade() {
                    p.export();
                }
            });
        }
        {
            // El diálogo abierto mantiene vivo el panel; al cerrarse se suelta (sin ciclos).
            let keep = RefCell::new(Some(panel.clone()));
            panel.inner.dialog.connect_closed(move || {
                if let Some(panel) = keep.borrow_mut().take() {
                    panel.inner.list.remove_all();
                }
            });
        }
        panel
    }

    fn weak(&self) -> WeakPanel {
        WeakPanel(Rc::downgrade(&self.inner))
    }

    fn set_status(&self, text: &str) {
        self.inner.status.set_text(text);
        self.inner.status.set_visible(!text.is_empty());
    }

    fn refresh(&self) {
        let seq = self.inner.refresh_seq.get().wrapping_add(1);
        self.inner.refresh_seq.set(seq);
        let query = self.inner.search.text();
        let panel = self.clone();
        self.inner.service.run(
            move |vault| {
                let status = vault.status();
                let list = vault.list(&query);
                let health = if !matches!(status, VaultStatus::Disabled { .. }) {
                    compute_vault_health(vault)
                } else {
                    PasswordHealthSummary::default()
                };
                (status, list, health)
            },
            move |(status, list, health)| {
                if panel.inner.refresh_seq.get() != seq {
                    return;
                }
                panel.populate(status, list, health);
            },
        );
    }

    fn update_health_badge(&self, health: PasswordHealthSummary) {
        if health.total == 0 {
            self.inner.health_box.set_visible(false);
            return;
        }
        self.inner.health_label.remove_css_class("warning");
        self.inner.health_label.remove_css_class("dim-label");
        if health.reused == 0 && health.weak == 0 {
            self.inner
                .health_icon
                .set_from_icon_name(Some("security-high-symbolic"));
            self.inner
                .health_label
                .set_text("Salud del llavero: todas las contraseñas son fuertes y únicas");
            self.inner.health_label.add_css_class("dim-label");
        } else {
            self.inner
                .health_icon
                .set_from_icon_name(Some("dialog-warning-symbolic"));
            let mut parts = Vec::new();
            if health.reused > 0 {
                parts.push(if health.reused == 1 {
                    "1 reutilizada".to_string()
                } else {
                    format!("{} reutilizadas", health.reused)
                });
            }
            if health.weak > 0 {
                parts.push(if health.weak == 1 {
                    "1 débil".to_string()
                } else {
                    format!("{} débiles", health.weak)
                });
            }
            self.inner
                .health_label
                .set_text(&format!("Salud del llavero: {}", parts.join(" · ")));
            self.inner.health_label.add_css_class("warning");
        }
        self.inner.health_box.set_visible(true);
    }

    fn populate(
        &self,
        status: VaultStatus,
        list: Result<Vec<CredentialMeta>, VaultError>,
        health: PasswordHealthSummary,
    ) {
        if let VaultStatus::Disabled { reason } = status {
            self.inner.disabled.set_text(&format!(
                "Vault desactivado: no hay Secret Service.\n{reason}\n\nDot nunca guarda \
                 contraseñas en texto plano. Inicia un servicio de secretos (GNOME Keyring, \
                 KWallet o el portal de secretos) y vuelve a abrir Dot."
            ));
            self.inner.disabled.set_visible(true);
            self.inner.health_box.set_visible(false);
            self.inner.body.set_visible(false);
            self.inner.footer.set_visible(false);
            return;
        }
        self.inner.disabled.set_visible(false);
        self.inner.body.set_visible(true);
        self.inner.footer.set_visible(true);
        self.update_health_badge(health);
        self.inner.list.remove_all();
        let entries = match list {
            Ok(entries) => entries,
            Err(err) => {
                self.set_status(&format!("No se pudo leer el llavero: {err}"));
                self.inner.count.set_text("");
                return;
            }
        };
        self.inner.count.set_text(&match entries.len() {
            1 => "1 contraseña".to_owned(),
            n => format!("{n} contraseñas"),
        });
        if entries.is_empty() {
            let empty = Label::new(Some(if self.inner.search.text().trim().is_empty() {
                "Aún no hay contraseñas. Inicia sesión en un sitio y elige Guardar, o importa las \
                 de otro navegador."
            } else {
                "Nada coincide con la búsqueda."
            }));
            empty.set_wrap(true);
            empty.add_css_class("dim-label");
            empty.set_margins(24, 24, 12, 12);
            self.inner.list.append(&empty);
            return;
        }
        for meta in entries {
            self.append_row(meta);
        }
    }

    fn append_row(&self, meta: CredentialMeta) {
        let row = ActionRow::new();
        row.set_use_markup(false);
        row.set_title(&site_label(&meta.origin));
        row.set_subtitle(user_label(&meta.username));

        let secret = Label::new(Some(MASK));
        secret.add_css_class("monospace");
        secret.add_css_class("dim-label");
        secret.set_valign(Align::Center);
        secret.set_max_width_chars(24);
        secret.set_ellipsize(EllipsizeMode::End);
        row.add_suffix(&secret);

        let reveal_gen: Rc<Cell<u64>> = Rc::new(Cell::new(0));
        let make = |icon: &str, tip: &str| {
            let b = Button::from_icon_name(icon);
            b.add_css_class("flat");
            b.set_valign(Align::Center);
            b.set_tooltip_text(Some(tip));
            row.add_suffix(&b);
            b
        };
        let show_btn = make("view-reveal-symbolic", "Mostrar");
        let copy_btn = make("edit-copy-symbolic", "Copiar contraseña");
        let edit_btn = make("document-edit-symbolic", "Editar");
        let delete_btn = make("user-trash-symbolic", "Borrar");

        let meta = Rc::new(meta);
        {
            let w = self.weak();
            let meta = meta.clone();
            let secret = secret.clone();
            let gen = reveal_gen.clone();
            show_btn.connect_clicked(move || {
                if let Some(p) = w.upgrade() {
                    p.reveal(&meta, &secret, &gen);
                }
            });
        }
        {
            let w = self.weak();
            let meta = meta.clone();
            copy_btn.connect_clicked(move || {
                if let Some(p) = w.upgrade() {
                    p.copy(&meta);
                }
            });
        }
        {
            let w = self.weak();
            let meta = meta.clone();
            edit_btn.connect_clicked(move || {
                if let Some(p) = w.upgrade() {
                    p.edit(&meta);
                }
            });
        }
        {
            let w = self.weak();
            let meta = meta.clone();
            delete_btn.connect_clicked(move || {
                if let Some(p) = w.upgrade() {
                    p.delete(&meta);
                }
            });
        }
        self.inner.list.append(&row);
    }

    fn reveal(&self, meta: &Rc<CredentialMeta>, secret: &Label, gen: &Rc<Cell<u64>>) {
        if secret.has_css_class("search-vault-shown") {
            gen.set(gen.get().wrapping_add(1));
            hide_secret(secret);
            return;
        }
        let site = site_label(&meta.origin);
        let body = format!(
            "Se mostrará la contraseña de {} en {site} durante {} segundos. Asegúrate de que \
             nadie más puede ver la pantalla.",
            user_label(&meta.username),
            REVEAL_FOR.as_secs()
        );
        let p = self.clone();
        let meta = meta.clone();
        let secret = secret.clone();
        let gen = gen.clone();
        confirm(
            &self.inner.parent,
            "¿Mostrar la contraseña?",
            &body,
            "Mostrar",
            ResponseAppearance::Suggested,
            move || {
                let origin = meta.origin.clone();
                let username = meta.username.clone();
                let secret = secret.clone();
                let gen = gen.clone();
                let panel = p.clone();
                p.inner.service.run(
                    move |vault| vault.password(&origin, &username),
                    move |result| match result {
                        Ok(Some(password)) => {
                            let current = gen.get().wrapping_add(1);
                            gen.set(current);
                            secret.set_text(password.expose());
                            secret.set_selectable(true);
                            secret.remove_css_class("dim-label");
                            secret.add_css_class("search-vault-shown");
                            let label = secret.clone();
                            let gen = gen.clone();
                            glib::timeout_add_local_once(REVEAL_FOR, move || {
                                if gen.get() == current {
                                    hide_secret(&label);
                                }
                            });
                        }
                        Ok(None) => panel.set_status("Esa contraseña ya no está en el llavero."),
                        Err(err) => panel.set_status(&format!("No se pudo leer el llavero: {err}")),
                    },
                );
            },
        );
    }

    fn copy(&self, meta: &CredentialMeta) {
        let origin = meta.origin.clone();
        let username = meta.username.clone();
        let panel = self.clone();
        self.inner.service.run(
            move |vault| vault.password(&origin, &username),
            move |result| match result {
                Ok(Some(password)) => {
                    if copy_secret_to_clipboard(&password) {
                        panel.set_status(&format!(
                            "Contraseña copiada. Se borrará del portapapeles en {} segundos.",
                            CLIPBOARD_CLEAR_AFTER.as_secs()
                        ));
                    } else {
                        panel.set_status("No se pudo acceder al portapapeles.");
                    }
                }
                Ok(None) => panel.set_status("Esa contraseña ya no está en el llavero."),
                Err(err) => panel.set_status(&format!("No se pudo leer el llavero: {err}")),
            },
        );
    }

    fn edit(&self, meta: &CredentialMeta) {
        let site = site_label(&meta.origin);
        let dialog = AlertDialog::new(
            "Editar contraseña",
            &format!("Cambia el usuario o la contraseña guardados para {site}."),
        );
        let user_entry = Entry::new();
        user_entry.set_placeholder_text("Usuario");
        user_entry.set_text(&meta.username);
        let pass_entry = PasswordEntry::new();
        pass_entry.set_show_peek_icon(true);
        let pass_hint = Label::new(Some("Deja la contraseña vacía para conservar la actual."));
        pass_hint.add_css_class("dim-label");
        pass_hint.add_css_class("caption");
        pass_hint.set_xalign(0.0);
        let fields = BoxWidget::new(Orientation::Vertical, 8);
        fields.append(&user_entry);
        fields.append(&pass_entry);
        fields.append(&pass_hint);
        dialog.set_extra_child(Some(&fields));
        dialog.add_response("cancel", "Cancelar");
        dialog.add_response("save", "Guardar");
        dialog.set_response_appearance("save", ResponseAppearance::Suggested);
        dialog.set_default_response("save");
        dialog.set_close_response("cancel");

        let panel = self.clone();
        let origin = meta.origin.clone();
        let old_username = meta.username.clone();
        let pass_for_response = pass_entry.clone();
        dialog.connect_response(move |id| {
            let new_password = Password::new(pass_for_response.text());
            pass_for_response.set_text("");
            if id != "save" {
                return;
            }
            let new_username = user_entry.text().trim().to_owned();
            let origin = origin.clone();
            let old_username = old_username.clone();
            let p = panel.clone();
            panel.inner.service.run(
                move |vault| {
                    let password = if new_password.is_empty() {
                        match vault.password(&origin, &old_username)? {
                            Some(p) => p,
                            None => return Err(VaultError::InvalidPassword),
                        }
                    } else {
                        new_password
                    };
                    vault.edit(&origin, &old_username, &new_username, &password)
                },
                move |result| match result {
                    Ok(()) => {
                        p.set_status("Cambios guardados.");
                        p.refresh();
                    }
                    Err(err) => p.set_status(&format!("No se pudo guardar: {err}")),
                },
            );
        });
        dialog.present(Some(&self.inner.parent));
    }

    fn delete(&self, meta: &CredentialMeta) {
        let site = site_label(&meta.origin);
        let body = format!(
            "Se borrará del llavero la contraseña de {} en {site}. No se puede deshacer.",
            user_label(&meta.username)
        );
        let panel = self.clone();
        let origin = meta.origin.clone();
        let username = meta.username.clone();
        confirm(
            &self.inner.parent,
            "¿Borrar la contraseña?",
            &body,
            "Borrar",
            ResponseAppearance::Destructive,
            move || {
                let origin = origin.clone();
                let username = username.clone();
                let p = panel.clone();
                panel.inner.service.run(
                    move |vault| vault.delete(&origin, &username),
                    move |result| match result {
                        Ok(_) => {
                            p.set_status("Contraseña borrada.");
                            p.refresh();
                        }
                        Err(err) => p.set_status(&format!("No se pudo borrar: {err}")),
                    },
                );
            },
        );
    }

    fn open_generator_popover(&self, anchor: &impl AsWidget) {
        let pop = Popover::new();
        pop.set_parent(anchor);
        pop.set_has_arrow(true);
        pop.set_autohide(true);
        pop.set_position(PositionType::Top);

        let root = BoxWidget::new(Orientation::Vertical, 10);
        root.set_margins(12, 12, 12, 12);
        root.set_size_request(330, -1);

        let title = Label::new(Some("Generador de contraseñas"));
        title.add_css_class("heading");
        title.set_xalign(0.0);
        root.append(&title);

        let opts = Rc::new(RefCell::new(GeneratorOptions::default()));
        let current_pw = Rc::new(RefCell::new(generate_password(*opts.borrow())));

        let preview = Label::new(Some(&current_pw.borrow()));
        preview.add_css_class("monospace");
        preview.set_selectable(true);
        preview.set_wrap(true);
        preview.set_xalign(0.0);
        preview.set_margins(6, 6, 8, 8);
        root.append(&preview);

        let strength_bar = ProgressBar::new();
        let strength_lbl = Label::new(None);
        strength_lbl.add_css_class("caption");
        strength_lbl.add_css_class("dim-label");
        strength_lbl.set_xalign(0.0);
        root.append(&strength_bar);
        root.append(&strength_lbl);

        let len_header = BoxWidget::new(Orientation::Horizontal, 8);
        let len_title = Label::new(Some("Longitud"));
        len_title.set_xalign(0.0);
        len_title.set_hexpand(true);
        let len_value = Label::new(Some("20 caracteres"));
        len_value.add_css_class("dim-label");
        len_value.add_css_class("caption");
        len_header.append(&len_title);
        len_header.append(&len_value);
        root.append(&len_header);

        let slider = Scale::with_range(Orientation::Horizontal, 12.0, 48.0, 1.0);
        slider.set_digits(0);
        slider.set_draw_value(false);
        slider.set_value(20.0);
        root.append(&slider);

        let toggles_list = ListBox::new();
        toggles_list.set_selection_mode(SelectionMode::None);
        toggles_list.add_css_class("boxed-list");

        let upper_row = SwitchRow::new();
        upper_row.set_title("Incluir mayúsculas (A-Z)");
        upper_row.set_active(true);

        let digits_row = SwitchRow::new();
        digits_row.set_title("Incluir dígitos (0-9)");
        digits_row.set_active(true);

        let symbols_row = SwitchRow::new();
        symbols_row.set_title("Incluir símbolos (!@#$…)");
        symbols_row.set_active(true);

        toggles_list.append(&upper_row);
        toggles_list.append(&digits_row);
        toggles_list.append(&symbols_row);
        root.append(&toggles_list);

        root.append(&Separator::new(Orientation::Horizontal));

        let actions = BoxWidget::new(Orientation::Horizontal, 8);
        let regen_btn = Button::with_label("Generar otra");
        regen_btn.add_css_class("flat");
        let copy_btn = Button::with_label("Copiar contraseña");
        copy_btn.add_css_class("suggested-action");
        copy_btn.set_hexpand(true);
        actions.append(&regen_btn);
        actions.append(&copy_btn);
        root.append(&actions);

        let update_ui: Rc<dyn Fn()> = {
            let opts = opts.clone();
            let current_pw = current_pw.clone();
            let preview = preview.clone();
            let strength_bar = strength_bar.clone();
            let strength_lbl = strength_lbl.clone();
            let len_value = len_value.clone();
            Rc::new(move || {
                let o = *opts.borrow();
                len_value.set_text(&format!("{} caracteres", o.length));
                let pw = generate_password(o);
                let (label, fraction, bits) = password_strength_summary(&pw);
                preview.set_text(&pw);
                strength_bar.set_fraction(fraction);
                strength_lbl.set_text(&format!("Seguridad: {label} ({:.0} bits)", bits));
                *current_pw.borrow_mut() = pw;
            })
        };
        update_ui();

        {
            let opts = opts.clone();
            let slider_ref = slider.clone();
            let update = update_ui.clone();
            slider.connect_value_changed(move || {
                opts.borrow_mut().length = (slider_ref.value().round() as usize).clamp(12, 48);
                update();
            });
        }
        {
            let opts = opts.clone();
            let row_ref = upper_row.clone();
            let update = update_ui.clone();
            upper_row.connect_active_notify(move || {
                opts.borrow_mut().uppercase = row_ref.is_active();
                update();
            });
        }
        {
            let opts = opts.clone();
            let row_ref = digits_row.clone();
            let update = update_ui.clone();
            digits_row.connect_active_notify(move || {
                opts.borrow_mut().digits = row_ref.is_active();
                update();
            });
        }
        {
            let opts = opts.clone();
            let row_ref = symbols_row.clone();
            let update = update_ui.clone();
            symbols_row.connect_active_notify(move || {
                opts.borrow_mut().symbols = row_ref.is_active();
                update();
            });
        }
        {
            let update = update_ui.clone();
            regen_btn.connect_clicked(move || update());
        }
        {
            let current_pw = current_pw.clone();
            let panel = self.clone();
            let pop_ref = pop.clone();
            copy_btn.connect_clicked(move || {
                let pw = Password::new(current_pw.borrow().as_str().to_owned());
                if copy_secret_to_clipboard(&pw) {
                    panel.set_status(&format!(
                        "Contraseña generada copiada. Se borrará del portapapeles en {} segundos.",
                        CLIPBOARD_CLEAR_AFTER.as_secs()
                    ));
                    pop_ref.popdown();
                } else {
                    panel.set_status("No se pudo acceder al portapapeles.");
                }
            });
        }

        pop.set_child(Some(&root));
        pop.popup();
    }

    fn export(&self) {
        let panel = self.clone();
        confirm(
            &self.inner.parent,
            "¿Exportar las contraseñas sin cifrar?",
            "El archivo CSV contendrá TODAS tus contraseñas en texto plano. Cualquier persona o \
             programa que pueda leer ese archivo verá tus contraseñas. Guárdalo solo en un lugar \
             seguro y bórralo en cuanto termines de usarlo.",
            "Exportar…",
            ResponseAppearance::Destructive,
            move || {
                let file_dialog = gtk::FileDialog::new();
                file_dialog.set_title("Exportar contraseñas a CSV");
                file_dialog.set_initial_name("contraseñas-search.csv");
                let p = panel.clone();
                file_dialog.save(Some(&panel.inner.parent), move |path| {
                    let Some(path) = path else {
                        return;
                    };
                    let shown = path.display().to_string();
                    let p2 = p.clone();
                    p.inner.service.run(
                        move |vault| -> Result<(), String> {
                            let csv = vault.export_csv().map_err(|e| e.to_string())?;
                            write_private_file(&path, csv.as_bytes()).map_err(|e| e.to_string())
                        },
                        move |result| match result {
                            Ok(()) => p2.set_status(&format!(
                                "Exportadas a {shown}. Bórralo cuando ya no lo necesites."
                            )),
                            Err(err) => {
                                alert(&p2.inner.parent, "No se pudo exportar", &err);
                            }
                        },
                    );
                });
            },
        );
    }
}

fn hide_secret(label: &Label) {
    label.set_text(MASK);
    label.set_selectable(false);
    label.remove_css_class("search-vault-shown");
    label.add_css_class("dim-label");
}

// ---------------------------------------------------------------------------------------------
// Importación (`ImportPanel.swift`)
// ---------------------------------------------------------------------------------------------

type AfterImport = Rc<dyn Fn()>;

/// Diálogo de importación: enseña primero los perfiles encontrados y solo lee uno cuando el
/// usuario pulsa «Importar».
pub fn present_import(parent: &ApplicationWindow, service: &VaultService, after: AfterImport) {
    let dialog = Dialog::new();
    dialog.set_title("Importar contraseñas");
    dialog.set_content_width(560);
    dialog.set_content_height(480);

    let intro = Label::new(Some(
        "Dot no lee nada hasta que pulses Importar. Las contraseñas se guardan en tu llavero \
         (Secret Service) y nunca en disco. Si un navegador no se puede leer, exporta sus \
         contraseñas a CSV e impórtalas desde el archivo.",
    ));
    intro.set_wrap(true);
    intro.set_xalign(0.0);
    intro.add_css_class("dim-label");

    let list = ListBox::new();
    list.set_selection_mode(SelectionMode::None);
    list.add_css_class("boxed-list");
    let searching = Label::new(Some("Buscando perfiles de otros navegadores…"));
    searching.add_css_class("dim-label");
    searching.set_margins(16, 16, 12, 12);
    list.append(&searching);

    let scroller = ScrolledWindow::new();
    scroller.set_policy(PolicyType::Never, PolicyType::Automatic);
    scroller.set_vexpand(true);
    scroller.set_child(Some(&list));

    let csv_btn = Button::with_label("Importar desde un archivo CSV…");
    csv_btn.set_halign(Align::Start);

    let content = BoxWidget::new(Orientation::Vertical, 12);
    content.set_margins(12, 12, 12, 12);
    content.append(&intro);
    content.append(&scroller);
    content.append(&csv_btn);

    let toolbar = ToolbarView::new();
    toolbar.add_top_bar(&HeaderBar::new());
    toolbar.set_content(Some(&content));
    dialog.set_child(Some(&toolbar));

    let ctx = Rc::new(ImportCtx {
        parent: parent.clone(),
        service: service.clone(),
        after,
    });

    {
        let ctx = ctx.clone();
        csv_btn.connect_clicked(move || ctx.import_csv());
    }

    let home = std::env::var_os("HOME").map(PathBuf::from);
    {
        let ctx = ctx.clone();
        let list = list.clone();
        service.run(
            move |_vault| {
                home.map(|h| vault_import::discover_profiles(&h))
                    .unwrap_or_default()
            },
            move |profiles| {
                list.remove_all();
                if profiles.is_empty() {
                    let none = Label::new(Some(
                        "No se encontraron perfiles de Firefox ni de navegadores Chromium. En \
                         Flatpak, Dot solo ve las carpetas a las que se le da acceso: usa la \
                         importación desde CSV.",
                    ));
                    none.set_wrap(true);
                    none.add_css_class("dim-label");
                    none.set_margins(16, 16, 12, 12);
                    list.append(&none);
                    return;
                }
                for profile in profiles {
                    let row = ActionRow::new();
                    row.set_use_markup(false);
                    row.set_title(&format!("{} — {}", profile.browser, profile.name));
                    row.set_subtitle(&profile.path.display().to_string());
                    let button = Button::with_label("Importar");
                    button.set_valign(Align::Center);
                    row.add_suffix(&button);
                    let ctx = ctx.clone();
                    button.connect_clicked(move || ctx.import_profile(profile.clone(), None));
                    list.append(&row);
                }
            },
        );
    }

    dialog.present(Some(parent));
}

/// Pista para el usuario y resumen. El lote (con contraseñas) se suelta en el hilo de Vault.
type ImportOutcome = (
    Option<String>,
    Result<dot_core::vault::ImportSummary, VaultError>,
);

struct ImportCtx {
    parent: ApplicationWindow,
    service: VaultService,
    after: AfterImport,
}

impl ImportCtx {
    fn import_profile(
        self: &Rc<Self>,
        profile: BrowserProfile,
        primary: Option<Zeroizing<String>>,
    ) {
        let temp_root = self.service.paths().runtime_dir.clone();
        let ctx = self.clone();
        let retry_profile = profile.clone();
        self.service.run(
            move |vault| -> Result<ImportOutcome, ImportError> {
                let batch = match &profile.kind {
                    ProfileKind::Firefox => vault_import::import_firefox(
                        &profile.path,
                        primary.as_ref().map(|p| p.as_str()),
                        &temp_root,
                    )?,
                    ProfileKind::Chromium { keyring_app } => {
                        let keys = ChromiumKeys {
                            v11_secret: vault_import::chromium_safe_storage_secret(keyring_app),
                        };
                        vault_import::import_chromium(&profile.path, &keys, &temp_root)?
                    }
                };
                let summary = vault_import::apply(vault, &batch);
                Ok((batch.hint.clone(), summary))
            },
            move |result| match result {
                Ok((hint, summary)) => ctx.show_summary(hint, summary),
                Err(ImportError::PrimaryPasswordRequired) => {
                    ctx.ask_primary_password(retry_profile, false)
                }
                Err(ImportError::WrongPrimaryPassword) => {
                    ctx.ask_primary_password(retry_profile, true)
                }
                Err(err) => alert(&ctx.parent, "No se pudo importar", &err.to_string()),
            },
        );
    }

    fn ask_primary_password(self: &Rc<Self>, profile: BrowserProfile, wrong: bool) {
        let body = if wrong {
            "La contraseña principal no es correcta. Inténtalo de nuevo."
        } else {
            "Este perfil de Firefox está protegido con una contraseña principal. Introdúcela para \
             importar sus contraseñas. Dot no la guarda."
        };
        let dialog = AlertDialog::new("Contraseña principal de Firefox", body);
        let entry = PasswordEntry::new();
        entry.set_show_peek_icon(true);
        dialog.set_extra_child(Some(&entry));
        dialog.add_response("cancel", "Cancelar");
        dialog.add_response("go", "Importar");
        dialog.set_response_appearance("go", ResponseAppearance::Suggested);
        dialog.set_default_response("go");
        dialog.set_close_response("cancel");
        let ctx = self.clone();
        let entry_for_response = entry.clone();
        dialog.connect_response(move |id| {
            let primary = Zeroizing::new(entry_for_response.text());
            entry_for_response.set_text("");
            if id == "go" {
                ctx.import_profile(profile.clone(), Some(primary));
            }
        });
        dialog.present(Some(&self.parent));
    }

    fn import_csv(self: &Rc<Self>) {
        let file_dialog = gtk::FileDialog::new();
        file_dialog.set_title("Importar contraseñas desde CSV");
        let ctx = self.clone();
        file_dialog.open(Some(&self.parent), move |path| {
            let Some(path) = path else {
                return;
            };
            let ctx2 = ctx.clone();
            ctx.service.run(
                move |vault| -> Result<ImportOutcome, String> {
                    let text =
                        Zeroizing::new(std::fs::read_to_string(&path).map_err(|e| e.to_string())?);
                    let batch = vault_import::import_csv(&text).map_err(|e| e.to_string())?;
                    let summary = vault_import::apply(vault, &batch);
                    Ok((batch.hint.clone(), summary))
                },
                move |result| match result {
                    Ok((hint, summary)) => ctx2.show_summary(hint, summary),
                    Err(err) => alert(&ctx2.parent, "No se pudo importar", &err),
                },
            );
        });
    }

    fn show_summary(
        &self,
        hint: Option<String>,
        summary: Result<dot_core::vault::ImportSummary, VaultError>,
    ) {
        match summary {
            Ok(s) => {
                let mut body = format!(
                    "Importadas: {}\nDuplicadas (ya estaban): {}\nFallidas: {}",
                    s.imported, s.duplicates, s.failed
                );
                if let Some(hint) = hint {
                    body.push_str("\n\n");
                    body.push_str(&hint);
                }
                alert(&self.parent, "Importación terminada", &body);
                (self.after)();
            }
            Err(err) => alert(&self.parent, "No se pudo importar", &err.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csprng_password_generator_respects_length_and_character_classes() {
        let opts = GeneratorOptions {
            length: 24,
            uppercase: true,
            digits: true,
            symbols: true,
        };
        let pw1 = generate_password(opts);
        let pw2 = generate_password(opts);
        assert_eq!(pw1.len(), 24);
        assert_ne!(*pw1, *pw2);
        assert!(pw1.chars().any(|c| c.is_ascii_lowercase()));
        assert!(pw1.chars().any(|c| c.is_ascii_uppercase()));
        assert!(pw1.chars().any(|c| c.is_ascii_digit()));
        assert!(pw1.chars().any(|c| SYMBOL_CHARS.contains(&(c as u8))));

        // Clamping fuera de rango [12, 48].
        let short = generate_password(GeneratorOptions {
            length: 4,
            uppercase: false,
            digits: false,
            symbols: false,
        });
        assert_eq!(short.len(), 12);
        assert!(short.chars().all(|c| c.is_ascii_lowercase()));

        let long = generate_password(GeneratorOptions {
            length: 99,
            uppercase: true,
            digits: true,
            symbols: false,
        });
        assert_eq!(long.len(), 48);
        assert!(!long.chars().any(|c| SYMBOL_CHARS.contains(&(c as u8))));
    }

    #[test]
    fn password_strength_and_weakness_checks_work() {
        assert!(is_weak_password("short123"));
        assert!(is_weak_password("aaaaaaaaaaaa"));
        assert!(!is_weak_password("Correct-Horse-99!"));
        let (label, frac, bits) = password_strength_summary("Correct-Horse-99!");
        assert!(bits >= 75.0);
        assert!(frac >= 0.8);
        assert!(label == "Fuerte" || label == "Muy fuerte");
    }
}
