# Arreglo de un bug con la causa ya encontrada

**Modelo:** Haiku. **Cuándo:** cuando el coordinador ya diagnosticó la causa (aquí: las líneas blancas disparadas al seleccionar un carril visto de canto).

```
Fix one rendering bug in a Rust/eframe 0.37 app at C:\Users\WinterOS\Desktop\LUCES (no git; skip commits). Use `$env:USERPROFILE\.cargo\bin\cargo.exe`. Only edit src/escena.rs. Do not spawn subagents.

Bug: when a carril is selected and seen nearly edge-on, long white spikes shoot out of it across the screen.
Root cause (confirmed): `dibujar` (escena.rs ~265-274) draws highlighted polygons with `egui::Shape::convex_polygon(pts, color, Stroke::new(2.0, WHITE))`. epaint 0.37 strokes closed polygons with unbounded miter joins (`CUT_OFF_SHARP_CORNERS = false` in epaint-0.37.0/src/tessellator.rs:461), so a sliver quad (box face seen edge-on) gets miter points far outside the polygon.

Fix: always fill polygons with `Stroke::NONE`; for `resaltado` polygons draw the outline as separate `p.line_segment([pts[i], pts[(i+1) % n]], Stroke::new(2.0, WHITE))` calls (open segments have no joins). Keep the draw order the same.

Test first (TDD): add `resaltado_sin_picos` in the escena tests: build a headless frame like the existing `dibujar_con_camara_dentro_no_falla` test does, and inside it call `dibujar` with a single Prim: `Forma::Poligono` of a sliver quad whose projection is a very thin, long quad (< 1 px wide and > 100 px long), `resaltado: true`. Then tessellate the output shapes and assert every mesh vertex lies within the bounding box of the projected quad expanded by 10 px. Run it, confirm it FAILS before the fix (spikes), then implement the fix and confirm it passes.

Then `cargo test` (all must pass) and `cargo build` with zero warnings, then `cargo build --release`.

Report: write the details (RED/GREEN output, change made) to <SCRATCHPAD>\picos-report.md. Reply with ONLY (under 10 lines): Status, whether the new test failed before the fix, one-line test/build summary, concerns.
```

**Qué salió:** la prueba fallaba antes, con un punto a 2.266 px fuera de su sitio. El agente descubrió además que el relleno con suavizado también generaba puntas, y cambió los rellenos por triángulos simples.
