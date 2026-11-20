use crate::geom::{param_mas_cercano, Camara, V3};
use crate::modelo::{direccion, Acabado, CamaraEscena, Luz, LuzPiso, Malla, Montaje, Objeto, Pared, Sala, Superficie, TipoLuz, TipoObjeto};
use eframe::egui;

#[derive(Clone, Debug, PartialEq)]
pub enum Sel {
    Carril(String),
    Toma(String),
    Luz(String),
    LuzPiso(String),
    Objeto(String),
    Camara(String),
}

pub enum Forma {
    Poligono(Vec<V3>),
    Linea(V3, V3, f32 /*px*/),
    Texto(V3, String),
}

pub struct Prim {
    pub forma: Forma,
    pub color: egui::Color32,
    pub sel: Option<Sel>,
    pub resaltado: bool,
    /// Cuerpo de una luz (cabeza, yugo): se dibuja, pero no proyecta sombra sobre sí misma.
    pub cuerpo_luz: bool,
}

pub const SIN_TOMA: egui::Color32 = egui::Color32::from_rgb(140, 140, 140);
const PARES: [egui::Color32; 8] = [
    egui::Color32::from_rgb(235, 80, 80),
    egui::Color32::from_rgb(240, 150, 50),
    egui::Color32::from_rgb(230, 210, 60),
    egui::Color32::from_rgb(110, 200, 80),
    egui::Color32::from_rgb(60, 190, 190),
    egui::Color32::from_rgb(80, 130, 235),
    egui::Color32::from_rgb(160, 95, 235),
    egui::Color32::from_rgb(235, 95, 185),
];
const REJILLA: egui::Color32 = egui::Color32::from_rgb(95, 95, 105);
const TECHO: egui::Color32 = egui::Color32::from_rgb(150, 150, 160);
const CARRIL: egui::Color32 = egui::Color32::from_rgb(110, 110, 125);
const TEXTO: egui::Color32 = egui::Color32::from_gray(230);
const MADERA: egui::Color32 = egui::Color32::from_rgb(160, 115, 70);
const MESA_COLOR: egui::Color32 = egui::Color32::from_rgb(120, 150, 145);
// Parche "light skin" de la carta ColorChecker: reflectancia ≈ 0.35, como piel humana clara-media.
const MANIQUI_COLOR: egui::Color32 = egui::Color32::from_rgb(194, 150, 130);
const CARTON: egui::Color32 = egui::Color32::from_rgb(170, 160, 100);
const CAMARA_COLOR: egui::Color32 = egui::Color32::from_rgb(95, 105, 120);
const CAMARA_LINEA: egui::Color32 = egui::Color32::from_rgb(120, 200, 255);
const LUZ_COLOR: egui::Color32 = egui::Color32::from_rgb(190, 165, 100);
const ESCENA_COLOR: egui::Color32 = egui::Color32::from_rgb(125, 105, 140);
const AUDIO_COLOR: egui::Color32 = egui::Color32::from_rgb(95, 125, 160);
const LADOS: [f32; 2] = [-1.0, 1.0];
const ESQUINAS: [(f32, f32); 4] = [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)];

pub fn color_par(i: usize) -> egui::Color32 {
    PARES[i % PARES.len()]
}

pub fn centro_luz(m: &Montaje, l: &Luz) -> V3 {
    let x = m.carriles.iter().find(|c| c.id == l.carril).map_or(0.0, |c| c.x);
    V3::new(x, m.sala.altura_carriles - 0.25, l.z)
}

fn push(v: &mut Vec<Prim>, forma: Forma, color: egui::Color32, sel: Option<Sel>) {
    v.push(Prim { forma, color, sel, resaltado: false, cuerpo_luz: false });
}

const EJES: [V3; 3] = [V3 { x: 1.0, y: 0.0, z: 0.0 }, V3 { x: 0.0, y: 1.0, z: 0.0 }, V3 { x: 0.0, y: 0.0, z: 1.0 }];

/// Seis caras de una caja centrada en `c` con medias `h` sobre los ejes `ejes` (ortonormales).
/// Cada cara sale con la normal hacia fuera, para la malla de la GPU.
fn caras_en(c: V3, ejes: [V3; 3], h: [f32; 3]) -> Vec<Vec<V3>> {
    let mut caras = Vec::with_capacity(6);
    for eje in 0..3 {
        let (a, b) = ((eje + 1) % 3, (eje + 2) % 3);
        for s in [-1.0f32, 1.0] {
            let mut cara: Vec<V3> = [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                .map(|(u, v)| c + ejes[eje] * (s * h[eje]) + ejes[a] * (u * h[a]) + ejes[b] * (v * h[b]))
                .to_vec();
            if (cara[1] - cara[0]).cross(cara[2] - cara[0]).dot(ejes[eje] * s) < 0.0 {
                cara.reverse();
            }
            caras.push(cara);
        }
    }
    caras
}

/// Six quads of an axis-aligned box centred at `c` with half sizes `h`.
fn caras_caja(c: V3, h: [f32; 3]) -> Vec<Vec<V3>> {
    caras_en(c, EJES, h)
}

fn caja(v: &mut Vec<Prim>, c: V3, h: [f32; 3], color: egui::Color32, sel: Option<Sel>) {
    for cara in caras_caja(c, h) {
        push(v, Forma::Poligono(cara), color, sel.clone());
    }
}

/// Caja con su eje `d` a lo largo de `dir` (unitario); `h` son las medias en (lado, lado, d).
/// Base (u, v) perpendicular a `dir` de un cuerpo de luz: el cuerpo dibujado y el haz (CPU y shaders) la comparten.
pub fn ejes_luz(dir: V3) -> (V3, V3) {
    let ayuda = if dir.y.abs() < 0.9 { V3::new(0.0, 1.0, 0.0) } else { V3::new(1.0, 0.0, 0.0) };
    let u = dir.cross(ayuda).norm();
    (u, u.cross(dir))
}

fn caja_orientada(v: &mut Vec<Prim>, c: V3, dir: V3, h: [f32; 3], color: egui::Color32, sel: Option<Sel>) {
    let (u, w) = ejes_luz(dir);
    let ejes = [u, w, dir];
    for cara in caras_en(c, ejes, h) {
        push(v, Forma::Poligono(cara), color, sel.clone());
    }
}

/// Marca como cuerpo de luz las primitivas recién añadidas.
fn marcar_cuerpo(prims: &mut [Prim]) {
    for p in prims {
        p.cuerpo_luz = true;
    }
}

/// Punto en coordenadas locales de un objeto (origen en su centro en el suelo) a coordenadas de la sala.
fn en_mundo(o: &Objeto, p: V3) -> V3 {
    let (s, c) = o.rot.to_radians().sin_cos();
    V3::new(o.x + p.x * c + p.z * s, p.y, o.z - p.x * s + p.z * c)
}

/// Tres patas, columna y cabeza del trípode (compartidas por cámara y trípode).
fn patas_tripode() -> Vec<([f32; 3], [f32; 3])> {
    let mut v: Vec<_> = [(-0.36, -0.36), (0.36, -0.36), (0.0, 0.36)]
        .into_iter()
        .map(|(x, z)| ([x, 0.22, z], [0.07, 0.44, 0.07]))
        .collect();
    v.extend([([0.0, 0.5, 0.0], [0.1, 0.2, 0.1]), ([0.0, 0.62, 0.0], [0.3, 0.05, 0.3])]);
    v
}

/// Partes de cada tipo como (centro, tamaño) en fracciones de (ancho, alto, fondo).
fn partes(tipo: TipoObjeto) -> Vec<([f32; 3], [f32; 3])> {
    match tipo {
        TipoObjeto::Silla => {
            let mut v = vec![([0.0, 0.48, 0.0], [1.0, 0.05, 1.0]), ([0.0, 0.75, -0.46], [1.0, 0.5, 0.08])];
            v.extend(ESQUINAS.map(|(x, z)| ([0.42 * x, 0.24, 0.42 * z], [0.08, 0.48, 0.08])));
            v
        }
        TipoObjeto::Mesa => {
            let mut v = vec![([0.0, 0.96, 0.0], [1.0, 0.08, 1.0])];
            v.extend(ESQUINAS.map(|(x, z)| ([0.45 * x, 0.46, 0.43 * z], [0.06, 0.92, 0.08])));
            v
        }
        TipoObjeto::Maniqui => {
            let mut v = vec![([0.0, 0.65, 0.0], [0.7, 0.3, 0.8]), ([0.0, 0.89, 0.0], [0.35, 0.12, 0.6])];
            // La nariz indica hacia dónde mira el maniquí (+Z).
            v.push(([0.0, 0.89, 0.35], [0.08, 0.03, 0.1]));
            for s in LADOS {
                v.push(([0.2 * s, 0.25, 0.0], [0.3, 0.5, 0.5]));
                v.push(([0.44 * s, 0.63, 0.0], [0.12, 0.3, 0.4]));
            }
            v
        }
        TipoObjeto::Caja => vec![([0.0, 0.5, 0.0], [1.0, 1.0, 1.0])],
        TipoObjeto::CamaraTripode => {
            let mut v = patas_tripode();
            v.extend([([0.0, 0.72, 0.0], [0.34, 0.16, 0.3]), ([0.0, 0.72, 0.4], [0.14, 0.14, 0.2])]);
            v
        }
        TipoObjeto::Tripode => {
            let mut v = patas_tripode();
            v.push(([0.0, 0.12, -0.36], [0.72, 0.03, 0.04]));
            v
        }
        TipoObjeto::Slider => vec![
            ([-0.38, 0.02, 0.0], [0.12, 0.04, 0.8]),
            ([0.38, 0.02, 0.0], [0.12, 0.04, 0.8]),
            ([-0.38, 0.31, 0.0], [0.06, 0.56, 0.1]),
            ([0.38, 0.31, 0.0], [0.06, 0.56, 0.1]),
            ([0.0, 0.6, 0.0], [0.96, 0.05, 0.2]),
            ([0.1, 0.68, 0.0], [0.24, 0.12, 0.22]),
            ([0.1, 0.82, 0.0], [0.2, 0.12, 0.2]),
        ],
        TipoObjeto::Grua => vec![
            ([0.0, 0.03, 0.0], [0.5, 0.06, 0.4]),
            ([0.0, 0.5, 0.0], [0.05, 0.9, 0.05]),
            ([0.0, 0.94, 0.0], [0.96, 0.04, 0.06]),
            ([-0.42, 0.86, 0.0], [0.12, 0.12, 0.12]),
            ([0.36, 0.8, 0.0], [0.02, 0.26, 0.02]),
            ([0.36, 0.62, 0.0], [0.2, 0.1, 0.16]),
        ],
        TipoObjeto::Teleprompter => vec![
            ([0.0, 0.02, 0.0], [0.8, 0.04, 0.8]),
            ([0.0, 0.42, 0.0], [0.06, 0.76, 0.06]),
            ([0.0, 0.8, -0.1], [0.9, 0.4, 0.3]),
            ([0.0, 0.8, 0.07], [0.84, 0.36, 0.02]),
        ],
        TipoObjeto::CStand => vec![
            ([0.0, 0.03, -0.2], [0.9, 0.04, 0.08]),
            ([-0.3, 0.03, 0.0], [0.08, 0.04, 0.5]),
            ([0.3, 0.03, 0.0], [0.08, 0.04, 0.5]),
            ([0.0, 0.5, 0.0], [0.04, 0.96, 0.04]),
            ([0.0, 0.85, 0.2], [0.04, 0.03, 0.4]),
            ([0.0, 0.85, 0.4], [0.16, 0.14, 0.14]),
        ],
        TipoObjeto::SoporteLuz => vec![
            ([0.0, 0.01, 0.0], [0.5, 0.02, 0.5]),
            ([0.0, 0.5, 0.0], [0.04, 0.96, 0.04]),
            ([0.0, 0.88, 0.0], [0.12, 0.04, 0.12]),
            ([0.0, 0.88, 0.1], [0.2, 0.2, 0.16]),
        ],
        TipoObjeto::SoftboxPie => vec![
            ([0.0, 0.01, 0.0], [0.5, 0.02, 0.5]),
            ([0.0, 0.2, 0.0], [0.04, 0.4, 0.04]),
            ([0.0, 0.4, 0.0], [0.1, 0.04, 0.1]),
            ([0.0, 0.68, 0.05], [0.8, 0.56, 0.3]),
            ([0.0, 0.68, 0.21], [0.72, 0.48, 0.02]),
        ],
        TipoObjeto::Paraguas => vec![
            ([0.0, 0.01, 0.0], [0.5, 0.02, 0.5]),
            ([0.0, 0.4, 0.0], [0.04, 0.76, 0.04]),
            ([0.0, 0.8, 0.0], [0.9, 0.06, 0.9]),
            ([0.0, 0.86, 0.0], [0.6, 0.06, 0.6]),
            ([0.0, 0.7, 0.0], [0.12, 0.12, 0.12]),
        ],
        TipoObjeto::Reflector => vec![
            ([0.0, 0.62, 0.0], [0.84, 0.62, 0.02]),
            ([0.0, 0.95, 0.0], [0.9, 0.04, 0.04]),
            ([0.0, 0.29, 0.0], [0.9, 0.04, 0.04]),
            ([-0.43, 0.62, 0.0], [0.04, 0.62, 0.04]),
            ([0.43, 0.62, 0.0], [0.04, 0.62, 0.04]),
            ([0.0, 0.01, -0.06], [0.5, 0.02, 0.4]),
            ([0.0, 0.14, -0.06], [0.04, 0.28, 0.04]),
        ],
        TipoObjeto::Bandera => vec![
            ([0.0, 0.01, 0.0], [0.5, 0.02, 0.5]),
            ([0.0, 0.5, -0.1], [0.04, 0.98, 0.04]),
            ([0.0, 0.62, 0.0], [0.9, 0.7, 0.05]),
            ([0.0, 0.62, -0.06], [0.1, 0.04, 0.08]),
        ],
        TipoObjeto::PanelLed => vec![
            ([0.0, 0.01, 0.0], [0.6, 0.02, 0.6]),
            ([0.0, 0.16, 0.0], [0.05, 0.28, 0.05]),
            ([0.0, 0.31, 0.0], [0.2, 0.03, 0.12]),
            ([0.0, 0.62, 0.02], [0.84, 0.62, 0.06]),
        ],
        TipoObjeto::Fondo => vec![
            ([0.0, 0.5, 0.0], [0.9, 0.92, 0.03]),
            ([0.0, 0.97, -0.15], [0.96, 0.03, 0.06]),
            ([-0.44, 0.48, -0.15], [0.03, 0.96, 0.03]),
            ([0.44, 0.48, -0.15], [0.03, 0.96, 0.03]),
            ([-0.44, 0.02, -0.15], [0.05, 0.04, 0.5]),
            ([0.44, 0.02, -0.15], [0.05, 0.04, 0.5]),
        ],
        TipoObjeto::Ciclorama => vec![
            ([0.0, 0.02, 0.1], [0.96, 0.04, 0.8]),
            ([0.0, 0.06, -0.32], [0.96, 0.12, 0.36]),
            ([0.0, 0.2, -0.43], [0.96, 0.4, 0.14]),
            ([0.0, 0.5, -0.47], [0.96, 0.94, 0.06]),
        ],
        TipoObjeto::Sofa => vec![
            ([0.0, 0.22, 0.05], [0.96, 0.3, 0.84]),
            ([0.0, 0.66, -0.4], [0.96, 0.6, 0.2]),
            ([-0.42, 0.36, 0.0], [0.16, 0.56, 0.9]),
            ([0.42, 0.36, 0.0], [0.16, 0.56, 0.9]),
            ([0.0, 0.42, 0.1], [0.66, 0.1, 0.72]),
            ([-0.42, 0.04, -0.36], [0.08, 0.08, 0.08]),
            ([0.42, 0.04, -0.36], [0.08, 0.08, 0.08]),
            ([-0.42, 0.04, 0.36], [0.08, 0.08, 0.08]),
            ([0.42, 0.04, 0.36], [0.08, 0.08, 0.08]),
        ],
        TipoObjeto::Escritorio => vec![
            ([0.0, 0.96, 0.0], [0.98, 0.06, 0.98]),
            ([-0.42, 0.47, 0.0], [0.12, 0.94, 0.94]),
            ([0.42, 0.47, 0.0], [0.12, 0.94, 0.94]),
            ([0.0, 0.04, 0.0], [0.84, 0.06, 0.9]),
        ],
        TipoObjeto::Taburete => vec![
            ([0.0, 0.94, 0.0], [0.98, 0.06, 0.98]),
            ([-0.36, 0.46, -0.36], [0.07, 0.92, 0.07]),
            ([0.36, 0.46, -0.36], [0.07, 0.92, 0.07]),
            ([-0.36, 0.46, 0.36], [0.07, 0.92, 0.07]),
            ([0.36, 0.46, 0.36], [0.07, 0.92, 0.07]),
            ([0.0, 0.3, -0.36], [0.8, 0.04, 0.04]),
            ([0.0, 0.3, 0.36], [0.8, 0.04, 0.04]),
            ([-0.36, 0.3, 0.0], [0.04, 0.04, 0.8]),
            ([0.36, 0.3, 0.0], [0.04, 0.04, 0.8]),
        ],
        TipoObjeto::Planta => vec![
            ([0.0, 0.1, 0.0], [0.6, 0.2, 0.6]),
            ([0.0, 0.4, 0.0], [0.08, 0.4, 0.08]),
            ([0.0, 0.62, 0.0], [0.8, 0.36, 0.8]),
            ([0.0, 0.88, 0.0], [0.5, 0.24, 0.5]),
            ([-0.28, 0.7, 0.0], [0.22, 0.22, 0.3]),
            ([0.28, 0.7, 0.0], [0.22, 0.22, 0.3]),
        ],
        TipoObjeto::Estanteria => vec![
            ([-0.47, 0.5, 0.0], [0.06, 0.98, 0.98]),
            ([0.47, 0.5, 0.0], [0.06, 0.98, 0.98]),
            ([0.0, 0.01, 0.0], [0.94, 0.02, 0.98]),
            ([0.0, 0.34, 0.0], [0.94, 0.02, 0.98]),
            ([0.0, 0.67, 0.0], [0.94, 0.02, 0.98]),
            ([0.0, 0.99, 0.0], [0.94, 0.02, 0.98]),
            ([0.0, 0.5, -0.47], [0.94, 0.98, 0.04]),
            ([-0.2, 0.45, 0.05], [0.4, 0.2, 0.6]),
        ],
        TipoObjeto::MicrofonoPie => vec![
            ([0.0, 0.01, 0.0], [0.8, 0.02, 0.8]),
            ([0.0, 0.42, 0.0], [0.04, 0.84, 0.04]),
            ([0.0, 0.9, 0.1], [0.12, 0.14, 0.14]),
        ],
        TipoObjeto::Boom => vec![
            ([0.0, 0.01, 0.0], [0.8, 0.02, 0.8]),
            ([0.0, 0.44, 0.0], [0.04, 0.88, 0.04]),
            ([0.0, 0.9, 0.25], [0.04, 0.04, 0.5]),
            ([0.0, 0.9, 0.42], [0.1, 0.1, 0.14]),
        ],
        TipoObjeto::MonitorSoporte => vec![
            ([0.0, 0.01, 0.0], [0.6, 0.02, 0.5]),
            ([0.0, 0.34, -0.1], [0.06, 0.68, 0.06]),
            ([0.0, 0.68, -0.1], [0.2, 0.04, 0.1]),
            ([0.0, 0.82, -0.1], [0.84, 0.32, 0.08]),
        ],
        TipoObjeto::TvSoporte => vec![
            ([0.0, 0.01, 0.0], [0.6, 0.02, 0.5]),
            ([0.0, 0.25, -0.1], [0.06, 0.46, 0.06]),
            ([0.0, 0.48, -0.1], [0.3, 0.04, 0.1]),
            ([0.0, 0.72, -0.1], [0.9, 0.5, 0.06]),
        ],
        TipoObjeto::Altavoz => vec![
            ([0.0, 0.01, 0.0], [0.6, 0.02, 0.6]),
            ([0.0, 0.24, 0.0], [0.1, 0.46, 0.1]),
            ([0.0, 0.74, 0.0], [0.9, 0.5, 0.9]),
            ([0.0, 0.7, 0.46], [0.7, 0.3, 0.02]),
            ([0.0, 0.9, 0.46], [0.2, 0.08, 0.02]),
        ],
    }
}

fn color_objeto(tipo: TipoObjeto) -> egui::Color32 {
    match tipo {
        TipoObjeto::Silla => MADERA,
        TipoObjeto::Mesa => MESA_COLOR,
        TipoObjeto::Maniqui => MANIQUI_COLOR,
        TipoObjeto::Caja => CARTON,
        _ => match tipo.grupo() {
            "Cámaras y soportes" => CAMARA_COLOR,
            "Iluminación de suelo" => LUZ_COLOR,
            "Escenografía" => ESCENA_COLOR,
            _ => AUDIO_COLOR,
        },
    }
}

/// Caja rotada con el objeto; cada cara es un polígono con la misma selección.
fn parte_objeto(v: &mut Vec<Prim>, o: &Objeto, (c, s): ([f32; 3], [f32; 3]), color: egui::Color32, sel: Option<Sel>) {
    let centro = V3::new(c[0] * o.ancho, c[1] * o.alto, c[2] * o.fondo);
    let media = [s[0] * o.ancho / 2.0, s[1] * o.alto / 2.0, s[2] * o.fondo / 2.0];
    for cara in caras_caja(centro, media) {
        let cara = cara.into_iter().map(|p| en_mundo(o, p)).collect();
        push(v, Forma::Poligono(cara), color, sel.clone());
    }
}

/// Triángulos de un modelo importado, escalados a las medidas del objeto y rotados con él.
fn parte_malla(v: &mut Vec<Prim>, o: &Objeto, m: &Malla, sel: Option<Sel>) {
    // Un tamaño casi nulo no se escala (evita dividir entre cero).
    let escala = |medida: f32, objetivo: f32| if medida > 1e-6 { objetivo / medida } else { 1.0 };
    let [tx, ty, tz] = m.tam;
    let (ex, ey, ez) = (escala(tx, o.ancho), escala(ty, o.alto), escala(tz, o.fondo));
    let pts: Vec<V3> = m
        .vertices
        .iter()
        .map(|p| en_mundo(o, V3::new(p[0] * ex, p[1] * ey, p[2] * ez)))
        .collect();
    for (t, color) in m.triangulos.iter().zip(&m.colores) {
        let Some(tri) = t.iter().map(|&i| pts.get(i as usize).copied()).collect::<Option<Vec<V3>>>() else {
            continue;
        };
        let [r, g, b] = *color;
        push(v, Forma::Poligono(tri), egui::Color32::from_rgb(r, g, b), sel.clone());
    }
}

/// Partes de una forma del catálogo, rotadas con el objeto.
fn partes_de(v: &mut Vec<Prim>, o: &Objeto, tipo: TipoObjeto, sel: Option<Sel>) {
    let color = if tipo == TipoObjeto::Maniqui {
        o.piel.map_or(MANIQUI_COLOR, |[r, g, b]| egui::Color32::from_rgb(r, g, b))
    } else {
        color_objeto(tipo)
    };
    for parte in partes(tipo) {
        parte_objeto(v, o, parte, color, sel.clone());
    }
}

/// Hacia dentro de la sala y medias dimensiones de la toma (fina contra la pared).
fn normal_pared(p: Pared) -> (V3, [f32; 3]) {
    match p {
        Pared::Sur => (V3::new(0.0, 0.0, 1.0), [0.06, 0.06, 0.02]),
        Pared::Norte => (V3::new(0.0, 0.0, -1.0), [0.06, 0.06, 0.02]),
        Pared::Oeste => (V3::new(1.0, 0.0, 0.0), [0.02, 0.06, 0.06]),
        Pared::Este => (V3::new(-1.0, 0.0, 0.0), [0.02, 0.06, 0.06]),
    }
}

/// Color de cada luz, las de techo y luego las de piso (None si no tiene toma).
/// Los pares se numeran solo entre las conectadas.
fn colores_pares(m: &Montaje) -> Vec<Option<egui::Color32>> {
    let mut n = 0;
    m.luces
        .iter()
        .map(|l| &l.toma)
        .chain(m.luces_piso.iter().map(|l| &l.toma))
        .map(|toma| {
            let conectada = toma.as_ref().is_some_and(|t| m.tomas.iter().any(|x| &x.codigo == t));
            conectada.then(|| {
                n += 1;
                color_par(n - 1)
            })
        })
        .collect()
}

/// Cerramiento de la sala como cuatro vértices por cara: suelo, paredes (Sur, Norte, Oeste, Este) y techo.
fn cerramiento(s: &Sala) -> [[V3; 4]; 6] {
    let (w, l, ht) = (s.ancho, s.largo, s.alto_techo);
    [
        [V3::new(0.0, 0.0, 0.0), V3::new(w, 0.0, 0.0), V3::new(w, 0.0, l), V3::new(0.0, 0.0, l)],
        [V3::new(0.0, 0.0, 0.0), V3::new(w, 0.0, 0.0), V3::new(w, ht, 0.0), V3::new(0.0, ht, 0.0)],
        [V3::new(0.0, 0.0, l), V3::new(w, 0.0, l), V3::new(w, ht, l), V3::new(0.0, ht, l)],
        [V3::new(0.0, 0.0, 0.0), V3::new(0.0, 0.0, l), V3::new(0.0, ht, l), V3::new(0.0, ht, 0.0)],
        [V3::new(w, 0.0, 0.0), V3::new(w, 0.0, l), V3::new(w, ht, l), V3::new(w, ht, 0.0)],
        [V3::new(0.0, ht, 0.0), V3::new(w, ht, 0.0), V3::new(w, ht, l), V3::new(0.0, ht, l)],
    ]
}

/// Parte de la cabeza de una luz de piso según su tipo.
fn color_piso(t: TipoLuz) -> egui::Color32 {
    match t {
        TipoLuz::MonoluzSoftbox => egui::Color32::from_rgb(225, 225, 230),
        TipoLuz::PanelPie => egui::Color32::from_rgb(190, 205, 225),
        TipoLuz::FresnelPie => egui::Color32::from_rgb(235, 190, 120),
        TipoLuz::TuboLed => egui::Color32::from_rgb(170, 230, 200),
        _ => egui::Color32::from_rgb(200, 170, 230),
    }
}

/// Pie de una luz de piso: tres patas en el suelo, columna hasta 25 cm bajo la luz y cabeza.
/// Las patas y la columna dan sombra; la cabeza (soporte y luz) va marcada como cuerpo.
fn luz_piso(v: &mut Vec<Prim>, l: &LuzPiso, color: egui::Color32) {
    let sel = Some(Sel::LuzPiso(l.codigo.clone()));
    for (dx, dz) in [(-0.36, -0.36), (0.36, -0.36), (0.0, 0.36)] {
        caja(v, V3::new(l.x + dx, 0.22, l.z + dz), [0.035, 0.22, 0.035], LUZ_COLOR, sel.clone());
    }
    let tope = (l.altura - 0.25).max(0.0);
    caja(v, V3::new(l.x, tope / 2.0, l.z), [0.02, tope / 2.0, 0.02], LUZ_COLOR, sel.clone());

    let n = v.len();
    caja(v, V3::new(l.x, l.altura - 0.125, l.z), [0.025, 0.125, 0.025], LUZ_COLOR, sel.clone());
    let dir = V3::from_arr(direccion(l.ajustes.giro, l.ajustes.inclinacion));
    caja_orientada(v, V3::new(l.x, l.altura, l.z), dir, [0.15, 0.15, 0.1], color, sel.clone());
    marcar_cuerpo(&mut v[n..]);
    push(v, Forma::Texto(V3::new(l.x, l.altura + 0.25, l.z), l.codigo.clone()), TEXTO, sel);
}

/// Las 12 aristas de una caja centrada en `c` con medias `h` sobre los ejes `ejes`.
fn aristas_caja(c: V3, ejes: [V3; 3], h: [f32; 3]) -> Vec<(V3, V3)> {
    let esquina = |i: usize| (0..3).fold(c, |p, k| p + ejes[k] * (if (i >> k) & 1 == 1 { h[k] } else { -h[k] }));
    let mut v = Vec::with_capacity(12);
    for i in 0..8usize {
        for k in 0..3usize {
            if (i >> k) & 1 == 0 {
                v.push((esquina(i), esquina(i | 1 << k)));
            }
        }
    }
    v
}

/// Cámara de escena como alambre, como en Blender: cuerpo, pirámide del campo de visión a 0.5 m
/// y un triángulo sobre la base que señala «arriba». Solo líneas: no entra en las mallas ni da sombra.
fn camara_alambre(v: &mut Vec<Prim>, c: &CamaraEscena) {
    let pos = V3::new(c.x, c.altura, c.z);
    let cam = Camara::desde_escena(pos, V3::from_arr(c.direccion()), c.fov_y());
    let (dir, derecha, arriba) = cam.base();
    let mut lineas = aristas_caja(pos, [derecha, arriba, dir], [0.08, 0.05, 0.05]);
    // tan(fov_h/2) = sensor / (2·focal): media anchura de la base a 0.5 m.
    let hw = 0.5 * c.sensor_ancho_mm / (2.0 * c.focal_mm);
    let hh = hw / c.aspecto;
    let base = pos + dir * 0.5;
    let esquina = |sx: f32, sy: f32| base + derecha * (sx * hw) + arriba * (sy * hh);
    let [a, b, d, e] = [esquina(-1.0, -1.0), esquina(1.0, -1.0), esquina(1.0, 1.0), esquina(-1.0, 1.0)];
    lineas.extend([(pos, a), (pos, b), (pos, d), (pos, e), (a, b), (b, d), (d, e), (e, a)]);
    let (t1, t2, punta) = (esquina(-0.25, 1.0), esquina(0.25, 1.0), base + arriba * (hh * 1.3));
    lineas.extend([(t1, t2), (t2, punta), (punta, t1)]);
    let sel = Some(Sel::Camara(c.nombre.clone()));
    for (p, q) in lineas {
        push(v, Forma::Linea(p, q, 1.5), CAMARA_LINEA, sel.clone());
    }
    push(v, Forma::Texto(pos + V3::new(0.0, 0.2, 0.0), c.nombre.clone()), TEXTO, sel);
}

pub fn construir(m: &Montaje, seleccion: Option<&Sel>) -> Vec<Prim> {
    let s = &m.sala;
    let (w, l, h) = (s.ancho, s.largo, s.altura_carriles);
    let mut v = Vec::new();
    let cerr = cerramiento(s);

    let [r, g, b] = s.acabado_de(Superficie::Suelo).color;
    push(&mut v, Forma::Poligono(cerr[0].to_vec()), egui::Color32::from_rgb(r, g, b), None);
    for i in 0..=(w as usize) {
        let x = i as f32;
        push(&mut v, Forma::Linea(V3::new(x, 0.0, 0.0), V3::new(x, 0.0, l), 1.0), REJILLA, None);
    }
    for i in 0..=(l as usize) {
        let z = i as f32;
        push(&mut v, Forma::Linea(V3::new(0.0, 0.0, z), V3::new(w, 0.0, z), 1.0), REJILLA, None);
    }

    // Mismo orden que `cerramiento`: [suelo, Sur, Norte, Oeste, Este, techo].
    for (p, cara) in [Pared::Sur, Pared::Norte, Pared::Oeste, Pared::Este].into_iter().zip(&cerr[1..5]) {
        let [r, g, b] = s.acabado(p).color;
        push(&mut v, Forma::Poligono(cara.to_vec()), egui::Color32::from_rgba_unmultiplied(r, g, b, 30), None);
    }

    for i in 0..4 {
        push(&mut v, Forma::Linea(cerr[5][i], cerr[5][(i + 1) % 4], 1.0), TECHO, None);
    }

    for c in &m.carriles {
        let sel = Some(Sel::Carril(c.id.clone()));
        caja(&mut v, V3::new(c.x, h, l / 2.0), [0.03, 0.03, l / 2.0], CARRIL, sel.clone());
        push(&mut v, Forma::Texto(V3::new(c.x, h + 0.15, l / 2.0), c.id.clone()), TEXTO, sel);
    }

    let colores = colores_pares(m);
    for (luz, color) in m.luces.iter().zip(&colores) {
        let color = color.unwrap_or(SIN_TOMA);
        let toma = luz.toma.as_ref().and_then(|t| m.tomas.iter().find(|x| &x.codigo == t));
        let sel = Some(Sel::Luz(luz.codigo.clone()));
        let c = centro_luz(m, luz);
        let dir = V3::from_arr(direccion(luz.ajustes.giro, luz.ajustes.inclinacion));
        let n = v.len();
        caja_orientada(&mut v, c, dir, luz.specs.tipo.medio_cuerpo(), color, sel.clone());
        // Las difusas (Kino Flo) no tienen lente: el panel es todo el cuerpo.
        if !luz.specs.tipo.es_difusa() {
            caja_orientada(&mut v, c + dir * 0.19, dir, [0.07, 0.07, 0.04], color, sel.clone());
        }
        marcar_cuerpo(&mut v[n..]);
        push(&mut v, Forma::Texto(V3::new(c.x, c.y + 0.25, c.z), luz.codigo.clone()), TEXTO, sel.clone());
        if let Some(t) = toma {
            push(&mut v, Forma::Linea(c, V3::from_arr(m.punto_toma(t)), 2.0), color, sel);
        }
    }
    for (luz, color) in m.luces_piso.iter().zip(&colores[m.luces.len()..]) {
        let color = color.unwrap_or(SIN_TOMA);
        luz_piso(&mut v, luz, color_piso(luz.specs.tipo));
        if let Some(t) = luz.toma.as_ref().and_then(|t| m.tomas.iter().find(|x| &x.codigo == t)) {
            let head = V3::new(luz.x, luz.altura, luz.z);
            push(&mut v, Forma::Linea(head, V3::from_arr(m.punto_toma(t)), 2.0), color, Some(Sel::LuzPiso(luz.codigo.clone())));
        }
    }

    for t in &m.tomas {
        let color = m
            .luces
            .iter()
            .map(|l| &l.toma)
            .chain(m.luces_piso.iter().map(|l| &l.toma))
            .zip(&colores)
            .find(|(toma, _)| toma.as_deref() == Some(t.codigo.as_str()))
            .and_then(|(_, c)| *c)
            .unwrap_or(SIN_TOMA);
        let sel = Some(Sel::Toma(t.codigo.clone()));
        let (n, hs) = normal_pared(t.pared);
        let p = V3::from_arr(m.punto_toma(t));
        caja(&mut v, p + n * 0.02, hs, color, sel.clone());
        push(&mut v, Forma::Texto(p + n * 0.1 + V3::new(0.0, 0.15, 0.0), t.codigo.clone()), TEXTO, sel);
    }

    for o in &m.objetos {
        let sel = Some(Sel::Objeto(o.nombre.clone()));
        match o.malla.as_deref() {
            Some(nombre) => match m.modelos.get(nombre) {
                Some(malla) => parte_malla(&mut v, o, malla, sel.clone()),
                // Modelo que falta en el montaje: se dibuja como caja.
                None => partes_de(&mut v, o, TipoObjeto::Caja, sel.clone()),
            },
            None => partes_de(&mut v, o, o.tipo, sel.clone()),
        }
        push(&mut v, Forma::Texto(V3::new(o.x, o.alto + 0.15, o.z), o.nombre.clone()), TEXTO, sel);
    }

    for c in &m.camaras {
        camara_alambre(&mut v, c);
    }

    if let Some(sel) = seleccion {
        for c in cotas(m, sel) {
            if c.texto.is_empty() {
                // Guía de un carril: línea fina sin marcas ni texto.
                push(&mut v, Forma::Linea(c.a, c.b, 1.0), COLOR_COTA, None);
                continue;
            }
            let marca = V3::new(0.0, 0.08, 0.0);
            push(&mut v, Forma::Linea(c.a, c.b, 1.5), COLOR_COTA, None);
            push(&mut v, Forma::Linea(c.a - marca, c.a + marca, 1.0), COLOR_COTA, None);
            push(&mut v, Forma::Linea(c.b - marca, c.b + marca, 1.0), COLOR_COTA, None);
            push(&mut v, Forma::Texto((c.a + c.b) * 0.5, c.texto), COLOR_COTA, None);
        }
    }

    for p in &mut v {
        p.resaltado = p.sel.is_some() && p.sel.as_ref() == seleccion;
    }
    v
}

/// Polígonos sólidos de luces, objetos y carriles (sin sala, textos ni líneas), con su color.
/// Sin `cuerpos` se quitan los cuerpos de las luces: no proyectan sombra sobre sí mismas.
fn poligonos_equipos(m: &Montaje, cuerpos: bool) -> Vec<(Vec<V3>, egui::Color32, Option<Sel>)> {
    construir(m, None)
        .into_iter()
        .filter(|p| p.sel.is_some() && (cuerpos || !p.cuerpo_luz))
        .filter_map(|p| match p.forma {
            Forma::Poligono(vs) => Some((vs, p.color, p.sel)),
            _ => None,
        })
        .collect()
}

/// Abanico de triángulos de un polígono convexo (ninguno si tiene menos de tres vértices).
fn triangulos_de(vs: &[V3]) -> impl Iterator<Item = [V3; 3]> + '_ {
    (1..vs.len().saturating_sub(1)).map(move |i| [vs[0], vs[i], vs[i + 1]])
}

/// Vértice de la malla para la GPU: posición, normal unitaria y albedo en lineal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VerticeGpu {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub albedo: [f32; 3],
    /// Marca de material: 0.0 normal; 1.0 carcasa de la sala (suelo, paredes, techo), que la GPU recorta desde fuera; 2.0 piel del maniquí.
    pub carcasa: f32,
}

/// Añade un triángulo; los degenerados se omiten. Con `dentro`, la normal apunta hacia ese punto.
fn triangulo_gpu(out: &mut Vec<VerticeGpu>, t: [V3; 3], albedo: [f32; 3], dentro: Option<V3>, carcasa: f32) {
    let cruz = (t[1] - t[0]).cross(t[2] - t[0]);
    if cruz.len() < 1e-9 {
        return;
    }
    let mut n = cruz.norm();
    if dentro.is_some_and(|c| (c - t[0]).dot(n) < 0.0) {
        n = -n;
    }
    let normal = [n.x, n.y, n.z];
    for p in t {
        out.push(VerticeGpu { pos: [p.x, p.y, p.z], normal, albedo, carcasa });
    }
}

/// Color sRGB a lineal, para el albedo de la GPU.
fn lineal(c: egui::Color32) -> [f32; 3] {
    let [r, g, b, _] = egui::Rgba::from(c).to_array();
    [r, g, b]
}

fn luminancia(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// Albedo GPU de una pared: el tinte del color, con la reflexión como brillo.
pub fn albedo(a: Acabado) -> [f32; 3] {
    let [r, g, b] = a.color;
    let c = lineal(egui::Color32::from_rgb(r, g, b));
    let lum = luminancia(c);
    if lum < 1e-4 {
        return [a.reflexion; 3];
    }
    let k = a.reflexion / lum;
    c.map(|x| x * k)
}

/// Reflexión (luminancia lineal) de un color sRGB; el panel la muestra para la piel.
pub fn reflexion_de_color([r, g, b]: [u8; 3]) -> f32 {
    luminancia(lineal(egui::Color32::from_rgb(r, g, b)))
}

/// Malla de la escena para la GPU: sala (gris con su reflectancia), luces, objetos y carriles.
pub fn malla_gpu(m: &Montaje) -> Vec<VerticeGpu> {
    let s = &m.sala;
    let dentro = V3::new(s.ancho / 2.0, s.alto_techo / 2.0, s.largo / 2.0);
    let albedos = [
        albedo(s.acabado_de(Superficie::Suelo)),
        albedo(s.acabado(Pared::Sur)),
        albedo(s.acabado(Pared::Norte)),
        albedo(s.acabado(Pared::Oeste)),
        albedo(s.acabado(Pared::Este)),
        albedo(s.acabado_de(Superficie::Techo)),
    ];
    let mut v = Vec::new();
    for (cara, a) in cerramiento(s).iter().zip(albedos) {
        for t in triangulos_de(cara) {
            triangulo_gpu(&mut v, t, a, Some(dentro), 1.0);
        }
    }
    // Maniquíes sin modelo importado: su piel va con marca 2 (subsuperficie en el shader).
    let pieles: Vec<&str> = m
        .objetos
        .iter()
        .filter(|o| o.tipo == TipoObjeto::Maniqui && o.malla.is_none())
        .map(|o| o.nombre.as_str())
        .collect();
    for (vs, color, sel) in poligonos_equipos(m, true) {
        let albedo = lineal(color);
        let marca = match sel {
            Some(Sel::Objeto(n)) if pieles.contains(&n.as_str()) => 2.0,
            _ => 0.0,
        };
        for t in triangulos_de(&vs) {
            triangulo_gpu(&mut v, t, albedo, None, marca);
        }
    }
    v
}

/// Posiciones (de tres en tres) que proyectan sombra: sala, equipos y objetos, sin los cuerpos de las luces.
pub fn malla_sombras(m: &Montaje) -> Vec<[f32; 3]> {
    let mut caras: Vec<Vec<V3>> = cerramiento(&m.sala).iter().map(|c| c.to_vec()).collect();
    caras.extend(poligonos_equipos(m, false).into_iter().map(|(vs, _, _)| vs));
    caras.iter().flat_map(|vs| triangulos_de(vs)).flatten().map(|p| [p.x, p.y, p.z]).collect()
}

/// Los triángulos de `malla_sombras`, agrupados de tres en tres, para los rayos de sombra en CPU.
pub fn triangulos(m: &Montaje) -> Vec<[[f32; 3]; 3]> {
    malla_sombras(m).chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect()
}

/// Proyecta un polígono entero; None si algún vértice queda detrás del plano cercano.
fn proyectar_todos(cam: &Camara, rect: egui::Rect, vs: &[V3]) -> Option<(Vec<egui::Pos2>, f32)> {
    if vs.len() < 3 {
        return None;
    }
    let mut pts = Vec::with_capacity(vs.len());
    let mut prof = 0.0;
    for v in vs {
        let (s, d) = cam.proyectar(*v, rect)?;
        pts.push(s);
        prof += d;
    }
    Some((pts, prof / vs.len() as f32))
}

fn sombrear(c: egui::Color32, k: f32) -> egui::Color32 {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    let f = |x: u8| (x as f32 * k).round() as u8;
    egui::Color32::from_rgba_unmultiplied(f(r), f(g), f(b), a)
}

/// Suelo y rejilla: todos sus vértices están en y = 0.
fn es_suelo(pr: &Prim) -> bool {
    match &pr.forma {
        Forma::Poligono(vs) => vs.iter().all(|v| v.y == 0.0),
        Forma::Linea(a, b, _) => a.y == 0.0 && b.y == 0.0,
        Forma::Texto(..) => false,
    }
}

fn sombra(pr: &Prim, vs: &[V3], luz: V3) -> egui::Color32 {
    let n = (vs[1] - vs[0]).cross(vs[2] - vs[0]).norm();
    sombrear(pr.color, 0.55 + 0.45 * n.dot(luz).abs())
}

pub fn dibujar(p: &egui::Painter, cam: &Camara, rect: egui::Rect, prims: &[Prim]) {
    if rect.width() < 1.0 || rect.height() < 1.0 {
        return;
    }
    let luz = V3::new(0.3, 1.0, 0.5).norm();

    // La cámara está siempre sobre el suelo: suelo y rejilla van debajo de todo lo demás.
    for pr in prims.iter().filter(|pr| es_suelo(pr)) {
        match &pr.forma {
            Forma::Poligono(vs) => {
                if let Some((pts, _)) = proyectar_todos(cam, rect, vs) {
                    p.add(egui::Shape::convex_polygon(pts, sombra(pr, vs, luz), egui::Stroke::NONE));
                }
            }
            Forma::Linea(a, b, px) => {
                if let (Some((sa, _)), Some((sb, _))) = (cam.proyectar(*a, rect), cam.proyectar(*b, rect)) {
                    p.line_segment([sa, sb], egui::Stroke::new(*px, pr.color));
                }
            }
            Forma::Texto(..) => {}
        }
    }

    let mut polis = Vec::new();
    for pr in prims.iter().filter(|pr| !es_suelo(pr)) {
        let Forma::Poligono(vs) = &pr.forma else { continue };
        let Some((pts, prof)) = proyectar_todos(cam, rect, vs) else { continue };
        polis.push((prof, pts, sombra(pr, vs, luz), pr.resaltado));
    }
    polis.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (_, pts, color, res) in polis {
        // Relleno y contorno sin uniones en inglete: epaint las usa también al difuminar el relleno
        // y se disparan en caras vistas de canto.
        let n = pts.len();
        p.add(relleno(&pts, color));
        if res {
            let borde = egui::Stroke::new(2.0, egui::Color32::WHITE);
            for i in 0..n {
                p.line_segment([pts[i], pts[(i + 1) % n]], borde);
            }
        }
    }

    for pr in prims.iter().filter(|pr| !es_suelo(pr)) {
        match &pr.forma {
            Forma::Poligono(_) => {}
            Forma::Linea(a, b, px) => {
                if let (Some((sa, _)), Some((sb, _))) = (cam.proyectar(*a, rect), cam.proyectar(*b, rect)) {
                    p.line_segment([sa, sb], egui::Stroke::new(*px, pr.color));
                }
            }
            Forma::Texto(..) => {}
        }
    }
    for pr in prims {
        if let Forma::Texto(at, t) = &pr.forma {
            if let Some((s, _)) = cam.proyectar(*at, rect) {
                p.text(s, egui::Align2::CENTER_BOTTOM, t.as_str(), egui::FontId::proportional(12.0), pr.color);
            }
        }
    }
}

/// Lo que la GPU no dibuja: líneas (rejilla, cotas), textos y el contorno de lo resaltado.
pub fn dibujar_superpuesto(p: &egui::Painter, cam: &Camara, rect: egui::Rect, prims: &[Prim]) {
    if rect.width() < 1.0 || rect.height() < 1.0 {
        return;
    }
    for pr in prims {
        // La rejilla del suelo la pinta la GPU; sin esto se vería por encima de la geometría.
        if es_suelo(pr) {
            continue;
        }
        match &pr.forma {
            Forma::Poligono(vs) if pr.resaltado => {
                if let Some((pts, _)) = proyectar_todos(cam, rect, vs) {
                    let borde = egui::Stroke::new(2.0, egui::Color32::WHITE);
                    for i in 0..pts.len() {
                        p.line_segment([pts[i], pts[(i + 1) % pts.len()]], borde);
                    }
                }
            }
            Forma::Linea(a, b, px) => {
                if let (Some((sa, _)), Some((sb, _))) = (cam.proyectar(*a, rect), cam.proyectar(*b, rect)) {
                    p.line_segment([sa, sb], egui::Stroke::new(*px, pr.color));
                }
            }
            Forma::Texto(at, t) => {
                if let Some((s, _)) = cam.proyectar(*at, rect) {
                    p.text(s, egui::Align2::CENTER_BOTTOM, t.as_str(), egui::FontId::proportional(12.0), pr.color);
                }
            }
            Forma::Poligono(_) => {}
        }
    }
}

/// Abanico de triángulos sin difuminar (convexo).
fn relleno(pts: &[egui::Pos2], color: egui::Color32) -> egui::Shape {
    if pts.len() < 3 {
        return egui::Shape::Noop;
    }
    let mut malla = egui::Mesh::default();
    for &p in pts {
        malla.colored_vertex(p, color);
    }
    for i in 1..pts.len() as u32 - 1 {
        malla.add_triangle(0, i, i + 1);
    }
    egui::Shape::mesh(malla)
}

/// Dentro de un polígono convexo (cualquier orientación): todos los productos cruzados con el mismo signo.
fn dentro(pts: &[egui::Pos2], p: egui::Pos2) -> bool {
    let n = pts.len();
    let (mut pos, mut neg) = (false, false);
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let c = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
        pos |= c > 0.0;
        neg |= c < 0.0;
    }
    !(pos && neg)
}

fn dist_segmento(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ab = b - a;
    let l2 = ab.length_sq();
    let t = if l2 > 0.0 { ((p - a).dot(ab) / l2).clamp(0.0, 1.0) } else { 0.0 };
    (p - (a + ab * t)).length()
}

pub fn elegir(cam: &Camara, rect: egui::Rect, prims: &[Prim], raton: egui::Pos2) -> Option<Sel> {
    if rect.width() < 1.0 || rect.height() < 1.0 {
        return None;
    }
    let mut mejor: Option<(f32, &Sel)> = None;
    for pr in prims {
        let (Some(sel), Forma::Poligono(vs)) = (&pr.sel, &pr.forma) else { continue };
        let Some((pts, prof)) = proyectar_todos(cam, rect, vs) else { continue };
        if dentro(&pts, raton) && mejor.is_none_or(|(d, _)| prof < d) {
            mejor = Some((prof, sel));
        }
    }
    if let Some((_, sel)) = mejor {
        return Some(sel.clone());
    }
    let mut mejor_linea: Option<(f32, &Sel)> = None;
    for pr in prims {
        let (Some(sel), Forma::Linea(a, b, _)) = (&pr.sel, &pr.forma) else { continue };
        let (Some((sa, _)), Some((sb, _))) = (cam.proyectar(*a, rect), cam.proyectar(*b, rect)) else { continue };
        let d = dist_segmento(raton, sa, sb);
        if d <= 6.0 && mejor_linea.is_none_or(|(md, _)| d < md) {
            mejor_linea = Some((d, sel));
        }
    }
    mejor_linea.map(|(_, sel)| sel.clone())
}

pub fn arrastrar(m: &mut Montaje, sel: &Sel, origen: V3, dir: V3) {
    let h = m.sala.altura_carriles;
    match sel {
        Sel::Luz(id) => {
            let Some(cx) = m
                .luces
                .iter()
                .find(|l| &l.codigo == id)
                .and_then(|l| m.carriles.iter().find(|c| c.id == l.carril))
                .map(|c| c.x)
            else {
                return;
            };
            let t = param_mas_cercano(origen, dir, V3::new(cx, h - 0.25, 0.0), V3::new(0.0, 0.0, 1.0));
            if let Some(l) = m.luces.iter_mut().find(|l| &l.codigo == id) {
                l.z = t;
            }
        }
        Sel::Carril(id) => {
            // Punto del rayo sobre el plano de los carriles: el carril sigue al puntero sin saltar.
            let s = (h - origen.y) / dir.y;
            let t = if dir.y.abs() > 1e-3 && s > 0.0 {
                origen.x + dir.x * s
            } else {
                param_mas_cercano(origen, dir, V3::new(0.0, h, m.sala.largo / 2.0), V3::new(1.0, 0.0, 0.0))
            };
            if let Some(c) = m.carriles.iter_mut().find(|c| &c.id == id) {
                c.x = t;
            }
        }
        Sel::Toma(id) => {
            let Some((pared, nivel)) = m.tomas.iter().find(|t| &t.codigo == id).map(|t| (t.pared, t.nivel)) else {
                return;
            };
            let punto = V3::from_arr(pared.punto(&m.sala, 0.0, nivel));
            let t = param_mas_cercano(origen, dir, punto, V3::from_arr(pared.eje()));
            if let Some(toma) = m.tomas.iter_mut().find(|t| &t.codigo == id) {
                toma.pos = t;
            }
        }
        Sel::Objeto(nombre) => {
            let Some(o) = m.objetos.iter_mut().find(|o| &o.nombre == nombre) else {
                return;
            };
            // Punto del rayo sobre el plano horizontal a media altura del objeto.
            let s = (o.alto / 2.0 - origen.y) / dir.y;
            if dir.y.abs() > 1e-3 && s > 0.0 {
                let p = origen + dir * s;
                o.x = p.x;
                o.z = p.z;
            }
        }
        Sel::LuzPiso(id) => {
            let Some(l) = m.luces_piso.iter_mut().find(|l| &l.codigo == id) else {
                return;
            };
            // Plano a la altura de la cabeza: la luz sigue al puntero sin saltar.
            let s = (l.altura - origen.y) / dir.y;
            if dir.y.abs() > 1e-3 && s > 0.0 {
                let p = origen + dir * s;
                l.x = p.x;
                l.z = p.z;
            }
        }
        Sel::Camara(nombre) => {
            let Some(c) = m.camaras.iter_mut().find(|c| &c.nombre == nombre) else {
                return;
            };
            // Plano a la altura de la cámara: sigue al puntero sin saltar.
            let s = (c.altura - origen.y) / dir.y;
            if dir.y.abs() > 1e-3 && s > 0.0 {
                let p = origen + dir * s;
                c.x = p.x;
                c.z = p.z;
            }
        }
    }
    m.ajustar_a_sala();
}

pub const COLOR_COTA: egui::Color32 = egui::Color32::from_rgb(255, 210, 0);

pub struct Cota {
    pub a: V3,
    pub b: V3,
    pub texto: String,
}

/// Cota entre a y b; None si mide menos de 5 mm (p. ej. carril pegado a la pared).
fn cota(a: V3, b: V3) -> Option<Cota> {
    let l = (b - a).len();
    (l >= 0.005).then(|| Cota { a, b, texto: format!("{l:.2} m") })
}

/// Tramos a lo largo de un eje desde `pos` hasta el vecino más cercano a cada lado (o hasta la pared `fin`).
/// Si hay vecino, además la cota total de pared a `pos` (marcada `true`).
fn cotas_de(pos: f32, vecinos: &[f32], fin: f32, punto: impl Fn(f32, bool) -> V3) -> Vec<Cota> {
    let izq = vecinos.iter().copied().filter(|&v| v < pos).reduce(f32::max);
    let der = vecinos.iter().copied().filter(|&v| v > pos).reduce(f32::min);
    let mut tramos = Vec::with_capacity(4);
    match izq {
        Some(v) => tramos.extend([(v, pos, false), (0.0, pos, true)]),
        None => tramos.push((0.0, pos, false)),
    }
    match der {
        Some(v) => tramos.extend([(pos, v, false), (pos, fin, true)]),
        None => tramos.push((pos, fin, false)),
    }
    tramos.into_iter().filter_map(|(a, b, total)| cota(punto(a, total), punto(b, total))).collect()
}

/// Cotas en X de un carril: `punto(x, total)` da el punto de la cota en esa coordenada.
fn cotas_x(m: &Montaje, x: f32, punto: impl Fn(f32, bool) -> V3) -> Vec<Cota> {
    let xs: Vec<f32> = m.carriles.iter().map(|c| c.x).collect();
    cotas_de(x, &xs, m.sala.ancho, punto)
}

/// Cotas temporales de la selección (vacío si no existe).
pub fn cotas(m: &Montaje, sel: &Sel) -> Vec<Cota> {
    let s = &m.sala;
    let (w, l, h) = (s.ancho, s.largo, s.altura_carriles);
    match sel {
        Sel::Carril(id) => {
            let Some(c) = m.carriles.iter().find(|c| &c.id == id) else { return Vec::new() };
            cotas_x(m, c.x, |x, total| V3::new(x, h, if total { l / 2.0 + 0.4 } else { l / 2.0 }))
        }
        Sel::Luz(id) => {
            let Some(luz) = m.luces.iter().find(|x| &x.codigo == id) else { return Vec::new() };
            let c = centro_luz(m, luz);
            let zt = (luz.z + 0.4).min(l);
            let mut v = cotas_x(m, c.x, |x, total| V3::new(x, c.y, if total { zt } else { luz.z }));
            let zs: Vec<f32> = m.luces.iter().filter(|o| o.carril == luz.carril).map(|o| o.z).collect();
            let xt = (c.x + 0.4).min(w);
            v.extend(cotas_de(luz.z, &zs, l, |z, total| V3::new(if total { xt } else { c.x }, c.y, z)));
            v
        }
        Sel::Toma(id) => {
            let Some(t) = m.tomas.iter().find(|t| &t.codigo == id) else { return Vec::new() };
            let p = |pos: f32| V3::from_arr(t.pared.punto(s, pos, t.nivel));
            [cota(p(0.0), p(t.pos)), cota(p(t.pos), p(t.pared.largo(s)))].into_iter().flatten().collect()
        }
        Sel::Objeto(nombre) => {
            let Some(o) = m.objetos.iter().find(|o| &o.nombre == nombre) else { return Vec::new() };
            cotas_suelo(m, o.x, o.z)
        }
        Sel::LuzPiso(id) => {
            let Some(o) = m.luces_piso.iter().find(|o| &o.codigo == id) else { return Vec::new() };
            cotas_suelo(m, o.x, o.z)
        }
        Sel::Camara(nombre) => {
            let Some(c) = m.camaras.iter().find(|c| &c.nombre == nombre) else { return Vec::new() };
            cotas_suelo(m, c.x, c.z)
        }
    }
}

/// Cotas de algo apoyado en el suelo en (x, z): tramos hasta las paredes y guías hasta los carriles vecinos.
fn cotas_suelo(m: &Montaje, x: f32, z: f32) -> Vec<Cota> {
    let (l, h) = (m.sala.largo, m.sala.altura_carriles);
    let zt = (z + 0.4).min(l);
    let mut v = cotas_x(m, x, |xs, total| V3::new(xs, 0.02, if total { zt } else { z }));
    v.extend(cotas_de(z, &[], l, |zs, _| V3::new(x, 0.02, zs)));
    // Guías hasta los carriles vecinos que delimitan las cotas en X.
    let xs: Vec<f32> = m.carriles.iter().map(|c| c.x).collect();
    let izq = xs.iter().copied().filter(|&c| c < x).reduce(f32::max);
    let der = xs.iter().copied().filter(|&c| c > x).reduce(f32::min);
    for c in [izq, der].into_iter().flatten().filter(|c| (x - c).abs() >= 0.005) {
        v.push(Cota { a: V3::new(c, 0.02, z), b: V3::new(c, h, z), texto: String::new() });
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fotometria::fuentes;
    use crate::modelo::{direccion, CamaraEscena, Equipo, Superficie, TipoObjeto, ACABADOS, CATEGORIA_LUCES, TONOS_PIEL};

    #[test]
    fn ejes_luz_ortonormales() {
        for d in [[0.0, -1.0, 0.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.3, 0.5, -0.8], [0.95, -0.2, 0.1]] {
            let dir = V3::from_arr(d).norm();
            let (u, v) = ejes_luz(dir);
            assert!(u.dot(dir).abs() < 1e-5 && v.dot(dir).abs() < 1e-5, "{d:?}");
            assert!((u.len() - 1.0).abs() < 1e-5 && (v.len() - 1.0).abs() < 1e-5, "{d:?}");
            let w = u.cross(dir);
            assert!((v.x - w.x).abs() < 1e-5 && (v.y - w.y).abs() < 1e-5 && (v.z - w.z).abs() < 1e-5, "{d:?}");
        }
    }

    fn equipo(codigo: &str) -> Equipo {
        Equipo {
            codigo: codigo.to_string(),
            marca: "Aputure".to_string(),
            modelo: "300d".to_string(),
            categoria: CATEGORIA_LUCES.to_string(),
            specs: None,
        }
    }

    fn rect() -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::new(800.0, 600.0))
    }

    #[test]
    fn linea_y_colores_del_par() {
        let mut m = Montaje::default();
        m.colocar_luz(&equipo("E1"), "C1").unwrap();
        m.anadir_toma();
        m.anadir_toma();
        m.conectar("E1", Some("T-01")).unwrap();
        let prims = construir(&m, None);

        let lineas: Vec<&Prim> = prims
            .iter()
            .filter(|p| matches!(p.forma, Forma::Linea(..)) && p.sel == Some(Sel::Luz("E1".into())))
            .collect();
        assert_eq!(lineas.len(), 1);
        assert_eq!(lineas[0].color, color_par(0));

        let caja_t1 = prims
            .iter()
            .find(|p| p.sel == Some(Sel::Toma("T-01".into())) && matches!(p.forma, Forma::Poligono(_)))
            .unwrap();
        assert_eq!(caja_t1.color, color_par(0));

        let caja_t2 = prims
            .iter()
            .find(|p| p.sel == Some(Sel::Toma("T-02".into())) && matches!(p.forma, Forma::Poligono(_)))
            .unwrap();
        assert_eq!(caja_t2.color, SIN_TOMA);
    }

    #[test]
    fn seleccion_resalta() {
        let m = Montaje::default();
        let sel = Sel::Carril("C2".into());
        let prims = construir(&m, Some(&sel));
        assert!(prims.iter().any(|p| p.resaltado));
        assert!(prims.iter().filter(|p| p.sel.as_ref() == Some(&sel)).all(|p| p.resaltado));
        assert!(prims
            .iter()
            .filter(|p| p.sel == Some(Sel::Carril("C1".into())))
            .all(|p| !p.resaltado));
    }

    #[test]
    fn elegir_luz_en_su_pixel() {
        let mut m = Montaje::default();
        m.luces.clear();
        m.colocar_luz(&equipo("E1"), "C1").unwrap();
        let cam = Camara::para_sala(&m.sala);
        let px = cam.proyectar(centro_luz(&m, &m.luces[0]), rect()).unwrap().0;
        let prims = construir(&m, None);
        assert_eq!(elegir(&cam, rect(), &prims, px), Some(Sel::Luz("E1".into())));
    }

    #[test]
    fn arrastrar_luz_sobre_carril() {
        let mut m = Montaje::default();
        m.luces.clear();
        m.colocar_luz(&equipo("E1"), "C1").unwrap();
        let cam = Camara::para_sala(&m.sala);
        let px = cam.proyectar(V3::new(1.5, 3.55, 1.0), rect()).unwrap().0;
        let (o, d) = cam.rayo(px, rect());
        arrastrar(&mut m, &Sel::Luz("E1".into()), o, d);
        assert!((m.luces[0].z - 1.0).abs() < 0.01, "z = {}", m.luces[0].z);
    }

    #[test]
    fn arrastrar_toma_recorta() {
        let mut m = Montaje::default();
        m.anadir_toma();
        let cam = Camara::para_sala(&m.sala);
        let objetivo = V3::new(m.sala.ancho + 10.0, m.sala.altura_carriles, 0.0);
        let px = cam.proyectar(objetivo, rect()).unwrap().0;
        let (o, d) = cam.rayo(px, rect());
        arrastrar(&mut m, &Sel::Toma("T-01".into()), o, d);
        assert_eq!(m.tomas[0].pos, m.sala.ancho);
    }

    #[test]
    fn arrastrar_carril_sin_salto() {
        let mut m = Montaje::default();
        let cam = Camara::para_sala(&m.sala);
        let h = m.sala.altura_carriles;
        // C3 está en x = 4.5; se agarra cerca de su extremo (z = 0.5), no en el centro.
        let px = cam.proyectar(V3::new(4.5, h, 0.5), rect()).unwrap().0;
        let (o, d) = cam.rayo(px, rect());
        arrastrar(&mut m, &Sel::Carril("C3".into()), o, d);
        let x = m.carriles.iter().find(|c| c.id == "C3").unwrap().x;
        assert!((x - 4.5).abs() < 0.01, "x = {x}");
    }

    #[test]
    fn color_cuenta_solo_conectadas() {
        let mut m = Montaje::default();
        m.colocar_luz(&equipo("L1"), "C1").unwrap();
        m.colocar_luz(&equipo("L2"), "C1").unwrap();
        m.anadir_toma();
        m.conectar("L2", Some("T-01")).unwrap();
        let prims = construir(&m, None);
        let linea = prims
            .iter()
            .find(|p| matches!(p.forma, Forma::Linea(..)) && p.sel == Some(Sel::Luz("L2".into())))
            .unwrap();
        assert_eq!(linea.color, color_par(0));
    }

    #[test]
    fn dibujar_con_camara_dentro_no_falla() {
        let mut m = Montaje::default();
        m.colocar_luz(&equipo("E1"), "C1").unwrap();
        m.anadir_toma();
        m.conectar("E1", Some("T-01")).unwrap();
        let mut cam = Camara::para_sala(&m.sala);
        cam.objetivo = V3::new(3.0, 2.0, 3.0);
        cam.distancia = 1.0;
        let prims = construir(&m, Some(&Sel::Luz("E1".into())));
        let ctx = egui::Context::default();
        let mut salida = ctx.run_pass(egui::RawInput::default(), |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::background());
            dibujar(&painter, &cam, rect(), &prims);
        });
        // Sin pantalla no se aplican las texturas de fuentes; hay que vaciarlas o epaint entra en pánico al soltarlas.
        salida.textures_delta.clear();
    }

    #[test]
    fn resaltado_sin_picos() {
        let m = Montaje::default();
        let cam = Camara::para_sala(&m.sala);
        // Cara de 6 m con el plano que contiene la dirección de vista (vista de canto) y 1 mm de ancho.
        let fwd = (cam.objetivo - cam.ojo()).norm();
        let a = cam.objetivo - V3::new(3.0, 0.0, 0.0);
        let b = cam.objetivo + V3::new(3.0, 0.0, 0.0);
        let e = fwd * 0.001;
        let quad = vec![a, b, b + e, a + e];
        let proj: Vec<egui::Pos2> = quad.iter().map(|v| cam.proyectar(*v, rect()).unwrap().0).collect();
        // Precondiciones: la proyección es un sliver de verdad (< 1 px de ancho, > 100 px de largo).
        let ancho = dist_segmento(proj[2], proj[0], proj[1]).max(dist_segmento(proj[3], proj[0], proj[1]));
        assert!(ancho < 1.0, "ancho proyectado {ancho}");
        assert!(proj[0].distance(proj[1]) > 100.0, "largo proyectado {}", proj[0].distance(proj[1]));

        let prims = vec![Prim {
            forma: Forma::Poligono(quad),
            color: egui::Color32::GRAY,
            sel: Some(Sel::Carril("C1".into())),
            resaltado: true,
            cuerpo_luz: false,
        }];
        let ctx = egui::Context::default();
        let mut salida = ctx.run_pass(egui::RawInput::default(), |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::background());
            dibujar(&painter, &cam, rect(), &prims);
        });
        let teselado = ctx.tessellate(std::mem::take(&mut salida.shapes), salida.pixels_per_point);
        salida.textures_delta.clear();

        let margen = 10.0;
        let min_x = proj.iter().map(|p| p.x).fold(f32::INFINITY, f32::min) - margen;
        let max_x = proj.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max) + margen;
        let min_y = proj.iter().map(|p| p.y).fold(f32::INFINITY, f32::min) - margen;
        let max_y = proj.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max) + margen;
        let mut vertices = 0;
        for cp in teselado {
            if let egui::epaint::Primitive::Mesh(malla) = cp.primitive {
                for v in malla.vertices {
                    vertices += 1;
                    let p = v.pos;
                    assert!(
                        p.x >= min_x && p.x <= max_x && p.y >= min_y && p.y <= max_y,
                        "pico en {p:?}, fuera de x [{min_x}, {max_x}] y [{min_y}, {max_y}]"
                    );
                }
            }
        }
        assert!(vertices > 0, "no se generó geometría");
    }

    fn assert_lens(v: &[Cota], esperadas: &[f32]) {
        let mut l: Vec<f32> = v.iter().map(|c| (c.b - c.a).len()).collect();
        l.sort_by(f32::total_cmp);
        assert_eq!(l.len(), esperadas.len(), "{l:?}");
        for (a, b) in l.iter().zip(esperadas) {
            assert!((a - b).abs() < 0.001, "{l:?} vs {esperadas:?}");
        }
    }

    #[test]
    fn cotas_carril_entre_dos() {
        let m = Montaje::default();
        let v = cotas(&m, &Sel::Carril("C2".into()));
        assert_lens(&v, &[1.5, 1.5, 3.0, 3.0]);
    }

    #[test]
    fn cotas_carril_extremo() {
        let m = Montaje::default();
        let v = cotas(&m, &Sel::Carril("C1".into()));
        assert_lens(&v, &[1.5, 1.5, 4.5]);
    }

    #[test]
    fn cotas_luz_en_z() {
        let mut m = Montaje::default();
        m.luces.clear();
        m.colocar_luz(&equipo("L1"), "C1").unwrap();
        m.colocar_luz(&equipo("L2"), "C1").unwrap();
        m.luces[0].z = 3.0;
        m.luces[1].z = 1.0;
        let v = cotas(&m, &Sel::Luz("L1".into()));
        // X: 1.5 (pared), 1.5 (C2), 4.5 (total Este); Z: 2.0 (L2), 3.0 (Norte), 3.0 (total Sur)
        assert_lens(&v, &[1.5, 1.5, 2.0, 3.0, 3.0, 4.5]);
    }

    #[test]
    fn cotas_toma() {
        let mut m = Montaje::default();
        m.anadir_toma();
        m.tomas[0].pared = Pared::Sur;
        m.tomas[0].pos = 2.0;
        let v = cotas(&m, &Sel::Toma("T-01".into()));
        assert_lens(&v, &[2.0, 4.0]);
    }

    #[test]
    fn cotas_sel_inexistente_vacio() {
        let m = Montaje::default();
        assert!(cotas(&m, &Sel::Carril("X".into())).is_empty());
    }

    #[test]
    fn construir_dibuja_cotas() {
        let m = Montaje::default();
        let prims = construir(&m, Some(&Sel::Carril("C2".into())));
        let con_color: Vec<&Prim> = prims.iter().filter(|p| p.color == COLOR_COTA).collect();
        let lineas = con_color
            .iter()
            .filter(|p| matches!(p.forma, Forma::Linea(_, _, w) if w == 1.5))
            .count();
        assert_eq!(lineas, 4);
        let mut textos: Vec<String> = con_color
            .iter()
            .filter_map(|p| match &p.forma {
                Forma::Texto(_, t) => Some(t.clone()),
                _ => None,
            })
            .collect();
        textos.sort();
        assert_eq!(textos, ["1.50 m", "1.50 m", "3.00 m", "3.00 m"]);
    }

    #[test]
    fn kinoflo_cuerpo_panel() {
        use crate::modelo::{AjustesLuz, Luz, TipoLuz};
        let mut m = Montaje::default();
        m.luces.clear();
        m.luces_piso.clear();
        let specs = TipoLuz::KinoFloCeleb850.specs();
        let ajustes = AjustesLuz::por_defecto(&specs, false);
        m.luces.push(Luz { codigo: "K1".into(), marca: String::new(), modelo: String::new(), carril: "C1".into(), z: 1.5, toma: None, specs, ajustes });
        let prims = construir(&m, None);
        // El panel de 1.27 m es el cuerpo de la luz: alguna arista de sus caras mide más de 1.2 m.
        let arista_max = prims
            .iter()
            .filter(|p| p.cuerpo_luz)
            .filter_map(|p| match &p.forma {
                Forma::Poligono(vs) => Some((0..vs.len()).map(|i| (vs[i] - vs[(i + 1) % vs.len()]).len()).fold(0.0, f32::max)),
                _ => None,
            })
            .fold(0.0, f32::max);
        assert!(arista_max >= 1.2, "{arista_max}");
    }

    #[test]
    fn cotas_omite_longitud_cero() {
        let mut m = Montaje::default();
        // C1 y C2 en la pared Oeste: la cota C1-pared mediría 0 y debe omitirse.
        m.carriles[0].x = 0.0;
        m.carriles[1].x = 0.0;
        let v = cotas(&m, &Sel::Carril("C1".into()));
        assert_lens(&v, &[4.5, 6.0]);
    }

    #[test]
    fn objeto_seleccionable_y_resaltado() {
        let mut m = Montaje::default();
        m.anadir_objeto(TipoObjeto::Silla);
        let sel = Sel::Objeto("Silla 1".into());
        let prims = construir(&m, None);
        assert!(prims.iter().any(|p| p.sel.as_ref() == Some(&sel)));
        assert!(prims.iter().all(|p| !p.resaltado));
        let prims = construir(&m, Some(&sel));
        let propios: Vec<&Prim> = prims.iter().filter(|p| p.sel.as_ref() == Some(&sel)).collect();
        assert!(!propios.is_empty());
        assert!(propios.iter().all(|p| p.resaltado));
    }

    #[test]
    fn elegir_objeto_en_su_pixel() {
        let mut m = Montaje::default();
        m.objetos.clear();
        m.anadir_objeto(TipoObjeto::Silla);
        let cam = Camara::para_sala(&m.sala);
        let o = &m.objetos[0];
        let asiento = V3::new(o.x, 0.48 * o.alto, o.z);
        let px = cam.proyectar(asiento, rect()).unwrap().0;
        let prims = construir(&m, None);
        assert_eq!(elegir(&cam, rect(), &prims, px), Some(Sel::Objeto("Silla 1".into())));
    }

    #[test]
    fn rotacion_gira_la_forma() {
        let mut m = Montaje::default();
        m.objetos.clear();
        m.anadir_objeto(TipoObjeto::Mesa);
        m.objetos[0].rot = 90.0;
        let prims = construir(&m, None);
        let sel = Sel::Objeto("Mesa 1".into());
        let (mut x, mut z) = ((f32::INFINITY, f32::NEG_INFINITY), (f32::INFINITY, f32::NEG_INFINITY));
        for p in prims.iter().filter(|p| p.sel.as_ref() == Some(&sel)) {
            let Forma::Poligono(vs) = &p.forma else { continue };
            for v in vs {
                x = (x.0.min(v.x), x.1.max(v.x));
                z = (z.0.min(v.z), z.1.max(v.z));
            }
        }
        // Rotada 90°, el fondo (0.70) va en X y el ancho (1.20) en Z.
        assert!((x.1 - x.0 - 0.70).abs() < 0.01, "X {x:?}");
        assert!((z.1 - z.0 - 1.20).abs() < 0.01, "Z {z:?}");
    }

    #[test]
    fn arrastrar_objeto_por_el_suelo() {
        let mut m = Montaje::default();
        m.objetos.clear();
        m.anadir_objeto(TipoObjeto::Silla);
        let cam = Camara::para_sala(&m.sala);
        let px = cam.proyectar(V3::new(2.0, 0.45, 1.5), rect()).unwrap().0;
        let (o, d) = cam.rayo(px, rect());
        arrastrar(&mut m, &Sel::Objeto("Silla 1".into()), o, d);
        let s = &m.objetos[0];
        assert!((s.x - 2.0).abs() < 0.01 && (s.z - 1.5).abs() < 0.01, "x = {}, z = {}", s.x, s.z);
    }

    #[test]
    fn cotas_objeto_entre_carriles() {
        let mut m = Montaje::default();
        m.objetos.clear();
        m.anadir_objeto(TipoObjeto::Caja);
        m.objetos[0].x = 2.0;
        m.objetos[0].z = 3.0;
        let v = cotas(&m, &Sel::Objeto("Caja 1".into()));
        let guias = v.iter().filter(|c| c.texto.is_empty()).count();
        assert_eq!(guias, 2);
        let mut l: Vec<f32> = v.iter().filter(|c| !c.texto.is_empty()).map(|c| (c.b - c.a).len()).collect();
        l.sort_by(f32::total_cmp);
        let esperadas = [0.5, 1.0, 2.0, 3.0, 3.0, 4.0];
        assert_eq!(l.len(), esperadas.len(), "{l:?}");
        for (a, b) in l.iter().zip(esperadas) {
            assert!((a - b).abs() < 0.001, "{l:?}");
        }
    }

    fn malla_triangulo() -> Malla {
        Malla {
            nombre: "tri".to_string(),
            vertices: vec![[-0.5, 0.0, -0.5], [0.5, 0.0, 0.5], [0.0, 1.0, 0.0]],
            triangulos: vec![[0, 1, 2]],
            colores: vec![[255, 0, 0]],
            tam: [1.0, 1.0, 1.0],
        }
    }

    #[test]
    fn malla_se_dibuja_escalada() {
        let mut m = Montaje::default();
        m.objetos.clear();
        let nombre = m.colocar_malla(&malla_triangulo());
        m.objetos[0].ancho = 2.0;
        let prims = construir(&m, None);
        let sel = Sel::Objeto(nombre);
        let (mut min, mut max) = (f32::INFINITY, f32::NEG_INFINITY);
        for p in prims.iter().filter(|p| p.sel.as_ref() == Some(&sel)) {
            let Forma::Poligono(vs) = &p.forma else { continue };
            for v in vs {
                min = min.min(v.x);
                max = max.max(v.x);
            }
        }
        assert!((max - min - 2.0).abs() < 0.01, "ancho dibujado {}", max - min);
    }

    #[test]
    fn malla_ausente_dibuja_caja() {
        let mut m = Montaje::default();
        m.objetos.clear();
        m.anadir_objeto(TipoObjeto::Mesa);
        m.objetos[0].malla = Some("No existe".to_string());
        let sel = Sel::Objeto("Mesa 1".into());
        let prims = construir(&m, None);
        let propios: Vec<&Prim> = prims
            .iter()
            .filter(|p| p.sel.as_ref() == Some(&sel) && matches!(p.forma, Forma::Poligono(_)))
            .collect();
        assert_eq!(propios.len(), 6);
        assert!(propios.iter().all(|p| p.color == CARTON));
    }

    #[test]
    fn objeto_malla_seleccionable() {
        let mut m = Montaje::default();
        m.objetos.clear();
        let nombre = m.colocar_malla(&malla_triangulo());
        let cam = Camara::para_sala(&m.sala);
        let prims = construir(&m, None);
        let sel = Sel::Objeto(nombre);
        let Some(Forma::Poligono(vs)) = prims.iter().find(|p| p.sel.as_ref() == Some(&sel)).map(|p| &p.forma) else {
            panic!("sin triángulos");
        };
        let centro = (vs[0] + vs[1] + vs[2]) * (1.0 / 3.0);
        let px = cam.proyectar(centro, rect()).unwrap().0;
        assert_eq!(elegir(&cam, rect(), &prims, px), Some(sel));
    }

    #[test]
    fn partes_dentro_de_su_caja() {
        const T: f32 = 0.001;
        for t in TipoObjeto::TODOS {
            let v = partes(t);
            assert!(v.len() <= 20, "{}: {} partes", t.nombre(), v.len());
            for (c, s) in v {
                assert!(s.iter().all(|x| *x > 0.0), "{}", t.nombre());
                assert!(c[0].abs() + s[0] / 2.0 <= 0.5 + T, "{} x {c:?} {s:?}", t.nombre());
                assert!(c[2].abs() + s[2] / 2.0 <= 0.5 + T, "{} z {c:?} {s:?}", t.nombre());
                assert!(c[1] - s[1] / 2.0 >= -T && c[1] + s[1] / 2.0 <= 1.0 + T, "{} y {c:?} {s:?}", t.nombre());
            }
        }
    }

    /// Distancia de un punto a un triángulo: al plano si la proyección cae dentro, si no a sus aristas.
    fn dist_triangulo(p: V3, t: [V3; 3]) -> f32 {
        let [a, b, c] = t;
        let aristas = [(a, b), (b, c), (c, a)];
        let n = (b - a).cross(c - a).norm();
        let q = p - n * (p - a).dot(n);
        if aristas.iter().all(|&(u, v)| (v - u).cross(q - u).dot(n) >= 0.0) {
            return (p - q).len();
        }
        aristas
            .iter()
            .map(|&(u, v)| {
                let uv = v - u;
                let t = ((p - u).dot(uv) / uv.dot(uv).max(1e-12)).clamp(0.0, 1.0);
                (p - (u + uv * t)).len()
            })
            .fold(f32::INFINITY, f32::min)
    }

    #[test]
    fn malla_gpu_normales_unitarias() {
        let m = Montaje::default();
        let v = malla_gpu(&m);
        assert!(!v.is_empty());
        for x in &v {
            let n = V3::from_arr(x.normal);
            assert!((n.len() - 1.0).abs() < 1e-3, "normal {:?}", x.normal);
        }
    }

    #[test]
    fn suelo_mira_arriba() {
        let mut m = Montaje::default();
        m.objetos.clear();
        m.luces.clear();
        m.luces_piso.clear();
        let v = malla_gpu(&m);
        let suelo: Vec<&[VerticeGpu]> =
            v.chunks_exact(3).filter(|t| t.iter().all(|x| x.pos[1] == 0.0)).collect();
        assert!(suelo.len() >= 2, "{} triángulos de suelo", suelo.len());
        for t in suelo {
            assert!(t[0].normal[1] > 0.999, "{:?}", t[0].normal);
        }
    }

    #[test]
    fn malla_gpu_marca_carcasa() {
        let m = Montaje::default();
        let v = malla_gpu(&m);
        let (ancho, alto, largo) = (m.sala.ancho, m.sala.alto_techo, m.sala.largo);
        let carcasa: Vec<&VerticeGpu> = v.iter().filter(|x| (x.carcasa - 1.0).abs() < 0.5).collect();
        assert_eq!(carcasa.len(), 6 * 6, "6 caras de dos triángulos cada una");
        for x in &carcasa {
            let [px, py, pz] = x.pos;
            let en_borde = px.abs() < 1e-4 || (px - ancho).abs() < 1e-4 || py.abs() < 1e-4
                || (py - alto).abs() < 1e-4 || pz.abs() < 1e-4 || (pz - largo).abs() < 1e-4;
            assert!(en_borde, "vértice de carcasa fuera de la sala: {:?}", x.pos);
        }
        assert!(v.iter().any(|x| x.carcasa < 0.5), "los equipos no son carcasa");
    }

    #[test]
    fn luz_piso_seleccionable() {
        let mut m = Montaje::default();
        m.luces.clear();
        m.objetos.clear();
        let l = m.luces_piso[0].clone();
        let cam = Camara::para_sala(&m.sala);
        let px = cam.proyectar(V3::new(l.x, l.altura, l.z), rect()).unwrap().0;
        let prims = construir(&m, None);
        assert_eq!(elegir(&cam, rect(), &prims, px), Some(Sel::LuzPiso(l.codigo)));
    }

    #[test]
    fn luz_apunta_a_su_direccion() {
        let mut m = Montaje::default();
        m.luces[0].ajustes.giro = 30.0;
        m.luces[0].ajustes.inclinacion = -45.0;
        let sel = Sel::Luz(m.luces[0].codigo.clone());
        let c = centro_luz(&m, &m.luces[0]);
        let d = V3::from_arr(direccion(30.0, -45.0));
        let prims = construir(&m, None);
        let (mut max_perp, mut min_eje, mut max_eje) = (0.0f32, f32::INFINITY, f32::NEG_INFINITY);
        for p in prims.iter().filter(|p| p.sel.as_ref() == Some(&sel)) {
            let Forma::Poligono(vs) = &p.forma else { continue };
            for v in vs {
                let r = *v - c;
                let eje = r.dot(d);
                max_perp = max_perp.max((r - d * eje).len());
                min_eje = min_eje.min(eje);
                max_eje = max_eje.max(eje);
            }
        }
        // Cuerpo (±0.15 a lo largo del eje, 0.125 de lado) y yugo (hasta 0.23 por delante).
        assert!(max_perp <= 0.18 + 1e-3, "distancia perpendicular {max_perp}");
        assert!(max_eje > 0.2 && min_eje < -0.14, "eje [{min_eje}, {max_eje}]");
    }

    #[test]
    fn sombras_sin_cuerpos_de_luz() {
        let m = Montaje::default();
        let luces: Vec<V3> = fuentes(&m).iter().map(|f| V3::from_arr(f.pos)).collect();
        assert_eq!(luces.len(), m.luces.len() + m.luces_piso.len());
        for t in triangulos(&m) {
            let t = t.map(V3::from_arr);
            for l in &luces {
                let d = dist_triangulo(*l, t);
                assert!(d >= 0.15, "triángulo a {d:.3} m de la luz {l:?}: {t:?}");
            }
        }
    }

    #[test]
    fn albedo_gris_es_refl() {
        let a = Acabado { color: [188, 188, 188], reflexion: 0.5 };
        for c in albedo(a) {
            assert!((c - 0.5).abs() < 0.01, "{c}");
        }
    }

    #[test]
    fn maniqui_reflexion_piel() {
        let c = lineal(MANIQUI_COLOR);
        assert!((0.30..0.40).contains(&luminancia(c)), "{}", luminancia(c));
        assert!(c[0] > c[2], "{c:?}");
    }

    #[test]
    fn tonos_piel_reflexion() {
        let refl: Vec<f32> = TONOS_PIEL.iter().map(|(_, c)| reflexion_de_color(*c)).collect();
        assert!(refl.windows(2).all(|w| w[0] > w[1]), "{refl:?}");
        assert!((0.30..0.40).contains(&refl[1]), "{}", refl[1]);
    }

    #[test]
    fn malla_marca_piel() {
        let mut m = Montaje::default();
        let piel_de = |m: &Montaje| -> Vec<VerticeGpu> { malla_gpu(m).into_iter().filter(|v| v.carcasa == 2.0).collect() };
        let claro = piel_de(&m);
        assert!(!claro.is_empty());
        // Caja del maniquí por defecto: centro (4.2, 2.0), 0.5 × 1.75 × 0.3 m.
        for v in &claro {
            let [x, y, z] = v.pos;
            assert!((x - 4.2).abs() <= 0.26 && (z - 2.0).abs() <= 0.15 && (0.0..=1.75).contains(&y), "{:?}", v.pos);
        }
        let i = m.objetos.iter().position(|o| o.tipo == TipoObjeto::Maniqui).unwrap();
        m.objetos[i].piel = Some(TONOS_PIEL[5].1);
        let oscura = piel_de(&m);
        assert_eq!(oscura.len(), claro.len());
        assert!(oscura[0].albedo[1] < claro[0].albedo[1], "{:?} vs {:?}", oscura[0].albedo, claro[0].albedo);
    }

    #[test]
    fn albedo_verde_tiñe() {
        let a = albedo(ACABADOS[1].1);
        assert!(a[1] > a[0] && a[1] > a[2], "{a:?}");
        let lum = 0.2126 * a[0] + 0.7152 * a[1] + 0.0722 * a[2];
        assert!((lum - 0.32).abs() < 0.01, "{lum}");
    }

    #[test]
    fn malla_suelo_coloreado() {
        let mut m = Montaje::default();
        *m.sala.acabado_de_mut(Superficie::Suelo) = ACABADOS[1].1;
        let suelo: Vec<VerticeGpu> = malla_gpu(&m).into_iter().filter(|v| v.carcasa == 1.0 && v.normal[1] > 0.99).collect();
        assert!(!suelo.is_empty());
        for v in suelo {
            let [r, g, _] = v.albedo;
            assert!(g > r, "{:?}", v.albedo);
        }
    }

    #[test]
    fn malla_gpu_pared_por_pared() {
        let mut m = Montaje::default();
        for p in [Pared::Norte, Pared::Este, Pared::Oeste] {
            *m.sala.acabado_mut(p) = ACABADOS[0].1;
        }
        *m.sala.acabado_mut(Pared::Sur) = ACABADOS[1].1;
        // Las normales de las paredes son ±z: Sur mira a +z (hacia dentro), Norte a −z.
        let paredes: Vec<VerticeGpu> = malla_gpu(&m).into_iter().filter(|v| v.carcasa == 1.0 && v.normal[2].abs() > 0.99).collect();
        let sur: Vec<_> = paredes.iter().filter(|v| v.normal[2] > 0.0).collect();
        let norte: Vec<_> = paredes.iter().filter(|v| v.normal[2] < 0.0).collect();
        assert!(!sur.is_empty() && !norte.is_empty());
        for v in sur {
            assert!(v.albedo[1] > v.albedo[0], "{:?}", v.albedo);
        }
        for v in norte {
            let [r, g, b] = v.albedo;
            assert!((r - g).abs() < 0.05 && (g - b).abs() < 0.05, "{:?}", v.albedo);
        }
    }

    #[test]
    fn camara_de_escena_solo_lineas() {
        let m = Montaje::default();
        let propia = Sel::Camara("CAM1".to_string());
        let prims = construir(&m, None);
        let lineas = prims.iter().filter(|p| p.sel.as_ref() == Some(&propia) && matches!(p.forma, Forma::Linea(..))).count();
        // Cuerpo (12), pirámide (4), base (4) y triángulo (3).
        assert_eq!(lineas, 23);
        assert!(prims.iter().all(|p| p.sel.as_ref() != Some(&propia) || !matches!(p.forma, Forma::Poligono(_))));
    }

    #[test]
    fn camara_no_entra_en_las_mallas() {
        let mut m = Montaje::default();
        let (gpu, sombras) = (malla_gpu(&m).len(), malla_sombras(&m).len());
        m.camaras.push(CamaraEscena::nueva("CAM2".to_string(), &m.sala));
        assert_eq!(malla_gpu(&m).len(), gpu);
        assert_eq!(malla_sombras(&m).len(), sombras);
    }
}
