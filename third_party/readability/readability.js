/*
 * Extractor de artículos original para el modo lectura de Search Linux (licencia MIT del proyecto).
 * Inspirado en la heurística de densidad de prosa de `upstream/Sources/Search/Reader.swift` (MIT)
 * y Mozilla Readability (sin código de terceros vendoreado; ver `third_party/readability/README.md`).
 *
 * No muta el DOM de la página original: clona únicamente el candidato seleccionado, resuelve
 * URLs absolutas e imágenes diferidas y envía el resultado a Rust por el puente IPC autenticado.
 */
(function () {
  'use strict';
  var IPC_PREFIX = __SEARCH_IPC_PREFIX__;
  var dbg = console.debug.bind(console);

  function send(payload) {
    try {
      dbg(IPC_PREFIX + JSON.stringify(payload));
    } catch (e) {}
  }

  if (document.getElementById('office-reader')) {
    send({ kind: 'reader_article', ok: false, reason: 'already_reader' });
    return;
  }

  function proseScore(el) {
    var paragraphs = el.querySelectorAll('p');
    if (paragraphs.length < 2) return 0;
    var letters = 0;
    for (var i = 0; i < paragraphs.length; i++) {
      var t = (paragraphs[i].innerText || paragraphs[i].textContent || '').trim();
      if (t.length >= 25) letters += t.length;
    }
    if (letters < 300) return 0;

    var whole = (el.textContent || '').trim().length;
    if (!whole) return 0;
    var linkChars = 0;
    var links = el.querySelectorAll('a');
    for (var j = 0; j < links.length; j++) {
      linkChars += (links[j].textContent || '').length;
    }
    var linkDensity = Math.min(linkChars / whole, 1);
    if (linkDensity > 0.55) return 0;

    var bonus = 1.0;
    var tag = el.tagName.toLowerCase();
    if (tag === 'article') bonus = 1.25;
    else if (tag === 'main' || el.getAttribute('role') === 'main') bonus = 1.15;

    return letters * (1 - linkDensity) * bonus;
  }

  function findBestArticle() {
    var candidates = document.querySelectorAll(
      'article, main, [role="main"], .post, .entry, .article, .content, #content, section, div'
    );
    var best = null, bestScore = 0;
    for (var i = 0; i < candidates.length; i++) {
      var el = candidates[i];
      if (el.closest('nav, footer, header, aside, [role="navigation"], [role="banner"]')) continue;
      var score = proseScore(el);
      if (!score) continue;
      if (!best) {
        best = el;
        bestScore = score;
        continue;
      }
      if (best.contains(el)) {
        if (score * 1.25 > bestScore) {
          best = el;
          bestScore = score;
        }
      } else if (el.contains(best)) {
        if (score > bestScore * 1.25) {
          best = el;
          bestScore = score;
        }
      } else if (score > bestScore) {
        best = el;
        bestScore = score;
      }
    }
    return best;
  }

  function absolutizeUrl(raw) {
    if (!raw) return '';
    try {
      return new URL(raw, location.href).href;
    } catch (e) {
      return raw;
    }
  }

  try {
    var article = findBestArticle();
    if (!article) {
      send({ kind: 'reader_article', ok: false, reason: 'not_article' });
      return;
    }

    // Buscar imagen principal justo antes del cuerpo del artículo si existe.
    var lead = null;
    for (var up = article.parentElement, depth = 0;
         up && up !== document.body && depth < 4 && !lead;
         up = up.parentElement, depth++) {
      var figs = up.querySelectorAll('figure, picture');
      for (var l = 0; l < figs.length && !lead; l++) {
        var img = figs[l].querySelector('img');
        if (img &&
            article.compareDocumentPosition(figs[l]) === Node.DOCUMENT_POSITION_PRECEDING &&
            !figs[l].closest('nav, aside, footer, [role="banner"]') &&
            img.getBoundingClientRect().width >= 360) {
          lead = figs[l];
        }
      }
    }

    // Clonar sin tocar el DOM original.
    var clone = article.cloneNode(true);
    if (lead) {
      var leadClone = lead.cloneNode(true);
      clone.insertBefore(leadClone, clone.firstChild);
    }

    // Resolver imágenes (currentSrc / data-src) a URLs absolutas.
    var lazyAttrs = ['data-src', 'data-original', 'data-lazy-src', 'data-lazy',
                     'data-full-src', 'data-hi-res-src', 'data-image', 'data-echo'];
    var origImgs = Array.prototype.slice.call(article.querySelectorAll('img'));
    var cloneImgs = Array.prototype.slice.call(clone.querySelectorAll('img'));
    for (var m = 0; m < cloneImgs.length; m++) {
      var cImg = cloneImgs[m];
      var src = cImg.getAttribute('src') || '';
      if ((!src || src.indexOf('data:image') === 0) && m < origImgs.length && origImgs[m].currentSrc) {
        src = origImgs[m].currentSrc;
      }
      if (!src || src.indexOf('data:image') === 0) {
        for (var k = 0; k < lazyAttrs.length; k++) {
          var cand = cImg.getAttribute(lazyAttrs[k]);
          if (cand) { src = cand; break; }
        }
      }
      if (!src || src.indexOf('data:image') === 0) {
        cImg.remove();
      } else {
        cImg.setAttribute('src', absolutizeUrl(src));
        cImg.removeAttribute('srcset');
      }
    }

    // Absolutizar enlaces.
    var cloneLinks = clone.querySelectorAll('a[href]');
    for (var a = 0; a < cloneLinks.length; a++) {
      var href = cloneLinks[a].getAttribute('href');
      if (href && href.charAt(0) !== '#') {
        cloneLinks[a].setAttribute('href', absolutizeUrl(href));
      }
    }

    // Eliminar ruido estructural.
    var clutter = clone.querySelectorAll(
      'script,style,noscript,form,nav,aside,footer,button,input,select,textarea,' +
      '[role="complementary"],[role="navigation"],[role="banner"],[aria-hidden="true"]'
    );
    for (var c = 0; c < clutter.length; c++) clutter[c].remove();

    var newsletters = clone.querySelectorAll('[class*="newsletter" i]');
    for (var n = 0; n < newsletters.length; n++) {
      if (newsletters[n].querySelectorAll('p').length < 3) newsletters[n].remove();
    }

    var heading = document.querySelector('h1');
    var title = (heading && heading.innerText && heading.innerText.trim()) || document.title || '';

    // Evitar título duplicado dentro del cuerpo.
    var innerH1 = clone.querySelector('h1');
    if (innerH1 && innerH1.textContent.replace(/\s+/g, ' ').trim().toLowerCase() ===
        title.replace(/\s+/g, ' ').trim().toLowerCase()) {
      innerH1.remove();
    }

    // Buscar firma/autor.
    var byline = null;
    var bylineEl = document.querySelector('[rel="author"], [class*="byline" i], [itemprop="author"]');
    if (bylineEl) {
      var bt = (bylineEl.innerText || bylineEl.textContent || '').trim().replace(/\s+/g, ' ');
      if (bt && bt.length <= 140) byline = bt;
    }

    var siteMeta = document.querySelector('meta[property="og:site_name"]');
    var siteName = (siteMeta && siteMeta.getAttribute('content')) || location.host.replace(/^www\./, '');
    var textLen = (clone.textContent || '').trim().replace(/\s+/g, ' ').length;

    send({
      kind: 'reader_article',
      ok: textLen >= 250,
      reason: textLen >= 250 ? null : 'too_short',
      article: {
        title: title,
        site_name: siteName,
        byline: byline,
        url: location.href,
        content_html: clone.innerHTML,
        text_length: textLen
      }
    });
  } catch (err) {
    send({ kind: 'reader_article', ok: false, reason: String(err) });
  }
})();
