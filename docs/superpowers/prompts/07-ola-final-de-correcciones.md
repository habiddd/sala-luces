# Ola final de correcciones + release

**Modelo:** Haiku. **Cuándo:** una sola vez, con *todos* los hallazgos de la revisión final juntos (no un agente por hallazgo).

```
You are applying the final review's fixes to a finished Rust/eframe 0.37 Windows desktop app ("Sala de luces 3D") and then producing the release build (Task 7). Work from C:\Users\WinterOS\Desktop\LUCES (no git; skip commits). Use `$env:USERPROFILE\.cargo\bin\cargo.exe`. egui/eframe 0.37 is newer than your training data: confirm any API in `$env:USERPROFILE\.cargo\registry\src\*\egui-0.37*` / `eframe-0.37*` before using it. Do not spawn subagents.

Fix ALL of these (file:line refs are approximate; read the code):

1. Important — app.rs ~147: `send_viewport_cmd(ViewportCommand::Title(..))` runs every frame, which requests a repaint each time, so the app never idles (constant CPU/GPU). Add a field (e.g. `titulo_puesto: String`) and send the command only when the title text changes.

2. Important — escena.rs ~301-306 (`arrastrar`, Sel::Carril branch): the drag line is fixed at z = largo/2, so grabbing a rail away from its middle makes it jump sideways. Fix: intersect the ray with the plane y = h (h = altura_carriles): `s = (h - origen.y) / dir.y`; if `dir.y.abs() > 1e-3 && s > 0.0` then `x = origen.x + dir.x * s`, else fall back to the current `param_mas_cercano` result. Then ajustar_a_sala as before. Add test `arrastrar_carril_sin_salto`: ... assert x stays 4.5 ± 0.01.

3. escena.rs `dibujar`: the floor grid lines are drawn over rails and lights because all lines are drawn after all polygons. Draw first, before the depth-sorted polygons, every prim whose vertices are all at y == 0.0 (the floor quad and the grid lines); then the remaining polygons sorted farthest-first; then the remaining lines; then texts.

4. app.rs `numero` helper: add `.max_decimals(2)`.
5. Cargo.toml: add `"clipboard"` to eframe's features list.
6. app.rs Abrir: after a successful load with repair warnings, set `self.sucio = !avisos.is_empty();`.
7. Pair colours: the colour index must count only CONNECTED luces ... add test `color_cuenta_solo_conectadas` ... show `L-001 (C1) → T-01` in the luz list.

Then:
8. `cargo test` all pass; `cargo build` zero warnings.
9. Task 7: `cargo build --release`. Copy `target\release\sala-luces.exe` alone to a new empty temp folder, start it from there, wait 5 s, check HasExited is false, Stop-Process, delete the temp folder. This proves it needs no MinGW DLLs. Report the exe size.

Report: write the full report to <SCRATCHPAD>\sdd\final-fix-report.md. Then reply with ONLY (under 15 lines): Status, one-line test/build/release summary with exe size, concerns, report path.
```

**Lección:** después de esta ola, al medir la CPU se vio que el problema seguía. La revisión había dado una causa equivocada (el título). La causa real, el renderizador wgpu, se encontró comparando con una app mínima. Una corrección hay que medirla, no darla por buena.
