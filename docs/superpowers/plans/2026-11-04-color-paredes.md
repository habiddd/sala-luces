# Color de paredes — plan

**Goal:** Las paredes de la sala pueden tener un color, elegido en un desplegable de la barra superior. Un solo color para las cuatro paredes.

## Diseño
- El color es el tinte; `refl_paredes` sigue siendo el brillo (y lo usa fotometria para la indirecta, sin cambios).
- Elegir un color en el desplegable también pone `refl_paredes` = luminancia lineal del color (redondeada a 2 decimales, tope 0.95), para que "Negro" sea oscuro de verdad. El deslizador de reflectancia del panel de la sala sigue funcionando después.
- Albedo de pared en GPU = `lineal(color) * (refl_paredes / lum)`, donde `lum = 0.2126 r + 0.7152 g + 0.0722 b` del color lineal. Si `lum < 1e-4`, usar `[refl_paredes; 3]`. Con un color gris el albedo es exactamente `[refl; 3]`, así que el default se ve igual que hoy.

## Task A — `src/modelo.rs`
- Nuevo campo en `Sala`, después de `refl_techo`:
  ```rust
  /// Color sRGB de las paredes (tinte; el brillo lo da refl_paredes).
  #[serde(default = "color_paredes_defecto")]
  pub color_paredes: [u8; 3],
  ```
  `fn color_paredes_defecto() -> [u8; 3] { [188, 188, 188] }`.
- Rellenar el campo en `Montaje::default()` y en el literal `Sala { … }` de las pruebas (~línea 1374).
- Constante pública:
  ```rust
  pub const COLORES_PARED: [(&str, [u8; 3]); 7] = [
      ("Gris", [188, 188, 188]),
      ("Blanco", [242, 242, 238]),
      ("Beige", [214, 196, 160]),
      ("Gris oscuro", [90, 90, 92]),
      ("Negro", [28, 28, 28]),
      ("Verde croma", [0, 177, 64]),
      ("Azul croma", [0, 71, 187]),
  ];
  ```
- Prueba: el JSON viejo de la prueba existente (~línea 2100) carga con `color_paredes == [188,188,188]`.

## Task B — `src/escena.rs`
- Función pública `pub fn albedo_pared(s: &Sala) -> [f32; 3]` con la fórmula de arriba (usa `lineal(egui::Color32::from_rgb(..))`).
- Función pública `pub fn refl_de_color(c: [u8; 3]) -> f32`: luminancia lineal, redondeada a 2 decimales, `min(0.95)`.
- `malla_gpu`: las 4 paredes usan `albedo_pared(s)`; suelo y techo siguen en `[r; 3]`. (Cambiar el array `refl` por un array de albedos `[[f32;3]; 6]`.)
- `construir` (vista Esquema): las paredes usan `Color32::from_rgba_unmultiplied(r, g, b, 30)` con `s.color_paredes` en vez de la constante `PARED`; borrar `PARED` si queda sin uso.
- Pruebas:
  - `albedo_pared_gris_es_refl`: Sala por defecto → albedo ≈ `[refl_paredes; 3]` (±0.01).
  - `albedo_pared_verde_tiñe`: color `[0,177,64]` → `g > r` y `g > b`, y luminancia del albedo ≈ refl_paredes (±0.01).
  - `refl_de_color_negro_y_blanco`: negro `[28,28,28]` < 0.05; blanco `[242,242,238]` > 0.8 y ≤ 0.95.

## Task C — `src/app.rs`, en `barra` (línea ~725)
- Después del botón "Configuración" (antes del bloque `if self.render.is_some()`), añadir `ui.separator()`, `ui.label("Paredes")` y un `egui::ComboBox::from_id_salt("color_paredes")`:
  - Texto seleccionado: el nombre del preset cuyo color == `self.montaje.sala.color_paredes`, o `"Personalizado"`.
  - Cada opción: un cuadrito de color (pequeño `rect_filled` de ~12 px, o `egui::widgets::color_picker::show_color` si existe en 0.37) + el nombre, como `selectable_label`.
  - Al elegir: `sala.color_paredes = c; sala.refl_paredes = refl_de_color(c); self.marcar_cambio();`.
- `marcar_cambio` sube `version`, lo que ya rehace la malla GPU (línea ~821); confirmar.
