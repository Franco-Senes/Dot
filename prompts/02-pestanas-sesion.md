# FASE 2 — Pestañas y sesión (modelo potente)

Eres un ingeniero senior de Rust y GNOME que porta el navegador "Search" de macOS
a Linux (Rust + GTK4 + libadwaita + WebKitGTK 6.0).

## Lee primero
`AGENTS.md`, `PLAN.md`, `PORTING.md`, `PROGRESO.md`, `README.md` y
`git log --oneline`. Revisa el código actual antes de cambiar nada.
Comportamiento de referencia: `upstream/Sources/Search/Session.swift` y todo lo
relacionado con pestañas en `upstream/Sources/Search/`.

Haz SOLO la Fase 2.

## Tareas
1. **Pestañas**
   - AdwTabView como modelo único; cada página es un `BrowserView`.
   - Dos disposiciones, igual que el original: pestañas **arriba** (AdwTabBar
     integrada en la cabecera) o en una **barra lateral izquierda vertical**
     (widget propio enlazado al mismo AdwTabView: favicon + título, fijadas
     arriba, arrastrar para reordenar). Se cambia en los ajustes sin reiniciar.
   - Título, favicon (WebKitFaviconDatabase), indicador de carga y de audio
     (con clic para silenciar) en cada pestaña.
2. **Atajos**: Ctrl+T nueva, Ctrl+W cerrar, Ctrl+Shift+T reabrir la última
   cerrada (pila de al menos 20), Ctrl+Tab / Ctrl+Shift+Tab y
   Ctrl+RePág / Ctrl+AvPág, Ctrl+1…8 ir a la pestaña, Ctrl+9 la última,
   Ctrl+N ventana nueva, Ctrl+Shift+N ventana privada (WebKitNetworkSession
   efímero, sin historial). Clic central cierra la pestaña; clic central en un
   enlace la abre en segundo plano.
3. **Pestañas fijadas**: fijar/desfijar desde el menú contextual de la pestaña,
   se muestran compactas y no se cierran con Ctrl+W (como el original; compruébalo).
4. **Selector Ctrl+K**: popup centrado con búsqueda difusa (`fuzzy-matcher`) por
   título y URL de las pestañas abiertas de todas las ventanas. Flechas para
   moverse, Enter para ir, Ctrl+W/Supr para cerrar la marcada, Esc para salir.
   Copia el comportamiento del original.
5. **Enlaces nuevos**: `target=_blank` y `window.open` → pestaña nueva
   relacionada (señal `create` de WebKitWebView, conservando el opener cuando
   haga falta para popups de login OAuth).
6. **Sesión** (`session.rs`):
   - Guarda ventanas → pestañas (URL, título, fijada, orden, activa, disposición)
     en `$XDG_DATA_HOME/search/session.json`, con el esquema del original
     siempre que sea posible.
   - Escritura atómica con debounce (~1 s) y también al cerrar.
   - Al arrancar, restaura todo. Las pestañas no activas se cargan de forma
     diferida (solo al seleccionarlas) para que el arranque sea rápido.
   - Si el JSON está corrupto: guarda una copia `.bak`, arranca limpio y registra
     el error.
   - Las ventanas privadas no se guardan nunca.
7. **Menú contextual de la pestaña**: recargar, duplicar, fijar, silenciar,
   cerrar, cerrar las demás, mover a ventana nueva.

## Criterios de cierre
- Tests unitarios de la serialización/deserialización de la sesión, incluido el
  caso de JSON corrupto.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`
  y `cargo build --release` pasan.
- Prueba manual: abre 15 pestañas, fija 2, cierra la app y vuelve a abrir: todo
  se restaura y el arranque sigue siendo rápido.
- PORTING.md, PROGRESO.md y README actualizados.
- Commit: `Fase 2: pestañas y sesión`.

Detente y dame un resumen y los pasos para probarlo.
