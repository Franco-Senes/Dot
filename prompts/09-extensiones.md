# FASE 9 — Extensiones de Chrome (modelo potente)

Eres un ingeniero senior de Rust, GNOME y WebKit que porta el navegador "Search"
de macOS a Linux (Rust + GTK4 + libadwaita + WebKitGTK 6.0).

## Lee primero
`AGENTS.md`, `PLAN.md`, `PORTING.md` (sobre todo lo que verificaste en la Fase 0
sobre WebExtensions), `PROGRESO.md`, `README.md` y `git log --oneline`.
Referencia: `upstream/Sources/Search/Crx.swift`, `Extensions.swift`,
`ExtensionShims.swift` y `ExtensionNative.swift`.

Haz SOLO la Fase 9. Es la fase de más riesgo: la honestidad va por delante de
las promesas.

## Paso 1: verificar otra vez (obligatorio)
Consulta la documentación y el changelog actuales de WebKitGTK y del crate
`webkit6`: ¿hay una API pública para cargar WebExtensions (p. ej. algo como
WebKitWebExtensionController) en la versión que usamos o en el runtime de GNOME
del Flatpak? ¿Qué APIs de `chrome.*` / `browser.*` admite? Anota la respuesta y
la fuente en PORTING.md. Ojo: las "web process extensions" de WebKitGTK son
otra cosa (módulos nativos) y **no** son extensiones de navegador.

## Paso 2A: si hay una API usable
1. **CRX3** (`crx.rs`): descarga desde un enlace de la Chrome Web Store (misma
   URL de descarga que usa el original), parsea el encabezado CRX3 (protobuf),
   **verifica las firmas** (RSA/ECDSA, igual que `Crx.swift`) y el ID de la
   extensión, y extrae el ZIP de forma segura (sin path traversal, con límites de
   tamaño).
2. **Instalación** (`extensions.rs`): pega o abre un enlace de la Chrome Web Store
   → muestra nombre, icono y permisos que pide → el usuario confirma → se instala
   en `$XDG_DATA_HOME/search/extensions/<id>/`.
3. **Gestión**: panel para activar, desactivar, quitar, ver permisos y abrir las
   opciones. Iconos de las extensiones discretos (como el original, sin barra de
   herramientas: menú o popover).
4. **Shims** (`extension_shims.rs` + JS): porta los shims de
   `ExtensionShims.swift` para las APIs de Chrome que WebKitGTK no tenga.
5. **Mensajería nativa** (`ExtensionNative.swift`): si el original la admite,
   busca los manifiestos de native messaging en las rutas de Linux
   (`~/.config/google-chrome/NativeMessagingHosts/`, etc.). En Flatpak, documenta
   la limitación.
6. Prueba con 3 extensiones populares (p. ej. un gestor de contraseñas, uBlock
   Origin Lite y un traductor) y anota qué funciona.

## Paso 2B: si NO hay una API usable
1. Porta igualmente `crx.rs` completo (descarga, parseo, verificación de firma y
   extracción segura) con tests, porque es código valioso y reutilizable.
2. Deja la interfaz de gestión preparada pero oculta tras un flag de compilación
   o un ajuste experimental desactivado.
3. Explica el estado con claridad en el README ("las extensiones de Chrome no
   están disponibles todavía porque…") y en PORTING.md, con el enlace al issue
   o a la hoja de ruta de WebKitGTK correspondiente.
4. Si PORTING.md lo propone, documenta alternativas (p. ej. user scripts para
   casos concretos), pero **no** inventes un sistema de extensiones propio.

## Criterios de cierre
- Tests de CRX3: firma válida, firma inválida, ID que no coincide, ZIP con path
  traversal, tamaño excesivo (genera los datos en los propios tests).
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`
  y `cargo build --release` pasan.
- PORTING.md, PROGRESO.md y README actualizados.
- Commit: `Fase 9: extensiones`.

Detente y dime con claridad qué opción (2A o 2B) aplicaste y por qué.
