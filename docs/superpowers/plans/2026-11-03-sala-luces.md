# Sala de luces 3D — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A native Windows `.exe` in Rust that shows a recording room in 3D with ceiling rails, lights placed on rails (code, brand, model from an inventory) and the wall outlet (toma) each light plugs into, all movable by mouse drag or numeric panels, saved one file per setup.

**Architecture:** `eframe`/`egui` window with side panels; the 3D view is drawn by hand with `egui::Painter` (orbit camera, flat-shaded polygons sorted back-to-front, lines, labels). Pure logic (model rules, camera math, scene building, picking, dragging) lives in modules with unit tests; `app.rs` only wires UI to them.

**Tech Stack:** Rust stable (`stable-x86_64-pc-windows-gnu`), `eframe` 0.37 (wgpu renderer), `serde` 1 + `serde_json` 1, `rfd` 0.17.

**Spec:** `docs/superpowers/specs/2026-11-03-sala-luces-design.md` — read it before your task.

## Global Constraints

- All UI text in Spanish. Units are meters. Axes: X = ancho, Z = largo, Y = up; origin at a floor corner.
- Dependencies: only `eframe`, `serde` (feature `derive`), `serde_json`, `rfd`. Nothing else.
- `eframe` 0.37 with `default-features = false` and features for the **wgpu** renderer and default fonts (check the exact feature names in `~/.cargo/registry/src/*/eframe-0.37*/Cargo.toml`).
- egui/eframe 0.37 is newer than your training data: before using any egui/eframe API, confirm its signature in the crate source under `~/.cargo/registry/src/*/egui-0.37*/` / `eframe-0.37*/` (e.g. `App` trait, panel types, `DragValue`, `ViewportCommand`). Do not guess.
- Release builds have no console: `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` in `main.rs`.
- Codes and ids (luz, carril, toma, equipo) are trimmed; empty is rejected. Uniqueness is checked on the trimmed value.
- Errors are `Result<_, String>` with a Spanish message shown to the user; nothing panics on user input or file content.
- No git in this folder: skip commits. Every task ends with `cargo test` and `cargo build` passing with zero warnings (`cargo build 2>&1 | findstr warning` prints nothing).

## Review Focus

1. Unreadable/corrupt `inventario.json` at startup → app shows the error and **never overwrites** that file (inventory saving disabled for the session). Test in Task 3.
2. Save fails (folder read-only, path invalid) → error shown, the setup stays marked as unsaved. Test in Task 2 (`guardar` returns `Err` on a bad path).
3. Camera close to or inside the room → polygons with any vertex behind the near plane are skipped, no panic, no stretched garbage. Test in Task 4 / Task 5.
4. Renaming a carril or toma to an empty or already-used id → rejected, nothing changes. Test in Task 2.
5. Codes with spaces around them (`" L1 "`) collide with `"L1"` → treated as the same code. Test in Tasks 2 and 3.

---

## File structure

```
Cargo.toml
src/main.rs        – window launch only
src/modelo.rs      – Montaje + rules + JSON load/save (incl. Equipo type)
src/inventario.rs  – equipment catalog file
src/geom.rs        – V3, Camara (orbit, project, ray), closest-point math
src/escena.rs      – Sel, Prim; build primitives from a Montaje; draw; pick; drag
src/app.rs         – eframe App: panels, menus, dialogs, 3D view interaction
```

---

### Task 0: Toolchain (done by the controller, not a subagent)

- [ ] Download `https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-gnu/rustup-init.exe` to the scratchpad and run it with `-y --default-host x86_64-pc-windows-gnu --profile minimal`.
- [ ] Verify: `~/.cargo/bin/cargo --version` prints a version.

### Task 1: Project skeleton that opens a window

**Files:** Create `Cargo.toml`, `src/main.rs`, `src/app.rs` (stub).

**Interfaces:**
- Produces: `app::App` with `App::new(cc: &eframe::CreationContext) -> App` implementing `eframe::App`; `main.rs` declares `mod app; mod escena; mod geom; mod inventario; mod modelo;` (create the other four as empty files).

- [ ] **Step 1:** `Cargo.toml`: package `sala-luces`, edition 2024, deps per Global Constraints (`cargo add` them so versions resolve).
- [ ] **Step 2:** `main.rs` runs `eframe::run_native("Sala de luces", NativeOptions { viewport: 1280×800, ..}, …)` creating `App`. `App` shows a central panel with the label `"Sala de luces"`.
- [ ] **Step 3:** `cargo build` succeeds. If the wgpu renderer fails to compile on the GNU toolchain, report it (BLOCKED) with the error rather than switching renderer.
- [ ] **Step 4:** Run `target\debug\sala-luces.exe`, confirm a window appears (start it, wait 3 s, check the process is alive, then kill it).

### Task 2: Data model and rules (`modelo.rs`)

**Files:** `src/modelo.rs` (code + `#[cfg(test)] mod tests`).

**Interfaces — Produces** (all `#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]`; `Pared` also `Copy, Eq`):
```rust
pub struct Sala { pub ancho: f32, pub largo: f32, pub alto_techo: f32, pub altura_carriles: f32 }
pub struct Carril { pub id: String, pub x: f32 }               // runs along Z from 0 to largo
pub enum Pared { Norte, Sur, Este, Oeste }
pub struct Toma { pub codigo: String, pub pared: Pared, pub pos: f32 }
pub struct Equipo { pub codigo: String, pub marca: String, pub modelo: String }
pub struct Luz { pub codigo: String, pub marca: String, pub modelo: String,
                 pub carril: String, pub z: f32, pub toma: Option<String> }
pub struct Montaje { pub sala: Sala, pub carriles: Vec<Carril>, pub tomas: Vec<Toma>, pub luces: Vec<Luz> }

impl Pared {
    pub const TODAS: [Pared; 4];
    pub fn nombre(self) -> &'static str;                 // "Norte", "Sur", "Este", "Oeste"
    pub fn largo(self, s: &Sala) -> f32;                 // Sur/Norte: ancho; Este/Oeste: largo
    pub fn punto(self, s: &Sala, pos: f32) -> [f32; 3];  // Sur (pos,h,0) Norte (pos,h,largo) Oeste (0,h,pos) Este (ancho,h,pos), h = altura_carriles
    pub fn eje(self) -> [f32; 3];                        // Sur/Norte [1,0,0]; Este/Oeste [0,0,1]
}
impl Default for Montaje   // sala 6×6, techo 4.0, carriles 3.8; C1/C2/C3 at x = ancho*i/4 (1.5, 3.0, 4.5); no tomas, no luces
impl Montaje {
    pub fn ajustar_a_sala(&mut self);   // min sizes 1.0 m; altura_carriles in [0.5, alto_techo]; clamp carril.x [0,ancho], luz.z [0,largo], toma.pos [0, pared.largo]
    pub fn anadir_carril(&mut self) -> String;                         // first free "C{n}" n≥1, x = ancho/2
    pub fn borrar_carril(&mut self, id: &str) -> Result<(), String>;   // Err if any luz on it
    pub fn renombrar_carril(&mut self, viejo: &str, nuevo: &str) -> Result<(), String>; // updates luz.carril
    pub fn anadir_toma(&mut self) -> String;                           // first free "T-{nn:02}", Sur, pos = ancho/2
    pub fn borrar_toma(&mut self, codigo: &str);                       // luz.toma == codigo → None
    pub fn renombrar_toma(&mut self, viejo: &str, nuevo: &str) -> Result<(), String>; // updates luz.toma
    pub fn colocar_luz(&mut self, e: &Equipo, carril: &str) -> Result<(), String>; // copies marca/modelo, z = largo/2, toma None; Err if codigo already placed or carril missing
    pub fn quitar_luz(&mut self, codigo: &str);
    pub fn conectar(&mut self, luz: &str, toma: Option<&str>) -> Result<(), String>; // Err if toma missing or used by another luz
    pub fn tomas_libres_para(&self, luz: &str) -> Vec<String>; // free tomas + the one this luz already has, in tomas order
    pub fn reparar(&mut self) -> Vec<String>;  // after load; returns Spanish warnings, see below
    pub fn cargar(ruta: &std::path::Path) -> Result<(Montaje, Vec<String>), String>; // parse + reparar
    pub fn guardar(&self, ruta: &std::path::Path) -> Result<(), String>;             // pretty JSON
}
```
`reparar`, in this order: drop later duplicate carril ids / toma codes / luz codes; create a missing carril (x = ancho/2) for any luz whose carril does not exist; clear `toma` refs to missing tomas; when two luces share a toma, the first keeps it and later ones get `None`; then `ajustar_a_sala`. One warning per fix.

- [ ] **Step 1: Write failing tests** (in `modelo.rs`):
  - `default_tiene_tres_carriles`: ids `["C1","C2","C3"]`, xs `[1.5,3.0,4.5]`, sala `6,6,4,3.8`.
  - `anadir_carril_usa_primer_id_libre`: after `borrar_carril("C2")`, `anadir_carril()` == `"C2"` with x 3.0.
  - `no_borra_carril_con_luces`: colocar on C1 → `borrar_carril("C1")` is Err, carril still there.
  - `renombrar_carril_actualiza_luces` and `renombrar_carril_rechaza_vacio_o_duplicado` (`""`, `"  "`, `"C2"` → Err, nothing changed); same pair for tomas.
  - `colocar_luz_copia_datos_y_rechaza_duplicado`: z == 3.0, marca/modelo copied; placing `" L1 "` after `"L1"` → Err.
  - `una_luz_por_toma`: L1→T-01 ok; L2→T-01 Err; `tomas_libres_para("L2")` excludes T-01, `tomas_libres_para("L1")` includes it.
  - `borrar_toma_desconecta`: luz.toma becomes None.
  - `ajustar_recorta_al_encoger`: luz z 5.5, sala largo → 4.0, ajustar → z 4.0; toma on Sur at pos 5.0, ancho → 3.0 → pos 3.0.
  - `reparar_toma_compartida`: two luces with same toma → second None, 1 warning; luz on carril `"X"` missing → carril `"X"` created.
  - `guardar_y_cargar_igual`: save to temp dir file, load, equal, no warnings.
  - `guardar_en_ruta_invalida_falla`: path inside a non-existent directory → Err.
  - `cargar_json_roto_falla`: file with `"{no json"` → Err.
- [ ] **Step 2:** `cargo test modelo` → FAIL (not defined).
- [ ] **Step 3:** Implement. Use `std::env::temp_dir()` in tests for files (unique names per test).
- [ ] **Step 4:** `cargo test modelo` → all PASS; `cargo build` no warnings (unused items: add `#[allow(dead_code)]` only on the module until Task 6 uses them, then remove).

### Task 3: Inventory (`inventario.rs`)

**Interfaces:**
- Consumes: `modelo::Equipo`.
- Produces:
```rust
pub fn ruta_por_defecto() -> std::path::PathBuf; // %APPDATA%\SalaLuces\inventario.json; if APPDATA unset, "inventario.json"
pub struct Inventario { pub equipos: Vec<Equipo>, ruta: PathBuf, pub solo_lectura: bool }
impl Inventario {
    pub fn cargar(ruta: PathBuf) -> (Inventario, Option<String>); // missing file → empty, None. Unreadable/corrupt → empty, solo_lectura = true, Some(error)
    pub fn guardar(&self) -> Result<(), String>;  // Err if solo_lectura; create_dir_all; write "<ruta>.tmp" then rename over
    pub fn anadir(&mut self, e: Equipo) -> Result<(), String>;               // trims; Err empty or duplicate codigo; saves
    pub fn editar(&mut self, codigo: &str, e: Equipo) -> Result<(), String>; // same checks (excluding itself); saves
    pub fn borrar(&mut self, codigo: &str) -> Result<(), String>;            // saves
}
```
File content is `{"equipos":[...]}`.

- [ ] **Step 1: Failing tests:** `archivo_inexistente_da_vacio`; `anadir_guarda_y_recarga` (add, reload from same path, same list); `rechaza_codigo_vacio_y_duplicado` (`""`, `"E1"` twice, `" E1 "`); `editar_cambia_datos`; `archivo_corrupto_no_se_sobrescribe` (write `"basura"`, cargar → solo_lectura + Some(err); `anadir` → Err; file content still `"basura"`).
- [ ] **Step 2:** `cargo test inventario` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test inventario` → PASS.

### Task 4: Camera math (`geom.rs`)

**Interfaces — Produces:**
```rust
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct V3 { pub x: f32, pub y: f32, pub z: f32 }
impl V3 { pub fn new(x,y,z) -> V3; pub fn dot(self,o)->f32; pub fn cross(self,o)->V3; pub fn len(self)->f32; pub fn norm(self)->V3; pub fn from_arr(a:[f32;3])->V3 }
// std::ops Add, Sub, Mul<f32>, Neg
pub struct Camara { pub objetivo: V3, pub yaw: f32, pub pitch: f32, pub distancia: f32, pub fov_y: f32 }
impl Camara {
    pub fn para_sala(s: &Sala) -> Camara; // objetivo (ancho/2, alto_techo/2, largo/2), yaw 0.8, pitch 0.6, distancia 1.8*max(ancho,largo), fov_y 50° in rad
    pub fn ojo(&self) -> V3;              // objetivo + distancia*(cos p sin y, sin p, cos p cos y)
    pub fn proyectar(&self, p: V3, rect: egui::Rect) -> Option<(egui::Pos2, f32)>; // None if depth < 0.05; depth = (p-ojo)·forward; screen y grows downward; aspect from rect
    pub fn rayo(&self, s: egui::Pos2, rect: egui::Rect) -> (V3, V3); // (ojo, unit dir); exact inverse of proyectar
    pub fn orbitar(&mut self, d: egui::Vec2); // yaw -= d.x*0.01; pitch += d.y*0.01 clamped [0.05, 1.5]
    pub fn zoom(&mut self, scroll: f32);      // distancia *= exp(-scroll*0.002), clamp [1.0, 100.0]
}
pub fn param_mas_cercano(origen: V3, dir: V3, punto: V3, eje: V3) -> f32; // t such that punto + t*eje is closest to the ray; 0.0 if parallel
```
- [ ] **Step 1: Failing tests:** `objetivo_al_centro` (proyectar(objetivo) ≈ rect.center()); `detras_es_none` (point at ojo + 2*(ojo-objetivo).norm() → None); `rayo_inverso_de_proyectar` (for 3 points, rayo at their pixel, the ray passes within 1e-3 of the point); `param_cercano_recupera_punto` (ray through (2,1,0), line punto (0,1,0) eje X → t ≈ 2.0); `pitch_limitado` (orbitar huge dy → pitch == 1.5). Use rect 800×600 at (0,0).
- [ ] **Step 2–4:** `cargo test geom` FAIL → implement → PASS.

### Task 5: Scene: build, draw, pick, drag (`escena.rs`)

**Interfaces:**
- Consumes: Tasks 2 and 4.
- Produces:
```rust
#[derive(Clone, Debug, PartialEq)]
pub enum Sel { Carril(String), Toma(String), Luz(String) }
pub enum Forma { Poligono(Vec<V3>), Linea(V3, V3, f32 /*px*/), Texto(V3, String) }
pub struct Prim { pub forma: Forma, pub color: egui::Color32, pub sel: Option<Sel>, pub resaltado: bool }
pub const SIN_TOMA: egui::Color32; // gray (140,140,140)
pub fn color_par(i: usize) -> egui::Color32;          // 8 distinct saturated colors, cycles
pub fn centro_luz(m: &Montaje, l: &Luz) -> V3;        // (carril.x, altura_carriles - 0.25, l.z)
pub fn construir(m: &Montaje, seleccion: Option<&Sel>) -> Vec<Prim>;
pub fn dibujar(p: &egui::Painter, cam: &Camara, rect: egui::Rect, prims: &[Prim]);
pub fn elegir(cam: &Camara, rect: egui::Rect, prims: &[Prim], raton: egui::Pos2) -> Option<Sel>;
pub fn arrastrar(m: &mut Montaje, sel: &Sel, origen: V3, dir: V3); // move along the object's line, then clamp
```
Scene content (`construir`): floor quad + 1 m grid lines; four walls as alpha≈30 polygons; ceiling outline lines at `alto_techo`; each carril a thin box (0.06 × 0.06 × largo) at `altura_carriles` + label with id; each luz a body box 0.25×0.30×0.25 at `centro_luz` + a smaller lens box under it + label with codigo; each toma a 0.12×0.12×0.04 box at `Pared::punto` (thin side against the wall) + label; for each luz with toma, a `Linea` from `centro_luz` to the toma point. Pair color: luz at index i in `m.luces` uses `color_par(i)` for its boxes, its line and its toma; luces without toma and free tomas use `SIN_TOMA`. Every prim of a selected object has `resaltado = true`.

`dibujar`: polygons with **any** vertex failing `proyectar` are skipped; sort polygons by mean depth, farthest first; flat shading = color × (0.55 + 0.45·|normal·light|) with fixed light dir (0.3, 1, 0.5).norm(); `resaltado` polygons get a 2 px white stroke. Then lines, then texts (always on top).

`elegir`: among prims with `sel`, polygons containing `raton` in screen space (nearest by mean depth wins); if none, lines within 6 px of `raton`. Walls/floor have `sel = None`.

`arrastrar`: Luz → line (carril.x, h, 0) axis Z → `z = t`; Carril → line (0, h, largo/2) axis X → `x = t`; Toma → line `pared.punto(pos=0)` axis `pared.eje()` → `pos = t`; then `m.ajustar_a_sala()`.

- [ ] **Step 1: Failing tests:** `linea_y_colores_del_par` (default + E1 on C1 connected to T-01 → exactly one `Linea` with `sel == Some(Sel::Luz("E1"))`, its color == `color_par(0)` == the toma box color; a second free toma has color `SIN_TOMA`); `seleccion_resalta` (select C2 → all its prims `resaltado`, none of C1's); `elegir_luz_en_su_pixel` (project `centro_luz`, `elegir` there → `Some(Sel::Luz("E1"))`); `arrastrar_luz_sobre_carril` (ray from `cam.rayo` at the pixel of (1.5, 3.55, 1.0) → luz z ≈ 1.0 ± 0.01); `arrastrar_toma_recorta` (ray aimed at a point 10 m past the Sur wall's end → pos == ancho); `dibujar_con_camara_dentro_no_falla` (camara distancia 1.0 inside the room, `dibujar` on a `Painter` from a headless `egui::Context::run` frame, no panic).
- [ ] **Step 2–4:** `cargo test escena` FAIL → implement → PASS.

### Task 6: App UI (`app.rs`)

**Interfaces:**
- Consumes: everything above.
- Produces: final `App`.

State: `montaje`, `ruta: Option<PathBuf>`, `sucio: bool`, `inventario`, `camara`, `seleccion: Option<Sel>`, `arrastrando: bool`, `pendiente: Option<Accion>` (`Nuevo | Abrir | Cerrar`), `mensaje: Option<String>` (modal error/warning text), `ver_inventario: bool`.

- [ ] **Step 1: Top bar:** buttons `Nuevo`, `Abrir…`, `Guardar`, `Guardar como…`, `Inventario`. Window title via `ViewportCommand::Title` = `"Sala de luces — <nombre archivo | Sin título>"` + `" *"` when `sucio`. Files filter `"Montaje de sala" / ["json"]`, default name `montaje.sala.json`. Save success → `sucio = false`; failure → `mensaje`, `sucio` unchanged. Open: `cargar`; Err → `mensaje`, current montaje untouched; Ok with warnings → show them in `mensaje`; reset camara with `para_sala`.
- [ ] **Step 2: Unsaved guard:** `Nuevo`/`Abrir` with `sucio` set `pendiente` and show a modal "Hay cambios sin guardar" with `Guardar` / `Descartar` / `Cancelar`. Window close: when `close_requested` and `sucio`, send `CancelClose` and show the same modal with `pendiente = Cerrar`; after Guardar (only if the save succeeded) or Descartar, perform the pending action (`Cerrar` → `ViewportCommand::Close`).
- [ ] **Step 3: Left panel:** Sala section with `ancho`, `largo`, `alto_techo`, `altura_carriles` numeric fields (helper `fn numero(ui, v: &mut f32, rango) -> bool` using `DragValue` with speed 0.01, suffix `" m"`, and a custom parser that accepts `,` as decimal separator); any change → `ajustar_a_sala`, `sucio = true`. Lists Carriles / Tomas / Luces (selectable labels; click sets `seleccion`), each with `Añadir` and `Borrar` (on the selected one). `Añadir` luz opens a picker of inventory equipos not yet placed (shows `codigo — marca modelo`), placed on the selected carril or the first one; no carriles → message. Every `Err` goes to `mensaje`.
- [ ] **Step 4: Right panel (selected):** Carril: id (text, applied with `renombrar_carril` on Enter / focus lost, Err → `mensaje` and revert), `x`. Toma: código (same with `renombrar_toma`), pared combo (`Pared::TODAS`), `pos`. Luz: código/marca/modelo read-only, carril combo, `z`, toma combo with `"(sin toma)"` + `tomas_libres_para` → `conectar`. Edits set `sucio`.
- [ ] **Step 5: Inventory window** (`egui::Window`, toggled by `Inventario`): table of equipos with editable fields applied via `editar`, `Borrar`, and a row to add (`anadir`). If `inventario.solo_lectura`, show in red "No se pudo leer el inventario; no se guardarán cambios" and disable editing. Startup load error → `mensaje`.
- [ ] **Step 6: Central 3D view:** allocate the whole rect with `Sense::click_and_drag()`; build `construir` each frame and `dibujar`. Primary click → `seleccion = elegir(..)`. Primary drag start: pick under pointer; if it hits something, select it and set `arrastrando`; while dragging call `arrastrar` with `camara.rayo(pointer)` and set `sucio`. Secondary drag → `camara.orbitar(delta)`. Hover scroll → `camara.zoom(scroll_y)`. Small help text in the corner: "Clic: seleccionar · Arrastrar: mover · Botón derecho: girar · Rueda: zoom".
- [ ] **Step 7:** Remove any `#[allow(dead_code)]` added earlier; `cargo test` all PASS; `cargo build` no warnings.
- [ ] **Step 8: Smoke run:** `cargo run` starts, process alive after 5 s, then kill it. Report anything you could not verify visually.

### Task 7: Release build

- [ ] `cargo build --release`; confirm `target\release\sala-luces.exe` exists and runs (alive after 5 s, kill). Report its size.
