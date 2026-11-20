# Física más realista — plan maestro

Revisión del 2026-11-05 contra DIALux (radiosidad, validado con CIE 171), Unreal (unidades físicas, IES, luces rectangulares) y set.a.light 3D. Hay cuatro mejoras, que se hacen en este orden:

1. **Ronda A — rebotes sin cortar** (trazado). 
2. **Ronda A — emisor rectangular exacto** (fórmula de Lambert del polígono).
3. **Ronda B — perfiles IES reales** (archivo `.ies` del fabricante por equipo).
4. **Ronda C — luz rebotada por zonas en el modo Realista** (sondas de irradiancia).

---

## Ronda A.1 — Rebotes del trazado (src/trazado.rs, src/config.rs)
**Problema confirmado:** `camino()` (trazado.rs ~488) corta en `b == uRebotes`, y `Configuracion::limitar` deja `rebotes` entre 1 y 4, con 2 por defecto. La serie de rebotes es ρ + ρ² + …; con 2 rebotes solo se capta `1 − ρ²` de la indirecta. Para paredes de 0.85 eso es el 28 %, y para una sala media de 0.6, el 64 %.

**Arreglo:**
- La ruleta rusa (ya existe desde `b >= 2`) es lo que termina el camino, sin sesgo. `MAX_REBOTES` (GLSL) pasa a 32 y `uRebotes` es solo un tope de seguridad.
- `Configuracion.rebotes`: `limitar` lo deja entre 1 y 32. El valor por defecto pasa a 16 en todos los sitios donde hoy es 2 (busca `rebotes: 2` en config.rs). Actualiza las pruebas de config que esperan el tope 4 (p. ej. 9 → 9; 99 → 32).
- Si el desplegable o campo de rebotes de la ventana de Configuración (app.rs) tiene un rango 1..=4, pásalo a 1..=32.
- Sin cambios en el estimador.

## Ronda A.2 — Emisor rectangular exacto (src/fotometria.rs, FS_ESCENA en render_gl.rs, FS_TRAZA en trazado.rs)
**Problema:** el método del "punto del emisor más cercano" (ronda 2 de forma-haz) se pasa cerca de un panel grande. A 0.3 m de un Celeb da unos 98 000 lux, cuando lo real ronda los 30 000 (π·L, con L = I/A).

**Modelo:** factor de campo cercano exacto para un emisor lambertiano rectangular:
- La luz se sigue calculando como ahora en campo lejano, **desde el centro**: `E_lejos = cd · perfil_fuente(f,p) · cosθr / d²`, con d y θr medidos al centro (sin `punto_emisor`).
- Se multiplica por `k = E_lambert_exacta / E_lambert_puntual`, donde las dos se calculan para un emisor lambertiano de radiancia 1:
  - `E_lambert_puntual = A · cosθe · cosθr / d²` (desde el centro; θe = ángulo con `dir`). Si `cosθe ≤ 0` o `cosθr ≤ 0`, k = 1, y el perfil ya da 0 detrás.
  - `E_lambert_exacta = ½ Σ_i β_i · (n · γ_i)`: fórmula de Lambert para un polígono, con los vértices del rectángulo **relativos a p**. `β_i` = ángulo entre `v_i` y `v_{i+1}`; `γ_i = normalize(v_i × v_{i+1})`; n = normal del receptor. El orden de los vértices debe dar un resultado positivo; comprueba el signo con la prueba analítica y, si sale negativo, usa el valor absoluto del total.
  - **Recorte:** antes de la fórmula, recorta el polígono contra el semiespacio `n·(x − p) > 1e-5` (Sutherland–Hodgman, como mucho 5 vértices). Si queda vacío, E = 0.
- Emisor: el rectángulo `pos ± u·hu ± v·hv` para los rectangulares. Para los de disco (R > 0.05) se usa el cuadrado de la misma área (lado `R·√π`) en la misma base. Si R ≤ 0.05, k = 1.
- Límite de k: `min(k, 50)`, contra valores absurdos pegado al panel.
- Así `E → π·L·F`, finito, cerca del panel, y `k → 1` lejos (se conserva todo lo validado).
- Reemplaza el uso de `punto_emisor` en `iluminancia` y en los shaders (bucle de luces de FS_ESCENA; `perfil_luz` y el `ndl` de `directa` en FS_TRAZA). `punto_emisor` se queda solo si algo más lo usa; si no, bórralo con sus pruebas, y ajusta `kinoflo_meseta_bajo_panel` a lo que dé la fórmula exacta (debe seguir habiendo meseta: a 0.5 m en u ≥ 0.9 × centro).
- `pub fn campo_cercano(f: &FuenteLuz, p, n) -> f32` en CPU, y la misma función en GLSL en los dos shaders. GLSL 3.30: arrays de tamaño fijo `vec3 poly[8]`, sin recursión.
- **Pruebas:**
  - `lambert_cuadrado_analitico`: emisor 1 × 1 m (medias 0.5, 0.5), receptor paralelo en el eje a 1 m → `E_lambert_exacta` = 0.7522 (±0.5 %). Valor de la fórmula del factor de forma de un rectángulo paralelo, sumando 4 esquinas.
  - `campo_cercano_lejos_uno`: Celeb a 10 m en el eje → k en 1 ± 1 %.
  - `celeb_pegado_finito`: Celeb, receptor a 0.01 m bajo el centro → `iluminancia` ≤ 1.05 × π·cd/A, con A = 4·hu·hv.
  - `recorte_pared_perpendicular`: un receptor en un plano perpendicular al panel, que toca su borde → E finito y ≥ 0, sin NaN.

Cuando termines: `cargo test` (todo pasa, 0 avisos) y `cargo build --release`.

---

## Ronda B — Perfiles IES reales (LM-63)
Hoy el haz de cada equipo es una campana gaussiana ajustada a su apertura. DIALux, Unreal y AGi32 usan la tabla de candelas medida por el fabricante (archivo `.ies`, norma IES LM-63). Con un `.ies`, la intensidad sale de esa tabla.

### B.1 — Datos y física en CPU (src/ies.rs nuevo, src/modelo.rs, src/fotometria.rs, src/main.rs `mod ies;`)
- `src/ies.rs`:
  ```rust
  pub struct Ies { pub vert: Vec<f32>, pub horiz: Vec<f32>, pub cd: Vec<Vec<f32>> /* [h][v], ya × multiplicador */, pub max_cd: f32 }
  pub fn leer(texto: &str) -> Result<Ies, String>
  impl Ies { pub fn intensidad(&self, theta_deg: f32, phi_deg: f32) -> f32 }  // bilineal; 0 fuera de [vert.first, vert.last]
  ```
  - Formato: líneas de cabecera hasta `TILT=`. Solo se admite `TILT=NONE`; otro valor da `Err("TILT no soportado")`. Después vienen números separados por espacios, comas o saltos de línea: `n_lamparas lm_por_lampara multiplicador n_vert n_horiz tipo_fotometrico unidades ancho largo alto`, luego `factor_balasto factor_futuro vatios`, luego `n_vert` ángulos verticales, `n_horiz` horizontales y `n_horiz × n_vert` candelas (para cada ángulo horizontal, todos los verticales).
  - Solo tipo C (`tipo_fotometrico == 1`); otro tipo da `Err`. Las candelas se multiplican por `multiplicador`.
  - Simetría por el último ángulo horizontal: 0 → la misma para todo φ; 90 → por cuadrantes (φ se refleja a 0..90); 180 → bilateral (φ > 180 → 360 − φ); 360 → completa. Si los verticales empiezan en 90 (luminarias que solo emiten hacia arriba) se admite igual. Vertical 0 = nadir, que aquí es **el eje de la luz** (`dir`).
  - Valida: cuentas correctas, ángulos crecientes, sin NaN, `max_cd > 0`. Si no, `Err` con un mensaje en español.
- `SpecsLuz` gana `#[serde(default)] pub ies: Option<String>` (nombre del perfil). `Montaje` gana `#[serde(default)] pub fotometrias: BTreeMap<String, String>` (nombre → texto del `.ies`). `Montaje::guardar` solo conserva los que usa alguna luz, igual que `modelos`.
- `FuenteLuz` gana `pub ies: Option<std::sync::Arc<Ies>>`. `fuentes(m)` lo rellena leyendo `m.fotometrias` (con caché por nombre en la misma llamada). Un error de lectura deja `None` (se usa la campana).
- Física con IES: `cd_dir(f, p)` = `ies.intensidad(θ, φ) · dimmer` (si está apagada, 0). θ es el ángulo entre `dir` y (p − pos); φ es el ángulo de (p − pos) proyectado en el plano (u, v), medido desde u hacia v, en 0..360. Sustituye a `cd · perfil_fuente` en `iluminancia`. El factor `campo_cercano` (A.2), `luminancia(color)`, el coseno y d² se aplican igual. `intensidad_cd` devuelve `max_cd · dimmer` cuando hay IES; es la cifra que muestra el panel. `flujo_lm` integra la tabla: Σ cd · sinθ · Δθ · Δφ.
- Pruebas (ies.rs y fotometria.rs), con un `.ies` de ejemplo **escrito en la prueba**: tipo C, simetría 0, verticales 0, 30, 60, 90 y candelas 1000, 800, 300, 0.
  - `leer_ejemplo`: max_cd 1000; `intensidad(30, 123)` = 800; `intensidad(45, 0)` = 550 (interpolada).
  - `leer_simetria_180`: un archivo con horizontales 0, 90, 180 → `intensidad(v, 270)` = `intensidad(v, 90)`.
  - `leer_errores`: TILT=INCLUDE, tipo 2, faltan candelas → Err.
  - `iluminancia_con_ies`: una luz con ese IES, apuntando abajo, a 2 m en el eje → 1000 / 4 · dimmer lux (± 1 %; el campo cercano es ≈ 1 a 2 m para un emisor pequeño).
  - `guardar_solo_ies_usados`.
  - `montaje_viejo_sin_ies`.

### B.2 — GPU e interfaz (src/render_gl.rs, src/trazado.rs, src/app.rs, src/config.rs para la ruta)
- Cada IES se remuestrea a una rejilla fija de **64 θ (0..180°) × 32 φ (0..360°)**, normalizada a `max_cd`. Se suben en una textura 2D `R32F` de 64 × (32·MAX_LUCES); la luz i ocupa las filas 32·i … 32·i+31. Uniforms: `uIes[i]` (vec4: x = 1 si tiene IES, y = max_cd · dimmer). Sin IES, el shader sigue con `perfil_haz` y `uPosCd.w`. Con IES, intensidad = `uIes[i].y · texture(...)` con muestreo lineal (θ, φ calculados como en CPU). Esto va en FS_ESCENA y en FS_TRAZA (`perfil_luz` y su peso), y la textura se enlaza en las dos pasadas.
- El frustum de sombra de una luz con IES usa la apertura donde la tabla cae al 10 % del máximo (máx. 170°). Se calcula en CPU y va en `FuenteLuz.apertura`.
- Biblioteca de la app: `%APPDATA%\SalaLuces\ies\*.ies` (`ruta_ies()` en config.rs), que se carga al arrancar; los que no se pueden leer se avisan y se ignoran. Al abrir un montaje, sus `fotometrias` nuevas se añaden a la biblioteca (patrón `incorporar_acabados`).
- `editar_specs` (app.rs) gana una fila "Fotometría (IES)": un desplegable con `(ninguna)` y los de la biblioteca, más el botón `Importar .ies…`. El botón abre un diálogo `rfd` con el filtro `ies`, lee el archivo y lo valida con `ies::leer`. Si es válido, lo copia a la biblioteca con nombre = nombre del archivo sin extensión; si falla, `aviso(e)`. Elegir uno pone `specs.ies = Some(nombre)` y copia el texto a `montaje.fotometrias`. Con IES, Apertura y Lux a 1 m se muestran desactivados con la etiqueta "(según IES)".
- Prueba Rust: el remuestreo de la tabla de ejemplo a 64 × 32 da 1.0 en θ = 0 y 0 en θ ≥ 90.

---

## Ronda C — Luz rebotada por zonas en el modo Realista (sondas de irradiancia)
Hoy el modo Realista suma la misma indirecta (`uIndirecta`, una sola cifra) en todos los puntos. DIALux reparte el rebote por superficies (radiosidad). Aquí se reutiliza el trazador de caminos, que ya es físico, para llenar una rejilla de sondas.

- **Rejilla:** sondas cada 0.5 m dentro de la sala, en el centro de cada celda: `nx = ceil(ancho/0.5)`, y lo mismo para ny y nz. Como máximo 32 por eje; si se pasa, el paso crece. Cada sonda guarda **6 irradiancias RGB**, una por eje (+x, −x, +y, −y, +z, −z), en un "cubo ambiente".
- **Cálculo en GPU** (trazado.rs): nuevo programa `FS_SONDAS`, que reutiliza `recorrer`, `directa` y la parte de rebotes de `camino`. Dibuja un cuadro de `6·nx` × `ny·nz` texeles; cada texel es (sonda, dirección). Por fotograma y texel: una dirección con distribución coseno alrededor del eje, `L = camino(sonda, dir)` (radiancia que llega) y se acumula. Irradiancia = π · media(L). Acumulación ping-pong como el trazado de píxeles, hasta 128 muestras. Se rehace al cambiar la escena (`version`).
- **Uso** (FS_ESCENA): con las sondas listas, se reemplaza `E += uIndirecta` por la interpolación trilineal de las 8 sondas que rodean al punto. Cada sonda se evalúa con su cubo: `E = n.x² · E(±x) + n.y² · E(±y) + n.z² · E(±z)`, eligiendo el signo según n. Mientras no hay sondas (o si falla el trazado) se sigue usando `uIndirecta`.
- La **lectura de lux del cursor** (`lux_en`) suma la misma indirecta por sondas cuando existen. Hay que leerlas a CPU con `read_pixels` una vez que terminan.
- Las sondas incluyen el **color de las paredes**, así que el derrame de una pared verde croma aparece.
- Pruebas Rust: el tamaño de la rejilla (6 × 6 × 4 m → 12 × 8 × 12 sondas, tope 32), la ponderación del cubo ambiente (la suma de los n² es 1) y la interpolación trilineal en CPU (`indirecta_sondas(punto, n)`).

### Pendientes que salieron en B (se hacen en la ronda C)
- `fuentes()` vuelve a leer el texto IES y `apertura_ies` en **cada fotograma**. Hay que guardar el `Arc<Ies>` ya leído en una caché (en App o en un `thread_local`/`OnceLock` con mapa nombre+hash → `Arc<Ies>`), para que solo se lea cuando cambia el texto.
