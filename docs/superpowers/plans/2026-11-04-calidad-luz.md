# Calidad de luz: Alta y Fotorrealista (trazado de rayos) + Configuración — plan

**Approved by user (2026-11-04):** presets Rápida / Alta / Fotorrealista in a Configuración window (persisted); options: preset, muestras máx (64–2048), rebotes (1–4), resolución de sombras, suavizado (off/2/4/8), escala de resolución (50–100 %), GPU info. Cross-vendor (AMD/Intel/NVIDIA): OpenGL 3.3 core only, no vendor extensions; dedicated-GPU export; work split per frame to avoid driver timeouts (TDR); automatic step-down on failure; default preset by detected GPU.

Global: Spanish UI; no panics; `cargo test` passes, `cargo build` zero warnings; **idle CPU/GPU ~0 once a frame is final** (path tracer stops repainting when it reaches max samples); tests never write under %APPDATA%; no git; don't build release; only stop processes you start; confirm egui/eframe/glow 0.37 APIs in `~/.cargo/registry/src/*/`. Existing raster renderer = "Rápida" (src/render_gl.rs). Physics must stay consistent with src/fotometria.rs (cd, inverse square, cosine, half-peak gaussian profile, CCT colour, exposure EV100, ACES, sRGB).

---

## Task 1 — Configuración + GPU readiness (`src/config.rs` new, `main.rs`, `app.rs`, `render_gl.rs`)

- `main.rs`: export for hybrid laptops (exact symbols, read by NVIDIA/AMD drivers):
  ```rust
  #[unsafe(no_mangle)] pub static NvOptimusEnablement: u32 = 1;
  #[unsafe(no_mangle)] pub static AmdPowerXpressRequestHighPerformance: i32 = 1;
  ```
  (edition 2024 needs `#[unsafe(no_mangle)]`; verify they appear in the release exe exports later — controller checks.)
- `config.rs`:
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)] pub enum Preset { Rapida, Alta, Fotorrealista }
  #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
  pub struct Configuracion { pub preset: Preset, pub muestras_max: u32 /*64..2048*/, pub rebotes: u32 /*1..4*/,
      pub res_sombras: u32 /*1024|2048|4096*/, pub msaa: u32 /*0|2|4|8*/, pub escala: f32 /*0.5..1.0*/, #[serde(skip)] pub explicita: bool }
  pub fn ruta_config() -> PathBuf; // %APPDATA%\SalaLuces\config.json
  pub fn cargar(ruta: &Path) -> Option<Configuracion>;      // None if missing/unreadable (BOM tolerant); clamp values
  pub fn guardar(ruta: &Path, c: &Configuracion) -> Result<(), String>; // atomic tmp+rename, create_dir_all
  pub fn por_defecto_para(renderer: &str, vendor: &str) -> Configuracion; // integrated (renderer contains "Intel" (UHD/Iris/Arc integrated), or AMD "Radeon(TM)" + "Graphics" without "RX", or "Vega") → Alta, escala 0.75, msaa 2, sombras 2048; dedicated (NVIDIA/GeForce/RTX/Quadro, AMD "RX", Intel "Arc A") → Alta, escala 1.0, msaa 4, sombras 2048. muestras 256, rebotes 2.
  pub fn es_integrada(renderer: &str) -> bool;
  ```
  Tests: `config_ida_y_vuelta`, `config_limita_valores`, `config_corrupta_es_none`, `por_defecto_integrada` ("AMD Radeon(TM) 860M Graphics", "Intel(R) UHD Graphics 770" → integrated, escala 0.75), `por_defecto_dedicada` ("NVIDIA GeForce RTX 4070", "AMD Radeon RX 7800 XT", "Intel(R) Arc(TM) A770 Graphics" → dedicated).
- GPU info: in `Renderer::new` read `gl.get_parameter_string(glow::VENDOR / RENDERER / VERSION)` and store (`pub fn info_gpu(&self) -> (String, String, String)`).
- App: load config at startup; if none, use `por_defecto_para(renderer, vendor)` (don't write it until the user changes something). "Configuración" button in the top bar opens a window: Preset (radio), Muestras máx (slider 64–2048, log), Rebotes (1–4), Resolución de sombras (combo), Suavizado (combo Off/2×/4×/8×), Escala de resolución (slider 50–100 %), GPU: vendor / renderer / OpenGL version (labels), button "Valores recomendados para esta GPU". Any change → save (errors → usual message) and apply. Replace the Realista/Mapa/Esquema radios? No — keep them; Preset only affects Realista (and Mapa de lux uses raster always).
- Step-down: if a preset's renderer fails to initialize (shader compile/link, FBO incomplete), set preset to the next lower one, show "Tu GPU no admite <preset>: se usa <otro>" once, and save.
- Offscreen target: render the 3D view into an FBO of size viewport×escala (with MSAA samples = msaa if > 0, resolved), then blit/draw it into the egui rect (linear filter). This applies to Rápida and Alta. Shadow map size = res_sombras.
- Tests for pure parts only.

## Task 2 — Alta: soft shadows sized by the real emitter (`render_gl.rs`, `modelo.rs`)

- `TipoLuz::radio_emisor(self) -> f32` (m): FresnelLed 0.08, PanelLed 0.30, Elipsoidal 0.05, ParLed 0.10, SpaceLight 0.40, MonoluzSoftbox 0.45, PanelPie 0.20, FresnelPie 0.10, TuboLed 0.05, Ciclorama 0.20; FuenteLuz gains `radio: f32` (fotometria::fuentes fills it).
- PCSS (percentage-closer soft shadows) in the main shader when preset == Alta: blocker search (16 Poisson samples) → penumbra = (d_receiver − d_blocker)·radio/d_blocker → PCF with 32 Poisson samples rotated per pixel (interleaved gradient noise) scaled by penumbra (in shadow-map texels, clamped). Rápida keeps the 3×3 PCF.
- Contact shadows: cheap screen-independent approximation is out of scope; skip SSAO.
- Tests: `radio_emisor_tabla`, plus any pure PCSS helper (e.g. `tamano_penumbra`).

## Task 3 — Fotorrealista: progressive GPU path tracer (`src/trazado.rs` new + shaders, `render_gl.rs`, `app.rs`)

- CPU: build a BVH (SAH or binned SAH, leaf ≤ 4 tris) over `escena::malla_gpu` triangles (positions, normal, albedo; skip shell faces? NO — shell must be included for bounces, but the camera-facing cutaway shell faces (as in raster) must be invisible to PRIMARY rays only: give each triangle a `carcasa` flag; for primary rays skip shell triangles whose interior normal faces away from the eye; secondary rays see everything). Pack nodes and triangles into RGBA32F textures (2D, width 1024/2048) — GL 3.3 compatible (no SSBO). Rebuild only when the scene version changes.
- GPU: fragment shader per pixel of the offscreen target (escala applies): 1 sample per pixel per frame; ray-triangle (Möller–Trumbore) with stack-based BVH traversal (fixed stack 32); Lambert BSDF with cosine-weighted hemisphere sampling; next-event estimation to every light: sample a point on the emitter disk (radio_emisor, oriented by dir), shadow ray via BVH, contribution = cd·perfil(θ)·cos·/d² · colour (same as raster formula, so direct light matches Rápida/Alta); bounces up to `rebotes` with Russian roulette after the 2nd; accumulate into a RGBA32F ping-pong texture (running mean); RNG: PCG hash seeded by pixel+frame.
- Emitters are not visible directly except the fixture bodies (geometry is in the mesh); no environment light.
- Display: mean radiance × exposicion → ACES → sRGB (same as raster), drawn into the egui rect; overlays as today.
- Progressive control in app: reset accumulation when montaje version, camera, viewport size, preset/config or exposure changes; while `muestras < muestras_max` call `ctx.request_repaint()`; at max stop (idle ~0). Show "Muestras: n / max" in a corner.
- TDR safety: one sample per frame; if the frame time of the path-trace pass exceeds ~60 ms (measure with a GL timer query if available — `GL_TIME_ELAPSED` is core in 3.3 — else CPU timing around glFinish only in a debug measurement), automatically reduce escala by 0.1 (min 0.5) and note it in the Configuración window.
- Lux readout and Mapa de lux keep using the CPU/raster paths.
- Tests: `bvh_contiene_todos_los_triangulos`, `bvh_intersecta_como_fuerza_bruta` (random rays vs brute force on a small scene, same nearest t), `empaquetado_texturas_tamano`, `pcg_determinista`.
- Verification: screenshots by the controller (converged Fotorrealista image should show colour bleeding and soft shadows), idle CPU after convergence < 200 ms over 4 s.
