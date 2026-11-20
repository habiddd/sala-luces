# SDD ledger — plan: docs/superpowers/plans/2026-11-03-sala-luces.md
Ruling: no git in this folder; review is done on the files each task touches instead of git diffs — reviewers read whole files — cost if wrong: no per-task diff history.
Ruling: each module may carry `#![allow(dead_code)]` until Task 6 removes them — plan only mentioned modelo — cost: none.
Ruling: Luz drag line runs at centro_luz height (altura_carriles - 0.25), not at rail height, so the test point (1.5, 3.55, 1.0) lies exactly on it and dragging the body feels natural — cost: none visible.
Pre-flight scan:
| Pair/Task | Shared | Finding |
| T2→T3 | Equipo in modelo | consistent |
| T2→T4 | Sala in para_sala | consistent |
| T2,T4→T5 | Pared::punto/eje [f32;3] → V3::from_arr | consistent |
| T5 arrastrar test vs drag-line height | luz line | conflict → ruled above |
| T5→T6 | construir/dibujar/elegir/arrastrar | consistent |
| T1 | empty modules | consistent |
| T2..T5 | dead_code until T6 | ruled above |
Task 0: complete (rustup GNU, cargo 1.99.1)
Ruling: Task 1 reviewed by controller directly (3 tiny files, read in full) instead of a reviewer seat — skeleton only — cost: low.
Ruling: keep .gitignore created by cargo init — harmless — cost: none.
Note: eframe 0.37 App trait is `fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame)`; panels use `.show(ui, ..)`.
Task 1: complete (review clean)
Ruling: luz with empty carril in a loaded file is reassigned to the first carril (creating one if none) instead of dropped — keeps the user's data — cost: light may appear on an unexpected rail, with a warning.
Ruling: guardar made atomic (tmp + rename) though reviewer marked it Minor — data-loss prevention — cost: none.
Task 2: minor (deferred): borrar_carril Ok for unknown id.
Task 2: fix round 1/5 dispatched (empty ids in reparar; missing reparar tests; atomic guardar)
Task 2: fix round 1/5 (3 addressed, 0 open)
Task 2: minor (deferred): no fsync before rename; dropped duplicate luz warning doesn't mention lost toma.
Task 2: complete (review clean after fix round 1)
Task 3: implementer DONE; review dispatched. Task 4: implementer dispatched (parallel, separate file).
Ruling: inventory load drops empty-code and duplicate entries (keep first) and tells the user via the load message, instead of going read-only — keeps the inventory usable — cost: a hand-edited bad record is lost on next save, after a visible warning.
Task 3: minor (deferred): empty APPDATA gives relative path; no tests for borrar success / editar revert.
Task 3: fix round 1/5 dispatched (load messages for empty/duplicate; read-only test; tmp cleanup)
Task 3: fix round 1/5 (4 addressed, 0 open)
Task 3: complete (review clean after fix round 1)
Task 4: implementer DONE; review dispatched.
Task 4: minor (deferred): degenerate rect gives NaN coords (Task 5 dibujar/elegir skip rect < 1px); no zoom clamp test.
Task 4: complete (review clean; controller verified cargo build 0 warnings, 34 tests pass)
Ruling: room dimensions capped at 100 m in modelo::ajustar_a_sala (Task 5 implementer allowed to touch modelo.rs for this) — fixes unbounded grid from a hostile/broken file at the source — cost: rooms >100 m impossible.
Task 5: minor (deferred): headless draw test only asserts no panic.
Task 5: fix round 1/5 dispatched (max room size; closest-line picking)
Task 5: fix round 1/5 (2 addressed, 0 open)
Task 5: complete (review clean after fix round 1; 41 tests)
Ruling: window starts maximized with min size 900x600 — controller screenshot showed 1280x800 overflowing a 150% DPI screen — cost: none.
Ruling: editing an equipo in the inventory does not update already-placed luces — spec says luz keeps a snapshot copy for the record — cost: user must re-place a light to refresh its data.
Task 6: minor (deferred): app.rs 767 lines in one file (panels split into functions).
Task 6: fix round 1/5 dispatched (maximized window; sucio only on movement; editar only if changed; mensaje append)
Task 6: fix round 1/5 (4 addressed, 0 open)
Task 6: controller manual UI test (maximized window, add toma, add luz via picker, connect toma → red pair line, drag luz along rail z 3.0→0.89, close with unsaved → modal, Descartar exits) — all OK. Temp %APPDATA%\SalaLuces created for the test and removed.
Task 6: minor (controller visual): floor grid lines drawn over rails/lights (lines drawn after all polygons); Z DragValue shows 4 decimals.
Task 6: complete

## Pendiente (para la próxima sesión)
- Revisión final de todo el código (modelo más capaz) — se paró a petición del usuario antes de terminar.
- Corregir lo visual detectado a mano: la cuadrícula del suelo se dibuja encima de carriles y luces (escena.rs dibuja todas las líneas después de los polígonos); el campo Z muestra 4 decimales.
- Tarea 7: `cargo build --release` y comprobar `target\release\sala-luces.exe`.

## Revisión final (2026-11-03, Opus)
Veredicto: Needs fixes. Importantes: (1) el título se envía cada frame → la app redibuja sin parar; (2) arrastrar un carril lejos de su centro lo hace saltar de lado.
Ruling: also fix now: grid/floor drawn first; max_decimals(2); eframe "clipboard" feature (adds a dependency feature, not a new crate); sucio = true after a repaired load; pair colour index counts only connected luces + "→ T-xx" in the luz list — cheap and serve the "clear which luz goes to which toma" anti-goal — cost: none.
Ruling: leave the other minors as triaged by the final reviewer (unknown-id delete, fsync, APPDATA empty, extra tests, near-plane floor vanish, leftover .tmp, inventory row edit dropped on window close).
Final fix wave dispatched (Haiku) together with Task 7 (release build + run from clean folder).
Final fix wave: 7/7 addressed (scoped re-review clean); 43 tests pass; 0 warnings.
Controller found after the fix wave: release exe still used 100% of one CPU core idle. Root cause (minimal eframe probe): wgpu renderer spins at idle on this machine with DX12 and Vulkan backends; wgpu-GL and glow idle at 0.
Ruling: switch eframe renderer from wgpu to glow (Cargo.toml features ["glow","default_fonts","clipboard"]) — measured 0 ms CPU idle vs 4000 ms/4 s — cost if wrong: no DX12 fallback on PCs without proper OpenGL drivers (Remote Desktop / VMs). Done by controller (1-line change), verified by measurement, tests, screenshot.
Task 7: complete — target\release\sala-luces.exe 18.9 MB, runs from an empty folder (no extra DLLs), idle CPU 0.
## Estado: TERMINADO (primera versión). Sin pendientes del plan.

## Cotas temporales (2026-11-03)
Plan: docs/superpowers/plans/2026-11-03-cotas.md. Implementado por Haiku en escena.rs, revisión aprobada (solo menores: falta test de colocación de offsets, luz con carril inexistente mide contra x=0). Verificado a mano en la app (C2 seleccionado: 1.50/1.50/3.00/3.00). 50 tests, release recompilado.

## Picos al resaltar (2026-11-03)
Síntoma (captura del usuario): carril seleccionado visto casi de canto → líneas blancas larguísimas.
Causa: epaint 0.37 une esquinas de polígonos cerrados con inglete sin límite (tessellator.rs:461, CUT_OFF_SHARP_CORNERS=false); el relleno con feathering también. Caras vistas de canto = polígonos finísimos → puntas enormes.
Arreglo (Haiku): relleno como malla de triángulos sin feathering (`relleno`) y contorno de resaltado como segmentos sueltos. Test `resaltado_sin_picos` falló antes (vértice a -2266 px) y pasa después. Controller: guard `pts.len() < 3` en `relleno`; verificado a mano en 6 ángulos sin puntas. 51 tests.
Ruling: aceptar bordes de caras sin suavizado (antialias) a cambio de no tener puntas — en pantalla de 150 % DPI no se nota — cost: bordes algo dentados en pantallas de baja resolución.
Pendiente: recompilar release (el .exe release estaba abierto por el usuario y bloqueado).

## Objetos del suelo (2026-11-03)
Plan: docs/superpowers/plans/2026-11-03-objetos.md. Task A (modelo) y Task B (escena + app) por Haiku; revisión conjunta aprobada (solo menores: test no comprueba extremos de las guías; cota total duplicada si el vecino es la pared; rename con clic en otra fila — mismo patrón que carriles). Verificado a mano: silla, mesa, maniquí, arrastre por el suelo, cotas con guías a los carriles. 62 tests, 0 warnings.
Ruling: un objeto justo debajo de un carril (misma x) no muestra cota 0 a ese carril; mide a los vecinos estrictos — mismo criterio que carriles/luces — cost: no se ve "0.00 m" para "está debajo de C2".
Pendiente: recompilar release cuando el usuario cierre la app abierta (PID 424 bloquea el .exe).

## Props de ejemplo por defecto (2026-11-03)
Montaje::default trae Mesa 1 (3.0, 3.0), Silla 1 (3.0, 3.75, rot 180, mirando a la mesa), Maniquí 1 (4.2, 2.0). Archivos abiertos de disco no cambian (serde default = vacío). Test default_trae_objetos_de_ejemplo; 6 tests ajustados con m.objetos.clear() sin cambiar asserts. 63 tests. Verificado en captura.
Ruling: posiciones elegidas por Claude — cost: el usuario las mueve.
Ruling: sin revisor aparte (cambio de un constructor + tests, verificado por captura) — cost: bajo.

## Categorías de inventario (2026-11-03)
Plan: docs/superpowers/plans/2026-11-03-categorias.md. A+B por Haiku, revisión: 1 Importante (renombrar_objeto con código de luz) arreglado. Extras: BOM UTF-8 tolerado al cargar; buffer de alta por categoría; repaint al abrir ventanas/avisos (CPU en reposo 0); campos anchos y sin 'Borrar categoría' en las 4 base. 73 tests. Incidente: un inventario de prueba con BOM escrito por Claude en %APPDATA% mostró un error al usuario; borrado.
Ruling: las 4 categorías base no se pueden borrar — cost: ninguno.

## Inventario con pestañas (2026-11-03)
Ventana emergente con una pestaña por categoría (Haiku). Ruling: se mantiene como ventana emergente, no en un panel lateral — paneles ya llenos — cost: si el usuario lo prefiere en panel, moverlo.

## Categoría editable en objetos (2026-11-03)
Objeto.categoria + cambiar_categoria_objeto; combo en el panel derecho (todas menos Luces); forma según categoría. 77 tests; verificado en captura (Mesa 1 → Maniquíes).
Ruling: al cambiar de forma, las medidas vuelven a las del nuevo tipo; si la forma no cambia, se conservan — cost: el usuario re-ajusta medidas tras cambiar.

## Catálogo de producción + importar modelos (2026-11-03)
Spec: docs/superpowers/specs/2026-11-03-catalogo-e-importar-design.md; plan: docs/superpowers/plans/2026-11-03-catalogo-e-importar.md. 3 tareas Haiku.
- T1: 28 formas (24 nuevas), 8 categorías base, combo Forma. Revisado por captura (montaje de prueba con las 28).
- T2: importar glTF/GLB + OBJ (deps gltf 1.4, tobj 4), normalizar, simplificar ≤20 000 tris, biblioteca %APPDATA%\SalaLuces\modelos. Revisión: 3 Importantes (gltf::import decodificaba texturas; nombres reservados/colisiones de archivo; simplificar podía dejar vacío) → arreglados.
- T3: dibujo de mallas + "Importar modelo…" + Importados en Añadir y Forma. Revisión aprobada (menores: clone de Sel por triángulo; Caja conserva medidas del modelo al cambiar forma).
Prueba manual: lampara.obj (aro dorado + pie negro) importada, colores MTL correctos, medidas 0.96×1.68, arrastrable. Biblioteca de prueba borrada después. 102 tests; release 23.6 MB.
Ruling: texturas ignoradas, límite 20 000 triángulos, 4 categorías base nuevas, Categoría/Forma separadas — cost: modelos con textura se ven del color base.
Ruling: un modelo con datos dañados en un montaje se descarta al abrir (aviso) — cost: si no está en la biblioteca, se pierde al guardar.

## Simulación real de luces (2026-11-04)
Spec: docs/superpowers/specs/2026-11-03-luces-specs-design.md; plan: docs/superpowers/plans/2026-11-03-luces-simulacion.md. 5 tareas Haiku.
- T1 datos (SpecsLuz/AjustesLuz/TipoLuz, LuzPiso, tomas de suelo, Luces techo/piso, ejemplos EJ-T1/EJ-P1). T2 fotometria.rs (inverso cuadrado, coseno, perfil half-peak, CCT Wyman, EV100, indirecta por cavidad, lux_en con sombras por rayo). T3 malla_gpu/malla_sombras (cuerpos de luz sin sombra). T4 render_gl.rs (OpenGL 3.3 en PaintCallback, sombras 16×1024², PCF, ACES, mapa de lux, humo). T5 UI.
- Controller: 1 ronda en T4 (carcasa opaca tapaba la vista → vista de corte). Verificado con capturas: Realista con sombras, Mapa de lux, haz visible, panel de luz. CPU en reposo ~0. 140 tests.
- Sin revisor aparte en estas tareas (verificación visual + tests).
Ruling: valores típicos de tipos de luz; perfil gaussiano 50% en half-peak; rebote por fórmula de cavidad; >16 luces sin sombra, >32 ignoradas — cost: precisión de estimación, no trazado de rayos.

## Calidad de luz: Alta (PCSS) + Fotorrealista (path tracing) + Configuración (2026-11-04)
Plan: docs/superpowers/plans/2026-11-04-calidad-luz.md. 3 tareas Haiku + 1 depuración + 1 ronda de correcciones.
- T1 config.rs + ventana Configuración + offscreen FBO (escala, MSAA) + info GPU + defaults por GPU. T2 PCSS por radio de emisor. T3 trazador BVH en texturas RGBA32F, 1 muestra/frame, NEE, rebotes, ruleta rusa, acumulación progresiva que se detiene al máximo.
- Bug lux bajo cursor (38 vs ~1800): el rayo del cursor chocaba con el techo de la carcasa oculta → corregido (oculta_desde_fuera).
- Revisión cross-vendor (Opus): exports de GPU dedicada ausentes en el .exe (ld GNU) → exports.def vía build.rs, verificado en debug y release; TDR: teselas por fotograma con presupuesto 30 ms + 1 luz por vértice (muestreo por importancia) + escala 0.5 en integradas + bajar rebotes / caer a Alta; fugas en step-down; MSAA fallido silencioso; sin OpenGL → diálogo; renderizadores software → Rápida; NaN guard; clasificación de GPUs ampliada.
- Verificado en AMD Radeon 860M: Fotorrealista converge 256/256, máx 7.7 ms por lote de teselas (debug), CPU en reposo ~0. 159 tests. NVIDIA/Intel NO probados físicamente.
Ruling: Fotorrealista se activa a mano; Alta por defecto; 4096 solo en GPU dedicada — cost: ninguno.
Pendiente: recompilar target\release (bloqueado por la app abierta del usuario, PID 29180).
