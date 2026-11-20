# Cómo seguir desarrollando Sala de luces

Guía para quien retome el proyecto, sea una persona o un agente de IA. Explica cómo compilar, cómo comprobar los cambios y qué falta por hacer.

## Preparar el entorno

- Windows 10 u 11 con una GPU con OpenGL 3.3.
- [Rust](https://rustup.rs/) estable, **1.85 o superior** (edición 2024). El proyecto usa el toolchain GNU, que no necesita Visual Studio:

  ```powershell
  rustup toolchain install stable-x86_64-pc-windows-gnu
  rustup default stable-x86_64-pc-windows-gnu
  ```

  `build.rs` pasa `exports.def` al enlazador GNU para exportar los símbolos que eligen la GPU dedicada en portátiles híbridos. Con MSVC habría que revisar ese paso.

```powershell
cargo test              # pruebas (deben pasar todas)
cargo build             # sin avisos: la CI compila con -D warnings
cargo run --release     # abrir la app
```

## Antes de dar un cambio por bueno

**Las pruebas no ejecutan los shaders.** El GLSL (`FS_ESCENA` en `render_gl.rs`, `FS_TRAZA` y `FS_SONDAS` en `trazado.rs`) solo se compila al abrir la app. Si un shader falla, Fotorrealista cae a Alta sin avisar. Después de tocar luz o render:

1. Abre la app y mira el resultado en los tres modos de calidad.
2. Prueba **Fotorrealista** siempre: sus shaders pueden compilar y aun así dar una imagen negra.

Variables de entorno útiles para probar sin tocar la configuración guardada:

| Variable | Efecto |
|---|---|
| `SALA_PRESET=rapida\|alta\|fotorrealista` | Fuerza el preset de calidad solo en esa sesión. |
| `SALA_EV=<número>` | Fija la exposición EV100, por ejemplo para ver mejor las sombras. |
| `APPDATA=<carpeta temporal>` | La app guarda configuración, inventario y bibliotecas en `%APPDATA%\SalaLuces\`. Apuntarla a otra carpeta deja intactos tus datos reales. |

Las pruebas no deben escribir nunca en `%APPDATA%`.

### Trampas conocidas del driver (AMD Radeon 860M)

- `isnan()` en GLSL no detecta los NaN. Para aislar un fallo, anula términos uno a uno.
- `x = c ? f() : g()` con funciones costosas dejó negro todo Fotorrealista. Para elegir entre llamadas, usa `if/else`.
- `pow` con base negativa da NaN: usa `pow(clamp(base, 0.0, 1.0), e)`.
- Solo se ha probado en AMD. **NVIDIA e Intel están sin probar.**

### Decisiones que no conviene deshacer sin medir

- **glow, no wgpu.** En el equipo de desarrollo, wgpu ocupaba un núcleo entero de CPU con la ventana en reposo, tanto con DX12 como con Vulkan; glow no consume nada. La app debe seguir sin gastar CPU cuando no se toca: el trazador deja de repintar al llegar al máximo de muestras.
- **Bevy descartado** (versión 0.20): su trazador Solari no admite focos ni luces de área, y no tiene soporte IES.
- **Un solo `.exe`, sin instalar nada.** Las dependencias opcionales, como OIDN, se cargan en tiempo de ejecución y la app funciona sin ellas.

## Cómo se ha trabajado

Cada función nueva sigue el mismo camino, y todo queda escrito en [docs/superpowers/](docs/superpowers/):

1. **Spec** en `specs/`: qué se quiere, qué no entra y qué decidió el usuario.
2. **Plan** en `plans/`: tareas con firmas, valores y pruebas exactas.
3. **Implementación** con pruebas primero, y luego revisión. Los prompts que se usaron con agentes de IA están en [`prompts/`](docs/superpowers/prompts/README.md).
4. **Registro** en [`plans/2026-11-03-sala-luces-progreso.md`](docs/superpowers/plans/2026-11-03-sala-luces-progreso.md): qué se hizo, cómo se verificó y cada decisión (*Ruling*) con su coste.

Los prompts antiguos dicen "no git; skip commits" porque el repositorio no existía todavía. A partir de ahora, haz un commit por tarea terminada.

## Convenciones

- Interfaz, identificadores y comentarios en **español**.
- Unidades en metros; origen en una esquina del suelo; X = ancho, Z = largo, Y = altura.
- Cada atajo o deuda técnica lleva un comentario `// shortcut: …` que explica qué queda pendiente. Para ver la lista: `git grep "shortcut:"`.
- Los archivos de montaje deben seguir abriéndose: los campos nuevos llevan `#[serde(default)]`.

## Pendiente

- **Conectar el quitarruido OIDN (Tarea C)** de [`plans/2026-11-06-quitarruido.md`](docs/superpowers/plans/2026-11-06-quitarruido.md). Las tareas A (módulo `quitarruido.rs`) y B (lectura de albedo y normal en `trazado.rs`) están hechas y probadas, pero nada las llama todavía. Hay que integrarlas en `render_gl.rs`, añadir la casilla en la ventana Configuración y quitar los `#[allow(dead_code)]` marcados con `shortcut:`.
- **Probar en GPUs NVIDIA e Intel.**
- Deuda menor, marcada en el código: más de 32 luces se ignoran en la GPU (`MAX_LUCES`), la caché de perfiles IES se vacía entera al pasar de 64 y hay un límite de profundidad en el BVH.

## Publicar una versión

1. Sube la versión en `Cargo.toml`.
2. `git tag v0.2.0 && git push origin v0.2.0`.
3. El flujo [`release.yml`](.github/workflows/release.yml) pasa las pruebas, compila el `.exe` y lo publica en *Releases*.
