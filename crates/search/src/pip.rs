//! Picture-in-Picture flotante 16:9 sin decoraciones (`Float.swift`).
//!
//! Mueve el `BrowserView` de la pestaña a una `GtkWindow` flotante sin bordes
//! con `GtkAspectFrame` (16:9), `GtkWindowHandle` (arrastrable desde cualquier punto)
//! y controles superpuestos de reproducir/pausa, volver a la pestaña y cerrar.
//!
//! Limitación de Wayland: los clientes estándar de Wayland no pueden forzar
//! *always-on-top* ni posicionamiento global absoluto; el usuario puede fijar
//! "Siempre encima" desde el gestor de ventanas si su compositor lo ofrece.

use crate::browser_view::BrowserView;
use crate::icons::{self, IconId};
use gnome_platform::adw::ApplicationWindow;
use gnome_platform::gtk::{
    Align, AsWidget, AspectFrame, BoxWidget, Button, Orientation, Overlay, Window, WindowHandle,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const DEFAULT_PIP_WIDTH: i32 = 440;
const DEFAULT_PIP_HEIGHT: i32 = 248; // ~16:9

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
    paused: Cell<bool>,
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

    /// Abre PiP para `tab_id` moviendo `browser` a la ventana flotante 16:9.
    /// `on_restore(tab_id, browser, focus_tab)` devuelve el `BrowserView` a su pestaña.
    pub fn open(
        &self,
        tab_id: u64,
        browser: BrowserView,
        parent: Option<&ApplicationWindow>,
        on_restore: impl Fn(u64, BrowserView, bool) + 'static,
    ) {
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

        // Barra discreta de controles flotantes en la esquina superior derecha.
        let controls = BoxWidget::new(Orientation::Horizontal, 4);
        controls.add_css_class("search-pip-controls");
        controls.set_halign(Align::End);
        controls.set_valign(Align::Start);

        let play_btn = Button::new();
        play_btn.add_css_class("circular");
        play_btn.add_css_class("search-pip-btn");
        play_btn.set_tooltip_text(Some("Reproducir / Pausar"));
        play_btn.set_child(Some(&icons::image_for_theme(IconId::MediaPause, 14, true)));

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

        controls.append(&play_btn);
        controls.append(&return_btn);
        controls.append(&close_btn);

        overlay.set_child(Some(&video_slot));
        overlay.add_overlay(&controls);
        overlay.set_measure_overlay(&controls, false);

        handle.set_child(Some(&overlay));
        aspect.set_child(Some(&handle));
        win.set_child(Some(&aspect));

        let session = Rc::new(PipSession {
            tab_id,
            browser,
            window: win.clone(),
            video_slot,
            play_btn: play_btn.clone(),
            paused: Cell::new(false),
            on_restore: Box::new(on_restore),
            closing: Cell::new(false),
        });

        *self.active.borrow_mut() = Some(session.clone());

        // Conectar botón Play/Pause.
        {
            let ctrl = self.clone();
            play_btn.connect_clicked(move || {
                ctrl.toggle_play_pause();
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
