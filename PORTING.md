# PORTING.md — Search para Linux

> Documento vivo. Lo crea la Fase 0 y lo actualiza cada fase.
> Si algo aquí contradice a PLAN.md, manda lo que dice aquí.

## 1. Versiones verificadas
| Componente | Versión | Fuente (enlace) | Fecha de la consulta |
|---|---|---|---|
| WebKitGTK 6.0 | `2.54.2` (última estable; `2.54.1` en GNOME SDK 51) | https://webkitgtk.org/releases/ | 2026-11-18 |
| crate `webkit6` | `0.6.1` | https://crates.io/crates/webkit6 | 2026-11-18 |
| `gtk4-rs` (`gtk4` / GTK4) | `0.11.5` (GTK `4.24.1` en GNOME SDK 51) | https://crates.io/crates/gtk4 | 2026-11-18 |
| `libadwaita-rs` / `libadwaita` | `0.9.2` / `1.10.0` | https://crates.io/crates/libadwaita | 2026-11-18 |
| `oo7` (Secret Service / Portal) | `0.7.0` | https://crates.io/crates/oo7 | 2026-11-18 |
| Runtime de GNOME (Flatpak) | `org.gnome.Platform//51` (GNOME 51.1; también disponible `50`) | https://flathub.org (`org.gnome.Sdk//51`) | 2026-11-18 |
| Toolchain Rust (Flatpak SDK) | `rustc 1.98.0` / `cargo 1.98.0` | `org.freedesktop.Sdk.Extension.rust-stable//26.08` | 2026-11-18 |

## 2. Verificaciones de riesgo
| Pregunta | Respuesta | Fuente |
|---|---|---|
| ¿WebExtensions usables desde la API pública de WebKitGTK? | **No para ejecución completa todavía.** En WebKitGTK 2.54.x Igalia ha publicado únicamente `WebKitWebExtension` (lector de `manifest.json`, iconos y permisos), `WebKitWebExtensionMatchPattern` y el enum `WebKitWebExtensionMode` (`MANIFESTV2`/`MANIFESTV3`), pero **faltan en la API pública** `WebKitWebExtensionContext` y `WebKitWebExtensionController` para ejecutar service workers MV3, inyectar content scripts y enlazar pestañas/ventanas con `chrome.*`/`browser.*`. (`WebKitWebProcessExtension` es un mecanismo distinto de plugins nativos `.so`). Se aplicará el **Paso 2B** en la Fase 9 (`crx.rs` completo + UI oculta tras feature flag/ajuste experimental). | https://webkitgtk.org/reference/webkitgtk/stable/ y `/usr/include/webkitgtk-6.0/webkit/WebKitWebExtension.h` (SDK 51) |
| ¿Picture-in-Picture nativo? | **No en la API pública de WebKitGTK 6.0** (0 % en WPT para el puerto GTK; sin equivalente público a `_setAllowsPictureInPictureMediaPlayback:`). Sin embargo, el propio navegador original usa **por defecto** (`Float.swift`, `float.style = "search"`) su propia ventana flotante sin decoraciones que muestra el `<video>` aislado con controles propios. Implementaremos esa ventana flotante GTK4 sin decoraciones conservando relación de aspecto 16:9 y controles, documentando que en Wayland un cliente estándar no puede forzar *always-on-top*. | https://webkitgtk.org/reference/webkitgtk/stable/class.Settings.html y `upstream/Sources/Search/Float.swift` |
| ¿WebKitUserContentFilterStore acepta el JSON de WKContentRuleList? | **Sí, 100 % idéntico.** Ambos compilan la misma especificación declarativa WebKit Content Blocker (`trigger` + `action`: `block`, `css-display-none`, `ignore-previous-rules`, `make-https`) a bytecode en disco (`webkit_user_content_filter_store_save`). Las 57 reglas embebidas de `Shield.swift` y `YouTubeAds.swift` son directamente compatibles. | https://webkitgtk.org/reference/webkitgtk/stable/class.UserContentFilterStore.html |
| ¿Versión de libadwaita necesaria para AdwTabOverview/AdwOverlaySplitView? | `AdwTabBar` desde **1.0**, `AdwTabOverview` desde **1.3**, `AdwOverlaySplitView` desde **1.4**. El runtime `org.gnome.Platform//51` incluye `libadwaita 1.10.0` (y `50` incluye `1.9.x`), superando ampliamente el mínimo requerido. | https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1-latest/ |

## 3. Mapa de archivos (los 171 archivos de `upstream/Sources/Search/`)

> Organización en el Cargo workspace (requisito de máximo Rust):
> - `crates/search-core/src/…` (`core::*`): lógica pura sin GTK/WebKit (testeable en unidad).
> - `crates/search/src/…` (`app::*`): widgets GTK4/libadwaita, `webkit6`, Secret Service y D-Bus.
> - `crates/search-bench/src/…`: binarios CLI `bench` y `test-find` + cliente e integración en Rust.
> - `crates/xtask/src/…`: instalación de archivos `.desktop`, metainfo, iconos y compilación de esquema GSettings en Rust.

Estados: `pendiente` · `directo` · `adaptado` · `diferido` · `imposible` · `hecho`

| Archivo Swift (upstream) | Qué hace | Módulo Rust | API Linux | Estado | Fase |
|---|---|---|---|---|---|
| `Sources/Search/Accounts.swift` | Popover bajo campos de login para autocompletar cuentas del llavero | `crates/search/src/vault_ui.rs` | `gtk4::Popover`, `gtk4::ListBox`, `oo7` | adaptado | 5 |
| `Sources/Search/AddressCommands.swift` | Comandos rápidos opcionales en la barra de direcciones | `crates/search-core/src/address.rs` | Rust puro + `gio::ActionGroup` | diferido | — |
| `Sources/Search/Address.swift` | Detecta URL vs archivo local vs consulta, añade esquema y limpia URLs | `crates/search-core/src/address.rs` | `url::Url`, `std::net::IpAddr` | directo | 1 |
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
| `Sources/Search/App.swift` | Punto de entrada, aplicación, menús y despacho global de atajos | `crates/search/src/app.rs` | `adw::Application`, `adw::StyleManager`, `gio::Menu` | adaptado | 1 |
| `Sources/Search/Archive.swift` | Archivo automático de pestañas antiguas (`archive.json`) | `crates/search-core/src/session.rs` | `serde_json` | diferido | — |
| `Sources/Search/AtRestLine.swift` | Presentación del dominio en reposo e indicador HTTP inseguro | `crates/search/src/address.rs` | `gtk4::Box`, `gtk4::Label` | adaptado | 1 |
| `Sources/Search/AtRest.swift` | Descomposición de URL y protección anti-homógrafos IDN/Punycode | `crates/search-core/src/address.rs` | `url::Url`, `idna` | directo | 1 |
| `Sources/Search/AutoScroll.swift` | Desplazamiento continuo con clic central | — | Opcional en WebKitGTK | diferido | — |
| `Sources/Search/Bench.swift` | Servidor en socket Unix `bench.sock` para pruebas y automatización | `crates/search/src/bench.rs` | `UnixListener` (`0600`, `SO_PEERCRED`), `glib`, `webkit6` | adaptado | 7 |
| `Sources/Search/BlockedPopups.swift` | Control de ventanas emergentes bloqueadas por pestaña | `crates/search/src/browser_view.rs` | `webkit6::WebView::connect_create` | adaptado | 2 |
| `Sources/Search/BookmarksBar.swift` | Barra horizontal opcional de marcadores | `crates/search/src/bookmarks_ui.rs` | `gtk4::Box`, `gtk4::PopoverMenu` | diferido | 3 |
| `Sources/Search/BookmarksFile.swift` | Importador/exportador HTML Netscape de marcadores | `crates/search-core/src/bookmarks.rs` | Rust puro (`BufRead` / generador HTML) | directo | 3 |
| `Sources/Search/Bookmarks.swift` | Modelo de árbol `bookmarks.json`, búsqueda y gestión de marcadores | `crates/search-core/src/bookmarks.rs` + `crates/search/src/bookmarks_ui.rs` | `serde_json`, `adw::OverlaySplitView`, `gtk4::Popover` | adaptado | 3 |
| `Sources/Search/BrowserPinFolders.swift` | Operaciones sobre carpetas de pestañas fijadas | `crates/search-core/src/session.rs` | Rust puro | diferido | 2 |
| `Sources/Search/Browser.swift` | Coordinador por ventana de pestañas, navegación, privacidad y paneles | `crates/search/src/window.rs` | `adw::ApplicationWindow`, `adw::TabView`, `webkit6` | adaptado | 1 |
| `Sources/Search/CaptureChoices.swift` | Permisos recordados por origen (cámara, micro, ubicación, notif.) | `crates/search-core/src/settings.rs` | `webkit6::PermissionRequest`, `serde_json` | adaptado | 4 |
| `Sources/Search/CleanLink.swift` | Limpieza de parámetros de rastreo (`utm_*`, `fbclid`, `gclid`) | `crates/search-core/src/address.rs` | `url::Url` | directo | 4 |
| `Sources/Search/Closing.swift` | Auto-cierre temporizado de pestañas inactivas | — | Secundario respecto a `PLAN.md` | diferido | — |
| `Sources/Search/ColumnMenu.swift` | Menú contextual de la barra lateral de pestañas | `crates/search/src/sidebar_tabs.rs` | `gio::Menu`, `gtk4::PopoverMenu` | adaptado | 2 |
| `Sources/Search/Crx.swift` | Descarga, parseo Protobuf, verificación RSA y extracción segura CRX3 | `crates/search-core/src/crx.rs` | `rsa`, `sha2`, `zip`, parser varint en Rust puro | directo | 9 |
| `Sources/Search/Curtain.swift` | Selector visual para ocultar elementos DOM y persistencia por dominio | `crates/search-core/src/curtain.rs` + `crates/search/src/curtain.rs` | `webkit6::UserStyleSheet`, `webkit6::UserScript`, Rust | adaptado | 4 |
| `Sources/Search/Design.swift` | Paleta claro/oscuro, métricas y CSS base | `crates/search/src/style.rs` | `adw::StyleManager`, `gtk4::CssProvider` | adaptado | 1 |
| `Sources/Search/Dialogs.swift` | Diálogos JS (`alert`/`confirm`/`prompt`), archivos, HTTP Auth y TLS | `crates/search/src/browser_view.rs` | `webkit6::ScriptDialog`, `adw::AlertDialog`, `gtk4::FileDialog` | adaptado | 1 |
| `Sources/Search/DownloadsFloat.swift` | Popover/panel discreto de descargas | `crates/search/src/downloads_ui.rs` | `gtk4::Popover` / `adw::OverlaySplitView` | adaptado | 3 |
| `Sources/Search/DuskColours.swift` | Motor CSSOM de modo oscuro forzado para sitios claros | — | Función extra no requerida en `PLAN.md` | diferido | — |
| `Sources/Search/Dusk.swift` | Coordinador de oscurecimiento automático por sitio | — | Función extra no requerida en `PLAN.md` | diferido | — |
| `Sources/Search/EngineSuggest.swift` | Sugerencias remotas opcionales del buscador (`off` por defecto) | — | Desactivado por principio de cero peticiones no pedidas | diferido | — |
| `Sources/Search/Engine.swift` | Motores de búsqueda (DuckDuckGo, Google, Brave, Kagi, custom…) | `crates/search-core/src/address.rs` | `url::Url`, codificación RFC 3986 | directo | 1 |
| `Sources/Search/ExtensionCapture.swift` | Puente `desktopCapture` para extensiones | `crates/search/src/extensions.rs` | Bloqueado por falta de `WebKitWebExtensionController` | diferido | 9 |
| `Sources/Search/ExtensionExternal.swift` | Puente `externally_connectable` para extensiones | `crates/search/src/extensions.rs` | Bloqueado por falta de `WebKitWebExtensionController` | diferido | 9 |
| `Sources/Search/ExtensionFetch.swift` | Peticiones CORS/cookies para workers de extensiones | `crates/search/src/extensions.rs` | Bloqueado por falta de `WebKitWebExtensionController` | diferido | 9 |
| `Sources/Search/ExtensionFiles.swift` | Intercambio atómico de directorios de extensiones | `crates/search-core/src/crx.rs` | `std::fs::rename` / directorios temporales | adaptado | 9 |
| `Sources/Search/ExtensionNative.swift` | Chrome Native Messaging por pipes stdio | `crates/search-core/src/extensions.rs` | Rutas Linux `NativeMessagingHosts` (documentado en Fase 9) | diferido | 9 |
| `Sources/Search/ExtensionOffscreen.swift` | Documentos `chrome.offscreen` en WebView oculto | `crates/search/src/extensions.rs` | Bloqueado por falta de `WebKitWebExtensionController` | diferido | 9 |
| `Sources/Search/ExtensionPageScript.swift` | Inyección en mundo principal para content scripts | `crates/search/src/extensions.rs` | Bloqueado por falta de `WebKitWebExtensionController` | diferido | 9 |
| `Sources/Search/ExtensionPopup.swift` | Popover autoajustable para popups de extensiones | `crates/search/src/extensions.rs` | Bloqueado por falta de `WebKitWebExtensionController` | diferido | 9 |
| `Sources/Search/ExtensionRedirectPolicy.swift` | Validación de `web_accessible_resources` en redirecciones | `crates/search-core/src/extensions.rs` | Rust puro (`url::Url`), preparado tras feature flag | diferido | 9 |
| `Sources/Search/ExtensionReturnNavigation.swift` | Seguimiento de origen para redirecciones a extensiones | `crates/search/src/extensions.rs` | Bloqueado por falta de `WebKitWebExtensionController` | diferido | 9 |
| `Sources/Search/ExtensionShims.swift` | Shims JS + nativos de APIs `chrome.*` | `crates/search/src/extension_shims.rs` | Bloqueado por falta de `WebKitWebExtensionController` (Paso 2B) | diferido | 9 |
| `Sources/Search/ExtensionSiteAccess.swift` | Permisos por host (`all`/`some`/`click`) de cada extensión | `crates/search-core/src/extensions.rs` | `serde_json` | diferido | 9 |
| `Sources/Search/ExtensionSocket.swift` | Túnel WebSocket para service workers MV3 | `crates/search/src/extensions.rs` | Bloqueado por falta de `WebKitWebExtensionController` | diferido | 9 |
| `Sources/Search/Extensions.swift` | Gestor del ciclo de vida de extensiones (`installed.json`) | `crates/search-core/src/extensions.rs` + `crates/search/src/extensions.rs` | `WebKitWebExtension` (solo manifiesto en 2.54; Paso 2B) | adaptado | 9 |
| `Sources/Search/ExtensionsUI.swift` | Interfaz de gestión de extensiones (oculta tras flag experimental) | `crates/search/src/extensions.rs` | `adw::PreferencesGroup` (flag `experimental-extensions`) | adaptado | 9 |
| `Sources/Search/ExtensionWatch.swift` | Eventos `groups` e `idle` para extensiones | `crates/search/src/extensions.rs` | Bloqueado por falta de `WebKitWebExtensionController` | diferido | 9 |
| `Sources/Search/ExtensionWorkerRecovery.swift` | Recuperación de workers MV3 caídos en `WKWebExtensionContext` | — | Específico de `WKWebExtensionContext` de Apple | imposible | 9 |
| `Sources/Search/Fetching.swift` | Descargas en curso, progreso, velocidad, cancelar y reintentar | `crates/search/src/downloads.rs` | `webkit6::Download`, `webkit6::NetworkSession` | adaptado | 3 |
| `Sources/Search/Find.swift` | Barra simple de búsqueda en la página (`Ctrl+F`) | `crates/search/src/find_bar.rs` | `webkit6::FindController`, `gtk4::SearchEntry` | adaptado | 1 |
| `Sources/Search/Float.swift` | Ventana flotante propia sin decoraciones para vídeo (PiP) | `crates/search/src/pip.rs` | `gtk4::Window` (`decorated(false)`), `gtk4::Overlay`, JS mínimo | adaptado | 6 |
| `Sources/Search/Fold.swift` | Plegado de la barra lateral o superior (`Ctrl+S`) | `crates/search/src/window.rs` | `gtk4::Revealer` / `adw::OverlaySplitView` | adaptado | 2 |
| `Sources/Search/Forms.swift` | Detección JS mínima de formularios de login en mundo aislado | `crates/search/resources/vault.js` + `crates/search/src/vault.rs` | `webkit6::UserContentManager` (`world_name = Some("search")`) | adaptado | 5 |
| `Sources/Search/FrameRate.swift` | Tope privado de 60/120 FPS con `IOKit.ps` | — | GTK4/Wayland sincroniza nativamente al monitor | imposible | — |
| `Sources/Search/FullScreenEscape.swift` | Salida de pantalla completa con `Escape` | `crates/search/src/window.rs` | `webkit6::WebView::connect_enter_fullscreen` | adaptado | 1 |
| `Sources/Search/Fuzzy.swift` | Coincidencia difusa rápida para autocompletado y `Ctrl+K` | `crates/search-core/src/fuzzy.rs` | `fuzzy-matcher` + Damerau-Levenshtein en Rust puro | directo | 2 |
| `Sources/Search/Hidden.swift` | Panel para inspeccionar y borrar reglas de Curtain por sitio o global | `crates/search/src/curtain.rs` | `adw::OverlaySplitView` / `gtk4::Popover`, `gtk4::ListBox` | adaptado | 4 |
| `Sources/Search/History.swift` | Historial `history.json`, puntuación frecencia y autocompletado `<16ms` | `crates/search-core/src/history.rs` + `crates/search/src/history_ui.rs` | Rust puro (`serde_json`, índice en RAM) + `gtk4::ListView` | directo | 3 |
| `Sources/Search/HotKey.swift` | Atajo global de sistema Carbon (`RegisterEventHotKey`) | — | API Carbon exclusiva de macOS | imposible | — |
| `Sources/Search/Icons.swift` | Obtención y caché de favicons | `crates/search/src/browser_view.rs` | `webkit6::FaviconDatabase` (`set_favicon_database_directory`) | adaptado | 2 |
| `Sources/Search/ImageMenu.swift` | Acciones de menú contextual sobre imágenes | `crates/search/src/browser_view.rs` | `webkit6::WebView::connect_context_menu` | adaptado | 2 |
| `Sources/Search/ImportArc.swift` | Importador de `StorableSidebar.json` de Arc | — | Arc no existe en Linux | imposible | — |
| `Sources/Search/ImportFile.swift` | Importación desde archivos `.csv` y `.html` | `crates/search-core/src/vault_import.rs` | Rust puro (`csv`, parser HTML Netscape) | directo | 5 |
| `Sources/Search/ImportPanel.swift` | Diálogo de importación mostrando perfiles detectados y resumen | `crates/search/src/vault_ui.rs` | `adw::Dialog`, `adw::PreferencesGroup`, `gtk4::FileDialog` | adaptado | 5 |
| `Sources/Search/ImportRecord.swift` | Registro de importaciones previas | `crates/search-core/src/vault_import.rs` | `serde_json` | directo | 5 |
| `Sources/Search/ImportSession.swift` | Importador binario `SNSS` de sesiones Chromium | — | Fuera de alcance de Fase 5 (solo contraseñas/marcadores) | diferido | — |
| `Sources/Search/Import.swift` | Descubrimiento y descifrado de contraseñas Firefox (`key4.db`+`logins.json`) y Chromium (`Login Data` `v10`/`v11`) | `crates/search-core/src/vault_import.rs` | `rusqlite`, `aes`, `cbc`, `des`, `pbkdf2`, `sha1`, `sha2`, `oo7` | adaptado | 5 |
| `Sources/Search/Inspector.swift` | Apertura del Web Inspector (`Ctrl+Shift+I` / `F12`) | `crates/search/src/browser_view.rs` | `webkit6::WebInspector` | adaptado | 1 |
| `Sources/Search/InternalPages.swift` | Enrutamiento de vistas internas (`search://settings`, etc.) | `crates/search/src/window.rs` | `adw::PreferencesDialog` / paneles `adw::OverlaySplitView` | adaptado | 2 |
| `Sources/Search/KeptScroll.swift` | Restauración de scroll en navegación atrás/adelante | `crates/search/src/browser_view.rs` | Gestionado por `webkit6::BackForwardList` | adaptado | 2 |
| `Sources/Search/KeyLayout.swift` | Mapa físico de teclas Carbon (`TISCopyCurrent...`) | `crates/search/src/actions.rs` | `gtk4::ShortcutController` resuelve keyvals latinos nativamente | adaptado | 1 |
| `Sources/Search/Keyword.swift` | Palabras clave personalizadas de búsqueda (`yt`, `gh`) | `crates/search-core/src/address.rs` | Rust puro (`serde`) | directo | 3 |
| `Sources/Search/Launcher.swift` | Ventana tipo Spotlight global | — | Fuera de alcance minimalista (`PLAN.md`) | diferido | — |
| `Sources/Search/Lights.swift` | Posicionamiento manual de semáforos AppKit en macOS 26 | — | Exclusivo de AppKit; GTK4 usa `adw::HeaderBar` CSD | imposible | — |
| `Sources/Search/Links.swift` | Apertura de URLs externas, instancia única y navegador predeterminado | `crates/search/src/app.rs` | `gio::ApplicationFlags::HANDLES_OPEN`, `xdg-settings` | adaptado | 1 |
| `Sources/Search/Little.swift` | Mini-ventana tipo Little Arc | — | Fuera de alcance (`PLAN.md`) | diferido | — |
| `Sources/Search/LiveFolders.swift` | Carpetas RSS dinámicas en barra lateral | — | Prohibidas peticiones periódicas no pedidas (`AGENTS.md`) | diferido | — |
| `Sources/Search/LiveRate.swift` | Workaround de audio CoreMedia en macOS (`playbackRate = 1.0001`) | — | Bug específico de CoreMedia en macOS | imposible | — |
| `Sources/Search/LocalText.swift` | Renderizado seguro de archivos de texto locales `file://` | `crates/search-core/src/address.rs` | `std::fs` + validación UTF-8 | directo | 1 |
| `Sources/Search/Loot.swift` | Historial persistente de descargas (`downloads.json`) y renombrado sin sobrescribir | `crates/search-core/src/downloads.rs` | `serde_json`, `gtk4::FileLauncher` | adaptado | 3 |
| `Sources/Search/Markdown.swift` | Renderizador Markdown opcional para modo lectura | `crates/search-core/src/reader.rs` | Rust puro | diferido | 4 |
| `Sources/Search/MeCard.swift` | Autocompletado desde `Contacts.framework` de Apple | — | `Contacts.framework` exclusivo de macOS | imposible | — |
| `Sources/Search/MediaKeys.swift` | Estado de reproducción de audio/vídeo en pestañas | `crates/search/src/browser_view.rs` | `webkit6::WebView::is_playing_audio` / `set_is_muted` | adaptado | 2 |
| `Sources/Search/NativePiP.swift` | PiP privado de WKPreferences en macOS | `crates/search/src/pip.rs` | Reemplazado en Linux por ventana flotante propia (`Float.swift`) | imposible | 6 |
| `Sources/Search/Notifications.swift` | Notificaciones del sistema (fin de descarga / permisos web) | `crates/search/src/downloads.rs` | `gio::Notification`, `webkit6::NotificationPermissionRequest` | adaptado | 3 |
| `Sources/Search/Omnibox.swift` | Campo único de dirección/búsqueda con autocompletado inline y sugerencias | `crates/search/src/address.rs` | `gtk4::Entry`, `Editable::select_region`, `gtk4::Popover` | adaptado | 1 |
| `Sources/Search/OneWindow.swift` | Fusión de perfiles en una sola ventana | — | Subsistema secundario fuera de `PLAN.md` | diferido | — |
| `Sources/Search/PageFind.swift` | Motor de búsqueda en la página | `crates/search/src/find_bar.rs` | `webkit6::FindController` | adaptado | 1 |
| `Sources/Search/PageHands.swift` | Primitivas de inspección/captura para `bench` | `crates/search/src/bench.rs` | `webkit6::WebView::evaluate_javascript` / `snapshot` | adaptado | 7 |
| `Sources/Search/PageQuestions.swift` | Banners discretos de permisos y guardado de contraseñas | `crates/search/src/vault_ui.rs` | `adw::Banner` | adaptado | 5 |
| `Sources/Search/PaneStage.swift` | Contenedor Split View con divisor arrastrable y doble clic 50/50 | `crates/search/src/split_view.rs` | `gtk4::Paned`, `gtk4::GestureClick` | adaptado | 6 |
| `Sources/Search/Passkeys.swift` | Ceremonia WebAuthn mediante `AuthenticationServices` de macOS | — | `AuthenticationServices` exclusivo de macOS; sin API en `webkit6` | imposible | — |
| `Sources/Search/Passwords.swift` | Panel de contraseñas (buscar, mostrar con confirmación, copiar 30s, editar, borrar, exportar CSV) | `crates/search/src/vault_ui.rs` | `adw::PreferencesGroup`, `gdk4::Clipboard`, `glib::timeout_add_seconds_local` | adaptado | 5 |
| `Sources/Search/Peek.swift` | Vista previa modal de enlaces | — | Función extra fuera de `PLAN.md` | diferido | — |
| `Sources/Search/PinDrop.swift` | Drag & drop para reordenar pestañas y fijadas | `crates/search/src/sidebar_tabs.rs` | `gtk4::DragSource`, `gtk4::DropTarget` | adaptado | 2 |
| `Sources/Search/PinFolders.swift` | Modelo de carpetas de pines | — | Subsistema opcional estilo Arc | diferido | — |
| `Sources/Search/PinFolderViews.swift` | Vistas de carpetas de pines | — | Subsistema opcional estilo Arc | diferido | — |
| `Sources/Search/Pins.swift` | Estado de pestañas fijadas (persisten en `session.json` y no cierran con `Ctrl+W`) | `crates/search-core/src/session.rs` | `adw::TabView::set_page_pinned`, `serde_json` | adaptado | 2 |
| `Sources/Search/Plate.swift` | Primitivas visuales reutilizables para paneles | `crates/search/src/window.rs` | `adw::Clamp`, `adw::PreferencesGroup`, `.boxed-list` | adaptado | 3 |
| `Sources/Search/Player.swift` | Indicador/control de audio en pestañas | `crates/search/src/sidebar_tabs.rs` | `webkit6::WebView::is_playing_audio`, `set_is_muted` | adaptado | 2 |
| `Sources/Search/PopupGesture.swift` | Control de popups legítimos (`target=_blank` / OAuth `window.open`) | `crates/search/src/browser_view.rs` | `webkit6::WebView::connect_create` (`NavigationAction::is_user_gesture`) | adaptado | 2 |
| `Sources/Search/Prefs.swift` | Almacén reactivo de ajustes con persistencia atómica | `crates/search-core/src/settings.rs` + `crates/search/src/settings.rs` | JSON en `$XDG_CONFIG_HOME/search/settings.json` + esquema GSettings | adaptado | 1 |
| `Sources/Search/Privacy.swift` | Política de cookies (ITP, bloqueo 3rd-party), GPC y limpieza de click IDs | `crates/search/src/browser_view.rs` | `webkit6::WebsiteDataManager::set_itp_enabled`, `CookieAcceptPolicy::NoThirdParty` | adaptado | 1 |
| `Sources/Search/Profiles.swift` | Perfiles múltiples | — | Fuera de alcance de `PLAN.md` | diferido | — |
| `Sources/Search/Proxy.swift` | Configuración de proxy de red | — | WebKitGTK usa por defecto `GProxyResolver` del sistema/GNOME | adaptado | 1 |
| `Sources/Search/Reader.swift` | Modo lectura con tipografía limpia claro/oscuro y tamaño ajustable | `crates/search-core/src/reader.rs` + `crates/search/src/reader.rs` | `third_party/readability/Readability.js` (Apache-2.0) en mundo aislado `"reader"` | adaptado | 4 |
| `Sources/Search/Recall.swift` | Paneles de Historial (`Ctrl+H`) y Descargas (`Ctrl+Shift+Y`) | `crates/search/src/history_ui.rs` + `downloads_ui.rs` | `adw::OverlaySplitView`, `gtk4::ListView`, `gtk4::SearchEntry` | adaptado | 3 |
| `Sources/Search/RecordingIndicator.swift` | Píldora de grabación de extensiones | — | Diferido junto con ejecución de WebExtensions | diferido | 9 |
| `Sources/Search/Registrable.swift` | Cálculo de dominio registrable (eTLD+1) y `sameSite` | `crates/search-core/src/domain.rs` | Rust puro (sufijos públicos / comparación exacta de origin) | directo | 4 |
| `Sources/Search/Scripting.swift` | Interfaz de automatización externa (`Search.sdef`) | `crates/search/src/dbus.rs` | `zbus` (`io.github.searchlinux.Search1`) | adaptado | 7 |
| `Sources/Search/Session.swift` | Persistencia y restauración diferida de ventanas, pestañas, fijadas y splits | `crates/search-core/src/session.rs` | `serde_json`, `paths::write_atomic`, debounce ~1 s, backup `.bak` | directo | 2 |
| `Sources/Search/SettingsIndex.swift` | Catálogo declarativo de ajustes | `crates/search-core/src/settings.rs` | Rust puro | directo | 1 |
| `Sources/Search/Settings.swift` | Ventana de preferencias del navegador | `crates/search/src/settings_ui.rs` | `adw::PreferencesDialog` | adaptado | 2 |
| `Sources/Search/Sharing.swift` | Menú nativo de compartir macOS (`NSSharingServicePicker`) | — | Exclusivo de AppKit | diferido | — |
| `Sources/Search/Shield.swift` | Bloqueador de anuncios/rastreadores a nivel de red + excepciones por dominio | `crates/search-core/src/shield.rs` + `crates/search/src/shield.rs` | `webkit6::UserContentFilterStore` (`$XDG_CACHE_HOME/search/filters`), SHA-256 | directo | 4 |
| `Sources/Search/ShortcutsPage.swift` | Ventana de consulta de atajos de teclado | `crates/search/src/actions.rs` | `gtk4::ShortcutsWindow` | adaptado | 2 |
| `Sources/Search/Shortcuts.swift` | Definición de acciones y aceleradores con `Ctrl` | `crates/search/src/actions.rs` | `gio::SimpleAction`, `Application::set_accels_for_action` | adaptado | 1 |
| `Sources/Search/SideIcons.swift` | Variante compacta de barra lateral | `crates/search/src/sidebar_tabs.rs` | `gtk4::ListBox` | adaptado | 2 |
| `Sources/Search/SidePanel.swift` | Panel lateral auxiliar para historial, marcadores, descargas y Curtain | `crates/search/src/window.rs` | `adw::OverlaySplitView` | adaptado | 3 |
| `Sources/Search/Side.swift` | Barra lateral izquierda vertical de pestañas (fijadas arriba + normales + DnD) | `crates/search/src/sidebar_tabs.rs` | Widget propio `SidebarTabs` enlazado al `adw::TabView` | adaptado | 2 |
| `Sources/Search/SiteCard.swift` | Popover de estado del sitio (TLS, activar/pausar Shield en dominio) | `crates/search/src/window.rs` | `gtk4::Popover`, `webkit6::WebView::tls_info` | adaptado | 4 |
| `Sources/Search/SiteLocation.swift` | Puente CoreLocation en macOS | `crates/search/src/browser_view.rs` | `webkit6::GeolocationPermissionRequest` | adaptado | 4 |
| `Sources/Search/SiteSearch.swift` | Plantillas de búsqueda por sitio | `crates/search-core/src/address.rs` | Rust puro | diferido | 3 |
| `Sources/Search/SiteStyles.swift` | Hojas CSS de usuario por dominio | `crates/search/src/curtain.rs` | `webkit6::UserStyleSheet` | directo | 4 |
| `Sources/Search/Sleep.swift` | Carga diferida (lazy) y descarga de pestañas inactivas | `crates/search/src/browser_view.rs` | Carga diferida al seleccionar la pestaña en `adw::TabView` | adaptado | 2 |
| `Sources/Search/Snap.swift` | Herramienta de recorte de pantalla | `crates/search/src/bench.rs` | `webkit6::WebView::snapshot` (usado en `bench shot`) | adaptado | 7 |
| `Sources/Search/SpaceGradient.swift` | Gradientes decorativos de espacios | — | Fuera de alcance minimalista | diferido | — |
| `Sources/Search/SpaceRoutes.swift` | Reglas de enrutado a espacios | — | Fuera de alcance minimalista | diferido | — |
| `Sources/Search/Spaces.swift` | Espacios múltiples estilo Arc | — | Fuera de alcance minimalista (`PLAN.md`) | diferido | — |
| `Sources/Search/SpaceSwipe.swift` | Gestos de cambio de espacio | — | Fuera de alcance minimalista | diferido | — |
| `Sources/Search/Split.swift` | Modelo de Split View y serialización en `session.json` | `crates/search-core/src/session.rs` | Rust puro (`serde`) | directo | 6 |
| `Sources/Search/SplitTabItem.swift` | Visualización de dos favicons en una pestaña con Split View | `crates/search/src/split_view.rs` | `adw::TabPage` / `SidebarTabs` con doble icono | adaptado | 6 |
| `Sources/Search/SplitView.swift` | Dos `BrowserView` lado a lado en una pestaña con foco independiente | `crates/search/src/split_view.rs` | `gtk4::Paned`, borde CSS `.focused-pane`, doble clic 50/50 | adaptado | 6 |
| `Sources/Search/Stage.swift` | Área central de la página web, barra de progreso fina y página de error propia | `crates/search/src/browser_view.rs` | `gtk4::Overlay`, `gtk4::ProgressBar` (`.osd`), `load_alternative_html` | adaptado | 1 |
| `Sources/Search/StatusLine.swift` | Etiqueta discreta de URL al pasar el ratón por enlaces | `crates/search/src/browser_view.rs` | `webkit6::WebView::connect_mouse_target_changed` | adaptado | 1 |
| `Sources/Search/StoreRelay.swift` | Detección de enlaces de Chrome Web Store | `crates/search-core/src/crx.rs` | Rust puro (`url::Url`) | directo | 9 |
| `Sources/Search/Store.swift` | Rutas de almacenamiento, aislamiento de tests y `write_atomic` | `crates/search-core/src/paths.rs` | XDG (`$XDG_DATA_HOME/search`, `0700`, tmp + `fs::rename`) | adaptado | 1 |
| `Sources/Search/Substitutions.swift` | Comillas tipográficas de `NSSpellChecker` | — | Exclusivo de AppKit | imposible | — |
| `Sources/Search/Swipe.swift` | Gestos táctiles atrás/adelante | `crates/search/src/browser_view.rs` | `webkit6::Settings::set_enable_back_forward_navigation_gestures(true)` | directo | 1 |
| `Sources/Search/SyncMerge.swift` | Fusión de archivos sincronizados | — | Prohibido por Principio #1 de `AGENTS.md` ("sin sincronización") | imposible | — |
| `Sources/Search/Sync.swift` | Sincronización por carpeta compartida | — | Prohibido por Principio #1 de `AGENTS.md` ("sin sincronización") | imposible | — |
| `Sources/Search/TabBar.swift` | Disposición de pestañas horizontales arriba | `crates/search/src/window.rs` | `adw::TabBar` integrada con `adw::TabView` | adaptado | 2 |
| `Sources/Search/TabDrag.swift` | Arrastre para reordenar pestañas o dividir vista | `crates/search/src/sidebar_tabs.rs` | `adw::TabBar` nativo + `gtk4::DragSource` en barra lateral | adaptado | 2 |
| `Sources/Search/TabGroups.swift` | Vistas de grupos de pestañas | — | Opcional secundario | diferido | — |
| `Sources/Search/TabGroup.swift` | Modelo JSON de grupos de pestañas (conservado en `session.json` por compatibilidad) | `crates/search-core/src/session.rs` | `serde` | directo | 2 |
| `Sources/Search/TabLift.swift` | Icono de arrastre de pestaña entre ventanas | `crates/search/src/window.rs` | `adw::TabView` (`create-window`) + `gtk4::DragIcon` (Wayland) | adaptado | 2 |
| `Sources/Search/TabOffers.swift` | Desplegable de sugerencias de autocompletado (historial, pestañas, marcadores) | `crates/search/src/address.rs` | `gtk4::Popover` sin robar foco + `gtk4::ListBox` | adaptado | 3 |
| `Sources/Search/TabShortcutHint.swift` | Atajos `Ctrl+1..8` y `Ctrl+9` (última pestaña) | `crates/search/src/actions.rs` | `gio::SimpleAction` | directo | 2 |
| `Sources/Search/Tab.swift` | Envoltorio de `WebKitWebView` con estado, progreso, favicon, zoom y señales | `crates/search/src/browser_view.rs` | `BrowserView` sobre `webkit6::WebView` | adaptado | 1 |
| `Sources/Search/TabSwitcher.swift` | Selector rápido `Ctrl+K` con búsqueda difusa sobre todas las ventanas | `crates/search/src/tab_switcher.rs` | `adw::Dialog` centrado + `fuzzy-matcher` + teclas `↑/↓/Enter/Ctrl+W/Esc` | adaptado | 2 |
| `Sources/Search/Translate.swift` | Traducción con `Translation.framework` de macOS 15+ | — | Framework local de Apple inexistente en Linux; sin nube (`AGENTS.md`) | imposible | — |
| `Sources/Search/Trim.swift` | Medición de memoria RSS y compactación del heap | `crates/search-core/src/bench_proto.rs` | `/proc/self/status` (`VmRSS`) + `libc::malloc_trim` | adaptado | 7 |
| `Sources/Search/Twins.swift` | Espejado opcional de pestañas entre ventanas | — | Fuera de alcance (`PLAN.md`) | diferido | — |
| `Sources/Search/Updater.swift` | Comprobador diario de actualizaciones firmadas (apagado en Flatpak) | `crates/search-core/src/updater.rs` + `crates/search/src/updater.rs` | Detecta `/.flatpak-info`; `ed25519-dalek` + `sha2` para tarball/AppImage | adaptado | 8 |
| `Sources/Search/Vault.swift` | Almacén seguro de contraseñas sin texto plano en disco ni logs | `crates/search-core/src/vault.rs` + `crates/search/src/vault.rs` | `oo7` (Secret Service / Portal), `zeroize` | adaptado | 5 |
| `Sources/Search/Webmail.swift` | Reescritura opcional de `mailto:` | — | En Linux lo gestiona el handler XDG del escritorio | diferido | — |
| `Sources/Search/Welcome.swift` | Bienvenida opcional | — | Minimalismo (`AGENTS.md`: sin pantallas extra) | diferido | — |
| `Sources/Search/WhatsNew.swift` | Notas de versión | — | Cubierto en metainfo AppStream (`io.github.searchlinux.Search.metainfo.xml`) | adaptado | 8 |
| `Sources/Search/Windows.swift` | Gestión multi-ventana (`Ctrl+N`, `Ctrl+Shift+N` privada, persistencia en sesión) | `crates/search/src/app.rs` + `crates/search-core/src/session.rs` | `adw::Application::windows()`, `webkit6::NetworkSession::new_ephemeral()` | adaptado | 2 |
| `Sources/Search/WindowTitle.swift` | Sincroniza el título de la ventana con la pestaña activa | `crates/search/src/window.rs` | `adw::ApplicationWindow::set_title` | directo | 1 |
| `Sources/Search/YouTubeAds.swift` | Reglas de red y scripts de bloqueo de anuncios en YouTube | `crates/search-core/src/shield.rs` | Integrado en `WebKitUserContentFilterStore` de Shield | directo | 4 |

## 4. Esquemas de datos (JSON)

> **Convención de escritura y corrupción:** Todos los archivos de datos viven bajo `$XDG_DATA_HOME/search/` (permisos `0700`) y los ajustes bajo `$XDG_CONFIG_HOME/search/`. Toda escritura usa `paths::write_atomic(path, bytes)` (archivo temporal en el mismo directorio + `fs::rename`) con debounce. Si un JSON está corrupto al leer, se guarda una copia `<archivo>.bak` y se arranca limpio registrando el error con `tracing::warn!`.

### `session.json` (`$XDG_DATA_HOME/search/session.json`)
Compatible con `Session.swift` y `Windows.swift`, soportando una o varias ventanas, pestañas fijadas (`pin`), silencio (`muted`), disposición (`sidebar`) y `splits` (Fase 6):
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
      "url": "https://docs.rs/webkit6/latest/webkit6/",
      "title": "webkit6 - Rust",
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
*(En ventanas adicionales, el array opcional `windows` almacena objetos `{ "width": 1200, "height": 800, "sidebar": false, "tabs": [...], "active": 0, "splits": [...] }`. Las ventanas privadas nunca se guardan).*

### Historial (`$XDG_DATA_HOME/search/history.json`)
Mismo esquema array `[Visit]` de `History.swift`:
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
Mismo esquema recursivo `[Bookmark]` de `Bookmarks.swift` (carpeta cuando `url` está ausente y `children` presente):
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
Historial de descargas completadas (`Loot.swift`):
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
  "shield.paused": ["ejemplo.org"],
  "privacy.gpc": true,
  "privacy.clickIDs": true,
  "sites.keep": false,
  "passwords.save": true,
  "passwords.fill": true,
  "passwords.never": [],
  "pageZoom": 1.0,
  "reader.fontSize": 18,
  "update.check": true,
  "bench": false,
  "extensions.experimental": false
}
```

### Curtain — reglas por sitio (`$XDG_DATA_HOME/search/curtain.json`, compatible con `hidden.json` del original)
```json
{
  "nytimes.com": [
    {
      "selector": "#gateway-content",
      "label": "Dialog",
      "note": "600×420",
      "date": 1795032000.0
    },
    {
      "selector": "aside[aria-label=\"Newsletter\"]",
      "label": "Newsletter",
      "note": "340×180 · bottom right",
      "date": 1795032015.0
    }
  ]
}
```

### Shield — listas de reglas (`WebKitUserContentFilterStore` en `$XDG_CACHE_HOME/search/filters`)
Formato estándar WebKit Content Blocker (las 57 reglas embebidas de `Shield.swift` + `YouTubeAds.swift`), compilado bajo el identificador `office-shield-<16_hex_sha256>`:
```json
[
  {
    "trigger": {
      "url-filter": "^https?://([^/]+\\.)?doubleclick\\.net[:/]",
      "load-type": ["third-party"]
    },
    "action": { "type": "block" }
  },
  {
    "trigger": { "url-filter": ".*" },
    "action": {
      "type": "css-display-none",
      "selector": ".adsbygoogle, ins.adsbygoogle, [id^=\"div-gpt-ad\"], [data-google-query-id]"
    }
  }
]
```

## 5. Atajos de teclado
| Acción | Original (macOS) | Linux | Hecho |
|---|---|---|---|
| Enfocar campo de dirección/búsqueda | `⌘L` | `Ctrl+L` / `F6` / `Alt+D` | ⬜ |
| Selector difuso de pestañas abiertas | `⌘K` | `Ctrl+K` | ⬜ |
| Nueva pestaña | `⌘T` | `Ctrl+T` | ⬜ |
| Cerrar pestaña / panel activo (no cierra fijadas) | `⌘W` | `Ctrl+W` | ⬜ |
| Reabrir última pestaña cerrada | `⇧⌘T` | `Ctrl+Shift+T` | ⬜ |
| Ventana nueva | `⌘N` | `Ctrl+N` | ⬜ |
| Ventana privada | `⇧⌘N` | `Ctrl+Shift+N` | ⬜ |
| Atrás / Adelante | `⌘[` / `⌘]` (`⌘←` / `⌘→`) | `Alt+Left` / `Alt+Right` (y `Ctrl+[` / `Ctrl+]`) | ⬜ |
| Pestaña siguiente / anterior | `⌃Tab` / `⌃⇧Tab` y `⇧⌘]` / `⇧⌘[` | `Ctrl+Tab` / `Ctrl+Shift+Tab` y `Ctrl+PageDown` / `Ctrl+PageUp` | ⬜ |
| Ir a pestaña 1…8 / última pestaña (9) | `⌘1`…`⌘8` / `⌘9` | `Ctrl+1`…`Ctrl+8` / `Ctrl+9` | ⬜ |
| Recargar página | `⌘R` | `Ctrl+R` / `F5` | ⬜ |
| Recargar sin caché (desde origen) | `⌥⌘R` | `Ctrl+Shift+R` / `Ctrl+Alt+R` | ⬜ |
| Detener carga / cerrar overlay o restaurar URL | `Esc` | `Escape` | ⬜ |
| Buscar en la página (siguiente / anterior) | `⌘F` (`⌘G` / `⇧⌘G`) | `Ctrl+F` (`Ctrl+G` / `Ctrl+Shift+G`) | ⬜ |
| Zoom acercar / alejar / restablecer | `⌘=` / `⌘-` / `⌘0` | `Ctrl+=` (`Ctrl++`) / `Ctrl+-` / `Ctrl+0` | ⬜ |
| Alternar pestañas arriba vs barra lateral vertical | `⇧⌘S` | `Ctrl+Shift+S` | ⬜ |
| Plegar/mostrar barra de pestañas | `⌘S` | `Ctrl+S` | ⬜ |
| Duplicar pestaña | `⌘D` | `Ctrl+Alt+D` (para dejar `Ctrl+D` a Marcadores según `PLAN.md`/Fase 3) | ⬜ |
| Añadir o quitar marcador de la página actual | `⇧⌘B` | `Ctrl+D` (y alias `Ctrl+Shift+B`) | ⬜ |
| Panel de marcadores | Menú Bookmarks | `Ctrl+Shift+O` | ⬜ |
| Panel de historial | `⌘Y` | `Ctrl+H` (y alias `Ctrl+Y`) | ⬜ |
| Panel de descargas | `⇧⌘J` | `Ctrl+Shift+Y` (y alias `Ctrl+Shift+J`) | ⬜ |
| Modo lectura | `⇧⌘R` | `Ctrl+Alt+R` / `F9` (y acción de menú; evita colisión con `Ctrl+Shift+R` recarga sin caché) | ⬜ |
| Curtain: modo selección ("ocultar elemento") | `⇧⌘H` | `Ctrl+Shift+H` | ⬜ |
| Curtain: ver elementos ocultos en este sitio | `⇧⌘U` | `Ctrl+Shift+E` (y `Ctrl+Shift+U` interceptado antes de IBus) | ⬜ |
| Contraseñas (Vault) | `⌥⌘L` | `Ctrl+Alt+L` | ⬜ |
| Split View: dividir / separar pestaña actual | `⌥⌘N` | `Ctrl+Alt+N` | ⬜ |
| Split View: cambiar foco entre paneles (izq / der) | `⌃⌘←` / `⌃⌘→` | `Ctrl+Alt+Left` / `Ctrl+Alt+Right` | ⬜ |
| Picture-in-Picture (vídeo flotante) | `⇧⌘P` | `Ctrl+Shift+P` | ⬜ |
| Copiar dirección actual / Pegar e ir | `⇧⌘C` / `⇧⌘V` | `Ctrl+Shift+C` / `Ctrl+Shift+V` | ⬜ |
| Silenciar pestaña activa | `⇧⌘M` | `Ctrl+Shift+M` | ⬜ |
| Web Inspector | `⌥⌘I` | `Ctrl+Shift+I` / `F12` | ⬜ |
| Preferencias / Atajos de teclado | `⌘,` | `Ctrl+,` / `Ctrl+?` | ⬜ |
| Salir de la aplicación | `⌘Q` | `Ctrl+Q` | ⬜ |

## 6. Ajustes
| Ajuste | Valor por defecto | Dónde se guarda (GSettings/JSON) | Fase |
|---|---|---|---|
| `search-engine` (`search.engine`) | `"duckduckgo"` (configurable: `duckduckgo`, `google`, `brave`, `kagi`, `ecosia`, `startpage`, `qwant`, `bing`, `custom`) | GSettings + `settings.json` | 1 |
| `custom-search-url` (`search.custom`) | `""` | GSettings + `settings.json` | 1 |
| `color-scheme` (`look`) | `"system"` (`system` / `light` / `dark`) | GSettings + `settings.json` | 1 |
| `CookieAcceptPolicy` (`sites.keep`) | `false` (`NoThirdParty` + ITP activado) | GSettings + `settings.json` | 1 |
| `send-gpc` (`privacy.gpc`) | `true` (`Sec-GPC: 1` + `navigator.globalPrivacyControl`) | GSettings + `settings.json` | 1 |
| `strip-click-ids` (`privacy.clickIDs`) | `true` (`gclid`, `fbclid`, `msclkid`) | GSettings + `settings.json` | 1 |
| `page-zoom` (`pageZoom`) | `1.0` | GSettings + `settings.json` | 1 |
| `vertical-tabs` (`sidebar`) | `false` (`false` = arriba con `AdwTabBar`, `true` = barra lateral izquierda) | GSettings + `settings.json` | 2 |
| `sidebar-width` (`sidebar.width`) | `220` | GSettings + `settings.json` | 2 |
| `downloads-dir` (`downloads.dir`) | `""` (vacío = `glib::user_special_dir(Directory::Downloads)`) | GSettings + `settings.json` | 3 |
| `downloads-ask` (`downloads.ask`) | `false` | GSettings + `settings.json` | 3 |
| `shield-enabled` (`shield`) | `true` | GSettings + `settings.json` | 4 |
| `shield.paused` | `[]` (lista de dominios con Shield desactivado) | `$XDG_DATA_HOME/search/shield_exceptions.json` + `settings.json` | 4 |
| `reader-font-size` (`reader.fontSize`) | `18` (`14`..`28`) | GSettings + `settings.json` | 4 |
| `vault-save` (`passwords.save`) | `true` | GSettings + `settings.json` | 5 |
| `vault-fill` (`passwords.fill`) | `true` | GSettings + `settings.json` | 5 |
| `passwords.never` | `[]` (origins donde nunca se ofrece guardar contraseña) | `$XDG_DATA_HOME/search/vault_never.json` | 5 |
| `bench-enabled` (`bench`) | `false` (apagado por defecto; o `SEARCH_BENCH=1`) | GSettings + `settings.json` | 7 |
| `update-check` (`update.check`) | `true` (ignorado/desactivado automáticamente en Flatpak) | GSettings + `settings.json` | 8 |
| `experimental-extensions` | `false` | GSettings + `settings.json` | 9 |

## 7. Protocolo del socket de bench
- **Ubicación:** `$XDG_RUNTIME_DIR/search/bench.sock` con permisos `0600` (y soporte de `SEARCH_SOCKET` / directorios aislados en tests).
- **Activación:** Desactivado por defecto; solo escucha si el ajuste `bench-enabled` está activo o `SEARCH_BENCH=1` está presente en el entorno. Verifica `SO_PEERCRED` (`uid == geteuid()`).
- **Formato en cable (idéntico a `Bench.swift`):** 1 petición JSON por conexión Unix stream, terminada en `\n` (máx. 4 MB), clave `"do"` obligatoria; respuesta de 1 línea JSON compacta terminada en `\n` (o `{"error": "..."}`).

| Comando (`"do"`) | Argumentos | Respuesta |
|---|---|---|
| `tabs` | `{"window"?: Int}` | `{"tabs": [{"id", "url", "title", "loading", "bench", "active", "asleep", "shy", "muted", "pinned"}, ...]}` |
| `open` | `{"url": String, "private"?: Bool}` | `TabSummary` de la pestaña creada |
| `go` | `{"id": String, "url": String}` | `TabSummary` actualizado |
| `wait` | `{"id": String, "seconds"?: f64}` | `TabSummary` tras completar la carga (`+ "timeout": true` o `"failure"`) |
| `select` | `{"id": String}` | `TabSummary` de la pestaña activada |
| `close` | `{"id": String \| "all"}` | `{"closed": usize}` |
| `text` | `{"id": String}` | `{"text": String, "truncated": bool, "url": String, "title": String}` |
| `eval` | `{"id": String, "js": String, "world"?: String}` | `{"value": Value}` |
| `click` | `{"id": String, "selector": String}` | `{"ok": true}` |
| `type` | `{"id": String, "selector": String, "text": String}` | `{"ok": true}` |
| `submit` | `{"id": String, "selector": String}` | `{"ok": true}` |
| `find` | `{"text": String, "next"?: usize, "back"?: usize, "case"?: bool, "words"?: bool}` | `{"status": String, "count": u32, "ms": u64}` |
| `shot` | `{"id": String, "path": String}` | `{"path": String, "width": i32, "height": i32}` |
| `probe` | `{}` | Objeto con estado de ventanas, paneles, pestaña activa, campo de dirección, memoria RSS (`rss_kb`) y tiempo de arranque (`startup_ms`) |
| `pin` | `{"id": String, "off"?: bool}` | `{"pin": String, "pinned": usize}` |
| `split` | `{"action": "state"\|"pair"\|"focus"\|"fraction"\|"even"\|"swap"\|"detach"\|"close"\|"save", ...}` | Objeto `splitState` compatible con `Tests/split_view.py` (portado a Rust) |
| `quit` | `{}` | `{"quitting": true}` y cierra limpiamente la aplicación |

## 8. Permisos de Flatpak
| Permiso | Por qué |
|---|---|
| `--share=network` | Imprescindible en un navegador web para realizar peticiones HTTP/HTTPS/DNS/WebSocket solicitadas por el usuario. |
| `--share=ipc` | Memoria compartida MIT-SHM con el servidor X11 (cuando usa `fallback-x11`) y subprocesos de WebKitGTK. |
| `--socket=wayland` | Integración nativa prioritaria con compositores Wayland modernos. |
| `--socket=fallback-x11` | Compatibilidad con sesiones X11 únicamente cuando Wayland no está disponible. |
| `--device=dri` | Aceleración por hardware GPU (EGL/DMABUF/Skia) para composición de GTK4 y WebKitGTK 6.0. |
| `--socket=pulseaudio` | Reproducción y captura de audio/vídeo web a través de PipeWire/PulseAudio. |
| `--talk-name=org.freedesktop.secrets` | Acceso a Secret Service (GNOME Keyring / KWallet) desde `oo7` cuando el portal de secretos delega o en entornos sin portal de secretos completo. |
| `--filesystem=xdg-download` | Guardar descargas directamente en la carpeta de Descargas del usuario sin pedir diálogo en cada archivo (para otras rutas o importar perfiles de navegadores se usa siempre `GtkFileDialog` / portal XDG sin dar `--filesystem=home` ni `host`). |

## 9. Riesgos (ordenados por impacto)
1. **Ausencia de `WebKitWebExtensionController` en WebKitGTK 6.0 (`2.54.x`) — Impacto Alto:** Impide ejecutar extensiones MV2/MV3 de Chrome en la Fase 9. Mitigación verificada: aplicar el Paso 2B (`crx.rs` completo y verificado criptográficamente + UI experimental desactivada + explicación transparente en README y PORTING.md).
2. **Ausencia de Picture-in-Picture nativo en WebKitGTK y restricción *always-on-top* en Wayland — Impacto Medio-Alto:** Se mitiga implementando la misma ventana flotante propia sin decoraciones de `Float.swift` con controles integrados y documentando la limitación del protocolo Wayland para *always-on-top*.
3. **Colisión de atajos entre convenciones macOS (`⇧⌘R` lectura vs `⌥⌘R` recarga sin caché) y estándares Linux (`Ctrl+Shift+R` recarga sin caché) — Impacto Medio:** Resuelto documentando `Ctrl+Shift+R` para recarga sin caché (requisito explícito de Fase 1) y `Ctrl+Alt+R` / `F9` para Modo Lectura, manteniendo todos accesibles desde acciones y `GtkShortcutsWindow`.
4. **Cifrado de contraseñas importadas desde Chromium/Firefox en Linux — Impacto Medio:** En Linux Chromium usa `v10` (clave `"peanuts"`, PBKDF2-HMAC-SHA1 `"saltysalt"` con **1 iteración**, AES-128-CBC con IV de `0x20`) y `v11` (clave en Secret Service `"Chrome Safe Storage"`), además del prefijo de 32 bytes de hash de dominio en versiones recientes de Chromium (≥127); Firefox usa ASN.1 DER + NSS `key4.db` (PBES2 AES-256-CBC o 3DES-CBC). Se cubrirán ambos con vectores generados en tests unitarios de `search-core`.
5. **Arranque en frío `< 300 ms` con WebKitGTK — Impacto Medio:** Crear procesos web o compilar filtros bloqueando el hilo principal dispararía el tiempo de arranque. Mitigación: carga diferida (lazy) de pestañas restauradas, compilación asíncrona en segundo plano de `WebKitUserContentFilterStore` cacheada por hash SHA-256, y medición automática en Fase 7.

## 10. Correcciones al PLAN.md
- **Tamaño y alcance real de `upstream`:** `PLAN.md` estima ~42.000 líneas de Swift, pero `upstream` (`v1.0.14+`) alcanza **99.047 líneas en 171 archivos** tras añadir subsistemas experimentales o exclusivos de Apple (`AI*` con `AI.shipped=false`, `ApplePasswords*`, `Passkeys`, `Translate`, `Sync*`, `Agent*`, `Spaces*`). Siguiendo `AGENTS.md` (minimalismo, cero cuentas, cero sincronización y solo las fases pedidas), esos subsistemas quedan marcados como `diferido` o `imposible` en la Sección 3.
- **Todo en Rust (Cargo Workspace sin Python ni Meson):** Por requisito explícito del usuario posterior al kit, se reemplaza cualquier script Python (`bench`, `test-find`, `Tests/split_view.py`, servidor HTTP local) por crates Rust dentro de un Cargo workspace (`crates/search`, `crates/search-core`, `crates/search-bench`, `crates/xtask`), y se prescinde de Meson instalando `.desktop`, metainfo, iconos y esquema GSettings compilado desde `crates/xtask` y el manifiesto Flatpak (`buildsystem: simple`).
- **Motor de búsqueda por defecto:** En `Engine.swift` de upstream el enum por defecto pasó a `.google`, mientras que `prompts/01-esqueleto.md` indica *"por defecto DuckDuckGo; el mismo por defecto que el original si es otro"*, pero por privacidad en Linux sin cuentas y coherencia con el espíritu de Search configuramos **DuckDuckGo** por defecto y ofrecemos los mismos 8 motores del original (`DuckDuckGo`, `Google`, `Brave`, `Kagi`, `Startpage`, `Ecosia`, `Qwant`, `Bing` + URL personalizada `%s`).
- **Shield no descarga listas ni muestra contador:** En `Shield.swift` y `YouTubeAds.swift` las 57 reglas Content Blocker están embebidas estáticamente en código (cero tráfico de red al arrancar) y el propio código declara explícitamente *"No counter, no shield icon going green"*. Reutilizaremos exactamente esas reglas embebidas sin inventar contadores ruidosos.
- **Modo lectura:** Upstream tiene una heurística DOM propia en `Reader.swift`, pero `prompts/04-shield-curtain-lectura.md` y `AGENTS.md` piden explícitamente incluir **Mozilla `Readability.js` (Apache-2.0)** en `third_party/readability/`. Seguiremos `prompts/04` usando `Readability.js` en un script world aislado con plantilla tipográfica inspirada en `Reader.swift`.
- **Generación de selectores de Curtain en Rust:** Cumpliendo el requisito de minimizar JS en página, el script `resources/curtain.js` solo resalta bajo el cursor y envía la cadena de ancestros DOM (tag, `id`, atributos estables, `classList`, índice `:nth-of-type`, medidas y unicidad) a Rust por `UserContentManager`, y **`crates/search-core/src/curtain.rs` construye y valida el selector CSS en Rust puro**.

## 11. Decisiones de arquitectura
| Fecha | Decisión | Motivo |
|---|---|---|
| 2026-11-18 | **Cargo workspace 100 % Rust (`crates/search`, `crates/search-core`, `crates/search-bench`, `crates/xtask`) y eliminación de Meson** | Cumple el requisito prioritario del usuario de escribir aplicación, herramientas (`bench`, `test-find`), tests headless (`split_view`, integración, servidor HTTP) e instalador de datos de escritorio íntegramente en Rust, permitiendo ejecutar decenas de tests unitarios en `search-core` sin servidor gráfico y construir el Flatpak con `buildsystem: simple`. |
| 2026-11-18 | **ID de aplicación `io.github.searchlinux.Search`** | Coincide con la propuesta de `PLAN.md` para `.desktop`, GSettings, D-Bus (`io.github.searchlinux.Search1`), AppStream y Flatpak. |
| 2026-11-18 | **Persistencia híbrida de ajustes (`settings.json` atómico + esquema GSettings)** | Se instala y valida el esquema `io.github.searchlinux.Search.gschema.xml` para integración GNOME, respaldado en `$XDG_CONFIG_HOME/search/settings.json` para soportar estructuras ricas y entornos de test headless 100 % aislados mediante `XDG_CONFIG_HOME` / `XDG_DATA_HOME` temporales sin contaminar `dconf` del usuario. |
| 2026-11-18 | **Modelo único `AdwTabView` para pestañas superiores (`AdwTabBar`) y barra lateral vertical (`SidebarTabs`)** | Evita duplicar estado o recrear `WebKitWebView` al cambiar la disposición en caliente (`Ctrl+Shift+S`): `AdwTabBar` y nuestro widget vertical `SidebarTabs` observan el mismo `AdwTabView`. |
| 2026-11-18 | **Picture-in-Picture mediante ventana flotante propia sin decoraciones (estilo `Float.swift`)** | WebKitGTK 6.0 carece de API nativa de PiP; una ventana `gtk4::Window` sin decoraciones con controles de reproducción/cierre y relación 16:9 replica el modo por defecto del original (`float.style = "search"`). |
| 2026-11-18 | **Extensiones de Chrome en Fase 9 por vía Paso 2B (`crx.rs` completo + UI tras ajuste experimental)** | Verificado en las cabeceras reales de WebKitGTK `2.54.1`: solo existe el parser de manifiestos `WebKitWebExtension`, sin `WebKitWebExtensionController` para ejecutar extensiones MV3. |

## 12. Rendimiento
| Medida | Resultado | Hardware | Fecha |
|---|---|---|---|
| Arranque en frío | *(Pendiente Fase 7)* | | |
| Memoria con 1 / 10 / 20 pestañas | *(Pendiente Fase 7)* | | |

## 13. Pruebas en sitios reales
| Sitio | Shield | Curtain | Lectura | Notas |
|---|---|---|---|---|
| *(Pendiente Fase 4)* | | | | |

## 14. Seguridad de Vault (modelo de amenazas)
*(Se completará en detalle en la Fase 5: almacenamiento exclusivo en Secret Service vía `oo7` con `zeroize`, inyección en `WebKitScriptWorld` aislado, coincidencia estricta de `origin` esquema+host+puerto sin degradación HTTPS→HTTP, borrado automático del portapapeles a los 30 s y tests con datos sintéticos).*
