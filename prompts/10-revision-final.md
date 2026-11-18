# FASE 10 — Revisión final (modelo potente)

Eres un revisor de código senior especializado en Rust, GNOME, WebKit y
seguridad. No escribiste este código: revísalo con ojos nuevos.

## Lee primero
`AGENTS.md`, `PLAN.md`, `PORTING.md`, `PROGRESO.md`, `README.md`,
`git log --oneline`, **todo** el código de `src/`, `resources/`, `data/`,
`build-aux/`, `tests/`, `scripts/` y `.github/`, y el original en `upstream/`.

No añadas funciones nuevas.

## Revisa
1. **Paridad con el original**: recorre cada archivo de `upstream/Sources/Search/`
   y cada función del README original. ¿Falta algo? ¿Se comporta distinto?
   ¿Hay atajos que no coinciden?
2. **Corrección**: bugs, `unwrap()`/`expect()`/`panic!` en rutas que dependan del
   usuario, la red o el disco; condiciones de carrera; fugas de memoria (ciclos
   de referencias con closures de GTK sin `glib::clone!` débil); señales que no
   se desconectan.
3. **Privacidad**: cualquier petición de red no solicitada (revísalo con
   `WEBKIT_DEBUG`/tracing o un proxy), datos de navegación privada que lleguen al
   disco, datos sensibles en logs.
4. **Seguridad**: Vault (contraseñas en memoria, disco, portapapeles, logs),
   coincidencia de origins del autocompletado, scripts inyectados (¿están en un
   mundo aislado?, ¿pueden las páginas falsificar mensajes a Rust?), CRX3,
   updater, socket de bench (permisos, ¿está apagado por defecto?), D-Bus.
5. **Flatpak**: permisos que sobran; ¿funciona Secret Service?; ¿funciona el
   portal de archivos?
6. **Rendimiento**: arranque en frío, memoria con 20 pestañas, autocompletado
   con mucho historial, trabajo pesado en el hilo principal.
7. **Calidad**: código muerto, `TODO` olvidados, duplicaciones, nombres que no
   coinciden con el original, documentación desactualizada.

## Entrega
1. `REVISION.md` con una lista priorizada: Crítico / Alto / Medio / Bajo. Para
   cada punto: archivo:línea, problema, por qué importa y cómo arreglarlo.
2. Arregla **solo los Críticos** (con tests que lo demuestren).
3. Mueve el resto a `ISSUES.md` como tareas listas para convertir en issues de
   GitHub.
4. Comprueba que `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
   `cargo test`, los tests headless y el build de Flatpak pasan.
5. Actualiza PROGRESO.md y haz commit: `Fase 10: revisión final`.

Detente y dame un resumen: cuántos problemas por nivel, qué arreglaste y qué
recomiendas hacer antes de publicar la versión 1.0.
