# Imagen más limpia: sondas con más muestras y quitarruido OIDN

Dos mejoras de imagen sin cambiar de motor:

1. **Alta:** las manchas de colores de la luz indirecta vienen de que cada sonda se calcula con solo 128 caminos
   (≈ 9 % de ruido). Se sube a 1024 (≈ 3 %).
2. **Fotorrealista:** quitarruido Intel Open Image Denoise (OIDN 2.5, Apache 2.0) sobre el promedio del trazado,
   ayudado por el albedo y la normal del primer impacto. Se ve limpio con decenas de muestras en vez de cientos.

## Decisiones (ya tomadas, no cambiar)

- **OIDN se carga en tiempo de ejecución** con `libloading` (ya está en Cargo.lock como dependencia transitiva,
  versión 0.8): nada se enlaza al compilar, el toolchain GNU no importa y la app funciona igual sin las DLL
  (el quitarruido sale como «no disponible»).
- **Dónde se buscan las DLL**, en orden: carpeta del ejecutable, `%APPDATA%\SalaLuces\oidn\`. Archivo:
  `OpenImageDenoise.dll`. Se carga con `libloading::os::windows::Library::load_with_flags(ruta,
  LOAD_WITH_ALTERED_SEARCH_PATH)` para que encuentre a su lado `OpenImageDenoise_core.dll`, `tbb12.dll` y los
  `OpenImageDenoise_device_*.dll`.
- **Dispositivo:** `OIDN_DEVICE_TYPE_DEFAULT` (0): OIDN elige la GPU si puede (HIP en AMD) y si no la CPU. Se guarda
  el tipo elegido (`oidnGetDeviceInt(dev, "type")`) para mostrarlo: 1 CPU, 2 SYCL, 3 CUDA, 4 HIP.
- **Memoria:** buffers de OIDN (`oidnNewBuffer` + `oidnWriteBuffer` / `oidnReadBuffer`), no imágenes compartidas
  (con GPU la memoria del proceso no siempre es accesible).
- **Filtro:** `"RT"`, `hdr = true`, imágenes `color`, `albedo`, `normal`, `output`, formato `OIDN_FORMAT_FLOAT3` (3).
  Sin albedo/normal válidos (ver abajo) se usa solo `color`.
- **Cuándo:** al completar 8, 32, 128 muestras y al llegar al máximo (cada hito una vez por acumulación). El trabajo
  de OIDN va en un hilo; la GPU solo lee y sube texturas. Si la acumulación se reinicia mientras el hilo trabaja,
  el resultado se tira (contador de generación).
- **Qué se ve:** el último resultado limpio de la acumulación actual, si lo hay; si no, el promedio con ruido como
  ahora. La exposición, el balance y ACES se aplican después (FS_TONO), igual para las dos.
- **Interfaz:** en Configuración, casilla «Quitar ruido (OIDN)» (guardada en `Configuracion.quitar_ruido`,
  `#[serde(default = "verdadero")]`), y una línea de estado: «OIDN: GPU (HIP)», «OIDN: CPU», o
  «OIDN no encontrado: copia OpenImageDenoise*.dll y tbb12.dll junto al programa o en %APPDATA%\SalaLuces\oidn».
  Si la carga o un filtro fallan, se avisa una vez y se desactiva para la sesión.

## Tarea A — sondas y módulo OIDN (`trazado.rs` una línea, `src/quitarruido.rs` nuevo, `Cargo.toml`, `main.rs`)

1. `trazado.rs`: `MUESTRAS_SONDAS` de 128 a 1024. Comprobar que nada más dependa de 128 (tests, `por_fotograma`).
2. `Cargo.toml`: `libloading = "0.8"`.
3. `src/quitarruido.rs` (declarar `mod quitarruido;` en main.rs). Interfaz:

```rust
/// OIDN cargado y su dispositivo listo.
pub struct Oidn { /* Library, punteros a función, OIDNDevice, tipo */ }

impl Oidn {
    /// Busca OpenImageDenoise.dll en `carpetas` (en orden), la carga y crea y confirma el dispositivo por defecto.
    /// Err con un texto para el usuario si no está o falla.
    pub fn cargar(carpetas: &[std::path::PathBuf]) -> Result<Oidn, String>;
    /// "GPU (HIP)", "GPU (CUDA)", "GPU (SYCL)", "CPU".
    pub fn dispositivo(&self) -> &'static str;
    /// Quita el ruido de `color` (RGB lineal, w·h·3 f32). `aux` = Some((albedo, normal)) del mismo tamaño.
    /// Devuelve la imagen limpia (w·h·3) o el mensaje de error de OIDN.
    pub fn filtrar(&self, w: usize, h: usize, color: &[f32], aux: Option<(&[f32], &[f32])>) -> Result<Vec<f32>, String>;
}
impl Drop for Oidn { /* oidnReleaseDevice */ }
// Oidn debe ser Send + Sync (OIDN es seguro entre hilos con un dispositivo): `unsafe impl Send/Sync` con una línea
// que lo justifique. Se usa detrás de un Arc desde el hilo de quitarruido.

/// Carpetas donde se busca: la del ejecutable y %APPDATA%\SalaLuces\oidn.
pub fn carpetas_por_defecto() -> Vec<std::path::PathBuf>;
```

   Funciones C (todas `extern "C"`, tipos opacos `*mut c_void`, `size_t` = usize):
   `oidnNewDevice(i32) -> dev`, `oidnCommitDevice(dev)`, `oidnGetDeviceError(dev, *mut *const c_char) -> i32`,
   `oidnGetDeviceInt(dev, *const c_char) -> i32`, `oidnReleaseDevice(dev)`,
   `oidnNewBuffer(dev, usize) -> buf`, `oidnWriteBuffer(buf, usize off, usize bytes, *const c_void)`,
   `oidnReadBuffer(buf, usize off, usize bytes, *mut c_void)`, `oidnReleaseBuffer(buf)`,
   `oidnNewFilter(dev, *const c_char) -> filt`,
   `oidnSetFilterImage(filt, *const c_char nombre, buf, i32 formato, usize w, usize h, usize off, usize pixStride, usize rowStride)`
   (strides 0 = compacto), `oidnSetFilterBool(filt, *const c_char, bool)`, `oidnCommitFilter(filt)`,
   `oidnExecuteFilter(filt)`, `oidnReleaseFilter(filt)`.
   Confirmar nombres y firmas en `include/OpenImageDenoise/oidn.h` del paquete (ver «Archivos de OIDN»).
   Tras cada paso que puede fallar, `oidnGetDeviceError`; si ≠ 0, Err con el mensaje. Liberar buffers y filtro siempre
   (también en error). Comprobar `color.len() == w*h*3` (y aux) antes de llamar a nada: Err si no cuadra.

4. Pruebas (sin la DLL presente, que es el caso en los tests):
   - `cargar(&[carpeta_vacia])` da Err que menciona `OpenImageDenoise.dll`.
   - `carpetas_por_defecto()` tiene la carpeta del ejecutable primero y termina en `SalaLuces\oidn`.
   - Una prueba `#[ignore]` que, si `OIDN_DIR` está definida, carga desde ahí y filtra una imagen 64×64 de ruido
     sobre gris 0.5: la desviación de la salida es menor que la de la entrada. (La ejecutará el coordinador a mano.)

## Tarea B — trazado: albedo/normal y lectura (`trazado.rs`)

- `camino` gana `out vec3 alb0, out vec3 n0` (albedo y normal del primer impacto; 0 si no hay impacto; para la piel,
  su `alb`). `MAIN_SONDAS` los ignora.
- `Acumulo` gana el número de capas: `Acumulo::crear(gl, w, h, capas)`; sondas 1, píxeles 3. Con 3, cada lado del
  ping-pong tiene 3 texturas RGBA32F en COLOR_ATTACHMENT0..2 y `gl.draw_buffers(&[ATT0, ATT1, ATT2])`.
  `tex` pasa a ser por lado: `tex[lado][capa]` (o `tex[lado * capas + capa]`, lo que quede más claro).
- `MAIN_TRAZA`: `layout(location = 0) out vec4 fragColor;` (ya existe, comprobar), `layout(location = 1) out vec4 fragAlb;`
  `layout(location = 2) out vec4 fragNor;`, con `uPrevAlb`, `uPrevNor` (texturas 3 y 4) y la misma media incremental
  que el color. Si `FS_PREFIJO` declara `fragColor` para los dos programas, ver cómo hacerlo sin romper `MAIN_SONDAS`.
- `Trazador::leer_para_quitar_ruido(&self, gl) -> Option<(usize, usize, Vec<f32>, Vec<f32>, Vec<f32>)>`:
  lee las 3 capas del lado actual (`read_pixels` RGBA FLOAT del fbo con `read_buffer(ATTn)`) y devuelve RGB compacto:
  color = rgb / a donde a > 0 (si no, 0); albedo tal cual; normal tal cual (normalizada si su longitud > 1e-6).
- `Trazador::subir_limpia(&mut self, gl, w, h, rgb: &[f32])`: textura RGBA32F `limpia` (a = 1 donde la cobertura
  era > 0 en el momento de leer; guardar esa cobertura junto al resultado o rellenar a con la cobertura leída).
  `Trazador::reiniciar` olvida la limpia. `mostrar` usa la limpia si existe.
- Pruebas: la función de CPU que pasa RGBA → RGB (división por cobertura, normalización) como función libre con test.

## Tarea C — integración (`render_gl.rs`, `config.rs`, `app.rs`)

- `config.rs`: `quitar_ruido: bool` con serde default true; los presets lo ponen a true.
- `Renderer`: `oidn: Option<Arc<Oidn>>` (se carga una vez al crear el renderer; el error se guarda como texto para la
  interfaz), hitos `[8, 32, 128, muestras_max]`, `hito_hecho: usize`, `generacion: u64` (sube en cada reinicio de la
  acumulación), y un canal `std::sync::mpsc` con `(generacion, w, h, Vec<f32>)` del hilo.
  En `pasar_trazo`, tras `avanzar`: si `quitar_ruido` y hay OIDN, no hay hilo en curso, y `tz.muestras` alcanzó el
  siguiente hito: leer, lanzar `std::thread::spawn` con `filtrar`, marcar el hito. Cada fotograma: `try_recv`; si la
  generación coincide, `subir_limpia`. Mientras haya un hilo en curso, `pide_repintado()` devuelve true (para
  recoger el resultado).
  Error de OIDN en el hilo: se manda como Err, el renderer lo pasa a `avisos` una vez y apaga el quitarruido.
- `Renderer::estado_oidn(&self) -> String` con los textos de las decisiones.
- `app.rs`, ventana Configuración: la casilla (deshabilitada si no hay OIDN) y la línea de estado.
  Cambiarla aplica sin recrear el renderer si es posible (como otros ajustes de `aplicar_config`).

## Archivos de OIDN (coordinador)

Paquete oficial `oidn-2.5.1.x64.windows.zip` (github.com/RenderKit/oidn/releases) descargado en
`<scratchpad>\oidn_dl\oidn-2.5.1.x64.windows\`. Para usarlo hacen falta, junto al .exe o en
`%APPDATA%\SalaLuces\oidn\`: `OpenImageDenoise.dll`, `OpenImageDenoise_core.dll`, `OpenImageDenoise_device_cpu.dll`,
`OpenImageDenoise_device_hip.dll`, `tbb12.dll` (≈ 66 MB). Ejecutar esas DLL necesita el permiso del usuario.

## Comprobación final (coordinador)

`cargo test` sin warnings; en la app, Alta sin manchas de color tras converger las sondas; Fotorrealista con OIDN
limpio a las 32 muestras, y sin OIDN sigue igual que antes.
