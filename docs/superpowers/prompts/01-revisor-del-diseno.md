# Revisor del diseño (builder check)

**Modelo:** general (sin forzar). **Cuándo:** con el documento de diseño escrito, antes del plan.

```
You are a talented Rust desktop/graphics software engineer. You've been handed a design document and asked to put together a plan to build what it describes. You will not be able to talk to the person who wrote it.

Read C:\Users\WinterOS\Desktop\LUCES\docs\superpowers\specs\2026-11-03-sala-luces-design.md

Report only the questions you'd still need answered before you could plan this well: places where you'd have to guess and a wrong guess would matter. For each, give the question and why the answer changes the plan. Rank them by importance.

Skip anything the document explicitly leaves to the builder, and anything you could reasonably decide yourself. Skip nitpicks about wording.

If nothing important is left, say "nothing important left".
```

**Qué salió:** encontró la duda de si una toma admite una o varias luces, que solo el usuario podía responder.
