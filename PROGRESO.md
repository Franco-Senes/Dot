# PROGRESO.md

| Fase | Estado | Commit | Fecha | Notas |
|---|---|---|---|---|
| 0 — Análisis | ✅ | `549688e` | 2026-11-18 | Análisis de los 171 archivos Swift de upstream, esquemas JSON, atajos, ajustes y protocolo `bench.sock`. |
| 0b — Cambio de motor a CEF | ✅ | `9a2c71c` | 2026-11-18 | Transición a `cef-rs` (`156.6.0` / Chromium `156.0.8078.96`) en modo `Alloy` + OSR sobre GTK4/libadwaita, auditoría binaria de flags/features/códecs/DRM en `libcef.so` y actualización de `AGENTS.md` y `PORTING.md`. |
| 1 — Esqueleto | ✅ | `e8da83e` | 2026-11-18 | Workspace Rust completo (`gnome-platform`, `search-core`, `search`, `search-bench`, `xtask`), `chromium_flags.rs` (`SEARCH_CHROMIUM_FLAGS`), `paths.rs` (`0700` + `write_atomic`), `address.rs`, `settings.rs`, ventana minimalista `AdwApplicationWindow` + CEF 156 OSR (`BrowserView`), privacidad base (GPC, 3rd-party cookies, click-IDs), páginas propias de error/TLS y `--smoke-test` verificado en `370 ms`. |
| 2 — Pestañas y sesión | ✅ | *(este commit)* | 2026-11-18 | Modelo único `AdwTabView` + carga diferida (lazy), pestañas arriba (`AdwTabBar`) y barra lateral vertical (`SidebarTabs` con fijadas compactas y DnD) conmutables en vivo (`Ctrl+Shift+S`), plegado (`Ctrl+S`), favicons descargados vía CEF (`GdkTexture` + `GBytesIcon`), indicador/silencio de audio, menú contextual completo, protección de pestañas fijadas frente a `Ctrl+W`, `target=_blank` / clic central en nueva pestaña, selector difuso `Ctrl+K` (`search_core::fuzzy`) sobre todas las ventanas, multi-ventana (`Ctrl+N`) y ventanas privadas (`Ctrl+Shift+N`, contexto efímero), preferencias (`Ctrl+,`) y atajos (`Ctrl+?`), sesión atómica (`session.json`) con debounce ~1 s, respaldo `.bak` y pila `Ctrl+Shift+T` (≥ 20). Verificado con 15 pestañas (2 fijadas) restauradas en `353 ms` con solo `1` `cef::Browser` activo. |
| 3 — Historial, marcadores, descargas | ⬜ | | | |
| 4 — Shield, Curtain, modo lectura | ⬜ | | | |
| 5 — Vault | ⬜ | | | |
| 6 — Split View y PiP | ⬜ | | | |
| 7 — Bench, D-Bus, tests | ⬜ | | | |
| 8 — Empaquetado y CI | ⬜ | | | |
| 9 — Extensiones | ⬜ | | | |
| 10 — Revisión final | ⬜ | | | |

Leyenda: ⬜ pendiente · 🟡 en curso · ✅ hecha · ⚠️ hecha con pendientes

## Pendientes que arrastran las fases
- En el host Solus 4.9 Serenity faltan los paquetes `-devel` nativos (`sudo -n true` requiere contraseña); se utiliza el SDK local aislado en `.toolchain/` (`org.gnome.Sdk//51` + `rust-stable 1.98` + CEF precompilado `156.0.30`).

---

## Resúmenes por fase

### Fase 0 — Análisis
- **Resumen:** Se analizaron los **171 archivos Swift** (`99.047` líneas) y scripts de `upstream/`, documentando cada módulo, esquemas JSON (`session.json`, `history.json`, `bookmarks.json`, `downloads.json`, `curtain.json`, `Shield`), atajos con `Ctrl`, ajustes y protocolo completo de `bench.sock` en `PORTING.md`.

### Fase 0b — Cambio de motor a CEF y auditoría de flags
- **Resumen:** Se descargó y verificó el binario oficial precompilado de CEF (`156.0.30+g6028375+chromium-156.0.8078.96`) mediante `cef-rs` (`v156.6.0`). Se auditó `libcef.so` e `include/` para documentar en `PORTING.md` §2 la integración `CEF_RUNTIME_STYLE_ALLOY` + Windowless OSR con GTK4/libadwaita (Wayland y X11), el soporte de códecs abiertos (AV1/VP9/VP8/Opus) y Widevine, la tabla completa de equivalencias WebKitGTK → CEF y la lista exacta de switches, `base::Feature` y preferencias de Chromium 156 para `chromium_flags.rs` (descartando los flags obsoletos eliminados en Chromium 156). Se actualizó `AGENTS.md` con las reglas de "Stack obligatorio" y "Rendimiento y memoria".

### Fase 1 — Esqueleto
- **Resumen:** Se creó el workspace Cargo completo (`crates/gnome-platform`, `crates/search-core`, `crates/search`, `crates/search-bench`, `crates/xtask`), copiando `LICENSE` (MIT) y añadiendo `.desktop` y esquema GSettings (`io.github.searchlinux.Search.gschema.xml`). En `search-core` se implementaron `chromium_flags.rs` (todos los switches, features y preferencias de Chromium 156 con soporte dinámico `SEARCH_CHROMIUM_FLAGS` y protección inmutable de Site Isolation), `paths.rs` (directorios XDG `0700`, `write_atomic` y respaldos `.bak`), `address.rs` (URL vs archivo local vs búsqueda con DuckDuckGo por defecto, `localhost`/IPs privadas por `http://`, dominios públicos por `https://`, limpieza de click-IDs y presentación segura sin `user:pass@` ni homógrafos IDN) y `settings.rs` (`settings.json` + sincronización GSettings), con 20 tests unitarios en verde. En `crates/search` se implementaron el despacho temprano `cef::execute_process` antes de GTK4, `cef_engine.rs` (bombeo no bloqueante en el bucle GLib), `browser_view.rs` (vista reutilizable OSR sobre `GtkPicture` + `GdkMemoryTexture` `B8g8r8a8Premultiplied`, reenvío de entrada/escala HiDPI, `Sec-GPC: 1`, filtro de cookies cross-site, búsqueda en página `Ctrl+F`, zoom, DevTools y páginas sobrias propias de error y advertencia TLS autenticada), `window.rs` (`AdwApplicationWindow` minimalista con solo el campo centrado de dirección/búsqueda) y `--smoke-test <url>`, verificado bajo Xvfb + D-Bus en **348 ms** hasta presentar ventana y **370 ms** hasta pintar el primer fotograma OSR `1180×720` con código de salida `0`.

### Fase 2 — Pestañas y sesión
- **Resumen:** Se completó la gestión integral de pestañas, ventanas y sesiones. En `search-core` se implementaron `fuzzy.rs` (búsqueda difusa rápida con `memchr`, subsecuencia ordenada y respaldo Damerau-Levenshtein con `strsim` para erratas) y `session.rs` (`SessionState` compatible con `PORTING.md` §4, soporte multi-ventana, fijadas, grupos y splits, escritura atómica, recuperación de JSON corrupto mediante copia `.bak` y pila LIFO `ClosedTabStack` de capacidad ≥ 20), ampliando a **29 tests unitarios** en verde. En `gnome-platform` se añadieron bindings verificados contra las cabeceras de GNOME SDK 51 para `AdwTabView`, `AdwTabPage`, `AdwTabBar`, `AdwOverlaySplitView`, `AdwDialog`, `AdwPreferencesDialog`/`Page`/`Group`/`SwitchRow`/`ComboRow`/`EntryRow`, `AdwShortcutsDialog`, `GtkListBox`, `GtkScrolledWindow`, `GtkPopoverMenu`/`GMenu`, `GtkDragSource`/`GtkDropTarget`, `GdkTexture::from_bytes` y `GIcon`. En `crates/search` se integró un único `AdwTabView` con carga diferida (lazy) de pestañas inactivas, cambio en vivo entre pestañas arriba (`AdwTabBar`) y barra lateral izquierda vertical (`sidebar_tabs.rs` con sección compacta de fijadas y reordenación DnD), favicons descargados vía `BrowserHost::download_image`, indicador y silencio de audio por pestaña, menú contextual completo, protección de pestañas fijadas frente a `Ctrl+W`, apertura de `target=_blank`/`window.open`/clic central en nuevas pestañas conservando el `RequestContext`, selector difuso `Ctrl+K` (`tab_switcher.rs`) sobre todas las ventanas, ventanas normales (`Ctrl+N`) y privadas (`Ctrl+Shift+N`, contexto efímero nunca guardado), diálogo de preferencias (`settings_ui.rs`, `Ctrl+,`) y diálogo de atajos (`Ctrl+?`), además de corregir reentrada de `RefCell` en callbacks de `AddressBar`/`FindBar`/`SettingsStore`. Verificado headless bajo Xvfb + D-Bus: `--smoke-test` en **373 ms** (`1180×720`) y ciclo completo de 15 pestañas (2 fijadas) `--smoke-test-session-create` → `--smoke-test-session-restore` restaurando todas las pestañas en **353 ms** al presentar (**383 ms** al primer fotograma OSR) con exactamente **1** instancia `cef::Browser` activa.
