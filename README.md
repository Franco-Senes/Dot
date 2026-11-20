# Dot

Navegador web de escritorio minimalista, rápido y privado para Linux, escrito íntegramente en Rust sobre **GTK4**, **libadwaita** y **CEF (`cef-rs` / Chromium 156)**.

## Créditos y licencia

**Based on Search by Office Commun (MIT)** — adaptación nativa a Linux basada en **[Search](https://github.com/driceroland/Search)**, creado originalmente para macOS por **Office Commun** (`Copyright (c) 2026 Office Commun`).

Se distribuye bajo la misma licencia **MIT** del proyecto original (véase [`LICENSE`](./LICENSE)).

## Principios

1. **Privacidad sin concesiones:** sin telemetría, sin cuentas, sin sincronización, sin llamadas automáticas a servicios de Google (Safe Browsing, Translate, Optimization Guide, Component Updater y Crashpad permanecen desactivados), **DuckDuckGo** por defecto, bloqueo de cookies de terceros, cabecera `Sec-GPC: 1` (`navigator.globalPrivacyControl = true`) y limpieza automática de identificadores de clic (`gclid`, `fbclid`, `msclkid`, etc.).
2. **Minimalismo radical:** al abrirse solo aparece la ventana con una `AdwHeaderBar` limpia, el campo único centrado de dirección/búsqueda y la vista web. Sin botones de casa, recarga, extensiones ni menú hamburguesa en la barra principal.
3. **Nativo en Linux:** integración directa con Wayland y X11, esquema claro/oscuro sincronizado con el escritorio (`AdwStyleManager`), directorios XDG (`.../dot`) con permisos `0700` y migración única desde `.../search`, escritura atómica de datos y esquema `GSettings` (`dev.senes.Dot`).
4. **Seguridad estricta:** **Site Isolation (`site-per-process`)** permanece activo en todo momento; las páginas con errores TLS se bloquean por defecto mostrando una advertencia clara; las URLs mostradas en la interfaz sanean credenciales `user:pass@` y previenen ataques homógrafos IDN/Punycode.

## Motor CEF y configuración centralizada (`chromium_flags.rs`)

La renderización web corre sobre los binarios precompilados oficiales de **CEF `156.0.30` (Chromium `156.0.8078.96`)** mediante `cef-rs` (`156.6.0`) en modo **Windowless Off-Screen Rendering (`CEF_RUNTIME_STYLE_ALLOY`)**, pintando fotogramas BGRA8 sobre un `GtkPicture` con `GdkMemoryTexture` e integrando el bucle de eventos de CEF en el bucle principal de GLib sin mostrar interfaz alguna de Chrome.

Todos los switches de línea de comandos, `--enable-features`, `--disable-features` y preferencias del `RequestContext` viven en un único módulo centralizado y documentado:

- [`crates/dot-core/src/chromium_flags.rs`](./crates/dot-core/src/chromium_flags.rs)

Para probar o ajustar flags en tiempo de ejecución sin recompilar, puede definirse la variable de entorno `DOT_CHROMIUM_FLAGS` (con respaldo retrocompatible obsoleto para `SEARCH_CHROMIUM_FLAGS`):

```bash
# Ejemplo: limitar subprocesos renderer a 4, activar --process-per-site y probar una feature
DOT_CHROMIUM_FLAGS="--renderer-process-limit=4 --process-per-site +feature:BackForwardCache" ./target/release/dot
```

Sintaxis admitida en `DOT_CHROMIUM_FLAGS`:
- `--switch` o `--switch=valor`: añade o sustituye un switch.
- `!--switch` o `--no-switch:nombre`: elimina un switch no prohibido.
- `+feature:Nombre`: habilita una `base::Feature` en `--enable-features`.
- `-feature:Nombre`: deshabilita una `base::Feature` en `--disable-features` (cualquier intento de desactivar **Site Isolation** se rechaza automáticamente).

### Flags y features activos en Chromium 156 (`chromium_flags.rs`)

- **Switches de privacidad y minimalismo (`BASE_PRIVACY_SWITCHES`):** `--disable-background-networking`, `--disable-component-update`, `--disable-sync`, `--disable-default-apps`, `--disable-domain-reliability`, `--disable-breakpad`, `--no-pings`, `--no-first-run`, `--no-default-browser-check`, `--use-mock-keychain`, `--password-store=basic`, `--no-startup-window`, `--noerrdialogs`, `--hide-crash-restore-bubble`, `--disable-session-crashed-bubble`.
- **Features desactivadas (`BASE_DISABLED_FEATURES`):** `Translate`, `OptimizationHints`, `OptimizationTargetPrediction`, `MediaRouter`, `DialMediaRouteProvider`, `AutofillServerCommunication`, `SafeBrowsing`, `PasswordManager`.
- **Rendimiento y vídeo por hardware (configurables en `dot://settings`):** `--renderer-process-limit=<N>` (por defecto `6`), `--process-per-site` (opcional), `--js-flags=--lite-mode` (opcional), `performance_tuning.high_efficiency_mode.state` (ahorro de memoria) y VA-API Linux (`AcceleratedVideoDecodeLinuxGL`, `AcceleratedVideoDecodeLinuxZeroCopyGL`, `VaapiIgnoreDriverChecks`).
- **Seguridad inmutable:** `SitePerProcess` / `IsolateOrigins` (**Site Isolation**) permanece siempre activo y protegido contra overrides.

## Extensiones de Chrome (estado actual — Fase 9 Paso 2B)

Las extensiones de Chrome (Manifest V3) **no están disponibles para su ejecución todavía** en Dot:
- El renderizado windowless OSR de CEF 156 utiliza obligatoriamente `CEF_RUNTIME_STYLE_ALLOY` (`cef_types_runtime.h`), basado en la capa `//content` de Chromium, y la antigua API `CefExtension` fue eliminada en CEF 128+. Ejecutar extensiones MV3 (`chrome.runtime`, service workers) en Chromium 156 requiere `CEF_RUNTIME_STYLE_CHROME`, lo que activaría la interfaz propia de Chrome prohibida por los principios de diseño del navegador.
- Siguiendo el **Paso 2B**, Dot incluye el verificador completo de paquetes **CRX3** (`crates/dot-core/src/crx.rs`: descarga CWS, parseo Protobuf, verificación RSA/ECDSA e ID criptográfico y extracción ZIP segura sin *path traversal*) y el almacén local `$XDG_DATA_HOME/dot/extensions/installed.json` (`crates/dot-core/src/extensions.rs`), ocultos detrás del ajuste experimental desactivado por defecto `extensions_enabled` (`extensions.experimental` en `settings.json` y `dot://settings` → *Rendimiento y motor*).

## Actualizaciones

- **Flatpak:** cuando existe `/.flatpak-info`, el comprobador integrado se desactiva automáticamente; las actualizaciones llegan a través de Flathub / GNOME Software.
- **Instalación independiente (tarball / AppImage):** `crates/dot-core/src/updater.rs` verifica firmas **Ed25519** del manifiesto y hashes **SHA-256** antes de mostrar avisos, y nunca descarga ni instala sin confirmación explícita del usuario.

## Estructura del workspace Cargo

| Crate | Descripción |
|---|---|
| `crates/dot-core` | Lógica pura en Rust (sin GTK4 ni CEF): `chromium_flags`, `paths` XDG + migración única + `write_atomic`, resolución de direcciones/búsqueda/`dot://` (con alias `plasma://` y `search://`)/anti-homógrafos (`address`), variables `DOT_*` (`env`), ajustes JSON (`settings`), coincidencia difusa (`fuzzy`), sesiones y pila de cerradas (`session`), carpetas de pines estilo Arc (`pin_folders`), historial con frecencia e índice rápido `< 16 ms` (`history`), árbol de marcadores e import/export HTML Netscape (`bookmarks`), descargas sin sobrescribir (`downloads`) y trazas (`trace`). |
| `crates/gnome-platform` | Envoltorio seguro e idiomático en Rust sobre las bibliotecas C del SDK de GNOME (`gtk-4`, `libadwaita-1`, `gio-2.0`, `gobject-2.0`, `glib-2.0`), incluyendo `AdwTabView`/`AdwTabBar`, `AdwOverlaySplitView`, `AdwPreferencesGroup`/`SpinRow`, `AdwShortcutsDialog`, `GtkStack`, `GtkScale`, `GtkPopover`, `GtkFileDialog`, `GtkFileLauncher`, `GNotification`, `GtkDragSource`/`GtkDropTarget` y `GdkTexture`/`GdkMemoryTexture` como `GIcon`. |
| `crates/dot` | Ejecutable principal `dot` (`dev.senes.Dot`): aplicación `AdwApplication` multi-ventana (normales y privadas), iconos vectoriales propios sin dependencias de tema (`icons.rs`), favicons CEF vía `MemoryTexture` con respaldo `/favicon.ico` y monogramas, pestañas arriba o barra lateral estilo Arc (`Ctrl+Shift+S`, cuadrícula de Favoritos, carpetas plegables hasta 3 niveles y DnD), página nativa completa `dot://settings` (`Ctrl+,`, 8 secciones con búsqueda en vivo y 0 procesos CEF), selector difuso `Ctrl+K`, omnibox con autocompletado inline y sugerencias, paneles laterales discretos de Historial (`Ctrl+H`), Marcadores (`Ctrl+Shift+O`, popover `Ctrl+D`) y Descargas (`Ctrl+Shift+Y`), atajos (`Ctrl+?`), sesión diferida (`session.json`), búsqueda en página (`Ctrl+F`), motor `cef_engine` y vista `BrowserView`. |
| `crates/dot-bench` | Herramientas CLI de pruebas e inspección (`bench` y `test-find`). |
| `crates/xtask` | Tareas de construcción e instalación (`compile-schemas` e `install`). |

## Contraseñas (Vault)

- Las contraseñas se guardan **solo** en el Secret Service del escritorio (GNOME Keyring, KWallet o el portal de secretos de Flatpak) mediante `libsecret`. Dot nunca las escribe en disco ni en los logs; si no hay Secret Service, Vault se desactiva y lo explica (no hay alternativa en texto plano).
- Se ofrecen únicamente en el **mismo origin exacto** (esquema + host + puerto), en el marco principal o en iframes del mismo origin, y nunca de HTTPS a HTTP. En páginas HTTP sin cifrar se avisa antes.
- Panel de contraseñas con `Ctrl+Alt+L`: búsqueda, mostrar (con confirmación), copiar (el portapapeles se borra a los 30 s), editar, borrar, exportar a CSV (con advertencia) e importar desde Firefox, Chromium/Chrome/Brave/Edge/Vivaldi o un CSV, siempre a petición del usuario.
- Modelo de amenazas y detalles criptográficos verificados: [`PORTING.md` §14](./PORTING.md#14-seguridad-de-vault-modelo-de-amenazas).

## Dependencias del sistema

- Rust `1.80+` (`cargo`, `rustc`)
- GTK 4 (`>= 4.14`) y `libadwaita-1` (`>= 1.5`)
- GLib / GIO (`glib-compile-schemas`) y `desktop-file-utils` (`desktop-file-validate`)
- `libsecret-1` (Vault), `libsqlite3` y `libcrypto` de OpenSSL 3 (importación de contraseñas); todas incluidas en `org.gnome.Platform//51`
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
./.toolchain/sdk desktop-file-validate data/dev.senes.Dot.desktop
./.toolchain/sdk appstreamcli validate --no-net data/dev.senes.Dot.metainfo.xml

# Compilar en modo release
./.toolchain/sdk cargo build --workspace --release

# Ejecutar pruebas de humo headless (Xvfb + D-Bus + CEF OSR y página nativa dot://settings)
./.toolchain/sdk-xvfb ./target/release/dot --smoke-test dot://settings
./.toolchain/sdk-xvfb ./target/release/dot --smoke-test "data:text/html,<title>Hola Dot</title><h1>Hola CEF</h1>"

# Verificar persistencia y restauración diferida de 15 pestañas (2 fijadas, solo 1 CEF activo)
DOT_PROFILE_DIR=.toolchain/tmp/smoke-session ./.toolchain/sdk-xvfb ./target/release/dot --smoke-test-session-create
DOT_PROFILE_DIR=.toolchain/tmp/smoke-session ./.toolchain/sdk-xvfb ./target/release/dot --smoke-test-session-restore
rm -rf .toolchain/tmp/smoke-session

# Ejecutar el navegador en sesión gráfica Wayland o X11
./.toolchain/sdk ./target/release/dot
```
