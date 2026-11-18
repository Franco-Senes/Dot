# PORTING.md — Search para Linux

> Documento vivo. Lo crea la Fase 0 y lo actualiza cada fase.
> Si algo aquí contradice a PLAN.md, manda lo que dice aquí.

## 1. Versiones verificadas
| Componente | Versión | Fuente (enlace) | Fecha de la consulta |
|---|---|---|---|
| WebKitGTK 6.0 | | | |
| crate webkit6 | | | |
| gtk4-rs | | | |
| libadwaita-rs / libadwaita | | | |
| oo7 | | | |
| Runtime de GNOME (Flatpak) | | | |

## 2. Verificaciones de riesgo
| Pregunta | Respuesta | Fuente |
|---|---|---|
| ¿WebExtensions usables desde la API pública de WebKitGTK? | | |
| ¿Picture-in-Picture nativo? | | |
| ¿WebKitUserContentFilterStore acepta el JSON de WKContentRuleList? | | |
| ¿Versión de libadwaita necesaria para AdwTabOverview/AdwOverlaySplitView? | | |

## 3. Mapa de archivos
| Archivo Swift (upstream) | Qué hace | Módulo Rust | API Linux | Estado | Fase |
|---|---|---|---|---|---|
| Sources/Search/… | | src/… | | pendiente | |

Estados: `pendiente` · `directo` · `adaptado` · `diferido` · `imposible` · `hecho`

## 4. Esquemas de datos (JSON)
### session.json
```json
```
### Historial
### Marcadores
### Ajustes
### Curtain (reglas por sitio)
### Shield (listas de reglas)

## 5. Atajos de teclado
| Acción | Original (macOS) | Linux | Hecho |
|---|---|---|---|

## 6. Ajustes
| Ajuste | Valor por defecto | Dónde se guarda (GSettings/JSON) | Fase |
|---|---|---|---|

## 7. Protocolo del socket de bench
| Comando | Argumentos | Respuesta |
|---|---|---|

## 8. Permisos de Flatpak
| Permiso | Por qué |
|---|---|

## 9. Riesgos (ordenados por impacto)
1.

## 10. Correcciones al PLAN.md
-

## 11. Decisiones de arquitectura
| Fecha | Decisión | Motivo |
|---|---|---|

## 12. Rendimiento
| Medida | Resultado | Hardware | Fecha |
|---|---|---|---|
| Arranque en frío | | | |
| Memoria con 1 / 10 / 20 pestañas | | | |

## 13. Pruebas en sitios reales
| Sitio | Shield | Curtain | Lectura | Notas |
|---|---|---|---|---|

## 14. Seguridad de Vault (modelo de amenazas)
