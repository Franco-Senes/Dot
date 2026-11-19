# REVISION.md — Auditoría final (Fase 10)

Revisión integral de seguridad, privacidad, corrección, rendimiento, arquitectura y cumplimiento de `AGENTS.md` sobre el workspace Rust (`gnome-platform`, `search-core`, `search`, `search-bench`, `xtask`).

> **Resumen ejecutivo:** Se identificaron **3 hallazgos Críticos** (todos **corregidos en Fase 10**: el orden de prefijos IPC heredado de Fase 5/4 ya resuelto en `feat/phase7-bench-dbus` `9738c46`, los ciclos de referencia `Rc` ↔ `GObject` y el pánico reentrante de `RefCell` en `window.rs:1864` durante el cierre), **2 Altos**, **6 Medios** (dos de ellos —`bench.rs` y `dbus.rs`— ya implementados en `feat/phase7-bench-dbus` `9738c46`) y **4 Bajos**. Todos los puntos no críticos pendientes quedan registrados como tareas accionables en [`ISSUES.md`](ISSUES.md).

---

## Crítico

### C-1. Colisión de prefijo IPC rompe Curtain y Reader — **CORREGIDO** (`feat/phase7-bench-dbus`, `9738c46`)
- **Ubicación:** `crates/search/src/browser_view.rs:66`, `crates/search/src/browser_view.rs:1353-1357`, `crates/search/src/browser_view.rs:1370-1404` (y emisores en `curtain.js:3,20`, `crates/search/src/reader.rs:18-20`).
- **Problema:** `IPC_PREFIX = "__search_ipc_"` en `on_console_message` interceptaba *todo* mensaje que comenzara por `__search_ipc_`, lo enrutaba a `route_vault_ipc` y devolvía `1` anticipadamente. Como Curtain y Reader emiten `__search_ipc_<ipc_nonce>:<json>`, sus mensajes fallaban la búsqueda en `vault_frames` y se descartaban antes de llegar al bloque `ipc_prefix` (`__search_ipc_<ipc_nonce>:`), dejando inalcanzables `on_curtain_pick`, `on_curtain_picking_changed` y `on_reader_extracted`.
- **Por qué importa:** Rompe por completo el selector visual de Curtain y la extracción de artículos del modo Reader, además de mezclar el canal autenticado de Vault con mensajes no-Vault.
- **Cómo arreglarlo / Estado:** **Corregido en `feat/phase7-bench-dbus` (`9738c46`)** comprobando `__search_ipc_<ipc_nonce>:` antes que el prefijo genérico de Vault (manteniendo `return 1` para suprimir ambos del log de consola). Como mejora adicional de defensa en profundidad, conviene migrar Vault a un prefijo exclusivo (`__search_vault_<token>:`).

### C-2. Ciclos de referencia `Rc` ↔ `GObject` (fugas de pestañas, ventanas y diálogos) — **CORREGIDO en Fase 10**
- **Ubicación:**
  - `crates/gnome-platform/src/object.rs` (carecía de `WeakRef` / `GWeakRef`).
  - `crates/search/src/browser_view.rs:182-184, 696-704, 773-1068`: `SharedState` retiene `ObjectRef` a `overlay`, `picture` y `status_pill`, y `attach_gtk_controllers` capturaba `self.clone()` (`Rc<SharedState>`) fuertemente en 13 señales (además de autociclos en `motion`, `click` y `key`); `close_browser()` tampoco limpiaba `picture.set_texture(None)` ni `callbacks`.
  - `crates/search/src/window.rs:572, 578, 2974-3018, 3045, 3092, 3256`: `wire_signals` capturaba `Rc<WindowState>` fuertemente en señales de `address_bar`, `side_split`, `find_bar`, `tab_view` y `window.connect_close_request`; además, `window.rs:572, 578` se suscribían a `SettingsStore` y `PinFoldersManager` sin `unsubscribe`.
  - `crates/search/src/tab_switcher.rs:195-202`: `key.connect_key_pressed` capturaba `dialog.clone()` y `list_box.clone()`, creando un ciclo `AdwDialog` ↔ `EventControllerKey` en cada apertura de `Ctrl+Tab`.
- **Por qué importa:** Cada pestaña cerrada, ventana cerrada o diálogo `Ctrl+Tab` retenía en memoria el estado Rust, los widgets GTK y la última `GdkMemoryTexture` OSR sin liberarse nunca.
- **Cómo arreglarlo / Estado:** **Corregido en Fase 10** añadiendo `WeakRef` (`GWeakRef`) en `crates/gnome-platform/src/object.rs`, sustituyendo capturas fuertes por referencias débiles en señales GTK (`browser_view.rs`, `window.rs`, `actions.rs`, `tab_switcher.rs`), limpiando `picture.set_texture(None)`/`callbacks` al cerrar la pestaña y desuscribiendo los observadores (`SettingsStore`/`PinFoldersManager`) al cerrar la ventana, con tests unitarios que verifican `Weak::strong_count() == 0`.

### C-3. Pánico reentrante `RefCell already mutably borrowed` al cerrar (`window.rs:1864`) — **CORREGIDO en Fase 10**
- **Ubicación:** `crates/search/src/window.rs` (`close_all_browsers`, `put_tab_to_sleep`, `mount_settings_page_in_tab`).
- **Problema:** `close_all_browsers` mantenía `self.inner.tabs.borrow_mut()` mientras invocaba `bv.close_browser()`, cuya cascada de cierre en CEF/GTK podía reentrar en `selected_tab_id()` (`self.inner.tabs.borrow()`), provocando `RefCell already mutably borrowed` durante `headless_session_save_and_lazy_restore`.
- **Cómo arreglarlo / Estado:** **Corregido en Fase 10** extrayendo los `BrowserView` de `self.inner.tabs` y soltando el `RefMut` antes de llamar a `bv.close_browser()`.

---

## Alto

### A-1. E/S síncrona con `fsync` y parsing pesado en el hilo principal GTK
- **Ubicación:**
  - `crates/search-core/src/paths.rs:148-180+` (`write_atomic` ejecuta `sync_all` + `rename`) invocado síncronamente desde el hilo principal en `crates/search/src/app.rs:392-420` (`save_session_now`), `crates/search/src/history_ui.rs:124-138` (`HistoryManager::flush_now`), `crates/search/src/curtain.rs:159`, `crates/search/src/bookmarks_ui.rs:145`, `crates/search/src/sidebar_tabs.rs:111` y `crates/search/src/downloads.rs:428`.
  - `crates/search/src/bookmarks_ui.rs:585` (`fs::read_to_string` + importación HTML de marcadores en el hilo principal).
  - `crates/search/src/settings_page.rs:1843` → `crates/search/src/extensions.rs:34-42` (`install_crx_file`: lectura de archivo, verificación criptográfica RSA/ECDSA y extracción ZIP en el hilo principal).
- **Por qué importa:** Cada `fsync` o importación/instalación bloquea el bucle de eventos GTK4/Wayland, provocando micro-tirones de interfaz durante la navegación o congelamientos perceptibles al importar marcadores o instalar extensiones CRX grandes.
- **Cómo arreglarlo:** Mover `write_atomic` de los flushes periódicos, la importación HTML de marcadores y `install_crx_file` a un hilo de trabajo (`std::thread::spawn` / cola dedicada) devolviendo el resultado al hilo principal vía `glib::idle_add_once` o canal `mpsc`, manteniendo el guardado síncrono únicamente en el apagado final (`shutdown`).

### A-2. Escaneo completo del búfer OSR y copia completa en cada `on_paint`
- **Ubicación:** `crates/search/src/browser_view.rs:1135-1174` (especialmente línea `1167`).
- **Por qué importa:** `slice.iter().any(|&b| b != 0)` recorre los ~8 MiB+ del búfer BGRA completo en el hilo principal en *cada* fotograma, incluso cuando `non_empty_frames > 0` y `on_non_empty_paint` es `None`. Además se copia todo el framebuffer con `Bytes::from_slice(slice)` sin aprovechar `_dirty_rects`.
- **Cómo arreglarlo:** Cortocircuitar el escaneo cuando `non_empty_frames > 0 && callbacks.on_non_empty_paint.is_none()` (o muestrear antes), y evaluar actualización parcial o reutilización de búfer cuando `dirty_rects` cubra un área pequeña.

---

## Medio

### M-1. Navegación privada puede filtrar origins y credenciales de Vault a disco/llavero
- **Ubicación:** `crates/search/src/window.rs:2209-2216` y `crates/search-core/src/vault.rs:451-516, 758-760`.
- **Por qué importa:** `window.rs` conecta `crate::vault::attach(...)` sin pasar `self.inner.is_private`. En una ventana privada, enviar un formulario ofrece guardar la contraseña en Secret Service y pulsar «Nunca en este sitio» escribe el origin en `$XDG_DATA_HOME/search/vault_never.json`, violando el principio de no persistir actividad privada en disco.
- **Cómo arreglarlo:** Pasar `is_private` a `vault::attach` y desactivar la captura de credenciales y la acción «Nunca en este sitio» cuando `is_private == true` (permitiendo únicamente autocompletado explícito si el usuario lo solicita o deshabilitándolo según política).

### M-2. Ventana TOCTOU en `BrowserView::vault_fill`
- **Ubicación:** `crates/search/src/browser_view.rs:732-748`.
- **Por qué importa:** `vault_fill` revalida `frame.url()` frente a `expected_origin`, pero no vuelve a comprobar que `browser.main_frame().url()` siga perteneciendo al mismo origin antes de llamar a `execute_java_script`. Una navegación rápida del frame principal entre la solicitud y el rellenado podría romper el aislamiento esperado.
- **Cómo arreglarlo:** Revalidar también `browser.main_frame().url()` contra `expected_origin` inmediatamente antes de ejecutar el script de rellenado.

### M-3. Módulos de Fase 7/8/9 aún como stubs de 1 línea en `crates/search/src/`
- **Ubicación:** `crates/search/src/bench.rs:1`, `crates/search/src/dbus.rs:1`, `crates/search/src/updater.rs:1`, `crates/search/src/extension_shims.rs:1` (en la base `83163b2`).
- **Por qué importa:** Aunque la lógica de dominio y pruebas reside en `search-core` y `search-bench`, los puntos de entrada en `crates/search` figuraban como `TODO`.
- **Estado:** `bench.rs` y `dbus.rs` quedaron **implementados en `feat/phase7-bench-dbus` (`9738c46`)** (socket `$XDG_RUNTIME_DIR/search/bench.sock` `0600` con `SO_PEERCRED` y servicio GDBus `io.github.searchlinux.Search1`). Quedan pendientes de cableado completo en `crates/search` los módulos `updater.rs` y `extension_shims.rs`.

### M-4. Sondeo fijo de 10 ms en el bucle de mensajes CEF sobre GTK
- **Ubicación:** `crates/search/src/cef_engine.rs:304-310`.
- **Por qué importa:** `glib::timeout_add_local(Duration::from_millis(10), ...)` despierta el hilo principal 100 veces por segundo de manera continua aunque el navegador esté en reposo y ya exista `on_schedule_message_pump_work`.
- **Cómo arreglarlo:** Confiar en `on_schedule_message_pump_work` con un temporizador único reprogramable según `delay_ms` y un respaldo de baja frecuencia (o nulo en reposo) para reducir consumo de CPU/batería en portátil.

### M-5. Duplicación del patrón de gestores con debounce y suscripción
- **Ubicación:** `crates/search/src/{curtain.rs,bookmarks_ui.rs,history_ui.rs,sidebar_tabs.rs,downloads.rs,settings.rs}`.
- **Por qué importa:** Los seis módulos replican el patrón `Rc<RefCell<Inner>>` + `subscribe` / `flush_now` / `schedule_save` sin `unsubscribe` ni abstracción común, aumentando la superficie de bugs de ciclo de vida.
- **Cómo arreglarlo:** Extraer un helper genérico en `crates/search` (p. ej. `DebouncedStore<T>` con tokens de suscripción `SubscriptionId` o `Weak` callbacks).

### M-6. Módulos y funciones sobredimensionados en `crates/search`
- **Ubicación:**
  - `crates/search/src/window.rs` (3728 líneas; `BrowserWindow::new` 251-591, `wire_signals` 2956-3275, `install_tab_context_actions` 2722-2955, `create_browser_view_for_tab` 1999-2227).
  - `crates/search/src/settings_page.rs` (2202 líneas; secciones de ~200-306 líneas).
  - `crates/search/src/browser_view.rs` (2668 líneas; `attach_gtk_controllers` 298 líneas).
- **Por qué importa:** Dificulta la revisión, el mantenimiento y el aislamiento de responsabilidades.
- **Cómo arreglarlo:** Dividir en submódulos coherentes (`window/{actions,signals,tabs}.rs`, `settings_page/*.rs`, `browser_view/{controllers,ipc,osr}.rs`) sin cambiar la arquitectura pública.

---

## Bajo

### B-1. Reader sin `<meta http-equiv="Content-Security-Policy">` en `<head>`
- **Ubicación:** `crates/search-core/src/reader.rs:749-769` (`render_reader_document`).
- **Por qué importa:** Aunque `sanitize_article_html` limpia el HTML extraído, añadir `default-src 'none'; style-src 'unsafe-inline'; img-src https: http: data:; script-src 'none'; object-src 'none'` ofrece defensa en profundidad frente a mXSS.
- **Cómo arreglarlo:** Insertar la etiqueta `<meta http-equiv="Content-Security-Policy" ...>` en `render_reader_document`.

### B-2. Generación de nonces con fallback a reloj e ignorando `Result` de `read_exact`
- **Ubicación:** `crates/search/src/browser_view.rs:2491-2508` (`generate_hex_nonce_128`).
- **Por qué importa:** Si fallara `/dev/urandom`, el código ignora el `Result` de `read_exact` y cae en un hash de `SystemTime`, predecible.
- **Cómo arreglarlo:** Usar `gnome_platform::crypto::random_bytes` (o `ring::rand::SystemRandom`) propagando o registrando errores explícitamente.

### B-3. `no_sandbox = 1` fuera de Flatpak si falta `chrome-sandbox` setuid
- **Ubicación:** `crates/search/src/cef_engine.rs:129-149` (`detect_no_sandbox`).
- **Por qué importa:** En Flatpak/`bwrap` (`PR_GET_NO_NEW_PRIVS == 1`) es esperado, pero en instalaciones nativas sin `chrome-sandbox` `04755` desactiva el sandbox SUID de Chromium silenciosamente.
- **Cómo arreglarlo:** Emitir un `tracing::warn!` claro cuando `no_sandbox = 1` se active fuera de contenedor (`/.flatpak-info` ausente) y documentar el permiso `04755` en el empaquetado nativo.

### B-4. `.expect()` en constructores FFI de `gnome-platform` y benchmarks pendientes en `PORTING.md`
- **Ubicación:** `crates/gnome-platform/src/object.rs:31`, `crates/gnome-platform/src/adw.rs:194,643,651,659`, `crates/gnome-platform/src/glib.rs:44`; y `PORTING.md:534-540` (§12).
- **Por qué importa:** Aunque en `search`, `search-core` y `xtask` hay `0` `unwrap()`/`expect()` fuera de `#[cfg(test)]`, los 6 `.expect("... returned null")` en constructores GObject de `gnome-platform` podrían documentarse o devolverse como `Option`/`Result` si fallara el display. Además, en `83163b2` `PORTING.md` §12 tenía `*(Por medir)*` en flags de Chromium (ya medidos e incorporados en `feat/phase7-bench-dbus` `9738c46`).
- **Cómo arreglarlo:** Mantener actualizados los números de §12 tras integrar `feat/phase7-bench-dbus` y valorar `Result` en constructores FFI críticos.

---

## Controles verificados conformes

- **Secretos y Vault:** `Zeroizing` + `secret_password_free` (`crates/gnome-platform/src/secret.rs:354-357`); `ContentProvider::for_secret_string` + borrado de portapapeles a los 30 s solo si `clipboard_holds` (`crates/gnome-platform/src/gdk.rs:322-367`, `crates/search/src/vault_ui.rs:471-487`); exportación CSV con permisos `0600` (`vault_ui.rs:455-467`); `set_use_markup(false)` (`vault_ui.rs:734,1111`); copias temporales `0700` con limpieza en `Drop` (`vault_import.rs:272-312`); sin fallback en texto plano.
- **CRX3 y Updater (`search-core`):** Verificación RSA-PKCS1v15-SHA256 / ECDSA-P256-SHA256 + `crx_id` + protección anti-traversal, symlinks y zip-bombs (`crates/search-core/src/crx.rs:329-536, 864-1116`); verificación Ed25519 previa al parseo JSON + allowlist HTTPS + bloqueo automático cuando existe `/.flatpak-info` (`crates/search-core/src/updater.rs:281-390`).
- **Privacidad, CEF y `AGENTS.md`:** Cero servicios de Google y Site Isolation protegido frente a overrides en `SEARCH_CHROMIUM_FLAGS` (`crates/search-core/src/chromium_flags.rs:16-147, 400-450`); sin archivos `.py`, `.c` ni `.vala` fuera de `upstream/`; `0` `unwrap()`/`expect()`/`panic!()` fuera de `#[cfg(test)]` en `search`, `search-core` y `xtask`; FFI en `crates/gnome-platform/src/{secret.rs,crypto.rs,sqlite.rs}` usa `Zeroizing` y `Drop` correctamente; `history.suggest` en `crates/search-core/src/history.rs:330-404` optimizado con máscaras de bits y `memchr`.
- **Bench y D-Bus (`feat/phase7-bench-dbus`, `9738c46`):** Socket `$XDG_RUNTIME_DIR/search/bench.sock` apagado por defecto, permisos `0600` y comprobación `SO_PEERCRED` (`UID == geteuid()`); pruebas headless 100 % en Rust (`crates/search-bench/tests/headless_integration.rs`).

---

## Recomendaciones antes de publicar la versión 1.0

1. **Integrar y validar en conjunto:** Asegurar que tras el merge de `feat/phase7-bench-dbus` y los fixes Críticos de Fase 10 pasan `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `scripts/test-headless.sh` y el build de Flatpak.
2. **Resolver los ítems `Alto` (`A-1`, `A-2`) y `M-1`/`M-2`:** Sacar `write_atomic` e importación CRX/HTML del hilo principal, evitar el escaneo completo de `on_paint` en cada frame, aislar Vault en navegación privada (`M-1`) y revalidar `main_frame` en `vault_fill` (`M-2`).
3. **Completar cableado de `updater.rs` y `extension_shims.rs`** en `crates/search/src/` para cerrar la paridad de Fases 8 y 9.
4. **Documentación de release:** Mantener visible en las notas de versión la limitación de códecs patentados H.264/AAC y Widevine en CEF precompilado documentada en [`BLOQUEOS.md`](BLOQUEOS.md).
