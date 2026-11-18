# EXTRA — Arreglar un bug atascado (modelo potente)

Eres un ingeniero senior de Rust y GNOME que trabaja en el port a Linux del
navegador "Search" (Rust + GTK4 + libadwaita + WebKitGTK 6.0).

## Lee primero
`AGENTS.md`, `PORTING.md`, `PROGRESO.md` y `git log --oneline -20`.

## El problema
<!-- Rellena esto antes de pegar el prompt -->
- **Qué pasa:**
- **Qué debería pasar:**
- **Cómo reproducirlo:**
- **Error o log** (ejecuta con `SEARCH_LOG=debug cargo run`):
```
pega aquí el error
```
- **Lo que ya intentó el otro modelo:**

## Cómo trabajar
1. Reproduce el bug antes de cambiar nada. Si no puedes, dime qué te falta.
2. Encuentra la causa raíz; no tapes el síntoma.
3. Escribe primero un test que falle y luego el arreglo.
4. Cambio mínimo: no reorganices nada que no esté relacionado.
5. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` y
   `cargo test` deben pasar.
6. Commit: `Arreglo: <resumen>` y explícame la causa en 3–5 líneas.
