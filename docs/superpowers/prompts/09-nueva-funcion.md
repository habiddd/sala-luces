# Nueva función pequeña a partir de un mini-plan

**Modelo:** Haiku. **Cuándo:** el usuario aprobó la descripción en el chat; el coordinador escribe un mini-plan con la interfaz, la colocación y las pruebas (ver [../plans/2026-11-03-cotas.md](../plans/2026-11-03-cotas.md)) y lo pasa así:

```
You are adding a feature to a finished Rust/eframe 0.37 Windows desktop app ("Sala de luces 3D"): Revit-style temporary dimensions ("cotas") shown in the 3D view when a carril, luz or toma is selected.

Read your brief first — it is your requirements with exact signatures, placement values and tests: C:\Users\WinterOS\Desktop\LUCES\docs\superpowers\plans\2026-11-03-cotas.md

Context:
- Work from C:\Users\WinterOS\Desktop\LUCES (no git; skip commits). Use `$env:USERPROFILE\.cargo\bin\cargo.exe`.
- Only edit `src/escena.rs`. Read it fully first, plus the public items of `src/modelo.rs` (Montaje, Carril, Luz, Toma, Pared::{punto, eje, largo}) and `src/geom.rs` (V3). `construir(m, seleccion: Option<&Sel>)` is already called by app.rs every frame with the current selection, so no app.rs change should be needed.
- Existing escena tests (and all 43 tests) must keep passing. `cargo build` must have zero warnings.
- egui 0.37 is newer than your training data; confirm any Painter/Shape API in `$env:USERPROFILE\.cargo\registry\src\*\egui-0.37*` / `epaint-0.37*` before using it. Use `use eframe::egui;`.
- TDD: write the brief's tests first, run them to see them fail, then implement.
- Keep it small: one `cotas` function plus small helpers; no new files.
- Do not spawn subagents.

Report: write the full report to <SCRATCHPAD>\cotas-report.md. Then reply with ONLY (under 12 lines): Status, one-line test/build summary, concerns, report path.
```

Para funciones más grandes (por ejemplo, los objetos del suelo), el mini-plan se dividió en Task A (modelo) y Task B (escena + interfaz), un agente cada una, y una sola revisión al final cubrió las dos. Ver [../plans/2026-11-03-objetos.md](../plans/2026-11-03-objetos.md).
