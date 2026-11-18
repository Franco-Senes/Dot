# Plan maestro: Search para Linux

## 1. El original
- Repo: https://github.com/driceroland/Search (MIT), clonado en `./upstream`.
- Unas 42.000 líneas de Swift 6, para macOS 14+, con Xcode 16.
- SwiftUI para la interfaz, AppKit para la barra de título y el arrastre de la
  ventana, WKWebView para las páginas, WKWebExtension para las extensiones de
  Chrome (macOS 15.4+). No tiene dependencias externas.
- Build con Swift Package Manager y scripts (`build.sh`, `engine.sh`, `fresh.sh`,
  `publish.sh`, `tap.sh`). Se distribuye con un cask de Homebrew.
- Código en `Sources/Search/`, un archivo por responsabilidad: `Vault.swift`
  (keychain), `Shield.swift` (bloqueador), `Curtain.swift` (ocultar elementos),
  `Session.swift`, `Updater.swift`, `Bench.swift`, `Crx.swift`, `Extensions.swift`,
  `ExtensionShims.swift`, `ExtensionNative.swift`, etc.
- Otras carpetas: `Engine/`, `Icon/`, `Installer/`, `Prototype/`, `Tests/`,
  `skill/search-bench/`, `.github/`. También `Search.sdef` (AppleScript) y los
  scripts `bench` y `test-find`.
- Funciones:
  - Campo único de dirección y búsqueda con autocompletado desde el historial.
  - Pestañas fijadas y restauradas, selector de pestañas (⌘K), Split View.
  - Modo lectura, "ocultar elemento" persistente por sitio, Picture-in-Picture.
  - Bloqueo de anuncios y rastreadores a nivel de red (WKContentRuleList).
  - Contraseñas en el keychain, con importación desde otros navegadores.
  - Extensiones de Chrome desde enlaces de la Chrome Web Store, con
    verificación de firma CRX3 y shims para APIs de Chrome que faltan.
  - Paneles de marcadores, historial y descargas.
  - Comprobación diaria de actualizaciones con verificación de firma.
  - Socket Unix con scripts para pruebas (`./bench`), `Tests/split_view.py`.
- Datos: JSON en `~/Library/Application Support/Search/`.

## 2. Objetivo
Una app nativa de Linux que se comporte igual que el original y que parezca una
app de GNOME de verdad. Misma filosofía: minimalista, rápida, privada.

## 3. Stack
| Capa | Elección | Motivo |
|---|---|---|
| Lenguaje | Rust estable | Los bindings GTK más maduros; seguro en memoria; la concurrencia se parece a la de Swift 6. Swift en Linux no tiene bindings de GUI maduros. |
| UI | GTK4 + libadwaita | Equivalente nativo de SwiftUI/AppKit en el escritorio Linux moderno. |
| Motor | WebKitGTK 6.0 (crate `webkit6`) | Es el mismo motor que WKWebView, así que las páginas se comportan igual y muchas piezas se pueden portar casi directamente. |
| Build | Cargo + Meson | El estándar de las apps GNOME escritas en Rust. |
| Distribución | Flatpak (Flathub) | Funciona en cualquier distro, con sandbox y actualizaciones. Opcional: .deb/.rpm/AppImage. |

Crates recomendados (comprueba la versión estable actual de cada uno):
`gtk4`, `libadwaita`, `webkit6`, `glib`, `gio`, `serde`, `serde_json`,
`anyhow`, `thiserror`, `tracing`, `tracing-subscriber`, `oo7` (Secret Service),
`url`, `directories` o `glib::user_*_dir`, `rusqlite` (importar contraseñas),
`aes-gcm`/`pbkdf2`/`sha2` (descifrado), `ed25519-dalek` o `minisign-verify`,
`zbus` (D-Bus), `fuzzy-matcher` (selector Ctrl+K).

## 4. Equivalencias macOS → Linux
| macOS (original) | Linux (objetivo) |
|---|---|
| SwiftUI / AppKit, arrastre de la barra de título | AdwApplicationWindow, AdwHeaderBar, decoraciones del lado del cliente |
| WKWebView | WebKitWebView |
| WKWebsiteDataStore | WebKitNetworkSession persistente en rutas XDG |
| Pestañas | AdwTabView + AdwTabBar (arriba) o barra lateral vertical propia; AdwTabOverview |
| WKContentRuleList (Shield) | WebKitUserContentFilterStore: **mismo formato JSON de reglas** |
| Curtain (ocultar elementos) | WebKitUserStyleSheet + WebKitUserScript por dominio |
| Modo lectura | Mozilla Readability.js (Apache-2.0) inyectado como script |
| Keychain (Vault) | Secret Service (oo7/libsecret): GNOME Keyring, KWallet, portal en Flatpak |
| Importar contraseñas | Firefox (`logins.json` + `key4.db`), Chromium/Chrome/Brave/Edge (`Login Data`, clave en Secret Service/KWallet o "peanuts"), CSV |
| `~/Library/Application Support/Search/` | `$XDG_DATA_HOME/search` (+ config, cache, runtime) |
| Atajos con ⌘ | Ctrl; GtkShortcutsWindow con la lista |
| Split View | GtkPaned con dos WebKitWebView |
| Picture-in-Picture | PiP nativo si WebKitGTK lo ofrece; si no, ventana flotante sin decoraciones |
| Descargas | WebKitDownload, carpeta XDG de descargas, GtkFileDialog (portal) |
| Search.sdef (AppleScript) | Interfaz D-Bus `io.github.searchlinux.Search` (zbus) |
| Socket de pruebas (Bench) | Socket Unix en `$XDG_RUNTIME_DIR/search/bench.sock` |
| Updater (firma propia) | Desactivado en Flatpak; minisign/ed25519 para AppImage/tarball |
| Homebrew cask, DMG, notarización | Flathub, metainfo AppStream, flatpak-builder-lint |
| Navegador predeterminado | `.desktop` con `MimeType=x-scheme-handler/http;x-scheme-handler/https;text/html;` + `xdg-settings` |
| Icon/ | Iconos hicolor en SVG + versión simbólica |
| WKWebExtension + CRX | Según lo que ofrezca hoy WebKitGTK (se verifica en la Fase 0) |

ID de la app propuesto: `io.github.searchlinux.Search` (cámbialo si el usuario
tiene otro dominio).

## 5. Riesgos conocidos
1. **Extensiones de Chrome:** dependen de que WebKitGTK tenga soporte de
   WebExtensions usable. Se verifica en la Fase 0 y se implementa al final.
2. **Picture-in-Picture:** puede no existir de forma nativa en WebKitGTK.
3. **Barra de pestañas vertical:** AdwTabBar es horizontal; la vertical puede
   necesitar un widget propio enlazado al mismo AdwTabView.
4. **Importar contraseñas de Chromium:** el formato y el cifrado cambian entre
   versiones; hay que detectarlo y fallar de forma clara.

## 6. Fases
| # | Fase | Resultado |
|---|---|---|
| 0 | Análisis | PORTING.md con el mapa completo y las verificaciones |
| 1 | Esqueleto | Ventana, una WebView, campo de dirección y búsqueda |
| 2 | Pestañas y sesión | Pestañas, fijadas, Ctrl+K, restaurar sesión |
| 3 | Datos de navegación | Historial + autocompletado, marcadores, descargas |
| 4 | Contenido | Shield, Curtain, modo lectura |
| 5 | Vault | Contraseñas con Secret Service e importación |
| 6 | Vistas | Split View, Picture-in-Picture |
| 7 | Automatización | Socket de bench, D-Bus, tests headless, medición del arranque |
| 8 | Entrega | Flatpak, .desktop, metainfo, iconos, updater, CI |
| 9 | Extensiones | CRX3 y WebExtensions según lo verificado |
| 10 | Revisión | Auditoría completa y ISSUES.md |
