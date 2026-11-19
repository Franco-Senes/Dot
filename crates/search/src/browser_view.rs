//! Vista reutilizable `BrowserView` (no singleton): envuelve una instancia de `cef::Browser`
//! en modo Windowless Off-Screen (`RuntimeStyle::ALLOY`) pintando fotogramas BGRA8 en un
//! `gtk::Picture` mediante `gdk::MemoryTexture` (`B8g8r8a8Premultiplied`) y reenviando
//! redimensionado, factor de escala HiDPI, ratón, rueda, gestos táctiles, teclado y foco.
//!
//! Incluye en esta capa:
//! - Privacidad base (`Privacy.swift`): cabecera `Sec-GPC: 1`, `navigator.globalPrivacyControl`,
//!   limpieza de click-IDs al navegar y bloqueo de cookies cross-site (`CefCookieAccessFilter`).
//! - Página propia sobria de error de red (`Stage.swift` / `on_load_error`).
//! - Página propia de advertencia TLS (`Dialogs.swift` / `on_certificate_error`) con bloqueo
//!   estricto por defecto y botón explícito autenticado por nonce aleatorio de 128 bits para
//!   permitir temporalmente ese host concreto en la sesión actual.

use crate::curtain::{build_curtain_picker_script, build_curtain_stop_script, CurtainManager};
use crate::reader::build_reader_extract_script;
use crate::settings::SettingsStore;
use crate::shield::ShieldManager;
use anyhow::{Context, Result};
use cef::{rc::Rc as _, *};
use gnome_platform::gdk::{self, MemoryFormat, MemoryTexture, ModifierType, Texture};
use gnome_platform::glib::Bytes;
use gnome_platform::gtk::{
    Align, AsController, AsWidget, ContentFit, DrawingArea, EllipsizeMode, EventControllerFocus,
    EventControllerKey, EventControllerMotion, EventControllerScroll, GestureClick, GestureSwipe,
    Label, Overlay, Picture, ScrollFlags,
};
use percent_encoding::{percent_decode_str, utf8_percent_encode, NON_ALPHANUMERIC};
use search_core::address::{is_same_site, sanitize_url_for_ui, strip_click_ids};
use search_core::curtain::{
    build_peek_script, build_unpeek_script, build_veil_style_script, ElementPickSnapshot,
};
use search_core::reader::{
    build_apply_reader_options_script, ReaderArticle, ReaderOptions, READER_INTERNAL_MARKER,
};
use serde::Deserialize;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::io::Read;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

static ACTIVE_BROWSER_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Número actual de instancias activas de `cef::Browser` en este proceso.
/// Sirve para verificar la carga diferida (lazy) de pestañas restauradas.
pub fn active_browser_count() -> usize {
    ACTIVE_BROWSER_COUNT.load(Ordering::SeqCst)
}

const CEF_EVENTFLAG_CAPS_LOCK_ON: u32 = 1 << 0;
const CEF_EVENTFLAG_SHIFT_DOWN: u32 = 1 << 1;
const CEF_EVENTFLAG_CONTROL_DOWN: u32 = 1 << 2;
const CEF_EVENTFLAG_ALT_DOWN: u32 = 1 << 3;
const CEF_EVENTFLAG_LEFT_MOUSE_BUTTON: u32 = 1 << 4;
const CEF_EVENTFLAG_MIDDLE_MOUSE_BUTTON: u32 = 1 << 5;
const CEF_EVENTFLAG_RIGHT_MOUSE_BUTTON: u32 = 1 << 6;
const CEF_EVENTFLAG_COMMAND_DOWN: u32 = 1 << 7;

const TLS_OVERRIDE_HOST: &str = "search-tls-override.invalid";

pub type LoadingStateCallback = Box<dyn Fn(bool, bool, bool)>;
pub type PaintFrameCallback = Box<dyn Fn(u64, i32, i32)>;

/// Instantánea segura de `CefDownloadHandler::on_before_download`.
pub struct DownloadBeforeEvent {
    pub cef_id: u32,
    pub url: String,
    pub suggested_name: String,
    pub total_bytes: i64,
    pub callback: BeforeDownloadCallback,
}

/// Instantánea segura de `CefDownloadHandler::on_download_updated`.
pub struct DownloadUpdateEvent {
    pub cef_id: u32,
    pub url: String,
    pub full_path: String,
    pub suggested_name: String,
    pub received_bytes: i64,
    pub total_bytes: i64,
    pub percent_complete: i32,
    pub current_speed: i64,
    pub is_in_progress: bool,
    pub is_complete: bool,
    pub is_canceled: bool,
    pub is_interrupted: bool,
    pub is_paused: bool,
    pub callback: Option<DownloadItemCallback>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum PageIpcMessage {
    #[serde(rename = "curtain_pick")]
    CurtainPick { snapshot: ElementPickSnapshot },
    #[serde(rename = "curtain_off")]
    CurtainOff,
    #[serde(rename = "curtain_error")]
    CurtainError { message: String },
    #[serde(rename = "reader_article")]
    ReaderArticle {
        ok: bool,
        #[serde(default)]
        reason: Option<String>,
        #[serde(default)]
        article: Option<ReaderArticle>,
    },
}

pub type ReaderExtractedCallback = Box<dyn Fn(bool, Option<String>, Option<ReaderArticle>)>;

#[derive(Default)]
pub struct BrowserCallbacks {
    pub on_title_changed: Option<Box<dyn Fn(String)>>,
    pub on_address_changed: Option<Box<dyn Fn(String)>>,
    pub on_loading_state_changed: Option<LoadingStateCallback>,
    pub on_progress_changed: Option<Box<dyn Fn(f64)>>,
    pub on_favicon_changed: Option<Box<dyn Fn(Option<String>)>>,
    pub on_favicon_texture: Option<Box<dyn Fn(Texture)>>,
    pub on_audio_state_changed: Option<Box<dyn Fn(bool, bool)>>,
    pub on_open_new_tab: Option<Box<dyn Fn(String, bool)>>,
    pub on_fullscreen_changed: Option<Box<dyn Fn(bool)>>,
    pub on_find_result: Option<Box<dyn Fn(i32, i32)>>,
    pub on_load_finished: Option<Box<dyn Fn(String, i32)>>,
    pub on_non_empty_paint: Option<PaintFrameCallback>,
    pub on_before_download: Option<Box<dyn Fn(DownloadBeforeEvent)>>,
    pub on_download_updated: Option<Box<dyn Fn(DownloadUpdateEvent)>>,
    pub on_curtain_pick: Option<Box<dyn Fn(ElementPickSnapshot)>>,
    pub on_curtain_picking_changed: Option<Box<dyn Fn(bool)>>,
    pub on_reader_extracted: Option<ReaderExtractedCallback>,
    pub on_reader_state_changed: Option<Box<dyn Fn(bool)>>,
}

struct SharedState {
    browser: RefCell<Option<Browser>>,
    settings: SettingsStore,
    shield: ShieldManager,
    curtain: CurtainManager,
    overlay: Overlay,
    picture: Picture,
    status_pill: Label,
    logical_size: Cell<(i32, i32)>,
    scale_factor: Cell<f32>,
    last_pointer: Cell<(i32, i32)>,
    title: RefCell<String>,
    current_url: RefCell<String>,
    top_url_shared: Arc<RwLock<String>>,
    blocked_count: Arc<AtomicU64>,
    favicon_url: RefCell<Option<String>>,
    last_favicon_requested: RefCell<Option<String>>,
    is_loading: Cell<bool>,
    can_go_back: Cell<bool>,
    can_go_forward: Cell<bool>,
    is_audible: Cell<bool>,
    is_muted: Cell<bool>,
    progress: Cell<f64>,
    zoom_factor: Cell<f64>,
    paint_frames: Cell<u64>,
    non_empty_frames: Cell<u64>,
    tls_nonce: String,
    audio_nonce: String,
    ipc_nonce: String,
    curtain_picking: Cell<bool>,
    reader_original_url: RefCell<Option<String>>,
    tls_allowed_hosts: RefCell<HashSet<String>>,
    suppress_next_abort_error: Cell<bool>,
    last_load_was_error: Cell<bool>,
    counted_active: Cell<bool>,
    callbacks: RefCell<BrowserCallbacks>,
}

impl Drop for SharedState {
    fn drop(&mut self) {
        if self.counted_active.replace(false) {
            ACTIVE_BROWSER_COUNT.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

/// Instancia independiente de vista web lista para alojarse en una ventana o en `AdwTabView`.
#[derive(Clone)]
pub struct BrowserView {
    shared: Rc<SharedState>,
}

impl BrowserView {
    pub fn new(
        mut request_context: RequestContext,
        settings: SettingsStore,
        shield: ShieldManager,
        curtain: CurtainManager,
        initial_url: &str,
        dark_background: bool,
    ) -> Result<Self> {
        let overlay = Overlay::new();
        overlay.set_hexpand(true);
        overlay.set_vexpand(true);

        let picture = Picture::new();
        picture.set_can_shrink(true);
        picture.set_content_fit(ContentFit::Fill);
        picture.set_hexpand(true);
        picture.set_vexpand(true);
        picture.set_focusable(true);
        overlay.set_child(Some(&picture));

        // Sensor pasivo sin recepción de eventos para enterarnos de cada cambio exacto de tamaño.
        let resize_sensor = DrawingArea::new();
        resize_sensor.set_hexpand(true);
        resize_sensor.set_vexpand(true);
        resize_sensor.set_can_target(false);
        overlay.add_overlay(&resize_sensor);
        overlay.set_measure_overlay(&resize_sensor, false);

        // Píldora discreta inferior izquierda para mostrar el enlace bajo el cursor.
        let status_pill = Label::new(None);
        status_pill.add_css_class("search-status-pill");
        status_pill.set_halign(Align::Start);
        status_pill.set_valign(Align::End);
        status_pill.set_margins(0, 8, 8, 8);
        status_pill.set_ellipsize(EllipsizeMode::End);
        status_pill.set_max_width_chars(76);
        status_pill.set_can_target(false);
        status_pill.set_visible(false);
        overlay.add_overlay(&status_pill);
        overlay.set_measure_overlay(&status_pill, false);

        let initial_zoom = settings.with(|s| s.page_zoom);
        let initial_scale = picture.scale_factor() as f32;

        let shared = Rc::new(SharedState {
            browser: RefCell::new(None),
            settings,
            shield,
            curtain,
            overlay,
            picture: picture.clone(),
            status_pill,
            logical_size: Cell::new((1180, 720)),
            scale_factor: Cell::new(initial_scale.max(1.0)),
            last_pointer: Cell::new((0, 0)),
            title: RefCell::new(String::new()),
            current_url: RefCell::new(initial_url.to_string()),
            top_url_shared: Arc::new(RwLock::new(initial_url.to_string())),
            blocked_count: Arc::new(AtomicU64::new(0)),
            favicon_url: RefCell::new(None),
            last_favicon_requested: RefCell::new(None),
            is_loading: Cell::new(false),
            can_go_back: Cell::new(false),
            can_go_forward: Cell::new(false),
            is_audible: Cell::new(false),
            is_muted: Cell::new(false),
            progress: Cell::new(0.0),
            zoom_factor: Cell::new(initial_zoom),
            paint_frames: Cell::new(0),
            non_empty_frames: Cell::new(0),
            tls_nonce: generate_hex_nonce_128(),
            audio_nonce: generate_hex_nonce_128(),
            ipc_nonce: generate_hex_nonce_128(),
            curtain_picking: Cell::new(false),
            reader_original_url: RefCell::new(None),
            tls_allowed_hosts: RefCell::new(HashSet::new()),
            suppress_next_abort_error: Cell::new(false),
            last_load_was_error: Cell::new(false),
            counted_active: Cell::new(false),
            callbacks: RefCell::new(BrowserCallbacks::default()),
        });

        let window_info = WindowInfo {
            windowless_rendering_enabled: 1,
            shared_texture_enabled: 0,
            external_begin_frame_enabled: 0,
            runtime_style: RuntimeStyle::ALLOY,
            ..Default::default()
        };

        let bg_argb = if dark_background {
            0xff1c1c1e
        } else {
            0xffffffff
        };

        let browser_settings = BrowserSettings {
            windowless_frame_rate: 60,
            background_color: bg_argb,
            chrome_status_bubble: State::DISABLED,
            chrome_zoom_bubble: State::DISABLED,
            ..Default::default()
        };

        let mut client = build_cef_client(shared.clone());
        let start_url = if initial_url.trim().is_empty() {
            "about:blank"
        } else {
            initial_url.trim()
        };
        let cef_url = CefString::from(start_url);

        let browser = cef::browser_host_create_browser_sync(
            Some(&window_info),
            Some(&mut client),
            Some(&cef_url),
            Some(&browser_settings),
            None,
            Some(&mut request_context),
        )
        .context("cef::browser_host_create_browser_sync no pudo crear el navegador OSR")?;

        *shared.browser.borrow_mut() = Some(browser);
        shared.counted_active.set(true);
        ACTIVE_BROWSER_COUNT.fetch_add(1, Ordering::SeqCst);

        let view = Self { shared };
        view.attach_gtk_controllers(&resize_sensor);
        if (initial_zoom - 1.0).abs() > f64::EPSILON {
            view.set_zoom_factor(initial_zoom);
        }

        Ok(view)
    }

    pub fn widget(&self) -> &Overlay {
        &self.shared.overlay
    }

    pub fn grab_focus(&self) {
        self.shared.picture.grab_focus();
        if let Some(host) = self.host() {
            host.set_focus(1);
        }
    }

    pub fn set_callbacks(&self, callbacks: BrowserCallbacks) {
        *self.shared.callbacks.borrow_mut() = callbacks;
    }

    pub fn with_callbacks_mut(&self, f: impl FnOnce(&mut BrowserCallbacks)) {
        let mut cbs = self.shared.callbacks.borrow_mut();
        f(&mut cbs);
    }

    pub fn load_url(&self, raw_url: &str) {
        let final_url = if self.shared.settings.with(|s| s.strip_click_ids) {
            strip_click_ids(raw_url)
        } else {
            raw_url.to_string()
        };
        if self
            .shared
            .reader_original_url
            .borrow_mut()
            .take()
            .is_some()
        {
            if let Some(ref cb) = self.shared.callbacks.borrow().on_reader_state_changed {
                cb(false);
            }
        }
        *self.shared.current_url.borrow_mut() = final_url.clone();
        if let Ok(mut guard) = self.shared.top_url_shared.write() {
            *guard = final_url.clone();
        }
        if let Some(frame) = self.main_frame() {
            let cs = CefString::from(final_url.as_str());
            frame.load_url(Some(&cs));
        }
    }

    pub fn blocked_count(&self) -> u64 {
        self.shared.blocked_count.load(Ordering::Relaxed)
    }

    pub fn execute_js(&self, js: &str, source_name: &str) {
        if let Some(frame) = self.main_frame() {
            let cef_js = CefString::from(js);
            let cef_src = CefString::from(source_name);
            frame.execute_java_script(Some(&cef_js), Some(&cef_src), 1);
        }
    }

    pub fn is_curtain_picking(&self) -> bool {
        self.shared.curtain_picking.get()
    }

    pub fn set_curtain_picking(&self, active: bool) {
        self.shared.curtain_picking.set(active);
        if active {
            let script = build_curtain_picker_script(&self.shared.ipc_nonce);
            self.execute_js(&script, "search://curtain-picker");
        } else {
            self.execute_js(build_curtain_stop_script(), "search://curtain-stop");
        }
        if let Some(ref cb) = self.shared.callbacks.borrow().on_curtain_picking_changed {
            cb(active);
        }
    }

    pub fn apply_curtain_css(&self, css: &str) {
        let js = build_veil_style_script(css);
        self.execute_js(&js, "search://curtain-css");
    }

    pub fn peek_curtain_selector(&self, css_without: &str, selector: &str) {
        let js = build_peek_script(css_without, selector);
        self.execute_js(&js, "search://curtain-peek");
    }

    pub fn unpeek_curtain(&self, full_css: &str) {
        let js = build_unpeek_script(full_css);
        self.execute_js(&js, "search://curtain-unpeek");
    }

    pub fn is_reader_active(&self) -> bool {
        self.shared.reader_original_url.borrow().is_some()
    }

    pub fn trigger_reader_extract(&self) {
        let script = build_reader_extract_script(&self.shared.ipc_nonce);
        self.execute_js(&script, "search://reader-extract");
    }

    pub fn enter_reader_html(&self, original_url: &str, rendered_html: &str) {
        if self.shared.curtain_picking.replace(false) {
            if let Some(ref cb) = self.shared.callbacks.borrow().on_curtain_picking_changed {
                cb(false);
            }
        }
        *self.shared.reader_original_url.borrow_mut() = Some(original_url.to_string());
        *self.shared.current_url.borrow_mut() = original_url.to_string();
        if let Some(frame) = self.main_frame() {
            let data_url = reader_html_to_internal_data_url(rendered_html);
            let cs = CefString::from(data_url.as_str());
            frame.load_url(Some(&cs));
        }
        if let Some(ref cb) = self.shared.callbacks.borrow().on_reader_state_changed {
            cb(true);
        }
    }

    pub fn exit_reader_mode(&self) {
        if let Some(orig) = self.shared.reader_original_url.borrow_mut().take() {
            if let Some(ref cb) = self.shared.callbacks.borrow().on_reader_state_changed {
                cb(false);
            }
            if let Some(frame) = self.main_frame() {
                let cs = CefString::from(orig.as_str());
                frame.load_url(Some(&cs));
            }
        }
    }

    pub fn apply_reader_options(&self, options: ReaderOptions) {
        if self.is_reader_active() {
            let js = build_apply_reader_options_script(options);
            self.execute_js(&js, "search://reader-options");
        }
    }

    pub fn current_url(&self) -> String {
        self.shared.current_url.borrow().clone()
    }

    pub fn title(&self) -> String {
        self.shared.title.borrow().clone()
    }

    #[allow(dead_code)]
    pub fn favicon_url(&self) -> Option<String> {
        self.shared.favicon_url.borrow().clone()
    }

    pub fn is_loading(&self) -> bool {
        self.shared.is_loading.get()
    }

    #[allow(dead_code)]
    pub fn can_go_back(&self) -> bool {
        self.shared.can_go_back.get()
    }

    #[allow(dead_code)]
    pub fn can_go_forward(&self) -> bool {
        self.shared.can_go_forward.get()
    }

    pub fn go_back(&self) {
        if let Some(browser) = self.browser() {
            if browser.can_go_back() != 0 {
                browser.go_back();
            }
        }
    }

    pub fn go_forward(&self) {
        if let Some(browser) = self.browser() {
            if browser.can_go_forward() != 0 {
                browser.go_forward();
            }
        }
    }

    pub fn reload(&self) {
        if let Some(browser) = self.browser() {
            browser.reload();
        }
    }

    pub fn reload_ignore_cache(&self) {
        if let Some(browser) = self.browser() {
            browser.reload_ignore_cache();
        }
    }

    pub fn stop_load(&self) {
        if let Some(browser) = self.browser() {
            browser.stop_load();
        }
    }

    pub fn last_load_was_error(&self) -> bool {
        self.shared.last_load_was_error.get()
    }

    /// Inicia o reintenta una descarga directa mediante `CefBrowserHost::start_download`.
    pub fn start_download(&self, url: &str) {
        if let Some(host) = self.host() {
            let cs = CefString::from(url);
            host.start_download(Some(&cs));
        }
    }

    pub fn zoom_factor(&self) -> f64 {
        self.shared.zoom_factor.get()
    }

    pub fn set_zoom_factor(&self, factor: f64) {
        let clamped = factor.clamp(0.25, 5.0);
        self.shared.zoom_factor.set(clamped);
        if let Some(host) = self.host() {
            // En Blink/CEF el nivel de zoom es logarítmico en base 1.2 (`0.0` == `100 %`).
            let cef_level = clamped.ln() / 1.2_f64.ln();
            host.set_zoom_level(cef_level);
        }
    }

    pub fn zoom_in(&self) {
        let next = ((self.zoom_factor() * 1.10) * 100.0).round() / 100.0;
        self.set_zoom_factor(next);
    }

    pub fn zoom_out(&self) {
        let next = ((self.zoom_factor() / 1.10) * 100.0).round() / 100.0;
        self.set_zoom_factor(next);
    }

    pub fn zoom_reset(&self) {
        self.set_zoom_factor(1.0);
    }

    pub fn find(&self, text: &str, forward: bool, match_case: bool, find_next: bool) {
        let Some(host) = self.host() else {
            return;
        };
        if text.is_empty() {
            host.stop_finding(1);
            return;
        }
        let cs = CefString::from(text);
        host.find(
            Some(&cs),
            i32::from(forward),
            i32::from(match_case),
            i32::from(find_next),
        );
    }

    pub fn stop_finding(&self, clear_selection: bool) {
        if let Some(host) = self.host() {
            host.stop_finding(i32::from(clear_selection));
        }
    }

    pub fn toggle_dev_tools(&self) {
        let Some(host) = self.host() else {
            return;
        };
        if host.has_dev_tools() != 0 {
            host.close_dev_tools();
        } else {
            let win_info = WindowInfo {
                window_name: CefString::from("Search — Inspector web"),
                bounds: Rect {
                    x: 0,
                    y: 0,
                    width: 980,
                    height: 680,
                },
                ..Default::default()
            };
            let settings = BrowserSettings::default();
            host.show_dev_tools(Some(&win_info), None, Some(&settings), None);
        }
    }

    pub fn non_empty_paint_count(&self) -> u64 {
        self.shared.non_empty_frames.get()
    }

    #[allow(dead_code)]
    pub fn is_audible(&self) -> bool {
        self.shared.is_audible.get()
    }

    pub fn is_audio_muted(&self) -> bool {
        if let Some(host) = self.host() {
            let muted = host.is_audio_muted() != 0;
            self.shared.is_muted.set(muted);
            muted
        } else {
            self.shared.is_muted.get()
        }
    }

    pub fn set_audio_muted(&self, muted: bool) {
        self.shared.is_muted.set(muted);
        if let Some(host) = self.host() {
            host.set_audio_muted(i32::from(muted));
        }
        let audible = self.shared.is_audible.get();
        if let Some(ref cb) = self.shared.callbacks.borrow().on_audio_state_changed {
            cb(audible, muted);
        }
    }

    pub fn toggle_audio_muted(&self) -> bool {
        let next = !self.is_audio_muted();
        self.set_audio_muted(next);
        next
    }

    pub fn close_browser(&self) {
        if let Some(host) = self.host() {
            host.close_browser(1);
        }
        *self.shared.browser.borrow_mut() = None;
        if self.shared.counted_active.replace(false) {
            ACTIVE_BROWSER_COUNT.fetch_sub(1, Ordering::SeqCst);
        }
    }

    fn browser(&self) -> Option<Browser> {
        self.shared.browser.borrow().clone()
    }

    fn host(&self) -> Option<BrowserHost> {
        self.browser().and_then(|b| b.host())
    }

    fn main_frame(&self) -> Option<Frame> {
        self.browser().and_then(|b| b.main_frame())
    }

    fn attach_gtk_controllers(&self, resize_sensor: &DrawingArea) {
        // 1. Redimensionado lógico y factor de escala HiDPI.
        {
            let v = self.clone();
            resize_sensor.connect_resize(move |width, height| {
                let scale = v.shared.picture.scale_factor() as f32;
                let prev_scale = v.shared.scale_factor.replace(scale.max(1.0));
                if width > 0 && height > 0 {
                    let prev_size = v.shared.logical_size.replace((width, height));
                    if prev_size != (width, height) || (prev_scale - scale).abs() > f32::EPSILON {
                        if let Some(host) = v.host() {
                            if (prev_scale - scale).abs() > f32::EPSILON {
                                host.notify_screen_info_changed();
                            }
                            host.was_resized();
                            host.invalidate(PaintElementType::VIEW);
                        }
                    }
                }
            });
        }

        {
            let v = self.clone();
            self.shared.picture.connect_scale_factor_notify(move || {
                let scale = v.shared.picture.scale_factor() as f32;
                v.shared.scale_factor.set(scale.max(1.0));
                if let Some(host) = v.host() {
                    host.notify_screen_info_changed();
                    host.was_resized();
                    host.invalidate(PaintElementType::VIEW);
                }
            });
        }

        // 2. Movimiento del puntero.
        {
            let motion = EventControllerMotion::new();
            let v_enter = self.clone();
            let motion_enter = motion.clone();
            motion.connect_enter(move |x, y| {
                let (ix, iy) = (x.round() as i32, y.round() as i32);
                v_enter.shared.last_pointer.set((ix, iy));
                if let Some(host) = v_enter.host() {
                    let ev = MouseEvent {
                        x: ix,
                        y: iy,
                        modifiers: gdk_mods_to_cef(motion_enter.current_event_state()),
                    };
                    host.send_mouse_move_event(Some(&ev), 0);
                }
            });

            let v_move = self.clone();
            let motion_move = motion.clone();
            motion.connect_motion(move |x, y| {
                let (ix, iy) = (x.round() as i32, y.round() as i32);
                v_move.shared.last_pointer.set((ix, iy));
                if let Some(host) = v_move.host() {
                    let ev = MouseEvent {
                        x: ix,
                        y: iy,
                        modifiers: gdk_mods_to_cef(motion_move.current_event_state()),
                    };
                    host.send_mouse_move_event(Some(&ev), 0);
                }
            });

            let v_leave = self.clone();
            let motion_leave = motion.clone();
            motion.connect_leave(move || {
                let (ix, iy) = v_leave.shared.last_pointer.get();
                if let Some(host) = v_leave.host() {
                    let ev = MouseEvent {
                        x: ix,
                        y: iy,
                        modifiers: gdk_mods_to_cef(motion_leave.current_event_state()),
                    };
                    host.send_mouse_move_event(Some(&ev), 1);
                }
            });

            self.shared.picture.add_controller(&motion);
        }

        // 3. Clics de ratón (izquierdo, central y derecho).
        {
            let click = GestureClick::new();
            click.set_button(0);

            let v_press = self.clone();
            let click_press = click.clone();
            click.connect_pressed(move |n_press, x, y| {
                v_press.grab_focus();
                let (ix, iy) = (x.round() as i32, y.round() as i32);
                v_press.shared.last_pointer.set((ix, iy));
                let Some(btn) = gdk_button_to_cef(click_press.current_button()) else {
                    return;
                };
                if let Some(host) = v_press.host() {
                    let ev = MouseEvent {
                        x: ix,
                        y: iy,
                        modifiers: gdk_mods_to_cef(click_press.current_event_state()),
                    };
                    host.send_mouse_click_event(Some(&ev), btn, 0, n_press.clamp(1, 3));
                }
            });

            let v_release = self.clone();
            let click_release = click.clone();
            click.connect_released(move |n_press, x, y| {
                let (ix, iy) = (x.round() as i32, y.round() as i32);
                v_release.shared.last_pointer.set((ix, iy));
                let Some(btn) = gdk_button_to_cef(click_release.current_button()) else {
                    return;
                };
                if let Some(host) = v_release.host() {
                    let ev = MouseEvent {
                        x: ix,
                        y: iy,
                        modifiers: gdk_mods_to_cef(click_release.current_event_state()),
                    };
                    host.send_mouse_click_event(Some(&ev), btn, 1, n_press.clamp(1, 3));
                }
            });

            self.shared.picture.add_controller(&click);
        }

        // 4. Rueda de desplazamiento y zoom con Ctrl + rueda.
        {
            let scroll = EventControllerScroll::new(ScrollFlags::BOTH_AXES);
            let v_scroll = self.clone();
            let scroll_ref = scroll.clone();
            scroll.connect_scroll(move |dx, dy| {
                let mods = scroll_ref.current_event_state();
                if mods.contains(ModifierType::CONTROL_MASK) && dy.abs() > f64::EPSILON {
                    if dy < 0.0 {
                        v_scroll.zoom_in();
                    } else {
                        v_scroll.zoom_out();
                    }
                    return true;
                }

                if let Some(host) = v_scroll.host() {
                    let (ix, iy) = v_scroll.shared.last_pointer.get();
                    let ev = MouseEvent {
                        x: ix,
                        y: iy,
                        modifiers: gdk_mods_to_cef(mods),
                    };
                    let wheel_x = (-dx * 120.0).round() as i32;
                    let wheel_y = (-dy * 120.0).round() as i32;
                    if wheel_x != 0 || wheel_y != 0 {
                        host.send_mouse_wheel_event(Some(&ev), wheel_x, wheel_y);
                    }
                    return true;
                }
                false
            });
            self.shared.picture.add_controller(&scroll);
        }

        // 5. Gestos táctiles horizontales atrás/adelante (`Swipe.swift`).
        {
            let swipe = GestureSwipe::new();
            let v_swipe = self.clone();
            swipe.connect_swipe(move |vx, vy| {
                if vx.abs() > 480.0 && vx.abs() > vy.abs() * 2.0 {
                    if vx > 0.0 {
                        v_swipe.go_back();
                    } else {
                        v_swipe.go_forward();
                    }
                }
            });
            self.shared.picture.add_controller(&swipe);
        }

        // 6. Foco de teclado.
        {
            let focus = EventControllerFocus::new();
            let v_in = self.clone();
            focus.connect_enter(move || {
                if let Some(host) = v_in.host() {
                    host.set_focus(1);
                }
            });
            let v_out = self.clone();
            focus.connect_leave(move || {
                if let Some(host) = v_out.host() {
                    host.set_focus(0);
                }
            });
            self.shared.picture.add_controller(&focus);
        }

        // 7. Teclado y entrada de texto.
        {
            let key = EventControllerKey::new();
            let v_down = self.clone();
            key.connect_key_pressed(move |keyval, keycode, raw_state| {
                let mods = ModifierType::from_bits_truncate(raw_state);
                if should_propagate_shortcut_to_window(
                    keyval,
                    mods,
                    v_down.is_loading() || v_down.is_curtain_picking(),
                ) {
                    return false;
                }
                let Some(host) = v_down.host() else {
                    return false;
                };

                let cef_mods = gdk_mods_to_cef(mods);
                let win_vk = gdk_keyval_to_windows_vk(keyval);
                let unmod_ch = gdk::keyval_to_unicode(gdk::keyval_to_lower(keyval))
                    .map(|c| c as u16)
                    .unwrap_or(0);
                let ch = gdk::keyval_to_unicode(keyval)
                    .map(|c| c as u16)
                    .unwrap_or(0);

                let raw_down = KeyEvent {
                    type_: KeyEventType::RAWKEYDOWN,
                    modifiers: cef_mods,
                    windows_key_code: win_vk,
                    native_key_code: keycode as i32,
                    is_system_key: i32::from(mods.contains(ModifierType::ALT_MASK)),
                    character: ch,
                    unmodified_character: unmod_ch,
                    ..Default::default()
                };
                host.send_key_event(Some(&raw_down));

                let has_cmd_mod = mods.intersects(
                    ModifierType::CONTROL_MASK
                        | ModifierType::ALT_MASK
                        | ModifierType::SUPER_MASK
                        | ModifierType::META_MASK,
                );
                if !has_cmd_mod {
                    if keyval == gdk::keys::RETURN || keyval == gdk::keys::KP_ENTER {
                        let enter_char = KeyEvent {
                            type_: KeyEventType::CHAR,
                            modifiers: cef_mods,
                            windows_key_code: 0x0d,
                            native_key_code: keycode as i32,
                            character: b'\r' as u16,
                            unmodified_character: b'\r' as u16,
                            ..Default::default()
                        };
                        host.send_key_event(Some(&enter_char));
                    } else if let Some(unicode_char) = gdk::keyval_to_unicode(keyval) {
                        if !unicode_char.is_control() {
                            let mut utf16_buf = [0u16; 2];
                            for &unit in unicode_char.encode_utf16(&mut utf16_buf).iter() {
                                let char_ev = KeyEvent {
                                    type_: KeyEventType::CHAR,
                                    modifiers: cef_mods,
                                    windows_key_code: i32::from(unit),
                                    native_key_code: keycode as i32,
                                    character: unit,
                                    unmodified_character: unmod_ch,
                                    ..Default::default()
                                };
                                host.send_key_event(Some(&char_ev));
                            }
                        }
                    }
                }
                true
            });

            let v_up = self.clone();
            key.connect_key_released(move |keyval, keycode, raw_state| {
                let mods = ModifierType::from_bits_truncate(raw_state);
                let Some(host) = v_up.host() else {
                    return;
                };
                let key_up = KeyEvent {
                    type_: KeyEventType::KEYUP,
                    modifiers: gdk_mods_to_cef(mods),
                    windows_key_code: gdk_keyval_to_windows_vk(keyval),
                    native_key_code: keycode as i32,
                    is_system_key: i32::from(mods.contains(ModifierType::ALT_MASK)),
                    ..Default::default()
                };
                host.send_key_event(Some(&key_up));
            });

            self.shared.picture.add_controller(&key);
        }
    }
}

fn build_cef_client(shared: Rc<SharedState>) -> Client {
    let render_handler = SearchRenderHandler::new(shared.clone());
    let display_handler = SearchDisplayHandler::new(shared.clone());
    let load_handler = SearchLoadHandler::new(shared.clone());
    let life_span_handler = SearchLifeSpanHandler::new(shared.clone());
    let find_handler = SearchFindHandler::new(shared.clone());
    let download_handler = SearchDownloadHandler::new(shared.clone());
    let cookie_filter = SearchCookieFilter::new(shared.clone());
    let resource_req_handler = SearchResourceRequestHandler::new(shared.clone(), cookie_filter);
    let request_handler = SearchRequestHandler::new(shared, resource_req_handler);

    SearchClient::new(
        render_handler,
        display_handler,
        load_handler,
        life_span_handler,
        find_handler,
        download_handler,
        request_handler,
    )
}

wrap_render_handler! {
    struct SearchRenderHandler {
        shared: Rc<SharedState>,
    }

    impl RenderHandler {
        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            if let Some(rect) = rect {
                let (w, h) = self.shared.logical_size.get();
                rect.x = 0;
                rect.y = 0;
                rect.width = w.max(1);
                rect.height = h.max(1);
            }
        }

        fn screen_info(
            &self,
            _browser: Option<&mut Browser>,
            screen_info: Option<&mut ScreenInfo>,
        ) -> ::std::os::raw::c_int {
            if let Some(info) = screen_info {
                let (w, h) = self.shared.logical_size.get();
                let r = Rect {
                    x: 0,
                    y: 0,
                    width: w.max(1),
                    height: h.max(1),
                };
                info.device_scale_factor = self.shared.scale_factor.get().max(1.0);
                info.depth = 32;
                info.depth_per_component = 8;
                info.is_monochrome = 0;
                info.rect = r.clone();
                info.available_rect = r;
                return 1;
            }
            0
        }

        fn on_paint(
            &self,
            _browser: Option<&mut Browser>,
            type_: PaintElementType,
            _dirty_rects: Option<&[Rect]>,
            buffer: *const u8,
            width: ::std::os::raw::c_int,
            height: ::std::os::raw::c_int,
        ) {
            if type_ != PaintElementType::VIEW || buffer.is_null() || width <= 0 || height <= 0 {
                return;
            }
            let stride = (width as usize).saturating_mul(4);
            let total_bytes = stride.saturating_mul(height as usize);
            if total_bytes == 0 {
                return;
            }

            let slice = unsafe { std::slice::from_raw_parts(buffer, total_bytes) };
            let gbytes = Bytes::from_slice(slice);
            let texture = MemoryTexture::new(
                width,
                height,
                MemoryFormat::B8g8r8a8Premultiplied,
                &gbytes,
                stride,
            );
            self.shared.picture.set_texture(Some(&texture));

            let next_total = self.shared.paint_frames.get().saturating_add(1);
            self.shared.paint_frames.set(next_total);

            if slice.iter().any(|&b| b != 0) {
                let next_non_empty = self.shared.non_empty_frames.get().saturating_add(1);
                self.shared.non_empty_frames.set(next_non_empty);
                if let Some(ref cb) = self.shared.callbacks.borrow().on_non_empty_paint {
                    cb(next_non_empty, width, height);
                }
            }
        }
    }
}

wrap_download_image_callback! {
    struct FaviconDownloadCallback {
        shared: Rc<SharedState>,
    }

    impl DownloadImageCallback {
        fn on_download_image_finished(
            &self,
            _image_url: Option<&CefString>,
            http_status_code: ::std::os::raw::c_int,
            image: Option<&mut Image>,
        ) {
            if http_status_code != 200 && http_status_code != 0 {
                return;
            }
            let Some(img) = image else {
                return;
            };
            if img.is_empty() != 0 {
                return;
            }

            // 1. Ruta directa sin decodificador PNG/pixbuf/glycin: `as_bitmap` BGRA premultiplicado -> `GdkMemoryTexture`.
            let mut texture: Option<Texture> = None;
            for scale in [2.0_f32, 1.0_f32] {
                let mut pw = 0;
                let mut ph = 0;
                if let Some(bin) = img.as_bitmap(
                    scale,
                    ColorType::BGRA_8888,
                    AlphaType::PREMULTIPLIED,
                    Some(&mut pw),
                    Some(&mut ph),
                ) {
                    if pw > 0 && ph > 0 && pw <= 512 && ph <= 512 {
                        let expected = (pw as usize)
                            .saturating_mul(ph as usize)
                            .saturating_mul(4);
                        let size = bin.size();
                        if expected > 0 && size >= expected && size <= 2 * 1024 * 1024 {
                            let mut buf = vec![0u8; size];
                            let copied = bin.data(Some(&mut buf), 0);
                            if copied >= expected {
                                buf.truncate(expected);
                                let gbytes = Bytes::from_slice(&buf);
                                let mem = MemoryTexture::new(
                                    pw,
                                    ph,
                                    MemoryFormat::B8g8r8a8Premultiplied,
                                    &gbytes,
                                    (pw as usize) * 4,
                                );
                                texture = Some(mem.to_texture());
                                break;
                            }
                        }
                    }
                }
            }

            // 2. Respaldo PNG vía `gdk_texture_new_from_bytes` si `as_bitmap` no devolvió píxeles.
            if texture.is_none() {
                for scale in [2.0_f32, 1.0_f32] {
                    let mut pw = 0;
                    let mut ph = 0;
                    if let Some(bin) = img.as_png(scale, 1, Some(&mut pw), Some(&mut ph)) {
                        let size = bin.size();
                        if size > 0 && size <= 512 * 1024 {
                            let mut buf = vec![0u8; size];
                            let copied = bin.data(Some(&mut buf), 0);
                            buf.truncate(copied);
                            if !buf.is_empty() {
                                let gbytes = Bytes::from_slice(&buf);
                                if let Some(tex) = Texture::from_bytes(&gbytes) {
                                    texture = Some(tex);
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            if let Some(tex) = texture {
                if let Some(ref cb) = self.shared.callbacks.borrow().on_favicon_texture {
                    cb(tex);
                }
            }
        }
    }
}

wrap_display_handler! {
    struct SearchDisplayHandler {
        shared: Rc<SharedState>,
    }

    impl DisplayHandler {
        fn on_address_change(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            url: Option<&CefString>,
        ) {
            let is_main = frame.map(|f| f.is_main() != 0).unwrap_or(true);
            if !is_main {
                return;
            }
            let Some(url_str) = url.map(CefString::to_string) else {
                return;
            };
            // No pisar la dirección visible con páginas internas `data:text/html` generadas por
            // nuestros propios paneles de error de red o advertencia TLS.
            if url_str.starts_with("data:text/html;charset=utf-8,<!--search-internal-->") {
                return;
            }
            *self.shared.current_url.borrow_mut() = url_str.clone();
            if let Some(ref cb) = self.shared.callbacks.borrow().on_address_changed {
                cb(url_str);
            }
        }

        fn on_title_change(&self, _browser: Option<&mut Browser>, title: Option<&CefString>) {
            let new_title = title.map(CefString::to_string).unwrap_or_default();
            *self.shared.title.borrow_mut() = new_title.clone();
            if let Some(ref cb) = self.shared.callbacks.borrow().on_title_changed {
                cb(new_title);
            }
        }

        fn on_favicon_urlchange(
            &self,
            browser: Option<&mut Browser>,
            icon_urls: Option<&mut CefStringList>,
        ) {
            let candidates: Vec<String> = icon_urls
                .map(|list| list.clone().into_iter().filter(|s| !s.is_empty()).collect())
                .unwrap_or_default();
            // Preferir PNG/ICO/WebP/HTTP sobre SVG cuando haya varias etiquetas <link rel="icon">.
            let first = candidates
                .iter()
                .find(|u| !u.to_ascii_lowercase().ends_with(".svg"))
                .or_else(|| candidates.first())
                .cloned();
            *self.shared.favicon_url.borrow_mut() = first.clone();
            if let Some(ref cb) = self.shared.callbacks.borrow().on_favicon_changed {
                cb(first.clone());
            }
            if let Some(ref url) = first {
                let already =
                    self.shared.last_favicon_requested.borrow().as_deref() == Some(url.as_str());
                if !already {
                    *self.shared.last_favicon_requested.borrow_mut() = Some(url.clone());
                    if let Some(host) = browser.and_then(|b| b.host()) {
                        let cef_url = CefString::from(url.as_str());
                        let mut dl_cb = FaviconDownloadCallback::new(self.shared.clone());
                        host.download_image(Some(&cef_url), 1, 64, 0, Some(&mut dl_cb));
                    }
                }
            }
        }

        fn on_console_message(
            &self,
            _browser: Option<&mut Browser>,
            _level: LogSeverity,
            message: Option<&CefString>,
            _source: Option<&CefString>,
            _line: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            let Some(msg) = message.map(CefString::to_string) else {
                return 0;
            };
            let prefix = format!("__search_audio_{}:", self.shared.audio_nonce);
            if let Some(flag) = msg.strip_prefix(&prefix) {
                let playing = flag.trim() == "1";
                let changed = self.shared.is_audible.replace(playing) != playing;
                if changed {
                    let muted = self.shared.is_muted.get();
                    if let Some(ref cb) = self.shared.callbacks.borrow().on_audio_state_changed {
                        cb(playing, muted);
                    }
                }
                return 1;
            }
            let ipc_prefix = format!("__search_ipc_{}:", self.shared.ipc_nonce);
            if let Some(json_str) = msg.strip_prefix(&ipc_prefix) {
                match serde_json::from_str::<PageIpcMessage>(json_str) {
                    Ok(PageIpcMessage::CurtainPick { snapshot }) => {
                        if let Some(ref cb) = self.shared.callbacks.borrow().on_curtain_pick {
                            cb(snapshot);
                        }
                    }
                    Ok(PageIpcMessage::CurtainOff) => {
                        if self.shared.curtain_picking.replace(false) {
                            if let Some(ref cb) =
                                self.shared.callbacks.borrow().on_curtain_picking_changed
                            {
                                cb(false);
                            }
                        }
                    }
                    Ok(PageIpcMessage::CurtainError { message }) => {
                        search_core::warn!("Curtain error en página: {message}");
                    }
                    Ok(PageIpcMessage::ReaderArticle {
                        ok,
                        reason,
                        article,
                    }) => {
                        if let Some(ref cb) = self.shared.callbacks.borrow().on_reader_extracted {
                            cb(ok, reason, article);
                        }
                    }
                    Err(err) => {
                        search_core::warn!("Mensaje IPC inválido descartado: {err}");
                    }
                }
                return 1;
            }
            0
        }

        fn on_fullscreen_mode_change(
            &self,
            _browser: Option<&mut Browser>,
            fullscreen: ::std::os::raw::c_int,
        ) {
            if let Some(ref cb) = self.shared.callbacks.borrow().on_fullscreen_changed {
                cb(fullscreen != 0);
            }
        }

        fn on_status_message(&self, _browser: Option<&mut Browser>, value: Option<&CefString>) {
            let raw = value.map(CefString::to_string).unwrap_or_default();
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                self.shared.status_pill.set_visible(false);
                self.shared.status_pill.set_text("");
            } else {
                let clean = sanitize_url_for_ui(trimmed);
                self.shared.status_pill.set_text(&clean);
                self.shared.status_pill.set_visible(true);
            }
        }

        fn on_loading_progress_change(&self, _browser: Option<&mut Browser>, progress: f64) {
            let clamped = progress.clamp(0.0, 1.0);
            self.shared.progress.set(clamped);
            if let Some(ref cb) = self.shared.callbacks.borrow().on_progress_changed {
                cb(clamped);
            }
        }

        fn on_cursor_change(
            &self,
            _browser: Option<&mut Browser>,
            _cursor: ::std::os::raw::c_ulong,
            type_: CursorType,
            _custom_cursor_info: Option<&CursorInfo>,
        ) -> ::std::os::raw::c_int {
            let css_cursor = cef_cursor_to_css_name(type_);
            self.shared.picture.set_cursor_from_name(Some(css_cursor));
            1
        }
    }
}

wrap_load_handler! {
    struct SearchLoadHandler {
        shared: Rc<SharedState>,
    }

    impl LoadHandler {
        fn on_loading_state_change(
            &self,
            _browser: Option<&mut Browser>,
            is_loading: ::std::os::raw::c_int,
            can_go_back: ::std::os::raw::c_int,
            can_go_forward: ::std::os::raw::c_int,
        ) {
            let loading = is_loading != 0;
            let back = can_go_back != 0;
            let fwd = can_go_forward != 0;
            self.shared.is_loading.set(loading);
            self.shared.can_go_back.set(back);
            self.shared.can_go_forward.set(fwd);
            if let Some(ref cb) = self.shared.callbacks.borrow().on_loading_state_changed {
                cb(loading, back, fwd);
            }
        }

        fn on_load_start(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _transition_type: TransitionType,
        ) {
            let Some(frame) = frame else {
                return;
            };
            let is_main = frame.is_main() != 0;
            let url = CefString::from(&frame.url()).to_string();
            let is_internal = url.starts_with("data:text/html;charset=utf-8,<!--search-internal-->");

            if is_main && !is_internal {
                self.shared.last_load_was_error.set(false);
                *self.shared.favicon_url.borrow_mut() = None;
                self.shared.blocked_count.store(0, Ordering::Relaxed);
                if let Ok(mut guard) = self.shared.top_url_shared.write() {
                    *guard = url.clone();
                }
                if self.shared.curtain_picking.replace(false) {
                    if let Some(ref cb) = self.shared.callbacks.borrow().on_curtain_picking_changed
                    {
                        cb(false);
                    }
                }
                if self.shared.reader_original_url.borrow_mut().take().is_some() {
                    if let Some(ref cb) = self.shared.callbacks.borrow().on_reader_state_changed {
                        cb(false);
                    }
                }
            }

            if !is_internal && (url.starts_with("http://") || url.starts_with("https://")) {
                // Inyección temprana de CSS cosmético de Shield (sin parpadeo).
                if is_main {
                    if let Some(cosmetic_js) = self.shared.shield.cosmetic_inject_script_for_url(&url)
                    {
                        let cef_js = CefString::from(cosmetic_js.as_str());
                        let src = CefString::from("search://shield-cosmetic");
                        frame.execute_java_script(Some(&cef_js), Some(&src), 1);
                    }
                    // Inyección temprana de reglas Curtain por dominio (sin parpadeo).
                    if let Some(veil_css) = self.shared.curtain.css_for_url(&url) {
                        let veil_js = build_veil_style_script(&veil_css);
                        let cef_js = CefString::from(veil_js.as_str());
                        let src = CefString::from("search://curtain-css");
                        frame.execute_java_script(Some(&cef_js), Some(&src), 1);
                    }
                }

                // Scripts anti-anuncios en hosts reproductores de YouTube (`YouTubeAds.swift`).
                if let Some((page_js, skipper_js)) = self.shared.shield.youtube_scripts_for_url(&url)
                {
                    let cef_page = CefString::from(page_js.as_str());
                    let src_page = CefString::from("search://shield-yt-page");
                    frame.execute_java_script(Some(&cef_page), Some(&src_page), 1);

                    let cef_skip = CefString::from(skipper_js.as_str());
                    let src_skip = CefString::from("search://shield-yt-skip");
                    frame.execute_java_script(Some(&cef_skip), Some(&src_skip), 1);
                }
            }

            if self.shared.settings.with(|s| s.send_gpc) {
                let gpc_js = CefString::from(
                    "try{Object.defineProperty(Navigator.prototype,'globalPrivacyControl',{get:()=>true,configurable:true});}catch(_){}",
                );
                let src = CefString::from("search://gpc");
                frame.execute_java_script(Some(&gpc_js), Some(&src), 1);
            }
        }

        fn on_load_end(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            http_status_code: ::std::os::raw::c_int,
        ) {
            let Some(frame) = frame else {
                return;
            };
            if frame.is_main() == 0 {
                return;
            }
            let url = CefString::from(&frame.url()).to_string();
            if !url.starts_with("data:text/html;charset=utf-8,<!--search-internal-->")
                && url != "about:blank"
            {
                let audio_js = format!(
                    "(()=>{{if(window.__sAud)return;window.__sAud=1;const p='__search_audio_{}:';let last=false;const chk=()=>{{const ms=document.querySelectorAll('audio,video');let a=false;for(const m of ms){{if(!m.paused&&!m.ended&&!m.muted&&m.volume>0){{a=true;break;}}}}if(a!==last){{last=a;console.debug(p+(a?'1':'0'));}}}};document.addEventListener('play',chk,true);document.addEventListener('pause',chk,true);document.addEventListener('ended',chk,true);document.addEventListener('volumechange',chk,true);chk();}})();",
                    self.shared.audio_nonce
                );
                let cef_js = CefString::from(audio_js.as_str());
                let src = CefString::from("search://audio");
                frame.execute_java_script(Some(&cef_js), Some(&src), 1);

                // Respaldo automático `/favicon.ico` para sitios que no declaran `<link rel="icon">`.
                if self.shared.favicon_url.borrow().is_none() {
                    if let Ok(parsed) = url::Url::parse(&url) {
                        if matches!(parsed.scheme(), "http" | "https") {
                            if let Some(host_str) = parsed.host_str() {
                                let port_part = parsed
                                    .port()
                                    .map(|p| format!(":{p}"))
                                    .unwrap_or_default();
                                let fallback_ico =
                                    format!("{}://{}{}/favicon.ico", parsed.scheme(), host_str, port_part);
                                let already = self.shared.last_favicon_requested.borrow().as_deref()
                                    == Some(fallback_ico.as_str());
                                if !already {
                                    *self.shared.last_favicon_requested.borrow_mut() =
                                        Some(fallback_ico.clone());
                                    if let Some(host) = browser.and_then(|b| b.host()) {
                                        let cef_url = CefString::from(fallback_ico.as_str());
                                        let mut dl_cb =
                                            FaviconDownloadCallback::new(self.shared.clone());
                                        host.download_image(
                                            Some(&cef_url),
                                            1,
                                            64,
                                            0,
                                            Some(&mut dl_cb),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if let Some(ref cb) = self.shared.callbacks.borrow().on_load_finished {
                cb(url, http_status_code);
            }
        }

        fn on_load_error(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            error_code: Errorcode,
            error_text: Option<&CefString>,
            failed_url: Option<&CefString>,
        ) {
            if error_code == Errorcode::ABORTED {
                self.shared.suppress_next_abort_error.set(false);
                return;
            }
            if self.shared.suppress_next_abort_error.replace(false) {
                return;
            }
            let Some(frame) = frame else {
                return;
            };
            if frame.is_main() == 0 {
                return;
            }

            let failed = failed_url.map(CefString::to_string).unwrap_or_default();
            if failed.starts_with("data:") || failed.contains(TLS_OVERRIDE_HOST) {
                return;
            }
            let err_desc = error_text
                .map(CefString::to_string)
                .unwrap_or_else(|| "Error de red".to_string());

            self.shared.last_load_was_error.set(true);
            *self.shared.current_url.borrow_mut() = failed.clone();
            if let Some(ref cb) = self.shared.callbacks.borrow().on_address_changed {
                cb(failed.clone());
            }

            let html = build_error_page_html(&failed, &err_desc);
            let data_url = html_to_internal_data_url(&html);
            let cef_data = CefString::from(data_url.as_str());
            frame.load_url(Some(&cef_data));
        }
    }
}

wrap_life_span_handler! {
    struct SearchLifeSpanHandler {
        shared: Rc<SharedState>,
    }

    impl LifeSpanHandler {
        fn on_before_popup(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: ::std::os::raw::c_int,
            target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            target_disposition: WindowOpenDisposition,
            user_gesture: ::std::os::raw::c_int,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            // Bloquea popups no solicitados; los iniciados por gesto (`target=_blank`,
            // `window.open` de OAuth, clic central) se abren en una nueva pestaña de la
            // misma ventana conservando el mismo `RequestContext`.
            if user_gesture != 0 {
                if let Some(url) = target_url {
                    let url_s = url.to_string();
                    if !url_s.is_empty() {
                        let clean = if self.shared.settings.with(|s| s.strip_click_ids) {
                            strip_click_ids(&url_s)
                        } else {
                            url_s
                        };
                        let background =
                            target_disposition == WindowOpenDisposition::NEW_BACKGROUND_TAB;
                        if let Some(ref open_cb) = self.shared.callbacks.borrow().on_open_new_tab {
                            open_cb(clean, background);
                            return 1;
                        }
                        if let Some(main_frame) = browser.and_then(|b| b.main_frame()) {
                            let cs = CefString::from(clean.as_str());
                            main_frame.load_url(Some(&cs));
                        }
                    }
                }
            }
            1
        }
    }
}

wrap_find_handler! {
    struct SearchFindHandler {
        shared: Rc<SharedState>,
    }

    impl FindHandler {
        fn on_find_result(
            &self,
            _browser: Option<&mut Browser>,
            _identifier: ::std::os::raw::c_int,
            count: ::std::os::raw::c_int,
            _selection_rect: Option<&Rect>,
            active_match_ordinal: ::std::os::raw::c_int,
            _final_update: ::std::os::raw::c_int,
        ) {
            if let Some(ref cb) = self.shared.callbacks.borrow().on_find_result {
                cb(active_match_ordinal, count);
            }
        }
    }
}

wrap_cookie_access_filter! {
    struct SearchCookieFilter {
        shared: Rc<SharedState>,
    }

    impl CookieAccessFilter {
        fn can_send_cookie(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _cookie: Option<&Cookie>,
        ) -> ::std::os::raw::c_int {
            i32::from(allow_cookie_for_request(&self.shared, browser, frame, request))
        }

        fn can_save_cookie(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _response: Option<&mut Response>,
            _cookie: Option<&Cookie>,
        ) -> ::std::os::raw::c_int {
            i32::from(allow_cookie_for_request(&self.shared, browser, frame, request))
        }
    }
}

fn allow_cookie_for_request(
    shared: &SharedState,
    browser: Option<&mut Browser>,
    frame: Option<&mut Frame>,
    request: Option<&mut Request>,
) -> bool {
    if !shared.settings.with(|s| s.block_third_party_cookies()) {
        return true;
    }
    let Some(req) = request else {
        return true;
    };
    let req_url = CefString::from(&req.url()).to_string();
    let mut top_url = CefString::from(&req.first_party_for_cookies()).to_string();
    if top_url.is_empty() {
        if let Some(b) = browser {
            if let Some(mf) = b.main_frame() {
                top_url = CefString::from(&mf.url()).to_string();
            }
        }
    }
    if top_url.is_empty() {
        if let Some(f) = frame {
            top_url = CefString::from(&f.url()).to_string();
        }
    }
    if top_url.is_empty() || top_url == "about:blank" {
        return true;
    }
    is_same_site(&req_url, &top_url)
}

wrap_resource_request_handler! {
    struct SearchResourceRequestHandler {
        shared: Rc<SharedState>,
        cookie_filter: CookieAccessFilter,
    }

    impl ResourceRequestHandler {
        fn cookie_access_filter(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
        ) -> Option<CookieAccessFilter> {
            Some(self.cookie_filter.clone())
        }

        fn on_before_resource_load(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _callback: Option<&mut Callback>,
        ) -> ReturnValue {
            let Some(request) = request else {
                return ReturnValue::CONTINUE;
            };

            let req_url = CefString::from(&request.url()).to_string();
            let mut top_url = CefString::from(&request.first_party_for_cookies()).to_string();
            if top_url.is_empty() {
                if let Some(b) = browser {
                    if let Some(mf) = b.main_frame() {
                        top_url = CefString::from(&mf.url()).to_string();
                    }
                }
            }
            if top_url.is_empty() {
                if let Some(f) = frame {
                    top_url = CefString::from(&f.url()).to_string();
                }
            }
            if top_url.is_empty() {
                if let Ok(guard) = self.shared.top_url_shared.read() {
                    top_url = guard.clone();
                }
            }

            // Shield: bloqueo de red antes de enviar la petición (`Shield.swift` + `YouTubeAds.swift`).
            if self.shared.shield.should_block_request(&req_url, &top_url) {
                self.shared.blocked_count.fetch_add(1, Ordering::Relaxed);
                return ReturnValue::CANCEL;
            }

            let (send_gpc, strip_ids) = self
                .shared
                .settings
                .with(|s| (s.send_gpc, s.strip_click_ids));

            if send_gpc {
                let hdr_name = CefString::from("Sec-GPC");
                let hdr_val = CefString::from("1");
                request.set_header_by_name(Some(&hdr_name), Some(&hdr_val), 1);
            }

            if strip_ids && request.is_read_only() == 0 {
                let cleaned = strip_click_ids(&req_url);
                if cleaned != req_url {
                    let cs = CefString::from(cleaned.as_str());
                    request.set_url(Some(&cs));
                }
            }

            ReturnValue::CONTINUE
        }
    }
}

wrap_request_handler! {
    struct SearchRequestHandler {
        shared: Rc<SharedState>,
        resource_handler: ResourceRequestHandler,
    }

    impl RequestHandler {
        fn on_before_browse(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _user_gesture: ::std::os::raw::c_int,
            _is_redirect: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            let (Some(frame), Some(request)) = (frame, request) else {
                return 0;
            };
            if frame.is_main() == 0 {
                return 0;
            }

            let req_url = CefString::from(&request.url()).to_string();

            // Acción autenticada "Continuar a este sitio" desde nuestra advertencia TLS.
            if let Some(target_https) = try_parse_tls_override_url(&req_url, &self.shared.tls_nonce)
            {
                if let Ok(parsed) = url::Url::parse(&target_https) {
                    if let Some(host) = parsed.host_str() {
                        self.shared
                            .tls_allowed_hosts
                            .borrow_mut()
                            .insert(host.to_ascii_lowercase());
                        self.shared.suppress_next_abort_error.set(true);
                        let cs = CefString::from(target_https.as_str());
                        frame.load_url(Some(&cs));
                        return 1;
                    }
                }
            }

            if self.shared.settings.with(|s| s.strip_click_ids) {
                let cleaned = strip_click_ids(&req_url);
                if cleaned != req_url {
                    self.shared.suppress_next_abort_error.set(true);
                    let cs = CefString::from(cleaned.as_str());
                    frame.load_url(Some(&cs));
                    return 1;
                }
            }

            0
        }

        fn on_open_urlfrom_tab(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            target_url: Option<&CefString>,
            target_disposition: WindowOpenDisposition,
            _user_gesture: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            let opens_tab = matches!(
                target_disposition,
                WindowOpenDisposition::NEW_BACKGROUND_TAB
                    | WindowOpenDisposition::NEW_FOREGROUND_TAB
                    | WindowOpenDisposition::NEW_WINDOW
                    | WindowOpenDisposition::NEW_POPUP
            );
            if !opens_tab {
                return 0;
            }
            let Some(url) = target_url.map(CefString::to_string) else {
                return 0;
            };
            if url.is_empty() {
                return 0;
            }
            let clean = if self.shared.settings.with(|s| s.strip_click_ids) {
                strip_click_ids(&url)
            } else {
                url
            };
            let background = target_disposition == WindowOpenDisposition::NEW_BACKGROUND_TAB;
            if let Some(ref open_cb) = self.shared.callbacks.borrow().on_open_new_tab {
                open_cb(clean, background);
                return 1;
            }
            0
        }

        fn resource_request_handler(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
            _is_navigation: ::std::os::raw::c_int,
            _is_download: ::std::os::raw::c_int,
            _request_initiator: Option<&CefString>,
            _disable_default_handling: Option<&mut ::std::os::raw::c_int>,
        ) -> Option<ResourceRequestHandler> {
            Some(self.resource_handler.clone())
        }

        fn on_certificate_error(
            &self,
            browser: Option<&mut Browser>,
            cert_error: Errorcode,
            request_url: Option<&CefString>,
            _ssl_info: Option<&mut Sslinfo>,
            callback: Option<&mut Callback>,
        ) -> ::std::os::raw::c_int {
            let failed_url = request_url.map(CefString::to_string).unwrap_or_default();
            let host = url::Url::parse(&failed_url)
                .ok()
                .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
                .unwrap_or_default();

            if !host.is_empty() && self.shared.tls_allowed_hosts.borrow().contains(&host) {
                if let Some(cb) = callback {
                    cb.cont();
                    return 1;
                }
            }

            if let Some(cb) = callback {
                cb.cancel();
            }

            self.shared.suppress_next_abort_error.set(true);
            self.shared.last_load_was_error.set(true);
            *self.shared.current_url.borrow_mut() = failed_url.clone();
            if let Some(ref addr_cb) = self.shared.callbacks.borrow().on_address_changed {
                addr_cb(failed_url.clone());
            }

            if let Some(browser) = browser {
                if let Some(main_frame) = browser.main_frame() {
                    let html = build_tls_warning_html(
                        &host,
                        &failed_url,
                        cert_error,
                        &self.shared.tls_nonce,
                    );
                    let data_url = html_to_internal_data_url(&html);
                    let cs = CefString::from(data_url.as_str());
                    main_frame.load_url(Some(&cs));
                }
            }
            1
        }
    }
}

wrap_download_handler! {
    struct SearchDownloadHandler {
        shared: Rc<SharedState>,
    }

    impl DownloadHandler {
        fn can_download(
            &self,
            _browser: Option<&mut Browser>,
            _url: Option<&CefString>,
            _request_method: Option<&CefString>,
        ) -> ::std::os::raw::c_int {
            1
        }

        fn on_before_download(
            &self,
            _browser: Option<&mut Browser>,
            download_item: Option<&mut DownloadItem>,
            suggested_name: Option<&CefString>,
            callback: Option<&mut BeforeDownloadCallback>,
        ) -> ::std::os::raw::c_int {
            let (Some(item), Some(cb)) = (download_item, callback) else {
                return 0;
            };
            let url = {
                let u = CefString::from(&item.url()).to_string();
                if u.is_empty() {
                    CefString::from(&item.original_url()).to_string()
                } else {
                    u
                }
            };
            let name = suggested_name
                .map(CefString::to_string)
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| CefString::from(&item.suggested_file_name()).to_string());

            let ev = DownloadBeforeEvent {
                cef_id: item.id(),
                url,
                suggested_name: name,
                total_bytes: item.total_bytes(),
                callback: cb.clone(),
            };

            let has_handler = self.shared.callbacks.borrow().on_before_download.is_some();
            if has_handler {
                if let Some(ref handler) = self.shared.callbacks.borrow().on_before_download {
                    handler(ev);
                }
                1
            } else {
                0
            }
        }

        fn on_download_updated(
            &self,
            _browser: Option<&mut Browser>,
            download_item: Option<&mut DownloadItem>,
            callback: Option<&mut DownloadItemCallback>,
        ) {
            let Some(item) = download_item else {
                return;
            };
            if item.is_valid() == 0 {
                return;
            }
            let url = {
                let u = CefString::from(&item.url()).to_string();
                if u.is_empty() {
                    CefString::from(&item.original_url()).to_string()
                } else {
                    u
                }
            };
            let ev = DownloadUpdateEvent {
                cef_id: item.id(),
                url,
                full_path: CefString::from(&item.full_path()).to_string(),
                suggested_name: CefString::from(&item.suggested_file_name()).to_string(),
                received_bytes: item.received_bytes(),
                total_bytes: item.total_bytes(),
                percent_complete: item.percent_complete(),
                current_speed: item.current_speed(),
                is_in_progress: item.is_in_progress() != 0,
                is_complete: item.is_complete() != 0,
                is_canceled: item.is_canceled() != 0,
                is_interrupted: item.is_interrupted() != 0,
                is_paused: item.is_paused() != 0,
                callback: callback.map(|c| c.clone()),
            };
            if let Some(ref handler) = self.shared.callbacks.borrow().on_download_updated {
                handler(ev);
            }
        }
    }
}

wrap_client! {
    struct SearchClient {
        render_handler: RenderHandler,
        display_handler: DisplayHandler,
        load_handler: LoadHandler,
        life_span_handler: LifeSpanHandler,
        find_handler: FindHandler,
        download_handler: DownloadHandler,
        request_handler: RequestHandler,
    }

    impl Client {
        fn render_handler(&self) -> Option<RenderHandler> {
            Some(self.render_handler.clone())
        }

        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(self.display_handler.clone())
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(self.load_handler.clone())
        }

        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(self.life_span_handler.clone())
        }

        fn find_handler(&self) -> Option<FindHandler> {
            Some(self.find_handler.clone())
        }

        fn download_handler(&self) -> Option<DownloadHandler> {
            Some(self.download_handler.clone())
        }

        fn request_handler(&self) -> Option<RequestHandler> {
            Some(self.request_handler.clone())
        }
    }
}

fn try_parse_tls_override_url(raw_url: &str, expected_nonce: &str) -> Option<String> {
    let parsed = url::Url::parse(raw_url).ok()?;
    if parsed.host_str() != Some(TLS_OVERRIDE_HOST) {
        return None;
    }
    let mut token_ok = false;
    let mut target: Option<String> = None;
    for (k, v) in parsed.query_pairs() {
        if k == "token" && v == expected_nonce {
            token_ok = true;
        } else if k == "target" {
            let decoded = percent_decode_str(&v).decode_utf8_lossy().into_owned();
            if decoded.starts_with("https://") {
                target = Some(decoded);
            }
        }
    }
    if token_ok {
        target
    } else {
        None
    }
}

fn html_to_internal_data_url(html: &str) -> String {
    let encoded = utf8_percent_encode(html, NON_ALPHANUMERIC).to_string();
    format!("data:text/html;charset=utf-8,<!--search-internal-->{encoded}")
}

fn reader_html_to_internal_data_url(html: &str) -> String {
    let body = html.strip_prefix(READER_INTERNAL_MARKER).unwrap_or(html);
    let encoded = utf8_percent_encode(body, NON_ALPHANUMERIC).to_string();
    format!("data:text/html;charset=utf-8,<!--search-internal-->{encoded}")
}

fn escape_html(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn build_error_page_html(failed_url: &str, error_text: &str) -> String {
    let safe_display = escape_html(&sanitize_url_for_ui(failed_url));
    let safe_err = escape_html(error_text);
    let safe_href = escape_html(failed_url);
    format!(
        r#"<!doctype html>
<html lang="es">
<head>
<meta charset="utf-8">
<title>No se pudo abrir la página</title>
<style>
  :root {{ color-scheme: light dark; }}
  body {{
    margin: 0; min-height: 100vh; display: flex; align-items: center; justify-content: center;
    font: 15px/1.55 system-ui, -apple-system, sans-serif;
    background: #fafafa; color: #1c1c1e; padding: 24px; box-sizing: border-box;
  }}
  @media (prefers-color-scheme: dark) {{
    body {{ background: #1c1c1e; color: #f2f2f7; }}
    .card {{ border-color: rgba(255,255,255,0.12); background: rgba(255,255,255,0.03); }}
    .btn {{ background: #f2f2f7; color: #1c1c1e; }}
  }}
  .card {{
    max-width: 480px; width: 100%; padding: 28px 30px; border-radius: 14px;
    border: 1px solid rgba(0,0,0,0.10); background: #ffffff;
  }}
  h1 {{ font-size: 18px; font-weight: 600; margin: 0 0 10px; }}
  p {{ margin: 0 0 14px; opacity: 0.80; word-break: break-word; }}
  code {{ font-size: 12.5px; opacity: 0.65; display: block; margin-bottom: 20px; }}
  .btn {{
    display: inline-block; text-decoration: none; font-weight: 500; font-size: 14px;
    padding: 8px 16px; border-radius: 8px; background: #1c1c1e; color: #ffffff;
  }}
</style>
</head>
<body>
  <main class="card">
    <h1>No se pudo abrir esta página</h1>
    <p>{safe_display}</p>
    <code>{safe_err}</code>
    <a class="btn" href="{safe_href}">Reintentar</a>
  </main>
</body>
</html>"#
    )
}

fn build_tls_warning_html(
    host: &str,
    failed_url: &str,
    cert_error: Errorcode,
    nonce: &str,
) -> String {
    let safe_host = escape_html(if host.is_empty() { failed_url } else { host });
    let encoded_target = utf8_percent_encode(failed_url, NON_ALPHANUMERIC).to_string();
    let override_href =
        format!("https://{TLS_OVERRIDE_HOST}/?token={nonce}&target={encoded_target}");
    let err_num = *cert_error.as_ref() as i32;
    format!(
        r#"<!doctype html>
<html lang="es">
<head>
<meta charset="utf-8">
<title>Conexión no privada</title>
<style>
  :root {{ color-scheme: light dark; }}
  body {{
    margin: 0; min-height: 100vh; display: flex; align-items: center; justify-content: center;
    font: 15px/1.55 system-ui, -apple-system, sans-serif;
    background: #fafafa; color: #1c1c1e; padding: 24px; box-sizing: border-box;
  }}
  @media (prefers-color-scheme: dark) {{
    body {{ background: #1c1c1e; color: #f2f2f7; }}
    .card {{ border-color: rgba(255,159,10,0.35); background: rgba(255,255,255,0.03); }}
    .primary {{ background: #f2f2f7; color: #1c1c1e; }}
  }}
  .card {{
    max-width: 500px; width: 100%; padding: 28px 30px; border-radius: 14px;
    border: 1px solid rgba(217,119,6,0.40); background: #ffffff;
  }}
  h1 {{ font-size: 18px; font-weight: 600; margin: 0 0 10px; }}
  p {{ margin: 0 0 14px; opacity: 0.84; word-break: break-word; }}
  code {{ font-size: 12.5px; opacity: 0.65; display: block; margin-bottom: 20px; }}
  .actions {{ display: flex; gap: 12px; align-items: center; flex-wrap: wrap; }}
  .primary {{
    border: 0; cursor: pointer; font-weight: 500; font-size: 14px;
    padding: 8px 16px; border-radius: 8px; background: #1c1c1e; color: #ffffff;
  }}
  .proceed {{ font-size: 13.5px; color: #c2410c; text-decoration: underline; }}
</style>
</head>
<body>
  <main class="card">
    <h1>El certificado de este sitio no es de confianza</h1>
    <p>Search ha bloqueado la conexión a <strong>{safe_host}</strong> porque su certificado TLS no es válido. Podría tratarse de un servidor mal configurado o de un intento de interceptar tu conexión.</p>
    <code>Código TLS: {err_num}</code>
    <div class="actions">
      <button class="primary" onclick="history.back()">Volver atrás</button>
      <a class="proceed" href="{override_href}">Continuar a este sitio (no seguro)</a>
    </div>
  </main>
</body>
</html>"#
    )
}

fn generate_hex_nonce_128() -> String {
    let mut buf = [0u8; 16];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = f.read_exact(&mut buf);
    } else {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0x9e3779b97f4a7c15);
        buf.copy_from_slice(&nanos.to_le_bytes());
    }
    let mut out = String::with_capacity(32);
    for b in buf {
        use std::fmt::Write;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

fn gdk_button_to_cef(btn: u32) -> Option<MouseButtonType> {
    match btn {
        1 => Some(MouseButtonType::LEFT),
        2 => Some(MouseButtonType::MIDDLE),
        3 => Some(MouseButtonType::RIGHT),
        _ => None,
    }
}

fn gdk_mods_to_cef(mods: ModifierType) -> u32 {
    let mut flags = 0u32;
    if mods.contains(ModifierType::LOCK_MASK) {
        flags |= CEF_EVENTFLAG_CAPS_LOCK_ON;
    }
    if mods.contains(ModifierType::SHIFT_MASK) {
        flags |= CEF_EVENTFLAG_SHIFT_DOWN;
    }
    if mods.contains(ModifierType::CONTROL_MASK) {
        flags |= CEF_EVENTFLAG_CONTROL_DOWN;
    }
    if mods.contains(ModifierType::ALT_MASK) {
        flags |= CEF_EVENTFLAG_ALT_DOWN;
    }
    if mods.contains(ModifierType::BUTTON1_MASK) {
        flags |= CEF_EVENTFLAG_LEFT_MOUSE_BUTTON;
    }
    if mods.contains(ModifierType::BUTTON2_MASK) {
        flags |= CEF_EVENTFLAG_MIDDLE_MOUSE_BUTTON;
    }
    if mods.contains(ModifierType::BUTTON3_MASK) {
        flags |= CEF_EVENTFLAG_RIGHT_MOUSE_BUTTON;
    }
    if mods.intersects(ModifierType::SUPER_MASK | ModifierType::META_MASK) {
        flags |= CEF_EVENTFLAG_COMMAND_DOWN;
    }
    flags
}

fn should_propagate_shortcut_to_window(keyval: u32, mods: ModifierType, is_loading: bool) -> bool {
    let lower = gdk::keyval_to_lower(keyval);
    let ctrl = mods.contains(ModifierType::CONTROL_MASK);
    let alt = mods.contains(ModifierType::ALT_MASK);
    let shift = mods.contains(ModifierType::SHIFT_MASK);

    if matches!(
        keyval,
        gdk::keys::F5 | gdk::keys::F6 | gdk::keys::F9 | gdk::keys::F11 | gdk::keys::F12
    ) {
        return true;
    }

    if keyval == gdk::keys::ESCAPE && is_loading {
        return true;
    }

    if alt && !ctrl && !shift && matches!(lower, gdk::keys::LEFT | gdk::keys::RIGHT | 0x64) {
        // Alt+Left, Alt+Right, Alt+D
        return true;
    }

    if ctrl && !alt {
        match lower {
            // Ctrl+L, Ctrl+F, Ctrl+G, Ctrl+R, Ctrl+Q, Ctrl+T, Ctrl+W, Ctrl+K, Ctrl+N, Ctrl+S,
            // Ctrl+H, Ctrl+Y, Ctrl+D
            0x6c | 0x66 | 0x67 | 0x72 | 0x71 | 0x74 | 0x77 | 0x6b | 0x6e | 0x73 | 0x68 | 0x79
            | 0x64 => return true,
            // Ctrl+Shift+I, C, V, M, B, O, J, E (Curtain panel), H (Curtain pick)
            0x69 | 0x63 | 0x76 | 0x6d | 0x62 | 0x6f | 0x6a | 0x65 if shift => return true,
            // Ctrl+1..9
            gdk::keys::KEY_1..=gdk::keys::KEY_9 => return true,
            // Cambio de pestañas, preferencias y atajos
            gdk::keys::TAB
            | gdk::keys::ISO_LEFT_TAB
            | gdk::keys::PAGE_UP
            | gdk::keys::PAGE_DOWN
            | gdk::keys::COMMA
            | gdk::keys::QUESTION => return true,
            gdk::keys::SLASH if shift => return true,
            // Zoom y navegación Ctrl+[ / Ctrl+]
            gdk::keys::PLUS
            | gdk::keys::EQUAL
            | gdk::keys::MINUS
            | gdk::keys::ZERO
            | gdk::keys::KP_ADD
            | gdk::keys::KP_SUBTRACT
            | gdk::keys::KP_0
            | gdk::keys::BRACKET_LEFT
            | gdk::keys::BRACKET_RIGHT => return true,
            _ => {}
        }
    }

    if ctrl && alt && !shift && matches!(lower, 0x72 | 0x64) {
        // Ctrl+Alt+R, Ctrl+Alt+D (duplicar pestaña)
        return true;
    }

    false
}

fn gdk_keyval_to_windows_vk(keyval: u32) -> i32 {
    let lower = gdk::keyval_to_lower(keyval);
    match lower {
        gdk::keys::BACKSPACE => 0x08,
        gdk::keys::TAB | gdk::keys::ISO_LEFT_TAB => 0x09,
        gdk::keys::RETURN | gdk::keys::KP_ENTER => 0x0D,
        gdk::keys::SHIFT_L | gdk::keys::SHIFT_R => 0x10,
        gdk::keys::CONTROL_L | gdk::keys::CONTROL_R => 0x11,
        gdk::keys::ALT_L | gdk::keys::ALT_R => 0x12,
        gdk::keys::ESCAPE => 0x1B,
        gdk::keys::SPACE => 0x20,
        gdk::keys::PAGE_UP => 0x21,
        gdk::keys::PAGE_DOWN => 0x22,
        gdk::keys::END => 0x23,
        gdk::keys::HOME => 0x24,
        gdk::keys::LEFT => 0x25,
        gdk::keys::UP => 0x26,
        gdk::keys::RIGHT => 0x27,
        gdk::keys::DOWN => 0x28,
        gdk::keys::INSERT => 0x2D,
        gdk::keys::DELETE => 0x2E,
        0x30..=0x39 => lower as i32,
        0x61..=0x7a => (lower - 0x20) as i32, // 'A'..'Z'
        gdk::keys::F1..=gdk::keys::F12 => (0x70 + (lower - gdk::keys::F1)) as i32,
        gdk::keys::MINUS | gdk::keys::KP_SUBTRACT => 0xBD,
        gdk::keys::EQUAL | gdk::keys::PLUS | gdk::keys::KP_ADD => 0xBB,
        gdk::keys::BRACKET_LEFT => 0xDB,
        gdk::keys::BRACKET_RIGHT => 0xDD,
        _ => lower as i32,
    }
}

fn cef_cursor_to_css_name(cursor: CursorType) -> &'static str {
    if cursor == CursorType::HAND {
        "pointer"
    } else if cursor == CursorType::IBEAM {
        "text"
    } else if cursor == CursorType::CROSS {
        "crosshair"
    } else if cursor == CursorType::WAIT {
        "wait"
    } else if cursor == CursorType::HELP {
        "help"
    } else if cursor == CursorType::EASTWESTRESIZE
        || cursor == CursorType::COLUMNRESIZE
        || cursor == CursorType::EASTRESIZE
        || cursor == CursorType::WESTRESIZE
    {
        "ew-resize"
    } else if cursor == CursorType::NORTHSOUTHRESIZE
        || cursor == CursorType::ROWRESIZE
        || cursor == CursorType::NORTHRESIZE
        || cursor == CursorType::SOUTHRESIZE
    {
        "ns-resize"
    } else {
        "default"
    }
}
