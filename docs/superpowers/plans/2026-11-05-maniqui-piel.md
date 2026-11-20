# Maniquí con reflectancia de piel humana — plan

**Goal:** el maniquí refleja y absorbe la luz como una persona (piel clara-media), no como un plástico gris claro.

## Investigación
- El albedo GPU de los objetos es `lineal(color_objeto(tipo))` (escena.rs `malla_gpu`), así que la reflectancia es la luminancia lineal del color.
- Hoy `MANIQUI_COLOR = (200,190,180)` → luminancia lineal ≈ 0.52 y casi sin tinte: demasiado claro.
- Referencia: la carta X-Rite ColorChecker, cuyos parches de piel están hechos para igualar piel humana medida:
  - "light skin": sRGB (194,150,130), Y ≈ 0.35 → reflexión 35 %, absorción 65 %.
  - "dark skin": sRGB (115,82,68), Y ≈ 0.10.
- Regla de fotografía: la piel clara-media queda ~1 paso sobre el gris 18 % (~36 %), coherente con la carta.

## Cambio (src/escena.rs)
- `MANIQUI_COLOR` → `egui::Color32::from_rgb(194, 150, 130)` con un comentario de una línea: parche "light skin" de ColorChecker, reflectancia ≈ 0.35.

## Prueba (escena.rs, módulo de tests)
- `maniqui_reflexion_piel`: `luminancia(lineal(MANIQUI_COLOR))` está en 0.30..0.40 y el rojo lineal es mayor que el azul (tinte cálido de la piel).
