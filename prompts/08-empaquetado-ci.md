# FASE 8 — Empaquetado, actualizaciones y CI (modelo rápido)

Eres un ingeniero de Rust y GNOME que porta el navegador "Search" de macOS a
Linux (Rust + GTK4 + libadwaita + WebKitGTK 6.0).

## Lee primero
`AGENTS.md`, `PLAN.md`, `PORTING.md`, `PROGRESO.md`, `README.md` y
`git log --oneline`.
Referencia: `upstream/Sources/Search/Updater.swift`, `upstream/build.sh`,
`upstream/publish.sh`, `upstream/Icon/`, `upstream/Installer/` y
`upstream/.github/`.

Haz SOLO la Fase 8.

## Tareas
1. **Archivos de escritorio** (en `data/`, instalados por Meson)
   - `<ID>.desktop` con `MimeType=text/html;x-scheme-handler/http;x-scheme-handler/https;`,
     `StartupWMClass`, acciones "Ventana nueva" y "Ventana privada".
   - `<ID>.metainfo.xml` (AppStream): descripción, capturas de pantalla (hazlas y
     guárdalas en `data/screenshots/`), licencia MIT, `content_rating` OARS,
     notas de versión, URL del repositorio y créditos al original.
   - Esquema GSettings completo con todos los ajustes.
   - Iconos: convierte `upstream/Icon/` a `hicolor/scalable/apps/<ID>.svg` y
     `hicolor/symbolic/apps/<ID>-symbolic.svg`. Si el original solo tiene PNG o
     .icns, haz un SVG fiel a partir de ellos.
2. **Navegador predeterminado**: en los ajustes, un botón "Hacer navegador
   predeterminado" que ejecute `xdg-settings set default-web-browser <ID>.desktop`
   (o use el portal, si existe uno adecuado) y muestre el estado actual.
3. **Flatpak** (`build-aux/flatpak/<ID>.json` o `.yml`)
   - Runtime y SDK `org.gnome.Platform`/`org.gnome.Sdk` estable actual (debe
     incluir WebKitGTK 6.0 y la versión de libadwaita necesaria), extensión
     `org.freedesktop.Sdk.Extension.rust-stable`.
   - Dependencias de Cargo vendorizadas con `flatpak-cargo-generator.py`
     (`cargo-sources.json`).
   - Permisos mínimos: `--share=network`, `--share=ipc`, `--socket=wayland`,
     `--socket=fallback-x11`, `--device=dri`, `--socket=pulseaudio`,
     `--talk-name=org.freedesktop.secrets`, `--filesystem=xdg-download`.
     Nada de `--filesystem=home` ni `host`. Justifica cada permiso en PORTING.md.
   - `scripts/flatpak-local.sh` para compilar e instalar en local.
4. **Updater** (`updater.rs`), igual que el original pero adaptado:
   - Si se ejecuta dentro de Flatpak (existe `/.flatpak-info`): desactivado,
     actualiza Flathub.
   - En AppImage/tarball: comprobación diaria (desactivable) de un manifiesto
     firmado con minisign o ed25519. Verifica la firma antes de mostrar nada.
     Nunca descarga ni instala sin que el usuario lo confirme.
   - Documenta en el README cómo generar las claves y firmar una versión.
5. **Otros formatos (opcional, si da tiempo)**: AppImage con `linuxdeploy` y
   paquetes .deb/.rpm con `cargo-deb`/`cargo-generate-rpm`. Si no los haces,
   déjalo anotado como pendiente.
6. **CI** (`.github/workflows/`)
   - `ci.yml`: en cada push y PR, `cargo fmt --check`, `cargo clippy --all-targets
     -- -D warnings`, `cargo test` y los tests headless, dentro de un contenedor
     con el SDK de GNOME (p. ej. `ghcr.io/flathub-infra/flatpak-github-actions:gnome-<versión>`)
     o instalando las dependencias en Ubuntu.
   - `flatpak.yml`: compila el Flatpak, ejecuta `flatpak-builder-lint` y
     `appstreamcli validate`, y sube el `.flatpak` como artefacto.
   - `release.yml`: al crear un tag `v*`, publica el `.flatpak` (y los otros
     formatos si existen) en GitHub Releases.
7. **README**: instalación (Flatpak, desde el código fuente con Meson),
   capturas, atajos, privacidad, créditos al original y licencia.

## Criterios de cierre
- `scripts/flatpak-local.sh` produce un Flatpak que se instala y se abre.
- `flatpak-builder-lint` y `appstreamcli validate` sin errores.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`
  y `cargo build --release` pasan.
- PORTING.md, PROGRESO.md y README actualizados.
- Commit: `Fase 8: empaquetado, updater y CI`.

Detente y dame un resumen y los pasos para instalar el Flatpak.
