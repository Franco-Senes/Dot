# PORTING.md — Search para Linux

> Documento vivo. Lo crea la Fase 0, lo amplía la Fase 0b (cambio de motor a CEF) y lo actualiza cada fase.
> Si algo aquí contradice a PLAN.md, manda lo que dice aquí.

## 1. Versiones verificadas
| Componente | Versión | Fuente (enlace) | Fecha de la consulta |
|---|---|---|---|
| Crate `cef` (`tauri-apps/cef-rs`) | `156.6.0+156.0.30` | https://crates.io/crates/cef | 2026-11-18 |
| Binario precompilado CEF / Chromium | `156.0.30+g6028375+chromium-156.0.8078.96` (`linux64_minimal`) | `cef_version.h` en `.toolchain/cef/include/` (SpotifyCDN) | 2026-11-18 |
| `gtk4-rs` (`gtk4` / GTK4) | `0.11.5` (GTK `4.24.1` en GNOME SDK 51) | https://crates.io/crates/gtk4 | 2026-11-18 |
| `libadwaita-rs` / `libadwaita` | `0.9.2` / `1.10.0` | https://crates.io/crates/libadwaita | 2026-11-18 |
| `oo7` (Secret Service / Portal) | `0.7.0` | https://crates.io/crates/oo7 | 2026-11-18 |
| Runtime de GNOME (Flatpak) | `org.gnome.Platform//51` (GNOME 51.1) | https://flathub.org (`org.gnome.Sdk//51`) | 2026-11-18 |
| Toolchain Rust (Flatpak SDK) | `rustc 1.98.0` / `cargo 1.98.0` | `org.freedesktop.Sdk.Extension.rust-stable//26.08` | 2026-11-18 |
| Hardware de referencia local | AMD Ryzen 5 5600G with Radeon Graphics (12 hilos, `/dev/dri/renderD128`) | `lscpu` + `/dev/dri` | 2026-11-18 |

## 2. Verificaciones de CEF, interfaz, códecs, DRM y flags de Chromium

### 2.1. Verificaciones clave de CEF 156 (`cef-rs`)
| Pregunta | Respuesta verificada | Fuente |
|---|---|---|
| ¿Cómo se integra CEF con GTK4/libadwaita en Wayland y X11 sin mostrar UI de Chrome? | **Renderizado Windowless Off-Screen (`CEF_RUNTIME_STYLE_ALLOY` + `windowless_rendering_enabled = 1`)** con `external_message_pump = 1` integrado al bucle de GLib (`on_schedule_message_pump_work` + latido de respaldo). `RenderHandler::on_paint` entrega buffers BGRA8 pintados en un `gtk4::Picture` mediante `gdk4::MemoryTexture` (`B8g8r8a8Premultiplied`), y `GtkEventControllerMotion` / `GestureClick` / `EventControllerScroll` / `EventControllerKey` / `EventControllerFocus` reenvían la entrada a `BrowserHost`. Funciona de forma idéntica en **Wayland y X11**, sin ventanas X11 hijas ni barras de Chrome, permitiendo pestañas ilimitadas, Split View y PiP. | `cef_types_runtime.h` y `cef-rs/examples/osr` |
| ¿Qué códecs de audio/vídeo y soporte DRM (Widevine) trae el CEF precompilado? | **Códecs abiertos incluidos de serie:** VP8, VP9 (`vp09.00`), AV1 (`av01.0`), Opus, Vorbis, FLAC, PCM, WebM y Ogg (YouTube 1080p/4K sirve VP9 y AV1 nativamente). **Códecs propietarios (H.264/AAC/HEVC):** Ausentes en el binario oficial público de SpotifyCDN por licencias de patentes (al no compilar nuestro propio CEF). **DRM Widevine:** `libcef.so` incluye el adaptador `WidevineCdm` capaz de cargar un `libwidevinecdm.so` del usuario si está instalado localmente, aunque el binario propietario de Google no puede redistribuirse. | Inspección binaria de `.toolchain/cef/libcef.so` |
| ¿Soporta CEF 156 extensiones de Chrome (Manifest V3) en modo Alloy/OSR? | En CEF 128+ fue eliminada la antigua API `CefExtension` (`include/*extension*` ya no existe). El flag `--load-extension` existe en `libcef.so`, pero opera sobre la capa `CEF_RUNTIME_STYLE_CHROME`, mientras que `windowless_rendering_enabled` utiliza siempre `CEF_RUNTIME_STYLE_ALLOY` (`cef_types_runtime.h:49`: *"Windowless rendering will always use Alloy style"*). En Fase 9 se verificará empíricamente y, si Alloy no carga MV3 sin activar la UI de Chrome, se aplicará el **Paso 2B** (`crx.rs` completo + UI experimental). | `cef_types_runtime.h` e inspección de `libcef.so` |
| ¿Picture-in-Picture en CEF + GTK4? | En modo Alloy/OSR se implementa igual que `Float.swift` del original (`float.style = "search"`): ventana flotante `gtk4::Window` sin decoraciones con relación 16:9 y controles propios de reproducir/pausa/cerrar. En Wayland se documenta que los clientes estándar no pueden forzar *always-on-top*. | `upstream/Sources/Search/Float.swift` |
| ¿Cómo se traduce el JSON de `WKContentRuleList` (Shield) a CEF? | `crates/search-core/src/shield.rs` compila las 57 reglas JSON originales (`Shield.swift` + `YouTubeAds.swift`) en memoria/caché (`$XDG_CACHE_HOME/search/filters`) a dos artefactos: (1) reglas de bloqueo de red (`RegexSet` + filtros `third-party` / `if-domain` / `unless-domain`) evaluadas en `CefResourceRequestHandler::on_before_resource_load` antes de salir a la red, y (2) hojas CSS cosméticas (`css-display-none`) inyectadas en `on_load_start`. | `cef_resource_request_handler.h` |

### 2.2. Traducción arquitectónica WebKitGTK → CEF (`cef-rs`)
| Pieza en los prompts originales (WebKitGTK) | Equivalente adoptado en CEF (`cef-rs` 156.6) |
|---|---|
| `WebKitWebView` | `BrowserView` en Rust envolviendo `cef::Browser` + lienzo OSR `gtk4::Picture` (`gdk4::MemoryTexture`) |
| `WebKitNetworkSession` persistente / efímero | `cef::RequestContext` con `cache_path = $XDG_CACHE_HOME/search/cef` (persistente) vs `cache_path = ""` en memoria (ventanas privadas, sin tocar disco) |
| Política de cookies (`NoThirdParty` / ITP) | Preferencias de `RequestContext` + filtrado de cookies en `CefCookieAccessFilter` (`can_send_cookie` / `can_save_cookie` bloqueando cookies cross-site cuando `!sites.keep`) + GPC |
| `WebKitUserContentFilterStore` (Shield) | Motor Rust `search_core::shield::CompiledShield` en `CefResourceRequestHandler::on_before_resource_load` (`ReturnValue::CANCEL`) + CSS cosmético en `on_load_start` |
| `WebKitUserStyleSheet` + `WebKitUserScript` + message handler (Curtain, Vault, Reader) | Inyección JS/CSS en `LoadHandler::on_load_start` / `on_load_end` (`Frame::execute_java_script`) + puente IPC autenticado JS→Rust mediante `console.debug("__search_ipc_" + token + ":" + json)` interceptado en `DisplayHandler::on_console_message` (suprimido del log y validado con un token aleatorio de 128 bits por instancia para impedir falsificación por páginas web) |
| `WebKitDownload` | `CefDownloadHandler` (`on_before_download` asignando ruta única sin sobrescribir `archivo (1).pdf` o `GtkFileDialog`, y `on_download_updated` con progreso, velocidad y cancelación) |
| `WebKitFindController` (`Ctrl+F`) | `BrowserHost::find` + `BrowserHost::stop_finding` + `CefFindHandler::on_find_result` |
| `WebKitFaviconDatabase` | `CefDisplayHandler::on_favicon_urlchange` + descarga de icono en memoria/caché |
| Páginas de error y errores TLS | `CefLoadHandler::on_load_error` (HTML sobrio propio) + `CefRequestHandler::on_certificate_error` (bloqueo por defecto con opción explícita de continuar para ese host) |
| `target=_blank` / `window.open` | `CefLifeSpanHandler::on_before_popup` redirigiendo popups legítimos a una pestaña nueva en `AdwTabView` |

### 2.3. Auditoría de flags, features y preferencias en Chromium `156.0.8078.96` (`chromium_flags.rs`)
Todos los candidatos se comprobaron a nivel binario en `.toolchain/cef/libcef.so`:

| Flag / Feature / Preferencia | ¿Existe en Chromium 156? | Acción en `chromium_flags.rs` |
|---|---|---|
| `--disable-background-networking` | ✅ Sí (`EXACT_CSTR`) | Activo por defecto: corta peticiones automáticas en segundo plano. |
| `--disable-component-update` | ✅ Sí (`EXACT_CSTR`) | Activo por defecto: desactiva descargas del actualizador de componentes de Google. |
| `--disable-sync` | ✅ Sí (`disable-sync`) | Activo por defecto: desactiva Chrome Sync. |
| `--disable-default-apps` | ✅ Sí (`EXACT_CSTR`) | Activo por defecto: no instala apps predeterminadas. |
| `--disable-domain-reliability` | ✅ Sí (`EXACT_CSTR`) | Activo por defecto: impide envío de telemetría de fallos de red a Google. |
| `--disable-breakpad` | ✅ Sí (`EXACT_CSTR`) | Activo por defecto: desactiva subida de informes de caídas. |
| `--no-pings` | ✅ Sí (`EXACT_CSTR`) | Activo por defecto: bloquea auditoría `<a ping>` de hipervínculos. |
| `--autoplay-policy=user-gesture-required` (opc.) | ✅ Sí (`EXACT_CSTR`) | Disponible según ajuste de reproducción automática. |
| `--disable-client-side-phishing-detection` | ❌ Eliminado en Chr 156 | **Descartado** (cubierto por `SafeBrowsing` desactivado). |
| `--safebrowsing-disable-auto-update` | ❌ Eliminado en Chr 156 | **Descartado** (cubierto por `SafeBrowsing` + pref `safebrowsing.enabled=false`). |
| `--ozone-platform-hint` | ❌ Eliminado en Chr 156 | **Descartado**; sustituido por `--ozone-platform=wayland|x11|headless` sincronizado automáticamente con el display real de GTK4. |
| Features a desactivar: `Translate,OptimizationHints,OptimizationTargetPrediction,MediaRouter,DialMediaRouteProvider,AutofillServerCommunication,SafeBrowsing,PasswordManager` | ✅ Todas verificadas en `libcef.so` | Activas en `--disable-features=...` para cortar servicios de Google y gestor nativo de contraseñas. |
| Feature `OptimizationGuideModelDownloading` | ❌ Renombrada/ausente en Chr 156 | **Descartada** (sustituida por `OptimizationHints` + `OptimizationTargetPrediction`). |
| Preferencias `RequestContext`: `safebrowsing.enabled=false`, `credentials_enable_service=false`, `autofill.profile_enabled=false`, `autofill.credit_card_enabled=false`, `translate.enabled=false` | ✅ Todas `EXACT_CSTR` (`profile.password_manager_enabled` ya no existe) | Aplicadas al inicializar el `RequestContext` persistente y efímero. |
| `--renderer-process-limit` (`4`, `6`, `8`), `--process-per-site`, `--js-flags=--lite-mode` (`lite_mode` en V8) | ✅ Todos verificados en `libcef.so` | Se prueban y **miden uno por uno** en carga/RAM (20 pestañas y sitios reales), manteniendo siempre **Site Isolation intacto**. |
| `HighEfficiencyModeAvailable` (feature) vs `performance_tuning.high_efficiency_mode.state` (pref) | ❌ La feature ya se graduó; ✅ existe la preferencia exacta | Se controla y mide mediante la preferencia `performance_tuning.high_efficiency_mode.state` en `RequestContext`. |
| Feature `VaapiVideoDecoder` vs `AcceleratedVideoDecodeLinuxGL,AcceleratedVideoDecodeLinuxZeroCopyGL,VaapiIgnoreDriverChecks` | ❌ `VaapiVideoDecoder` dejó de ser nombre de `base::Feature`; ✅ existen las 3 actuales | En Chromium 156 Linux VA-API se activa con `AcceleratedVideoDecodeLinuxGL,AcceleratedVideoDecodeLinuxZeroCopyGL,VaapiIgnoreDriverChecks`; se medirá en Radeon Vega (Ryzen 5 5600G). |

## 3. Mapa de archivos (los 171 archivos de `upstream/Sources/Search/` + `chromium_flags.rs`)

> Organización en el Cargo workspace (100 % Rust):
> - `crates/search-core/src/…`: lógica pura sin GTK/CEF (`chromium_flags.rs`, `paths.rs`, `address.rs`, `settings.rs`, `session.rs`, `history.rs`, `bookmarks.rs`, `downloads.rs`, `shield.rs`, `curtain.rs`, `reader.rs`, `vault.rs`, `vault_import.rs`, `updater.rs`, `crx.rs`, `extensions.rs`, `bench_proto.rs`).
> - `crates/search/src/…`: GUI `gtk4` + `libadwaita`, integración `cef` (`cef-rs`), Secret Service (`oo7`) y D-Bus (`zbus`).
> - `crates/search-bench/src/…`: binarios CLI `bench` y `test-find` + tests headless e integración en Rust.
> - `crates/xtask/src/…`: instalación de `.desktop`, metainfo, iconos, esquemas GSettings y bundle CEF.

Estados: `pendiente` · `directo` · `adaptado` · `diferido` · `imposible` · `hecho`

| Archivo Swift (upstream) | Qué hace | Módulo Rust | API Linux (GTK4 / Adw / CEF) | Estado | Fase |
|---|---|---|---|---|---|
| *(Nuevo requisito Linux)* | Flags, features y preferencias de Chromium 156 (`SEARCH_CHROMIUM_FLAGS`) | `crates/search-core/src/chromium_flags.rs` + `crates/search/src/cef_engine.rs` | `cef::CommandLine`, `cef::Settings`, `cef::RequestContext::set_preference` | hecho | 1 |
| `Sources/Search/Accounts.swift` | Popover bajo campos de login para autocompletar cuentas del llavero | `crates/search/src/vault_ui.rs` | `gtk4::Popover`, `gtk4::ListBox`, `oo7` | adaptado | 5 |
| `Sources/Search/AddressCommands.swift` | Comandos rápidos opcionales en la barra de direcciones | `crates/search-core/src/address.rs` | Rust puro + `gio::ActionGroup` | diferido | — |
| `Sources/Search/Address.swift` | Detecta URL vs archivo local vs consulta, añade esquema y limpia URLs | `crates/search-core/src/address.rs` | `url::Url`, `std::net::IpAddr` | hecho | 1 |
| `Sources/Search/AgentBanner.swift` | Píldora de estado cuando un agente MCP controla pestañas | — | Fuera de alcance (`search-mcp`) | diferido | — |
| `Sources/Search/AgentControl.swift` | Servidor socket para agentes MCP (`search-mcp`) | — | Fuera de alcance (`search-mcp`) | diferido | — |
| `Sources/Search/AgentTrust.swift` | Verificación de firma Apple (`SecCode`) del proceso agente | — | `SecCode` exclusivo de macOS | imposible | — |
| `Sources/Search/AIAssist.swift` | Panel de resumen y preguntas sobre la página con IA (`AI.shipped=false`) | — | Retenido en upstream; sin peticiones no pedidas | diferido | — |
| `Sources/Search/AIClient.swift` | Cliente SSE para proveedores LLM remotos/locales | — | Retenido en upstream (`AI.shipped=false`) | diferido | — |
| `Sources/Search/AIEngine.swift` | Ejecutor Mach-O en sandbox para `search-ai-engine` (`llama.cpp`) | — | Binario Mach-O + `SecCode` exclusivos de macOS | imposible | — |
| `Sources/Search/AIPage.swift` | Extractor de texto de página para prompts de IA | — | Retenido en upstream (`AI.shipped=false`) | diferido | — |
| `Sources/Search/AISignIn.swift` | Flujo OAuth PKCE hacia OpenRouter | — | Prohibido inicio de sesión / cuentas (`AGENTS.md`) | diferido | — |
| `Sources/Search/AI.swift` | Catálogo de modelos IA y bandera `AI.shipped = false` | — | Retenido en upstream | diferido | — |
| `Sources/Search/ApplePasswordsOffer.swift` | Tarjeta para ofrecer integración con Apple Passwords | — | App Contraseñas de Apple exclusiva de macOS | imposible | — |
| `Sources/Search/ApplePasswordsProtocol.swift` | Protocolo SRP-6a + AES-GCM del helper de Apple Passwords | — | Demonio propietario de macOS | imposible | — |
| `Sources/Search/ApplePasswordsSettings.swift` | Ajustes de emparejamiento con Apple Passwords | — | Exclusivo de macOS (`oo7` cubre llaveros Linux) | imposible | — |
| `Sources/Search/ApplePasswords.swift` | Cliente Native Messaging hacia `PasswordManagerBrowserExtensionHelper` | — | Binario `/System/Library/...` inexistente en Linux | imposible | — |
| `Sources/Search/App.swift` | Punto de entrada, subprocesos CEF, aplicación, menús y atajos | `crates/search/src/app.rs` | `cef::execute_process`, `adw::Application`, `adw::StyleManager` | hecho | 1 |
| `Sources/Search/Archive.swift` | Archivo automático de pestañas antiguas (`archive.json`) | `crates/search-core/src/session.rs` | `serde_json` | diferido | — |
| `Sources/Search/AtRestLine.swift` | Presentación del dominio en reposo e indicador HTTP inseguro | `crates/search/src/address.rs` | `gtk4::Box`, `gtk4::Label` | hecho | 1 |
| `Sources/Search/AtRest.swift` | Descomposición de URL y protección anti-homógrafos IDN/Punycode | `crates/search-core/src/address.rs` | `url::Url`, `idna` | hecho | 1 |
| `Sources/Search/AutoScroll.swift` | Desplazamiento continuo con clic central | — | Opcional | diferido | — |
| `Sources/Search/Bench.swift` | Servidor en socket Unix `bench.sock` para pruebas y automatización | `crates/search/src/bench.rs` | `UnixListener` (`0600`, `SO_PEERCRED`), `glib`, `cef::Browser` | adaptado | 7 |
| `Sources/Search/BlockedPopups.swift` | Control de ventanas emergentes bloqueadas por pestaña | `crates/search/src/browser_view.rs` | `cef::LifeSpanHandler::on_before_popup` | hecho | 2 |
| `Sources/Search/BookmarksBar.swift` | Barra horizontal opcional de marcadores | `crates/search/src/bookmarks_ui.rs` | `gtk4::Box`, `gtk4::PopoverMenu` | diferido | 3 |
| `Sources/Search/BookmarksFile.swift` | Importador/exportador HTML Netscape de marcadores | `crates/search-core/src/bookmarks.rs` | Rust puro (`BufRead` / generador HTML) | hecho | 3 |
| `Sources/Search/Bookmarks.swift` | Modelo de árbol `bookmarks.json`, búsqueda y gestión de marcadores | `crates/search-core/src/bookmarks.rs` + `crates/search/src/bookmarks_ui.rs` | `serde_json`, `adw::OverlaySplitView`, `gtk4::Popover` | hecho | 3 |
| `Sources/Search/BrowserPinFolders.swift` | Operaciones sobre carpetas de pestañas fijadas | `crates/search-core/src/session.rs` | Rust puro | diferido | 2 |
| `Sources/Search/Browser.swift` | Coordinador por ventana de pestañas, navegación, privacidad y paneles | `crates/search/src/window.rs` | `adw::ApplicationWindow`, `adw::TabView`, `cef::RequestContext` | hecho | 1 |
| `Sources/Search/CaptureChoices.swift` | Permisos recordados por origen (cámara, micro, ubicación, notif.) | `crates/search-core/src/settings.rs` | `cef::PermissionHandler`, `serde_json` | adaptado | 4 |
| `Sources/Search/CleanLink.swift` | Limpieza de parámetros de rastreo (`utm_*`, `fbclid`, `gclid`) | `crates/search-core/src/address.rs` | `url::Url` | hecho | 1 |
| `Sources/Search/Closing.swift` | Auto-cierre temporizado de pestañas inactivas | — | Secundario respecto a `PLAN.md` | diferido | — |
| `Sources/Search/ColumnMenu.swift` | Menú contextual de la barra lateral de pestañas | `crates/search/src/sidebar_tabs.rs` | `gio::Menu`, `gtk4::PopoverMenu` | hecho | 2 |
| `Sources/Search/Crx.swift` | Descarga, parseo Protobuf, verificación RSA y extracción segura CRX3 | `crates/search-core/src/crx.rs` | `rsa`, `sha2`, `zip`, parser varint en Rust puro | directo | 9 |
| `Sources/Search/Curtain.swift` | Selector visual para ocultar elementos DOM y persistencia por dominio | `crates/search-core/src/curtain.rs` + `crates/search/src/curtain.rs` | `cef::Frame::execute_java_script`, puente IPC, Rust | adaptado | 4 |
| `Sources/Search/Design.swift` | Paleta claro/oscuro, métricas y CSS base | `crates/search/src/style.rs` | `adw::StyleManager`, `gtk4::CssProvider` | hecho | 1 |
| `Sources/Search/Dialogs.swift` | Diálogos JS (`alert`/`confirm`/`prompt`), archivos, HTTP Auth y TLS | `crates/search/src/browser_view.rs` | `cef::JSDialogHandler`, `cef::DialogHandler`, `on_certificate_error` | hecho | 1 |
| `Sources/Search/DownloadsFloat.swift` | Popover/panel discreto de descargas | `crates/search/src/downloads_ui.rs` | `gtk4::Popover` / `adw::OverlaySplitView` | hecho | 3 |
| `Sources/Search/DuskColours.swift` | Motor CSSOM de modo oscuro forzado para sitios claros | — | Función extra no requerida en `PLAN.md` | diferido | — |
| `Sources/Search/Dusk.swift` | Coordinador de oscurecimiento automático por sitio | — | Función extra no requerida en `PLAN.md` | diferido | — |
| `Sources/Search/EngineSuggest.swift` | Sugerencias remotas opcionales del buscador (`off` por defecto) | — | Desactivado por principio de cero peticiones no pedidas | diferido | — |
| `Sources/Search/Engine.swift` | Motores de búsqueda (DuckDuckGo, Google, Brave, Kagi, custom…) | `crates/search-core/src/address.rs` | `url::Url`, codificación RFC 3986 | hecho | 1 |
| `Sources/Search/ExtensionCapture.swift` | Puente `desktopCapture` para extensiones | `crates/search/src/extensions.rs` | Según soporte Alloy/Chrome en Fase 9 | diferido | 9 |
| `Sources/Search/ExtensionExternal.swift` | Puente `externally_connectable` para extensiones | `crates/search/src/extensions.rs` | Según soporte Alloy/Chrome en Fase 9 | diferido | 9 |
| `Sources/Search/ExtensionFetch.swift` | Peticiones CORS/cookies para workers de extensiones | `crates/search/src/extensions.rs` | Según soporte Alloy/Chrome en Fase 9 | diferido | 9 |
| `Sources/Search/ExtensionFiles.swift` | Intercambio atómico de directorios de extensiones | `crates/search-core/src/crx.rs` | `std::fs::rename` / directorios temporales | adaptado | 9 |
| `Sources/Search/ExtensionNative.swift` | Chrome Native Messaging por pipes stdio | `crates/search-core/src/extensions.rs` | Rutas Linux `NativeMessagingHosts` | diferido | 9 |
| `Sources/Search/ExtensionOffscreen.swift` | Documentos `chrome.offscreen` en vista oculta | `crates/search/src/extensions.rs` | Según soporte Alloy/Chrome en Fase 9 | diferido | 9 |
| `Sources/Search/ExtensionPageScript.swift` | Inyección en mundo principal para content scripts | `crates/search/src/extensions.rs` | Según soporte Alloy/Chrome en Fase 9 | diferido | 9 |
| `Sources/Search/ExtensionPopup.swift` | Popover autoajustable para popups de extensiones | `crates/search/src/extensions.rs` | Según soporte Alloy/Chrome en Fase 9 | diferido | 9 |
| `Sources/Search/ExtensionRedirectPolicy.swift` | Validación de `web_accessible_resources` en redirecciones | `crates/search-core/src/extensions.rs` | Rust puro (`url::Url`) | diferido | 9 |
| `Sources/Search/ExtensionReturnNavigation.swift` | Seguimiento de origen para redirecciones a extensiones | `crates/search/src/extensions.rs` | Según soporte Alloy/Chrome en Fase 9 | diferido | 9 |
| `Sources/Search/ExtensionShims.swift` | Shims JS + nativos de APIs `chrome.*` | `crates/search/src/extension_shims.rs` | Evaluación en Fase 9 (CEF Alloy vs Paso 2B) | diferido | 9 |
| `Sources/Search/ExtensionSiteAccess.swift` | Permisos por host (`all`/`some`/`click`) de cada extensión | `crates/search-core/src/extensions.rs` | `serde_json` | diferido | 9 |
| `Sources/Search/ExtensionSocket.swift` | Túnel WebSocket para service workers MV3 | `crates/search/src/extensions.rs` | Según soporte Alloy/Chrome en Fase 9 | diferido | 9 |
| `Sources/Search/Extensions.swift` | Gestor del ciclo de vida de extensiones (`installed.json`) | `crates/search-core/src/extensions.rs` + `crates/search/src/extensions.rs` | Verificación en Fase 9 (`--load-extension` vs Paso 2B) | adaptado | 9 |
| `Sources/Search/ExtensionsUI.swift` | Interfaz de gestión de extensiones | `crates/search/src/extensions.rs` | `adw::PreferencesGroup` | adaptado | 9 |
| `Sources/Search/ExtensionWatch.swift` | Eventos `groups` e `idle` para extensiones | `crates/search/src/extensions.rs` | Según soporte Alloy/Chrome en Fase 9 | diferido | 9 |
| `Sources/Search/ExtensionWorkerRecovery.swift` | Recuperación de workers MV3 en `WKWebExtensionContext` | — | Específico de `WKWebExtensionContext` de Apple | imposible | 9 |
| `Sources/Search/Fetching.swift` | Descargas en curso, progreso, velocidad, cancelar y reintentar | `crates/search/src/downloads.rs` | `cef::DownloadHandler`, `cef::DownloadItem` | hecho | 3 |
| `Sources/Search/Find.swift` | Barra simple de búsqueda en la página (`Ctrl+F`) | `crates/search/src/find_bar.rs` | `cef::BrowserHost::find`, `cef::FindHandler`, `gtk4::SearchEntry` | hecho | 1 |
| `Sources/Search/Float.swift` | Ventana flotante propia sin decoraciones para vídeo (PiP) | `crates/search/src/pip.rs` | `gtk4::Window` (`decorated(false)`), `BrowserView` / controles | adaptado | 6 |
| `Sources/Search/Fold.swift` | Plegado de la barra lateral o superior (`Ctrl+S`) | `crates/search/src/window.rs` | `gtk4::Revealer` / `adw::OverlaySplitView` | hecho | 2 |
| `Sources/Search/Forms.swift` | Detección JS mínima de formularios de login | `crates/search/resources/vault.js` + `crates/search/src/vault.rs` | `cef::Frame::execute_java_script` + puente IPC autenticado | adaptado | 5 |
| `Sources/Search/FrameRate.swift` | Tope privado de 60/120 FPS con `IOKit.ps` | `crates/search/src/browser_view.rs` | `BrowserSettings::windowless_frame_rate` (60 FPS por defecto) | hecho | 1 |
| `Sources/Search/FullScreenEscape.swift` | Salida de pantalla completa con `Escape` | `crates/search/src/window.rs` | `cef::DisplayHandler::on_fullscreen_mode_change` | hecho | 1 |
| `Sources/Search/Fuzzy.swift` | Coincidencia difusa rápida para autocompletado y `Ctrl+K` | `crates/search-core/src/fuzzy.rs` | `memchr` + subsecuencia + Damerau-Levenshtein (`strsim`) en Rust puro | hecho | 2 |
| `Sources/Search/Hidden.swift` | Panel para inspeccionar y borrar reglas de Curtain por sitio o global | `crates/search/src/curtain.rs` | `adw::OverlaySplitView` / `gtk4::Popover`, `gtk4::ListBox` | adaptado | 4 |
| `Sources/Search/History.swift` | Historial `history.json`, puntuación frecencia y autocompletado `<16ms` | `crates/search-core/src/history.rs` + `crates/search/src/history_ui.rs` | Rust puro (`serde_json`, índice en RAM) + `gtk4::ListView` | hecho | 3 |
| `Sources/Search/HotKey.swift` | Atajo global de sistema Carbon (`RegisterEventHotKey`) | — | API Carbon exclusiva de macOS | imposible | — |
| `Sources/Search/Icons.swift` | Obtención y caché de favicons | `crates/search/src/browser_view.rs` | `cef::DisplayHandler::on_favicon_urlchange` + `BrowserHost::download_image` | hecho | 2 |
| `Sources/Search/ImageMenu.swift` | Acciones de menú contextual sobre imágenes | `crates/search/src/browser_view.rs` | `cef::ContextMenuHandler` | adaptado | 2 |
| `Sources/Search/ImportArc.swift` | Importador de `StorableSidebar.json` de Arc | — | Arc no existe en Linux | imposible | — |
| `Sources/Search/ImportFile.swift` | Importación desde archivos `.csv` y `.html` | `crates/search-core/src/vault_import.rs` | Rust puro (`csv`, parser HTML Netscape) | directo | 5 |
| `Sources/Search/ImportPanel.swift` | Diálogo de importación mostrando perfiles detectados y resumen | `crates/search/src/vault_ui.rs` | `adw::Dialog`, `adw::PreferencesGroup`, `gtk4::FileDialog` | adaptado | 5 |
| `Sources/Search/ImportRecord.swift` | Registro de importaciones previas | `crates/search-core/src/vault_import.rs` | `serde_json` | directo | 5 |
| `Sources/Search/ImportSession.swift` | Importador binario `SNSS` de sesiones Chromium | — | Fuera de alcance de Fase 5 (solo contraseñas/marcadores) | diferido | — |
| `Sources/Search/Import.swift` | Descubrimiento y descifrado de contraseñas Firefox (`key4.db`+`logins.json`) y Chromium (`Login Data` `v10`/`v11`) | `crates/search-core/src/vault_import.rs` | `rusqlite`, `aes`, `cbc`, `des`, `pbkdf2`, `sha1`, `sha2`, `oo7` | adaptado | 5 |
| `Sources/Search/Inspector.swift` | Apertura de DevTools (`Ctrl+Shift+I` / `F12`) | `crates/search/src/browser_view.rs` | `cef::BrowserHost::show_dev_tools` / `close_dev_tools` | hecho | 1 |
| `Sources/Search/InternalPages.swift` | Enrutamiento de vistas internas (`search://settings`, etc.) | `crates/search/src/window.rs` | `adw::PreferencesDialog` / paneles `adw::OverlaySplitView` | hecho | 2 |
| `Sources/Search/KeptScroll.swift` | Restauración de scroll en navegación atrás/adelante | `crates/search/src/browser_view.rs` | Nativo en el historial de sesión de Blink/CEF | hecho | 2 |
| `Sources/Search/KeyLayout.swift` | Mapa físico de teclas Carbon (`TISCopyCurrent...`) | `crates/search/src/actions.rs` | `gtk4::ShortcutController` resuelve keyvals latinos nativamente | hecho | 1 |
| `Sources/Search/Keyword.swift` | Palabras clave personalizadas de búsqueda (`yt`, `gh`) | `crates/search-core/src/address.rs` | Rust puro (`serde`) | directo | 3 |
| `Sources/Search/Launcher.swift` | Ventana tipo Spotlight global | — | Fuera de alcance minimalista (`PLAN.md`) | diferido | — |
| `Sources/Search/Lights.swift` | Posicionamiento manual de semáforos AppKit en macOS 26 | — | Exclusivo de AppKit; GTK4 usa `adw::HeaderBar` CSD | imposible | — |
| `Sources/Search/Links.swift` | Apertura de URLs externas, instancia única y navegador predeterminado | `crates/search/src/app.rs` | `gio::ApplicationFlags::HANDLES_OPEN`, `xdg-settings` | hecho | 1 |
| `Sources/Search/Little.swift` | Mini-ventana tipo Little Arc | — | Fuera de alcance (`PLAN.md`) | diferido | — |
| `Sources/Search/LiveFolders.swift` | Carpetas RSS dinámicas en barra lateral | — | Prohibidas peticiones periódicas no pedidas (`AGENTS.md`) | diferido | — |
| `Sources/Search/LiveRate.swift` | Workaround de audio CoreMedia en macOS (`playbackRate = 1.0001`) | — | Bug específico de CoreMedia en macOS | imposible | — |
| `Sources/Search/LocalText.swift` | Renderizado seguro de archivos de texto locales `file://` | `crates/search-core/src/address.rs` | `std::fs` + validación UTF-8 | hecho | 1 |
| `Sources/Search/Loot.swift` | Historial persistente de descargas (`downloads.json`) y renombrado sin sobrescribir | `crates/search-core/src/downloads.rs` | `serde_json`, `gtk4::FileLauncher` | hecho | 3 |
| `Sources/Search/Markdown.swift` | Renderizador Markdown opcional para modo lectura | `crates/search-core/src/reader.rs` | Rust puro | diferido | 4 |
| `Sources/Search/MeCard.swift` | Autocompletado desde `Contacts.framework` de Apple | — | `Contacts.framework` exclusivo de macOS | imposible | — |
| `Sources/Search/MediaKeys.swift` | Estado de reproducción de audio/vídeo en pestañas | `crates/search/src/browser_view.rs` | Observador de audio autenticado + `BrowserHost::set_audio_muted` / `is_audio_muted` | hecho | 2 |
| `Sources/Search/NativePiP.swift` | PiP privado de WKPreferences en macOS | `crates/search/src/pip.rs` | Reemplazado en Linux por ventana flotante propia (`Float.swift`) | imposible | 6 |
| `Sources/Search/Notifications.swift` | Notificaciones del sistema (fin de descarga / permisos web) | `crates/search/src/downloads.rs` | `gio::Notification`, `cef::PermissionHandler` | hecho | 3 |
| `Sources/Search/Omnibox.swift` | Campo único de dirección/búsqueda con autocompletado inline y sugerencias | `crates/search/src/address.rs` | `gtk4::Entry`, `Editable::select_region`, `gtk4::Popover` | hecho | 1 |
| `Sources/Search/OneWindow.swift` | Fusión de perfiles en una sola ventana | — | Subsistema secundario fuera de `PLAN.md` | diferido | — |
| `Sources/Search/PageFind.swift` | Motor de búsqueda en la página | `crates/search/src/find_bar.rs` | `cef::BrowserHost::find` + `cef::FindHandler` | hecho | 1 |
| `Sources/Search/PageHands.swift` | Primitivas de inspección/captura para `bench` | `crates/search/src/bench.rs` | `cef::Frame::execute_java_script` + buffer OSR `gdk4::Texture::save_to_png` | adaptado | 7 |
| `Sources/Search/PageQuestions.swift` | Banners discretos de permisos y guardado de contraseñas | `crates/search/src/vault_ui.rs` | `adw::Banner` | adaptado | 5 |
| `Sources/Search/PaneStage.swift` | Contenedor Split View con divisor arrastrable y doble clic 50/50 | `crates/search/src/split_view.rs` | `gtk4::Paned`, `gtk4::GestureClick` | adaptado | 6 |
| `Sources/Search/Passkeys.swift` | Ceremonia WebAuthn mediante `AuthenticationServices` de macOS | — | `AuthenticationServices` exclusivo de macOS | imposible | — |
| `Sources/Search/Passwords.swift` | Panel de contraseñas (buscar, mostrar con confirmación, copiar 30s, editar, borrar, exportar CSV) | `crates/search/src/vault_ui.rs` | `adw::PreferencesGroup`, `gdk4::Clipboard`, `glib::timeout_add_seconds_local` | adaptado | 5 |
| `Sources/Search/Peek.swift` | Vista previa modal de enlaces | — | Función extra fuera de `PLAN.md` | diferido | — |
| `Sources/Search/PinDrop.swift` | Drag & drop para reordenar pestañas y fijadas | `crates/search/src/sidebar_tabs.rs` | `gtk4::DragSource`, `gtk4::DropTarget` | hecho | 2 |
| `Sources/Search/PinFolders.swift` | Modelo de carpetas de pines | — | Subsistema opcional estilo Arc | diferido | — |
| `Sources/Search/PinFolderViews.swift` | Vistas de carpetas de pines | — | Subsistema opcional estilo Arc | diferido | — |
| `Sources/Search/Pins.swift` | Estado de pestañas fijadas (persisten en `session.json` y no cierran con `Ctrl+W`) | `crates/search-core/src/session.rs` | `adw::TabView::set_page_pinned`, `serde_json` | hecho | 2 |
| `Sources/Search/Plate.swift` | Primitivas visuales reutilizables para paneles | `crates/search/src/window.rs` | `adw::Clamp`, `adw::PreferencesGroup`, `.boxed-list` | hecho | 3 |
| `Sources/Search/Player.swift` | Indicador/control de audio en pestañas | `crates/search/src/sidebar_tabs.rs` | `BrowserHost::is_audio_muted` / `set_audio_muted` | hecho | 2 |
| `Sources/Search/PopupGesture.swift` | Control de popups legítimos (`target=_blank` / OAuth `window.open`) | `crates/search/src/browser_view.rs` | `cef::LifeSpanHandler::on_before_popup` (`user_gesture`) | hecho | 2 |
| `Sources/Search/Prefs.swift` | Almacén reactivo de ajustes con persistencia atómica | `crates/search-core/src/settings.rs` + `crates/search/src/settings.rs` | JSON en `$XDG_CONFIG_HOME/search/settings.json` + esquema GSettings | hecho | 1 |
| `Sources/Search/Privacy.swift` | Política de cookies (3rd-party bloqueadas), GPC y limpieza de click IDs | `crates/search/src/browser_view.rs` | `CefCookieAccessFilter`, cabecera `Sec-GPC: 1` en `on_before_resource_load` | hecho | 1 |
| `Sources/Search/Profiles.swift` | Perfiles múltiples | — | Fuera de alcance de `PLAN.md` | diferido | — |
| `Sources/Search/Proxy.swift` | Configuración de proxy de red | — | CEF respeta variables de proxy del entorno / sistema | hecho | 1 |
| `Sources/Search/Reader.swift` | Modo lectura con tipografía limpia claro/oscuro y tamaño ajustable | `crates/search-core/src/reader.rs` + `crates/search/src/reader.rs` | `third_party/readability/Readability.js` (Apache-2.0) + vista HTML limpia | adaptado | 4 |
| `Sources/Search/Recall.swift` | Paneles de Historial (`Ctrl+H`) y Descargas (`Ctrl+Shift+Y`) | `crates/search/src/history_ui.rs` + `downloads_ui.rs` | `adw::OverlaySplitView`, `gtk4::ListView`, `gtk4::SearchEntry` | hecho | 3 |
| `Sources/Search/RecordingIndicator.swift` | Píldora de grabación de extensiones | — | Diferido a Fase 9 | diferido | 9 |
| `Sources/Search/Registrable.swift` | Cálculo de dominio registrable (eTLD+1) y `sameSite` | `crates/search-core/src/domain.rs` | Rust puro (sufijos públicos / comparación exacta de origin) | directo | 4 |
| `Sources/Search/Scripting.swift` | Interfaz de automatización externa (`Search.sdef`) | `crates/search/src/dbus.rs` | `zbus` (`io.github.searchlinux.Search1`) | adaptado | 7 |
| `Sources/Search/Session.swift` | Persistencia y restauración diferida de ventanas, pestañas, fijadas y splits | `crates/search-core/src/session.rs` | `serde_json`, `paths::write_atomic`, debounce ~1 s, backup `.bak` | hecho | 2 |
| `Sources/Search/SettingsIndex.swift` | Catálogo declarativo de ajustes | `crates/search-core/src/settings.rs` | Rust puro | hecho | 1 |
| `Sources/Search/Settings.swift` | Ventana de preferencias del navegador | `crates/search/src/settings_ui.rs` | `adw::PreferencesDialog` | hecho | 2 |
| `Sources/Search/Sharing.swift` | Menú nativo de compartir macOS (`NSSharingServicePicker`) | — | Exclusivo de AppKit | diferido | — |
| `Sources/Search/Shield.swift` | Bloqueador de anuncios/rastreadores a nivel de red + excepciones por dominio | `crates/search-core/src/shield.rs` + `crates/search/src/shield.rs` | `CefResourceRequestHandler::on_before_resource_load` + CSS cosmético | adaptado | 4 |
| `Sources/Search/ShortcutsPage.swift` | Ventana de consulta de atajos de teclado | `crates/search/src/actions.rs` | `adw::ShortcutsDialog` | hecho | 2 |
| `Sources/Search/Shortcuts.swift` | Definición de acciones y aceleradores con `Ctrl` | `crates/search/src/actions.rs` | `gio::SimpleAction`, `Application::set_accels_for_action` | hecho | 1 |
| `Sources/Search/SideIcons.swift` | Variante compacta de barra lateral | `crates/search/src/sidebar_tabs.rs` | `gtk4::ListBox` | hecho | 2 |
| `Sources/Search/SidePanel.swift` | Panel lateral auxiliar para historial, marcadores, descargas y Curtain | `crates/search/src/window.rs` | `adw::OverlaySplitView` | hecho | 3 |
| `Sources/Search/Side.swift` | Barra lateral izquierda vertical de pestañas (fijadas arriba + normales + DnD) | `crates/search/src/sidebar_tabs.rs` | Widget propio `SidebarTabs` enlazado al `adw::TabView` | hecho | 2 |
| `Sources/Search/SiteCard.swift` | Popover de estado del sitio (TLS, activar/pausar Shield en dominio) | `crates/search/src/window.rs` | `gtk4::Popover`, `cef::NavigationEntry::ssl_status` | adaptado | 4 |
| `Sources/Search/SiteLocation.swift` | Puente CoreLocation en macOS | `crates/search/src/browser_view.rs` | `cef::PermissionHandler` | adaptado | 4 |
| `Sources/Search/SiteSearch.swift` | Plantillas de búsqueda por sitio | `crates/search-core/src/address.rs` | Rust puro | diferido | 3 |
| `Sources/Search/SiteStyles.swift` | Hojas CSS de usuario por dominio | `crates/search/src/curtain.rs` | Inyección CSS en `on_load_start` (`Curtain`) | adaptado | 4 |
| `Sources/Search/Sleep.swift` | Carga diferida (lazy) de pestañas restauradas | `crates/search/src/browser_view.rs` | No instancia `cef::Browser` hasta seleccionar la pestaña en `adw::TabView` | hecho | 2 |
| `Sources/Search/Snap.swift` | Captura PNG del viewport de una pestaña | `crates/search/src/bench.rs` | Buffer OSR `gdk4::Texture::save_to_png` (`bench shot`) | adaptado | 7 |
| `Sources/Search/SpaceGradient.swift` | Gradientes decorativos de espacios | — | Fuera de alcance minimalista | diferido | — |
| `Sources/Search/SpaceRoutes.swift` | Reglas de enrutado a espacios | — | Fuera de alcance minimalista | diferido | — |
| `Sources/Search/Spaces.swift` | Espacios múltiples estilo Arc | — | Fuera de alcance minimalista (`PLAN.md`) | diferido | — |
| `Sources/Search/SpaceSwipe.swift` | Gestos de cambio de espacio | — | Fuera de alcance minimalista | diferido | — |
| `Sources/Search/Split.swift` | Modelo de Split View y serialización en `session.json` | `crates/search-core/src/session.rs` | Rust puro (`serde`) | directo | 6 |
| `Sources/Search/SplitTabItem.swift` | Visualización de dos favicons en una pestaña con Split View | `crates/search/src/split_view.rs` | `adw::TabPage` / `SidebarTabs` con doble icono | adaptado | 6 |
| `Sources/Search/SplitView.swift` | Dos `BrowserView` lado a lado en una pestaña con foco independiente | `crates/search/src/split_view.rs` | `gtk4::Paned`, borde CSS `.focused-pane`, doble clic 50/50 | adaptado | 6 |
| `Sources/Search/Stage.swift` | Área central de la página web, barra de progreso fina y página de error propia | `crates/search/src/browser_view.rs` | `gtk4::Overlay`, `gtk4::ProgressBar` (`.osd`), `on_load_error` | hecho | 1 |
| `Sources/Search/StatusLine.swift` | Etiqueta discreta de URL al pasar el ratón por enlaces | `crates/search/src/browser_view.rs` | `cef::DisplayHandler::on_status_message` | hecho | 1 |
| `Sources/Search/StoreRelay.swift` | Detección de enlaces de Chrome Web Store | `crates/search-core/src/crx.rs` | Rust puro (`url::Url`) | directo | 9 |
| `Sources/Search/Store.swift` | Rutas de almacenamiento, aislamiento de tests y `write_atomic` | `crates/search-core/src/paths.rs` | XDG (`$XDG_DATA_HOME/search`, `0700`, tmp + `fs::rename`) | hecho | 1 |
| `Sources/Search/Substitutions.swift` | Comillas tipográficas de `NSSpellChecker` | — | Exclusivo de AppKit | imposible | — |
| `Sources/Search/Swipe.swift` | Gestos táctiles atrás/adelante | `crates/search/src/browser_view.rs` | `gtk4::GestureSwipe` -> `Browser::go_back` / `go_forward` | hecho | 1 |
| `Sources/Search/SyncMerge.swift` | Fusión de archivos sincronizados | — | Prohibido por Principio #1 de `AGENTS.md` ("sin sincronización") | imposible | — |
| `Sources/Search/Sync.swift` | Sincronización por carpeta compartida | — | Prohibido por Principio #1 de `AGENTS.md` ("sin sincronización") | imposible | — |
| `Sources/Search/TabBar.swift` | Disposición de pestañas horizontales arriba | `crates/search/src/window.rs` | `adw::TabBar` integrada con `adw::TabView` | hecho | 2 |
| `Sources/Search/TabDrag.swift` | Arrastre para reordenar pestañas o dividir vista | `crates/search/src/sidebar_tabs.rs` | `adw::TabBar` nativo + `gtk4::DragSource` en barra lateral | hecho | 2 |
| `Sources/Search/TabGroups.swift` | Vistas de grupos de pestañas | — | Opcional secundario | diferido | — |
| `Sources/Search/TabGroup.swift` | Modelo JSON de grupos de pestañas (conservado en `session.json` por compatibilidad) | `crates/search-core/src/session.rs` | `serde` | hecho | 2 |
| `Sources/Search/TabLift.swift` | Icono de arrastre de pestaña entre ventanas | `crates/search/src/window.rs` | `adw::TabView` (`create-window`) + `gtk4::DragIcon` (Wayland) | adaptado | 2 |
| `Sources/Search/TabOffers.swift` | Desplegable de sugerencias de autocompletado (historial, pestañas, marcadores) | `crates/search/src/address.rs` | `gtk4::Popover` sin robar foco + `gtk4::ListBox` | hecho | 3 |
| `Sources/Search/TabShortcutHint.swift` | Atajos `Ctrl+1..8` y `Ctrl+9` (última pestaña) | `crates/search/src/actions.rs` | `gio::SimpleAction` | hecho | 2 |
| `Sources/Search/Tab.swift` | Envoltorio `BrowserView` sobre `cef::Browser` con progreso, favicon, zoom y señales | `crates/search/src/browser_view.rs` | `BrowserView` sobre `cef::Browser` (OSR en `gtk4::Picture`) | hecho | 1 |
| `Sources/Search/TabSwitcher.swift` | Selector rápido `Ctrl+K` con búsqueda difusa sobre todas las ventanas | `crates/search/src/tab_switcher.rs` | `adw::Dialog` centrado + `search_core::fuzzy` + teclas `↑/↓/Enter/Ctrl+W/Esc` | hecho | 2 |
| `Sources/Search/Translate.swift` | Traducción con `Translation.framework` de macOS 15+ | — | Framework local de Apple inexistente en Linux; `Translate` de Google apagado | imposible | — |
| `Sources/Search/Trim.swift` | Medición de memoria RSS del árbol de procesos CEF | `crates/search-core/src/bench_proto.rs` | Suma RSS de proceso principal + subprocesos CEF en `/proc` | adaptado | 7 |
| `Sources/Search/Twins.swift` | Espejado opcional de pestañas entre ventanas | — | Fuera de alcance (`PLAN.md`) | diferido | — |
| `Sources/Search/Updater.swift` | Comprobador diario de actualizaciones firmadas (apagado en Flatpak) | `crates/search-core/src/updater.rs` + `crates/search/src/updater.rs` | Detecta `/.flatpak-info`; `ed25519-dalek` + `sha2` para tarball/AppImage | adaptado | 8 |
| `Sources/Search/Vault.swift` | Almacén seguro de contraseñas sin texto plano en disco ni logs | `crates/search-core/src/vault.rs` + `crates/search/src/vault.rs` | `oo7` (Secret Service / Portal), `zeroize` | adaptado | 5 |
| `Sources/Search/Webmail.swift` | Reescritura opcional de `mailto:` | — | En Linux lo gestiona el handler XDG del escritorio | diferido | — |
| `Sources/Search/Welcome.swift` | Bienvenida opcional | — | Minimalismo (`AGENTS.md`: sin pantallas extra) | diferido | — |
| `Sources/Search/WhatsNew.swift` | Notas de versión | — | Cubierto en metainfo AppStream (`io.github.searchlinux.Search.metainfo.xml`) | adaptado | 8 |
| `Sources/Search/Windows.swift` | Gestión multi-ventana (`Ctrl+N`, `Ctrl+Shift+N` privada, persistencia en sesión) | `crates/search/src/app.rs` + `crates/search-core/src/session.rs` | `adw::Application`, `cef::RequestContext` en memoria | hecho | 2 |
| `Sources/Search/WindowTitle.swift` | Sincroniza el título de la ventana con la pestaña activa | `crates/search/src/window.rs` | `adw::ApplicationWindow::set_title` | hecho | 1 |
| `Sources/Search/YouTubeAds.swift` | Reglas de red y scripts de bloqueo de anuncios en YouTube | `crates/search-core/src/shield.rs` | Integrado en `CompiledShield` (`on_before_resource_load` + JS) | adaptado | 4 |

## 4. Esquemas de datos (JSON)

> **Convención de escritura y corrupción:** Todos los archivos de datos viven bajo `$XDG_DATA_HOME/search/` (permisos `0700`) y los ajustes bajo `$XDG_CONFIG_HOME/search/`. Toda escritura usa `paths::write_atomic(path, bytes)` (archivo temporal en el mismo directorio + `fs::rename`) con debounce. Si un JSON está corrupto al leer, se guarda una copia `<archivo>.bak` y se arranca limpio registrando el error con `tracing::warn!`.

### `session.json` (`$XDG_DATA_HOME/search/session.json`)
```json
{
  "tabs": [
    {
      "url": "https://example.com/",
      "title": "Example Domain",
      "pin": "E",
      "home": "https://example.com/",
      "touched": 1795032000.0,
      "muted": false
    },
    {
      "url": "https://docs.rs/cef/latest/cef/",
      "title": "cef - Rust",
      "touched": 1795032100.0,
      "muted": false
    }
  ],
  "active": 1,
  "sidebar": false,
  "splits": [
    {
      "tabs": [0, 1],
      "axis": "horizontal",
      "sizes": [0.5, 0.5],
      "focused": 1
    }
  ],
  "windows": []
}
```

### Historial (`$XDG_DATA_HOME/search/history.json`)
```json
[
  {
    "url": "https://github.com/driceroland/Search",
    "key": "https://github.com/driceroland/Search",
    "title": "driceroland/Search: A small, fast web browser",
    "count": 18,
    "last": 1795032015.412
  }
]
```

### Marcadores (`$XDG_DATA_HOME/search/bookmarks.json`)
```json
[
  {
    "id": "550E8400-E29B-41D4-A716-446655440000",
    "title": "Example",
    "url": "https://example.com/"
  },
  {
    "id": "8F14E45F-CEEA-467A-9A38-123456789ABC",
    "title": "Documentación Rust",
    "children": [
      {
        "id": "C9F0F895-FB98-4B91-B512-987654321DEF",
        "title": "The Rust Book",
        "url": "https://doc.rust-lang.org/book/"
      }
    ]
  }
]
```

### Descargas (`$XDG_DATA_HOME/search/downloads.json`)
```json
[
  {
    "id": "A1B2C3D4-1111-4222-8333-444455556666",
    "name": "manual (1).pdf",
    "path": "/home/usuario/Descargas/manual (1).pdf",
    "url": "https://example.com/manual.pdf",
    "bytes": 1048576,
    "finished": 1795032500.0
  }
]
```

### Ajustes (`$XDG_CONFIG_HOME/search/settings.json` + esquema GSettings `io.github.searchlinux.Search`)
```json
{
  "search.engine": "duckduckgo",
  "search.custom": "",
  "look": "system",
  "sidebar": false,
  "sidebar.width": 220.0,
  "downloads.dir": "",
  "downloads.ask": false,
  "shield": true,
  "shield.paused": [],
  "privacy.gpc": true,
  "privacy.clickIDs": true,
  "sites.keep": false,
  "passwords.save": true,
  "passwords.fill": true,
  "passwords.never": [],
  "pageZoom": 1.0,
  "reader.fontSize": 18,
  "hardwareVideoDecode": true,
  "rendererProcessLimit": 6,
  "update.check": true,
  "bench": false,
  "extensions.experimental": false
}
```

### Curtain — reglas por sitio (`$XDG_DATA_HOME/search/curtain.json`)
```json
{
  "nytimes.com": [
    {
      "selector": "#gateway-content",
      "label": "Dialog",
      "note": "600×420",
      "date": 1795032000.0
    }
  ]
}
```

### Shield — listas de reglas (`$XDG_CACHE_HOME/search/filters`)
Mismo formato JSON declarativo de `WKContentRuleList` (las 57 reglas de `Shield.swift` + `YouTubeAds.swift`), compilado en Rust por hash SHA-256 para bloqueo de red en `CefResourceRequestHandler` e inyección CSS cosmética:
```json
[
  {
    "trigger": {
      "url-filter": "^https?://([^/]+\\.)?doubleclick\\.net[:/]",
      "load-type": ["third-party"]
    },
    "action": { "type": "block" }
  }
]
```

## 5. Atajos de teclado
| Acción | Original (macOS) | Linux | Hecho |
|---|---|---|---|
| Enfocar campo de dirección/búsqueda | `⌘L` | `Ctrl+L` / `F6` / `Alt+D` | ✅ |
| Selector difuso de pestañas abiertas | `⌘K` | `Ctrl+K` | ✅ |
| Nueva pestaña | `⌘T` | `Ctrl+T` | ✅ |
| Cerrar pestaña / panel activo (no cierra fijadas) | `⌘W` | `Ctrl+W` | ✅ |
| Reabrir última pestaña cerrada | `⇧⌘T` | `Ctrl+Shift+T` | ✅ |
| Ventana nueva | `⌘N` | `Ctrl+N` | ✅ |
| Ventana privada | `⇧⌘N` | `Ctrl+Shift+N` | ✅ |
| Atrás / Adelante | `⌘[` / `⌘]` (`⌘←` / `⌘→`) | `Alt+Left` / `Alt+Right` (y `Ctrl+[` / `Ctrl+]`) | ✅ |
| Pestaña siguiente / anterior | `⌃Tab` / `⌃⇧Tab` y `⇧⌘]` / `⇧⌘[` | `Ctrl+Tab` / `Ctrl+Shift+Tab` y `Ctrl+PageDown` / `Ctrl+PageUp` | ✅ |
| Ir a pestaña 1…8 / última pestaña (9) | `⌘1`…`⌘8` / `⌘9` | `Ctrl+1`…`Ctrl+8` / `Ctrl+9` | ✅ |
| Recargar página | `⌘R` | `Ctrl+R` / `F5` | ✅ |
| Recargar sin caché (desde origen) | `⌥⌘R` | `Ctrl+Shift+R` / `Ctrl+Alt+R` | ✅ |
| Detener carga / cerrar overlay o restaurar URL | `Esc` | `Escape` | ✅ |
| Buscar en la página (siguiente / anterior) | `⌘F` (`⌘G` / `⇧⌘G`) | `Ctrl+F` (`Ctrl+G` / `Ctrl+Shift+G`) | ✅ |
| Zoom acercar / alejar / restablecer | `⌘=` / `⌘-` / `⌘0` | `Ctrl+=` (`Ctrl++`) / `Ctrl+-` / `Ctrl+0` | ✅ |
| Alternar pestañas arriba vs barra lateral vertical | `⇧⌘S` | `Ctrl+Shift+S` | ✅ |
| Plegar/mostrar barra de pestañas | `⌘S` | `Ctrl+S` | ✅ |
| Duplicar pestaña | `⌘D` | `Ctrl+Alt+D` | ✅ |
| Añadir o quitar marcador de la página actual | `⇧⌘B` | `Ctrl+D` (y alias `Ctrl+Shift+B`) | ✅ |
| Panel de marcadores | Menú Bookmarks | `Ctrl+Shift+O` | ✅ |
| Panel de historial | `⌘Y` | `Ctrl+H` (y alias `Ctrl+Y`) | ✅ |
| Panel de descargas | `⇧⌘J` | `Ctrl+Shift+Y` (y alias `Ctrl+Shift+J`) | ✅ |
| Modo lectura | `⇧⌘R` | `Ctrl+Alt+R` / `F9` | ⬜ |
| Curtain: modo selección ("ocultar elemento") | `⇧⌘H` | `Ctrl+Shift+H` | ⬜ |
| Curtain: ver elementos ocultos en este sitio | `⇧⌘U` | `Ctrl+Shift+E` | ⬜ |
| Contraseñas (Vault) | `⌥⌘L` | `Ctrl+Alt+L` | ⬜ |
| Split View: dividir / separar pestaña actual | `⌥⌘N` | `Ctrl+Alt+N` | ⬜ |
| Split View: cambiar foco entre paneles (izq / der) | `⌃⌘←` / `⌃⌘→` | `Ctrl+Alt+Left` / `Ctrl+Alt+Right` | ⬜ |
| Picture-in-Picture (vídeo flotante) | `⇧⌘P` | `Ctrl+Shift+P` | ⬜ |
| Copiar dirección actual / Pegar e ir | `⇧⌘C` / `⇧⌘V` | `Ctrl+Shift+C` / `Ctrl+Shift+V` | ✅ |
| Silenciar pestaña activa | `⇧⌘M` | `Ctrl+Shift+M` | ✅ |
| DevTools (Web Inspector) | `⌥⌘I` | `Ctrl+Shift+I` / `F12` | ✅ |
| Preferencias / Atajos de teclado | `⌘,` | `Ctrl+,` / `Ctrl+?` | ✅ |
| Salir de la aplicación | `⌘Q` | `Ctrl+Q` | ✅ |

## 6. Ajustes
| Ajuste | Valor por defecto | Dónde se guarda (GSettings/JSON) | Fase |
|---|---|---|---|
| `search-engine` (`search.engine`) | `"duckduckgo"` | GSettings + `settings.json` | 1 |
| `custom-search-url` (`search.custom`) | `""` | GSettings + `settings.json` | 1 |
| `color-scheme` (`look`) | `"system"` (`system` / `light` / `dark`) | GSettings + `settings.json` | 1 |
| `block-third-party-cookies` (`!sites.keep`) | `true` | GSettings + `settings.json` | 1 |
| `send-gpc` (`privacy.gpc`) | `true` | GSettings + `settings.json` | 1 |
| `strip-click-ids` (`privacy.clickIDs`) | `true` | GSettings + `settings.json` | 1 |
| `page-zoom` (`pageZoom`) | `1.0` | GSettings + `settings.json` | 1 |
| `hardware-video-decode` (`hardwareVideoDecode`) | Según medición VA-API en Radeon Vega | GSettings + `settings.json` | 1 |
| `renderer-process-limit` (`rendererProcessLimit`) | Según medición 4 vs 6 vs 8 | GSettings + `settings.json` | 1 |
| `vertical-tabs` (`sidebar`) | `false` | GSettings + `settings.json` | 2 |
| `sidebar-width` (`sidebar.width`) | `220` | GSettings + `settings.json` | 2 |
| `downloads-dir` (`downloads.dir`) | `""` (carpeta XDG Descargas) | GSettings + `settings.json` | 3 |
| `downloads-ask` (`downloads.ask`) | `false` | GSettings + `settings.json` | 3 |
| `shield-enabled` (`shield`) | `true` | GSettings + `settings.json` | 4 |
| `shield.paused` | `[]` | `settings.json` | 4 |
| `reader-font-size` (`reader.fontSize`) | `18` | GSettings + `settings.json` | 4 |
| `vault-save` (`passwords.save`) | `true` | GSettings + `settings.json` | 5 |
| `vault-fill` (`passwords.fill`) | `true` | GSettings + `settings.json` | 5 |
| `passwords.never` | `[]` | `$XDG_DATA_HOME/search/vault_never.json` | 5 |
| `bench-enabled` (`bench`) | `false` (o `SEARCH_BENCH=1`) | GSettings + `settings.json` | 7 |
| `update-check` (`update.check`) | `true` (desactivado en Flatpak) | GSettings + `settings.json` | 8 |
| `experimental-extensions` | `false` | GSettings + `settings.json` | 9 |

## 7. Protocolo del socket de bench
- **Ubicación:** `$XDG_RUNTIME_DIR/search/bench.sock` con permisos `0600` (y soporte de `SEARCH_SOCKET`).
- **Activación:** Desactivado por defecto; se activa en ajustes o con `SEARCH_BENCH=1`. Verifica `SO_PEERCRED` (`uid == geteuid()`).
- **Formato en cable:** 1 línea JSON por conexión terminada en `\n`, clave `"do"` obligatoria, respuesta JSON de 1 línea terminada en `\n`.

| Comando (`"do"`) | Argumentos | Respuesta |
|---|---|---|
| `tabs` | `{"window"?: Int}` | `{"tabs": [TabSummary, ...]}` |
| `open` | `{"url": String, "private"?: Bool}` | `TabSummary` de la pestaña creada |
| `go` | `{"id": String, "url": String}` | `TabSummary` actualizado |
| `wait` | `{"id": String, "seconds"?: f64}` | `TabSummary` al terminar la carga |
| `select` | `{"id": String}` | `TabSummary` de la pestaña activada |
| `close` | `{"id": String \| "all"}` | `{"closed": usize}` |
| `text` | `{"id": String}` | `{"text": String, "truncated": bool, "url": String, "title": String}` |
| `eval` | `{"id": String, "js": String}` | `{"value": Value}` |
| `click` / `type` / `submit` | `{"id": String, "selector": String, "text"?: String}` | `{"ok": true}` |
| `find` | `{"text": String, "next"?: usize, "back"?: usize, "case"?: bool}` | `{"status": String, "count": u32, "ms": u64}` |
| `shot` | `{"id": String, "path": String}` | `{"path": String, "width": i32, "height": i32}` |
| `probe` | `{}` | Estado de ventanas, pestaña activa, RSS total del árbol CEF (`rss_kb`), subprocesos y tiempo de arranque (`startup_ms`) |
| `pin` | `{"id": String, "off"?: bool}` | `{"pin": String, "pinned": usize}` |
| `split` | `{"action": "state"\|"pair"\|"focus"\|"fraction"\|"even"\|"swap"\|"detach"\|"close"\|"save", ...}` | Objeto `splitState` compatible con `Tests/split_view` |
| `quit` | `{}` | `{"quitting": true}` |

## 8. Permisos de Flatpak
| Permiso | Por qué |
|---|---|
| `--share=network` | Peticiones HTTP/HTTPS/DNS/WebSocket iniciadas por el usuario. |
| `--share=ipc` | Memoria compartida POSIX/SYSV necesaria entre el proceso principal y los subprocesos de Chromium/CEF. |
| `--socket=wayland` | Salida nativa en Wayland. |
| `--socket=fallback-x11` | Respaldo en sesiones exclusivamente X11. |
| `--device=dri` | Aceleración GPU y decodificación de vídeo por hardware (VA-API en Radeon Vega / Mesa). |
| `--socket=pulseaudio` | Audio web a través de PipeWire/PulseAudio. |
| `--talk-name=org.freedesktop.secrets` | Acceso a Secret Service desde `oo7` para Vault. |
| `--filesystem=xdg-download` | Descargas en la carpeta estándar de Descargas sin exponer `home` ni `host`. |

*(Nota sobre el sandbox interno de CEF dentro de Flatpak: como `flatpak run` ya aísla toda la aplicación con `bwrap` e impide crear `CLONE_NEWUSER` anidados sin `zypak`, al detectar `/.flatpak-info` se activa `no_sandbox = 1` en `CefSettings` delegando el aislamiento de sistema de archivos/red al propio sandbox de Flatpak, manteniendo el aislamiento de procesos **Site Isolation** intacto).*

*(Pendiente antes de activar `.github/workflows/flatpak.yml` en push/PR y publicar en Flathub: generar `build-aux/flatpak/cargo-sources.json` con `flatpak-cargo-generator` desde `Cargo.lock` y sustituir el marcador `REPLACE_WITH_PINNED_CEF_156_0_30_LINUX64_MINIMAL_SHA256` en `build-aux/flatpak/io.github.searchlinux.Search.yml` por el SHA-256 del tarball oficial de CEF `156.0.30`; mientras tanto `flatpak.yml` queda en `workflow_dispatch`).*

## 9. Riesgos (ordenados por impacto)
1. **Integración de bucle de eventos e input OSR entre GTK4 y CEF — Impacto Alto:** Requiere despachar `cef::execute_process` antes de inicializar GTK4 en subprocesos `--type=*`, bombear `cef::do_message_loop_work()` sin bloqueo ni *busy-wait*, y traducir con fidelidad coordenadas lógicas/escala HiDPI, rueda y teclado.
2. **Consumo de RAM de Chromium con muchas pestañas — Impacto Alto:** Se mitiga cargando de forma diferida (lazy) las pestañas restauradas y midiendo empíricamente `--renderer-process-limit` (4, 6, 8), `--process-per-site`, `high_efficiency_mode` y `--js-flags=--lite-mode` sin desactivar jamás Site Isolation.
3. **Ausencia de códecs propietarios H.264/AAC en el binario precompilado oficial de CEF — Impacto Medio:** YouTube, WebM, AV1, VP9 y Opus funcionan nativamente; los flujos exclusivamente H.264/AAC patentados requerirían compilar CEF con `proprietary_codecs=true` (excluido expresamente por requisito del usuario).
4. **Extensiones de Chrome en modo Alloy/OSR (Fase 9) — Impacto Medio:** `CEF_RUNTIME_STYLE_ALLOY` no expone la UI de extensiones de Chrome; se verificará en Fase 9 y se aplicará Paso 2B (`crx.rs` completo + UI experimental) si procede.
5. **VA-API en entornos headless / contenedores — Impacto Medio:** Se medirá en la Radeon Vega del Ryzen 5 5600G (`/dev/dri/renderD128`) y se expondrá como ajuste por si falla en algún driver.

## 10. Correcciones al PLAN.md
- **Cambio de motor de WebKitGTK 6.0 a CEF (`cef-rs` 156.6, Chromium 156) sin compilar Chromium:** Por requisito explícito del usuario, el motor pasa a ser los binarios precompilados de CEF descargados por `cef-rs`, conservando la interfaz nativa minimalista en GTK4 + libadwaita mediante renderizado windowless OSR (`Alloy`).
- **Todo en Rust (Cargo Workspace sin Python ni Meson):** Aplicación, herramientas (`bench`, `test-find`), tests headless (`split_view`, servidor HTTP) e instalador (`xtask`) en Rust puro.
- **Optimización de Chromium con flags en `chromium_flags.rs` (Nivel 2):** Todos los flags y features verificados contra `libcef.so` (`156.0.8078.96`), descartando switches obsoletos (`--safebrowsing-disable-auto-update`, `--disable-client-side-phishing-detection`, `OptimizationGuideModelDownloading`, `VaapiVideoDecoder` como feature) y sustituyéndolos por sus equivalentes reales de Chromium 156.
- **Tamaño real de `upstream`:** 99.047 líneas en 171 archivos Swift (`v1.0.14+`); los subsistemas retenidos o exclusivos de Apple (`AI*`, `ApplePasswords*`, `Passkeys`, `Translate`, `Sync*`) quedan fuera de alcance conforme a `AGENTS.md`.

## 11. Decisiones de arquitectura
| Fecha | Decisión | Motivo |
|---|---|---|
| 2026-11-18 | **Motor CEF precompilado (`cef-rs` v156.6.0 / Chromium 156.0.8078.96) en modo `Alloy` + OSR sobre GTK4/libadwaita** | Requisito explícito del usuario; el modo windowless OSR funciona igual en Wayland y X11, oculta cualquier UI de Chrome y permite múltiples vistas simultáneas (pestañas, Split View y PiP). |
| 2026-11-18 | **Módulo centralizado `crates/search-core/src/chromium_flags.rs` + `SEARCH_CHROMIUM_FLAGS`** | Requisito explícito del usuario: concentra todos los switches, features y preferencias verificados en `libcef.so`, apaga servicios de Google/telemetría/gestor de contraseñas y permite pruebas dinámicas sin recompilar manteniendo Site Isolation intacto. |
| 2026-11-18 | **Cargo workspace 100 % Rust (`crates/search`, `crates/search-core`, `crates/search-bench`, `crates/xtask`) sin Meson** | Cumple el requisito de máximo Rust y simplifica tests unitarios puros y empaquetado Flatpak (`buildsystem: simple`). |
| 2026-11-18 | **Puente IPC JS→Rust autenticado por token efímero de 128 bits** | Permite a los scripts mínimos de Curtain, Vault y modo lectura enviar datos estructurados a Rust sin exponer APIs globales accesibles a scripts maliciosos de la página. |
| 2026-11-18 | **ID `io.github.searchlinux.Search`, `AdwTabView` único y PiP en ventana flotante propia** | Coherencia con GNOME/Flathub, cambio instantáneo entre pestañas superiores y barra lateral vertical, y paridad con `Float.swift`. |
| 2026-11-18 | **Crate propia `crates/gnome-platform` sobre las bibliotecas C del SDK de GNOME 51 (`gtk-4`, `adwaita-1`, `gio-2.0`, `gobject-2.0`, `glib-2.0`)** | Permite compilar 100 % offline contra el SDK local con control directo de referencias `GObject`, `GdkMemoryTexture` OSR y `GSettings`, lista para ampliar en Fases 2–8 (`AdwTabView`, `AdwTabBar`, `AdwOverlaySplitView`, `libsecret-1`, `GDBus`). |
| 2026-11-18 | **Sincronización automática `--ozone-platform=wayland\|x11\|headless` y `gtk_disable_setlocale()`** | Evita que Chromium intente conectar a un socket Wayland inexistente cuando GTK4 corre bajo X11/Xvfb y elimina conflictos de locale entre GTK4 y `libcef.so`. |

## 12. Rendimiento y mediciones de flags de Chromium
> Hardware de medición: **AMD Ryzen 5 5600G with Radeon Graphics** (12 hilos, Linux x86_64).

| Configuración / Experimento | Arranque en frío | RAM (1 / 10 / 20 pestañas) | Carga sitios ref. (YouTube / Prensa / WebApp / X / Reddit) | Decisión (activo por defecto: Sí/No) | Fecha |
|---|---|---|---|---|---|
| Línea base CEF (flags de privacidad/sin Google + `renderer-process-limit=6`) | **348 ms** ventana / **370 ms** 1.er fotograma OSR (`1180×720`) | *(Se mide en Fase 7)* | *(Se mide en Fases 4 y 7)* | Sí (base obligatoria) | 2026-11-18 |
| `--renderer-process-limit=4` | | | | *(Por medir)* | |
| `--renderer-process-limit=6` | | | | *(Por medir)* | |
| `--renderer-process-limit=8` | | | | *(Por medir)* | |
| `--process-per-site` | | | | *(Por medir)* | |
| `performance_tuning.high_efficiency_mode.state` | | | | *(Por medir)* | |
| `--js-flags=--lite-mode` | | | | *(Por medir)* | |
| VA-API (`AcceleratedVideoDecodeLinuxGL,AcceleratedVideoDecodeLinuxZeroCopyGL,VaapiIgnoreDriverChecks`) — CPU vídeo 1080p | | | | *(Por medir)* | |

## 13. Pruebas en sitios reales
| Sitio | Shield | Curtain | Lectura | Notas |
|---|---|---|---|---|
| *(Pendiente Fase 4)* | | | | |

## 14. Seguridad de Vault (modelo de amenazas)
*(Se completará en detalle en la Fase 5: Secret Service `oo7` + `zeroize`, desactivación total del Password Manager y Autofill de Chromium en `chromium_flags.rs`, coincidencia exacta de `origin` sin degradación HTTPS→HTTP, borrado del portapapeles a los 30 s y tests con datos sintéticos).*
