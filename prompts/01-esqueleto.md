# FASE 1 — Esqueleto de la app (modelo potente)

Eres un ingeniero senior de Rust y GNOME que porta el navegador "Search" de macOS
a Linux (Rust + GTK4 + libadwaita + WebKitGTK 6.0).

## Lee primero
`AGENTS.md`, `PLAN.md`, `PORTING.md`, `PROGRESO.md` y `git log --oneline`.
Tómate en serio las decisiones y correcciones de PORTING.md: mandan sobre PLAN.md.
Para el comportamiento esperado, consulta el código de `upstream/Sources/Search/`.

Haz SOLO la Fase 1.

## Objetivo
Una app que abre una ventana, muestra una página web y navega. Es la base sobre
la que se construye todo lo demás, así que la arquitectura importa más que la
cantidad de funciones.

## Tareas
1. **Proyecto**
   - `Cargo.toml` con gtk4, libadwaita, webkit6, glib, gio, serde, serde_json,
     anyhow, thiserror, tracing, tracing-subscriber, url (versiones estables
     actuales, con las features de versión que necesite libadwaita).
   - `meson.build` + `src/meson.build` que compilen con Cargo e instalen
     `data/` (por ahora: .desktop básico y esquema GSettings).
   - ID de la app: el de PORTING.md (por defecto `io.github.searchlinux.Search`).
2. **Módulos** (crea todos los que indique PORTING.md, aunque algunos queden
   vacíos con un `// TODO(Fase N)`): como mínimo `main.rs`, `app.rs`,
   `window.rs`, `browser_view.rs`, `address.rs`, `settings.rs`, `paths.rs`,
   `actions.rs`.
3. **paths.rs**: rutas XDG de datos, config, caché y runtime bajo `search/`,
   creadas al arrancar con permisos 0700. Utilidad `write_atomic(path, bytes)`.
4. **Aplicación**: AdwApplication con `AdwStyleManager` siguiendo el tema del
   sistema (claro/oscuro). Ajustes con GSettings.
5. **Ventana**: AdwApplicationWindow con decoraciones del lado del cliente y un
   AdwHeaderBar mínimo que contenga solo el campo de dirección/búsqueda
   (sin botones extra, como el original). La ventana se arrastra desde la
   barra de cabecera.
6. **Motor**: un único WebKitNetworkSession persistente con data_dir y cache_dir
   en las rutas XDG. Política de cookies igual a la del original (consúltalo).
   Desactiva todo lo que envíe datos sin que el usuario lo pida.
7. **browser_view.rs**: un tipo `BrowserView` que envuelva WebKitWebView y
   exponga título, URL, progreso, favicon, can_go_back/forward y señales.
   Pensado para que en la Fase 2 haya muchos (nada de un WebView global).
8. **Campo de dirección** (`address.rs`):
   - Si es una URL (con o sin esquema, `localhost`, IP, dominio con TLD) →
     navega (añade https:// si falta).
   - Si no → busca con el motor configurable (por defecto DuckDuckGo; el mismo
     por defecto que el original si es otro). Tests unitarios de la detección.
   - Muestra la URL de la página actual; al enfocar, selecciona todo; Esc
     restaura la URL.
9. **Atajos** (Ctrl en vez de ⌘): Ctrl+L / F6 / Alt+D enfoca el campo,
   Alt+← / Alt+→ atrás/adelante, Ctrl+R / F5 recargar, Ctrl+Shift+R sin caché,
   Esc detener, Ctrl+Q salir, Ctrl+= / Ctrl+- / Ctrl+0 zoom, Ctrl+F buscar en la
   página (barra de búsqueda simple).
10. **Estado**: título de la ventana = título de la página; barra de progreso
    fina mientras carga; página de error propia y sobria (sin red, DNS,
    certificado no válido con opción explícita de continuar).
11. **Logs**: `tracing` con nivel por variable de entorno `SEARCH_LOG`.

## Criterios de cierre
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`
  y `cargo build --release` pasan.
- `cargo run` abre la ventana, navega a URLs y busca texto, en Wayland y X11
  (`GDK_BACKEND=x11 cargo run`).
- README.md con: qué es, créditos al original (MIT), dependencias y cómo compilar.
- PORTING.md y PROGRESO.md actualizados.
- Commit: `Fase 1: esqueleto de la app`.

Detente y dame un resumen: qué hiciste, cómo probarlo y qué decisiones de
arquitectura tomaste para las pestañas de la Fase 2.
