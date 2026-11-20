//! Configuración de calidad del render (preset, muestras, rebotes, sombras, suavizado, escala),
//! guardada en %APPDATA%\SalaLuces\config.json. Sin archivo se usan valores por GPU.

use crate::modelo::Acabado;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Preset {
    Rapida,
    Alta,
    Fotorrealista,
}

impl Preset {
    pub const TODOS: [Preset; 3] = [Preset::Rapida, Preset::Alta, Preset::Fotorrealista];

    pub fn nombre(self) -> &'static str {
        match self {
            Preset::Rapida => "Rápida",
            Preset::Alta => "Alta",
            Preset::Fotorrealista => "Fotorrealista",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Configuracion {
    pub preset: Preset,
    /// 64..=2048
    pub muestras_max: u32,
    /// 1..=32 (tope de seguridad: el trazado termina por ruleta rusa)
    pub rebotes: u32,
    /// 1024, 2048 o 4096
    pub res_sombras: u32,
    /// 0, 2, 4 u 8 muestras por píxel (0 = sin suavizado)
    pub msaa: u32,
    /// 0.5..=1.0 de la resolución de la ventana
    pub escala: f32,
    /// Quitar el ruido del trazado con OIDN (si la biblioteca está instalada).
    #[serde(default = "verdadero")]
    pub quitar_ruido: bool,
    /// true si viene de un archivo guardado; los valores por GPU no lo son.
    #[serde(skip)]
    pub explicita: bool,
}

fn verdadero() -> bool {
    true
}

/// Opción de `opciones` más cercana a `v`.
fn mas_cercano(v: u32, opciones: &[u32]) -> u32 {
    opciones.iter().copied().min_by_key(|o| o.abs_diff(v)).unwrap_or(v)
}

impl Configuracion {
    /// Lleva cada valor a su rango; un archivo editado a mano puede traer cualquier cosa.
    fn limitar(mut self) -> Self {
        self.muestras_max = self.muestras_max.clamp(64, 2048);
        self.rebotes = self.rebotes.clamp(1, 32);
        self.res_sombras = mas_cercano(self.res_sombras, &[1024, 2048, 4096]);
        self.msaa = mas_cercano(self.msaa, &[0, 2, 4, 8]);
        self.escala = self.escala.clamp(0.5, 1.0);
        self
    }
}

/// %APPDATA%\SalaLuces\config.json; sin APPDATA, "config.json" en la carpeta actual.
pub fn ruta_config() -> PathBuf {
    match std::env::var_os("APPDATA") {
        Some(appdata) => PathBuf::from(appdata).join("SalaLuces").join("config.json"),
        None => PathBuf::from("config.json"),
    }
}

/// Lee la configuración; None si no existe o no se puede leer. Tolera BOM y limita los valores.
pub fn cargar(ruta: &Path) -> Option<Configuracion> {
    let texto = std::fs::read_to_string(ruta).ok()?;
    // Notepad y PowerShell pueden añadir un BOM UTF-8 que serde_json no acepta.
    let texto = texto.strip_prefix('\u{feff}').unwrap_or(&texto);
    let c: Configuracion = serde_json::from_str(texto).ok()?;
    Some(Configuracion { explicita: true, ..c.limitar() })
}

/// Guarda en un temporal y lo renombra encima: si falla, el archivo anterior queda intacto.
pub fn guardar(ruta: &Path, c: &Configuracion) -> Result<(), String> {
    if let Some(dir) = ruta.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("No se pudo crear la carpeta {}: {e}", dir.display()))?;
    }
    let json = serde_json::to_string_pretty(c).map_err(|e| format!("No se pudo preparar la configuración: {e}"))?;
    escribir_atomico(ruta, &json)
}

/// Escribe `json` en un temporal y lo renombra encima de `ruta`.
fn escribir_atomico(ruta: &Path, json: &str) -> Result<(), String> {
    let mut tmp = ruta.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, json).map_err(|e| format!("No se pudo guardar {}: {e}", ruta.display()))?;
    std::fs::rename(&tmp, ruta).map_err(|e| {
        std::fs::remove_file(&tmp).ok();
        format!("No se pudo guardar {}: {e}", ruta.display())
    })
}

/// %APPDATA%\SalaLuces\acabados.json; sin APPDATA, "acabados.json" en la carpeta actual.
pub fn ruta_acabados() -> PathBuf {
    match std::env::var_os("APPDATA") {
        Some(appdata) => PathBuf::from(appdata).join("SalaLuces").join("acabados.json"),
        None => PathBuf::from("acabados.json"),
    }
}

/// %APPDATA%\SalaLuces\ies (biblioteca de perfiles IES); sin APPDATA, "ies" en la carpeta actual.
pub fn ruta_ies() -> PathBuf {
    match std::env::var_os("APPDATA") {
        Some(appdata) => PathBuf::from(appdata).join("SalaLuces").join("ies"),
        None => PathBuf::from("ies"),
    }
}

/// Presets de pared del usuario. Vacío si el archivo no existe; Err si no se puede leer o el JSON está roto.
pub fn cargar_acabados(ruta: &Path) -> Result<BTreeMap<String, Acabado>, String> {
    let texto = match std::fs::read_to_string(ruta) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(e) => return Err(format!("No se pudo leer {}: {e}", ruta.display())),
    };
    // Notepad y PowerShell pueden añadir un BOM UTF-8 que serde_json no acepta.
    let texto = texto.strip_prefix('\u{feff}').unwrap_or(&texto);
    let mut a: BTreeMap<String, Acabado> =
        serde_json::from_str(texto).map_err(|e| format!("El archivo de presets no es válido: {e}"))?;
    for x in a.values_mut() {
        x.reflexion = x.reflexion.clamp(0.0, 0.95);
    }
    Ok(a)
}

/// Crea la carpeta si falta y guarda los presets de forma atómica.
pub fn guardar_acabados(ruta: &Path, a: &BTreeMap<String, Acabado>) -> Result<(), String> {
    if let Some(dir) = ruta.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("No se pudo crear la carpeta {}: {e}", dir.display()))?;
    }
    let json = serde_json::to_string_pretty(a).map_err(|e| format!("No se pudo preparar los presets: {e}"))?;
    escribir_atomico(ruta, &json)
}

/// Renderers de software (sin GPU): van con Rápida, escala 0.5 y sin suavizado.
const SOFTWARE: [&str; 5] = ["Basic Render", "GDI Generic", "llvmpipe", "SwiftShader", "softpipe"];

/// ¿El renderer de OpenGL es software (CPU)?
pub fn es_software(renderer: &str) -> bool {
    SOFTWARE.iter().any(|m| renderer.contains(m))
}

/// GPU dedicada por su (renderer, vendor) de OpenGL. Las APU de AMD ("RX Vega … Graphics", "R7 Graphics") cuentan
/// como integradas; las desconocidas también (es el caso prudente).
fn es_dedicada(renderer: &str, vendor: &str) -> bool {
    let n = format!("{renderer} {vendor}");
    const DEDICADAS: [&str; 8] = ["NVIDIA", "GeForce", "RTX", "Quadro", "Radeon Pro", "Arc(TM) A", "Arc(TM) B", "Arc A"];
    if DEDICADAS.iter().any(|m| n.contains(m)) {
        return true;
    }
    if n.contains("RX") {
        return !(n.contains("RX Vega") && n.contains("Graphics"));
    }
    let amd = n.contains("AMD") || n.contains("ATI") || n.contains("Radeon");
    amd && ["Radeon VII", "R9 ", "R7 ", "HD "].iter().any(|m| n.contains(m)) && !n.contains("Graphics")
}

/// Si la GPU es integrada (las desconocidas cuentan como integradas: es el caso prudente).
pub fn es_integrada(renderer: &str, vendor: &str) -> bool {
    !es_dedicada(renderer, vendor)
}

/// Ajusta una configuración a la GPU: sombras de 4096 px solo con GPU dedicada; sin Fotorrealista en software.
pub fn limitar_a_gpu(c: Configuracion, renderer: &str, vendor: &str) -> Configuracion {
    let mut c = c;
    if c.res_sombras > 2048 && es_integrada(renderer, vendor) {
        c.res_sombras = 2048;
    }
    if es_software(renderer) && c.preset == Preset::Fotorrealista {
        c.preset = Preset::Rapida;
    }
    c
}

/// Valores recomendados para la GPU detectada (`renderer` y `vendor` de OpenGL).
pub fn por_defecto_para(renderer: &str, vendor: &str) -> Configuracion {
    if es_software(renderer) {
        return Configuracion {
            preset: Preset::Rapida,
            muestras_max: 256,
            rebotes: 16,
            res_sombras: 2048,
            msaa: 0,
            escala: 0.5,
            quitar_ruido: true,
            explicita: false,
        };
    }
    let integrada = es_integrada(renderer, vendor);
    Configuracion {
        preset: Preset::Alta,
        muestras_max: 256,
        rebotes: 16,
        res_sombras: 2048,
        msaa: if integrada { 2 } else { 4 },
        escala: if integrada { 0.75 } else { 1.0 },
        quitar_ruido: true,
        explicita: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ruta_prueba(nombre: &str) -> PathBuf {
        std::env::temp_dir().join(format!("sala_luces_config_{nombre}_{}.json", std::process::id()))
    }

    #[test]
    fn config_ida_y_vuelta() {
        let ruta = ruta_prueba("ida_vuelta");
        let mut c = por_defecto_para("NVIDIA GeForce RTX 4070", "NVIDIA Corporation");
        c.preset = Preset::Fotorrealista;
        c.muestras_max = 1024;
        c.rebotes = 3;
        c.res_sombras = 4096;
        c.msaa = 8;
        c.escala = 0.6;
        guardar(&ruta, &c).unwrap();
        let leida = cargar(&ruta).expect("debe leerse");
        std::fs::remove_file(&ruta).ok();
        assert!(leida.explicita, "un archivo guardado es explícito");
        assert_eq!(Configuracion { explicita: false, ..leida }, c);
    }

    #[test]
    fn config_limita_valores() {
        let ruta = ruta_prueba("limita");
        let json = r#"{"preset":"Alta","muestras_max":10,"rebotes":9,"res_sombras":3000,"msaa":5,"escala":0.1}"#;
        std::fs::write(&ruta, json).unwrap();
        let c = cargar(&ruta);
        std::fs::remove_file(&ruta).ok();
        let c = c.expect("JSON válido");
        assert_eq!((c.muestras_max, c.rebotes, c.res_sombras, c.msaa), (64, 9, 2048, 4));
        assert!((c.escala - 0.5).abs() < 1e-6, "{}", c.escala);
    }

    #[test]
    fn config_limita_rebotes_a_32() {
        let ruta = ruta_prueba("rebotes");
        std::fs::write(&ruta, r#"{"preset":"Alta","muestras_max":256,"rebotes":99,"res_sombras":2048,"msaa":0,"escala":1.0}"#).unwrap();
        let c = cargar(&ruta);
        std::fs::remove_file(&ruta).ok();
        assert_eq!(c.map(|c| c.rebotes), Some(32));
    }

    #[test]
    fn config_corrupta_es_none() {
        let ruta = ruta_prueba("corrupta");
        std::fs::write(&ruta, "{ roto").unwrap();
        let roto = cargar(&ruta);
        std::fs::remove_file(&ruta).ok();
        assert!(roto.is_none());
        assert!(cargar(&ruta_prueba("no_existe")).is_none());
    }

    #[test]
    fn config_tolera_bom() {
        let ruta = ruta_prueba("bom");
        std::fs::write(&ruta, "\u{feff}{\"preset\":\"Rapida\",\"muestras_max\":256,\"rebotes\":2,\"res_sombras\":1024,\"msaa\":0,\"escala\":1.0}").unwrap();
        let c = cargar(&ruta);
        std::fs::remove_file(&ruta).ok();
        assert_eq!(c.map(|c| c.preset), Some(Preset::Rapida));
    }

    /// Los nombres que dan integrada, con su vendor típico.
    #[test]
    fn integradas_conocidas() {
        let casos = [
            ("AMD Radeon(TM) 860M Graphics", "ATI Technologies Inc."),
            ("AMD Radeon(TM) 780M Graphics", "ATI Technologies Inc."),
            ("AMD Radeon RX Vega 8 Graphics", "ATI Technologies Inc."),
            ("AMD Radeon(TM) R7 Graphics", "ATI Technologies Inc."),
            ("Intel(R) UHD Graphics 770", "Intel"),
            ("Intel(R) Iris(R) Xe Graphics", "Intel"),
            ("Intel(R) HD Graphics 630", "Intel"),
            ("Intel(R) Arc(TM) Graphics", "Intel"),
            ("Intel(R) Arc(TM) 140V GPU", "Intel"),
        ];
        for (renderer, vendor) in casos {
            assert!(es_integrada(renderer, vendor), "{renderer}");
            let c = por_defecto_para(renderer, vendor);
            assert_eq!((c.preset, c.escala, c.msaa, c.res_sombras), (Preset::Alta, 0.75, 2, 2048), "{renderer}");
            assert_eq!((c.muestras_max, c.rebotes), (256, 16));
        }
    }

    /// Los nombres que dan dedicada, con su vendor típico.
    #[test]
    fn dedicadas_conocidas() {
        let casos = [
            ("NVIDIA GeForce RTX 4070", "NVIDIA Corporation"),
            ("NVIDIA GeForce RTX 3050 Laptop GPU", "NVIDIA Corporation"),
            ("AMD Radeon RX 7800 XT", "ATI Technologies Inc."),
            ("AMD Radeon(TM) RX 480", "ATI Technologies Inc."),
            ("AMD Radeon RX Vega 64", "ATI Technologies Inc."),
            ("AMD Radeon VII", "ATI Technologies Inc."),
            ("AMD Radeon R9 390 Series", "ATI Technologies Inc."),
            ("AMD Radeon R7 200 Series", "ATI Technologies Inc."),
            ("AMD Radeon HD 7970", "ATI Technologies Inc."),
            ("AMD Radeon Pro W5700", "ATI Technologies Inc."),
            ("Intel(R) Arc(TM) A770 Graphics", "Intel"),
            ("Intel(R) Arc(TM) A380", "Intel"),
            ("Intel(R) Arc(TM) B580 Graphics", "Intel"),
        ];
        for (renderer, vendor) in casos {
            assert!(!es_integrada(renderer, vendor), "{renderer}");
            let c = por_defecto_para(renderer, vendor);
            assert_eq!((c.preset, c.escala, c.msaa), (Preset::Alta, 1.0, 4), "{renderer}");
        }
    }

    #[test]
    fn gpus_de_software() {
        for renderer in ["GDI Generic", "Microsoft Basic Render Driver", "llvmpipe (LLVM 15.0.7, 256 bits)", "SwiftShader Device (Subzero)", "softpipe"] {
            assert!(es_software(renderer), "{renderer}");
            assert!(es_integrada(renderer, "Mesa"), "{renderer}");
            let c = por_defecto_para(renderer, "Microsoft Corporation");
            assert_eq!((c.preset, c.escala, c.msaa), (Preset::Rapida, 0.5, 0), "{renderer}");
            let forzada = limitar_a_gpu(Configuracion { preset: Preset::Fotorrealista, ..c }, renderer, "Mesa");
            assert_eq!(forzada.preset, Preset::Rapida, "{renderer}");
        }
        assert!(!es_software("NVIDIA GeForce RTX 4070"));
        assert!(!es_software("Intel(R) UHD Graphics 770"));
    }

    fn dir_acabados(nombre: &str) -> PathBuf {
        std::env::temp_dir().join(format!("sala_luces_acabados_{nombre}_{}", std::process::id()))
    }

    #[test]
    fn acabados_ida_y_vuelta() {
        let dir = dir_acabados("ida_vuelta");
        let ruta = dir.join("acabados.json");
        let mut a = BTreeMap::new();
        a.insert("Mi verde".to_string(), Acabado { color: [10, 200, 30], reflexion: 0.4 });
        a.insert("Gris".to_string(), Acabado { color: [90, 90, 90], reflexion: 0.2 });
        let escrito = guardar_acabados(&ruta, &a);
        let leido = cargar_acabados(&ruta);
        std::fs::remove_dir_all(&dir).ok();
        escrito.unwrap();
        assert_eq!(leido, Ok(a));
    }

    #[test]
    fn acabados_inexistente_es_vacio() {
        let ruta = dir_acabados("no_existe").join("acabados.json");
        assert_eq!(cargar_acabados(&ruta), Ok(BTreeMap::new()));
    }

    #[test]
    fn acabados_json_roto_falla() {
        let dir = dir_acabados("roto");
        std::fs::create_dir_all(&dir).unwrap();
        let ruta = dir.join("acabados.json");
        std::fs::write(&ruta, "\u{feff}{ roto").unwrap();
        let r = cargar_acabados(&ruta);
        std::fs::remove_dir_all(&dir).ok();
        assert!(r.is_err());
    }

    #[test]
    fn acabados_limita_reflexion() {
        let dir = dir_acabados("limita");
        std::fs::create_dir_all(&dir).unwrap();
        let ruta = dir.join("acabados.json");
        std::fs::write(&ruta, r#"{"Brillante":{"color":[255,255,255],"reflexion":1.5}}"#).unwrap();
        let r = cargar_acabados(&ruta);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(r.unwrap()["Brillante"].reflexion, 0.95);
    }

    #[test]
    fn ruta_acabados_termina_en_acabados_json() {
        assert!(ruta_acabados().ends_with("acabados.json"));
    }

    #[test]
    fn limitar_sombras_en_integrada() {
        let c = Configuracion { res_sombras: 4096, ..por_defecto_para("NVIDIA GeForce RTX 4070", "NVIDIA") };
        assert_eq!(limitar_a_gpu(c.clone(), "Intel(R) UHD Graphics 770", "Intel").res_sombras, 2048);
        assert_eq!(limitar_a_gpu(c, "NVIDIA GeForce RTX 4070", "NVIDIA").res_sombras, 4096);
    }
}
