# REPORTE-FINAL.md — Port autónomo de Search a Linux

Port nativo en **Rust + GTK4/libadwaita + CEF 156 (`Chromium 156.0.8078.96`)** del navegador **Search** (originalmente Swift/WebKit en macOS, licencia MIT) ubicado en `/home/franco/search-linux-2`.

---

## 1. Resumen de fases y commits

Todas las fases (`0` a `10`, más `0b` y `3b`) se completaron y verificaron en la rama `main`:

| Fase | Alcance principal | Estado | Commit(s) |
|---|---|---|---|
| **Kit inicial** | Copia del kit y clonado de `upstream/` (171 archivos Swift) | Completada | `c146324` |
| **Fase 0** | Análisis completo de `upstream/`, esquemas JSON, atajos, `PORTING.md` y `BLOQUEOS.md` | Completada | `549688e` |
| **Fase 0b** | Cambio de motor a **CEF precompilado** (`156.0.30` / Chromium `156.0.8078.96`) y auditoría binaria de `libcef.so` | Completada | `9a2c71c` |
| **Fase 1** | Workspace Cargo (`gnome-platform`, `search-core`, `search`, `search-bench`, `xtask`), `chromium_flags.rs`, OSR `BrowserView` y barra de direcciones | Completada | `e8da83e` |
| **Fase 2** | `AdwTabView`, barra superior + barra lateral vertical (`Ctrl+S`/`Ctrl+Shift+S`), carga diferida, `Ctrl+K`, ventanas privadas, recuperación atómica `.bak` | Completada | `ca65ca3` |
| **Fase 3** | Historial (`Ctrl+H`/`Ctrl+Y`), autocompletado `<0.35 ms` en 50k entradas, marcadores (`Ctrl+D`, `Ctrl+Shift+O`, Netscape HTML), descargas (`CefDownloadHandler`, `GNotification`) | Completada | `285a7cd` |
| **Fase 3b** | `plasma://settings` como pestaña nativa (0 procesos CEF), rasterizador vectorial de iconos en Rust (`icons.rs`), favicons `as_bitmap`, carpetas de pines estilo Arc (`pin-folders.json`), pulido UI y barra lateral redimensionable con el mouse | Completada | `28f75de`, `f7030c2` |
| **Fase 4** | Shield (58 reglas embebidas + YouTube + caché SHA-256), Curtain (selector CSS en Rust + `curtain.json`), Modo Lectura (`Ctrl+Alt+R`/`F9`, sanitizador HTML) | Completada | `922a32a` (`c695e73`) |
| **Fase 5** | Vault sobre `libsecret-1` + `zeroize`, coincidencia exacta de origin, auto-limpieza de portapapeles a 30s, importadores Firefox (`key4.db`)/Chromium (`v10`/`v11`)/CSV con tests sintéticos | Completada | `b1115b7` (`b436964`) |
| **Fase 6** | Split View (`GtkPaned`, 50/50 doble clic, favicons dobles, persistencia en `session.json`) y Picture-in-Picture (`GtkWindow` 16:9 flotante) | Completada | `1598767` (`f0dfd25`) |
| **Fase 7** | Socket `$XDG_RUNTIME_DIR/search/bench.sock` (`0600`, `SO_PEERCRED`), CLI `bench`/`test-find`, D-Bus `io.github.searchlinux.Search1`, tests headless 100% Rust (`scripts/test-headless.sh`) y benchmarks en `PORTING.md` §12 | Completada | `ec307be`, `9738c46` (`ffbdb5f`) |
| **Fase 8** | `.desktop`, AppStream `.metainfo.xml`, iconos SVG, botón navegador predeterminado (`xdg-settings`), `updater.rs` (Ed25519 + desactivado en Flatpak), manifiesto Flatpak y CI GitHub Actions | Completada | `ec307be`, `83163b2` |
| **Fase 9** | `crx.rs` (CRX3 protobuf + RSA/ECDSA + extracción ZIP segura) y Paso 2B (`ExtensionStore` detrás de `extensions_enabled = false` en `plasma://settings`) | Completada | `ec307be`, `83163b2` |
| **Fase 10** | Auditoría de seguridad/privacidad/rendimiento (`REVISION.md`, `ISSUES.md`), corrección de 3 bugs Críticos (colisión IPC, ciclos `Rc`↔`GObject` con `WeakRef`, pánico reentrante `RefCell` en cierre) | Completada | `49b0d2d`, `c861014` |

---

## 2. Cómo probarlo y verificarlo

Todas las comprobaciones se ejecutan dentro del SDK autocontenido `.toolchain/sdk` (sin modificar el sistema anfitrión):

```bash
cd /home/franco/search-linux-2

# 1. Formato, clippy estricto y tests unitarios (149 tests: 14 gnome-platform, 119 search-core, 13 search, 3 search-bench)
./.toolchain/sdk cargo fmt --all -- --check
./.toolchain/sdk cargo clippy --workspace --all-targets -- -D warnings
./.toolchain/sdk cargo test --workspace

# 2. Compilación release
./.toolchain/sdk cargo build --workspace --release

# 3. Pruebas de integración headless (Xvfb + D-Bus + TestHttpServer en Rust puro)
./.toolchain/sdk-xvfb bash scripts/test-headless.sh

# 4. Ejecutar el navegador en escritorio (ejemplo abriendo plasma://settings)
./.toolchain/sdk ./target/release/search plasma://settings
```

---

## 3. Qué quedó deshabilitado o bloqueado (`BLOQUEOS.md`)

1. **Dependencias en el host Solus 4.9 Serenity:** `scripts/instalar-dependencias.sh` no soporta `eopkg` y `sudo` requiere contraseña. Se resolvió de forma autónoma instalando el SDK de GNOME 51 (`GTK 4.24.1`, `libadwaita 1.10.0`, `libsecret 0.21.8.2`, `sqlite3 3.53.4`, `openssl 3.5.9`), Rust `1.98.0` y CEF `156.0.30` dentro de `.toolchain/`.
2. **Códecs patentados (H.264/AAC) y Widevine:** La distribución minimal precompilada de CEF 156 incluye **AV1, VP9, VP8, Opus, Vorbis y FLAC**, pero no incluye decodificadores H.264/AAC ni el binario propietario `libwidevinecdm.so`.
3. **Extensiones MV3 en tiempo de ejecución (Fase 9 — Paso 2B):** En CEF 156, `windowless_rendering_enabled` exige `CEF_RUNTIME_STYLE_ALLOY` (sin `CefExtension` ni service workers MV3 sin exponer la UI de Chrome). La verificación CRX3 e instalación segura en disco quedaron implementadas tras `extensions_enabled = false` en `plasma://settings`.
4. **Always-on-top en PiP bajo Wayland:** Wayland no permite que una ventana cliente fuerce `always-on-top` sin protocolo de compositor; documentado en `PORTING.md`.
5. **Pruebas en sitios reales de internet y Flatpak CI:** El sandbox no tiene red ni permite `bwrap` anidado (`flatpak-builder`), por lo que Shield/Curtain/Reader se validaron con suites unitarias y fixtures HTML locales, y `.github/workflows/flatpak.yml` quedó en `workflow_dispatch` con un TODO en `PORTING.md` §8 para rellenar `cargo-sources.json` y el SHA-256 final de CEF.

---

## 4. Decisiones arquitectónicas tomadas de forma autónoma

- **CEF 156 precompilado (`Alloy` + OSR) sobre GTK4/libadwaita:** Evita compilar Chromium desde cero, funciona de forma homogénea en X11 y Wayland pintando sobre `GtkPicture` + `GdkMemoryTexture`, y permite múltiples vistas simultáneas (pestañas, Split View y PiP).
- **Workspace 100 % Rust sin Python/C/Vala ni Meson:** Incluso las herramientas `bench`, `test-find` y el servidor HTTP de pruebas (`crates/search-bench`) son Rust puro. Solo se usa JS mínimo dentro de la página para recolectores de Curtain, Vault y extracción Reader, autenticados por token efímero de 128 bits.
- **Crate propia `crates/gnome-platform`:** Envoltorio FFI seguro y 100 % offline sobre `libgtk-4`, `libadwaita-1`, `libgio-2.0` (incluyendo `GDBus`), `libsecret-1`, `libsqlite3` y `libcrypto`, evitando dependencias externas no cacheadas.
- **Iconos vectoriales en Rust (`crates/search/src/icons.rs`):** Evita que iconos simbólicos desaparezcan en escritorios KDE Plasma/Breeze o entornos sin `glycin`/`gdk-pixbuf`.
- **Página nativa `plasma://settings`:** Pestaña integrada en `AdwTabView` sin lanzar procesos CEF adicionales.

---

## 5. Rendimiento y mediciones de flags de Chromium (`PORTING.md` §12)

Medido con binario `--release` bajo Xvfb (`1280×800`) en **AMD Ryzen 5 5600G with Radeon Graphics**:

- **Arranque en frío (línea base con `--renderer-process-limit=6`):** `435 ms` ventana / `500 ms` 1.er fotograma OSR (`517 ms` carga).
- **Consumo RAM del árbol de procesos:**
  - `1` pestaña despierta: **`885,7 MiB`** (`8` procesos)
  - `10` pestañas despiertas: **`1.808,2 MiB`** (`15` procesos)
  - `20` pestañas despiertas: **`3.028,1 MiB`** (`25` procesos)
  - Restauración diferida (`active_cef = 1`): **`~882 MiB`**
- **Flags evaluados individualmente:**
  - `--renderer-process-limit=4` (`427 ms` / `881,9 MiB` 1 tab) y `=8` (`428 ms` / `882,2 MiB`): diferencia `<0,3%`, se mantiene **`6`** por defecto.
  - `--process-per-site`, `performance_tuning.high_efficiency_mode.state` y `--js-flags=--lite-mode`: ahorro `<0,2%` o penalización de arranque (`+14 ms` en `--lite-mode`); quedan como **opcionales en `plasma://settings`**.
  - **VA-API** (`AcceleratedVideoDecodeLinuxGL`, `AcceleratedVideoDecodeLinuxZeroCopyGL`, `VaapiIgnoreDriverChecks`): opcional (`hardware_video_decode`), desactivado por defecto para evitar pantallas negras en drivers incompatibles.
  - **Site Isolation:** siempre activo y protegido frente a `SEARCH_CHROMIUM_FLAGS`.

---

## 6. Comandos que requieren `sudo` o ejecución en el host

Si querés compilar o empaquetar directamente en el sistema anfitrión **Solus 4.9 Serenity** fuera de `.toolchain/`:

```bash
sudo eopkg update-repo
sudo eopkg install -c system.devel
sudo eopkg install git rust cargo libgtk-4-devel libadwaita-devel webkitgtk-6.0-devel \
  libsecret-devel sqlite3-devel openssl-devel flatpak-builder xorg-server-xvfb weston socat
```

Para construir el Flatpak localmente en el host (fuera del sandbox sin `bwrap` anidado):

```bash
bash scripts/flatpak-local.sh
```

---

## 7. Siguientes pasos recomendados antes de publicar

1. Atender los ítems no críticos priorizados en [`ISSUES.md`](ISSUES.md):
   - `[A-1]` Mover `write_atomic`, importación HTML y `install_crx_file` a un hilo secundario para evitar `fsync` en el hilo principal GTK.
   - `[A-2]` Evitar el escaneo completo `any(|&b| b != 0)` en `on_paint` cuando ya existen fotogramas no vacíos.
   - `[M-1]` y `[M-2]` Desactivar captura/`vault_never.json` de Vault en ventanas privadas y revalidar `main_frame` en `vault_fill`.
2. Probar Shield, Curtain, Modo Lectura y VA-API (Radeon Vega) en sesión gráfica real con acceso a red sobre sitios reales (periódicos, YouTube, webapps).
3. Generar `cargo-sources.json` y fijar el SHA-256 definitivo del tarball de CEF en `build-aux/flatpak/io.github.searchlinux.Search.yml` para habilitar `.github/workflows/flatpak.yml`.
