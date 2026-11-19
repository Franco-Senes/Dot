# Extractor de artículos (`third_party/readability`)

- **Componente:** Extractor DOM ligero para el modo lectura de Search Linux (`readability.js`).
- **Autoría y licencia:** Implementación original bajo la licencia **MIT** del proyecto (ver `LICENSE` en la raíz del repositorio), inspirada en la heurística de densidad de prosa de `upstream/Sources/Search/Reader.swift` (MIT) y Mozilla Readability. No contiene código de terceros de Mozilla vendoreado (entorno de compilación estrictamente offline).
- **Arquitectura:** El script únicamente clona y extrae el contenido candidato sin mutar el DOM de la página original; todo el saneamiento HTML, cálculo de tiempo de lectura y generación de la plantilla HTML/CSS clara/oscura vive en Rust puro (`crates/search-core/src/reader.rs`).
