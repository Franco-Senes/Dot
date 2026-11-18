# FASE 5 — Vault: contraseñas (modelo potente)

Eres un ingeniero senior de Rust, GNOME y seguridad que porta el navegador
"Search" de macOS a Linux (Rust + GTK4 + libadwaita + WebKitGTK 6.0).

## Lee primero
`AGENTS.md`, `PLAN.md`, `PORTING.md`, `PROGRESO.md`, `README.md` y
`git log --oneline`. Sigue la arquitectura existente.
Referencia: `upstream/Sources/Search/Vault.swift` y el código de importación de
contraseñas del original.

Haz SOLO la Fase 5. Aquí la seguridad va antes que la comodidad.

## Tareas
1. **Almacenamiento** (`vault.rs`)
   - Secret Service con el crate `oo7` (funciona con GNOME Keyring, KWallet y el
     portal de secretos dentro de Flatpak). Alternativa: libsecret.
   - Cada credencial es un secreto con atributos: `app=search`, `origin`
     (esquema+host+puerto), `username`, fechas de creación y último uso.
   - Nunca escribas contraseñas en disco, en la sesión, en el historial ni en
     los logs. Pon a cero los buffers cuando sea posible (`zeroize`).
   - Si no hay Secret Service disponible, desactiva Vault y explícalo en la
     interfaz. No hay alternativa en texto plano.
2. **Captura y autocompletado** (`resources/vault.js`)
   - Detecta los envíos de formularios de login (incluidos los de SPA, con
     fetch/XHR tras pulsar el botón) y envía origin+usuario+contraseña a Rust
     con un message handler en un mundo de scripts aislado.
   - Barra discreta (AdwBanner o similar): "¿Guardar la contraseña de
     usuario@sitio?" → Guardar / Nunca en este sitio / Ahora no. Detecta
     cambios de contraseña y ofrece actualizar.
   - Al cargar una página con un formulario de login, ofrece las credenciales de
     ese origin (popover junto al campo, como el original). Solo para el mismo
     origin exacto y solo en el marco principal o en iframes del mismo origin.
     Nunca en HTTP sin cifrar sin avisar.
3. **Panel de contraseñas** (en los ajustes o un panel lateral, como el original)
   - Lista con búsqueda; mostrar la contraseña con confirmación (y, si se puede,
     desbloqueando el llavero); copiar (el portapapeles se borra a los 30 s);
     editar; borrar; exportar a CSV con advertencia clara.
4. **Importación** (`vault_import.rs`). Solo cuando el usuario lo pide, enseñando
   antes qué perfiles se han encontrado:
   - **Firefox**: perfiles de `~/.mozilla/firefox/profiles.ini` (y la ruta de
     Flatpak/Snap). Descifra `logins.json` con `key4.db` (NSS: PBKDF2 +
     AES-256-CBC o 3DES según la versión). Si hay contraseña principal, pídela.
   - **Chromium, Chrome, Brave, Edge, Vivaldi** (en `~/.config/…` y rutas de
     Flatpak/Snap): copia `Login Data` a un temporal y ábrelo con rusqlite. Las
     contraseñas `v10` usan la clave "peanuts"; las `v11`, la clave guardada en
     Secret Service/KWallet ("Chrome Safe Storage", etc.). AES-128-CBC con
     PBKDF2-SHA1 (salt "saltysalt", 1 iteración). Si la versión no se reconoce,
     falla con un mensaje claro y sugiere importar un CSV.
   - **CSV** exportado de Chrome, Firefox, Bitwarden o 1Password: es la vía
     recomendada si lo demás falla.
   - Al terminar: resumen con cuántas se importaron, duplicadas y fallidas. Borra
     los temporales.
   - Verifica cada detalle criptográfico con la documentación o el código fuente
     actual de cada navegador; no te fíes de estos parámetros sin comprobarlos.
5. **Flatpak**: anota en PORTING.md los permisos que hacen falta para leer los
   perfiles de otros navegadores (lo ideal es pedir la carpeta con el portal de
   archivos en vez de dar acceso permanente).

## Criterios de cierre
- Tests de: parser CSV, descifrado de Chromium y Firefox con **datos de prueba
  generados por los propios tests** (nunca datos reales), coincidencia de
  origins y que ninguna contraseña aparece en los logs.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`
  y `cargo build --release` pasan.
- Sección "Seguridad de Vault" en PORTING.md con el modelo de amenazas.
- PORTING.md, PROGRESO.md y README actualizados.
- Commit: `Fase 5: Vault y contraseñas`.

Detente y dame un resumen, los pasos para probarlo y cualquier riesgo de
seguridad que quede.
