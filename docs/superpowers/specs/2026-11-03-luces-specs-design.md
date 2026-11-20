# Simulación real de luces de estudio — diseño

Fecha: 2026-11-03

## Para qué

Planificar la iluminación de un plató viendo el resultado como lo vería la cámara: cada luz con sus especificaciones de fabricante y sus ajustes (dimmer, apertura, temperatura, orientación), iluminando de verdad suelo, paredes, objetos y modelos importados, con sombras, caída con la distancia, color según temperatura y exposición de cámara. Además, un modo de **mapa de lux** para medir.

## Decidido por el usuario

- Ajustables por luz: intensidad, ángulo de apertura, color (temperatura) y potencia.
- Categorías **Luces techo** y **Luces piso**.
- Las luces se orientan con **giro e inclinación**.
- Luces de piso conectadas a **tomas de suelo aparte**.
- **Simulación real** (no solo conos).

## Cómo se consigue una simulación real (investigación)

Lo que hacen los programas de iluminación y los motores con luces físicas:

1. **Unidades fotométricas.** La ficha da lux a 1 m en el eje → eso es directamente la **intensidad en candelas** (E = I / d², con d = 1 m → I = lux@1m). Al abrir el haz, la misma luz se reparte: I = lux@1m × (tan(apertura_mín/2) / tan(apertura/2))². Dimmer multiplica I.
2. **Ley del inverso del cuadrado y del coseno.** Iluminancia en un punto: E = I(θ) · cos(α) / d², con α el ángulo entre la normal de la superficie y la dirección a la luz.
3. **Perfil del haz.** El "ángulo de apertura" de las fichas es el de 50 % (half-peak). Perfil suave: I(θ) = I₀ · exp(−ln2 · (θ / (apertura/2))²), recortado al "campo" (≈ 1.6 × apertura/2), así a la mitad del ángulo da exactamente 50 %.
4. **Sombras.** Mapa de sombras por luz (render de profundidad desde cada luz con su cono).
5. **Rebote de la sala (luz indirecta).** Fórmula clásica de cavidad: E_indirecta = Φ_total · ρ_media / (A_sala · (1 − ρ_media)), con Φ = flujo total de las luces y ρ la reflectancia media (suelo, paredes, techo configurables). Da la luz de relleno real que no es de los focos.
6. **Color.** Temperatura de color (K) → color RGB lineal del cuerpo negro (aproximación estándar normalizada a luminancia 1), como el nodo Blackbody de Blender/Unity.
7. **Superficies.** Difusas (Lambert): radiancia = albedo × E / π.
8. **Cámara.** Exposición física: EV100 = log2(N² / t · 100 / ISO) con ISO, diafragma f/N y obturación t ajustables, o **automática**; después mapeo de tonos filmic (ACES) y sRGB. Así una escena con 200 lux se ve oscura con f/8 y bien con f/2.8, como en la realidad.
9. **Haz visible (opcional).** Conos con "humo" (haze) aditivo para ver el haz en el aire.

Todo esto se hace en la **GPU con shaders** (OpenGL, el mismo que ya usa la app), dentro de la vista 3D de egui mediante un *paint callback* (patrón oficial de egui `custom3d_glow`). Sin dependencias nuevas. Las etiquetas, cotas y resaltados siguen dibujándose encima como ahora.

## Modos de vista

- **Realista**: lo de arriba (lo que vería la cámara).
- **Mapa de lux**: superficies coloreadas por lux recibidos (escala de color con leyenda: 0–50–200–500–1000–2000+ lux), como el "false color" de los fotómetros y programas de iluminación.
- **Esquema**: la vista actual (rápida, sin luz), por si la GPU falla o para editar.
- En los tres: **lectura de lux bajo el cursor** (calculada en CPU con la misma fórmula, incluidas sombras por rayo).

## Especificaciones (inventario, por equipo de luz)

Tipo de luminaria, potencia (W), lux a 1 m (en la apertura mínima), apertura mín–máx (°), temperatura mín–máx (K), CRI. Tipos predefinidos con valores típicos (editables):

| Categoría | Tipo | W | lux@1m | Apertura | CCT | CRI |
|---|---|---|---|---|---|---|
| Luces techo | Fresnel LED | 150 | 20 000 | 15–50° | 3200–5600 | 96 |
| Luces techo | Panel LED | 100 | 3 300 | 65° | 2700–6500 | 96 |
| Luces techo | Elipsoidal | 200 | 26 000 | 19–36° | 3200 | 95 |
| Luces techo | PAR LED | 200 | 12 000 | 25° | 2700–6500 | 95 |
| Luces techo | Space light | 400 | 2 000 | 120° | 3200–5600 | 95 |
| Luces piso | Monoluz COB + softbox | 300 | 4 000 | 90° | 5600 | 96 |
| Luces piso | Panel LED en pie | 60 | 2 000 | 65° | 2700–6500 | 96 |
| Luces piso | Fresnel en pie | 300 | 30 000 | 15–45° | 3200–5600 | 96 |
| Luces piso | Tubo LED | 30 | 600 | 120° | 2700–10000 | 95 |
| Luces piso | Luz de ciclorama | 200 | 3 000 | 100° | 2700–6500 | 95 |

## Ajustes de cada luz colocada

Dimmer 0–100 %, apertura (en su rango), temperatura (en su rango), giro 0–360°, inclinación −90…+90° (techo empieza en −90°, piso en 0°), encendida/apagada. Se muestra la potencia efectiva (W × dimmer) e intensidad (cd). Luces de piso: X, Z y altura del foco (0.5–3 m, por defecto 1.8 m), dibujadas como pie con foco.

## Sala y materiales

Reflectancia (0–1) de suelo, paredes y techo en el panel Sala (por defecto suelo 0.2, paredes 0.5, techo 0.7 — plató típico; se pueden poner paredes negras 0.05). Cada objeto usa su color como albedo.

## Tomas de suelo

Tomas con nivel *carriles* (como ahora) o *suelo* (0.3 m). Techo ↔ tomas de carriles; piso ↔ tomas de suelo. Una luz por toma.

## Inventario

"Luces" pasa a **"Luces techo"** (los equipos existentes se quedan ahí) y se añade **"Luces piso"** (ambas base). Sus pestañas muestran las columnas de especificaciones y un desplegable de tipo que rellena valores típicos.

## Ejemplos por defecto

La sala nueva trae una luz de techo (Fresnel LED en C2 apuntando a la mesa) y una de piso (Monoluz COB + softbox junto a la mesa), encendidas.

## Rendimiento

Hasta 16 luces con sombra (más luces: sin sombra, con aviso). Mapas de sombra 1024². Se redibuja solo al cambiar algo (en reposo, CPU ~0). Si OpenGL no permite los shaders, la app pasa a modo Esquema con un aviso.

## Compatibilidad

Montajes e inventarios antiguos abren: luces sin especificaciones → valores de Panel LED con aviso; tomas sin nivel → carriles; salas sin reflectancias → valores por defecto.

## No entra

Trazado de rayos con varios rebotes, archivos IES, texturas, DMX, suma de carga por circuito.

## Decisiones de Claude (a revisar)

- Valores típicos de la tabla; perfil gaussiano de 50 % en el half-peak; rebote por fórmula de cavidad (una estimación estándar, no radiosidad completa).
- Renderizador propio en OpenGL dentro de egui (sin motor 3D externo) para no cambiar de base ni añadir dependencias.
- Renombrar "Luces" a "Luces techo"; luces de ejemplo en la sala nueva.
