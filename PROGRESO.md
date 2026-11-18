# PROGRESO.md

| Fase | Estado | Commit | Fecha | Notas |
|---|---|---|---|---|
| 0 — Análisis | ✅ | *(este commit)* | 2026-11-18 | Análisis de los 171 archivos Swift de upstream, verificaciones de WebKitGTK 2.54 / GTK4 / libadwaita 1.10 / oo7 y arquitectura Cargo workspace 100 % Rust en `PORTING.md`. |
| 1 — Esqueleto | ⬜ | | | |
| 2 — Pestañas y sesión | ⬜ | | | |
| 3 — Historial, marcadores, descargas | ⬜ | | | |
| 4 — Shield, Curtain, modo lectura | ⬜ | | | |
| 5 — Vault | ⬜ | | | |
| 6 — Split View y PiP | ⬜ | | | |
| 7 — Bench, D-Bus, tests | ⬜ | | | |
| 8 — Empaquetado y CI | ⬜ | | | |
| 9 — Extensiones | ⬜ | | | |
| 10 — Revisión final | ⬜ | | | |

Leyenda: ⬜ pendiente · 🟡 en curso · ✅ hecha · ⚠️ hecha con pendientes

## Pendientes que arrastran las fases
- En el host Solus 4.9 Serenity faltan los paquetes `-devel` nativos (`sudo -n true` requiere contraseña); se utiliza un entorno GNOME SDK 51 + Rust 1.98 aislado en `.toolchain/` (documentado en `BLOQUEOS.md`).

---

## Resúmenes por fase

### Fase 0 — Análisis
- **Resumen:** Se analizaron los **171 archivos Swift** (`99.047` líneas) y scripts de `upstream/`, documentando cada módulo, esquemas JSON (`session.json`, `history.json`, `bookmarks.json`, `downloads.json`, `curtain.json`, `Shield`), atajos con `Ctrl`, ajustes y protocolo completo de `bench.sock` en `PORTING.md`. Se verificó contra documentación actual y cabeceras del SDK (`org.gnome.Sdk//51`): WebKitGTK `2.54.2`/`2.54.1` (`webkit6 0.6.1`), `gtk4 0.11.5`, `libadwaita 0.9.2` (`1.10.0`) y `oo7 0.7.0`.
- **Decisiones adoptadas autónomamente (registradas en `PORTING.md` §11):**
  1. **Workspace Cargo 100 % Rust (`crates/search`, `crates/search-core`, `crates/search-bench`, `crates/xtask`) sin Meson ni Python:** cumple el requisito explícito del usuario y permite tests unitarios puros + empaquetado Flatpak `buildsystem: simple`.
  2. **ID de aplicación `io.github.searchlinux.Search`** y persistencia híbrida (`settings.json` atómico + esquema GSettings).
  3. **Modelo único `AdwTabView`** compartido por `AdwTabBar` (arriba) y `SidebarTabs` (barra lateral vertical).
  4. **Picture-in-Picture** con ventana flotante propia sin decoraciones (como `Float.swift`), dado que WebKitGTK carece de PiP nativo.
  5. **Extensiones de Chrome (Fase 9, Paso 2B):** WebKitGTK 2.54 solo expone `WebKitWebExtension` (manifiesto) sin `WebKitWebExtensionController`, por lo que se portará `crx.rs` completo y se dejará la UI tras un ajuste experimental.
- **Funciones imposibles en Linux:** Integración con Apple Passwords (`PasswordManagerBrowserExtensionHelper`), Passkeys vía `AuthenticationServices`, traducción local con `Translation.framework` de macOS 15+, motor Mach-O `search-ai-engine` + verificación `SecCode`, `Contacts.framework` (`MeCard`), y ejecución completa de extensiones `WKWebExtensionController` hasta que WebKitGTK publique el controlador.
