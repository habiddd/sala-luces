use crate::modelo::Malla;
use std::path::{Path, PathBuf};

/// Por encima de este número de triángulos el modelo se simplifica al importarlo.
const LIMITE_TRIANGULOS: usize = 20_000;
/// Color de los triángulos sin material (o sin color en el MTL).
const GRIS: [u8; 3] = [200, 200, 200];

const IDENTIDAD: [[f32; 4]; 4] = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

/// Lee un .glb, .gltf u .obj (la extensión no distingue mayúsculas) y lo deja listo para la sala.
pub fn importar(ruta: &Path) -> Result<(Malla, Vec<String>), String> {
    let ext = ruta.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let nombre = ruta
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("modelo")
        .to_string();
    let (mut m, mut avisos) = match ext.as_str() {
        "glb" | "gltf" => leer_gltf(ruta, nombre)?,
        "obj" => leer_obj(ruta, nombre)?,
        _ => return Err(format!("Formato no soportado: {}. Usa .glb, .gltf u .obj", ruta.display())),
    };
    if m.triangulos.is_empty() {
        return Err("El modelo no tiene geometría (ningún triángulo)".to_string());
    }
    normalizar(&mut m);
    // Antes de simplificar: los índices se usan ahí.
    m.valida().map_err(|e| format!("El modelo no es válido: {e}"))?;
    if m.triangulos.len() > LIMITE_TRIANGULOS {
        let antes = m.triangulos.len();
        simplificar(&mut m, LIMITE_TRIANGULOS);
        // La simplificación puede achicar el modelo: se vuelve a apoyar y medir.
        normalizar(&mut m);
        if m.triangulos.is_empty() {
            return Err("La simplificación se llevó toda la geometría; el modelo es demasiado fino".to_string());
        }
        avisos.push(format!("Modelo simplificado de {antes} a {} triángulos", m.triangulos.len()));
    }
    Ok((m, avisos))
}

/// Centra en x/z, apoya en y = 0 y guarda el tamaño real.
fn normalizar(m: &mut Malla) {
    if m.vertices.is_empty() {
        return;
    }
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for v in &m.vertices {
        for k in 0..3 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
    }
    let centro = [(lo[0] + hi[0]) / 2.0, lo[1], (lo[2] + hi[2]) / 2.0];
    for v in &mut m.vertices {
        for k in 0..3 {
            v[k] -= centro[k];
        }
    }
    m.tam = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
}

/// Agrupa vértices en una rejilla que crece hasta que quedan como mucho `max` triángulos.
/// La malla debe estar validada (índices dentro de rango).
fn simplificar(m: &mut Malla, max: usize) {
    if m.triangulos.len() <= max {
        return;
    }
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for v in &m.vertices {
        for k in 0..3 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
    }
    let ext = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]).max(1e-6);
    let celdas = |s: f32| -> Vec<[i64; 3]> {
        m.vertices
            .iter()
            .map(|v| [0, 1, 2].map(|k| ((v[k] - lo[k]) / s).floor() as i64))
            .collect()
    };
    let sobreviven = |c: &[[i64; 3]]| {
        m.triangulos
            .iter()
            .filter(|t| {
                let (a, b, d) = (c[t[0] as usize], c[t[1] as usize], c[t[2] as usize]);
                a != b && b != d && a != d
            })
            .count()
    };
    let mut s = ext / 1024.0;
    let mut claves = celdas(s);
    while sobreviven(&claves) > max {
        s *= 1.25;
        claves = celdas(s);
    }

    let mut indice: std::collections::HashMap<[i64; 3], u32> = std::collections::HashMap::new();
    let mut sumas: Vec<([f64; 3], u32)> = Vec::new();
    let mut celda_de = Vec::with_capacity(m.vertices.len());
    for (v, c) in m.vertices.iter().zip(&claves) {
        let j = *indice.entry(*c).or_insert_with(|| {
            sumas.push(([0.0; 3], 0));
            sumas.len() as u32 - 1
        });
        let (suma, n) = &mut sumas[j as usize];
        for k in 0..3 {
            suma[k] += v[k] as f64;
        }
        *n += 1;
        celda_de.push(j);
    }
    let vertices: Vec<[f32; 3]> = sumas
        .iter()
        .map(|(suma, n)| [0, 1, 2].map(|k| (suma[k] / *n as f64) as f32))
        .collect();
    // Solo quedan los vértices que usan triángulos sobrevivientes (índices reasignados).
    let mut nuevo = vec![u32::MAX; vertices.len()];
    let mut compactos: Vec<[f32; 3]> = Vec::new();
    let mut triangulos = Vec::new();
    let mut colores = Vec::new();
    for (t, col) in m.triangulos.iter().zip(&m.colores) {
        let t = t.map(|i| celda_de[i as usize] as usize);
        if t[0] != t[1] && t[1] != t[2] && t[0] != t[2] {
            triangulos.push(t.map(|c| {
                if nuevo[c] == u32::MAX {
                    nuevo[c] = compactos.len() as u32;
                    compactos.push(vertices[c]);
                }
                nuevo[c]
            }));
            colores.push(*col);
        }
    }
    m.vertices = compactos;
    m.triangulos = triangulos;
    m.colores = colores;
}

fn mul(a: [[f32; 4]; 4], b: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut c = [[0.0f32; 4]; 4];
    for col in 0..4 {
        for fila in 0..4 {
            c[col][fila] = (0..4).map(|k| a[k][fila] * b[col][k]).sum::<f32>();
        }
    }
    c
}

fn aplica(m: &[[f32; 4]; 4], p: [f32; 3]) -> [f32; 3] {
    let q = [p[0], p[1], p[2], 1.0];
    [0, 1, 2].map(|f| (0..4).map(|k| m[k][f] * q[k]).sum::<f32>())
}

/// Color base de un material glTF en 0..255; sin material, gris.
fn color_gltf(mat: gltf::Material) -> [u8; 3] {
    if mat.index().is_none() {
        return GRIS;
    }
    let [r, g, b, _] = mat.pbr_metallic_roughness().base_color_factor();
    [r, g, b].map(|c| (c * 255.0).round().clamp(0.0, 255.0) as u8)
}

fn leer_gltf(ruta: &Path, nombre: String) -> Result<(Malla, Vec<String>), String> {
    // Solo buffers: las imágenes no se usan, y decodificarlas haría fallar un modelo con textura rota.
    let gltf::Gltf { document: doc, blob } =
        gltf::Gltf::open(ruta).map_err(|e| format!("No se pudo leer {}: {e}", ruta.display()))?;
    let buffers = gltf::import_buffers(&doc, ruta.parent(), blob)
        .map_err(|e| format!("No se pudo leer {}: {e}", ruta.display()))?;
    let mut avisos = Vec::new();
    if doc.materials().any(|m| m.pbr_metallic_roughness().base_color_texture().is_some()) {
        avisos.push("Las texturas no se usan: se dibuja el color base".to_string());
    }
    let mut m = Malla { nombre, vertices: Vec::new(), triangulos: Vec::new(), colores: Vec::new(), tam: [0.0; 3] };

    // Con escenas, se recorren sus nodos raíz; sin ellas, los nodos que no son hijos de otro.
    let mut hijos = std::collections::HashSet::new();
    for n in doc.nodes() {
        for c in n.children() {
            hijos.insert(c.index());
        }
    }
    let mut pila: Vec<(gltf::Node, [[f32; 4]; 4])> = if doc.scenes().next().is_some() {
        doc.scenes().flat_map(|s| s.nodes()).map(|n| (n, IDENTIDAD)).collect()
    } else {
        doc.nodes().filter(|n| !hijos.contains(&n.index())).map(|n| (n, IDENTIDAD)).collect()
    };

    let mut visitados = std::collections::HashSet::new();
    let mut no_triangulos = 0usize;
    let mut sin_posicion = 0usize;
    while let Some((nodo, padre)) = pila.pop() {
        if !visitados.insert(nodo.index()) {
            continue;
        }
        let mundo = mul(padre, nodo.transform().matrix());
        for h in nodo.children() {
            pila.push((h, mundo));
        }
        let Some(mesh) = nodo.mesh() else { continue };
        for prim in mesh.primitives() {
            if prim.mode() != gltf::mesh::Mode::Triangles {
                no_triangulos += 1;
                continue;
            }
            let lector = prim.reader(|b| buffers.get(b.index()).map(|d| &d.0[..]));
            let Some(posiciones) = lector.read_positions() else {
                sin_posicion += 1;
                continue;
            };
            let base = m.vertices.len();
            m.vertices.extend(posiciones.map(|p| aplica(&mundo, p)));
            let n = m.vertices.len() - base;
            let indices: Vec<u32> = match lector.read_indices() {
                Some(i) => i.into_u32().collect(),
                None => (0..n as u32).collect(),
            };
            let color = color_gltf(prim.material());
            for t in indices.chunks_exact(3) {
                if t.iter().any(|&i| i as usize >= n) {
                    return Err(format!("El modelo tiene un índice fuera de rango en {}", ruta.display()));
                }
                let b = base as u32;
                m.triangulos.push([t[0] + b, t[1] + b, t[2] + b]);
                m.colores.push(color);
            }
        }
    }
    if m.vertices.len() > u32::MAX as usize {
        return Err("El modelo tiene demasiados vértices".to_string());
    }
    if no_triangulos > 0 {
        avisos.push(format!("{no_triangulos} partes que no son triángulos se ignoran"));
    }
    if sin_posicion > 0 {
        avisos.push(format!("{sin_posicion} partes sin posiciones se ignoran"));
    }
    Ok((m, avisos))
}

fn leer_obj(ruta: &Path, nombre: String) -> Result<(Malla, Vec<String>), String> {
    let opciones = tobj::LoadOptions { triangulate: true, single_index: true, ..Default::default() };
    let (modelos, materiales) =
        tobj::load_obj(ruta, &opciones).map_err(|e| format!("No se pudo leer {}: {e}", ruta.display()))?;
    let mut avisos = Vec::new();
    let mats = match materiales {
        Ok(v) => v,
        Err(e) => {
            avisos.push(format!("No se pudo leer el MTL ({e}): se usa gris"));
            Vec::new()
        }
    };
    if mats.iter().any(|m| m.diffuse_texture.is_some()) {
        avisos.push("Las texturas no se usan: se dibuja el color base".to_string());
    }
    let mut m = Malla { nombre, vertices: Vec::new(), triangulos: Vec::new(), colores: Vec::new(), tam: [0.0; 3] };
    for modelo in &modelos {
        let mesh = &modelo.mesh;
        let base = m.vertices.len();
        m.vertices.extend(mesh.positions.chunks_exact(3).map(|p| [p[0], p[1], p[2]]));
        let n = m.vertices.len() - base;
        let color = mesh
            .material_id
            .and_then(|i| mats.get(i))
            .and_then(|mt| mt.diffuse)
            .map(|d| d.map(|c| (c * 255.0).round().clamp(0.0, 255.0) as u8))
            .unwrap_or(GRIS);
        for t in mesh.indices.chunks_exact(3) {
            if t.iter().any(|&i| i as usize >= n) {
                return Err(format!("El modelo tiene un índice fuera de rango en {}", ruta.display()));
            }
            let b = base as u32;
            m.triangulos.push([t[0] + b, t[1] + b, t[2] + b]);
            m.colores.push(color);
        }
    }
    if m.vertices.len() > u32::MAX as usize {
        return Err("El modelo tiene demasiados vértices".to_string());
    }
    Ok((m, avisos))
}

/// %APPDATA%\SalaLuces\modelos; sin APPDATA, "modelos" en la carpeta actual.
pub fn ruta_biblioteca() -> PathBuf {
    match std::env::var_os("APPDATA") {
        Some(appdata) => PathBuf::from(appdata).join("SalaLuces").join("modelos"),
        None => PathBuf::from("modelos"),
    }
}

/// Nombre de archivo válido en Windows: sin caracteres prohibidos, sin puntos ni espacios al final,
/// y sin nombres reservados (CON, COM1...), que Windows no deja crear ni con extensión.
pub(crate) fn archivo_seguro(nombre: &str) -> String {
    let s: String = nombre
        .chars()
        .map(|c| if c.is_control() || "\\/:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    let s = s.trim().trim_end_matches(['.', ' ']);
    if s.is_empty() {
        return "modelo".to_string();
    }
    let base = s.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reservado = matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (base.len() == 4 && (base.starts_with("COM") || base.starts_with("LPT")) && base[3..].parse::<u8>().is_ok_and(|n| (1..=9).contains(&n)));
    if reservado { format!("_{s}") } else { s.to_string() }
}

/// Escribe `<dir>/<nombre>.json` a un temporal y lo renombra encima, como Montaje::guardar.
pub fn guardar_en_biblioteca(dir: &Path, m: &Malla) -> Result<(), String> {
    m.valida()?;
    std::fs::create_dir_all(dir).map_err(|e| format!("No se pudo crear {}: {e}", dir.display()))?;
    let json = serde_json::to_string(m).map_err(|e| format!("No se pudo preparar el modelo: {e}"))?;
    let ruta = dir.join(format!("{}.json", archivo_seguro(&m.nombre)));
    // Dos nombres distintos pueden caer en el mismo archivo (mayúsculas en Windows, "a:b" y "a_b"): no se pisa otro modelo.
    if let Ok(texto) = std::fs::read_to_string(&ruta) {
        let texto = texto.strip_prefix('\u{feff}').unwrap_or(&texto);
        if let Ok(otro) = serde_json::from_str::<Malla>(texto) {
            if otro.nombre != m.nombre {
                return Err(format!(
                    "Ya hay un modelo guardado como {} con el nombre {}: elige otro nombre",
                    ruta.display(),
                    otro.nombre
                ));
            }
        }
    }
    let mut tmp = ruta.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, json).map_err(|e| format!("No se pudo guardar {}: {e}", ruta.display()))?;
    std::fs::rename(&tmp, &ruta).map_err(|e| {
        std::fs::remove_file(&tmp).ok();
        format!("No se pudo guardar {}: {e}", ruta.display())
    })
}

/// Lee los .json de la biblioteca. Los archivos rotos o repetidos se omiten con aviso.
pub fn cargar_biblioteca(dir: &Path) -> (Vec<Malla>, Vec<String>) {
    let entradas = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (Vec::new(), Vec::new()),
        Err(e) => return (Vec::new(), vec![format!("No se pudo leer la biblioteca {}: {e}", dir.display())]),
    };
    let mut rutas: Vec<PathBuf> = entradas
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("json")) && p.is_file())
        .collect();
    rutas.sort();
    let mut lista: Vec<Malla> = Vec::new();
    let mut avisos = Vec::new();
    for ruta in rutas {
        let archivo = ruta.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
        let leido = std::fs::read_to_string(&ruta)
            .map_err(|e| e.to_string())
            .and_then(|t| {
                // Igual que Montaje::cargar: un BOM al principio no es JSON válido.
                let t = t.strip_prefix('\u{feff}').unwrap_or(&t);
                serde_json::from_str::<Malla>(t).map_err(|e| e.to_string())
            })
            .and_then(|m| m.valida().map(|()| m));
        match leido {
            Ok(m) if lista.iter().any(|x| x.nombre == m.nombre) => {
                avisos.push(format!("{archivo}: ya hay un modelo {} en la biblioteca, omitido", m.nombre));
            }
            Ok(m) => lista.push(m),
            Err(e) => avisos.push(format!("{archivo} no se pudo leer ({e}): omitido")),
        }
    }
    (lista, avisos)
}

/// "Silla" si está libre; si no, "Silla (2)", "Silla (3)"...
pub fn nombre_libre(base: &str, usados: &[String]) -> String {
    // Se compara el archivo que ocuparía (sin mayúsculas): "silla" choca con "Silla".
    let clave = |n: &str| archivo_seguro(n).to_lowercase();
    let ocupado = |c: &str| usados.iter().any(|u| clave(u) == clave(c));
    if !ocupado(base) {
        return base.to_string();
    }
    (2u32..)
        .map(|n| format!("{base} ({n})"))
        .find(|c| !ocupado(c.as_str()))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CUBO_OBJ: &str = "v 10 5 -1
v 12 5 -1
v 12 6 -1
v 10 6 -1
v 10 5 2
v 12 5 2
v 12 6 2
v 10 6 2
f 1 3 2
f 1 4 3
f 5 6 7
f 5 7 8
f 1 2 6
f 1 6 5
f 2 3 7
f 2 7 6
f 3 4 8
f 3 8 7
f 4 1 5
f 4 5 8
";

    fn dir_temp(nombre: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sala_luces_importar_{nombre}_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn escribir(dir: &Path, archivo: &str, contenido: &[u8]) -> PathBuf {
        let ruta = dir.join(archivo);
        std::fs::write(&ruta, contenido).unwrap();
        ruta
    }

    fn cerca(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    fn base64(bytes: &[u8]) -> String {
        const ABC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for c in bytes.chunks(3) {
            let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
            let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
            for i in 0..4 {
                if i <= c.len() {
                    out.push(ABC[((n >> (18 - 6 * i)) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    #[test]
    fn importar_obj_cubo() {
        let dir = dir_temp("cubo");
        let ruta = escribir(&dir, "cubo.obj", CUBO_OBJ.as_bytes());
        let r = importar(&ruta);
        std::fs::remove_dir_all(&dir).ok();
        let (m, avisos) = r.unwrap();
        assert!(avisos.is_empty(), "{avisos:?}");
        assert_eq!(m.nombre, "cubo");
        assert_eq!(m.vertices.len(), 8);
        assert_eq!(m.triangulos.len(), 12);
        assert_eq!(m.colores.len(), 12);
        assert!(m.colores.iter().all(|c| *c == GRIS), "sin material: gris");
        for (a, b) in m.tam.iter().zip([2.0, 1.0, 3.0]) {
            assert!(cerca(*a, b), "tam {:?}", m.tam);
        }
        let miny = m.vertices.iter().map(|v| v[1]).fold(f32::MAX, f32::min);
        assert!(cerca(miny, 0.0), "apoyado en el suelo");
        let minx = m.vertices.iter().map(|v| v[0]).fold(f32::MAX, f32::min);
        let maxz = m.vertices.iter().map(|v| v[2]).fold(f32::MIN, f32::max);
        assert!(cerca(minx, -1.0) && cerca(maxz, 1.5), "centrado en x y z");
        assert!(m.valida().is_ok());
    }

    #[test]
    fn importar_obj_con_mtl_color() {
        let dir = dir_temp("mtl");
        let obj = "mtllib rojo.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nusemtl rojo\nf 1 2 3\n";
        escribir(&dir, "rojo.mtl", b"newmtl rojo\nKd 1 0 0\n");
        let ruta = escribir(&dir, "tri.obj", obj.as_bytes());
        let con_mtl = importar(&ruta);
        // Sin el MTL: aviso y color gris, no error.
        let sin_mtl = escribir(&dir, "solo.obj", b"mtllib no_existe.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nusemtl x\nf 1 2 3\n");
        let sin = importar(&sin_mtl);
        std::fs::remove_dir_all(&dir).ok();
        let (m, avisos) = con_mtl.unwrap();
        assert!(avisos.is_empty(), "{avisos:?}");
        assert_eq!(m.colores, vec![[255, 0, 0]]);
        let (m, avisos) = sin.unwrap();
        assert_eq!(m.colores, vec![GRIS]);
        assert_eq!(avisos.len(), 1, "{avisos:?}");
    }

    /// Un triángulo rojo en .gltf con el buffer en base64. Con `textura`, el material usa una
    /// imagen que no existe en disco.
    fn gltf_triangulo(textura: bool) -> String {
        // Tres vértices (0,0,0) (1,0,0) (0,1,0) en f32 LE, luego índices 0,1,2 en u16 LE: 42 bytes.
        let mut bytes = Vec::new();
        for v in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            for c in v {
                bytes.extend_from_slice(&c.to_le_bytes());
            }
        }
        for i in [0u16, 1, 2] {
            bytes.extend_from_slice(&i.to_le_bytes());
        }
        let (imagenes, texturas, base_color) = if textura {
            (
                r#""images": [{"uri": "no_existe.png"}],"#,
                r#""textures": [{"source": 0}],"#,
                r#""baseColorTexture": {"index": 0},"#,
            )
        } else {
            ("", "", "")
        };
        format!(
            r#"{{
  "asset": {{"version": "2.0"}},
  "scene": 0,
  "scenes": [{{"nodes": [0]}}],
  "nodes": [{{"mesh": 0}}],
  "meshes": [{{"primitives": [{{"attributes": {{"POSITION": 0}}, "indices": 1, "material": 0}}]}}],
  "materials": [{{"pbrMetallicRoughness": {{{base_color} "baseColorFactor": [1.0, 0.0, 0.0, 1.0]}}}}],
  {imagenes}
  {texturas}
  "accessors": [
    {{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0, 0, 0], "max": [1, 1, 0]}},
    {{"bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR"}}
  ],
  "bufferViews": [
    {{"buffer": 0, "byteOffset": 0, "byteLength": 36}},
    {{"buffer": 0, "byteOffset": 36, "byteLength": 6}}
  ],
  "buffers": [{{"byteLength": 42, "uri": "data:application/octet-stream;base64,{}"}}]
}}"#,
            base64(&bytes)
        )
    }

    #[test]
    fn importar_gltf_triangulo() {
        let dir = dir_temp("gltf");
        let ruta = escribir(&dir, "tri.gltf", gltf_triangulo(false).as_bytes());
        let r = importar(&ruta);
        std::fs::remove_dir_all(&dir).ok();
        let (m, _) = r.unwrap();
        assert_eq!(m.triangulos, vec![[0, 1, 2]]);
        assert_eq!(m.colores, vec![[255, 0, 0]]);
        assert_eq!(m.vertices.len(), 3);
    }

    #[test]
    fn importar_gltf_con_textura_ausente() {
        let dir = dir_temp("textura");
        let ruta = escribir(&dir, "tri.gltf", gltf_triangulo(true).as_bytes());
        let r = importar(&ruta);
        std::fs::remove_dir_all(&dir).ok();
        let (m, avisos) = r.unwrap();
        assert_eq!(m.colores, vec![[255, 0, 0]]);
        assert!(avisos.iter().any(|a| a.contains("texturas")), "{avisos:?}");
    }

    #[test]
    fn importar_extension_desconocida_falla() {
        let dir = dir_temp("ext");
        let stl = escribir(&dir, "pieza.stl", b"solid x");
        let sin_ext = escribir(&dir, "pieza", b"x");
        let r1 = importar(&stl);
        let r2 = importar(&sin_ext);
        let r3 = importar(&dir.join("no_existe.glb"));
        std::fs::remove_dir_all(&dir).ok();
        assert!(r1.is_err() && r2.is_err() && r3.is_err());
    }

    #[test]
    fn importar_archivo_roto_falla() {
        let dir = dir_temp("roto");
        let glb = escribir(&dir, "roto.glb", b"glTF\x02\x00\x00\x00basura");
        let gltf = escribir(&dir, "roto.gltf", b"{no json");
        let obj = escribir(&dir, "roto.obj", b"f 1 2 3\nf 9 9 9\n");
        let vacio = escribir(&dir, "vacio.obj", b"# nada\n");
        let r = [importar(&glb), importar(&gltf), importar(&obj), importar(&vacio)];
        std::fs::remove_dir_all(&dir).ok();
        for r in r {
            assert!(r.is_err());
        }
    }

    #[test]
    fn simplificar_respeta_limite() {
        // Rejilla de 100 x 150 cuadrados: 30 000 triángulos.
        let (nx, nz) = (100usize, 150usize);
        let mut vertices = Vec::new();
        for z in 0..=nz {
            for x in 0..=nx {
                vertices.push([x as f32 * 0.01, 0.0, z as f32 * 0.01]);
            }
        }
        let mut triangulos = Vec::new();
        let f = |x: usize, z: usize| (z * (nx + 1) + x) as u32;
        for z in 0..nz {
            for x in 0..nx {
                triangulos.push([f(x, z), f(x + 1, z), f(x + 1, z + 1)]);
                triangulos.push([f(x, z), f(x + 1, z + 1), f(x, z + 1)]);
            }
        }
        let n = triangulos.len();
        let mut m = Malla {
            nombre: "rejilla".to_string(),
            vertices,
            triangulos,
            colores: vec![[1, 2, 3]; n],
            tam: [1.0, 0.0, 1.5],
        };
        assert_eq!(n, 30_000);
        simplificar(&mut m, LIMITE_TRIANGULOS);
        assert!(m.triangulos.len() <= LIMITE_TRIANGULOS, "{}", m.triangulos.len());
        assert!(!m.triangulos.is_empty());
        assert_eq!(m.colores.len(), m.triangulos.len());
        assert!(m.valida().is_ok());

        // Por importar: la misma rejilla en .obj, a 3 m de altura y con relieve. Tras simplificar
        // debe volver a apoyarse en y = 0 y medir lo que queda.
        let mut obj = String::new();
        for z in 0..=nz {
            for x in 0..=nx {
                obj += &format!("v {} {} {}\n", x as f32 * 0.01, 3.0 + x as f32 * 0.001, z as f32 * 0.01);
            }
        }
        for z in 0..nz {
            for x in 0..nx {
                let v = |x: usize, z: usize| f(x, z) + 1;
                obj += &format!(
                    "f {} {} {}\nf {} {} {}\n",
                    v(x, z), v(x + 1, z), v(x + 1, z + 1), v(x, z), v(x + 1, z + 1), v(x, z + 1)
                );
            }
        }
        let dir = dir_temp("simplificar");
        let ruta = escribir(&dir, "rejilla.obj", obj.as_bytes());
        let r = importar(&ruta);
        std::fs::remove_dir_all(&dir).ok();
        let (g, avisos) = r.unwrap();
        assert!(avisos.iter().any(|a| a.contains("simplificado")), "{avisos:?}");
        assert!(!g.triangulos.is_empty() && g.triangulos.len() <= LIMITE_TRIANGULOS);
        assert!(g.valida().is_ok());
        let lo = |k: usize| g.vertices.iter().map(|v| v[k]).fold(f32::MAX, f32::min);
        let hi = |k: usize| g.vertices.iter().map(|v| v[k]).fold(f32::MIN, f32::max);
        assert!(lo(1).abs() < 1e-4, "min y {}", lo(1));
        for k in 0..3 {
            assert!(cerca(hi(k) - lo(k), g.tam[k]), "tam {:?}", g.tam);
        }
        assert!(cerca((lo(0) + hi(0)) / 2.0, 0.0) && cerca((lo(2) + hi(2)) / 2.0, 0.0));
    }

    fn malla_de(nombre: &str) -> Malla {
        Malla {
            nombre: nombre.to_string(),
            vertices: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            triangulos: vec![[0, 1, 2]],
            colores: vec![[1, 2, 3]],
            tam: [1.0, 1.0, 0.0],
        }
    }

    #[test]
    fn nombre_reservado_se_guarda() {
        let dir = dir_temp("reservado");
        let r = guardar_en_biblioteca(&dir, &malla_de("CON"));
        let (lista, avisos) = cargar_biblioteca(&dir);
        std::fs::remove_dir_all(&dir).ok();
        r.unwrap();
        assert_eq!(lista, vec![malla_de("CON")]);
        assert!(avisos.is_empty());
    }

    #[test]
    fn colision_de_archivo_no_sobrescribe() {
        let dir = dir_temp("colision");
        // En Windows "silla" y "Silla" son el mismo archivo; "a:b" y "a_b" también.
        let r1 = guardar_en_biblioteca(&dir, &malla_de("Silla"));
        let r2 = guardar_en_biblioteca(&dir, &malla_de("silla"));
        let r3 = guardar_en_biblioteca(&dir, &malla_de("a:b"));
        let r4 = guardar_en_biblioteca(&dir, &malla_de("a_b"));
        let (lista, _) = cargar_biblioteca(&dir);
        std::fs::remove_dir_all(&dir).ok();
        assert!(r1.is_ok() && r3.is_ok());
        assert!(r2.is_err() && r4.is_err());
        let mut nombres: Vec<&str> = lista.iter().map(|m| m.nombre.as_str()).collect();
        nombres.sort();
        assert_eq!(nombres, ["Silla", "a:b"]);
    }

    #[test]
    fn nombre_libre_ignora_mayusculas() {
        assert_eq!(nombre_libre("silla", &["Silla".to_string()]), "silla (2)");
        assert_eq!(nombre_libre("a:b", &["a_b".to_string()]), "a:b (2)");
    }

    #[test]
    fn biblioteca_guardar_y_cargar() {
        let dir = dir_temp("biblioteca");
        let vacia = dir.join("no_existe");
        let (lista, avisos) = cargar_biblioteca(&vacia);
        assert!(lista.is_empty() && avisos.is_empty());

        let malla = Malla {
            nombre: "Silla: X/1".to_string(),
            vertices: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            triangulos: vec![[0, 1, 2]],
            colores: vec![[9, 8, 7]],
            tam: [1.0, 1.0, 0.0],
        };
        guardar_en_biblioteca(&dir, &malla).unwrap();
        escribir(&dir, "roto.json", b"{no json");
        let (lista, avisos) = cargar_biblioteca(&dir);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(lista, vec![malla]);
        assert_eq!(avisos.len(), 1, "{avisos:?}");
    }

    #[test]
    fn nombre_libre_numera() {
        assert_eq!(nombre_libre("Silla", &[]), "Silla");
        assert_eq!(nombre_libre("Silla", &["Silla".to_string()]), "Silla (2)");
        let usados = ["Silla".to_string(), "Silla (2)".to_string()];
        assert_eq!(nombre_libre("Silla", &usados), "Silla (3)");
    }

    #[test]
    fn ruta_biblioteca_termina_en_modelos() {
        assert!(ruta_biblioteca().ends_with("modelos"));
    }
}
