# Catálogo de producción e importación de modelos 3D — diseño

Fecha: 2026-11-03

## Para qué

Que la sala se pueda amueblar con lo que hay en un plató real sin importar nada, y que además se puedan traer modelos 3D propios (de Blender u otros programas libres) y colocarlos como cualquier objeto.

## Decidido por el usuario

- Catálogo incluido de los cuatro grupos: cámaras y soportes, iluminación de suelo, escenografía, audio y monitores.
- Importar **glTF/GLB y OBJ**.
- Los modelos importados se guardan **en los dos sitios**: en la biblioteca de la app (disponibles para todos los montajes) y dentro del montaje que los usa (se puede pasar a otra persona en un solo archivo).
- Al importar se usan las **medidas reales del archivo** (metros); luego se cambian ancho/fondo/alto como en cualquier objeto.
- Al importar se crea el objeto en la sala.

## Catálogo incluido (formas simples con cajas y prismas)

Además de Silla, Mesa, Maniquí y Caja:

| Grupo | Formas (medidas por defecto aprox. ancho × fondo × alto) |
|---|---|
| Cámaras y soportes | Cámara en trípode (0.7×0.7×1.6), Trípode (0.7×0.7×1.5), Slider (1.2×0.3×0.9), Grúa / jib (2.5×0.8×1.8), Teleprompter (0.6×0.7×1.5) |
| Iluminación de suelo | C-stand (0.9×0.9×2.5), Soporte de luz (0.8×0.8×2.2), Softbox de pie (0.9×0.9×2.0), Paraguas (1.0×1.0×2.0), Reflector / rebotador (1.0×0.6×1.6), Bandera (0.9×0.6×1.8), Panel LED de pie (0.6×0.6×1.9) |
| Escenografía | Fondo con soporte (2.7×0.6×2.7), Ciclorama (4.0×2.0×3.0), Sofá (2.0×0.9×0.85), Escritorio (1.4×0.7×0.75), Taburete alto (0.4×0.4×0.75), Planta (0.5×0.5×1.2), Estantería (1.0×0.35×1.8) |
| Audio y monitores | Micrófono de pie (0.5×0.5×1.6), Boom / caña (0.6×0.6×2.2), Monitor en soporte (0.6×0.5×1.5), TV en soporte (1.2×0.6×1.7), Altavoz (0.3×0.3×1.4) |

Cada forma debe reconocerse de un vistazo (p. ej. el C-stand con su base de tres patas y brazo, la cámara con cuerpo y objetivo apuntando a su frente +Z, el ciclorama con la curva suelo-pared aproximada en tramos).

## Forma y categoría

- El objeto tiene **Categoría** (organizativa, la del inventario) y **Forma** (cómo se dibuja). En el panel derecho hay un desplegable **Forma** con todas las formas del catálogo, agrupadas, y los modelos importados.
- Cambiar la categoría sigue eligiendo la forma por defecto de esa categoría (como ahora); después se puede cambiar la forma a mano.
- El inventario gana cuatro categorías base más: **Cámaras y soportes, Iluminación de suelo, Escenografía, Audio y monitores** (no borrables, como las otras base).

## Importar

- Botón **"Importar modelo…"** en la sección Objetos → diálogo de archivo (`.glb`, `.gltf`, `.obj`).
- Se lee la geometría (triángulos) y el color base de cada material; **las texturas no se usan** (se dibuja con el color base). Ejes: Y arriba, metros (lo que exportan Blender y la mayoría).
- Se recoloca para que el origen quede en el centro de la base (apoyado en el suelo).
- Límite de **20 000 triángulos**: si tiene más, se simplifica automáticamente (agrupando vértices cercanos) y se avisa.
- Se guarda en la biblioteca (`%APPDATA%\SalaLuces\modelos\<nombre>.json`) con su nombre (el del archivo, editable si ya existe uno igual) y queda como forma disponible.
- Se crea un objeto con esa forma en el centro de la sala, seleccionado, con las medidas reales del modelo.
- Errores (archivo roto, formato no soportado, sin geometría) → mensaje claro; nada se guarda.

## Guardar y abrir

- El montaje guarda dentro una copia de cada modelo importado que usa (`modelos` en el archivo). Al abrir un montaje con un modelo que no está en la biblioteca, se añade a la biblioteca.
- Si un montaje usa un modelo que no está ni dentro ni en la biblioteca (archivo antiguo/manipulado), el objeto se dibuja como caja y se avisa.
- Montajes y bibliotecas antiguas siguen abriéndose.

## Rendimiento

- La vista sigue dibujada a mano. Los modelos importados se dibujan triángulo a triángulo con sombreado plano y orden por profundidad. Con el límite de 20 000 triángulos por modelo la vista debe seguir fluida con varios modelos; en reposo la CPU sigue a 0.

## Dependencias nuevas (decisión de Claude)

- `gltf` 1.4 (lector oficial de glTF en Rust) y `tobj` 4 (lector de OBJ). Son las estándar; escribir lectores propios sería mucho más código y menos fiable.

## No entra

Texturas, animaciones, exportar modelos, FBX/3DS/STL, editar la malla dentro de la app.

## Decisiones de Claude (a revisar)

- Texturas ignoradas, solo color base.
- Límite de 20 000 triángulos con simplificación automática.
- Las cuatro categorías nuevas del inventario y la separación Categoría / Forma.
- La biblioteca guarda la malla ya procesada (no el archivo original).
