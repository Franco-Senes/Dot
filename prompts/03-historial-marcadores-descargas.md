# FASE 3 — Historial, marcadores y descargas (modelo rápido)

Eres un ingeniero de Rust y GNOME que porta el navegador "Search" de macOS a
Linux (Rust + GTK4 + libadwaita + WebKitGTK 6.0).

## Lee primero
`AGENTS.md`, `PLAN.md`, `PORTING.md`, `PROGRESO.md`, `README.md` y
`git log --oneline`. Lee el código actual: **sigue su arquitectura, no la
reorganices**. Comportamiento de referencia: los archivos de historial,
marcadores, descargas y autocompletado de `upstream/Sources/Search/`.

Haz SOLO la Fase 3.

## Tareas
1. **Historial** (`history.rs`)
   - Guarda cada visita: URL, título, fecha de la última visita y número de
     visitas. Ignora ventanas privadas, `about:`, páginas de error y
     redirecciones intermedias.
   - Almacenamiento: el formato del original (JSON). Si PORTING.md decidió otra
     cosa por rendimiento (p. ej. SQLite con rusqlite), síguelo.
   - Ctrl+H abre el panel de historial agrupado por día, con búsqueda, abrir,
     borrar una entrada, borrar un sitio entero y "borrar todo / última hora /
     último día".
2. **Autocompletado del campo de dirección**
   - Sugerencias mientras escribes: historial (puntuación por frecuencia +
     recencia, como el original), pestañas abiertas ("cambiar a la pestaña") y
     marcadores.
   - Autocompleta en línea el dominio más probable (texto seleccionado tras el
     cursor), como el original. Retroceso lo quita.
   - Flechas para moverse, Enter para abrir, Shift+Supr para borrar una entrada
     del historial.
   - Debe responder en menos de 16 ms con 50.000 entradas: haz un test de rendimiento.
3. **Marcadores** (`bookmarks.rs`)
   - Ctrl+D añade o quita el marcador de la página actual (popover para editar
     título y carpeta).
   - Ctrl+Shift+O abre el panel: carpetas, búsqueda, editar, borrar, arrastrar
     para reordenar.
   - Importar y exportar en formato HTML de marcadores de Netscape (compatible
     con Firefox y Chrome).
4. **Descargas** (`downloads.rs`)
   - Señales de WebKitDownload. Destino: carpeta XDG de descargas (o preguntar
     siempre, según los ajustes, con GtkFileDialog).
   - Si el archivo existe, renombra (`archivo (1).pdf`), nunca sobrescribas.
   - Ctrl+Shift+Y abre el panel: progreso, velocidad, cancelar, reintentar,
     abrir y mostrar en la carpeta (portal OpenURI / `gtk::FileLauncher`).
   - Notificación del sistema al terminar si la ventana no tiene el foco.
5. **Presentación**: los paneles deben ser discretos (AdwOverlaySplitView lateral
   o popover), como en el original. No añadas botones permanentes a la cabecera.
6. Todo se guarda con `paths::write_atomic` y debounce.

## Criterios de cierre
- Tests de: puntuación del autocompletado, import/export de marcadores,
  renombrado de descargas y rendimiento con 50.000 entradas.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`
  y `cargo build --release` pasan.
- PORTING.md, PROGRESO.md y README actualizados.
- Commit: `Fase 3: historial, marcadores y descargas`.

Detente y dame un resumen y los pasos para probarlo.
