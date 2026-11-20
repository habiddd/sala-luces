# Simulación real de luces — plan

**Spec (binding, approved 2026-11-03):** docs/superpowers/specs/2026-11-03-luces-specs-design.md — read it, especially "Cómo se consigue una simulación real".

Global: Spanish UI; SI units (m, lux, cd, K, W); Y up; no panics on user input/file content; old montajes/inventories load (serde defaults); corrupt-inventory protection intact; `cargo test` passes; `cargo build` zero warnings; idle CPU ~0 (repaint only on change); no new dependencies (OpenGL via `eframe::glow` / `eframe::egui_glow`, already enabled by the `glow` feature); confirm egui/eframe/glow 0.37 APIs in `~/.cargo/registry/src/*/` before use; tests never write under %APPDATA%; no git; don't build release; only stop processes you start.

---

## Task 1 — data (`modelo.rs`, `inventario.rs`)

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TipoLuz { FresnelLed, PanelLed, Elipsoidal, ParLed, SpaceLight,          // techo
                   MonoluzSoftbox, PanelPie, FresnelPie, TuboLed, Ciclorama }      // piso
impl TipoLuz { pub const TODOS: [TipoLuz; 10]; pub fn nombre(self) -> &'static str; pub fn es_piso(self) -> bool;
               pub fn specs(self) -> SpecsLuz; }   // values of the spec table
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpecsLuz { pub tipo: TipoLuz, pub potencia_w: f32, pub lux_1m: f32,
                      pub apertura_min: f32, pub apertura_max: f32, pub cct_min: f32, pub cct_max: f32, pub cri: f32 }
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AjustesLuz { pub encendida: bool, pub dimmer: f32 /*0..1*/, pub apertura: f32, pub cct: f32,
                        pub giro: f32 /*deg 0..360*/, pub inclinacion: f32 /*deg -90..90*/ }
```
- `Equipo` gains `#[serde(default)] pub specs: Option<SpecsLuz>` (Some for light categories).
- `Luz` (ceiling) gains `#[serde(default = ..)] pub specs: SpecsLuz` (default PanelLed specs) and `pub ajustes: AjustesLuz` (default: encendida, dimmer 1, apertura = apertura_min, cct = clamp(5600), giro 0, inclinacion −90).
- New `LuzPiso { codigo, marca, modelo, specs, ajustes, x, z, altura /*0.5..3, def 1.8*/, toma: Option<String> }`; `Montaje` gains `#[serde(default)] pub luces_piso: Vec<LuzPiso>`; ajustes default inclinacion 0.
- `Toma` gains `#[serde(default)] pub nivel: NivelToma { Carriles (default), Suelo }`; `Pared::punto` height: Carriles → altura_carriles, Suelo → 0.3 (add `pub fn punto_toma(&self, t: &Toma) -> [f32;3]` and use it everywhere tomas are placed).
- `Sala` gains `#[serde(default)]` reflectancias `refl_suelo 0.2, refl_paredes 0.5, refl_techo 0.7` (clamp 0..0.95).
- Rules: `conectar` (techo) only to Carriles tomas; new `conectar_piso` only to Suelo tomas; one luz per toma across both vecs; `equipo_colocado` also checks luces_piso; `colocar_luz` copies `e.specs` (or PanelLed default) and default ajustes; new `colocar_luz_piso(&Equipo) -> Result<(), String>` (centre of room, altura 1.8); `quitar_luz_piso`; `ajustar_a_sala` clamps ajustes (dimmer 0..1, apertura in [min,max], cct in [min,max], giro rem_euclid 360, inclinacion −90..90) and luz piso x/z/altura; `reparar` handles luces_piso like luces (dup codes, missing toma refs, wrong-level toma → None with warning).
- Categories: `CATEGORIA_LUCES` → "Luces techo"; add "Luces piso"; both base. Inventory load: equipos with categoria "Luces" → "Luces techo"; categories list "Luces" renamed. Light equipos without specs get the PanelLed (techo) / PanelPie (piso) specs on load, with one warning.
- Default montaje: add ceiling luz "EJ-T1" (FresnelLed specs, marca "Ejemplo", modelo "Fresnel LED 150W", carril C2, z such that it is above-behind the table, inclinacion/giro aimed at the table centre (compute), dimmer 0.8, cct 5600) and luz piso "EJ-P1" (MonoluzSoftbox, x 1.8, z 3.0, altura 1.8, aimed at the table, dimmer 0.7). Add `pub fn apuntar(desde: [f32;3], hacia: [f32;3]) -> (f32 /*giro*/, f32 /*inclinacion*/)` with giro measured in the X–Z plane from +Z toward +X.
- Direction convention (used by Tasks 2–4): `pub fn direccion(giro, inclinacion) -> [f32;3]` = (cos(incl)·sin(giro), sin(incl), cos(incl)·cos(giro)), degrees in.
- Tests: `tipos_luz_tabla` (10 types, values of the spec row for FresnelLed and TuboLed), `ajustes_se_limitan`, `apuntar_y_direccion_inversas` (direccion(apuntar(a,b)) ≈ (b−a).norm()), `luz_techo_solo_toma_carriles`, `luz_piso_solo_toma_suelo`, `una_luz_por_toma_entre_techo_y_piso`, `inventario_luces_viejas_a_luces_techo`, `montaje_viejo_sin_luces_piso_carga`, `default_trae_luces_de_ejemplo`, `guardar_y_cargar_con_luces_piso`.

## Task 2 — photometry (new `src/fotometria.rs`, pure, CPU)

```rust
pub fn cct_a_rgb(k: f32) -> [f32; 3];               // linear RGB of a blackbody at k (1000..40000 clamped), normalized so luminance (0.2126 r + 0.7152 g + 0.0722 b) == 1
pub fn intensidad_cd(s: &SpecsLuz, a: &AjustesLuz) -> f32; // 0 if !encendida; lux_1m × dimmer × (tan(apertura_min/2)/tan(apertura/2))²
pub fn perfil(theta_deg: f32, apertura_deg: f32) -> f32;   // exp(−ln2·(θ/(apertura/2))²), 0 beyond 1.6·apertura/2
pub struct FuenteLuz { pub pos: [f32;3], pub dir: [f32;3], pub cd: f32, pub apertura: f32, pub color: [f32;3] }
pub fn fuentes(m: &Montaje) -> Vec<FuenteLuz>;      // all ceiling + floor lights (positions: ceiling = centro_luz-like point under its carril; floor = (x, altura, z))
pub fn iluminancia(p: [f32;3], n: [f32;3], f: &FuenteLuz) -> f32; // cd·perfil·max(0, n·l)/d², no shadow
pub fn flujo_lm(f: &FuenteLuz) -> f32;              // numeric integral of cd·perfil over the sphere (e.g. 64×32 samples)
pub fn indirecta_lux(m: &Montaje, fs: &[FuenteLuz]) -> f32; // Φ·ρ_media/(A·(1−ρ_media)), ρ_media area-weighted from sala reflectancias, A = total room surface
pub fn ev100(iso: f32, f_num: f32, t_s: f32) -> f32;       // log2(N²/t · 100/ISO)
pub fn exposicion(ev100: f32) -> f32;                      // 1 / (1.2 · 2^ev100)
pub fn ev100_auto(lum_media: f32) -> f32;                  // log2(lum_media · 100 / 12.5)
pub fn aces(x: [f32;3]) -> [f32;3];                        // Narkowicz ACES fit, clamp 0..1
pub fn color_lux(lux: f32) -> egui::Color32;               // false-colour scale: 0 black/blue … 50 blue, 200 green, 500 yellow, 1000 orange, 2000+ red (log interpolation)
pub fn lux_en(m: &Montaje, p: [f32;3], n: [f32;3], tris: &[[[f32;3];3]]) -> f32; // Σ iluminancia with shadow ray test against tris (Möller–Trumbore, skip hits within 1 cm of p) + indirecta
```
Tests (exact values): `inverso_del_cuadrado` (1000 cd straight down at 2 m → 250 lux ±0.1%), `coseno_de_incidencia` (60° → half), `perfil_mitad_en_half_peak` (perfil(apertura/2) = 0.5), `abrir_haz_baja_intensidad`, `apagada_es_cero`, `cct_calida_y_fria` (3200 K: r > g > b; 6500 K: |r−b| small (<0.1 after normalization) ; luminance 1), `ev100_valores` (ISO100 f/1 1 s → 0; ISO 400 f/2.8 1/50 → ≈ 6.6), `aces_monotona`, `sombra_bloquea` (a quad between light and point → only indirect remains), `indirecta_positiva_y_crece_con_reflectancia`.

## Task 3 — scene geometry for GPU (`escena.rs`)

- Floor-light shape: tripod legs + vertical pole to `altura` + fixture box oriented by `direccion` (colour by type, selectable as `Sel::LuzPiso(codigo)` — add that Sel variant; handle it in elegir/arrastrar (drag on floor plane like objects)/cotas (like objects)/app matches minimally so it compiles; the real editor comes in Task 5).
- Ceiling luces: fixture box oriented by `direccion` (rotate the existing light body so it points where aimed).
- `pub struct VerticeGpu { pub pos: [f32;3], pub normal: [f32;3], pub albedo: [f32;3] }` and `pub fn malla_gpu(m: &Montaje) -> Vec<VerticeGpu>`: every solid polygon of `construir(m, None)` (floor, walls, ceiling plane, carriles, objects, imported meshes, light bodies) triangulated as a fan; normal from the polygon (flip to face the room interior for walls/floor/ceiling); albedo = linear (sRGB→linear) of the prim colour, but floor/walls/ceiling use grey = their reflectance. Exclude texts/lines/cotas. Also `pub fn triangulos(m) -> Vec<[[f32;3];3]>` (same geometry, positions only) for CPU shadow rays. Cache-friendly: pure function of the Montaje.
- Tests: `malla_gpu_normales_unitarias`, `suelo_mira_arriba` (floor triangles normal ≈ +Y), `luz_piso_seleccionable`, `luz_apunta_a_su_direccion` (body's long axis ≈ direccion).

## Task 4 — GPU renderer (new `src/render_gl.rs`)

Pattern: egui's `custom3d_glow` demo — `Arc<Mutex<Renderer>>` created from `cc.gl` in `App::new`; the central view adds `egui::PaintCallback { rect, callback: Arc::new(egui_glow::CallbackFn::new(move |_info, painter| renderer.lock().paint(painter.gl(), ...))) }`. Restore GL state you change (program, VAO, buffers, depth test, framebuffer, viewport, blend, cull) before returning.
- Upload `malla_gpu` into a VBO only when the Montaje changed (hash or a `version` counter in App).
- Shadow pass: for each lit FuenteLuz (max 16), render depth from the light (perspective fov = min(170°, 1.6·apertura), near 0.05, far 30) into a layer of a depth texture array 1024²; slope-scaled bias.
- Main pass (MSAA if the default framebuffer has it; otherwise fine): vertex → world pos/normal; fragment: E = Σ cd·perfil(θ)·max(0,n·l)/d²·color·sombra(PCF 3×3) + E_ind·(average light colour); radiance = albedo·E/π; mode Realista: rgb = aces(radiance·exposicion) → sRGB; mode Mapa de lux: colour = false-colour of luminance(E) (same scale as `color_lux`, implemented in GLSL).
- Haze (optional toggle): additive translucent cones per light (fresnel-ish alpha), drawn after opaque, depth test on, depth write off.
- Uniforms come from `fotometria::fuentes` and `exposicion`; pass arrays (≤16 lights) as uniform arrays.
- GLSL version: query `gl.version()`; use `#version 330 core` (desktop) — if shader compile/link fails, return Err from `Renderer::new` / set a flag so the app falls back to Esquema and shows the error once.
- Auto exposure: CPU-side estimate from `lux_en` at the camera target region (avg of a few floor samples × average albedo / π) → `ev100_auto`; recompute only when the scene/camera changes.
- Tests: pure helpers only (`matriz_vista`, `matriz_proyeccion`, `matriz_luz` correctness; uniform packing length/order). Visual verification is done by the controller with screenshots.

## Task 5 — UI (`app.rs`, `inventario.rs` UI parts)

- Top/left: view mode selector **Realista / Mapa de lux / Esquema** (default Realista; Esquema forced if the renderer failed), checkbox "Haz visible (humo)". Camera exposure panel: Auto (default) or manual ISO (100–12800), f/ (1.4–22), obturación (1/1000–1 s, shown as 1/x).
- Sala section: reflectancias suelo/paredes/techo (0–0.95, 2 decimals).
- Tomas: nivel combo (Carriles / Suelo) in the toma editor; list shows "(suelo)".
- Luces (techo) editor: specs read-only (tipo, W, lux@1m, rangos, CRI), ajustes: Encendida, Dimmer % slider, Apertura slider (min–max; disabled if fixed), Temperatura slider (min–max; disabled if fixed) with a colour swatch from `cct_a_rgb`, Giro, Inclinación, "Apuntar a…" combo listing objetos (sets giro/incl via `apuntar`), toma combo (only Carriles tomas). Show "Intensidad: N cd · Potencia: N W".
- New left section **"Luces de piso"** with Añadir (picker of "Luces piso" inventory equipos not placed) / Borrar / list; editor like the ceiling one plus X, Z, Altura; toma combo only Suelo tomas.
- Inventory light tabs ("Luces techo", "Luces piso"): extra columns Tipo (combo filling `TipoLuz::specs`), W, lux@1m, apertura min/max, CCT min/max, CRI; only types matching the tab (`es_piso`).
- Lux readout: while hovering the 3D view, raycast to the surface under the cursor (use the scene triangles), show "≈ N lux" next to the cursor (CPU `lux_en`); recompute only when the pointer moves.
- Smoke + idle CPU check (< 200 ms in 4 s).
