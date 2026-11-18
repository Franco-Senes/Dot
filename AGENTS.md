# Reglas del proyecto search-linux (léelas siempre)

## Qué es este proyecto
Port a Linux de "Search" (https://github.com/driceroland/Search, licencia MIT),
un navegador pequeño, rápido y silencioso para macOS basado en WebKit. La
interfaz es solo pestañas (arriba o en una barra lateral) y la página: sin barra
de herramientas, sin página de inicio, sin inicio de sesión.
El código original está en `./upstream` (solo lectura, ignorado por git).

## Stack obligatorio
- Rust estable (edición 2021 o posterior).
- gtk4-rs, libadwaita-rs y el crate `webkit6` (WebKitGTK 6.0, la API para GTK4).
- Build: Cargo para el código y Meson para instalar los datos (.desktop,
  metainfo, iconos, esquema GSettings).
- Distribución principal: Flatpak (runtime org.gnome.Platform).
- Debe funcionar en Wayland y en X11.

## Principios que no se negocian
1. Sin telemetría, sin cuentas, sin sincronización, sin peticiones de red que
   el usuario no haya pedido (la única excepción es la comprobación de
   actualizaciones, que se puede desactivar y está apagada en Flatpak).
2. Minimalismo: no añadas barras, botones ni pantallas que el original no tenga.
3. Rendimiento: arranque en frío por debajo de ~300 ms en hardware medio.
4. Ninguna contraseña ni dato sensible en texto plano en disco ni en los logs.

## Estructura del código
- Un módulo por responsabilidad, con los mismos nombres que el original para
  poder compararlos: `vault.rs`, `shield.rs`, `curtain.rs`, `session.rs`,
  `updater.rs`, `bench.rs`, `crx.rs`, `extensions.rs`, etc.
- Rutas solo a través de `paths.rs` (XDG):
  - datos: `$XDG_DATA_HOME/search`
  - ajustes: `$XDG_CONFIG_HOME/search`
  - caché: `$XDG_CACHE_HOME/search`
  - runtime/sockets: `$XDG_RUNTIME_DIR/search`
- Datos en JSON con el mismo esquema que el original cuando sea posible.
  Escritura siempre atómica (archivo temporal + rename) y con debounce.
- Los atajos con ⌘ del original pasan a Ctrl.
- Sin `unwrap()`/`expect()` en rutas que dependan del usuario, la red o el disco.
  Usa `anyhow`/`thiserror` y registra con `tracing`.

## Forma de trabajar
- Haz SOLO la fase que se te pide. No adelantes trabajo de otras fases.
- No reorganices la arquitectura existente sin justificarlo en PORTING.md.
- Antes de usar una API de WebKitGTK, gtk4-rs o libadwaita, comprueba que existe
  en la documentación de la versión que usamos. No inventes APIs. Si algo no es
  posible, dilo y documéntalo en PORTING.md.
- Antes de dar una fase por terminada, todo esto debe pasar:
  `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test` y `cargo build --release`.
- Al terminar: actualiza PORTING.md (columna de estado) y PROGRESO.md, haz commit
  con el mensaje `Fase N: <resumen>` y detente con un resumen para el usuario:
  qué se hizo, cómo probarlo y qué quedó pendiente.

## Licencias
El original es MIT: conserva el aviso de copyright en LICENSE y da crédito en el
README. El código de terceros que se incluya (p. ej. Readability.js, Apache-2.0)
va en `third_party/` con su licencia.
