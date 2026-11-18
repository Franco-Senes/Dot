# Search para Linux

Navegador web de escritorio minimalista, rápido y privado para Linux, escrito íntegramente en Rust sobre **GTK4**, **libadwaita** y **CEF (`cef-rs` / Chromium 156)**.

## Créditos y licencia

Este proyecto es una adaptación nativa a Linux de **[Search](https://github.com/driceroland/Search)**, creado originalmente para macOS por **Office Commun** (`driceroland/Search`).

Se distribuye bajo la misma licencia **MIT** del proyecto original (véase [`LICENSE`](./LICENSE)).

## Principios

1. **Privacidad sin concesiones:** sin telemetría, sin cuentas, sin sincronización, sin llamadas automáticas a servicios de Google (Safe Browsing, Translate, Optimization Guide, Component Updater y Crashpad permanecen desactivados), **DuckDuckGo** por defecto, bloqueo de cookies de terceros, cabecera `Sec-GPC: 1` (`navigator.globalPrivacyControl = true`) y limpieza automática de identificadores de clic (`gclid`, `fbclid`, `msclkid`, etc.).
2. **Minimalismo radical:** al abrirse solo aparece la ventana con una `AdwHeaderBar` limpia, el campo único centrado de dirección/búsqueda y la vista web. Sin botones de casa, recarga, extensiones ni menú hamburguesa en la barra principal.
3. **Nativo en Linux:** integración directa con Wayland y X11, esquema claro/oscuro sincronizado con el escritorio (`AdwStyleManager`), directorios XDG con permisos `0700`, escritura atómica de datos y esquema `GSettings` (`io.github.searchlinux.Search`).
4. **Seguridad estricta:** **Site Isolation (`site-per-process`)** permanece activo en todo momento; las páginas con errores TLS se bloquean por defecto mostrando una advertencia clara; las URLs mostradas en la interfaz sanean credenciales `user:pass@` y previenen ataques homógrafos IDN/Punycode.

## Motor CEF y configuración centralizada (`chromium_flags.rs`)

La renderización web corre sobre los binarios precompilados oficiales de **CEF `156.0.30` (Chromium `156.0.8078.96`)** mediante `cef-rs` (`156.6.0`) en modo **Windowless Off-Screen Rendering (`CEF_RUNTIME_STYLE_ALLOY`)**, pintando fotogramas BGRA8 sobre un `GtkPicture` con `GdkMemoryTexture` e integrando el bucle de eventos de CEF en el bucle principal de GLib sin mostrar interfaz alguna de Chrome.

Todos los switches de línea de comandos, `--enable-features`, `--disable-features` y preferencias del `RequestContext` viven en un único módulo centralizado y documentado:

- [`crates/search-core/src/chromium_flags.rs`](./crates/search-core/src/chromium_flags.rs)

Para probar o ajustar flags en tiempo de ejecución sin recompilar, puede definirse la variable de entorno `SEARCH_CHROMIUM_FLAGS`:

```bash
# Ejemplo: limitar subprocesos renderer a 4, activar --process-per-site y probar una feature
SEARCH_CHROMIUM_FLAGS="--renderer-process-limit=4 --process-per-site +feature:BackForwardCache" ./target/release/search
```

Sintaxis admitida en `SEARCH_CHROMIUM_FLAGS`:
- `--switch` o `--switch=valor`: añade o sustituye un switch.
- `!--switch` o `--no-switch:nombre`: elimina un switch no prohibido.
- `+feature:Nombre`: habilita una `base::Feature` en `--enable-features`.
- `-feature:Nombre`: deshabilita una `base::Feature` en `--disable-features` (cualquier intento de desactivar **Site Isolation** se rechaza automáticamente).

## Estructura del workspace Cargo

| Crate | Descripción |
|---|---|
| `crates/search-core` | Lógica pura en Rust (sin GTK4 ni CEF): `chromium_flags`, `paths` XDG + `write_atomic`, resolución de direcciones/búsqueda/anti-homógrafos (`address`), ajustes JSON (`settings`) y trazas (`trace`). |
| `crates/gnome-platform` | Envoltorio seguro e idiomático en Rust sobre las bibliotecas C del SDK de GNOME (`gtk-4`, `libadwaita-1`, `gio-2.0`, `gobject-2.0`, `glib-2.0`). |
| `crates/search` | Ejecutable principal `search`: aplicación `AdwApplication`, ventana minimalista, barra de direcciones, búsqueda en página (`Ctrl+F`), motor `cef_engine` y vista reutilizable `BrowserView`. |
| `crates/search-bench` | Herramientas CLI de pruebas e inspección (`bench` y `test-find`). |
| `crates/xtask` | Tareas de construcción e instalación (`compile-schemas` e `install`). |

## Dependencias del sistema

- Rust `1.80+` (`cargo`, `rustc`)
- GTK 4 (`>= 4.14`) y `libadwaita-1` (`>= 1.5`)
- GLib / GIO (`glib-compile-schemas`) y `desktop-file-utils` (`desktop-file-validate`)
- Binario precompilado de CEF `156` (`CEF_PATH` apuntando al directorio con `libcef.so`, `icudtl.dat`, `resources.pak` y `locales/`)

En este repositorio el entorno autónomo ya incluye el SDK de GNOME 51, Rust 1.98 y CEF 156 bajo `.toolchain/`, accesibles mediante `.toolchain/sdk` y `.toolchain/sdk-xvfb`.

## Compilación, pruebas y ejecución

```bash
# Verificar formato y lints estrictos
./.toolchain/sdk cargo fmt --all -- --check
./.toolchain/sdk cargo clippy --workspace --all-targets -- -D warnings

# Ejecutar tests unitarios del workspace
./.toolchain/sdk cargo test --workspace

# Compilar esquemas GSettings y validar archivo .desktop
./.toolchain/sdk cargo run -p xtask -- compile-schemas
./.toolchain/sdk desktop-file-validate data/io.github.searchlinux.Search.desktop

# Compilar en modo release
./.toolchain/sdk cargo build --workspace --release

# Ejecutar prueba de humo headless (Xvfb + D-Bus + CEF OSR)
./.toolchain/sdk-xvfb ./target/release/search --smoke-test "data:text/html,<title>Hola Search</title><h1>Hola CEF</h1>"

# Ejecutar el navegador en sesión gráfica Wayland o X11
./.toolchain/sdk ./target/release/search
```
