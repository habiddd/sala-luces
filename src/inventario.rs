use crate::modelo::{Equipo, TipoLuz, CATEGORIAS_BASE, CATEGORIA_LUCES, CATEGORIA_LUCES_PISO};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
struct Fichero {
    equipos: Vec<Equipo>,
    #[serde(default)]
    categorias: Vec<String>,
}

/// %APPDATA%\SalaLuces\inventario.json; sin APPDATA, "inventario.json" en la carpeta actual.
pub fn ruta_por_defecto() -> PathBuf {
    match std::env::var_os("APPDATA") {
        Some(appdata) => PathBuf::from(appdata).join("SalaLuces").join("inventario.json"),
        None => PathBuf::from("inventario.json"),
    }
}

/// Código, marca, modelo y categoría sin espacios; el código no puede quedar vacío.
fn normaliza(e: Equipo) -> Result<Equipo, String> {
    let codigo = e.codigo.trim().to_string();
    if codigo.is_empty() {
        return Err("El código no puede estar vacío".to_string());
    }
    // "Luces" era el nombre anterior de la categoría de luces de techo.
    let categoria = match e.categoria.trim() {
        "" | "Luces" => CATEGORIA_LUCES,
        c => c,
    };
    Ok(Equipo {
        codigo,
        marca: e.marca.trim().to_string(),
        modelo: e.modelo.trim().to_string(),
        categoria: categoria.to_string(),
        specs: e.specs,
    })
}

/// Un equipo de luz sin especificaciones válidas recibe las típicas de su categoría; cuenta los cambios.
fn ponle_specs(mut e: Equipo, cuenta: &mut usize) -> Equipo {
    let tipo = match e.categoria.as_str() {
        CATEGORIA_LUCES => Some(TipoLuz::PanelLed),
        CATEGORIA_LUCES_PISO => Some(TipoLuz::PanelPie),
        _ => None,
    };
    let Some(tipo) = tipo else { return e };
    if !e.specs.as_ref().is_some_and(|s| s.valido()) {
        e.specs = Some(tipo.specs());
        *cuenta += 1;
    }
    e
}

/// Las categorías base que falten van delante; luego las guardadas y las que usen los equipos.
fn completa_categorias(guardadas: Vec<String>, equipos: &[Equipo]) -> Vec<String> {
    let mut usadas: Vec<String> = Vec::new();
    for c in guardadas.iter().map(|c| c.trim()).filter(|c| !c.is_empty()) {
        // Nombre anterior de la categoría de luces de techo.
        let c = if c == "Luces" { CATEGORIA_LUCES } else { c };
        if !usadas.iter().any(|u| u == c) {
            usadas.push(c.to_string());
        }
    }
    let mut cats: Vec<String> = CATEGORIAS_BASE
        .into_iter()
        .filter(|b| !usadas.iter().any(|u| u == b))
        .map(String::from)
        .collect();
    cats.extend(usadas);
    for e in equipos {
        if !cats.contains(&e.categoria) {
            cats.push(e.categoria.clone());
        }
    }
    cats
}

pub struct Inventario {
    pub equipos: Vec<Equipo>,
    /// En el orden de las pestañas; las base siempre están.
    pub categorias: Vec<String>,
    ruta: PathBuf,
    /// Activo si el archivo existe pero no se pudo leer: nunca se sobrescribe.
    pub solo_lectura: bool,
}

impl Inventario {
    pub fn cargar(ruta: PathBuf) -> (Inventario, Option<String>) {
        let texto = match std::fs::read_to_string(&ruta) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let categorias = completa_categorias(Vec::new(), &[]);
                let inv = Inventario { equipos: Vec::new(), categorias, ruta, solo_lectura: false };
                return (inv, None);
            }
            Err(e) => {
                let msg = format!("No se pudo leer {}: {e}", ruta.display());
                let categorias = completa_categorias(Vec::new(), &[]);
                return (
                    Inventario { equipos: Vec::new(), categorias, ruta, solo_lectura: true },
                    Some(msg),
                );
            }
        };
        // Notepad y PowerShell pueden añadir un BOM UTF-8 que serde_json no acepta.
        let texto = texto.strip_prefix('\u{feff}').unwrap_or(&texto);
        match serde_json::from_str::<Fichero>(texto) {
            Ok(f) => {
                let mut vistos = std::collections::HashSet::new();
                let mut equipos = Vec::new();
                let mut sin_codigo = 0;
                let mut repetidos = Vec::new();
                let mut sin_specs = 0;
                for e in f.equipos {
                    match normaliza(e) {
                        Ok(e) if vistos.insert(e.codigo.clone()) => equipos.push(ponle_specs(e, &mut sin_specs)),
                        Ok(e) => repetidos.push(e.codigo),
                        Err(_) => sin_codigo += 1,
                    }
                }
                let mut avisos = Vec::new();
                if sin_codigo > 0 {
                    avisos.push(format!("Se ignoraron {sin_codigo} equipos sin código"));
                }
                if sin_specs > 0 {
                    avisos.push(format!("Se pusieron especificaciones típicas a {sin_specs} equipos de luz"));
                }
                if !repetidos.is_empty() {
                    avisos.push(format!(
                        "Se ignoraron {} equipos repetidos: {}",
                        repetidos.len(),
                        repetidos.join(", ")
                    ));
                }
                let aviso = if avisos.is_empty() { None } else { Some(avisos.join(". ")) };
                let categorias = completa_categorias(f.categorias, &equipos);
                (Inventario { equipos, categorias, ruta, solo_lectura: false }, aviso)
            }
            Err(e) => {
                let msg = format!("El inventario {} no es válido: {e}", ruta.display());
                let categorias = completa_categorias(Vec::new(), &[]);
                (Inventario { equipos: Vec::new(), categorias, ruta, solo_lectura: true }, Some(msg))
            }
        }
    }

    pub fn guardar(&self) -> Result<(), String> {
        if self.solo_lectura {
            return Err("El inventario no se guarda porque el archivo no se pudo leer".to_string());
        }
        if let Some(dir) = self.ruta.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("No se pudo crear la carpeta {}: {e}", dir.display()))?;
        }
        let fichero = Fichero { equipos: self.equipos.clone(), categorias: self.categorias.clone() };
        let json = serde_json::to_string_pretty(&fichero)
            .map_err(|e| format!("No se pudo preparar el inventario: {e}"))?;
        let mut tmp = self.ruta.clone().into_os_string();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        std::fs::write(&tmp, json)
            .map_err(|e| format!("No se pudo guardar {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &self.ruta).map_err(|e| {
            std::fs::remove_file(&tmp).ok();
            format!("No se pudo guardar {}: {e}", self.ruta.display())
        })
    }

    /// Guarda; si falla, vuelve a las listas de equipos y categorías anteriores.
    fn guardar_o_revertir(&mut self, anterior: (Vec<Equipo>, Vec<String>)) -> Result<(), String> {
        self.guardar().map_err(|e| {
            self.equipos = anterior.0;
            self.categorias = anterior.1;
            e
        })
    }

    pub fn anadir(&mut self, e: Equipo) -> Result<(), String> {
        let e = normaliza(e)?;
        if !self.categorias.contains(&e.categoria) {
            return Err(format!("No existe la categoría {}", e.categoria));
        }
        if self.equipos.iter().any(|x| x.codigo == e.codigo) {
            return Err(format!("Ya existe el equipo {}", e.codigo));
        }
        let anterior = (self.equipos.clone(), self.categorias.clone());
        self.equipos.push(e);
        self.guardar_o_revertir(anterior)
    }

    pub fn editar(&mut self, codigo: &str, e: Equipo) -> Result<(), String> {
        let codigo = codigo.trim();
        let e = normaliza(e)?;
        let i = self
            .equipos
            .iter()
            .position(|x| x.codigo == codigo)
            .ok_or_else(|| format!("No existe el equipo {codigo}"))?;
        if !self.categorias.contains(&e.categoria) {
            return Err(format!("No existe la categoría {}", e.categoria));
        }
        if self.equipos.iter().enumerate().any(|(j, x)| j != i && x.codigo == e.codigo) {
            return Err(format!("Ya existe el equipo {}", e.codigo));
        }
        let anterior = (self.equipos.clone(), self.categorias.clone());
        self.equipos[i] = e;
        self.guardar_o_revertir(anterior)
    }

    pub fn borrar(&mut self, codigo: &str) -> Result<(), String> {
        let codigo = codigo.trim();
        let i = self
            .equipos
            .iter()
            .position(|x| x.codigo == codigo)
            .ok_or_else(|| format!("No existe el equipo {codigo}"))?;
        let anterior = (self.equipos.clone(), self.categorias.clone());
        self.equipos.remove(i);
        self.guardar_o_revertir(anterior)
    }

    /// Nombre sin espacios; no puede repetir otra categoría (sin distinguir mayúsculas).
    pub fn anadir_categoria(&mut self, nombre: &str) -> Result<(), String> {
        let nombre = nombre.trim();
        if nombre.is_empty() {
            return Err("El nombre de la categoría no puede estar vacío".to_string());
        }
        let minusculas = nombre.to_lowercase();
        if self.categorias.iter().any(|c| c.to_lowercase() == minusculas) {
            return Err(format!("Ya existe la categoría {nombre}"));
        }
        let anterior = (self.equipos.clone(), self.categorias.clone());
        self.categorias.push(nombre.to_string());
        self.guardar_o_revertir(anterior)
    }

    /// Solo si está vacía; las base no se borran (al cargar volverían).
    pub fn borrar_categoria(&mut self, nombre: &str) -> Result<(), String> {
        let nombre = nombre.trim();
        let i = self
            .categorias
            .iter()
            .position(|c| c == nombre)
            .ok_or_else(|| format!("No existe la categoría {nombre}"))?;
        if CATEGORIAS_BASE.iter().any(|b| *b == nombre) {
            return Err(format!("La categoría {nombre} es base y no se puede borrar"));
        }
        if self.equipos.iter().any(|e| e.categoria == nombre) {
            return Err(format!("No se puede borrar la categoría {nombre}: tiene equipos"));
        }
        let anterior = (self.equipos.clone(), self.categorias.clone());
        self.categorias.remove(i);
        self.guardar_o_revertir(anterior)
    }

    /// Equipos de una categoría, en el orden de la lista.
    pub fn de_categoria(&self, cat: &str) -> Vec<&Equipo> {
        let cat = cat.trim();
        self.equipos.iter().filter(|e| e.categoria == cat).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modelo::{TipoLuz, CATEGORIA_LUCES_PISO};

    fn equipo(codigo: &str) -> Equipo {
        equipo_en(codigo, CATEGORIA_LUCES)
    }

    fn equipo_en(codigo: &str, categoria: &str) -> Equipo {
        let specs = match categoria {
            CATEGORIA_LUCES => Some(TipoLuz::PanelLed.specs()),
            CATEGORIA_LUCES_PISO => Some(TipoLuz::PanelPie.specs()),
            _ => None,
        };
        Equipo {
            codigo: codigo.to_string(),
            marca: "Aputure".to_string(),
            modelo: "300d".to_string(),
            categoria: categoria.to_string(),
            specs,
        }
    }

    fn ruta_temp(nombre: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("sala_luces_inv_{nombre}_{}.json", std::process::id()))
    }

    #[test]
    fn archivo_inexistente_da_vacio() {
        let ruta = ruta_temp("inexistente");
        std::fs::remove_file(&ruta).ok();
        let (inv, err) = Inventario::cargar(ruta);
        assert!(inv.equipos.is_empty());
        assert!(err.is_none());
        assert!(!inv.solo_lectura);
    }

    #[test]
    fn anadir_guarda_y_recarga() {
        let ruta = ruta_temp("anadir");
        std::fs::remove_file(&ruta).ok();
        let (mut inv, _) = Inventario::cargar(ruta.clone());
        inv.anadir(equipo("E1")).unwrap();
        let (recargado, err) = Inventario::cargar(ruta.clone());
        std::fs::remove_file(&ruta).ok();
        assert!(err.is_none());
        assert_eq!(recargado.equipos, inv.equipos);
    }

    #[test]
    fn rechaza_codigo_vacio_y_duplicado() {
        let ruta = ruta_temp("rechaza");
        std::fs::remove_file(&ruta).ok();
        let (mut inv, _) = Inventario::cargar(ruta.clone());
        assert!(inv.anadir(equipo("")).is_err());
        assert!(inv.anadir(equipo("  ")).is_err());
        inv.anadir(equipo("E1")).unwrap();
        assert!(inv.anadir(equipo("E1")).is_err());
        assert!(inv.anadir(equipo(" E1 ")).is_err());
        std::fs::remove_file(&ruta).ok();
        assert_eq!(inv.equipos.len(), 1);
    }

    #[test]
    fn editar_cambia_datos() {
        let ruta = ruta_temp("editar");
        std::fs::remove_file(&ruta).ok();
        let (mut inv, _) = Inventario::cargar(ruta.clone());
        inv.anadir(equipo("E1")).unwrap();
        inv.anadir(equipo("E2")).unwrap();
        let nuevo = Equipo {
            codigo: "E1".to_string(),
            marca: "Godox".to_string(),
            modelo: "SL60".to_string(),
            categoria: CATEGORIA_LUCES.to_string(),
            specs: Some(TipoLuz::PanelLed.specs()),
        };
        inv.editar("E1", nuevo.clone()).unwrap();
        assert_eq!(inv.equipos[0], nuevo);
        assert!(inv.editar("E1", equipo("E2")).is_err());
        assert_eq!(inv.equipos[0], nuevo);
        let (recargado, _) = Inventario::cargar(ruta.clone());
        std::fs::remove_file(&ruta).ok();
        assert_eq!(recargado.equipos, inv.equipos);
    }

    #[test]
    fn archivo_corrupto_no_se_sobrescribe() {
        let ruta = ruta_temp("corrupto");
        std::fs::write(&ruta, "basura").unwrap();
        let (mut inv, err) = Inventario::cargar(ruta.clone());
        assert!(inv.solo_lectura);
        assert!(err.is_some());
        assert!(inv.anadir(equipo("E1")).is_err());
        assert!(inv.editar("E1", equipo("E1")).is_err());
        assert!(inv.borrar("E1").is_err());
        assert!(inv.guardar().is_err());
        assert!(inv.equipos.is_empty());
        let contenido = std::fs::read_to_string(&ruta).unwrap();
        std::fs::remove_file(&ruta).ok();
        assert_eq!(contenido, "basura");
    }

    #[test]
    fn inventario_con_bom_carga() {
        let ruta = ruta_temp("bom");
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(br#"{"equipos":[{"codigo":"E1","marca":"Aputure","modelo":"300d"}]}"#);
        std::fs::write(&ruta, bytes).unwrap();
        let (inv, aviso) = Inventario::cargar(ruta.clone());
        std::fs::remove_file(&ruta).ok();
        assert!(aviso.is_some());
        // Sin categoría ni especificaciones: es de luces techo y recibe las típicas (con aviso).
        assert!(aviso.is_some());
        assert!(!inv.solo_lectura);
        assert_eq!(inv.equipos, vec![equipo("E1")]);
    }

    #[test]
    fn cargar_limpia_vacios_y_repetidos() {
        let ruta = ruta_temp("limpia");
        let json = r#"{"equipos":[
            {"codigo":"","marca":"Aputure","modelo":"300d"},
            {"codigo":"E1","marca":"Aputure","modelo":"300d"},
            {"codigo":" E1 ","marca":"Godox","modelo":"SL60"}]}"#;
        std::fs::write(&ruta, json).unwrap();
        let (inv, aviso) = Inventario::cargar(ruta.clone());
        std::fs::remove_file(&ruta).ok();
        assert_eq!(inv.equipos, vec![equipo("E1")]);
        assert!(aviso.is_some());
        assert!(!inv.solo_lectura);
    }

    #[test]
    fn solo_lectura_bloquea_editar_y_borrar() {
        let ruta = ruta_temp("solo_lectura");
        std::fs::write(&ruta, "basura").unwrap();
        let mut inv = Inventario {
            equipos: vec![equipo("E1")],
            categorias: vec![CATEGORIA_LUCES.to_string()],
            ruta: ruta.clone(),
            solo_lectura: true,
        };
        let r_editar = inv.editar("E1", equipo("E2"));
        let r_borrar = inv.borrar("E1");
        let contenido = std::fs::read_to_string(&ruta).unwrap();
        std::fs::remove_file(&ruta).ok();
        assert!(r_editar.is_err());
        assert!(r_borrar.is_err());
        assert_eq!(inv.equipos, vec![equipo("E1")]);
        assert_eq!(contenido, "basura");
    }

    #[test]
    fn fallo_al_guardar_revierte_lista() {
        let bloqueo = ruta_temp("bloqueo");
        std::fs::write(&bloqueo, "no es carpeta").unwrap();
        let mut inv = Inventario {
            equipos: vec![equipo("E1")],
            categorias: vec![CATEGORIA_LUCES.to_string()],
            ruta: bloqueo.join("inventario.json"),
            solo_lectura: false,
        };
        let r = inv.anadir(equipo("E2"));
        std::fs::remove_file(&bloqueo).ok();
        assert!(r.is_err());
        assert_eq!(inv.equipos, vec![equipo("E1")]);
    }

    #[test]
    fn equipo_viejo_va_a_luces() {
        let ruta = ruta_temp("viejo");
        std::fs::write(&ruta, r#"{"equipos":[{"codigo":"E1","marca":"Aputure","modelo":"300d"}]}"#).unwrap();
        let (inv, aviso) = Inventario::cargar(ruta.clone());
        std::fs::remove_file(&ruta).ok();
        assert!(aviso.is_some());
        assert_eq!(inv.equipos[0].categoria, CATEGORIA_LUCES);
        assert_eq!(inv.categorias, crate::modelo::CATEGORIAS_BASE);
    }

    #[test]
    fn anadir_y_borrar_categoria() {
        let ruta = ruta_temp("categorias");
        std::fs::remove_file(&ruta).ok();
        let (mut inv, _) = Inventario::cargar(ruta.clone());
        inv.anadir_categoria(" Trípodes ").unwrap();
        let (recargado, _) = Inventario::cargar(ruta.clone());
        assert!(recargado.categorias.contains(&"Trípodes".to_string()));
        assert!(inv.anadir_categoria(" trípodes ").is_err());
        assert!(inv.anadir_categoria("  ").is_err());
        inv.anadir(equipo_en("T1", "Trípodes")).unwrap();
        assert!(inv.borrar_categoria("Trípodes").is_err());
        inv.borrar("T1").unwrap();
        assert!(inv.borrar_categoria(CATEGORIA_LUCES).is_err());
        inv.borrar_categoria("Trípodes").unwrap();
        let (recargado, _) = Inventario::cargar(ruta.clone());
        std::fs::remove_file(&ruta).ok();
        assert_eq!(recargado.categorias, crate::modelo::CATEGORIAS_BASE);
        assert_eq!(recargado.categorias, inv.categorias);
    }

    #[test]
    fn anadir_rechaza_categoria_inexistente() {
        let ruta = ruta_temp("sin_categoria");
        std::fs::remove_file(&ruta).ok();
        let (mut inv, _) = Inventario::cargar(ruta.clone());
        assert!(inv.anadir(equipo_en("T1", "Trípodes")).is_err());
        assert!(inv.equipos.is_empty());
        inv.anadir_categoria("Trípodes").unwrap();
        inv.anadir(equipo_en("T1", "Trípodes")).unwrap();
        assert!(inv.editar("T1", equipo_en("T1", "Fantasma")).is_err());
        assert_eq!(inv.equipos[0].categoria, "Trípodes");
        assert_eq!(inv.de_categoria("Trípodes").len(), 1);
        std::fs::remove_file(&ruta).ok();
    }

    #[test]
    fn categorias_base_nuevas() {
        let ruta = ruta_temp("base_nuevas");
        std::fs::remove_file(&ruta).ok();
        let (mut inv, _) = Inventario::cargar(ruta.clone());
        std::fs::remove_file(&ruta).ok();
        assert_eq!(inv.categorias.len(), 9);
        assert_eq!(inv.categorias, crate::modelo::CATEGORIAS_BASE);
        for c in ["Cámaras y soportes", "Iluminación de suelo", "Escenografía", "Audio y monitores"] {
            assert!(inv.categorias.iter().any(|x| x == c), "{c}");
        }
        assert!(inv.borrar_categoria("Escenografía").is_err());
    }

    #[test]
    fn inventario_luces_viejas_a_luces_techo() {
        let ruta = ruta_temp("luces_viejas");
        let json = r#"{"equipos":[
            {"codigo":"E1","marca":"Aputure","modelo":"300d"},
            {"codigo":"P1","marca":"Godox","modelo":"x","categoria":"Luces piso"}],
            "categorias":["Luces","Trípodes"]}"#;
        std::fs::write(&ruta, json).unwrap();
        let (inv, aviso) = Inventario::cargar(ruta.clone());
        std::fs::remove_file(&ruta).ok();
        assert!(!inv.solo_lectura);
        assert!(aviso.is_some());
        assert_eq!(inv.equipos[0].categoria, CATEGORIA_LUCES);
        assert_eq!(inv.equipos[0].specs, Some(TipoLuz::PanelLed.specs()));
        assert_eq!(inv.equipos[1].specs, Some(TipoLuz::PanelPie.specs()));
        assert!(!inv.categorias.iter().any(|c| c == "Luces"));
        assert!(inv.categorias.iter().any(|c| c == "Trípodes"));
        assert_eq!(inv.categorias.len(), CATEGORIAS_BASE.len() + 1);
    }
}
