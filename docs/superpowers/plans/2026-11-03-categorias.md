# Inventario por categorías — plan

**Goal:** El inventario se divide en categorías (Luces, Mesas, Sillas, Maniquíes por defecto; el usuario puede añadir más). Las luces se colocan desde la categoría Luces; los objetos del suelo también se pueden colocar desde el inventario (forma según su categoría), además de los objetos genéricos de siempre.

**Approved by user (2026-11-03).** Claude's calls: a category can only be deleted when empty; código unique across the whole inventory; existing inventory goes to "Luces"; an inventory object is drawn by its category's shape (Mesas→Mesa, Sillas→Silla, Maniquíes→Maniqui, any other→Caja); the same equipo can't be placed twice in a montaje (as a luz or as an objeto).

Global: Spanish UI; meters; only existing dependencies; no panics on user input/file content; corrupt-inventory protection (solo_lectura, never overwrite) must keep working; `cargo test` passes and `cargo build` has zero warnings; confirm egui 0.37 APIs in `~/.cargo/registry/src/*/egui-0.37*`; no git (skip commits); don't build release and don't kill processes.

---

## Task A — data (`src/modelo.rs`, `src/inventario.rs`)

`modelo.rs`:
```rust
pub const CATEGORIA_LUCES: &str = "Luces";
pub const CATEGORIAS_BASE: [&str; 4] = ["Luces", "Mesas", "Sillas", "Maniquíes"];
pub struct Equipo { pub codigo, pub marca, pub modelo: String,
                    #[serde(default = "categoria_luces")] pub categoria: String }   // old files → "Luces"
pub fn tipo_para_categoria(cat: &str) -> TipoObjeto; // "Mesas"→Mesa, "Sillas"→Silla, "Maniquíes"→Maniqui, else Caja
// Objeto gains (both #[serde(default)]): pub marca: String, pub modelo: String   (empty for generic objects)
impl Montaje {
    pub fn colocar_objeto(&mut self, e: &Equipo) -> Result<(), String>;
    // nombre = e.codigo (trimmed); tipo = tipo_para_categoria(&e.categoria); copies marca/modelo;
    // x = ancho/2, z = largo/2, rot 0, medidas = tipo.medidas();
    // Err if the código is already used by any objeto nombre OR any luz código.
    pub fn equipo_colocado(&self, codigo: &str) -> bool; // true if a luz or objeto uses it
}
```
`colocar_luz` must also reject a código already used as an objeto nombre. Update every `Equipo { .. }` literal in the code and tests (add `categoria`).

`inventario.rs`:
```rust
pub struct Inventario { pub equipos: Vec<Equipo>, #[serde(default)] pub categorias: Vec<String>, ruta, pub solo_lectura }
impl Inventario {
    pub fn anadir_categoria(&mut self, nombre: &str) -> Result<(), String>; // trim; Err empty or duplicate (case-insensitive); saves (revert on save error)
    pub fn borrar_categoria(&mut self, nombre: &str) -> Result<(), String>; // Err if any equipo has it; saves
    pub fn de_categoria(&self, cat: &str) -> Vec<&Equipo>;                // in list order
}
```
- On load (and for a new/empty inventory): make sure the 4 `CATEGORIAS_BASE` exist (prepend missing ones in that order), and append any category used by an equipo that isn't in the list. Equipo categoria is trimmed; empty → "Luces". Keep the existing load messages / solo_lectura behaviour.
- `anadir` / `editar` reject an equipo whose categoria isn't in `categorias`.
- Saved JSON: `{"equipos":[...], "categorias":[...]}`.

Tests: `equipo_viejo_va_a_luces` (inventory JSON without categoria/categorias → equipo categoria "Luces", categorias == the 4 base); `anadir_y_borrar_categoria` (add "Trípodes" ok, " trípodes " duplicate Err, borrar with an equipo in it Err, borrar empty ok, persists after reload); `anadir_rechaza_categoria_inexistente`; `colocar_objeto_desde_inventario` (equipo "M-01" Mesas → objeto nombre "M-01", tipo Mesa, marca/modelo copied, medidas of Mesa); `no_colocar_dos_veces` (same código as luz then as objeto → Err, and vice versa); `objeto_viejo_sin_marca_carga` (montaje JSON whose objetos lack marca/modelo loads).

---

## Task B — UI (`src/app.rs`)

- **Inventario window:** one `CollapsingHeader` per category (in `categorias` order, default open), title `"<cat> (<n>)"`; inside, the existing row editor (código, marca, modelo, Borrar) for that category's equipos and an add row that creates the equipo in that category. Each category header has "Borrar categoría" (Err → mensaje). At the bottom: text field + "Añadir categoría". Read-only inventory: everything shown as labels, no add/delete (as now).
- **Luces → Añadir** picker: only equipos of category "Luces" not yet placed (`equipo_colocado`).
- **Objetos → Añadir** picker: first the 4 generic types (as now); then, for every category except "Luces", a heading with its not-yet-placed equipos (`código — marca modelo`) → `colocar_objeto`, select it, sucio. Err → mensaje.
- **Right panel for an objeto:** if `marca` or `modelo` is non-empty, show "Marca" and "Modelo" as read-only labels under Tipo.

Smoke run the debug build (Start-Process -PassThru, 5 s alive, stop that Id only).
