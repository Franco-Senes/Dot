# FASE 0 — Análisis (modelo potente)

Eres un ingeniero senior de Rust y GNOME. Vamos a portar a Linux el navegador
"Search" para macOS (https://github.com/driceroland/Search, Swift, MIT).

En esta sesión NO escribas código de la aplicación. Solo análisis y documentación.

## Lee primero
1. `AGENTS.md`: reglas permanentes del proyecto. Cúmplelas siempre.
2. `PLAN.md`: plan maestro, stack y equivalencias macOS → Linux.
3. `plantillas/PORTING.md` y `plantillas/PROGRESO.md`.

Si `./upstream` no existe, clona el original:
`git clone --depth 1 https://github.com/driceroland/Search upstream`
y comprueba que `upstream/` está en `.gitignore`.

## Tareas
1. Lee completos: `upstream/README.md`, `NOTES.md`, `ROADMAP.md`, `CHANGELOG.md`,
   `CONTRIBUTING.md`, `SECURITY.md`, `Package.swift`, los scripts `build.sh`,
   `bench` y `test-find`, `Search.sdef`, `Tests/` y **todos** los archivos de
   `upstream/Sources/Search/`. Mira también `Engine/`, `Installer/` y
   `skill/search-bench/`.
2. Consulta la documentación actual (no de memoria) y anota versión y fuente:
   - Versión estable actual de WebKitGTK 6.0 y del crate `webkit6`.
   - ¿Tiene WebKitGTK soporte de WebExtensions usable desde la API pública?
     ¿Desde qué versión? ¿Admite extensiones de Chrome (Manifest V3)?
   - ¿Hay Picture-in-Picture nativo en WebKitGTK?
   - Confirma que WebKitUserContentFilterStore acepta el mismo JSON que
     WKContentRuleList.
   - Versiones estables de gtk4-rs, libadwaita-rs y oo7.
   - ¿Qué versión de libadwaita hace falta para AdwTabOverview, AdwTabBar y
     AdwOverlaySplitView? ¿Qué runtime de GNOME en Flatpak la incluye?
3. Copia `plantillas/PORTING.md` a `PORTING.md` en la raíz y rellénalo entero:
   - Tabla de archivo Swift → módulo Rust → API Linux → estado → fase.
     Una fila por cada archivo de `Sources/Search/`, sin saltarte ninguno.
   - Esquemas JSON del original (sesión, historial, marcadores, ajustes, reglas
     de Curtain, lista de Shield), con ejemplos.
   - Lista completa de atajos de teclado del original y su equivalente con Ctrl.
   - Lista de ajustes del original y dónde se guardarán (GSettings o JSON).
   - Comandos del socket de bench y su protocolo.
   - Resultado de cada verificación del punto 2, con enlace a la fuente.
   - Riesgos, ordenados por impacto.
   - Correcciones al PLAN.md si algo resultó incorrecto. No edites PLAN.md:
     escribe las correcciones aquí.
4. Copia `plantillas/PROGRESO.md` a `PROGRESO.md` y marca la Fase 0.
5. Commit: `Fase 0: análisis y PORTING.md`.

## Cuando termines
Detente y dame:
- Un resumen de 10 líneas como máximo.
- Las 3–5 decisiones que debo revisar o aprobar antes de la Fase 1.
- Cualquier función del original que sea imposible en Linux y por qué.
