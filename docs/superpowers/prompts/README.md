# Ejemplos de prompts usados en el proyecto

Estos son los prompts reales con los que se construyó la app: el diseño y la investigación los hizo Opus; la implementación, agentes Haiku 5.5; y las revisiones, agentes Sonnet (la revisión final de todo el código, Opus).

Las rutas `...\scratchpad\...` eran carpetas temporales de aquella sesión y ya no existen. Al reutilizar un prompt, cámbialas por las tuyas.

| Archivo | Para qué sirve | Modelo |
|---|---|---|
| [01-revisor-del-diseno.md](01-revisor-del-diseno.md) | Que un lector externo encuentre huecos en el documento de diseño antes de planificar | general |
| [02-implementador-tarea.md](02-implementador-tarea.md) | Implementar una tarea del plan con TDD (ejemplo: Tarea 2, modelo de datos) | Haiku |
| [03-revisor-tarea.md](03-revisor-tarea.md) | Revisar una tarea: cumplimiento del spec y calidad | Sonnet |
| [04-ronda-de-correccion.md](04-ronda-de-correccion.md) | Devolver al implementador los hallazgos del revisor | Haiku (el mismo agente) |
| [05-re-revision-acotada.md](05-re-revision-acotada.md) | Comprobar solo que los hallazgos quedaron resueltos | Sonnet |
| [06-revision-final.md](06-revision-final.md) | Revisión de todo el proyecto antes de entregarlo | Opus |
| [07-ola-final-de-correcciones.md](07-ola-final-de-correcciones.md) | Aplicar de una vez todos los hallazgos finales y generar el release | Haiku |
| [08-arreglo-de-bug-con-causa.md](08-arreglo-de-bug-con-causa.md) | Arreglar un bug ya diagnosticado, con una prueba que lo reproduce primero | Haiku |
| [09-nueva-funcion.md](09-nueva-funcion.md) | Añadir una función pequeña a partir de un mini-plan (ejemplo: cotas) | Haiku |

Lo que hace que funcionen:

- **Un brief por tarea.** El agente lee un archivo con los requisitos exactos (firmas, valores, pruebas) en lugar de un resumen.
- **Contexto que el brief no sabe.** Por ejemplo, qué existe ya, qué archivos no tocar, que egui 0.37 es más nuevo que su entrenamiento y que confirme la API en el código fuente, o que cargo puede no estar en el PATH.
- **Decisiones ya tomadas (rulings).** Se le dan resueltas para que no tenga que adivinar.
- **Prohibido lanzar subagentes.** La revisión la hace otro agente después.
- **Formato de respuesta corto.** El detalle va a un archivo de informe y la respuesta solo trae estado, pruebas e inquietudes.
- **El revisor no se fía del informe.** Verifica contra el código y clasifica cada hallazgo como Crítico, Importante o Menor.
