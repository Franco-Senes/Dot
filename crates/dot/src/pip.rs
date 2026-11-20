//! Picture-in-Picture flotante 16:9 sin decoraciones (`Float.swift`).
//!
//! Mueve el `BrowserView` de la pestaña a una `GtkWindow` flotante sin bordes
//! con `GtkAspectFrame` (16:9), `GtkWindowHandle` (arrastrable desde cualquier punto)
//! y controles flotantes en hover (`better-ui`): salto `-10s` / `+10s`,
//! reproducir/pausa, selector de velocidad (`1x`, `1.25x`, `1.5x`, `2x`),
//! silencio, opacidad (`100%` / `85%`), volver a la pestaña y cerrar.
//!
//! Limitación de Wayland: los clientes estándar de Wayland no pueden forzar
//! *always-on-top* ni posicionamiento global absoluto; el usuario puede fijar
//! "Siempre encima" desde el gestor de ventanas si su compositor lo ofrece.

use crate::browser_view::BrowserView;
use crate::icons::{self, IconId};
use gnome_platform::adw::ApplicationWindow;
use gnome_platform::gdk::Display;
use gnome_platform::gtk::{
    Align, AsWidget, AspectFrame, BoxWidget, Button, CssProvider, EventControllerMotion, Label,
    Orientation, Overlay, Revealer, RevealerTransitionType, Window, WindowHandle,
    STYLE_PROVIDER_PRIORITY_APPLICATION,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Once;

const DEFAULT_PIP_WIDTH: i32 = 440;
const DEFAULT_PIP_HEIGHT: i32 = 248; // ~16:9

/// Velocidades de reproducción soportadas por el selector de PiP.
pub const PIP_PLAYBACK_RATES: [f64; 4] = [1.0, 1.25, 1.5, 2.0];

const PIP_CSS: &str = r#"
window.search-pip-window {
    transition-property: opacity;
    transition-duration: 150ms;
    transition-timing-function: cubic-bezier(0.2, 0, 0, 1);
}

window.search-pip-window.pip-translucent {
    opacity: 0.85;
}

box.search-pip-controls {
    margin-top: 8px;
    margin-right: 8px;
    padding: 4px 6px;
    border-radius: 999px;
    background-color: alpha(#141416, 0.76);
    border: 1px solid alpha(#ffffff, 0.12);
    box-shadow: 0 4px 12px alpha(#000000, 0.28);
}

button.search-pip-btn,
togglebutton.search-pip-btn {
    min-width: 24px;
    min-height: 24px;
    padding: 2px 6px;
    border-radius: 999px;
    color: #ffffff;
    font-size: 0.74rem;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    transition-property: background-color, transform;
    transition-duration: 150ms;
    transition-timing-function: cubic-bezier(0.2, 0, 0, 1);
}

button.search-pip-btn:hover,
togglebutton.search-pip-btn:hover {
    background-color: alpha(#ffffff, 0.18);
}

button.search-pip-btn:active,
togglebutton.search-pip-btn:active {
    transform: scale(0.96);
}

button.search-pip-btn.active,
togglebutton.search-pip-btn:checked,
togglebutton.search-pip-btn.active {
    background-color: alpha(@accent_bg_color, 0.78);
    color: #ffffff;
}
"#;

fn ensure_pip_css() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(display) = Display::default_display() {
            let provider = CssProvider::new();
            provider.load_from_string(PIP_CSS);
            provider.add_to_display(&display, STYLE_PROVIDER_PRIORITY_APPLICATION + 1);
        }
    });
}

fn format_playback_rate(rate: f64) -> &'static str {
    if (rate - 1.25).abs() < 0.05 {
        "1.25x"
    } else if (rate - 1.5).abs() < 0.05 {
        "1.5x"
    } else if (rate - 2.0).abs() < 0.05 {
        "2x"
    } else {
        "1x"
    }
}

fn pip_seek_js(delta_seconds: f64) -> String {
    format!(
        r#"(function() {{
  try {{
    var v = document.querySelector('video[data-search-pip="1"]') || document.querySelector('video');
    if (!v) return;
    var dur = isFinite(v.duration) ? v.duration : Infinity;
    v.currentTime = Math.max(0, Math.min(dur, (v.currentTime || 0) + ({delta_seconds})));
  }} catch (e) {{}}
}})();"#,
    )
}

fn pip_set_muted_js(muted: bool) -> String {
    format!(
        r#"(function() {{
  try {{
    var v = document.querySelector('video[data-search-pip="1"]') || document.querySelector('video');
    if (v) v.muted = {muted};
  }} catch (e) {{}}
}})();"#,
    )
}

fn pip_set_playback_rate_js(rate: f64) -> String {
    format!(
        r#"(function() {{
  try {{
    var v = document.querySelector('video[data-search-pip="1"]') || document.querySelector('video');
    if (v) v.playbackRate = {rate};
  }} catch (e) {{}}
}})();"#,
    )
}

/// Script mínimo inyectado para aislar el `<video>` activo en primer plano (estilo `Float.swift`).
pub const PIP_ENTER_JS: &str = r#"(function() {
  try {
    var videos = Array.prototype.slice.call(document.querySelectorAll('video'));
    if (!videos.length) return;
    var target = document.querySelector('video[data-search-pip="1"]');
    if (!target) {
      var best = null, bestArea = 0;
      for (var i = 0; i < videos.length; i++) {
        var v = videos[i];
        if (!v.paused && !v.ended && v.readyState >= 2) {
          var r = v.getBoundingClientRect();
          var area = r.width * r.height;
          if (area >= bestArea) { bestArea = area; best = v; }
        }
      }
      if (!best) {
        for (var j = 0; j < videos.length; j++) {
          var w = videos[j];
          if (w.readyState >= 1) {
            var rw = w.getBoundingClientRect();
            var aw = rw.width * rw.height;
            if (aw >= bestArea) { bestArea = aw; best = w; }
          }
        }
      }
      target = best || videos[0];
    }
    if (!target) return;
    target.setAttribute('data-search-pip', '1');
    var style = document.getElementById('search-pip-style');
    if (!style) {
      style = document.createElement('style');
      style.id = 'search-pip-style';
      (document.head || document.documentElement).appendChild(style);
    }
    style.textContent = [
      'html, body { overflow: hidden !important; background: #000 !important; }',
      'body *:not(video[data-search-pip="1"]):not(:has(video[data-search-pip="1"])) { visibility: hidden !important; }',
      'video[data-search-pip="1"] {',
      '  visibility: visible !important;',
      '  position: fixed !important;',
      '  inset: 0 !important;',
      '  width: 100vw !important;',
      '  height: 100vh !important;',
      '  max-width: 100vw !important;',
      '  max-height: 100vh !important;',
      '  margin: 0 !important;',
      '  object-fit: contain !important;',
      '  background: #000 !important;',
      '  z-index: 2147483647 !important;',
      '}'
    ].join('\n');
    if (target.paused) {
      var p = target.play();
      if (p && typeof p.catch === 'function') p.catch(function(){});
    }
  } catch (e) {}
})();"#;

/// Alterna reproducción/pausa sobre el vídeo en PiP.
pub const PIP_TOGGLE_PLAY_JS: &str = r#"(function() {
  try {
    var v = document.querySelector('video[data-search-pip="1"]') || document.querySelector('video');
    if (!v) return;
    if (v.paused) {
      var p = v.play();
      if (p && typeof p.catch === 'function') p.catch(function(){});
    } else {
      v.pause();
    }
  } catch (e) {}
})();"#;

/// Pausa el vídeo al cerrar la ventana flotante de PiP.
pub const PIP_PAUSE_JS: &str = r#"(function() {
  try {
    var v = document.querySelector('video[data-search-pip="1"]') || document.querySelector('video');
    if (v && !v.paused) v.pause();
  } catch (e) {}
})();"#;

/// Restaura la página a su estado normal al volver a la pestaña.
pub const PIP_EXIT_JS: &str = r#"(function() {
  try {
    var style = document.getElementById('search-pip-style');
    if (style && style.parentNode) style.parentNode.removeChild(style);
    var marked = document.querySelectorAll('video[data-search-pip="1"]');
    for (var i = 0; i < marked.length; i++) {
      marked[i].removeAttribute('data-search-pip');
    }
  } catch (e) {}
})();"#;

struct PipSession {
    tab_id: u64,
    browser: BrowserView,
    window: Window,
    video_slot: BoxWidget,
    play_btn: Button,
    mute_btn: Button,
    speed_label: Label,
    opacity_btn: Button,
    opacity_label: Label,
    paused: Cell<bool>,
    muted: Cell<bool>,
    playback_rate: Cell<f64>,
    translucent: Cell<bool>,
    on_restore: Box<dyn Fn(u64, BrowserView, bool)>,
    closing: Cell<bool>,
}

/// Controlador de ventana flotante Picture-in-Picture (máximo 1 activa por ventana).
#[derive(Clone)]
pub struct PipController {
    active: Rc<RefCell<Option<Rc<PipSession>>>>,
}

impl PipController {
    pub fn new() -> Self {
        Self {
            active: Rc::new(RefCell::new(None)),
        }
    }

    pub fn active_tab_id(&self) -> Option<u64> {
        self.active.borrow().as_ref().map(|s| s.tab_id)
    }

    pub fn is_active(&self) -> bool {
        self.active.borrow().is_some()
    }

    pub fn playback_rate(&self) -> f64 {
        self.active
            .borrow()
            .as_ref()
            .map_or(1.0, |s| s.playback_rate.get())
    }

    pub fn is_muted(&self) -> bool {
        self.active.borrow().as_ref().is_some_and(|s| s.muted.get())
    }

    pub fn is_translucent(&self) -> bool {
        self.active
            .borrow()
            .as_ref()
            .is_some_and(|s| s.translucent.get())
    }

    /// Abre PiP para `tab_id` moviendo `browser` a la ventana flotante 16:9.
    /// `on_restore(tab_id, browser, focus_tab)` devuelve el `BrowserView` a su pestaña.
    pub fn open(
        &self,
        tab_id: u64,
        browser: BrowserView,
        parent: Option<&ApplicationWindow>,
        on_restore: impl Fn(u64, BrowserView, bool) + 'static,
    ) {
        ensure_pip_css();

        // Si ya había un PiP abierto, lo cerramos devolviéndolo primero a su pestaña.
        if self.active.borrow().is_some() {
            self.close(false, false);
        }

        let win = Window::new();
        win.set_title("Picture-in-Picture");
        win.set_decorated(false);
        win.set_resizable(true);
        win.set_default_size(DEFAULT_PIP_WIDTH, DEFAULT_PIP_HEIGHT);
        win.set_transient_for(parent);
        win.add_css_class("search-pip-window");

        let aspect = AspectFrame::new(0.5, 0.5, 16.0 / 9.0, false);
        let handle = WindowHandle::new();
        let overlay = Overlay::new();

        let video_slot = BoxWidget::new(Orientation::Vertical, 0);
        video_slot.add_css_class("search-pip-video");
        video_slot.set_hexpand(true);
        video_slot.set_vexpand(true);

        browser.widget().unparent();
        video_slot.append(browser.widget());
        browser.execute_javascript(PIP_ENTER_JS);

        // Barra flotante de controles en hover (esquina superior derecha, agrupada con espacio).
        let controls_revealer = Revealer::new();
        controls_revealer.set_transition_type(RevealerTransitionType::Crossfade);
        controls_revealer.set_transition_duration(150);
        controls_revealer.set_reveal_child(false);
        controls_revealer.set_halign(Align::End);
        controls_revealer.set_valign(Align::Start);

        let controls = BoxWidget::new(Orientation::Horizontal, 10);
        controls.add_css_class("search-pip-controls");

        // Grupo 1: reproducción, salto y audio.
        let media_group = BoxWidget::new(Orientation::Horizontal, 4);

        let seek_back_btn = Button::new();
        seek_back_btn.add_css_class("flat");
        seek_back_btn.add_css_class("search-pip-btn");
        seek_back_btn.set_tooltip_text(Some("Retroceder 10 s"));
        let seek_back_box = BoxWidget::new(Orientation::Horizontal, 3);
        seek_back_box.append(&icons::image_for_theme(IconId::Undo, 12, true));
        seek_back_box.append(&Label::new(Some("-10s")));
        seek_back_btn.set_child(Some(&seek_back_box));

        let play_btn = Button::new();
        play_btn.add_css_class("circular");
        play_btn.add_css_class("search-pip-btn");
        play_btn.set_tooltip_text(Some("Reproducir / Pausar"));
        play_btn.set_child(Some(&icons::image_for_theme(IconId::MediaPause, 14, true)));

        let seek_fwd_btn = Button::new();
        seek_fwd_btn.add_css_class("flat");
        seek_fwd_btn.add_css_class("search-pip-btn");
        seek_fwd_btn.set_tooltip_text(Some("Avanzar 10 s"));
        let seek_fwd_box = BoxWidget::new(Orientation::Horizontal, 3);
        seek_fwd_box.append(&Label::new(Some("+10s")));
        seek_fwd_box.append(&icons::image_for_theme(IconId::ChevronRight, 12, true));
        seek_fwd_btn.set_child(Some(&seek_fwd_box));

        let speed_btn = Button::new();
        speed_btn.add_css_class("flat");
        speed_btn.add_css_class("search-pip-btn");
        speed_btn.set_tooltip_text(Some("Velocidad de reproducción (1x / 1.25x / 1.5x / 2x)"));
        let speed_label = Label::new(Some("1x"));
        speed_btn.set_child(Some(&speed_label));

        let mute_btn = Button::new();
        mute_btn.add_css_class("circular");
        mute_btn.add_css_class("search-pip-btn");
        mute_btn.set_tooltip_text(Some("Silenciar / Activar audio"));
        mute_btn.set_child(Some(&icons::image_for_theme(IconId::VolumeHigh, 14, true)));

        media_group.append(&seek_back_btn);
        media_group.append(&play_btn);
        media_group.append(&seek_fwd_btn);
        media_group.append(&speed_btn);
        media_group.append(&mute_btn);

        // Grupo 2: ventana PiP (opacidad, volver a pestaña, cerrar).
        let window_group = BoxWidget::new(Orientation::Horizontal, 4);

        let opacity_btn = Button::new();
        opacity_btn.add_css_class("flat");
        opacity_btn.add_css_class("search-pip-btn");
        opacity_btn.set_tooltip_text(Some("Opacidad de ventana (100% / 85%)"));
        let opacity_box = BoxWidget::new(Orientation::Horizontal, 3);
        let opacity_label = Label::new(Some("100%"));
        opacity_box.append(&icons::image_for_theme(IconId::Palette, 12, true));
        opacity_box.append(&opacity_label);
        opacity_btn.set_child(Some(&opacity_box));

        let return_btn = Button::new();
        return_btn.add_css_class("circular");
        return_btn.add_css_class("search-pip-btn");
        return_btn.set_tooltip_text(Some("Volver a la pestaña"));
        return_btn.set_child(Some(&icons::image_for_theme(IconId::PipReturn, 14, true)));

        let close_btn = Button::new();
        close_btn.add_css_class("circular");
        close_btn.add_css_class("search-pip-btn");
        close_btn.set_tooltip_text(Some("Cerrar Picture-in-Picture"));
        close_btn.set_child(Some(&icons::image_for_theme(IconId::Close, 14, true)));

        window_group.append(&opacity_btn);
        window_group.append(&return_btn);
        window_group.append(&close_btn);

        controls.append(&media_group);
        controls.append(&window_group);
        controls_revealer.set_child(Some(&controls));

        overlay.set_child(Some(&video_slot));
        overlay.add_overlay(&controls_revealer);
        overlay.set_measure_overlay(&controls_revealer, false);

        handle.set_child(Some(&overlay));
        aspect.set_child(Some(&handle));
        win.set_child(Some(&aspect));

        let session = Rc::new(PipSession {
            tab_id,
            browser,
            window: win.clone(),
            video_slot,
            play_btn: play_btn.clone(),
            mute_btn: mute_btn.clone(),
            speed_label,
            opacity_btn: opacity_btn.clone(),
            opacity_label,
            paused: Cell::new(false),
            muted: Cell::new(false),
            playback_rate: Cell::new(1.0),
            translucent: Cell::new(false),
            on_restore: Box::new(on_restore),
            closing: Cell::new(false),
        });

        *self.active.borrow_mut() = Some(session.clone());

        // Mostrar controles solo cuando el puntero está sobre la ventana PiP.
        {
            let motion = EventControllerMotion::new();
            let rev_enter = controls_revealer.clone();
            motion.connect_enter(move |_, _| {
                rev_enter.set_reveal_child(true);
            });
            let rev_leave = controls_revealer;
            motion.connect_leave(move || {
                rev_leave.set_reveal_child(false);
            });
            overlay.add_controller(&motion);
        }

        // Conectar botones de salto -10s / +10s.
        {
            let ctrl = self.clone();
            seek_back_btn.connect_clicked(move || {
                ctrl.seek_by(-10.0);
            });
        }
        {
            let ctrl = self.clone();
            seek_fwd_btn.connect_clicked(move || {
                ctrl.seek_by(10.0);
            });
        }

        // Conectar botón Play/Pause.
        {
            let ctrl = self.clone();
            play_btn.connect_clicked(move || {
                ctrl.toggle_play_pause();
            });
        }

        // Conectar selector de velocidad (1x -> 1.25x -> 1.5x -> 2x).
        {
            let ctrl = self.clone();
            speed_btn.connect_clicked(move || {
                ctrl.cycle_playback_rate();
            });
        }

        // Conectar botón de silencio.
        {
            let ctrl = self.clone();
            mute_btn.connect_clicked(move || {
                ctrl.toggle_mute();
            });
        }

        // Conectar botón de opacidad (100% / 85%).
        {
            let ctrl = self.clone();
            opacity_btn.connect_clicked(move || {
                ctrl.toggle_opacity();
            });
        }

        // Conectar botón Volver a la pestaña.
        {
            let ctrl = self.clone();
            return_btn.connect_clicked(move || {
                ctrl.close(true, false);
            });
        }

        // Conectar botón Cerrar (pausa el vídeo y vuelve a la pestaña sin robar foco).
        {
            let ctrl = self.clone();
            close_btn.connect_clicked(move || {
                ctrl.close(false, true);
            });
        }

        // Cierre desde el gestor de ventanas (Alt+F4 / Esc / etc.).
        {
            let ctrl = self.clone();
            win.connect_close_request(move || {
                ctrl.close(false, true);
                false
            });
        }

        win.present();
    }

    pub fn toggle_play_pause(&self) {
        if let Some(session) = self.active.borrow().as_ref() {
            session.browser.execute_javascript(PIP_TOGGLE_PLAY_JS);
            let now_paused = !session.paused.get();
            session.paused.set(now_paused);
            let icon = if now_paused {
                IconId::MediaPlay
            } else {
                IconId::MediaPause
            };
            session
                .play_btn
                .set_child(Some(&icons::image_for_theme(icon, 14, true)));
        }
    }

    /// Salta `delta_seconds` segundos en el vídeo activo (p. ej. `-10.0` o `+10.0`).
    pub fn seek_by(&self, delta_seconds: f64) {
        if let Some(session) = self.active.borrow().as_ref() {
            session
                .browser
                .execute_javascript(&pip_seek_js(delta_seconds));
        }
    }

    /// Establece la velocidad de reproducción (`1x`, `1.25x`, `1.5x`, `2x`).
    pub fn set_playback_rate(&self, rate: f64) {
        if let Some(session) = self.active.borrow().as_ref() {
            let clamped = rate.clamp(0.5, 4.0);
            session.playback_rate.set(clamped);
            session.speed_label.set_text(format_playback_rate(clamped));
            session
                .browser
                .execute_javascript(&pip_set_playback_rate_js(clamped));
        }
    }

    /// Cicla la velocidad entre `1x`, `1.25x`, `1.5x` y `2x`.
    pub fn cycle_playback_rate(&self) -> f64 {
        let current = self.playback_rate();
        let idx = PIP_PLAYBACK_RATES
            .iter()
            .position(|&r| (r - current).abs() < 0.05)
            .unwrap_or(0);
        let next = PIP_PLAYBACK_RATES[(idx + 1) % PIP_PLAYBACK_RATES.len()];
        self.set_playback_rate(next);
        next
    }

    /// Activa o desactiva el silencio del vídeo en PiP.
    pub fn set_muted(&self, muted: bool) {
        if let Some(session) = self.active.borrow().as_ref() {
            session.muted.set(muted);
            let icon = if muted {
                IconId::VolumeMuted
            } else {
                IconId::VolumeHigh
            };
            session
                .mute_btn
                .set_child(Some(&icons::image_for_theme(icon, 14, true)));
            if muted {
                session.mute_btn.add_css_class("active");
            } else {
                session.mute_btn.remove_css_class("active");
            }
            session.browser.execute_javascript(&pip_set_muted_js(muted));
        }
    }

    pub fn toggle_mute(&self) -> bool {
        let next = !self.is_muted();
        self.set_muted(next);
        next
    }

    /// Alterna la opacidad de la ventana PiP entre `100%` (`false`) y `85%` (`true`).
    pub fn set_translucent(&self, translucent: bool) {
        if let Some(session) = self.active.borrow().as_ref() {
            session.translucent.set(translucent);
            if translucent {
                session.window.add_css_class("pip-translucent");
                session.opacity_btn.add_css_class("active");
                session.opacity_label.set_text("85%");
            } else {
                session.window.remove_css_class("pip-translucent");
                session.opacity_btn.remove_css_class("active");
                session.opacity_label.set_text("100%");
            }
        }
    }

    pub fn toggle_opacity(&self) -> bool {
        let next = !self.is_translucent();
        self.set_translucent(next);
        next
    }

    /// Cierra la ventana flotante de PiP y devuelve el `BrowserView` a su pestaña.
    pub fn close(&self, focus_tab: bool, pause_video: bool) {
        let Some(session) = self.active.borrow_mut().take() else {
            return;
        };
        if session.closing.replace(true) {
            return;
        }
        session.browser.execute_javascript(PIP_EXIT_JS);
        if pause_video {
            session.browser.execute_javascript(PIP_PAUSE_JS);
        }
        session.browser.widget().unparent();
        session.video_slot.unparent();
        (session.on_restore)(session.tab_id, session.browser.clone(), focus_tab);
        session.window.destroy();
    }
}

impl Default for PipController {
    fn default() -> Self {
        Self::new()
    }
}
