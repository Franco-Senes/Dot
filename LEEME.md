# Kit para portar "Search" a Linux

Esta carpeta tiene todo lo que necesita un agente de IA (Antigravity CLI u otro)
para portar https://github.com/driceroland/Search (navegador WebKit para macOS,
Swift) a una app nativa de Linux en Rust + GTK4 + libadwaita + WebKitGTK 6.0.

## Contenido

| Archivo | Para qué sirve |
|---|---|
| `AGENTS.md` / `GEMINI.md` | Reglas permanentes del proyecto. El agente las lee siempre (son el mismo contenido con dos nombres, para que lo detecte cualquier herramienta). |
| `PLAN.md` | Plan maestro: contexto, stack, equivalencias macOS → Linux, fases. |
| `prompts/00-…10-*.md` | Un prompt completo por fase. Copia y pega el contenido entero. |
| `plantillas/PORTING.md` | Plantilla que el agente rellena en la Fase 0. |
| `plantillas/PROGRESO.md` | Registro de fases terminadas (el agente lo actualiza). |
| `scripts/instalar-dependencias.sh` | Instala Rust, GTK4, libadwaita, WebKitGTK 6.0, Meson y Flatpak (Debian/Ubuntu, Fedora, Arch, openSUSE). |
| `scripts/clonar-upstream.sh` | Clona el repo original en `./upstream`. |

## Cómo usarlo (paso a paso)

1. Crea una carpeta vacía para el proyecto, por ejemplo `~/proyectos/search-linux`,
   y copia dentro TODO el contenido de este kit.
2. Abre una terminal en esa carpeta y ejecuta:
   ```bash
   bash scripts/instalar-dependencias.sh
   bash scripts/clonar-upstream.sh
   git init && git add -A && git commit -m "Kit inicial del port"
   ```
3. Abre Antigravity CLI en esa carpeta.
4. **Fase 0:** abre `prompts/00-analisis.md`, copia TODO y pégalo. Usa el modelo
   más potente.
5. Cuando termine, **revisa `PORTING.md`**: es el punto de control más importante.
6. Para cada fase siguiente: **abre una sesión nueva**, elige el modelo indicado y
   pega el prompt completo de esa fase.
7. Después de cada fase, prueba tú mismo con `cargo run` antes de seguir.

## Qué modelo usar en cada fase

| Fase | Prompt | Modelo |
|---|---|---|
| 0 | Análisis y PORTING.md | Potente |
| 1 | Esqueleto | Potente |
| 2 | Pestañas y sesión | Potente |
| 3 | Historial, marcadores, descargas | Rápido |
| 4 | Shield, Curtain, modo lectura | Rápido |
| 5 | Vault (contraseñas) | Potente |
| 6 | Split View y PiP | Rápido |
| 7 | Bench, D-Bus, tests | Rápido |
| 8 | Empaquetado y CI | Rápido |
| 9 | Extensiones de Chrome | Potente |
| 10 | Revisión final | Potente |

Si el modelo rápido no arregla un bug en dos intentos, abre una sesión con el
modelo potente y usa `prompts/extra-arreglar-bug.md`.

## Consejos

- Una sesión por fase. Todos los prompts le piden al agente que lea primero
  `AGENTS.md`, `PLAN.md`, `PORTING.md`, `PROGRESO.md` y `git log`, así que no
  hace falta el historial del chat anterior.
- Cada fase termina en commit. Si algo sale mal: `git reset --hard HEAD~1`.
- No le pidas varias fases a la vez.
