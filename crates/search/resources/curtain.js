(function () {
  'use strict';
  var IPC_PREFIX = __SEARCH_IPC_PREFIX__;
  var dbg = console.debug.bind(console);
  if (window.__searchCurtain && window.__searchCurtain.on) {
    window.__searchCurtain.on();
    return;
  }

  var frame = null, tag = null, target = null, live = false;
  var HOOKS = ['data-testid', 'data-test', 'data-qa', 'data-cy', 'aria-label', 'name', 'role'];
  var KNOWN = {
    nav: 'Navegación', header: 'Cabecera', footer: 'Pie de página', aside: 'Barra lateral',
    form: 'Formulario', dialog: 'Diálogo', video: 'Vídeo', img: 'Imagen',
    button: 'Botón', iframe: 'Contenido incrustado', figure: 'Figura', table: 'Tabla'
  };

  function post(payload) {
    try {
      dbg(IPC_PREFIX + JSON.stringify(payload));
    } catch (e) {}
  }

  function chrome() {
    if (frame) return frame;
    frame = document.createElement('div');
    frame.id = 'office-curtain-frame';
    frame.style.cssText = 'position:fixed;z-index:2147483646;pointer-events:none;' +
      'border:2px solid rgba(23,23,23,.9);background:rgba(23,23,23,.08);' +
      'border-radius:4px;transition:all .06s ease-out;display:none';
    tag = document.createElement('div');
    tag.style.cssText = 'position:absolute;font:500 11px system-ui,-apple-system,sans-serif;' +
      'color:#fff;background:#171717;padding:2px 7px;border-radius:5px;' +
      'white-space:nowrap;overflow:hidden;text-overflow:ellipsis';
    frame.appendChild(tag);
    (document.documentElement || document.body).appendChild(frame);
    return frame;
  }

  function clip(text, n) {
    return text.length > n ? text.slice(0, n) + '…' : text;
  }

  function labelFor(el) {
    var said = el.getAttribute && (el.getAttribute('aria-label') || el.getAttribute('title'));
    if (said && said.trim()) return clip(said.trim(), 40);
    var t = el.tagName.toLowerCase();
    if (KNOWN[t]) return KNOWN[t];
    var role = el.getAttribute && el.getAttribute('role');
    if (role) return role.charAt(0).toUpperCase() + role.slice(1);
    var text = (el.innerText || '').trim().replace(/\s+/g, ' ');
    return text ? clip(text, 40) : t;
  }

  function noteFor(el) {
    var r = el.getBoundingClientRect();
    var cx = r.left + r.width / 2, cy = r.top + r.height / 2;
    var side = cx < window.innerWidth / 3 ? 'izq.'
             : (cx > window.innerWidth * 2 / 3 ? 'der.' : 'centro');
    var band = cy < window.innerHeight / 3 ? 'arriba'
             : (cy > window.innerHeight * 2 / 3 ? 'abajo' : 'medio');
    return Math.round(r.width) + '×' + Math.round(r.height) + ' · ' + band + ' ' + side;
  }

  function place(el) {
    var box = chrome(), r = el.getBoundingClientRect();
    box.style.display = 'block';
    box.style.left = r.left + 'px';
    box.style.top = r.top + 'px';
    box.style.width = r.width + 'px';
    box.style.height = r.height + 'px';
    tag.textContent = labelFor(el);
    tag.style.top = r.top >= 26 ? '-21px' : '3px';
    tag.style.left = Math.max(2, -r.left + 4) + 'px';
    tag.style.maxWidth = Math.max(80, window.innerWidth - Math.max(0, r.left) - 16) + 'px';
  }

  function isUnique(sel) {
    try { return document.querySelectorAll(sel).length === 1; } catch (e) { return false; }
  }

  function escIdent(s) {
    return (typeof CSS !== 'undefined' && CSS.escape) ? CSS.escape(s) : s.replace(/[^\w-]/g, '\\$&');
  }

  function snapshotNode(node) {
    var tagName = node.tagName.toLowerCase();
    var id = node.id ? String(node.id) : null;
    var idUnique = !!(id && isUnique('#' + escIdent(id)));

    var rawClasses = (node.className && typeof node.className === 'string')
      ? node.className.trim().split(/\s+/).filter(Boolean).slice(0, 12)
      : [];
    var uniqueClasses = [];
    for (var c = 0; c < rawClasses.length; c++) {
      if (isUnique(tagName + '.' + escIdent(rawClasses[c]))) {
        uniqueClasses.push(rawClasses[c]);
      }
    }
    var comboUnique = false;
    if (rawClasses.length > 0 && rawClasses.length <= 6) {
      var comboSel = tagName + '.' + rawClasses.map(escIdent).join('.');
      comboUnique = isUnique(comboSel);
    }

    var attrs = [];
    for (var i = 0; i < HOOKS.length; i++) {
      var v = node.getAttribute && node.getAttribute(HOOKS[i]);
      if (v && v.trim()) {
        var trimmed = v.trim();
        var attrSel = tagName + '[' + HOOKS[i] + '="' + trimmed.replace(/\\/g, '\\\\').replace(/"/g, '\\"') + '"]';
        attrs.push({ name: HOOKS[i], value: trimmed, unique_with_tag: isUnique(attrSel) });
      }
    }

    var nth = 1, siblings = 1;
    if (node.parentElement) {
      var kids = node.parentElement.children;
      var same = 0, pos = 0;
      for (var k = 0; k < kids.length; k++) {
        if (kids[k].tagName === node.tagName) {
          same++;
          if (kids[k] === node) pos = same;
        }
      }
      siblings = same || 1;
      nth = pos || 1;
    }

    return {
      tag: tagName,
      id: id,
      id_unique: idUnique,
      classes: rawClasses,
      unique_classes: uniqueClasses,
      classes_combo_unique: comboUnique,
      attrs: attrs,
      nth_of_type: nth,
      same_tag_siblings: siblings
    };
  }

  function collectAncestry(el) {
    var chain = [];
    var cur = el;
    var depth = 0;
    while (cur && cur.nodeType === 1 && cur !== document.documentElement && depth < 12) {
      chain.push(snapshotNode(cur));
      if (cur === document.body) break;
      cur = cur.parentElement;
      depth++;
    }
    return {
      ancestry: chain,
      label: labelFor(el),
      note: noteFor(el)
    };
  }

  function onMove(e) {
    if (!live) return;
    var el = document.elementFromPoint(e.clientX, e.clientY);
    if (!el || el === frame || el === tag || el === document.documentElement || el === document.body) return;
    target = el;
    place(el);
  }

  function swallow(e) {
    if (!live) return;
    e.preventDefault();
    e.stopPropagation();
    e.stopImmediatePropagation();
  }

  function onPress(e) {
    if (!live) return;
    swallow(e);
    var el = target || document.elementFromPoint(e.clientX, e.clientY);
    if (!el || el === frame || el === tag || el === document.documentElement || el === document.body) return;
    try {
      post({ kind: 'curtain_pick', snapshot: collectAncestry(el) });
    } catch (err) {
      post({ kind: 'curtain_error', message: String(err) });
    }
    target = null;
    if (frame) frame.style.display = 'none';
  }

  function onKeyDown(e) {
    if (!live) return;
    if (e.key === 'Escape') {
      swallow(e);
      stop(true);
    }
  }

  var PRESSES = ['pointerdown', 'mousedown', 'pointerup', 'mouseup', 'click', 'dblclick', 'contextmenu', 'touchstart'];

  function start() {
    if (live) return;
    live = true;
    chrome();
    document.documentElement.style.cursor = 'crosshair';
    document.addEventListener('mousemove', onMove, true);
    document.addEventListener('pointermove', onMove, true);
    document.addEventListener('keydown', onKeyDown, true);
    for (var i = 0; i < PRESSES.length; i++) {
      document.addEventListener(PRESSES[i], PRESSES[i] === 'pointerdown' ? onPress : swallow, true);
    }
  }

  function stop(notify) {
    if (!live) return;
    live = false;
    target = null;
    if (frame) frame.style.display = 'none';
    document.documentElement.style.cursor = '';
    document.removeEventListener('mousemove', onMove, true);
    document.removeEventListener('pointermove', onMove, true);
    document.removeEventListener('keydown', onKeyDown, true);
    for (var i = 0; i < PRESSES.length; i++) {
      document.removeEventListener(PRESSES[i], PRESSES[i] === 'pointerdown' ? onPress : swallow, true);
    }
    if (notify) post({ kind: 'curtain_off' });
  }

  window.__searchCurtain = {
    on: start,
    off: function () { stop(false); }
  };

  start();
})();
