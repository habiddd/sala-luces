# Catálogo de producción e importación — plan

**Spec (binding, approved 2026-11-03):** docs/superpowers/specs/2026-11-03-catalogo-e-importar-design.md — read it.

Global: Spanish UI; meters; Y up; no panics on user input or file content; corrupt-file protections keep working; old montajes/inventories still load (serde defaults); `cargo test` passes and `cargo build` zero warnings; idle CPU stays ~0; confirm egui 0.37 / gltf 1.4 / tobj 4 APIs in `~/.cargo/registry/src/*/` before use; tests never write under %APPDATA% (use temp_dir); no git; don't build release; only stop processes you start.

---

## Task 1 — built-in catalogue (`modelo.rs`, `escena.rs`, `app.rs`, `inventario.rs`)

- `TipoObjeto` gains the 24 forms of the spec table (variant names in ASCII CamelCase, e.g. `CamaraTripode, Tripode, Slider, Grua, Teleprompter, CStand, SoporteLuz, SoftboxPie, Paraguas, Reflector, Bandera, PanelLed, Fondo, Ciclorama, Sofa, Escritorio, Taburete, Planta, Estanteria, MicrofonoPie, Boom, MonitorSoporte, TvSoporte, Altavoz`). `TODOS` lists all 28; `nombre()` returns the Spanish names of the spec table; `medidas()` the spec defaults; new `pub fn grupo(self) -> &'static str` → "Básicos" (Silla, Mesa, Maniquí, Caja) or the spec group name.
- `CATEGORIAS_BASE` gains "Cámaras y soportes", "Iluminación de suelo", "Escenografía", "Audio y monitores" (non-deletable like the others; inventory load already prepends missing base categories). `tipo_para_categoria`: those 4 map to CamaraTripode, SoporteLuz, Fondo, MicrofonoPie.
- `Montaje::cambiar_forma_objeto(&mut self, nombre: &str, tipo: TipoObjeto) -> Result<(), String>`: sets tipo; if it changed, reset medidas to `tipo.medidas()`; `ajustar_a_sala`. (Categoría stays.)
- `escena.rs` `partes(tipo)`: design each new form from boxes in the same fraction style as Silla/Mesa (centre and size as fractions of ancho/alto/fondo, front = +Z). Thin vertical poles/legs as thin boxes; a "cylinder"/disc may be a box or an 8-sided prism if you add a small helper. Every part must stay inside the unit bounding box (|x| ≤ 0.5w, 0 ≤ y ≤ h, |z| ≤ 0.5d). Make each recognizable (see spec examples). Colours: one muted colour per group.
- UI: Objetos → Añadir picker shows the generic forms grouped by `grupo()` (collapsible or headings), then the inventory categories as now. Right panel: a "Forma" ComboBox with all `TipoObjeto::TODOS` grouped by `grupo()` → `cambiar_forma_objeto`; keep "Categoría" combo above it.
- Tests: `catalogo_completo` (28 forms, unique names, medidas > 0); `partes_dentro_de_su_caja` (for every form, every part's box lies inside the unit bounding box ±0.001); `cambiar_forma_resetea_medidas`; `categorias_base_nuevas` (fresh inventory has the 8 base categories).

## Task 2 — import + library (`Cargo.toml`, new `src/importar.rs`, `modelo.rs`)

- Add deps `gltf = "1.4"` (default features; must read .glb and .gltf with embedded or adjacent buffers) and `tobj = "4"`.
- `modelo.rs`:
```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Malla { pub nombre: String, pub vertices: Vec<[f32; 3]>, pub triangulos: Vec<[u32; 3]>,
                   pub colores: Vec<[u8; 3]> /* one per triangle */, pub tam: [f32; 3] /* ancho, alto, fondo */ }
// Objeto: #[serde(default)] pub malla: Option<String>   (name of an imported model; when Some, it is drawn instead of `tipo`)
// Montaje: #[serde(default)] pub modelos: std::collections::BTreeMap<String, Malla>
impl Montaje {
    pub fn colocar_malla(&mut self, m: &Malla) -> String; // inserts m into modelos (if absent), creates objeto: nombre = first free "<m.nombre> <n>", tipo Caja, malla Some(m.nombre), categoria "", centre of room, ancho/alto/fondo = m.tam (clamped by ajustar), returns its nombre
}
```
  `guardar` keeps only models used by some objeto in `modelos` (drop unused before writing — on a clone, don't mutate self). `reparar`: an objeto whose `malla` isn't in `modelos` keeps `malla` (the app may resolve it from the library) — no data dropped.
- `src/importar.rs`:
```rust
pub fn importar(ruta: &Path) -> Result<(Malla, Vec<String>), String>; // by extension .glb/.gltf/.obj (case-insensitive); else Err. Warnings e.g. simplification, textures ignored.
fn normalizar(m: &mut Malla);       // translate so min y = 0 and x/z centred at 0; tam = extents; Err upstream if no triangles or non-finite values
fn simplificar(m: &mut Malla, max: usize); // if triangulos > max (20_000): vertex clustering on a uniform grid sized so the result ≤ max (grow cell until it fits), drop degenerate triangles, keep each surviving triangle's colour
pub fn ruta_biblioteca() -> PathBuf;  // %APPDATA%\SalaLuces\modelos (fallback "modelos")
pub fn guardar_en_biblioteca(dir: &Path, m: &Malla) -> Result<(), String>; // <dir>/<nombre>.json, atomic (tmp + rename), create_dir_all; nombre sanitized for file names
pub fn cargar_biblioteca(dir: &Path) -> (Vec<Malla>, Vec<String>);       // unreadable files skipped with a warning; missing dir → empty
pub fn nombre_libre(base: &str, usados: &[String]) -> String;            // "Silla" → "Silla (2)" if taken
```
  glTF: walk all scene nodes applying node transforms (world matrix), all triangle primitives; colour = material base_color_factor (sRGB-ish ×255); missing material → light grey. OBJ: all models; colour from MTL `diffuse` if available, else light grey; missing MTL is a warning, not an error. Both: Y up, meters, no axis swap.
- Tests (build fixtures in temp_dir from strings): `importar_obj_cubo` (8 v / 12 tris cube text 2×1×3 offset → tam [2,1,3], min y 0, centred), `importar_obj_con_mtl_color`, `importar_gltf_triangulo` (minimal .gltf JSON with base64 data URI buffer, one triangle, base colour factor red → colour [255,0,0]), `importar_extension_desconocida_falla`, `importar_archivo_roto_falla`, `simplificar_respeta_limite` (generated grid mesh of 30 000 tris → ≤ 20 000, > 0), `biblioteca_guardar_y_cargar`, `colocar_malla_crea_objeto`, `guardar_solo_modelos_usados`, `montaje_viejo_sin_modelos_carga`.

## Task 3 — draw imported models + UI (`escena.rs`, `app.rs`)

- `construir`: an objeto with `malla: Some(n)` and `m.modelos[n]` present → one `Forma::Poligono` per triangle: vertex scaled by (ancho/tam[0], alto/tam[1], fondo/tam[2]) (guard tam ≤ 1e-6 → 1), rotated like other objects, the triangle's colour, `sel: Some(Sel::Objeto(nombre))`; name label above. If the malla is missing → draw the Caja form (no panic).
- App state: `biblioteca: Vec<Malla>` loaded at startup from `ruta_biblioteca()` (warnings → message). When a montaje is opened, any `modelos` not in the library are saved to it and added; any objeto whose malla is not in `montaje.modelos` but is in the library gets it copied in; otherwise warn once ("se dibuja como caja").
- Objetos section: button "Importar modelo…" → `rfd` file dialog (filter "Modelos 3D": glb, gltf, obj) → `importar` → if a library model has the same name, use `nombre_libre` → save to library (Err → message, stop) → add to `biblioteca` → `colocar_malla` → select it, sucio; warnings → message.
- Objetos → Añadir picker: new group "Importados" listing library models → copy the Malla into the montaje via `colocar_malla`.
- Right panel "Forma" combo: also lists library models under "Importados"; choosing one sets `objeto.malla = Some(name)` (copy the Malla into `montaje.modelos`), medidas = its `tam`; choosing a built-in form sets `malla = None` and uses `cambiar_forma_objeto`.
- Performance check: with an imported model of 20 000 triangles visible, interaction stays responsive; idle CPU ~0.
- Tests (escena): `malla_se_dibuja_escalada` (one-triangle Malla tam [1,1,1], objeto ancho 2 → the triangle's world extent along X doubles), `malla_ausente_dibuja_caja`, `objeto_malla_seleccionable` (elegir at a triangle's projected centre → Objeto).
