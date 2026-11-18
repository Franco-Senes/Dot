# FASE 7 — Bench, D-Bus y tests automáticos (modelo rápido)

Eres un ingeniero de Rust y GNOME que porta el navegador "Search" de macOS a
Linux (Rust + GTK4 + libadwaita + WebKitGTK 6.0).

## Lee primero
`AGENTS.md`, `PLAN.md`, `PORTING.md`, `PROGRESO.md`, `README.md` y
`git log --oneline`. Sigue la arquitectura existente.
Referencia: `upstream/Sources/Search/Bench.swift`, el script `upstream/bench`,
`upstream/test-find`, `upstream/Search.sdef`, `upstream/Tests/` y
`upstream/skill/search-bench/`.

Haz SOLO la Fase 7.

## Tareas
1. **Socket de bench** (`bench.rs`)
   - Socket Unix en `$XDG_RUNTIME_DIR/search/bench.sock` con permisos 0600.
   - **Desactivado por defecto**; se activa en los ajustes (igual que el original)
     o con la variable `SEARCH_BENCH=1` para los tests.
   - Implementa **el mismo protocolo y los mismos comandos** que el original
     (documentados en PORTING.md en la Fase 0). Las respuestas deben ser
     idénticas para que los scripts sirvan sin cambios.
   - Usa el bucle principal de GLib de forma asíncrona; no bloquees la interfaz.
2. **Scripts**: porta `bench` y `test-find` a `scripts/` para que funcionen en
   Linux (`socat`/`nc -U` o Python sin dependencias). Mismo uso que el original.
3. **D-Bus** (`dbus.rs`, con `zbus`)
   - Interfaz `io.github.searchlinux.Search1` (o el ID de PORTING.md) con los
     equivalentes útiles de `Search.sdef`: abrir URL (en pestaña nueva o actual),
     listar ventanas y pestañas (id, título, URL), activar pestaña, cerrar pestaña,
     recargar, obtener la URL actual.
   - La app debe ser instancia única: `search https://…` desde la terminal abre
     una pestaña en la ventana existente (GApplication con HANDLES_OPEN).
   - Documenta la interfaz en el README con ejemplos de `busctl` / `gdbus`.
4. **Tests headless**
   - Porta `upstream/Tests/split_view.py` a `tests/` para que se ejecute sin
     pantalla: con `weston --backend=headless` (Wayland) o `xvfb-run` (X11),
     hablando con la app por el socket de bench.
   - Añade un test de integración básico: abrir, navegar a una página local
     (servidor HTTP de prueba en el test), crear pestañas, guardar sesión y
     comprobar la restauración.
   - `scripts/test-headless.sh` ejecuta todo.
5. **Rendimiento**
   - Mide el arranque en frío (hasta la primera página pintada) y la memoria
     con 1, 10 y 20 pestañas. Usa bench para medir.
   - Objetivo: arranque < ~300 ms. Si no se cumple, perfila (`perf`,
     `tracing` con tiempos) y optimiza lo obvio.
   - Anota los resultados y el hardware en PORTING.md.

## Criterios de cierre
- `scripts/test-headless.sh` pasa en local.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`
  y `cargo build --release` pasan.
- PORTING.md, PROGRESO.md y README actualizados.
- Commit: `Fase 7: bench, D-Bus y tests`.

Detente y dame un resumen, las cifras de rendimiento y los pasos para probarlo.
