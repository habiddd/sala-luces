# Presets de pared del usuario — plan

**Goal:** En la ventana "Paredes" se puede guardar el acabado de la pared elegida como preset nuevo, con un nombre. El preset queda en la app (sirve para cualquier escena) y también en el archivo del montaje **si alguna pared lo usa**.

Sigue el mismo patrón que los modelos importados: `Montaje.modelos` + biblioteca en `%APPDATA%\SalaLuces` + `incorporar_modelos` al abrir + `guardar` que solo conserva lo usado.

## Reglas
- Un nombre válido se recorta (trim), no queda vacío y no coincide con un preset de fábrica (`ACABADOS`), sin distinguir mayúsculas. Si ya existe un preset del usuario con ese nombre, se reemplaza.
- "Usado" = alguna de las 4 paredes tiene exactamente ese `Acabado` (la misma igualdad que ya usa el desplegable).
- Al abrir un montaje, sus presets cuyo nombre no está en la biblioteca se añaden a ella y se guardan. Si el nombre ya existe, gana la biblioteca (igual que con los modelos).

## Task A — `src/modelo.rs`
- En `Montaje`, después de `modelos`:
  ```rust
  /// Presets de pared del usuario que usa alguna pared; los montajes viejos no los tienen.
  #[serde(default)]
  pub acabados: std::collections::BTreeMap<String, Acabado>,
  ```
  Rellenar en `Montaje::default()` y en cualquier literal `Montaje { … }` (grep).
- `pub fn nombre_preset_valido(nombre: &str) -> Option<String>`: devuelve el nombre recortado, o `None` si queda vacío o coincide (ignorando mayúsculas) con un nombre de `ACABADOS`.
- `Montaje::guardar`: además de filtrar `modelos`, `copia.acabados.retain(|_, a| paredes usadas contienen *a)`. Usa `copia.sala.acabado(p)` para las 4 `Pared::TODAS`.
- Pruebas:
  - `guardar_solo_presets_usados`: dos presets en `acabados` y Sur con el acabado del primero → al guardar en un archivo temporal y cargarlo solo queda el primero. Copia el patrón de archivos temporales de las pruebas que ya existen para `guardar`/`cargar`.
  - `montaje_viejo_sin_acabados`: el JSON viejo existente carga con `acabados` vacío.
  - `nombre_preset_valido_reglas`: `"  Mi verde "` → `Some("Mi verde")`; `""` y `"   "` → `None`; `"pintura blanca mate"` → `None`.

## Task B — `src/config.rs` (biblioteca de presets)
```rust
pub fn ruta_acabados() -> PathBuf  // %APPDATA%\SalaLuces\acabados.json, o "acabados.json" sin APPDATA (igual que ruta_config)
/// Ok(vacío) si el archivo no existe; Err con el mensaje si no se puede leer o el JSON está roto.
pub fn cargar_acabados(ruta: &Path) -> Result<BTreeMap<String, Acabado>, String>
/// Crea la carpeta si falta; escribe a `.tmp` y renombra, como `Montaje::guardar`.
pub fn guardar_acabados(ruta: &Path, a: &BTreeMap<String, Acabado>) -> Result<(), String>
```
Quita el BOM al leer, como `Montaje::cargar`. Al cargar, limita cada `reflexion` a 0..=0.95.
- Pruebas (en una carpeta temporal única bajo `std::env::temp_dir()`, como las que ya hay):
  - ida y vuelta: guardar 2 presets → cargar → iguales;
  - archivo inexistente → `Ok` vacío;
  - JSON roto → `Err`.

## Task C — `src/app.rs`
- Campos nuevos: `acabados_usuario: BTreeMap<String, Acabado>`, `acabados_bloqueados: bool` (true si `cargar_acabados` falló, para no sobrescribir el archivo del usuario) y `nombre_preset: String`.
- En `App::new`: `cargar_acabados(&ruta_acabados())`; si da `Err(e)`, `avisos.push(format!("No se pudieron leer los presets de pared: {e}"))`, usar una biblioteca vacía y marcarla bloqueada.
- Método `fn guardar_presets(&mut self)`: si está bloqueada, no hace nada; si no, llama a `guardar_acabados`, y si da error usa `self.aviso(e)`.
- `incorporar_modelos` (o un `incorporar_acabados` llamado justo después, en `abrir`, ~línea 560): los presets del montaje cuyo nombre no está en `acabados_usuario` se añaden a la biblioteca y se llama a `guardar_presets`.
- Ventana Paredes:
  - Desplegable "Material": primero `ACABADOS`; después, si hay presets del usuario, `ui.separator()` y los de `acabados_usuario`. El texto seleccionado busca primero en los de fábrica y luego en los del usuario, y si no hay coincidencia muestra `"Personalizado"`. Elegir un preset del usuario copia su `Acabado` a la pared **y** hace `self.montaje.acabados.insert(nombre, acabado)`, para que se guarde con la escena.
  - Debajo del Grid: una fila con `ui.text_edit_singleline(&mut self.nombre_preset)` (pista: `"Nombre del preset"`) y un botón `"Guardar como preset"`, activo solo si `nombre_preset_valido` da `Some`. Al pulsarlo: insertar `(nombre, acabado de la pared editada)` en `acabados_usuario` y en `self.montaje.acabados`, llamar a `guardar_presets()` y a `marcar_cambio()`, y vaciar `nombre_preset`. Con el nombre de un preset de fábrica, mostrar debajo en gris `"Ese nombre es de un preset de fábrica"`.
- No añadas borrar ni renombrar presets.
