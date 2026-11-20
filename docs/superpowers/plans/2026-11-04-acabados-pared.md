# Acabado por pared (color, reflexión, absorción) — plan

**Goal:** Cada una de las 4 paredes tiene su propio acabado: color, reflexión y absorción. Se edita en una ventana "Paredes" que se abre desde la barra superior. La ventana muestra un plano de la sala con las 4 paredes, y se hace clic en una para elegirla. Hay materiales predefinidos (pintura blanca, pintura verde croma y otros) y también valores manuales.

Reemplaza el desplegable de color único que se añadió antes hoy ([2026-11-04-color-paredes.md](2026-11-04-color-paredes.md)).

## Física (decisión)
- La pared es opaca: la luz que llega se refleja o se absorbe. **absorción = 1 − reflexión.** Se guarda solo `reflexion`; la absorción se muestra y se edita como su complemento, y al mover una se mueve la otra.
- La reflexión es la fracción difusa (LRV/100). El render es lambertiano; no hay brillo especular.
- El color es el tinte. Albedo GPU = `lineal(color) * (reflexion / lum)`, igual que hoy (`albedo_pared`). Si `lum < 1e-4`, usar `[reflexion; 3]`.
- La luz indirecta (fotometria) usa la media de reflexión ponderada por el área de cada pared. El tinte no entra en la indirecta.

## Materiales predefinidos (fuentes: LRV publicados de pinturas; croma calculado del sRGB del color de referencia)
| Nombre | sRGB | reflexión |
|---|---|---|
| Pintura blanca mate | [242, 242, 238] | 0.85 |
| Pintura verde croma | [0, 177, 64] | 0.32 |
| Pintura azul croma | [0, 71, 187] | 0.08 |
| Pintura gris 18 % | [118, 118, 118] | 0.18 |
| Pintura negra mate | [28, 28, 28] | 0.05 |
| Molton negro (tela) | [18, 18, 18] | 0.02 |
| Concreto | [160, 160, 155] | 0.35 |

## Task A — `src/modelo.rs`
```rust
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Acabado {
    /// sRGB, tinte de la pared.
    pub color: [u8; 3],
    /// Fracción de luz reflejada (0..0.95); absorción = 1 − reflexion.
    pub reflexion: f32,
}
pub const ACABADOS: [(&str, Acabado); 7] = [ /* tabla de arriba, mismo orden */ ];
```
- En `Sala`: borrar `color_paredes` del uso normal y añadir:
  ```rust
  /// Acabado de cada pared, en el orden de `Pared` (Norte, Sur, Este, Oeste).
  #[serde(default)]
  pub paredes: Vec<Acabado>,
  ```
  Mantener `refl_paredes` y `color_paredes` **solo para leer montajes viejos**: `#[serde(default = "...", skip_serializing)]`, con doc "solo migración".
- `impl Sala { pub fn acabado(&self, p: Pared) -> Acabado }` → `self.paredes.get(p as usize).copied()`, y si no hay, `Acabado { color: self.color_paredes, reflexion: self.refl_paredes }`.
- `impl Sala { pub fn acabado_mut(&mut self, p: Pared) -> &mut Acabado }`: antes llama a la migración (ver abajo) para que haya 4 entradas.
- Migración en `ajustar_a_sala`: si `paredes.len() != 4`, `paredes = vec![Acabado { color: color_paredes, reflexion: refl_paredes }; 4]`. Después limitar cada `reflexion` con `limita(_, 0.95)`. Ponla en un método privado `fn completar_paredes(&mut self)` de `Sala`, para que la usen tanto `ajustar_a_sala` como `acabado_mut`. Confirmar que `cargar` → `reparar` llama a `ajustar_a_sala`.
- `Montaje::default()`: 4 acabados `{ color: [188,188,188], reflexion: 0.5 }`, para que el aspecto por defecto no cambie. Arreglar el literal `Sala { … }` de las pruebas.
- Borrar `COLORES_PARED`.
- Pruebas:
  - `json_viejo_migra_paredes`: el JSON viejo existente (~línea 2140) da 4 acabados con reflexion 0.5 y color [188,188,188].
  - `json_con_refl_paredes_migra`: `"refl_paredes":0.3` en la sala → las 4 con 0.3.
  - `paredes_independientes_ida_y_vuelta`: poner Sur en verde croma, serializar y deserializar → Sur verde y Norte gris; el JSON no contiene `"refl_paredes"` ni `"color_paredes"`.
  - `ajustar_limita_reflexion`: reflexion 1.5 → 0.95; −1 → 0.

## Task B — `src/escena.rs` y `src/fotometria.rs`
- `albedo_pared(s: &Sala)` → `pub fn albedo(a: Acabado) -> [f32; 3]`, con la misma fórmula usando `a.color` y `a.reflexion`. Borrar `refl_de_color` si queda sin uso.
- `malla_gpu`: el orden de `cerramiento` es [suelo, Sur, Norte, Oeste, Este, techo]. Albedos `[[refl_suelo;3], albedo(acabado(Sur)), albedo(acabado(Norte)), albedo(acabado(Oeste)), albedo(acabado(Este)), [refl_techo;3]]`. **Comprobar ese orden leyendo `cerramiento`.**
- `construir` (Esquema): cada cara de pared con el color de su acabado, alfa 30, en el mismo orden.
- `fotometria::indirecta_lux`: `rho = (refl_suelo·suelo + Σ reflexion_p·area_p + refl_techo·techo) / area`, donde area de Sur/Norte = ancho·alto_techo y de Este/Oeste = largo·alto_techo. El helper de pruebas `sala(refl)` pone las 4 paredes con `refl` en vez de `refl_paredes`.
- Pruebas:
  - Adaptar `albedo_pared_*` a `albedo(Acabado)`.
  - `malla_gpu_pared_por_pared`: Sur verde croma y las demás blancas → los vértices con `carcasa == 1.0` y normal ≈ (0,0,1) (Sur está en z=0 y su normal apunta hacia dentro, +z) tienen g > r; los de normal (0,0,−1) (Norte) tienen r ≈ g ≈ b.
  - `indirecta_sube_con_una_pared_blanca`: todo a 0.2 vs la misma sala con Norte a 0.85 → la indirecta es mayor.

## Task C — `src/app.rs`
- Borrar el desplegable de color de la barra (~líneas 750–771) y la fila "Reflectancia paredes" del panel de la sala (~línea 911). Suelo y techo se quedan.
- En la barra, después de "Configuración": botón `"Paredes"` que alterna `self.ver_paredes` (mismo patrón que `ver_config`, campo nuevo `ver_paredes: bool` + `pared_editada: Pared`, por defecto `Pared::Sur`).
- Ventana `egui::Window::new("Paredes")` con `.open(&mut abierta)` (copiar el patrón de la ventana de configuración ~línea 1681):
  1. **Plano** (`ui.allocate_exact_size(egui::vec2(220.0, 160.0), egui::Sense::hover())`): rectángulo proporcional a ancho (X, horizontal) × largo (Z, vertical), centrado, con **Norte arriba** (z = largo arriba), Sur abajo, Oeste a la izquierda (x = 0) y Este a la derecha. Cada pared es un segmento grueso (6 px; 10 px si está seleccionada) pintado con su color, con contorno blanco si está seleccionada y la letra N/S/E/O fuera del rectángulo. Cada pared es clicable: `ui.interact(rect_de_esa_pared_engordado, id, Sense::click())`; un clic pone `pared_editada`.
  2. Debajo, `ui.heading(pared_editada.nombre())` y un Grid:
     - "Material": ComboBox con `ACABADOS`. El texto es el nombre del preset igual al acabado actual, o `"Personalizado"`. Elegir uno copia el `Acabado` completo.
     - "Color": `egui::color_picker::color_edit_button_srgb(ui, &mut a.color)` (confirmar la API en egui-0.37).
     - "Reflexión": `egui::Slider` 0.0..=0.95 sobre `a.reflexion`, con 2 decimales.
     - "Absorción": Slider 0.05..=1.0 sobre una copia `abs = 1.0 - a.reflexion`; si cambia, `a.reflexion = 1.0 - abs`.
  3. Cualquier cambio → `self.marcar_cambio()`. Usa `self.montaje.sala.acabado_mut(p)`.
- Con `marcar_cambio` ya se rehace la malla GPU y la de trazado (sube `version`); confirmar.
