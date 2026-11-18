# FASE 4 — Shield, Curtain y modo lectura (modelo rápido)

Eres un ingeniero de Rust y GNOME que porta el navegador "Search" de macOS a
Linux (Rust + GTK4 + libadwaita + WebKitGTK 6.0).

## Lee primero
`AGENTS.md`, `PLAN.md`, `PORTING.md`, `PROGRESO.md`, `README.md` y
`git log --oneline`. Sigue la arquitectura existente.
Referencia: `upstream/Sources/Search/Shield.swift`, `Curtain.swift` y el código
del modo lectura del original.

Haz SOLO la Fase 4.

## Tareas
1. **Shield: bloqueo de anuncios y rastreadores** (`shield.rs`)
   - Usa las mismas listas de reglas que el original (búscalas en `upstream/`;
     si se descargan, usa las mismas fuentes). El formato JSON de
     WKContentRuleList es el mismo que el de WebKitUserContentFilterStore, así
     que reutilízalas tal cual.
   - Compílalas con WebKitUserContentFilterStore en `$XDG_CACHE_HOME/search/filters`
     de forma asíncrona y recompila solo si cambia el hash de la lista. El
     arranque no debe esperar a la compilación.
   - Aplica el filtro a todos los WebKitUserContentManager (también en ventanas
     privadas).
   - Excepciones por sitio (desactivar Shield en este dominio) desde el menú de
     la página, guardadas en JSON.
   - Si el original muestra un contador de bloqueos, impleméntalo igual.
2. **Curtain: ocultar elementos** (`curtain.rs` + `resources/curtain.js`)
   - Modo selección (mismo atajo que el original, con Ctrl): al pasar el ratón
     se resalta el elemento; clic para ocultarlo; Esc para salir.
   - Genera un selector CSS estable y corto (prioriza id, atributos estables y
     clases que no parezcan generadas; evita nth-child cuando se pueda). Copia
     la lógica del original si existe.
   - El JS se comunica con Rust mediante un message handler de
     WebKitUserContentManager.
   - Reglas por dominio en `$XDG_DATA_HOME/search/curtain.json`, aplicadas con
     WebKitUserStyleSheet (`display: none !important`) desde el inicio de la
     carga, sin parpadeo.
   - Panel para ver y borrar las reglas del sitio actual o de todos.
3. **Modo lectura** (`reader.rs`)
   - Incluye Mozilla Readability.js en `third_party/readability/` con su
     licencia Apache-2.0 y la versión anotada.
   - Al activarlo (mismo atajo que el original, con Ctrl), inyecta Readability
     en un mundo aislado (script world), obtén el artículo y muéstralo con una
     plantilla HTML+CSS propia: tipografía legible, ancho máximo ~70 caracteres,
     tema claro/oscuro según el sistema, tamaño de letra ajustable.
   - Si la página no es un artículo, avisa con discreción en lugar de fallar.
   - Volver a pulsar el atajo regresa a la página original.
4. Los scripts JS van en `resources/` y se incluyen con GResource o
   `include_str!`.

## Criterios de cierre
- Pruébalo en al menos 4 sitios reales (un periódico, un blog, un sitio con
  muchos anuncios, YouTube) y anota en PORTING.md qué se bloqueó y si algo se rompió.
- Tests del generador de selectores (con HTML de prueba) y del caché de filtros.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`
  y `cargo build --release` pasan.
- PORTING.md, PROGRESO.md y README actualizados.
- Commit: `Fase 4: Shield, Curtain y modo lectura`.

Detente y dame un resumen y los pasos para probarlo.
