# FASE 6 — Split View y Picture-in-Picture (modelo rápido)

Eres un ingeniero de Rust y GNOME que porta el navegador "Search" de macOS a
Linux (Rust + GTK4 + libadwaita + WebKitGTK 6.0).

## Lee primero
`AGENTS.md`, `PLAN.md`, `PORTING.md`, `PROGRESO.md`, `README.md` y
`git log --oneline`. Sigue la arquitectura existente.
Referencia: el código de Split View y Picture-in-Picture de
`upstream/Sources/Search/` y `upstream/Tests/split_view.py`.

Haz SOLO la Fase 6.

## Tareas
1. **Split View** (`split_view.rs`)
   - Una pestaña puede contener dos `BrowserView` lado a lado en un GtkPaned,
     igual que el original (mismos atajos, con Ctrl en vez de ⌘).
   - Formas de abrirlo: atajo, menú contextual de la pestaña ("abrir al lado") y
     menú contextual de un enlace ("abrir enlace al lado").
   - El panel con foco tiene un borde sutil; el campo de dirección y los atajos
     actúan sobre él. Atajo para cambiar el foco entre paneles.
   - Arrastra el divisor para redimensionar; doble clic lo devuelve a 50/50.
   - Cerrar un panel deja la pestaña con el otro. "Separar" convierte cada panel
     en una pestaña.
   - La pestaña muestra los dos favicons.
   - Se guarda y se restaura con la sesión (amplía el esquema de `session.rs` de
     forma compatible y añade tests).
2. **Picture-in-Picture** (`pip.rs`)
   - Sigue lo que decidió PORTING.md en la Fase 0:
     - Si WebKitGTK tiene PiP nativo: actívalo y conéctalo al mismo atajo/menú
       que el original.
     - Si no: crea una ventana flotante pequeña sin decoraciones que muestre el
       vídeo (por ejemplo, con una WebView mínima que reciba el stream o el
       elemento), con controles de reproducir/pausa y cerrar, que se pueda mover
       y redimensionar y que conserve la relación de aspecto. En Wayland no se
       puede forzar "siempre encima": documenta esa limitación.
     - Si ninguna opción es viable con calidad, deja la función desactivada y
       explica el motivo en PORTING.md y en el README. No entregues algo a medias.
   - Añade "Picture-in-Picture" al menú contextual de los vídeos.

## Criterios de cierre
- Tests de serialización de la sesión con Split View.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`
  y `cargo build --release` pasan.
- Prueba manual: Split View con dos sitios, reinicia la app, se restaura; PiP con
  un vídeo de YouTube.
- PORTING.md, PROGRESO.md y README actualizados.
- Commit: `Fase 6: Split View y Picture-in-Picture`.

Detente y dame un resumen y los pasos para probarlo.
