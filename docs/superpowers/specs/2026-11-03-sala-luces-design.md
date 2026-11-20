# Sala de luces 3D — diseño

Fecha: 2026-11-03

## Para qué

Una app nativa de Windows (un `.exe`, Rust) que muestra en 3D una sala de grabación cuadrada con
carriles de luces en el techo. Se usa para **planificar** un montaje (dónde va cada luz y a qué toma
se conecta) y para **registrar** cómo quedó montada la sala. Todo lo que hay en la sala se puede
mover.

Lo más importante: los carriles del techo, las luces colgadas de ellos con su código de equipo,
marca y modelo, y a qué toma va cada luz.

## Decidido por el usuario

- Uso: planificar y registrar.
- Carriles: filas **paralelas en una sola dirección**, de pared a pared. Se pueden añadir, quitar y
  mover.
- Tomas: en las **paredes**, a la altura de los carriles, identificadas **solo por un código**
  (ej. `T-01`).
- Mover: **las dos formas**, arrastrando con el ratón en la vista 3D y escribiendo números en un
  panel.
- Guardar: **un archivo por montaje** (Nuevo / Abrir / Guardar / Guardar como).
- Luces: salen de un **catálogo de equipos** (código, marca, modelo). Primero se da de alta el
  equipo y luego se coloca. Un mismo código no puede estar colocado dos veces en un montaje.
- En 3D se dibuja una **línea de cada luz a su toma**.
- Medidas de la sala **editables en la app**.
- Para empezar, sala cuadrada (rectangular también vale, ver modelo).
- Rust y nativo de Windows.

## No entra (non-goals)

Potencia o carga de las tomas, varias zonas o salas, modelos 3D reales por marca, exportar a
PDF o imagen, deshacer/rehacer, multiusuario.

## Fracaso aunque funcione (anti-goals)

- Perder trabajo: cerrar, abrir o crear un montaje con cambios sin guardar sin preguntar antes.
- Que no quede claro qué luz va a qué toma.
- Que haga falta instalar algo aparte para ejecutar el `.exe`.

## Tecnología (decisión de Claude)

- Rust estable, toolchain `stable-x86_64-pc-windows-gnu` (evita instalar Visual Studio Build
  Tools). Si falla al compilar, la alternativa es MSVC.
- `eframe` / `egui` **0.37** (versión comprobada en crates.io el 2026-11-03) para la ventana y
  los paneles, con el renderizador **glow** (OpenGL). Se probó wgpu, pero en este equipo
  consumía un núcleo de CPU entero con la ventana en reposo (con DX12 y con Vulkan); glow
  consume 0.
- Vista 3D **dibujada a mano** con `egui::Painter`: cámara en perspectiva que orbita, polígonos
  con sombreado plano ordenados por profundidad (algoritmo del pintor) y líneas. No se usa Bevy
  (su API cambia en cada versión y compila muy lento; para cajas y líneas no hace falta).
- `serde` + `serde_json` para los archivos, `rfd` 0.17 para los diálogos de Abrir/Guardar.
- La interfaz en español.

## Modelo de datos

Unidades: metros. Origen en una esquina del suelo. X = ancho, Z = largo, Y = altura.

```
Montaje (archivo .sala.json)
  sala: { ancho: f32 (X), largo: f32 (Z), alto_techo: f32, altura_carriles: f32 }
  carriles: [ { id: String, x: f32 } ]        // cada carril va a lo largo de Z, de z=0 a z=largo
  tomas:    [ { codigo: String, pared: Norte|Sur|Este|Oeste, pos: f32 } ]
  luces:    [ { codigo: String, marca: String, modelo: String,
                carril: String, z: f32, toma: Option<String> } ]

Inventario (archivo único, %APPDATA%\SalaLuces\inventario.json)
  equipos: [ { codigo: String, marca: String, modelo: String } ]
```

- Valores por defecto de un montaje nuevo: sala 6 × 6 m, techo 4 m, carriles a 3,8 m, tres
  carriles (`C1`, `C2`, `C3`) repartidos por igual, ninguna toma, ninguna luz.
- Paredes: Sur z=0, Norte z=largo, Oeste x=0, Este x=ancho. `pos` es la distancia a lo largo de
  la pared desde su esquina de menor coordenada. Todas las tomas están a `altura_carriles`.
- La luz guarda una **copia** de marca y modelo para que el montaje se entienda por sí solo como
  registro aunque el equipo cambie o desaparezca del inventario.
- Los ids de carril y los códigos de toma son únicos en un montaje. Los códigos de luz también.

## Reglas

- Las posiciones siempre se limitan a la sala: carril `x` en [0, ancho]; luz `z` en [0, largo];
  toma `pos` en [0, largo de su pared].
- Al encoger la sala, todo lo que quede fuera se ajusta al borde.
- Mover un carril arrastra sus luces (su `z` no cambia).
- Una luz se cambia de carril desde el panel (desplegable). Arrastrando se mueve solo a lo largo
  de su carril.
- No se puede borrar un carril que tenga luces: se avisa y hay que mover o quitar las luces antes.
- **Una toma alimenta a una sola luz** (decidido por el usuario). El desplegable de toma de una
  luz solo ofrece tomas libres (más la que ya tiene). Si un archivo cargado trae dos luces en la
  misma toma, la primera la conserva, a las demás se les quita y se muestra un aviso.
- Borrar una toma deja sin toma a la luz conectada a ella.
- Renombrar el id de un carril o el código de una toma actualiza las luces que lo usan.
- El código de una luz colocada no se edita; para cambiar de equipo se quita y se coloca otro.
- Para colocar una luz se elige un equipo del inventario que todavía no esté en el montaje.
- Borrar un equipo del inventario no toca los montajes.
- Hay que confirmar (Guardar / Descartar / Cancelar) antes de Nuevo, Abrir o cerrar la ventana si
  hay cambios sin guardar. Un archivo que no se puede leer muestra un error y no cambia el
  montaje abierto.

## Interfaz

- **Barra superior:** Nuevo, Abrir, Guardar, Guardar como, Inventario. El título muestra el
  nombre del archivo con `*` si hay cambios sin guardar.
- **Panel izquierdo:** medidas de la sala, y listas de Carriles, Tomas y Luces con botones de
  Añadir y Borrar. Al hacer clic en un elemento de la lista se selecciona (también en 3D).
- **Panel derecho:** el elemento seleccionado con sus campos editables (posición, carril, toma
  desde un desplegable, códigos).
- **Ventana Inventario:** tabla de equipos con alta, edición y borrado. Los códigos son únicos.
- **Vista 3D (centro):**
  - Suelo con una cuadrícula de 1 m, paredes semitransparentes (no tapan lo que hay dentro) y el
    contorno del techo.
  - Carriles: barras finas a `altura_carriles`, con su id.
  - Luces: una forma genérica (caja como cuerpo y un disco o caja más pequeña como foco) colgada
    bajo su carril, con su código como etiqueta.
  - Tomas: cajitas en la pared, con su código.
  - Una línea de cada luz a su toma, del mismo color para todo el par (luz + línea + toma). Cada
    par tiene un color distinto; una luz sin toma va en gris.
  - Lo seleccionado se resalta.
  - Ratón: clic izquierdo para seleccionar; arrastrar con el izquierdo para mover lo seleccionado
    (luz a lo largo de su carril, carril a lo largo de X, toma a lo largo de su pared); arrastrar
    con el derecho para orbitar; rueda para el zoom. Para arrastrar se proyecta el rayo del ratón
    sobre la línea en la que se mueve el objeto.

## Calidad

- Pruebas unitarias de lo que puede fallar en silencio: proyección y rayo del ratón, ajuste de
  posiciones a la sala, unicidad de códigos, reglas de borrado y que el montaje se guarde y se
  vuelva a leer igual (ida y vuelta por JSON).
- `cargo build --release` genera un único `.exe` que se ejecuta sin ventana de consola.

## Decisiones de Claude (a revisar)

- eframe con una 3D dibujada a mano en vez de Bevy; toolchain GNU; renderizador glow.
- El inventario va en un archivo propio y se guarda en cuanto se cambia.
- La luz guarda una copia de marca y modelo.
- No se borra un carril que tenga luces.
- Lo nuevo aparece en el centro: carril en x = ancho/2, luz a mitad de su carril, toma a mitad de
  la pared Sur. Códigos automáticos (`C4`, `T-01`…) que se pueden editar.
- Al escribir números se aceptan coma y punto como separador decimal.

## Queda a criterio de quien lo construya

Colores, tamaños exactos de las formas, la organización interna de los módulos, el sensor de
arrastre y la sensibilidad de la cámara.
