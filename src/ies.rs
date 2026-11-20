//! Perfiles fotométricos IES (LM-63): tabla de candelas medida por el fabricante, tipo C.

use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, PartialEq)]
pub struct Ies {
    pub vert: Vec<f32>,
    pub horiz: Vec<f32>,
    /// Candelas ya × multiplicador, [h][v].
    pub cd: Vec<Vec<f32>>,
    pub max_cd: f32,
}

/// Lee un archivo IES LM-63 de tipo C con TILT=NONE. Error con un mensaje en español.
pub fn leer(texto: &str) -> Result<Ies, String> {
    // Cabecera hasta la línea TILT=; lo que sigue son números separados por espacios, comas o saltos.
    let mut tilt = false;
    let mut resto = String::new();
    for linea in texto.lines() {
        if tilt {
            resto.push_str(linea);
            resto.push(' ');
        } else if let Some(v) = linea.trim().to_ascii_uppercase().strip_prefix("TILT=") {
            if v.trim() != "NONE" {
                return Err("TILT no soportado".to_string());
            }
            tilt = true;
        }
    }
    if !tilt {
        return Err("Falta la línea TILT= del archivo IES".to_string());
    }
    let nums = resto
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|t| !t.is_empty())
        .map(|t| t.parse::<f32>().ok().filter(|x| x.is_finite()).ok_or_else(|| format!("Número no válido: {t}")))
        .collect::<Result<Vec<f32>, String>>()?;
    let mut it = nums.into_iter();
    let mut sig = || it.next().ok_or_else(|| "Faltan datos en el archivo IES".to_string());
    // n_lamparas, lm_por_lampara, multiplicador, n_vert, n_horiz, tipo, unidades, ancho, largo, alto.
    sig()?;
    sig()?;
    let mult = sig()?;
    let n_vert = entero(sig()?)?;
    let n_horiz = entero(sig()?)?;
    if sig()? != 1.0 {
        return Err("Solo se admiten perfiles de tipo C".to_string());
    }
    for _ in 0..4 {
        sig()?;
    }
    // factor_balasto, factor_futuro, vatios.
    for _ in 0..3 {
        sig()?;
    }
    let vert: Vec<f32> = (0..n_vert).map(|_| sig()).collect::<Result<_, _>>()?;
    let horiz: Vec<f32> = (0..n_horiz).map(|_| sig()).collect::<Result<_, _>>()?;
    creciente(&vert, "verticales", 0.0, 180.0)?;
    creciente(&horiz, "horizontales", 0.0, 360.0)?;
    let mut cd = Vec::with_capacity(n_horiz);
    for _ in 0..n_horiz {
        let fila: Vec<f32> = (0..n_vert).map(|_| sig().map(|c| c * mult)).collect::<Result<_, _>>()?;
        if let Some(c) = fila.iter().find(|c| **c < 0.0) {
            return Err(format!("Candelas negativas en el archivo IES: {c}"));
        }
        cd.push(fila);
    }
    let max_cd = cd.iter().flatten().fold(0.0f32, |m, &c| m.max(c));
    if !(max_cd > 0.0) {
        return Err("El perfil IES no tiene candelas positivas".to_string());
    }
    Ok(Ies { vert, horiz, cd, max_cd })
}

/// Cuenta de la cabecera: entero positivo.
fn entero(x: f32) -> Result<usize, String> {
    if x >= 1.0 && x.fract() == 0.0 {
        Ok(x as usize)
    } else {
        Err(format!("Cuenta no válida en el archivo IES: {x}"))
    }
}

/// Ángulos estrictamente crecientes dentro de [lo, hi].
fn creciente(v: &[f32], que: &str, lo: f32, hi: f32) -> Result<(), String> {
    let ok = v.iter().all(|&a| a >= lo && a <= hi) && v.windows(2).all(|w| w[0] < w[1]);
    if ok { Ok(()) } else { Err(format!("Ángulos {que} no válidos en el archivo IES")) }
}

/// Índices y peso para interpolar `x` en `xs` (crecientes); x fuera se recorta al rango.
fn pesos(xs: &[f32], x: f32) -> (usize, usize, f32) {
    let x = x.max(xs[0]).min(xs[xs.len() - 1]);
    let i1 = xs.partition_point(|&a| a < x);
    if i1 == 0 {
        return (0, 0, 0.0);
    }
    let i0 = i1 - 1;
    (i0, i1, (x - xs[i0]) / (xs[i1] - xs[i0]))
}

impl Ies {
    /// Candelas en (θ, φ) en grados; 0 fuera de los verticales. Bilineal, con la simetría del archivo.
    pub fn intensidad(&self, theta_deg: f32, phi_deg: f32) -> f32 {
        let (vl, vh) = (self.vert[0], self.vert[self.vert.len() - 1]);
        if !(theta_deg >= vl && theta_deg <= vh) {
            return 0.0;
        }
        // Simetría por el último ángulo horizontal: 0 = misma para todo φ, 90 = cuadrantes, 180 = bilateral.
        let p = phi_deg.rem_euclid(360.0);
        let p = match self.horiz[self.horiz.len() - 1] as i32 {
            0 => 0.0,
            90 => {
                let a = if p > 180.0 { 360.0 - p } else { p };
                if a > 90.0 { 180.0 - a } else { a }
            }
            180 => {
                if p > 180.0 {
                    360.0 - p
                } else {
                    p
                }
            }
            _ => p,
        };
        let (v0, v1, tv) = pesos(&self.vert, theta_deg);
        let (h0, h1, th) = pesos(&self.horiz, p);
        let a = self.cd[h0][v0] + (self.cd[h0][v1] - self.cd[h0][v0]) * tv;
        let b = self.cd[h1][v0] + (self.cd[h1][v1] - self.cd[h1][v0]) * tv;
        a + (b - a) * th
    }
}

/// Biblioteca de perfiles en `dir`: nombre (archivo sin .ies) → texto. Los que no se leen se avisan y se
/// ignoran. Una carpeta que no existe es una biblioteca vacía.
pub fn cargar_biblioteca(dir: &Path) -> (BTreeMap<String, String>, Vec<String>) {
    let entradas = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (BTreeMap::new(), Vec::new()),
        Err(e) => return (BTreeMap::new(), vec![format!("No se pudo leer la biblioteca IES {}: {e}", dir.display())]),
    };
    let mut lib = BTreeMap::new();
    let mut avisos = Vec::new();
    for ruta in entradas.filter_map(|e| e.ok().map(|e| e.path())) {
        if !ruta.is_file() || !ruta.extension().is_some_and(|e| e.eq_ignore_ascii_case("ies")) {
            continue;
        }
        let nombre = ruta.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        match std::fs::read(&ruta) {
            Ok(bytes) => {
                let texto = String::from_utf8_lossy(&bytes).into_owned();
                match leer(&texto) {
                    Ok(_) => {
                        lib.insert(nombre, texto);
                    }
                    Err(e) => avisos.push(format!("{}: {e}", ruta.display())),
                }
            }
            Err(e) => avisos.push(format!("No se pudo leer {}: {e}", ruta.display())),
        }
    }
    (lib, avisos)
}

/// Copia un perfil a la biblioteca como `dir/<nombre>.ies`; el nombre se limpia de caracteres de ruta.
pub fn guardar_en_biblioteca(dir: &Path, nombre: &str, texto: &str) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("No se pudo crear {}: {e}", dir.display()))?;
    let ruta = dir.join(format!("{}.ies", crate::importar::archivo_seguro(nombre)));
    std::fs::write(&ruta, texto).map_err(|e| format!("No se pudo guardar {}: {e}", ruta.display()))
}

/// Tipo C, simetría 0, verticales 0, 30, 60, 90 y candelas 1000, 800, 300, 0 (lo usan las pruebas de todo el crate).
#[cfg(test)]
pub(crate) const EJEMPLO: &str = "IESNA:LM-63-2002\n[TEST] ejemplo\nTILT=NONE\n1 1000 1 4 1 1 2 0.1 0.1 0.1\n1 1 100\n0 30 60 90\n0\n1000 800 300 0\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leer_ejemplo() {
        let ies = leer(EJEMPLO).unwrap();
        assert_eq!(ies.max_cd, 1000.0);
        assert_eq!(ies.intensidad(30.0, 123.0), 800.0);
        assert_eq!(ies.intensidad(45.0, 0.0), 550.0);
        assert_eq!(ies.intensidad(95.0, 0.0), 0.0, "fuera de los verticales");
    }

    #[test]
    fn leer_simetria_180() {
        // Horizontales 0, 90 y 180 (verticales 0 y 90): φ y 360 − φ dan lo mismo.
        let texto = "IESNA:LM-63-2002\nTILT=NONE\n1 1000 1 2 3 1 2 0.1 0.1 0.1\n1 1 100\n0 90\n0 90 180\n100 50 200 80 300 90\n";
        let ies = leer(texto).unwrap();
        assert_eq!(ies.max_cd, 300.0);
        for v in [0.0, 45.0, 90.0] {
            assert_eq!(ies.intensidad(v, 270.0), ies.intensidad(v, 90.0), "v {v}");
        }
    }

    #[test]
    fn leer_errores() {
        let incluye = leer(&EJEMPLO.replace("TILT=NONE", "TILT=INCLUDE"));
        assert_eq!(incluye.unwrap_err(), "TILT no soportado");
        assert!(leer(&EJEMPLO.replace("1 1000 1 4 1 1 2", "1 1000 1 4 1 2 2")).is_err(), "tipo B");
        assert!(leer(&EJEMPLO.replace("1000 800 300 0", "1000 800 300")).is_err(), "faltan candelas");
    }
}
