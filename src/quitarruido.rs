//! Quitarruido con Intel Open Image Denoise. La DLL se carga en tiempo de ejecución: si no está, la app sigue igual.

use std::ffi::{c_char, c_void, CStr};
use std::path::{Path, PathBuf};
use std::ptr::null_mut;

use libloading::os::windows::{Library, LOAD_WITH_ALTERED_SEARCH_PATH};

const DLL: &str = "OpenImageDenoise.dll";
/// `OIDN_FORMAT_FLOAT3` de oidn.h.
const FORMATO_FLOAT3: i32 = 3;

type Dev = *mut c_void;
type Buf = *mut c_void;
type Filt = *mut c_void;

/// Funciones de OIDN 2.5 (oidn.h), resueltas al cargar la DLL.
struct Fns {
    new_device: unsafe extern "C" fn(i32) -> Dev,
    commit_device: unsafe extern "C" fn(Dev),
    get_device_error: unsafe extern "C" fn(Dev, *mut *const c_char) -> i32,
    get_device_int: unsafe extern "C" fn(Dev, *const c_char) -> i32,
    release_device: unsafe extern "C" fn(Dev),
    new_buffer: unsafe extern "C" fn(Dev, usize) -> Buf,
    write_buffer: unsafe extern "C" fn(Buf, usize, usize, *const c_void),
    read_buffer: unsafe extern "C" fn(Buf, usize, usize, *mut c_void),
    release_buffer: unsafe extern "C" fn(Buf),
    new_filter: unsafe extern "C" fn(Dev, *const c_char) -> Filt,
    set_filter_image: unsafe extern "C" fn(Filt, *const c_char, Buf, i32, usize, usize, usize, usize, usize),
    set_filter_bool: unsafe extern "C" fn(Filt, *const c_char, bool),
    commit_filter: unsafe extern "C" fn(Filt),
    execute_filter: unsafe extern "C" fn(Filt),
    release_filter: unsafe extern "C" fn(Filt),
}

impl Fns {
    /// Error pendiente del dispositivo (o global si `dev` es nulo), con su texto.
    fn error(&self, dev: Dev) -> Option<String> {
        let mut msg: *const c_char = std::ptr::null();
        // SAFETY: dev es nulo o un dispositivo vivo; msg recibe un texto que OIDN guarda hasta la siguiente llamada.
        let codigo = unsafe { (self.get_device_error)(dev, &mut msg) };
        if codigo == 0 {
            return None;
        }
        if msg.is_null() {
            return Some(format!("error {codigo} de OIDN"));
        }
        // SAFETY: msg no es nulo y apunta a un texto terminado en nulo que OIDN acaba de devolver.
        Some(unsafe { CStr::from_ptr(msg) }.to_string_lossy().into_owned())
    }
}

/// OIDN cargado y su dispositivo listo.
pub struct Oidn {
    f: Fns,
    /// Mantiene la DLL cargada mientras existan los punteros de `f`.
    _lib: Library,
    dev: Dev,
    /// Tipo de dispositivo elegido por OIDN (1 CPU, 2 SYCL, 3 CUDA, 4 HIP).
    tipo: i32,
}

// SAFETY: OIDN usa el dispositivo solo desde el hilo de quitarruido, que es el único que lo toca tras cargarlo.
unsafe impl Send for Oidn {}
// SAFETY: el renderer nunca usa el mismo dispositivo desde dos hilos a la vez (lo comparte solo a través de Arc).
unsafe impl Sync for Oidn {}

/// Carga un símbolo de la DLL como puntero a función.
fn simbolo<T: Copy>(lib: &Library, nombre: &[u8]) -> Result<T, String> {
    // SAFETY: T es el tipo de función que oidn.h declara para ese nombre.
    let s = unsafe { lib.get::<T>(nombre) }.map_err(|e| format!("OIDN incompleto: {e}"))?;
    Ok(*s)
}

/// Comprueba que los tamaños de las imágenes cuadran con w·h·3 antes de llamar a OIDN.
fn comprobar_tamanos(w: usize, h: usize, color: &[f32], aux: Option<(&[f32], &[f32])>) -> Result<(), String> {
    let n = w * h * 3;
    let aux_ok = aux.is_none_or(|(al, no)| al.len() == n && no.len() == n);
    if color.len() == n && aux_ok {
        Ok(())
    } else {
        Err(format!("tamaños de imagen no cuadran con {w}×{h}×3 = {n}"))
    }
}

impl Oidn {
    /// Busca OpenImageDenoise.dll en `carpetas` (en orden), la carga y crea y confirma el dispositivo por defecto.
    pub fn cargar(carpetas: &[PathBuf]) -> Result<Oidn, String> {
        let ruta = carpetas
            .iter()
            .map(|c| c.join(DLL))
            .find(|p| p.is_file())
            .ok_or_else(|| {
                let buscadas: Vec<String> = carpetas.iter().map(|c| c.display().to_string()).collect();
                format!("{DLL} no encontrado (busqué en {})", buscadas.join(", "))
            })?;
        // SAFETY: es la DLL oficial de OIDN que el usuario copió; cargarla ejecuta su código de inicio, y eso es lo esperado.
        let lib = unsafe { Library::load_with_flags(&ruta, LOAD_WITH_ALTERED_SEARCH_PATH) }
            .map_err(|e| format!("no se pudo cargar {}: {e}", ruta.display()))?;
        let f = Fns {
            new_device: simbolo(&lib, b"oidnNewDevice\0")?,
            commit_device: simbolo(&lib, b"oidnCommitDevice\0")?,
            get_device_error: simbolo(&lib, b"oidnGetDeviceError\0")?,
            get_device_int: simbolo(&lib, b"oidnGetDeviceInt\0")?,
            release_device: simbolo(&lib, b"oidnReleaseDevice\0")?,
            new_buffer: simbolo(&lib, b"oidnNewBuffer\0")?,
            write_buffer: simbolo(&lib, b"oidnWriteBuffer\0")?,
            read_buffer: simbolo(&lib, b"oidnReadBuffer\0")?,
            release_buffer: simbolo(&lib, b"oidnReleaseBuffer\0")?,
            new_filter: simbolo(&lib, b"oidnNewFilter\0")?,
            set_filter_image: simbolo(&lib, b"oidnSetFilterImage\0")?,
            set_filter_bool: simbolo(&lib, b"oidnSetFilterBool\0")?,
            commit_filter: simbolo(&lib, b"oidnCommitFilter\0")?,
            execute_filter: simbolo(&lib, b"oidnExecuteFilter\0")?,
            release_filter: simbolo(&lib, b"oidnReleaseFilter\0")?,
        };
        // SAFETY: tipo 0 (OIDN_DEVICE_TYPE_DEFAULT) no toma punteros; el resultado se libera en Drop.
        let dev = unsafe { (f.new_device)(0) };
        if dev.is_null() {
            return Err(f.error(dev).unwrap_or_else(|| "OIDN no pudo crear el dispositivo".into()));
        }
        let mut oidn = Oidn { f, _lib: lib, dev, tipo: 0 };
        // SAFETY: dev es un dispositivo recién creado y sin confirmar.
        unsafe { (oidn.f.commit_device)(oidn.dev) };
        oidn.comprobar()?;
        // SAFETY: dev está confirmado; "type" y el texto son constantes terminadas en nulo.
        oidn.tipo = unsafe { (oidn.f.get_device_int)(oidn.dev, c"type".as_ptr()) };
        oidn.comprobar()?;
        Ok(oidn)
    }

    /// "GPU (HIP)", "GPU (CUDA)", "GPU (SYCL)", "CPU".
    pub fn dispositivo(&self) -> &'static str {
        match self.tipo {
            2 => "GPU (SYCL)",
            3 => "GPU (CUDA)",
            4 => "GPU (HIP)",
            _ => "CPU",
        }
    }

    /// Quita el ruido de `color` (RGB lineal, w·h·3 f32). `aux` = Some((albedo, normal)) del mismo tamaño.
    /// Devuelve la imagen limpia (w·h·3) o el mensaje de error de OIDN.
    pub fn filtrar(&self, w: usize, h: usize, color: &[f32], aux: Option<(&[f32], &[f32])>) -> Result<Vec<f32>, String> {
        comprobar_tamanos(w, h, color, aux)?;
        let mut bufs = Vec::new();
        let mut filtro = null_mut();
        let resultado = self.ejecutar(w, h, color, aux, &mut bufs, &mut filtro);
        for b in bufs {
            // SAFETY: cada buffer se creó en `buffer` y se libera aquí una sola vez.
            unsafe { (self.f.release_buffer)(b) };
        }
        if !filtro.is_null() {
            // SAFETY: el filtro se creó en `ejecutar` y se libera aquí una sola vez.
            unsafe { (self.f.release_filter)(filtro) };
        }
        resultado
    }

    /// Error pendiente del dispositivo, si lo hay.
    fn comprobar(&self) -> Result<(), String> {
        self.f.error(self.dev).map_or(Ok(()), Err)
    }

    /// Crea un buffer de `bytes` (lo anota en `bufs` para liberarlo aunque falle lo demás) y, si hay datos, los copia.
    fn buffer(&self, bytes: usize, datos: Option<&[f32]>, bufs: &mut Vec<Buf>) -> Result<Buf, String> {
        // SAFETY: el dispositivo está vivo; el buffer se libera en `filtrar`.
        let b = unsafe { (self.f.new_buffer)(self.dev, bytes) };
        self.comprobar()?;
        bufs.push(b);
        if let Some(d) = datos {
            // SAFETY: b tiene `bytes` bytes y d tiene exactamente esos bytes.
            unsafe { (self.f.write_buffer)(b, 0, bytes, d.as_ptr().cast()) };
            self.comprobar()?;
        }
        Ok(b)
    }

    /// Asigna a `filtro` la imagen `nombre` con los datos del buffer `b` (formato float3, filas compactas).
    fn imagen(&self, filtro: Filt, nombre: &CStr, b: Buf, w: usize, h: usize) -> Result<(), String> {
        // SAFETY: filtro y b son válidos; strides 0 para que OIDN los calcule.
        unsafe { (self.f.set_filter_image)(filtro, nombre.as_ptr(), b, FORMATO_FLOAT3, w, h, 0, 0, 0) };
        self.comprobar()
    }

    /// Parte de `filtrar` que crea recursos: todo lo creado queda en `bufs` y `filtro` para liberarlo fuera.
    fn ejecutar(
        &self,
        w: usize,
        h: usize,
        color: &[f32],
        aux: Option<(&[f32], &[f32])>,
        bufs: &mut Vec<Buf>,
        filtro: &mut Filt,
    ) -> Result<Vec<f32>, String> {
        let n = w * h * 3;
        let bytes = n * std::mem::size_of::<f32>();
        // SAFETY: el dispositivo está vivo; el filtro se libera en `filtrar`.
        *filtro = unsafe { (self.f.new_filter)(self.dev, c"RT".as_ptr()) };
        self.comprobar()?;
        let f = *filtro;
        let col = self.buffer(bytes, Some(color), bufs)?;
        self.imagen(f, c"color", col, w, h)?;
        if let Some((albedo, normal)) = aux {
            let ab = self.buffer(bytes, Some(albedo), bufs)?;
            self.imagen(f, c"albedo", ab, w, h)?;
            let nb = self.buffer(bytes, Some(normal), bufs)?;
            self.imagen(f, c"normal", nb, w, h)?;
        }
        let salida = self.buffer(bytes, None, bufs)?;
        self.imagen(f, c"output", salida, w, h)?;
        // SAFETY: f es un filtro válido; hdr porque la luz es lineal y sin tope.
        unsafe { (self.f.set_filter_bool)(f, c"hdr".as_ptr(), true) };
        // SAFETY: f está configurado con sus imágenes y no se ha confirmado antes.
        unsafe { (self.f.commit_filter)(f) };
        self.comprobar()?;
        // SAFETY: f está confirmado y todas sus imágenes siguen vivas.
        unsafe { (self.f.execute_filter)(f) };
        self.comprobar()?;
        let mut out = vec![0f32; n];
        // SAFETY: salida tiene `bytes` bytes y out tiene n f32, es decir `bytes` bytes.
        unsafe { (self.f.read_buffer)(salida, 0, bytes, out.as_mut_ptr().cast()) };
        self.comprobar()?;
        Ok(out)
    }
}

impl Drop for Oidn {
    fn drop(&mut self) {
        // SAFETY: dev es el dispositivo creado en `cargar` y solo se libera aquí.
        unsafe { (self.f.release_device)(self.dev) };
    }
}

/// Carpetas donde se busca: la del ejecutable y %APPDATA%\SalaLuces\oidn.
pub fn carpetas_por_defecto() -> Vec<PathBuf> {
    let mut carpetas = Vec::new();
    if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
        carpetas.push(dir);
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        carpetas.push(PathBuf::from(appdata).join("SalaLuces").join("oidn"));
    }
    carpetas
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn carpeta_vacia() -> PathBuf {
        let dir = std::env::temp_dir().join("sala_luces_oidn_vacia");
        std::fs::create_dir_all(&dir).expect("crear carpeta de prueba");
        dir
    }

    #[test]
    fn cargar_sin_dll_avisa_con_el_nombre() {
        let err = Oidn::cargar(&[carpeta_vacia()]).err().expect("debe fallar sin la DLL");
        assert!(err.contains("OpenImageDenoise.dll"), "mensaje: {err}");
    }

    #[test]
    fn carpetas_por_defecto_empieza_por_el_ejecutable() {
        let carpetas = carpetas_por_defecto();
        let exe = std::env::current_exe().expect("ruta del ejecutable");
        assert_eq!(carpetas.first(), exe.parent().map(|p| p.to_path_buf()).as_ref());
        assert!(carpetas.last().expect("hay carpetas").ends_with(PathBuf::from("SalaLuces").join("oidn")));
    }

    #[test]
    fn tamanos_deben_cuadrar() {
        let (c, a) = (vec![0.0; 12], vec![0.0; 12]);
        assert!(comprobar_tamanos(2, 2, &c, Some((&a, &a))).is_ok());
        assert!(comprobar_tamanos(2, 2, &c[..11], None).is_err());
        assert!(comprobar_tamanos(2, 2, &c, Some((&a[..3], &a))).is_err());
    }

    /// Ruido determinista (LCG) alrededor de gris 0.5, con amplitud 0.1.
    fn ruido(n: usize) -> Vec<f32> {
        let mut s: u32 = 12345;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                0.5 + ((s >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.2
            })
            .collect()
    }

    fn desviacion(v: &[f32]) -> f32 {
        let m = v.iter().sum::<f32>() / v.len() as f32;
        (v.iter().map(|x| (x - m) * (x - m)).sum::<f32>() / v.len() as f32).sqrt()
    }

    /// Con `OIDN_DIR` apuntando a la carpeta del paquete oficial: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn filtrar_reduce_el_ruido() {
        let dir = std::env::var("OIDN_DIR").expect("definir OIDN_DIR");
        let oidn = Oidn::cargar(&[PathBuf::from(dir)]).expect("cargar OIDN");
        let (w, h) = (64, 64);
        let color = ruido(w * h * 3);
        let salida = oidn.filtrar(w, h, &color, None).expect("filtrar");
        assert_eq!(salida.len(), color.len());
        assert!(desviacion(&salida) < desviacion(&color), "la salida debe tener menos ruido");
    }
}
