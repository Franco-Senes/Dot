# Reglas del proyecto Dot (léelas siempre)

## Qué es este proyecto
Dot (`dev.senes.Dot`): navegador web minimalista, rápido y privado para Linux basado en "Search" (https://github.com/driceroland/Search, licencia MIT, Office Commun). La interfaz es solo
pestañas (arriba o en una barra lateral) y la página: sin barra de herramientas,
sin página de inicio, sin inicio de sesión.
El código original está en `./upstream` (solo lectura, ignorado por git).

## Stack obligatorio
- **Todo en Rust en la máxima medida posible** (requisito explícito del usuario, prioritario sobre el kit):
  - Rust estable (edición 2021 o posterior) en un **Cargo workspace**:
    - `crates/dot`: aplicación GUI nativa en `gtk4` + `libadwaita` con motor **Chromium Embedded Framework precompilado (`cef` / `cef-rs` v156.6.0, Chromium 156)** en modo `Alloy` + renderizado windowless OSR integrado en GTK4 (SIN compilar nuestro propio Chromium/CEF).
    - `crates/dot-core`: toda la lógica de dominio sin dependencia de GTK/CEF (`chromium_flags.rs`, rutas XDG + migración única desde `search/`, variables `DOT_*`, URL/búsqueda/`dot://`, sesiones, historial, marcadores, descargas, motor de reglas de Shield, generación de selectores CSS de Curtain, Vault, importación de contraseñas, updater, CRX3, protocolo de bench) para tests unitarios puros.
    - `crates/dot-bench`: herramientas CLI en Rust (`bench`, `test-find`), cliente del socket de bench y tests de integración/headless en Rust (incluido el port de `Tests/split_view.py` y el servidor HTTP local de pruebas en Rust).
    - `crates/xtask`: instalador de archivos de escritorio (`.desktop`, metainfo, iconos, compilación de esquema GSettings y empaquetado del bundle CEF) en Rust, sustituyendo a Meson.
  - Sin Python, sin C y sin Vala en la aplicación, herramientas ni tests.
  - JavaScript **únicamente** donde debe ejecutarse dentro de la página web (scripts mínimos de recolección para Curtain, detección de formularios de Vault, shims de extensiones y `Readability.js`); toda la lógica de decisión vive en Rust.
  - Bash **únicamente** para envoltorios mínimos (p. ej. invocar `flatpak-builder` o `xvfb-run`).
- Distribución principal: Flatpak (runtime `org.gnome.Platform`).
- Debe funcionar en Wayland y en X11.

## Principios que no se negocian
1. Sin telemetría, sin cuentas, sin sincronización, sin servicios de Google (tampoco Safe Browsing de Google) ni peticiones de red que el usuario no haya pedido (la única excepción es la comprobación de actualizaciones, que se puede desactivar y está apagada en Flatpak).
2. Minimalismo: no añadas barras, botones ni pantallas que el original no tenga. Nunca muestres la interfaz propia de Chrome.
3. Rendimiento: arranque en frío rápido y consumo contenido de RAM.
4. Ninguna contraseña ni dato sensible en texto plano en disco ni en los logs.

## Rendimiento y memoria (optimización de CEF con flags — Nivel 2)
1. **Módulo único `chromium_flags.rs`:** Todos los flags, features (`--enable-features` / `--disable-features`) y preferencias internas de Chromium se definen en un único sitio (`crates/dot-core/src/chromium_flags.rs` + aplicación en `on_before_command_line_processing`, `CefSettings` y preferencias del `RequestContext`), cada uno documentado con qué hace y por qué está.
2. **Override en tiempo de ejecución:** La variable de entorno `DOT_CHROMIUM_FLAGS` (con respaldo obsoleto `SEARCH_CHROMIUM_FLAGS`) permite añadir (`--foo` / `+feature:Bar`) o quitar (`---foo` / `-switch` / `!switch`) flags sin recompilar para pruebas y mediciones.
3. **Verificación contra la versión exacta de Chromium (156.0.8078.96):** Solo se incluyen flags y `base::Feature` cuya existencia se haya comprobado en el binario/código de Chromium 156; los inexistentes o renombrados se descartan y se documentan en `PORTING.md`.
4. **Cero servicios de Google ni segundo plano:** Desactivar `--disable-background-networking`, `--disable-component-update`, `--disable-sync`, `--disable-default-apps`, `--disable-domain-reliability`, `--disable-breakpad`, `--no-pings`, features `Translate`, `OptimizationHints`, `OptimizationTargetPrediction`, `MediaRouter`, `DialMediaRouteProvider`, `AutofillServerCommunication`, `SafeBrowsing`, `PasswordManager`, y desactivar en preferencias Safe Browsing, gestor de contraseñas y autocompletado de Chromium (lo gestiona `Vault`).
5. **Memoria, procesos y seguridad:** Probar y medir por separado `--renderer-process-limit` (4, 6 y 8), `--process-per-site`, ahorro de memoria (`performance_tuning.high_efficiency_mode.state`) y opcionalmente `--js-flags=--lite-mode`. **PROHIBIDO desactivar Site Isolation** (`site-per-process`). Dejar activos por defecto solo los flags que aporten mejora medible sin romper sitios web.
6. **Vídeo por hardware (AMD Radeon Vega — Ryzen 5 5600G):** Probar y medir decodificación VA-API (`AcceleratedVideoDecodeLinuxGL`, `AcceleratedVideoDecodeLinuxZeroCopyGL`, `VaapiIgnoreDriverChecks`); si causa fallos o pantallas negras en algún entorno, dejarla controlada por ajuste y documentarlo.

## Estructura del código
- Un módulo por responsabilidad, con los mismos nombres que el original para poder compararlos: `chromium_flags.rs`, `vault.rs`, `shield.rs`, `curtain.rs`, `session.rs`, `updater.rs`, `bench.rs`, `crx.rs`, `extensions.rs`, etc.
- Rutas solo a través de `paths.rs` (XDG):
  - datos: `$XDG_DATA_HOME/dot`
  - ajustes: `$XDG_CONFIG_HOME/dot`
  - caché: `$XDG_CACHE_HOME/dot`
  - runtime/sockets: `$XDG_RUNTIME_DIR/dot`
- Datos en JSON con el mismo esquema que el original cuando sea posible.
  Escritura siempre atómica (archivo temporal + rename) y con debounce.
- Los atajos con ⌘ del original pasan a Ctrl.
- Sin `unwrap()`/`expect()` en rutas que dependan del usuario, la red o el disco.
  Usa `anyhow`/`thiserror` y registra con `tracing`.

## Forma de trabajar
- Haz SOLO la fase que se te pide. No adelantes trabajo de otras fases.
- No reorganices la arquitectura existente sin justificarlo en PORTING.md.
- Antes de usar una API de `cef-rs`, `gtk4-rs` o `libadwaita`, comprueba que existe en la versión instalada (`.toolchain/cef-rs-ref` y cabeceras del SDK). No inventes APIs. Si algo no es posible, dilo y documéntalo en PORTING.md.
- Antes de dar una fase por terminada, todo esto debe pasar:
  `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test` y `cargo build --release`.
- Al terminar: actualiza PORTING.md (columna de estado) y PROGRESO.md, haz commit
  con el mensaje `Fase N: <resumen>` y continúa con la siguiente fase.

## Licencias
El original es MIT: conserva el aviso de copyright en LICENSE y da crédito en el
README. El código de terceros que se incluya (p. ej. Readability.js, Apache-2.0, y avisos de CEF/Chromium BSD) va documentado con su licencia.
