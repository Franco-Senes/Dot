// Vault de Search (Fase 5): detección mínima de formularios de inicio de sesión dentro de la
// página. Port reducido de `FormRelay.script` (`upstream/Sources/Search/Forms.swift`).
//
// Reglas (ver PORTING.md, «Seguridad de Vault»):
// - Solo detecta y avisa. Toda decisión (qué origin, qué ofrecer, qué guardar) vive en Rust.
// - El origin NUNCA sale de aquí: Rust lo toma de la URL del marco de CEF asociada al token.
// - Nunca escribe la contraseña en la consola salvo en el mensaje IPC autenticado, que Rust
//   intercepta en `DisplayHandler::on_console_message` y suprime del log.
// - Rust la inyecta solo en el marco principal y en iframes del mismo origin.
//
// `__SEARCH_VAULT_TOKEN__` lo sustituye Rust por un token aleatorio de 128 bits por marco.
(function () {
  'use strict';
  if (window.__searchVault) return;

  var PREFIX = '__search_ipc___SEARCH_VAULT_TOKEN__:';
  // Referencias tomadas al instalar, antes de que la página pueda cambiarlas después.
  var debug = console.debug.bind(console);
  var stringify = JSON.stringify;
  var now = Date.now;

  function post(obj) {
    try { debug(PREFIX + stringify(obj)); } catch (e) {}
  }

  function visible(el) {
    try {
      var r = el.getBoundingClientRect();
      return r.width > 0 && r.height > 0;
    } catch (e) { return false; }
  }

  function tokens(el) {
    return (el.getAttribute('autocomplete') || '').toLowerCase().split(/\s+/).filter(Boolean);
  }

  function newPassword(el) { return tokens(el).indexOf('new-password') >= 0; }

  function texty(el) {
    if (!el || (el.tagName || '').toLowerCase() !== 'input' || el.disabled || el.readOnly) return false;
    var kind = (el.type || 'text').toLowerCase();
    return ['text', 'email', 'tel', 'number'].indexOf(kind) >= 0 && visible(el);
  }

  // Campos que nunca son un usuario: búsqueda, códigos, mensajes, títulos…
  function aside(el) {
    var words = [el.name, el.id, el.getAttribute('aria-label'), el.getAttribute('placeholder')]
      .join(' ').toLowerCase();
    if (/(^|\s)(q|to|cc|bcc|subject)(\s|$)/.test(((el.name || '') + ' ' + (el.id || '')).toLowerCase())) return true;
    if (/query|search|buscar|otp|one-time|captcha|coupon|promo|message|comment/.test(words)) return true;
    return !!(el.closest && el.closest('[role="search"], form[role="search"]'));
  }

  // La caja de contraseña visible y el último campo de texto antes de ella (el usuario).
  function pair() {
    var boxes = document.querySelectorAll('input[type="password"]');
    var pass = null;
    for (var p = 0; p < boxes.length; p++) {
      if (visible(boxes[p])) { pass = boxes[p]; break; }
    }
    if (!pass) return null;
    var scope = pass.form || (pass.closest && pass.closest('form')) || document;
    var all = scope.querySelectorAll('input');
    var user = null;
    for (var i = 0; i < all.length; i++) {
      if (all[i] === pass) break;
      if (texty(all[i]) && !aside(all[i])) user = all[i];
    }
    return { user: user, pass: pass };
  }

  // ¿Es un campo donde ofrecer cuentas guardadas? La contraseña (si no es «nueva») o el usuario.
  function loginField(el) {
    if (!el || (el.tagName || '').toLowerCase() !== 'input') return false;
    var both = pair();
    if (!both || newPassword(both.pass)) return false;
    return el === both.pass || (!!both.user && el === both.user);
  }

  // Rectángulo del campo en píxeles CSS del marco principal (sumando iframes del mismo origin).
  function rectOf(el) {
    try {
      var r = el.getBoundingClientRect();
      var x = r.left, y = r.top;
      var w = window;
      while (w !== w.top) {
        var fe = w.frameElement; // null si el padre es de otro origin
        if (!fe) return null;
        var fr = fe.getBoundingClientRect();
        x += fr.left + (fe.clientLeft || 0);
        y += fr.top + (fe.clientTop || 0);
        w = w.parent;
      }
      return { x: x, y: y, w: r.width, h: r.height };
    } catch (e) { return null; }
  }

  // Valor de un campo a través del setter nativo + eventos, para que los frameworks lo vean.
  function put(box, value) {
    if (!box) return;
    var desc = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value');
    if (desc && desc.set) { desc.set.call(box, value); } else { box.value = value; }
    box.dispatchEvent(new Event('input', { bubbles: true }));
    box.dispatchEvent(new Event('change', { bubbles: true }));
  }

  // ---- Captura de envíos (formularios clásicos y SPA) ----------------------------------------
  var lastSent = '';
  var lastAt = 0;
  var captured = false;
  var interactedAt = 0;

  function offer() {
    var both = pair();
    if (!both || !both.pass.value) return;
    var user = both.user ? both.user.value : '';
    var key = user + '\u0000' + both.pass.value;
    var t = now();
    if (key === lastSent && t - lastAt < 3000) return;
    lastSent = key;
    lastAt = t;
    captured = true;
    post({ kind: 'vault-capture', username: user, password: both.pass.value });
  }

  document.addEventListener('submit', offer, true);
  document.addEventListener('keydown', function (e) {
    if (e.key !== 'Enter') return;
    var both = pair();
    if (both && (document.activeElement === both.pass || document.activeElement === both.user)) offer();
  }, true);
  // Muchos botones de inicio de sesión no están en un <form> ni disparan `submit`.
  document.addEventListener('click', function (e) {
    var el = e.target;
    if (!el || !el.closest) return;
    if (el.closest('button, input[type="submit"], input[type="image"], [role="button"]')) {
      interactedAt = now();
      setTimeout(offer, 0);
    }
  }, true);
  document.addEventListener('pointerdown', function () { interactedAt = now(); }, true);

  // SPA que envían con fetch/XHR tras un clic en algo que no parece un botón.
  function afterInteraction() {
    if (now() - interactedAt < 3000) { try { offer(); } catch (e) {} }
  }
  try {
    var realFetch = window.fetch;
    if (typeof realFetch === 'function') {
      window.fetch = function () {
        afterInteraction();
        return realFetch.apply(this, arguments);
      };
    }
  } catch (e) {}
  try {
    var realSend = XMLHttpRequest.prototype.send;
    XMLHttpRequest.prototype.send = function () {
      afterInteraction();
      return realSend.apply(this, arguments);
    };
  } catch (e) {}

  // ---- Presencia del formulario y foco -------------------------------------------------------
  var told = false;
  var hung = null;

  function report(focused) {
    var el = document.activeElement;
    var on = focused && loginField(el);
    var rect = on ? rectOf(el) : null;
    post({ kind: 'vault-forms', focused: !!(on && rect), rect: rect });
    hung = on && rect ? el : null;
  }

  function tell() {
    var both = pair();
    if (told || !both || newPassword(both.pass)) return;
    told = true;
    report(true);
  }

  if (document.readyState === 'complete') { tell(); } else { window.addEventListener('load', tell); }
  setTimeout(tell, 700);
  setTimeout(tell, 2200);

  // El formulario desaparece sin navegar tras un envío: el inicio de sesión se hizo en la página.
  var looking = null, settling = null;
  function look() {
    looking = null;
    if (!told) { tell(); return; }
    if (pair()) return;
    told = false;
    if (!captured) return;
    clearTimeout(settling);
    settling = setTimeout(function () {
      if (pair()) return;
      captured = false;
      post({ kind: 'vault-settled' });
    }, 400);
  }
  try {
    new MutationObserver(function () {
      if (!looking) looking = setTimeout(look, 250);
    }).observe(document.documentElement, { childList: true, subtree: true });
  } catch (e) {}

  document.addEventListener('focusin', function () {
    if (loginField(document.activeElement)) { report(true); }
    else if (hung) { hung = null; post({ kind: 'vault-blur' }); }
  }, true);
  document.addEventListener('focusout', function () {
    setTimeout(function () {
      // La ventana perdió el foco (p. ej. un clic en el propio popover): no cerrar.
      if (!document.hasFocus()) return;
      if (hung && document.activeElement !== hung) { hung = null; post({ kind: 'vault-blur' }); }
    }, 0);
  }, true);

  var moving = false;
  function moved() {
    if (!hung || moving) return;
    moving = true;
    requestAnimationFrame(function () { moving = false; if (hung) report(true); });
  }
  window.addEventListener('scroll', moved, true);
  window.addEventListener('resize', moved);

  // ---- API que Rust invoca con `Frame::execute_java_script` ----------------------------------
  var api = {
    fill: function (user, password) {
      var both = pair();
      if (!both || newPassword(both.pass)) return false;
      if (both.user && user && !both.user.value) put(both.user, user);
      put(both.pass, password);
      // No ofrecer guardar lo que acabamos de rellenar si no cambia.
      lastSent = (both.user ? both.user.value : '') + '\u0000' + password;
      lastAt = now();
      return true;
    }
  };
  try {
    Object.defineProperty(window, '__searchVault', { value: Object.freeze(api), configurable: false, enumerable: false, writable: false });
  } catch (e) {}
})();
