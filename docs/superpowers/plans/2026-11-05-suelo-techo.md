# Suelo y techo con acabado propio — plan

**Goal:** El suelo y el techo funcionan igual que cada pared: tienen color, reflexión y absorción, se editan por separado y usan los mismos presets (los de fábrica `ACABADOS` y los del usuario). Hoy solo tienen una reflectancia gris (`refl_suelo`, `refl_techo`) en el panel de la sala.

## Modelo (src/modelo.rs)
- `pub enum Superficie { Pared(Pared), Suelo, Techo }` (Clone, Copy, PartialEq, Eq, Debug) con `pub const TODAS: [Superficie; 6]` (Norte, Sur, Este, Oeste, Suelo, Techo) y `nombre()` ("Norte"…, "Suelo", "Techo").
- `Sala` gana `#[serde(default)] pub suelo: Option<Acabado>` y `#[serde(default)] pub techo: Option<Acabado>`. `refl_suelo` y `refl_techo` pasan a ser **solo de migración**, igual que `refl_paredes`: `#[serde(default = ..., skip_serializing)]`, con un doc "solo migración".
- `completar_paredes` (o una función hermana a la que llame `ajustar_a_sala`): si `suelo` es None, `Some(Acabado { color: [188,188,188], reflexion: refl_suelo })`; lo mismo para el techo. Con color gris el albedo es exactamente `[refl; 3]`, así que el aspecto por defecto no cambia. Después limita cada reflexión a 0..0.95.
- `Sala::acabado_de(s: Superficie) -> Acabado` (Pared → `acabado(p)`; Suelo/Techo → el campo o, si es None, el de migración) y `Sala::acabado_de_mut(s) -> &mut Acabado` (completa antes).
- `Montaje::guardar`: los presets "usados" son los que coinciden con **cualquiera de las 6 superficies**.
- `Montaje::default()` y el literal de pruebas: `suelo: Some(gris 0.2)`, `techo: Some(gris 0.7)`.

## Uso
- escena.rs `malla_gpu`: el suelo y el techo usan `albedo(acabado_de(Suelo/Techo))`. En `construir` (Esquema), el polígono del suelo usa el color del acabado (opaco, como hoy `SUELO`) si es fácil; si no, déjalo.
- fotometria.rs `indirecta_lux`: `refl_suelo` / `refl_techo` → `acabado_de(...).reflexion`. El helper de pruebas `sala(refl)` pone las 6 superficies.
- app.rs: quitar las filas "Reflectancia suelo" y "Reflectancia techo" del panel de la sala (~1093-1096).

## Ventana (app.rs)
- El botón de la barra y el título de la ventana pasan de "Paredes" a **"Superficies"**.
- `pared_editada: Pared` → `superficie_editada: Superficie` (por defecto `Pared(Sur)`).
- Plano: sigue igual (4 paredes clicables). Además, un **clic dentro del rectángulo** selecciona `Suelo` (relleno con el color del suelo a 40 % de opacidad; contorno blanco si está seleccionado). Debajo del plano, dos `selectable_label`: "Suelo" y "Techo".
- El encabezado y el Grid (Material, Color, Reflexión, Absorción, "Guardar como preset") funcionan igual para cualquier superficie, a través de `acabado_de_mut(superficie_editada)`. Los presets de fábrica y del usuario son los mismos para las 6.

## Pruebas
- `json_viejo_migra_suelo_techo`: un JSON con `refl_suelo: 0.3` y `refl_techo: 0.6`, sin `suelo` ni `techo` → acabados grises con 0.3 y 0.6.
- `suelo_techo_ida_y_vuelta`: suelo en verde croma; guardar y cargar → se conserva, y el JSON no contiene `"refl_suelo"`.
- `malla_suelo_coloreado`: suelo verde croma → los vértices de la carcasa con normal (0,1,0) tienen g > r.
- `guardar_preset_usado_en_suelo`: un preset del usuario que solo usa el suelo se conserva al guardar.
