# ISSUES.md — Backlog accionable post-Fase 10

Lista de mejoras no críticas (`Alto`, `Medio`, `Bajo`) detectadas durante la auditoría final de Fase 10 ([`REVISION.md`](REVISION.md)), listas para convertirse en issues de GitHub.

---

## Prioridad Alta

### [A-1] Mover E/S síncrona (`write_atomic`, importación HTML y `install_crx_file`) fuera del hilo principal GTK
- **Severidad:** Alto
- **Archivos afectados:**
  - `crates/search-core/src/paths.rs:148-180+`
  - `crates/search/src/app.rs:392-420` (`save_session_now`)
  - `crates/search/src/history_ui.rs:124-138` (`HistoryManager::flush_now`)
  - `crates/search/src/curtain.rs:159`, `crates/search/src/bookmarks_ui.rs:145, 585`, `crates/search/src/sidebar_tabs.rs:111`, `crates/search/src/downloads.rs:428`
  - `crates/search/src/settings_page.rs:1843`, `crates/search/src/extensions.rs:34-42`
- **Descripción:** Las escrituras atómicas con `sync_all()`, la lectura/parseo de marcadores HTML y la verificación/descompresión de `.crx` se ejecutan síncronamente en el hilo principal de GTK, pudiendo causar bloqueos de UI.
- **Criterio de aceptación:**
  - Los flushes periódicos con debounce envían la serialización al hilo secundario y solo `shutdown` espera confirmación síncrona.
  - La importación de marcadores HTML y `install_crx_file` corren en un hilo de trabajo y notifican a la UI vía `glib::idle_add_once`.

### [A-2] Evitar escaneo completo del búfer OSR y copias innecesarias en `on_paint`
- **Severidad:** Alto
- **Archivos afectados:** `crates/search/src/browser_view.rs:1135-1174` (línea `1167`)
- **Descripción:** `slice.iter().any(|&b| b != 0)` recorre todo el framebuffer BGRA en el hilo principal en cada fotograma incluso cuando `non_empty_frames > 0` y `on_non_empty_paint` es `None`. Además se copia todo el búfer en cada `on_paint` ignorando `dirty_rects`.
- **Criterio de aceptación:**
  - No ejecutar el escaneo `any(|&b| b != 0)` cuando ya haya frames no vacíos y no exista callback `on_non_empty_paint`.
  - Evaluar y documentar/aplicar optimización con `dirty_rects` o reducción de copias sin regresiones visuales en `test-headless.sh`.

---

## Prioridad Media

### [M-1] Desactivar captura y «Nunca en este sitio» de Vault en ventanas privadas
- **Severidad:** Medio
- **Archivos afectados:** `crates/search/src/window.rs:2209-2216`, `crates/search-core/src/vault.rs:451-516, 758-760`
- **Descripción:** `crate::vault::attach(...)` no recibe `self.inner.is_private`, permitiendo que en navegación privada se ofrezca guardar credenciales en Secret Service o se escriba el origin en `$XDG_DATA_HOME/search/vault_never.json`.
- **Criterio de aceptación:**
  - Pasar `is_private` a `vault::attach`.
  - En pestañas privadas, no mostrar banner de guardado ni escribir en `vault_never.json`.
  - Test unitario/integración verificando que el modo privado no persiste origins en `vault_never.json`.

### [M-2] Revalidar `main_frame` contra `expected_origin` en `BrowserView::vault_fill` (TOCTOU)
- **Severidad:** Medio
- **Archivos afectados:** `crates/search/src/browser_view.rs:732-748`
- **Descripción:** `vault_fill` valida `frame.url()` antes de inyectar JavaScript, pero no comprueba que `browser.main_frame().url()` siga coincidiendo con `expected_origin`.
- **Criterio de aceptación:**
  - Verificar también el origin de `browser.main_frame()` inmediatamente antes de `execute_java_script`.
  - Abortar con log `debug`/`warn` si el origin del frame principal cambió.

### [M-3] Completar cableado de `updater.rs` y `extension_shims.rs` en `crates/search`
- **Severidad:** Medio
- **Archivos afectados:** `crates/search/src/updater.rs:1`, `crates/search/src/extension_shims.rs:1`
- **Descripción:** Mientras que `bench.rs` y `dbus.rs` quedaron implementados en `feat/phase7-bench-dbus` (`9738c46`), `updater.rs` y `extension_shims.rs` en `crates/search/src/` siguen como stubs de 1 línea respecto a la lógica existente en `search-core`.
- **Criterio de aceptación:**
  - Conectar `updater.rs` con `search_core::updater` (respetando desactivación automática bajo `/.flatpak-info` y ajuste de usuario).
  - Conectar `extension_shims.rs` con la inyección/puente de extensiones Chrome MV2/MV3 en `BrowserView`.

### [M-4] Reemplazar sondeo fijo de 10 ms en `cef_engine.rs` por programación bajo demanda
- **Severidad:** Medio
- **Archivos afectados:** `crates/search/src/cef_engine.rs:304-310`
- **Descripción:** `glib::timeout_add_local(Duration::from_millis(10), ...)` despierta el hilo principal 100 veces/s de forma continua aunque exista `on_schedule_message_pump_work`.
- **Criterio de aceptación:**
  - Usar temporizador de un solo disparo reprogramado por `on_schedule_message_pump_work(delay_ms)` sin polling constante en reposo.
  - Verificar que las animaciones, cargas y pruebas headless siguen funcionando sin latencia añadida.

### [M-5] Extraer helper genérico para gestores con debounce y suscripción (`unsubscribe`)
- **Severidad:** Medio
- **Archivos afectados:** `crates/search/src/{curtain.rs,bookmarks_ui.rs,history_ui.rs,sidebar_tabs.rs,downloads.rs,settings.rs}`
- **Descripción:** Los seis gestores duplican el patrón `Rc<RefCell<Inner>>` + `subscribe`/`flush_now`/`schedule_save` y carecen de mecanismo de desuscripción.
- **Criterio de aceptación:**
  - Crear un helper reutilizable (p. ej. `DebouncedStore` o `ObserverList`) con soporte de `unsubscribe` / `SubscriptionGuard`.
  - Migrar los seis módulos eliminando código duplicado.

### [M-6] Refactorizar módulos sobredimensionados (`window.rs`, `settings_page.rs`, `browser_view.rs`)
- **Severidad:** Medio
- **Archivos afectados:** `crates/search/src/window.rs` (3728 líneas), `crates/search/src/browser_view.rs` (2668 líneas), `crates/search/src/settings_page.rs` (2202 líneas)
- **Descripción:** Concentran funciones de 200–340 líneas (`BrowserWindow::new`, `wire_signals`, `install_tab_context_actions`, `create_browser_view_for_tab`, `attach_gtk_controllers`).
- **Criterio de aceptación:**
  - Dividir en submódulos internos manteniendo intactas las interfaces públicas y pasando `cargo clippy --all-targets -- -D warnings` y `cargo test`.

---

## Prioridad Baja

### [B-1] Añadir `<meta http-equiv="Content-Security-Policy">` en el documento de Reader
- **Severidad:** Bajo
- **Archivos afectados:** `crates/search-core/src/reader.rs:749-769`
- **Descripción:** Reforzar `sanitize_article_html` con una política CSP estricta (`script-src 'none'; object-src 'none'`) en `render_reader_document`.
- **Criterio de aceptación:**
  - Incluir etiqueta `<meta http-equiv="Content-Security-Policy" ...>` en `<head>` y verificar con test unitario en `search-core`.

### [B-2] Usar CSPRNG del sistema sin fallback a reloj en `generate_hex_nonce_128`
- **Severidad:** Bajo
- **Archivos afectados:** `crates/search/src/browser_view.rs:2491-2508`
- **Descripción:** `generate_hex_nonce_128` ignora el `Result` de `read_exact` y usa `SystemTime` como fallback.
- **Criterio de aceptación:**
  - Reemplazar por `gnome_platform::crypto::random_bytes` (o `ring::rand::SystemRandom`) sin fallback determinista/temporal.

### [B-3] Registrar advertencia explícita si `no_sandbox = 1` se activa fuera de Flatpak
- **Severidad:** Bajo
- **Archivos afectados:** `crates/search/src/cef_engine.rs:129-149`
- **Descripción:** `detect_no_sandbox` desactiva el sandbox si `chrome-sandbox` no es `04755` o `PR_GET_NO_NEW_PRIVS == 1`. Fuera de contenedor conviene emitir un aviso claro.
- **Criterio de aceptación:**
  - Añadir `tracing::warn!` cuando `no_sandbox == 1` y no exista `/.flatpak-info`.

### [B-4] Revisar `.expect()` en constructores FFI de `gnome-platform` y alinear `PORTING.md` §12
- **Severidad:** Bajo
- **Archivos afectados:** `crates/gnome-platform/src/object.rs:31`, `crates/gnome-platform/src/adw.rs:194,643,651,659`, `crates/gnome-platform/src/glib.rs:44`, `PORTING.md:534-540`
- **Descripción:** Evaluar si los 6 `.expect("... returned null")` en constructores GObject deben propagar error o mantenerse como invariantes documentados; confirmar que `PORTING.md` §12 queda con los resultados reales tras el merge de `feat/phase7-bench-dbus`.
- **Criterio de aceptación:**
  - Sin `*(Por medir)*` pendientes en `PORTING.md` §12 tras el merge final y decisión documentada sobre los constructores FFI de `gnome-platform`.
