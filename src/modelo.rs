use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sala {
    pub ancho: f32,
    pub largo: f32,
    pub alto_techo: f32,
    pub altura_carriles: f32,
    /// Acabado de cada pared, en el orden de `Pared` (Norte, Sur, Este, Oeste).
    #[serde(default)]
    pub paredes: Vec<Acabado>,
    /// Acabado del suelo; los montajes viejos lo toman de `refl_suelo`.
    #[serde(default)]
    pub suelo: Option<Acabado>,
    /// Acabado del techo; los montajes viejos lo toman de `refl_techo`.
    #[serde(default)]
    pub techo: Option<Acabado>,
    /// Solo migración: montajes anteriores al acabado de suelo.
    #[serde(default = "refl_suelo_defecto", skip_serializing)]
    pub refl_suelo: f32,
    /// Solo migración: montajes anteriores al acabado de techo.
    #[serde(default = "refl_techo_defecto", skip_serializing)]
    pub refl_techo: f32,
    /// Solo migración: montajes anteriores al acabado por pared.
    #[serde(default = "refl_paredes_defecto", skip_serializing)]
    pub refl_paredes: f32,
    /// Solo migración: montajes anteriores al acabado por pared.
    #[serde(default = "color_paredes_defecto", skip_serializing)]
    pub color_paredes: [u8; 3],
}

/// Acabado de una pared: tinte y fracción de luz reflejada.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Acabado {
    /// sRGB, tinte de la pared.
    pub color: [u8; 3],
    /// Fracción de luz reflejada (0..0.95); absorción = 1 − reflexion.
    pub reflexion: f32,
}

/// Materiales predefinidos (nombre, acabado). Reflexión de pinturas: LRV publicado; croma: del sRGB de referencia.
pub const ACABADOS: [(&str, Acabado); 7] = [
    ("Pintura blanca mate", Acabado { color: [242, 242, 238], reflexion: 0.85 }),
    ("Pintura verde croma", Acabado { color: [0, 177, 64], reflexion: 0.32 }),
    ("Pintura azul croma", Acabado { color: [0, 71, 187], reflexion: 0.08 }),
    ("Pintura gris 18 %", Acabado { color: [118, 118, 118], reflexion: 0.18 }),
    ("Pintura negra mate", Acabado { color: [28, 28, 28], reflexion: 0.05 }),
    ("Molton negro (tela)", Acabado { color: [18, 18, 18], reflexion: 0.02 }),
    ("Concreto", Acabado { color: [160, 160, 155], reflexion: 0.35 }),
];

/// Nombre recortado de un preset del usuario; None si queda vacío o es el de un preset de fábrica (sin mayúsculas).
pub fn nombre_preset_valido(nombre: &str) -> Option<String> {
    let t = nombre.trim();
    let de_fabrica = ACABADOS.iter().any(|(n, _)| n.to_lowercase() == t.to_lowercase());
    (!t.is_empty() && !de_fabrica).then(|| t.to_string())
}

impl Sala {
    /// Acabado de la pared; un montaje sin `paredes` usa los campos de migración.
    pub fn acabado(&self, p: Pared) -> Acabado {
        self.paredes
            .get(p as usize)
            .copied()
            .unwrap_or(Acabado { color: self.color_paredes, reflexion: self.refl_paredes })
    }

    pub fn acabado_mut(&mut self, p: Pared) -> &mut Acabado {
        self.completar_acabados();
        &mut self.paredes[p as usize]
    }

    /// Acabado de cualquier superficie; un suelo o techo sin acabado usa el campo de migración.
    pub fn acabado_de(&self, s: Superficie) -> Acabado {
        match s {
            Superficie::Pared(p) => self.acabado(p),
            Superficie::Suelo => self.suelo.unwrap_or(acabado_migrado(self.refl_suelo)),
            Superficie::Techo => self.techo.unwrap_or(acabado_migrado(self.refl_techo)),
        }
    }

    pub fn acabado_de_mut(&mut self, s: Superficie) -> &mut Acabado {
        self.completar_acabados();
        match s {
            Superficie::Pared(p) => self.acabado_mut(p),
            Superficie::Suelo => self.suelo.get_or_insert(acabado_migrado(self.refl_suelo)),
            Superficie::Techo => self.techo.get_or_insert(acabado_migrado(self.refl_techo)),
        }
    }

    /// Deja todos los acabados completos (los montajes viejos copian los de migración) y limita la reflexión.
    fn completar_acabados(&mut self) {
        if self.paredes.len() != 4 {
            self.paredes = vec![Acabado { color: self.color_paredes, reflexion: self.refl_paredes }; 4];
        }
        self.suelo.get_or_insert(acabado_migrado(self.refl_suelo));
        self.techo.get_or_insert(acabado_migrado(self.refl_techo));
        for a in self.paredes.iter_mut().chain(&mut self.suelo).chain(&mut self.techo) {
            a.reflexion = limita(a.reflexion, 0.95);
        }
    }
}

/// Acabado gris de un montaje viejo, con la reflexión que tenía su campo de migración.
fn acabado_migrado(reflexion: f32) -> Acabado {
    Acabado { color: color_paredes_defecto(), reflexion }
}

/// Superficie de la sala con su propio acabado: cuatro paredes, suelo y techo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Superficie {
    Pared(Pared),
    Suelo,
    Techo,
}

impl Superficie {
    pub const TODAS: [Superficie; 6] = [
        Superficie::Pared(Pared::Norte),
        Superficie::Pared(Pared::Sur),
        Superficie::Pared(Pared::Este),
        Superficie::Pared(Pared::Oeste),
        Superficie::Suelo,
        Superficie::Techo,
    ];

    pub fn nombre(self) -> &'static str {
        match self {
            Superficie::Pared(p) => p.nombre(),
            Superficie::Suelo => "Suelo",
            Superficie::Techo => "Techo",
        }
    }
}

fn refl_suelo_defecto() -> f32 {
    0.2
}

fn refl_paredes_defecto() -> f32 {
    0.5
}

fn refl_techo_defecto() -> f32 {
    0.7
}

fn color_paredes_defecto() -> [u8; 3] {
    [188, 188, 188]
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Carril {
    pub id: String,
    pub x: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Pared {
    Norte,
    Sur,
    Este,
    Oeste,
}

/// Altura de una toma: en los carriles (techo) o pegada al suelo (luces de piso).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NivelToma {
    #[default]
    Carriles,
    Suelo,
}

/// Altura de las tomas de suelo, en metros.
pub const ALTURA_TOMA_SUELO: f32 = 0.3;

impl NivelToma {
    pub fn nombre(self) -> &'static str {
        match self {
            NivelToma::Carriles => "carriles",
            NivelToma::Suelo => "suelo",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Toma {
    pub codigo: String,
    pub pared: Pared,
    pub pos: f32,
    /// Los montajes anteriores a las tomas de suelo son todas de carriles.
    #[serde(default)]
    pub nivel: NivelToma,
}

pub const CATEGORIA_LUCES: &str = "Luces techo";
pub const CATEGORIA_LUCES_PISO: &str = "Luces piso";
pub const CATEGORIAS_BASE: [&str; 9] = [
    "Luces techo",
    "Luces piso",
    "Mesas",
    "Sillas",
    "Maniquíes",
    "Cámaras y soportes",
    "Iluminación de suelo",
    "Escenografía",
    "Audio y monitores",
];
/// Grupos del desplegable de formas y del selector de objetos, en orden.
pub const GRUPOS: [&str; 5] = [
    "Básicos",
    "Cámaras y soportes",
    "Iluminación de suelo",
    "Escenografía",
    "Audio y monitores",
];

fn categoria_luces() -> String {
    CATEGORIA_LUCES.to_string()
}

/// Es una categoría de luces (techo o piso), que no son objetos.
pub fn es_categoria_luz(cat: &str) -> bool {
    cat == CATEGORIA_LUCES || cat == CATEGORIA_LUCES_PISO
}

/// Tipo de luminaria; cada uno trae las especificaciones típicas del fabricante.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TipoLuz {
    FresnelLed,
    PanelLed,
    Elipsoidal,
    ParLed,
    SpaceLight,
    MonoluzSoftbox,
    PanelPie,
    FresnelPie,
    TuboLed,
    Ciclorama,
    KinoFloCeleb850,
    KinoFloFreeStyle31,
    FluotecCineLight120Lt,
    SilverStarFrezno250Hex,
}

impl TipoLuz {
    pub const TODOS: [TipoLuz; 14] = [
        TipoLuz::FresnelLed,
        TipoLuz::PanelLed,
        TipoLuz::Elipsoidal,
        TipoLuz::ParLed,
        TipoLuz::SpaceLight,
        TipoLuz::MonoluzSoftbox,
        TipoLuz::PanelPie,
        TipoLuz::FresnelPie,
        TipoLuz::TuboLed,
        TipoLuz::Ciclorama,
        TipoLuz::KinoFloCeleb850,
        TipoLuz::KinoFloFreeStyle31,
        TipoLuz::FluotecCineLight120Lt,
        TipoLuz::SilverStarFrezno250Hex,
    ];

    pub fn nombre(self) -> &'static str {
        match self {
            TipoLuz::FresnelLed => "Fresnel LED",
            TipoLuz::PanelLed => "Panel LED",
            TipoLuz::Elipsoidal => "Elipsoidal",
            TipoLuz::ParLed => "PAR LED",
            TipoLuz::SpaceLight => "Space light",
            TipoLuz::MonoluzSoftbox => "Monoluz COB + softbox",
            TipoLuz::PanelPie => "Panel LED en pie",
            TipoLuz::FresnelPie => "Fresnel en pie",
            TipoLuz::TuboLed => "Tubo LED",
            TipoLuz::Ciclorama => "Luz de ciclorama",
            TipoLuz::KinoFloCeleb850 => "Kino Flo Celeb 850",
            TipoLuz::KinoFloFreeStyle31 => "Kino Flo FreeStyle 31",
            TipoLuz::FluotecCineLight120Lt => "Fluotec CineLight 120 LT",
            TipoLuz::SilverStarFrezno250Hex => "Silver Star Frezno 250 Hex",
        }
    }

    /// Fuente difusa (panel blando): la rejilla recorta el derrame, pero no cambia el centro del haz.
    pub fn es_difusa(self) -> bool {
        matches!(self, TipoLuz::KinoFloCeleb850 | TipoLuz::KinoFloFreeStyle31 | TipoLuz::FluotecCineLight120Lt)
    }

    /// Medias del cuerpo medido (lado, lado, profundidad) en m, para dibujarlo.
    pub fn medio_cuerpo(self) -> [f32; 3] {
        match self {
            TipoLuz::KinoFloCeleb850 => [0.635, 0.355, 0.065],
            TipoLuz::KinoFloFreeStyle31 => [0.61, 0.15, 0.05],
            TipoLuz::FluotecCineLight120Lt => [0.6035, 0.1545, 0.062],
            TipoLuz::SilverStarFrezno250Hex => [0.127, 0.127, 0.19],
            _ => [0.125, 0.125, 0.15],
        }
    }

    /// Luminarias de piso (categoría Luces piso); el resto son de techo.
    pub fn es_piso(self) -> bool {
        matches!(
            self,
            TipoLuz::MonoluzSoftbox | TipoLuz::PanelPie | TipoLuz::FresnelPie | TipoLuz::TuboLed | TipoLuz::Ciclorama
        )
    }

    /// Radio aproximado del emisor (m): fija la penumbra de las sombras suaves (PCSS).
    pub fn radio_emisor(self) -> f32 {
        match self {
            TipoLuz::FresnelLed => 0.08,
            TipoLuz::PanelLed => 0.30,
            TipoLuz::Elipsoidal => 0.05,
            TipoLuz::ParLed => 0.10,
            TipoLuz::SpaceLight => 0.40,
            TipoLuz::MonoluzSoftbox => 0.45,
            TipoLuz::PanelPie => 0.20,
            TipoLuz::FresnelPie => 0.10,
            TipoLuz::TuboLed => 0.05,
            TipoLuz::Ciclorama => 0.20,
            TipoLuz::KinoFloCeleb850 => 0.53,
            TipoLuz::KinoFloFreeStyle31 => 0.34,
            TipoLuz::FluotecCineLight120Lt => 0.345,
            TipoLuz::SilverStarFrezno250Hex => 0.10,
        }
    }

    /// Exponente de la caída de intensidad al abrir el zoom: cd ∝ (tan(mín/2) / tan(apertura/2))^k. Con k = 2 el
    /// flujo no cambia; un fresnel real aprovecha más luz abierto. Frezno: 83 200 cd a 10° y 6075 cd a 55° → 1.47.
    pub fn exponente_zoom(self) -> f32 {
        match self {
            TipoLuz::SilverStarFrezno250Hex => 1.47,
            _ => 2.0,
        }
    }

    /// Nitidez del borde del haz: 1 gaussiana suave, 3 corte duro (elipsoidal).
    pub fn nitidez(self) -> f32 {
        match self {
            TipoLuz::Elipsoidal => 3.0,
            TipoLuz::ParLed => 1.5,
            TipoLuz::KinoFloCeleb850 | TipoLuz::KinoFloFreeStyle31 | TipoLuz::FluotecCineLight120Lt => 2.0,
            _ => 1.0,
        }
    }

    /// Medias del emisor rectangular (m, lados u y v); [0, 0] si el emisor es un disco de radio `radio_emisor`.
    pub fn medio_emisor(self) -> [f32; 2] {
        match self {
            TipoLuz::KinoFloCeleb850 | TipoLuz::KinoFloFreeStyle31 | TipoLuz::FluotecCineLight120Lt => {
                let m = self.medio_cuerpo();
                [m[0], m[1]]
            }
            _ => [0.0, 0.0],
        }
    }

    /// Valores típicos de la tabla de especificaciones (editables en el inventario).
    pub fn specs(self) -> SpecsLuz {
        // (W, lux a 1 m con apertura mínima, apertura mín, apertura máx, K mín, K máx, CRI)
        let (potencia_w, lux_1m, apertura_min, apertura_max, cct_min, cct_max, cri) = match self {
            TipoLuz::KinoFloCeleb850 => (575.0, 11000.0, 90.0, 160.0, 2500.0, 9900.0, 95.0),
            TipoLuz::KinoFloFreeStyle31 => (200.0, 2470.0, 90.0, 160.0, 2500.0, 9900.0, 96.0),
            // Fluotec CineLight Studio 120 Tunable Long Throw: manual (213 W, 120.7×30.9×12.4 cm); 3887 lx a 1.8 m ≈ 13 000 cd; haz 44°.
            TipoLuz::FluotecCineLight120Lt => (213.0, 13000.0, 44.0, 44.0, 2700.0, 6600.0, 90.0),
            // Silver Star Frezno 250 Hex (RGBALC, lente 8"): 83 200 cd con zoom mínimo (3328 lx a 5 m); haz 10°–55°.
            TipoLuz::SilverStarFrezno250Hex => (250.0, 83200.0, 10.0, 55.0, 1800.0, 10000.0, 95.0),
            TipoLuz::FresnelLed => (150.0, 20000.0, 15.0, 50.0, 3200.0, 5600.0, 96.0),
            TipoLuz::PanelLed => (100.0, 3300.0, 65.0, 65.0, 2700.0, 6500.0, 96.0),
            TipoLuz::Elipsoidal => (200.0, 26000.0, 19.0, 36.0, 3200.0, 3200.0, 95.0),
            TipoLuz::ParLed => (200.0, 12000.0, 25.0, 25.0, 2700.0, 6500.0, 95.0),
            TipoLuz::SpaceLight => (400.0, 2000.0, 120.0, 120.0, 3200.0, 5600.0, 95.0),
            TipoLuz::MonoluzSoftbox => (300.0, 4000.0, 90.0, 90.0, 5600.0, 5600.0, 96.0),
            TipoLuz::PanelPie => (60.0, 2000.0, 65.0, 65.0, 2700.0, 6500.0, 96.0),
            TipoLuz::FresnelPie => (300.0, 30000.0, 15.0, 45.0, 3200.0, 5600.0, 96.0),
            TipoLuz::TuboLed => (30.0, 600.0, 120.0, 120.0, 2700.0, 10000.0, 95.0),
            TipoLuz::Ciclorama => (200.0, 3000.0, 100.0, 100.0, 2700.0, 6500.0, 95.0),
        };
        let rgb = matches!(self, TipoLuz::KinoFloCeleb850 | TipoLuz::KinoFloFreeStyle31 | TipoLuz::SilverStarFrezno250Hex);
        SpecsLuz { tipo: self, potencia_w, lux_1m, apertura_min, apertura_max, cct_min, cct_max, cri, rgb, ies: None }
    }
}

/// Especificaciones de fabricante de un equipo de luz. Apertura en grados, temperatura en K.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpecsLuz {
    pub tipo: TipoLuz,
    pub potencia_w: f32,
    pub lux_1m: f32,
    pub apertura_min: f32,
    pub apertura_max: f32,
    pub cct_min: f32,
    pub cct_max: f32,
    pub cri: f32,
    /// Puede dar color (RGBW): sin esto, la saturación se queda en 0.
    #[serde(default)]
    pub rgb: bool,
    /// Nombre del perfil IES (`Montaje::fotometrias`); sin él, la luz usa la campana del tipo.
    #[serde(default)]
    pub ies: Option<String>,
}

impl SpecsLuz {
    /// Todos los números son finitos; un archivo editado a mano puede traer NaN.
    pub fn valido(&self) -> bool {
        [self.potencia_w, self.lux_1m, self.apertura_min, self.apertura_max, self.cct_min, self.cct_max, self.cri]
            .iter()
            .all(|v| v.is_finite())
    }
}

/// Ajustes de una luz colocada: dimmer 0..1, apertura en su rango, temperatura en su rango,
/// giro 0..360 y inclinación -90..90 grados. Color (solo RGBW): tono 0..360 y saturación 0..1.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AjustesLuz {
    pub encendida: bool,
    pub dimmer: f32,
    pub apertura: f32,
    pub cct: f32,
    pub giro: f32,
    pub inclinacion: f32,
    #[serde(default)]
    pub tono: f32,
    #[serde(default)]
    pub saturacion: f32,
}

/// Altura de una luz de piso al colocarla, y sus límites.
pub const ALTURA_PISO_DEFECTO: f32 = 1.8;
pub const ALTURA_PISO_MIN: f32 = 0.5;
pub const ALTURA_PISO_MAX: f32 = 3.0;

impl AjustesLuz {
    /// Ajustes de una luz recién colocada: encendida, al 100 %, apertura mínima, 5600 K dentro de su rango.
    /// Las de techo miran hacia abajo; las de piso, horizontales.
    pub fn por_defecto(s: &SpecsLuz, piso: bool) -> AjustesLuz {
        AjustesLuz {
            encendida: true,
            dimmer: 1.0,
            apertura: s.apertura_min,
            cct: 5600.0_f32.max(s.cct_min).min(s.cct_max),
            giro: 0.0,
            inclinacion: if piso { 0.0 } else { -90.0 },
            tono: 0.0,
            saturacion: 0.0,
        }
    }

    /// Recorta a sus rangos. Con NaN, cada valor vuelve a su mínimo o a 0.
    pub fn limitar(&mut self, s: &SpecsLuz) {
        self.dimmer = limita(self.dimmer, 1.0);
        let (lo, hi) = (s.apertura_min.min(s.apertura_max), s.apertura_min.max(s.apertura_max));
        self.apertura = self.apertura.max(lo).min(hi);
        self.cct = self.cct.max(s.cct_min).min(s.cct_max);
        // Con NaN, rem_euclid da NaN: la comparación lo manda a 0.
        let r = self.giro.rem_euclid(360.0);
        self.giro = if r < 360.0 { r } else { 0.0 };
        self.inclinacion = self.inclinacion.max(-90.0).min(90.0);
        let t = self.tono.rem_euclid(360.0);
        self.tono = if t < 360.0 { t } else { 0.0 };
        self.saturacion = if s.rgb { limita(self.saturacion, 1.0) } else { 0.0 };
    }
}

/// Dirección (en unidades de la escena, Y arriba) a la que mira una luz con ese giro e inclinación, en grados.
pub fn direccion(giro: f32, inclinacion: f32) -> [f32; 3] {
    let (g, i) = (giro.to_radians(), inclinacion.to_radians());
    [i.cos() * g.sin(), i.sin(), i.cos() * g.cos()]
}

/// Giro e inclinación (grados) para que una luz en `desde` mire a `hacia`; inversa de `direccion`.
pub fn apuntar(desde: [f32; 3], hacia: [f32; 3]) -> (f32, f32) {
    let d = [hacia[0] - desde[0], hacia[1] - desde[1], hacia[2] - desde[2]];
    let giro = d[0].atan2(d[2]).to_degrees().rem_euclid(360.0);
    let inclinacion = d[1].atan2(d[0].hypot(d[2])).to_degrees();
    (giro, inclinacion)
}

/// Sensores típicos: (nombre, ancho mm, alto mm).
pub const SENSORES: [(&str, f32, f32); 5] = [
    ("Full frame", 36.0, 24.0),
    ("Super 35", 24.89, 18.66),
    ("APS-C", 23.5, 15.6),
    ("Micro 4/3", 17.3, 13.0),
    ("1\"", 13.2, 8.8),
];
/// Formatos de imagen: (nombre, ancho/alto).
pub const FORMATOS: [(&str, f32); 5] = [("16:9", 16.0 / 9.0), ("2.39:1", 2.39), ("3:2", 1.5), ("4:3", 4.0 / 3.0), ("1:1", 1.0)];

/// Cámara colocada en la sala, como las de Blender: posición, orientación y ajustes físicos.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CamaraEscena {
    pub nombre: String,
    pub x: f32,
    pub altura: f32,
    pub z: f32,
    /// Grados, como las luces.
    pub giro: f32,
    /// Grados, −89..89.
    pub inclinacion: f32,
    /// 8..400.
    pub focal_mm: f32,
    /// Ancho del sensor; fija el campo horizontal. 5..70.
    pub sensor_ancho_mm: f32,
    /// Ancho/alto de la imagen. 0.5..3.
    pub aspecto: f32,
    pub iso: f32,
    pub diafragma: f32,
    /// Segundos, 1/8000..30.
    pub obturacion: f32,
    /// Kelvin, 2000..12000.
    pub balance_k: f32,
    /// Distancia de foco en metros, 0.1..100.
    pub foco_m: f32,
}

impl CamaraEscena {
    /// Cámara junto a la pared del fondo, a 1.6 m, mirando al centro de la sala.
    pub fn nueva(nombre: String, s: &Sala) -> CamaraEscena {
        let (x, altura, z) = (s.ancho / 2.0, 1.6, s.largo - 0.5);
        // Con giro 180 y inclinación −10, direccion da x = 0 y z < 0: mira hacia el centro.
        let centro = [s.ancho / 2.0, s.alto_techo / 2.0, s.largo / 2.0];
        let foco_m = ((altura - centro[1]).powi(2) + (z - centro[2]).powi(2)).sqrt();
        CamaraEscena {
            nombre,
            x,
            altura,
            z,
            giro: 180.0,
            inclinacion: -10.0,
            focal_mm: 35.0,
            sensor_ancho_mm: 36.0,
            aspecto: 16.0 / 9.0,
            iso: 800.0,
            diafragma: 2.8,
            obturacion: 1.0 / 50.0,
            balance_k: 5600.0,
            foco_m,
        }
    }

    /// Recorta cada campo a su rango. Con NaN, cada valor vuelve al de `nueva`
    /// (x y z a 0: sin sala no hay sitio mejor, y `ajustar_a_sala` las mete dentro).
    pub fn limitar(&mut self) {
        self.x = if self.x.is_nan() { 0.0 } else { self.x };
        self.z = if self.z.is_nan() { 0.0 } else { self.z };
        self.altura = acota(self.altura, 1.6, 0.1, 100.0);
        // Con NaN, rem_euclid da NaN: la comparación lo manda al giro de `nueva`.
        let r = self.giro.rem_euclid(360.0);
        self.giro = if r < 360.0 { r } else { 180.0 };
        self.inclinacion = acota(self.inclinacion, -10.0, -89.0, 89.0);
        self.focal_mm = acota(self.focal_mm, 35.0, 8.0, 400.0);
        self.sensor_ancho_mm = acota(self.sensor_ancho_mm, 36.0, 5.0, 70.0);
        self.aspecto = acota(self.aspecto, 16.0 / 9.0, 0.5, 3.0);
        self.iso = acota(self.iso, 800.0, 50.0, 25600.0);
        self.diafragma = acota(self.diafragma, 2.8, 1.0, 32.0);
        self.obturacion = acota(self.obturacion, 1.0 / 50.0, 1.0 / 8000.0, 30.0);
        self.balance_k = acota(self.balance_k, 5600.0, 2000.0, 12000.0);
        self.foco_m = acota(self.foco_m, 3.0, 0.1, 100.0);
    }

    /// Dirección a la que mira (Y arriba), como la de una luz.
    pub fn direccion(&self) -> [f32; 3] {
        direccion(self.giro, self.inclinacion)
    }

    /// Campo vertical en radianes: el ancho del sensor fija el horizontal y el formato el vertical.
    pub fn fov_y(&self) -> f32 {
        let fov_h = 2.0 * (self.sensor_ancho_mm / (2.0 * self.focal_mm)).atan();
        2.0 * ((fov_h / 2.0).tan() / self.aspecto).atan()
    }

    /// Radio de la pupila de entrada en metros (lente fina): focal / (2·N).
    pub fn radio_lente(&self) -> f32 {
        self.focal_mm / 1000.0 / (2.0 * self.diafragma)
    }
}

/// `v` recortado a [lo, hi]; NaN da `nan`. Usa max/min (no clamp) para no entrar en pánico.
fn acota(v: f32, nan: f32, lo: f32, hi: f32) -> f32 {
    if v.is_nan() { nan } else { v.max(lo).min(hi) }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Equipo {
    pub codigo: String,
    pub marca: String,
    pub modelo: String,
    /// Los inventarios guardados antes de existir categorías son todos de luces.
    #[serde(default = "categoria_luces")]
    pub categoria: String,
    /// Solo en las categorías de luces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub specs: Option<SpecsLuz>,
}

fn specs_techo_defecto() -> SpecsLuz {
    TipoLuz::PanelLed.specs()
}

fn ajustes_techo_defecto() -> AjustesLuz {
    AjustesLuz::por_defecto(&TipoLuz::PanelLed.specs(), false)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Luz {
    pub codigo: String,
    pub marca: String,
    pub modelo: String,
    pub carril: String,
    pub z: f32,
    pub toma: Option<String>,
    /// Los montajes anteriores a las especificaciones cargan con las de Panel LED.
    #[serde(default = "specs_techo_defecto")]
    pub specs: SpecsLuz,
    #[serde(default = "ajustes_techo_defecto")]
    pub ajustes: AjustesLuz,
}

/// Luz de suelo con pie: se coloca en (x, z) a la altura dada.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LuzPiso {
    pub codigo: String,
    pub marca: String,
    pub modelo: String,
    pub specs: SpecsLuz,
    pub ajustes: AjustesLuz,
    pub x: f32,
    pub z: f32,
    pub altura: f32,
    pub toma: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TipoObjeto {
    Silla,
    Mesa,
    Maniqui,
    Caja,
    CamaraTripode,
    Tripode,
    Slider,
    Grua,
    Teleprompter,
    CStand,
    SoporteLuz,
    SoftboxPie,
    Paraguas,
    Reflector,
    Bandera,
    PanelLed,
    Fondo,
    Ciclorama,
    Sofa,
    Escritorio,
    Taburete,
    Planta,
    Estanteria,
    MicrofonoPie,
    Boom,
    MonitorSoporte,
    TvSoporte,
    Altavoz,
}

impl TipoObjeto {
    pub const TODOS: [TipoObjeto; 28] = [
        TipoObjeto::Silla,
        TipoObjeto::Mesa,
        TipoObjeto::Maniqui,
        TipoObjeto::Caja,
        TipoObjeto::CamaraTripode,
        TipoObjeto::Tripode,
        TipoObjeto::Slider,
        TipoObjeto::Grua,
        TipoObjeto::Teleprompter,
        TipoObjeto::CStand,
        TipoObjeto::SoporteLuz,
        TipoObjeto::SoftboxPie,
        TipoObjeto::Paraguas,
        TipoObjeto::Reflector,
        TipoObjeto::Bandera,
        TipoObjeto::PanelLed,
        TipoObjeto::Fondo,
        TipoObjeto::Ciclorama,
        TipoObjeto::Sofa,
        TipoObjeto::Escritorio,
        TipoObjeto::Taburete,
        TipoObjeto::Planta,
        TipoObjeto::Estanteria,
        TipoObjeto::MicrofonoPie,
        TipoObjeto::Boom,
        TipoObjeto::MonitorSoporte,
        TipoObjeto::TvSoporte,
        TipoObjeto::Altavoz,
    ];

    pub fn nombre(self) -> &'static str {
        match self {
            TipoObjeto::Silla => "Silla",
            TipoObjeto::Mesa => "Mesa",
            TipoObjeto::Maniqui => "Maniquí",
            TipoObjeto::Caja => "Caja",
            TipoObjeto::CamaraTripode => "Cámara en trípode",
            TipoObjeto::Tripode => "Trípode",
            TipoObjeto::Slider => "Slider",
            TipoObjeto::Grua => "Grúa / jib",
            TipoObjeto::Teleprompter => "Teleprompter",
            TipoObjeto::CStand => "C-stand",
            TipoObjeto::SoporteLuz => "Soporte de luz",
            TipoObjeto::SoftboxPie => "Softbox de pie",
            TipoObjeto::Paraguas => "Paraguas",
            TipoObjeto::Reflector => "Reflector / rebotador",
            TipoObjeto::Bandera => "Bandera",
            TipoObjeto::PanelLed => "Panel LED de pie",
            TipoObjeto::Fondo => "Fondo con soporte",
            TipoObjeto::Ciclorama => "Ciclorama",
            TipoObjeto::Sofa => "Sofá",
            TipoObjeto::Escritorio => "Escritorio",
            TipoObjeto::Taburete => "Taburete alto",
            TipoObjeto::Planta => "Planta",
            TipoObjeto::Estanteria => "Estantería",
            TipoObjeto::MicrofonoPie => "Micrófono de pie",
            TipoObjeto::Boom => "Boom / caña",
            TipoObjeto::MonitorSoporte => "Monitor en soporte",
            TipoObjeto::TvSoporte => "TV en soporte",
            TipoObjeto::Altavoz => "Altavoz",
        }
    }

    /// Grupo del desplegable de formas (y categoría base de las nuevas).
    pub fn grupo(self) -> &'static str {
        match self {
            TipoObjeto::Silla | TipoObjeto::Mesa | TipoObjeto::Maniqui | TipoObjeto::Caja => "Básicos",
            TipoObjeto::CamaraTripode | TipoObjeto::Tripode | TipoObjeto::Slider | TipoObjeto::Grua | TipoObjeto::Teleprompter => {
                "Cámaras y soportes"
            }
            TipoObjeto::CStand
            | TipoObjeto::SoporteLuz
            | TipoObjeto::SoftboxPie
            | TipoObjeto::Paraguas
            | TipoObjeto::Reflector
            | TipoObjeto::Bandera
            | TipoObjeto::PanelLed => "Iluminación de suelo",
            TipoObjeto::Fondo
            | TipoObjeto::Ciclorama
            | TipoObjeto::Sofa
            | TipoObjeto::Escritorio
            | TipoObjeto::Taburete
            | TipoObjeto::Planta
            | TipoObjeto::Estanteria => "Escenografía",
            TipoObjeto::MicrofonoPie | TipoObjeto::Boom | TipoObjeto::MonitorSoporte | TipoObjeto::TvSoporte | TipoObjeto::Altavoz => {
                "Audio y monitores"
            }
        }
    }

    /// (ancho, fondo, alto) en metros.
    pub fn medidas(self) -> (f32, f32, f32) {
        match self {
            TipoObjeto::Silla => (0.45, 0.45, 0.90),
            TipoObjeto::Mesa => (1.20, 0.70, 0.75),
            TipoObjeto::Maniqui => (0.50, 0.30, 1.75),
            TipoObjeto::Caja => (1.0, 1.0, 1.0),
            TipoObjeto::CamaraTripode => (0.7, 0.7, 1.6),
            TipoObjeto::Tripode => (0.7, 0.7, 1.5),
            TipoObjeto::Slider => (1.2, 0.3, 0.9),
            TipoObjeto::Grua => (2.5, 0.8, 1.8),
            TipoObjeto::Teleprompter => (0.6, 0.7, 1.5),
            TipoObjeto::CStand => (0.9, 0.9, 2.5),
            TipoObjeto::SoporteLuz => (0.8, 0.8, 2.2),
            TipoObjeto::SoftboxPie => (0.9, 0.9, 2.0),
            TipoObjeto::Paraguas => (1.0, 1.0, 2.0),
            TipoObjeto::Reflector => (1.0, 0.6, 1.6),
            TipoObjeto::Bandera => (0.9, 0.6, 1.8),
            TipoObjeto::PanelLed => (0.6, 0.6, 1.9),
            TipoObjeto::Fondo => (2.7, 0.6, 2.7),
            TipoObjeto::Ciclorama => (4.0, 2.0, 3.0),
            TipoObjeto::Sofa => (2.0, 0.9, 0.85),
            TipoObjeto::Escritorio => (1.4, 0.7, 0.75),
            TipoObjeto::Taburete => (0.4, 0.4, 0.75),
            TipoObjeto::Planta => (0.5, 0.5, 1.2),
            TipoObjeto::Estanteria => (1.0, 0.35, 1.8),
            TipoObjeto::MicrofonoPie => (0.5, 0.5, 1.6),
            TipoObjeto::Boom => (0.6, 0.6, 2.2),
            TipoObjeto::MonitorSoporte => (0.6, 0.5, 1.5),
            TipoObjeto::TvSoporte => (1.2, 0.6, 1.7),
            TipoObjeto::Altavoz => (0.3, 0.3, 1.4),
        }
    }
}

/// Tipo con el que se dibuja un equipo de esa categoría; las demás son cajas.
pub fn tipo_para_categoria(cat: &str) -> TipoObjeto {
    match cat {
        "Mesas" => TipoObjeto::Mesa,
        "Sillas" => TipoObjeto::Silla,
        "Maniquíes" => TipoObjeto::Maniqui,
        "Cámaras y soportes" => TipoObjeto::CamaraTripode,
        "Iluminación de suelo" => TipoObjeto::SoporteLuz,
        "Escenografía" => TipoObjeto::Fondo,
        "Audio y monitores" => TipoObjeto::MicrofonoPie,
        _ => TipoObjeto::Caja,
    }
}

/// Categoría que corresponde a cada tipo; las cajas genéricas no tienen.
pub fn categoria_de_tipo(t: TipoObjeto) -> &'static str {
    match t {
        TipoObjeto::Silla => "Sillas",
        TipoObjeto::Mesa => "Mesas",
        TipoObjeto::Maniqui => "Maniquíes",
        TipoObjeto::Caja => "",
        _ => t.grupo(),
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Objeto {
    pub nombre: String,
    pub tipo: TipoObjeto,
    pub x: f32,
    pub z: f32,
    /// Grados, alrededor del eje Y.
    pub rot: f32,
    pub ancho: f32,
    pub fondo: f32,
    pub alto: f32,
    /// Vacíos en los objetos genéricos; los de inventario los copian del equipo.
    #[serde(default)]
    pub marca: String,
    #[serde(default)]
    pub modelo: String,
    /// Vacía en las cajas genéricas; los montajes viejos la reciben al cargar.
    #[serde(default)]
    pub categoria: String,
    /// Nombre de un modelo importado: si existe, se dibuja en lugar de `tipo`.
    #[serde(default)]
    pub malla: Option<String>,
    /// Tono de piel del maniquí; None = Clara.
    #[serde(default)]
    pub piel: Option<[u8; 3]>,
}

/// Tonos de piel de maniquí (sRGB), de más clara a más oscura.
pub const TONOS_PIEL: [(&str, [u8; 3]); 7] = [
    ("Muy clara", [199, 171, 153]),
    ("Clara", [194, 150, 130]),
    ("Intermedia", [175, 137, 112]),
    ("Morena clara", [161, 121, 94]),
    ("Morena", [138, 98, 74]),
    ("Oscura", [115, 82, 68]),
    ("Muy oscura", [107, 75, 59]),
];

/// Modelo importado ya procesado: triángulos con color base, en metros, Y arriba.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Malla {
    pub nombre: String,
    pub vertices: Vec<[f32; 3]>,
    pub triangulos: Vec<[u32; 3]>,
    /// Uno por triángulo.
    pub colores: Vec<[u8; 3]>,
    /// Ancho, alto, fondo en metros.
    pub tam: [f32; 3],
}

impl Malla {
    /// Comprueba que los índices, colores y medidas sean usables; un archivo editado a mano puede no serlo.
    pub fn valida(&self) -> Result<(), String> {
        if self.colores.len() != self.triangulos.len() {
            return Err("colores y triángulos no cuadran".to_string());
        }
        if self.vertices.iter().flatten().chain(&self.tam).any(|v| !v.is_finite()) {
            return Err("hay valores no finitos".to_string());
        }
        let n = self.vertices.len() as u64;
        if self.triangulos.iter().flatten().any(|&i| i as u64 >= n) {
            return Err("un triángulo usa un vértice que no existe".to_string());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Montaje {
    pub sala: Sala,
    pub carriles: Vec<Carril>,
    pub tomas: Vec<Toma>,
    pub luces: Vec<Luz>,
    /// Los montajes anteriores a las luces de piso cargan con la lista vacía.
    #[serde(default)]
    pub luces_piso: Vec<LuzPiso>,
    /// Los montajes guardados antes de existir objetos cargan con la lista vacía.
    #[serde(default)]
    pub objetos: Vec<Objeto>,
    /// Copia de los modelos importados que usan los objetos; los montajes viejos no la tienen.
    #[serde(default)]
    pub modelos: std::collections::BTreeMap<String, Malla>,
    /// Presets de pared del usuario que usa alguna pared; los montajes viejos no los tienen.
    #[serde(default)]
    pub acabados: std::collections::BTreeMap<String, Acabado>,
    /// Texto de los perfiles IES (.ies) que usa alguna luz, por nombre; los montajes viejos no los tienen.
    #[serde(default)]
    pub fotometrias: std::collections::BTreeMap<String, String>,
    /// Cámaras de la sala; los montajes anteriores a las cámaras cargan con una (`camaras_defecto`).
    #[serde(default = "camaras_defecto")]
    pub camaras: Vec<CamaraEscena>,
}

/// Sala con la que arranca un montaje nuevo.
fn sala_defecto() -> Sala {
    Sala {
        ancho: 6.0,
        largo: 6.0,
        alto_techo: 4.0,
        altura_carriles: 3.8,
        refl_suelo: refl_suelo_defecto(),
        paredes: vec![Acabado { color: color_paredes_defecto(), reflexion: refl_paredes_defecto() }; 4],
        suelo: Some(acabado_migrado(refl_suelo_defecto())),
        refl_techo: refl_techo_defecto(),
        techo: Some(acabado_migrado(refl_techo_defecto())),
        refl_paredes: refl_paredes_defecto(),
        color_paredes: color_paredes_defecto(),
    }
}

/// Montajes sin cámaras en el archivo: una, CAM1, en la sala por defecto.
fn camaras_defecto() -> Vec<CamaraEscena> {
    vec![CamaraEscena::nueva("CAM1".to_string(), &sala_defecto())]
}

impl Pared {
    pub const TODAS: [Pared; 4] = [Pared::Norte, Pared::Sur, Pared::Este, Pared::Oeste];

    pub fn nombre(self) -> &'static str {
        match self {
            Pared::Norte => "Norte",
            Pared::Sur => "Sur",
            Pared::Este => "Este",
            Pared::Oeste => "Oeste",
        }
    }

    pub fn largo(self, s: &Sala) -> f32 {
        match self {
            Pared::Sur | Pared::Norte => s.ancho,
            Pared::Este | Pared::Oeste => s.largo,
        }
    }

    pub fn punto(self, s: &Sala, pos: f32, nivel: NivelToma) -> [f32; 3] {
        let h = match nivel {
            NivelToma::Carriles => s.altura_carriles,
            NivelToma::Suelo => ALTURA_TOMA_SUELO,
        };
        match self {
            Pared::Sur => [pos, h, 0.0],
            Pared::Norte => [pos, h, s.largo],
            Pared::Oeste => [0.0, h, pos],
            Pared::Este => [s.ancho, h, pos],
        }
    }

    pub fn eje(self) -> [f32; 3] {
        match self {
            Pared::Sur | Pared::Norte => [1.0, 0.0, 0.0],
            Pared::Este | Pared::Oeste => [0.0, 0.0, 1.0],
        }
    }
}

impl Default for Montaje {
    fn default() -> Self {
        let sala = sala_defecto();
        let carriles = (1..=3)
            .map(|i| Carril {
                id: format!("C{i}"),
                x: sala.ancho * i as f32 / 4.0,
            })
            .collect();
        // Ejemplo para ver la escala: mesa, silla enfrentada (frente = +Z local) y maniquí.
        // Solo los montajes nuevos; cargar desde disco usa serde y no pasa por aquí.
        let objetos = [
            (TipoObjeto::Mesa, 3.0, 3.0, 0.0),
            (TipoObjeto::Silla, 3.0, 3.75, 180.0),
            (TipoObjeto::Maniqui, 4.2, 2.0, 0.0),
        ]
        .into_iter()
        .map(|(tipo, x, z, rot)| {
            let (ancho, fondo, alto) = tipo.medidas();
            Objeto {
                nombre: format!("{} 1", tipo.nombre()),
                tipo,
                x,
                z,
                rot,
                ancho,
                fondo,
                alto,
                marca: String::new(),
                modelo: String::new(),
                categoria: categoria_de_tipo(tipo).to_string(),
                malla: None,
                piel: None,
            }
        })
        .collect();
        // Luz de techo en C2 apuntada a la mesa (3, 0.75, 3) desde detrás, y una de piso junto a ella.
        let carril_x = sala.ancho * 2.0 / 4.0;
        let z_techo = 5.0;
        let (giro, inclinacion) = apuntar([carril_x, sala.altura_carriles - 0.25, z_techo], [3.0, 0.75, 3.0]);
        let specs_techo = TipoLuz::FresnelLed.specs();
        let techo = Luz {
            codigo: "EJ-T1".to_string(),
            marca: "Ejemplo".to_string(),
            modelo: "Fresnel LED 150W".to_string(),
            carril: "C2".to_string(),
            z: z_techo,
            toma: None,
            specs: specs_techo.clone(),
            ajustes: AjustesLuz {
                dimmer: 0.8,
                giro,
                inclinacion,
                ..AjustesLuz::por_defecto(&specs_techo, false)
            },
        };
        let specs_piso = TipoLuz::MonoluzSoftbox.specs();
        let (x, z) = (1.8, 3.0);
        let (giro, inclinacion) = apuntar([x, ALTURA_PISO_DEFECTO, z], [3.0, 0.75, 3.0]);
        let piso = LuzPiso {
            codigo: "EJ-P1".to_string(),
            marca: "Ejemplo".to_string(),
            modelo: "Monoluz COB + softbox".to_string(),
            specs: specs_piso.clone(),
            ajustes: AjustesLuz { dimmer: 0.7, giro, inclinacion, ..AjustesLuz::por_defecto(&specs_piso, true) },
            x,
            z,
            altura: ALTURA_PISO_DEFECTO,
            toma: None,
        };
        Montaje {
            sala,
            carriles,
            tomas: Vec::new(),
            luces: vec![techo],
            luces_piso: vec![piso],
            objetos,
            modelos: Default::default(),
            acabados: Default::default(),
            fotometrias: Default::default(),
            camaras: camaras_defecto(),
        }
    }
}

/// Recorta a [0, hi]. Usa max/min (no clamp) para no entrar en pánico con NaN.
fn limita(v: f32, hi: f32) -> f32 {
    v.max(0.0).min(hi)
}

/// Código sin espacios; vacío es error.
fn limpio(s: &str) -> Result<String, String> {
    let t = s.trim();
    if t.is_empty() {
        Err("El código no puede estar vacío".to_string())
    } else {
        Ok(t.to_string())
    }
}

/// Primer id `formato(n)` con n >= 1 que `usado` no reconoce.
fn primer_id_libre(usado: impl Fn(&str) -> bool, formato: impl Fn(u32) -> String) -> String {
    (1u32..).map(formato).find(|id| !usado(id)).unwrap_or_default()
}

fn quitar_repetidos<T>(
    v: &mut Vec<T>,
    clave: impl Fn(&T) -> String,
    que: &str,
    avisos: &mut Vec<String>,
) {
    let mut vistos = std::collections::HashSet::new();
    v.retain(|x| {
        let k = clave(x);
        if vistos.insert(k.clone()) {
            true
        } else {
            avisos.push(format!("{que} {k} repetido: eliminado"));
            false
        }
    });
}

impl Montaje {
    pub fn ajustar_a_sala(&mut self) {
        self.sala.completar_acabados();
        let s = &mut self.sala;
        // Tope de 100 m: sin él, una sala enorme cargada de un archivo bloquea la rejilla de la escena.
        s.ancho = s.ancho.max(1.0).min(100.0);
        s.largo = s.largo.max(1.0).min(100.0);
        s.alto_techo = s.alto_techo.max(1.0).min(100.0);
        s.altura_carriles = s.altura_carriles.max(0.5).min(s.alto_techo);

        let sala = self.sala.clone();
        for c in &mut self.carriles {
            c.x = limita(c.x, sala.ancho);
        }
        for l in &mut self.luces {
            l.z = limita(l.z, sala.largo);
            l.ajustes.limitar(&l.specs);
        }
        for l in &mut self.luces_piso {
            l.x = limita(l.x, sala.ancho);
            l.z = limita(l.z, sala.largo);
            l.altura = l.altura.max(ALTURA_PISO_MIN).min(ALTURA_PISO_MAX);
            l.ajustes.limitar(&l.specs);
        }
        for t in &mut self.tomas {
            t.pos = limita(t.pos, t.pared.largo(&sala));
        }
        for o in &mut self.objetos {
            o.x = limita(o.x, sala.ancho);
            o.z = limita(o.z, sala.largo);
            o.ancho = o.ancho.max(0.05).min(20.0);
            o.fondo = o.fondo.max(0.05).min(20.0);
            o.alto = o.alto.max(0.05).min(sala.alto_techo);
            // Con NaN, rem_euclid da NaN: la comparación lo manda a 0; 360 (por redondeo) también.
            let r = o.rot.rem_euclid(360.0);
            o.rot = if r < 360.0 { r } else { 0.0 };
        }
        for c in &mut self.camaras {
            c.limitar();
            c.x = limita(c.x, sala.ancho);
            c.z = limita(c.z, sala.largo);
            c.altura = c.altura.min(sala.alto_techo);
        }
    }

    pub fn anadir_objeto(&mut self, tipo: TipoObjeto) -> String {
        let nombre = primer_id_libre(
            |id| self.objetos.iter().any(|o| o.nombre == id) || self.hay_luz(id),
            |n| format!("{} {n}", tipo.nombre()),
        );
        let (ancho, fondo, alto) = tipo.medidas();
        self.objetos.push(Objeto {
            nombre: nombre.clone(),
            tipo,
            x: self.sala.ancho / 2.0,
            z: self.sala.largo / 2.0,
            rot: 0.0,
            ancho,
            fondo,
            alto,
            marca: String::new(),
            modelo: String::new(),
            categoria: categoria_de_tipo(tipo).to_string(),
            malla: None,
            piel: None,
        });
        nombre
    }

    /// Cambia la categoría; si cambia la forma, el objeto toma las medidas del nuevo tipo.
    pub fn cambiar_categoria_objeto(&mut self, nombre: &str, cat: &str) -> Result<(), String> {
        let nombre = nombre.trim();
        let cat = cat.trim();
        if cat.is_empty() {
            return Err("La categoría no puede estar vacía".to_string());
        }
        if es_categoria_luz(cat) {
            return Err(format!("Las luces no son objetos: no se puede poner la categoría {cat}"));
        }
        let i = self
            .objetos
            .iter()
            .position(|o| o.nombre == nombre)
            .ok_or_else(|| format!("No existe el objeto {nombre}"))?;
        let tipo = tipo_para_categoria(cat);
        let o = &mut self.objetos[i];
        if tipo != o.tipo {
            (o.ancho, o.fondo, o.alto) = tipo.medidas();
        }
        o.tipo = tipo;
        o.categoria = cat.to_string();
        self.ajustar_a_sala();
        Ok(())
    }

    /// Cambia la forma (no la categoría); si cambia, el objeto toma las medidas de la nueva.
    pub fn cambiar_forma_objeto(&mut self, nombre: &str, tipo: TipoObjeto) -> Result<(), String> {
        let nombre = nombre.trim();
        let i = self
            .objetos
            .iter()
            .position(|o| o.nombre == nombre)
            .ok_or_else(|| format!("No existe el objeto {nombre}"))?;
        let o = &mut self.objetos[i];
        if tipo != o.tipo {
            (o.ancho, o.fondo, o.alto) = tipo.medidas();
            o.tipo = tipo;
        }
        self.ajustar_a_sala();
        Ok(())
    }

    pub fn borrar_objeto(&mut self, nombre: &str) {
        let nombre = nombre.trim();
        self.objetos.retain(|o| o.nombre != nombre);
    }

    pub fn renombrar_objeto(&mut self, viejo: &str, nuevo: &str) -> Result<(), String> {
        let viejo = viejo.trim();
        let nuevo = limpio(nuevo)?;
        if self.hay_luz(&nuevo) {
            return Err(format!("Ya existe una luz {nuevo}"));
        }
        let i = self
            .objetos
            .iter()
            .position(|o| o.nombre == viejo)
            .ok_or_else(|| format!("No existe el objeto {viejo}"))?;
        if nuevo != viejo && self.objetos.iter().any(|o| o.nombre == nuevo) {
            return Err(format!("Ya existe un objeto {nuevo}"));
        }
        self.objetos[i].nombre = nuevo;
        Ok(())
    }

    pub fn anadir_camara(&mut self) -> String {
        let nombre = primer_id_libre(
            |id| self.camaras.iter().any(|c| c.nombre == id),
            |n| format!("CAM{n}"),
        );
        self.camaras.push(CamaraEscena::nueva(nombre.clone(), &self.sala));
        nombre
    }

    pub fn borrar_camara(&mut self, nombre: &str) {
        let nombre = nombre.trim();
        self.camaras.retain(|c| c.nombre != nombre);
    }

    pub fn renombrar_camara(&mut self, viejo: &str, nuevo: &str) -> Result<(), String> {
        let viejo = viejo.trim();
        let nuevo = limpio(nuevo)?;
        let i = self
            .camaras
            .iter()
            .position(|c| c.nombre == viejo)
            .ok_or_else(|| format!("No existe la cámara {viejo}"))?;
        if nuevo != viejo && self.camaras.iter().any(|c| c.nombre == nuevo) {
            return Err(format!("Ya existe una cámara {nuevo}"));
        }
        self.camaras[i].nombre = nuevo;
        Ok(())
    }

    pub fn anadir_carril(&mut self) -> String {
        let id = primer_id_libre(
            |id| self.carriles.iter().any(|c| c.id == id),
            |n| format!("C{n}"),
        );
        self.carriles.push(Carril {
            id: id.clone(),
            x: self.sala.ancho / 2.0,
        });
        id
    }

    pub fn borrar_carril(&mut self, id: &str) -> Result<(), String> {
        let id = id.trim();
        if self.luces.iter().any(|l| l.carril == id) {
            return Err(format!(
                "No se puede borrar el carril {id}: tiene luces. Muévelas o quítalas antes"
            ));
        }
        self.carriles.retain(|c| c.id != id);
        Ok(())
    }

    pub fn renombrar_carril(&mut self, viejo: &str, nuevo: &str) -> Result<(), String> {
        let viejo = viejo.trim();
        let nuevo = limpio(nuevo)?;
        let i = self
            .carriles
            .iter()
            .position(|c| c.id == viejo)
            .ok_or_else(|| format!("No existe el carril {viejo}"))?;
        if nuevo != viejo && self.carriles.iter().any(|c| c.id == nuevo) {
            return Err(format!("Ya existe un carril {nuevo}"));
        }
        for l in &mut self.luces {
            if l.carril == viejo {
                l.carril = nuevo.clone();
            }
        }
        self.carriles[i].id = nuevo;
        Ok(())
    }

    pub fn anadir_toma(&mut self) -> String {
        let codigo = primer_id_libre(
            |c| self.tomas.iter().any(|t| t.codigo == c),
            |n| format!("T-{n:02}"),
        );
        self.tomas.push(Toma {
            codigo: codigo.clone(),
            pared: Pared::Sur,
            pos: self.sala.ancho / 2.0,
            nivel: NivelToma::Carriles,
        });
        codigo
    }

    /// Posición 3D de la toma (en la pared, a la altura de su nivel).
    pub fn punto_toma(&self, t: &Toma) -> [f32; 3] {
        t.pared.punto(&self.sala, t.pos, t.nivel)
    }

    pub fn borrar_toma(&mut self, codigo: &str) {
        let codigo = codigo.trim();
        self.tomas.retain(|t| t.codigo != codigo);
        for l in &mut self.luces {
            if l.toma.as_deref() == Some(codigo) {
                l.toma = None;
            }
        }
        for l in &mut self.luces_piso {
            if l.toma.as_deref() == Some(codigo) {
                l.toma = None;
            }
        }
    }

    pub fn renombrar_toma(&mut self, viejo: &str, nuevo: &str) -> Result<(), String> {
        let viejo = viejo.trim();
        let nuevo = limpio(nuevo)?;
        let i = self
            .tomas
            .iter()
            .position(|t| t.codigo == viejo)
            .ok_or_else(|| format!("No existe la toma {viejo}"))?;
        if nuevo != viejo && self.tomas.iter().any(|t| t.codigo == nuevo) {
            return Err(format!("Ya existe una toma {nuevo}"));
        }
        for l in &mut self.luces {
            if l.toma.as_deref() == Some(viejo) {
                l.toma = Some(nuevo.clone());
            }
        }
        for l in &mut self.luces_piso {
            if l.toma.as_deref() == Some(viejo) {
                l.toma = Some(nuevo.clone());
            }
        }
        self.tomas[i].codigo = nuevo;
        Ok(())
    }

    /// Hay una luz (de techo o de piso) con ese código.
    fn hay_luz(&self, codigo: &str) -> bool {
        self.luces.iter().any(|l| l.codigo == codigo) || self.luces_piso.iter().any(|l| l.codigo == codigo)
    }

    /// Hay una luz o un objeto con ese código.
    pub fn equipo_colocado(&self, codigo: &str) -> bool {
        let codigo = codigo.trim();
        self.hay_luz(codigo) || self.objetos.iter().any(|o| o.nombre == codigo)
    }

    pub fn colocar_luz(&mut self, e: &Equipo, carril: &str) -> Result<(), String> {
        let codigo = limpio(&e.codigo)?;
        let carril = carril.trim();
        if self.equipo_colocado(&codigo) {
            return Err(format!("El equipo {codigo} ya está colocado en el montaje"));
        }
        if !self.carriles.iter().any(|c| c.id == carril) {
            return Err(format!("No existe el carril {carril}"));
        }
        let specs = e.specs.clone().unwrap_or_else(|| TipoLuz::PanelLed.specs());
        self.luces.push(Luz {
            codigo,
            marca: e.marca.clone(),
            modelo: e.modelo.clone(),
            carril: carril.to_string(),
            z: self.sala.largo / 2.0,
            toma: None,
            specs: specs.clone(),
            ajustes: AjustesLuz::por_defecto(&specs, false),
        });
        Ok(())
    }

    /// Pone una luz de piso en el centro de la sala, a la altura por defecto.
    pub fn colocar_luz_piso(&mut self, e: &Equipo) -> Result<(), String> {
        let codigo = limpio(&e.codigo)?;
        if self.equipo_colocado(&codigo) {
            return Err(format!("El equipo {codigo} ya está colocado en el montaje"));
        }
        let specs = e.specs.clone().unwrap_or_else(|| TipoLuz::PanelPie.specs());
        self.luces_piso.push(LuzPiso {
            codigo,
            marca: e.marca.clone(),
            modelo: e.modelo.clone(),
            specs: specs.clone(),
            ajustes: AjustesLuz::por_defecto(&specs, true),
            x: self.sala.ancho / 2.0,
            z: self.sala.largo / 2.0,
            altura: ALTURA_PISO_DEFECTO,
            toma: None,
        });
        Ok(())
    }

    /// Lleva marca, modelo y especificaciones de un equipo del inventario a la luz colocada con ese código
    /// (de techo o de piso). Devuelve true si cambió algo. Los ajustes se limitan al nuevo rango.
    pub fn actualizar_equipo(&mut self, codigo: &str, e: &Equipo) -> bool {
        if let Some(l) = self.luces.iter_mut().find(|l| l.codigo == codigo) {
            return Self::aplica_equipo(&mut l.marca, &mut l.modelo, &mut l.specs, &mut l.ajustes, e);
        }
        match self.luces_piso.iter_mut().find(|l| l.codigo == codigo) {
            Some(l) => Self::aplica_equipo(&mut l.marca, &mut l.modelo, &mut l.specs, &mut l.ajustes, e),
            None => false,
        }
    }

    fn aplica_equipo(marca: &mut String, modelo: &mut String, specs: &mut SpecsLuz, ajustes: &mut AjustesLuz, e: &Equipo) -> bool {
        let antes = (marca.clone(), modelo.clone(), specs.clone(), *ajustes);
        *marca = e.marca.clone();
        *modelo = e.modelo.clone();
        if let Some(s) = &e.specs {
            *specs = s.clone();
        }
        ajustes.limitar(specs);
        antes != (marca.clone(), modelo.clone(), specs.clone(), *ajustes)
    }

    pub fn quitar_luz_piso(&mut self, codigo: &str) {
        let codigo = codigo.trim();
        self.luces_piso.retain(|l| l.codigo != codigo);
    }

    /// Pone el equipo de inventario como objeto, con la forma de su categoría.
    pub fn colocar_objeto(&mut self, e: &Equipo) -> Result<(), String> {
        let codigo = limpio(&e.codigo)?;
        if self.equipo_colocado(&codigo) {
            return Err(format!("El equipo {codigo} ya está colocado en el montaje"));
        }
        let tipo = tipo_para_categoria(&e.categoria);
        let (ancho, fondo, alto) = tipo.medidas();
        self.objetos.push(Objeto {
            nombre: codigo,
            tipo,
            x: self.sala.ancho / 2.0,
            z: self.sala.largo / 2.0,
            rot: 0.0,
            ancho,
            fondo,
            alto,
            marca: e.marca.clone(),
            modelo: e.modelo.clone(),
            categoria: e.categoria.clone(),
            malla: None,
            piel: None,
        });
        Ok(())
    }

    /// Pone un modelo importado como objeto (la copia en `modelos` si falta) y devuelve su nombre.
    pub fn colocar_malla(&mut self, m: &Malla) -> String {
        self.modelos.entry(m.nombre.clone()).or_insert_with(|| m.clone());
        let nombre = primer_id_libre(
            |id| self.objetos.iter().any(|o| o.nombre == id) || self.hay_luz(id),
            |n| format!("{} {n}", m.nombre),
        );
        self.objetos.push(Objeto {
            nombre: nombre.clone(),
            tipo: TipoObjeto::Caja,
            x: self.sala.ancho / 2.0,
            z: self.sala.largo / 2.0,
            rot: 0.0,
            ancho: m.tam[0],
            fondo: m.tam[2],
            alto: m.tam[1],
            marca: String::new(),
            modelo: String::new(),
            categoria: String::new(),
            malla: Some(m.nombre.clone()),
            piel: None,
        });
        self.ajustar_a_sala();
        nombre
    }

    pub fn quitar_luz(&mut self, codigo: &str) {
        let codigo = codigo.trim();
        self.luces.retain(|l| l.codigo != codigo);
    }

    /// Conecta una luz de techo a una toma de carriles (None la desconecta).
    pub fn conectar(&mut self, luz: &str, toma: Option<&str>) -> Result<(), String> {
        let luz = luz.trim();
        if !self.luces.iter().any(|l| l.codigo == luz) {
            return Err(format!("No existe la luz {luz}"));
        }
        let nueva = self.toma_para(luz, toma, NivelToma::Carriles)?;
        if let Some(l) = self.luces.iter_mut().find(|l| l.codigo == luz) {
            l.toma = nueva;
        }
        Ok(())
    }

    /// Conecta una luz de piso a una toma de suelo (None la desconecta).
    pub fn conectar_piso(&mut self, luz: &str, toma: Option<&str>) -> Result<(), String> {
        let luz = luz.trim();
        if !self.luces_piso.iter().any(|l| l.codigo == luz) {
            return Err(format!("No existe la luz de piso {luz}"));
        }
        let nueva = self.toma_para(luz, toma, NivelToma::Suelo)?;
        if let Some(l) = self.luces_piso.iter_mut().find(|l| l.codigo == luz) {
            l.toma = nueva;
        }
        Ok(())
    }

    /// Cambia la altura de una toma; las luces que la usan y no son de ese nivel se desconectan.
    pub fn cambiar_nivel_toma(&mut self, codigo: &str, nivel: NivelToma) {
        let codigo = codigo.trim();
        let Some(t) = self.tomas.iter_mut().find(|t| t.codigo == codigo) else { return };
        t.nivel = nivel;
        for l in &mut self.luces {
            if l.toma.as_deref() == Some(codigo) && nivel != NivelToma::Carriles {
                l.toma = None;
            }
        }
        for l in &mut self.luces_piso {
            if l.toma.as_deref() == Some(codigo) && nivel != NivelToma::Suelo {
                l.toma = None;
            }
        }
    }

    /// Toma válida para esa luz: existe, es de su nivel y no la usa otra luz.
    fn toma_para(&self, luz: &str, toma: Option<&str>, nivel: NivelToma) -> Result<Option<String>, String> {
        let Some(t) = toma.map(str::trim) else { return Ok(None) };
        let Some(tm) = self.tomas.iter().find(|x| x.codigo == t) else {
            return Err(format!("No existe la toma {t}"));
        };
        if tm.nivel != nivel {
            return Err(format!(
                "La toma {t} es de {} y esta luz necesita una de {}",
                tm.nivel.nombre(),
                nivel.nombre()
            ));
        }
        if let Some(otra) = self.luz_con_toma(t, luz) {
            return Err(format!("La toma {t} ya la usa la luz {otra}"));
        }
        Ok(Some(t.to_string()))
    }

    /// Luz de techo o de piso, distinta de `excepto`, que usa la toma.
    fn luz_con_toma(&self, toma: &str, excepto: &str) -> Option<&str> {
        self.luces
            .iter()
            .map(|l| (&l.codigo, &l.toma))
            .chain(self.luces_piso.iter().map(|l| (&l.codigo, &l.toma)))
            .find(|(c, t)| c.as_str() != excepto && t.as_deref() == Some(toma))
            .map(|(c, _)| c.as_str())
    }

    /// Tomas que la luz puede usar: de su nivel (carriles para techo, suelo para piso) y libres.
    pub fn tomas_libres_para(&self, luz: &str) -> Vec<String> {
        let luz = luz.trim();
        let nivel = if self.luces_piso.iter().any(|l| l.codigo == luz) {
            NivelToma::Suelo
        } else {
            NivelToma::Carriles
        };
        self.tomas
            .iter()
            .filter(|t| t.nivel == nivel && self.luz_con_toma(&t.codigo, luz).is_none())
            .map(|t| t.codigo.clone())
            .collect()
    }

    /// Arregla un montaje cargado y devuelve un aviso por cada cambio.
    pub fn reparar(&mut self) -> Vec<String> {
        let mut avisos = Vec::new();
        for c in &mut self.carriles {
            c.id = c.id.trim().to_string();
        }
        for t in &mut self.tomas {
            t.codigo = t.codigo.trim().to_string();
        }
        for l in &mut self.luces {
            l.codigo = l.codigo.trim().to_string();
            l.carril = l.carril.trim().to_string();
            l.toma = l.toma.take().map(|t| t.trim().to_string());
        }
        for l in &mut self.luces_piso {
            l.codigo = l.codigo.trim().to_string();
            l.toma = l.toma.take().map(|t| t.trim().to_string());
        }
        for c in &mut self.camaras {
            c.nombre = c.nombre.trim().to_string();
        }
        for o in &mut self.objetos {
            o.nombre = o.nombre.trim().to_string();
            // Los montajes anteriores a las categorías no la guardaban.
            if o.categoria.is_empty() && o.tipo != TipoObjeto::Caja {
                o.categoria = categoria_de_tipo(o.tipo).to_string();
            }
            o.malla = o.malla.take().map(|m| m.trim().to_string()).filter(|m| !m.is_empty());
        }
        // Un modelo dañado no se puede dibujar: se quita y sus objetos quedan como caja.
        for (nombre, m) in std::mem::take(&mut self.modelos) {
            match m.valida() {
                Ok(()) => {
                    self.modelos.insert(nombre, m);
                }
                Err(e) => avisos.push(format!("Modelo {nombre} dañado ({e}): eliminado")),
            }
        }

        let n = self.carriles.len();
        self.carriles.retain(|c| !c.id.is_empty());
        avisos.extend((self.carriles.len()..n).map(|_| "Carril sin id: eliminado".to_string()));
        let n = self.tomas.len();
        self.tomas.retain(|t| !t.codigo.is_empty());
        avisos.extend((self.tomas.len()..n).map(|_| "Toma sin código: eliminada".to_string()));
        let n = self.luces.len();
        self.luces.retain(|l| !l.codigo.is_empty());
        avisos.extend((self.luces.len()..n).map(|_| "Luz sin código: eliminada".to_string()));
        let n = self.luces_piso.len();
        self.luces_piso.retain(|l| !l.codigo.is_empty());
        avisos.extend((self.luces_piso.len()..n).map(|_| "Luz de piso sin código: eliminada".to_string()));
        let n = self.objetos.len();
        self.objetos.retain(|o| !o.nombre.is_empty());
        avisos.extend((self.objetos.len()..n).map(|_| "Objeto sin nombre: eliminado".to_string()));
        let n = self.camaras.len();
        self.camaras.retain(|c| !c.nombre.is_empty());
        avisos.extend((self.camaras.len()..n).map(|_| "Cámara sin nombre: eliminada".to_string()));

        quitar_repetidos(&mut self.carriles, |c| c.id.clone(), "Carril", &mut avisos);
        quitar_repetidos(&mut self.tomas, |t| t.codigo.clone(), "Toma", &mut avisos);
        quitar_repetidos(&mut self.luces, |l| l.codigo.clone(), "Luz", &mut avisos);
        // Una luz de piso con el código de una de techo se quita; la de techo es la que cuenta.
        let techo: std::collections::HashSet<String> = self.luces.iter().map(|l| l.codigo.clone()).collect();
        self.luces_piso.retain(|l| {
            let repetida = techo.contains(&l.codigo);
            if repetida {
                avisos.push(format!("Luz de piso {} repetida: eliminada", l.codigo));
            }
            !repetida
        });
        quitar_repetidos(&mut self.luces_piso, |l| l.codigo.clone(), "Luz de piso", &mut avisos);
        quitar_repetidos(&mut self.objetos, |o| o.nombre.clone(), "Objeto", &mut avisos);
        quitar_repetidos(&mut self.camaras, |c| c.nombre.clone(), "Cámara", &mut avisos);

        // Especificaciones con NaN (archivo editado a mano): se usan las típicas de su tipo.
        for l in &mut self.luces {
            if !l.specs.valido() {
                l.specs = TipoLuz::PanelLed.specs();
                avisos.push(format!("La luz {} tenía especificaciones no válidas: se usan las de Panel LED", l.codigo));
            }
        }
        for l in &mut self.luces_piso {
            if !l.specs.valido() {
                l.specs = TipoLuz::PanelPie.specs();
                avisos.push(format!("La luz de piso {} tenía especificaciones no válidas: se usan las de Panel LED en pie", l.codigo));
            }
        }

        for i in 0..self.luces.len() {
            if !self.luces[i].carril.is_empty() {
                continue;
            }
            if self.carriles.is_empty() {
                self.anadir_carril();
            }
            let id = self.carriles[0].id.clone();
            avisos.push(format!("La luz {} no tenía carril: se pasa a {id}", self.luces[i].codigo));
            self.luces[i].carril = id;
        }

        for i in 0..self.luces.len() {
            let carril = self.luces[i].carril.clone();
            if !self.carriles.iter().any(|c| c.id == carril) {
                self.carriles.push(Carril {
                    id: carril.clone(),
                    x: self.sala.ancho / 2.0,
                });
                avisos.push(format!("Carril {carril} creado para la luz {}", self.luces[i].codigo));
            }
        }

        let tomas_ok: std::collections::HashMap<String, NivelToma> =
            self.tomas.iter().map(|t| (t.codigo.clone(), t.nivel)).collect();
        let mut usadas = std::collections::HashSet::new();
        let mut revisa = |codigo: &str, toma: &mut Option<String>, nivel: NivelToma, avisos: &mut Vec<String>| {
            let Some(t) = toma.clone() else { return };
            if t.is_empty() {
                avisos.push(format!("La luz {codigo} tenía una toma vacía: queda sin toma"));
                *toma = None;
            } else if !tomas_ok.contains_key(&t) {
                avisos.push(format!("La luz {codigo} tenía la toma {t}, que no existe: queda sin toma"));
                *toma = None;
            } else if tomas_ok[&t] != nivel {
                avisos.push(format!("La luz {codigo} necesita una toma de {}: la toma {t} no lo es, queda sin toma", nivel.nombre()));
                *toma = None;
            } else if !usadas.insert(t.clone()) {
                avisos.push(format!("La toma {t} ya la usaba otra luz: la luz {codigo} queda sin toma"));
                *toma = None;
            }
        };
        for l in &mut self.luces {
            revisa(&l.codigo, &mut l.toma, NivelToma::Carriles, &mut avisos);
        }
        for l in &mut self.luces_piso {
            revisa(&l.codigo, &mut l.toma, NivelToma::Suelo, &mut avisos);
        }

        self.ajustar_a_sala();
        avisos
    }

    pub fn cargar(ruta: &std::path::Path) -> Result<(Montaje, Vec<String>), String> {
        let texto = std::fs::read_to_string(ruta)
            .map_err(|e| format!("No se pudo leer {}: {e}", ruta.display()))?;
        // Notepad y PowerShell pueden añadir un BOM UTF-8 que serde_json no acepta.
        let texto = texto.strip_prefix('\u{feff}').unwrap_or(&texto);
        let mut m: Montaje = serde_json::from_str(texto)
            .map_err(|e| format!("El archivo no es un montaje válido: {e}"))?;
        let avisos = m.reparar();
        Ok((m, avisos))
    }

    pub fn guardar(&self, ruta: &std::path::Path) -> Result<(), String> {
        // Solo los modelos que usa algún objeto; el montaje en memoria no cambia.
        let usados: std::collections::HashSet<&str> =
            self.objetos.iter().filter_map(|o| o.malla.as_deref()).collect();
        let mut copia = self.clone();
        copia.modelos.retain(|n, _| usados.contains(n.as_str()));
        // Solo los presets que alguna pared tiene asignado.
        let usados: Vec<Acabado> = Superficie::TODAS.iter().map(|&s| self.sala.acabado_de(s)).collect();
        copia.acabados.retain(|_, a| usados.contains(a));
        // Solo los perfiles IES que alguna luz usa.
        let ies: std::collections::HashSet<&str> = self
            .luces
            .iter()
            .map(|l| l.specs.ies.as_deref())
            .chain(self.luces_piso.iter().map(|l| l.specs.ies.as_deref()))
            .flatten()
            .collect();
        copia.fotometrias.retain(|n, _| ies.contains(n.as_str()));
        let json = serde_json::to_string_pretty(&copia)
            .map_err(|e| format!("No se pudo preparar el montaje: {e}"))?;
        // Escribe a un temporal y lo renombra encima: si falla, el original queda intacto.
        let mut tmp = ruta.as_os_str().to_owned();
        tmp.push(".tmp");
        let tmp = std::path::PathBuf::from(tmp);
        std::fs::write(&tmp, json)
            .map_err(|e| format!("No se pudo guardar {}: {e}", ruta.display()))?;
        std::fs::rename(&tmp, ruta).map_err(|e| {
            std::fs::remove_file(&tmp).ok();
            format!("No se pudo guardar {}: {e}", ruta.display())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn equipo(codigo: &str) -> Equipo {
        equipo_en(codigo, CATEGORIA_LUCES)
    }

    fn equipo_en(codigo: &str, categoria: &str) -> Equipo {
        Equipo {
            codigo: codigo.to_string(),
            marca: "Aputure".to_string(),
            modelo: "300d".to_string(),
            categoria: categoria.to_string(),
            specs: None,
        }
    }

    fn ruta_temp(nombre: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("sala_luces_{nombre}_{}.json", std::process::id()))
    }

    #[test]
    fn default_tiene_tres_carriles() {
        let m = Montaje::default();
        let ids: Vec<&str> = m.carriles.iter().map(|c| c.id.as_str()).collect();
        let xs: Vec<f32> = m.carriles.iter().map(|c| c.x).collect();
        assert_eq!(ids, ["C1", "C2", "C3"]);
        assert_eq!(xs, [1.5, 3.0, 4.5]);
        assert_eq!(
            m.sala,
            Sala {
                ancho: 6.0,
                largo: 6.0,
                alto_techo: 4.0,
                altura_carriles: 3.8,
                refl_suelo: 0.2,
                paredes: vec![Acabado { color: [188, 188, 188], reflexion: 0.5 }; 4],
                suelo: Some(Acabado { color: [188, 188, 188], reflexion: 0.2 }),
                refl_techo: 0.7,
                techo: Some(Acabado { color: [188, 188, 188], reflexion: 0.7 }),
                refl_paredes: 0.5,
                color_paredes: [188, 188, 188],
            }
        );
    }

    #[test]
    fn anadir_carril_usa_primer_id_libre() {
        let mut m = Montaje::default();
        m.luces.clear();
        m.borrar_carril("C2").unwrap();
        assert_eq!(m.anadir_carril(), "C2");
        let c = m.carriles.iter().find(|c| c.id == "C2").unwrap();
        assert_eq!(c.x, 3.0);
    }

    #[test]
    fn no_borra_carril_con_luces() {
        let mut m = Montaje::default();
        m.colocar_luz(&equipo("L1"), "C1").unwrap();
        assert!(m.borrar_carril("C1").is_err());
        assert!(m.carriles.iter().any(|c| c.id == "C1"));
    }

    #[test]
    fn renombrar_carril_actualiza_luces() {
        let mut m = Montaje::default();
        m.luces.clear();
        m.colocar_luz(&equipo("L1"), "C1").unwrap();
        m.renombrar_carril("C1", "A").unwrap();
        assert_eq!(m.luces[0].carril, "A");
        assert!(m.carriles.iter().any(|c| c.id == "A"));
    }

    #[test]
    fn renombrar_carril_rechaza_vacio_o_duplicado() {
        let mut m = Montaje::default();
        let antes = m.clone();
        assert!(m.renombrar_carril("C1", "").is_err());
        assert!(m.renombrar_carril("C1", "  ").is_err());
        assert!(m.renombrar_carril("C1", "C2").is_err());
        assert_eq!(m, antes);
    }

    #[test]
    fn renombrar_toma_actualiza_luces() {
        let mut m = Montaje::default();
        m.luces.clear();
        m.anadir_toma();
        m.colocar_luz(&equipo("L1"), "C1").unwrap();
        m.conectar("L1", Some("T-01")).unwrap();
        m.renombrar_toma("T-01", "P1").unwrap();
        assert_eq!(m.luces[0].toma.as_deref(), Some("P1"));
        assert!(m.tomas.iter().any(|t| t.codigo == "P1"));
    }

    #[test]
    fn renombrar_toma_rechaza_vacio_o_duplicado() {
        let mut m = Montaje::default();
        m.anadir_toma();
        m.anadir_toma();
        let antes = m.clone();
        assert!(m.renombrar_toma("T-01", "").is_err());
        assert!(m.renombrar_toma("T-01", "  ").is_err());
        assert!(m.renombrar_toma("T-01", "T-02").is_err());
        assert_eq!(m, antes);
    }

    #[test]
    fn colocar_luz_copia_datos_y_rechaza_duplicado() {
        let mut m = Montaje::default();
        m.luces.clear();
        m.colocar_luz(&equipo("L1"), "C1").unwrap();
        let l = &m.luces[0];
        assert_eq!(l.z, 3.0);
        assert_eq!(l.marca, "Aputure");
        assert_eq!(l.modelo, "300d");
        assert!(m.colocar_luz(&equipo(" L1 "), "C1").is_err());
    }

    #[test]
    fn actualizar_equipo_lleva_datos_y_limita_ajustes() {
        let mut m = Montaje::default();
        m.luces.clear();
        m.colocar_luz(&equipo("L1"), "C1").unwrap();
        m.luces[0].ajustes.apertura = 1000.0;
        let (z, carril) = (m.luces[0].z, m.luces[0].carril.clone());
        let s = TipoLuz::KinoFloCeleb850.specs();
        let nuevo = Equipo { marca: "Nueva".into(), modelo: "X2".into(), specs: Some(s.clone()), ..equipo("L1") };
        assert!(m.actualizar_equipo("L1", &nuevo));
        let l = &m.luces[0];
        assert_eq!((l.codigo.as_str(), l.marca.as_str(), l.modelo.as_str()), ("L1", "Nueva", "X2"));
        assert_eq!((l.z, l.carril.as_str()), (z, carril.as_str()));
        assert_eq!(l.specs, s);
        assert_eq!(l.ajustes.apertura, s.apertura_max.max(s.apertura_min));
        assert!(!m.actualizar_equipo("L1", &nuevo));
    }

    #[test]
    fn actualizar_equipo_sin_luz_no_cambia_nada() {
        let mut m = Montaje::default();
        m.luces.clear();
        m.luces_piso.clear();
        m.colocar_luz(&equipo("L1"), "C1").unwrap();
        let antes = m.clone();
        assert!(!m.actualizar_equipo("NO", &equipo_en("NO", CATEGORIA_LUCES)));
        assert_eq!(m, antes);
    }

    #[test]
    fn actualizar_equipo_tambien_en_luz_de_piso() {
        let mut m = Montaje::default();
        m.luces_piso.clear();
        m.colocar_luz_piso(&equipo("P1")).unwrap();
        let nuevo = Equipo { marca: "Otra".into(), modelo: "P-2".into(), specs: None, ..equipo("P1") };
        assert!(m.actualizar_equipo("P1", &nuevo));
        assert_eq!((m.luces_piso[0].marca.as_str(), m.luces_piso[0].modelo.as_str()), ("Otra", "P-2"));
        assert!(!m.actualizar_equipo("P1", &nuevo));
    }

    #[test]
    fn una_luz_por_toma() {
        let mut m = Montaje::default();
        m.anadir_toma();
        m.colocar_luz(&equipo("L1"), "C1").unwrap();
        m.colocar_luz(&equipo("L2"), "C1").unwrap();
        m.conectar("L1", Some("T-01")).unwrap();
        assert!(m.conectar("L2", Some("T-01")).is_err());
        assert!(!m.tomas_libres_para("L2").contains(&"T-01".to_string()));
        assert!(m.tomas_libres_para("L1").contains(&"T-01".to_string()));
    }

    #[test]
    fn borrar_toma_desconecta() {
        let mut m = Montaje::default();
        m.luces.clear();
        m.anadir_toma();
        m.colocar_luz(&equipo("L1"), "C1").unwrap();
        m.conectar("L1", Some("T-01")).unwrap();
        m.borrar_toma("T-01");
        assert_eq!(m.luces[0].toma, None);
    }

    #[test]
    fn ajustar_recorta_al_encoger() {
        let mut m = Montaje::default();
        m.luces.clear();
        m.colocar_luz(&equipo("L1"), "C1").unwrap();
        m.luces[0].z = 5.5;
        m.anadir_toma();
        m.tomas[0].pos = 5.0;
        m.sala.largo = 4.0;
        m.sala.ancho = 3.0;
        m.ajustar_a_sala();
        assert_eq!(m.luces[0].z, 4.0);
        assert_eq!(m.tomas[0].pos, 3.0);
    }

    #[test]
    fn reparar_toma_compartida() {
        let mut m = Montaje::default();
        m.luces.clear();
        m.anadir_toma();
        m.colocar_luz(&equipo("L1"), "C1").unwrap();
        m.colocar_luz(&equipo("L2"), "C1").unwrap();
        m.conectar("L1", Some("T-01")).unwrap();
        m.luces[1].toma = Some("T-01".to_string());
        let avisos = m.reparar();
        assert_eq!(m.luces[0].toma.as_deref(), Some("T-01"));
        assert_eq!(m.luces[1].toma, None);
        assert_eq!(avisos.len(), 1);

        let mut m = Montaje::default();
        m.colocar_luz(&equipo("L1"), "C1").unwrap();
        m.luces[0].carril = "X".to_string();
        let avisos = m.reparar();
        assert!(m.carriles.iter().any(|c| c.id == "X"));
        assert_eq!(avisos.len(), 1);
    }

    #[test]
    fn reparar_quita_duplicados() {
        let mut m = Montaje::default();
        m.carriles.push(Carril { id: " C1".to_string(), x: 1.0 });
        m.tomas = vec![
            Toma { codigo: "T-01".to_string(), pared: Pared::Sur, pos: 1.0, nivel: NivelToma::Carriles },
            Toma { codigo: "T-01 ".to_string(), pared: Pared::Norte, pos: 2.0, nivel: NivelToma::Carriles },
        ];
        m.luces = vec![
            Luz { codigo: "L1".to_string(), marca: String::new(), modelo: String::new(), specs: TipoLuz::PanelLed.specs(), ajustes: AjustesLuz::por_defecto(&TipoLuz::PanelLed.specs(), false), carril: "C1".to_string(), z: 1.0, toma: None },
            Luz { codigo: " L1".to_string(), marca: String::new(), modelo: String::new(), specs: TipoLuz::PanelLed.specs(), ajustes: AjustesLuz::por_defecto(&TipoLuz::PanelLed.specs(), false), carril: "C2".to_string(), z: 2.0, toma: None },
        ];
        let avisos = m.reparar();
        assert_eq!(avisos.len(), 3);
        assert_eq!(m.carriles.len(), 3);
        assert_eq!(m.tomas.len(), 1);
        assert_eq!(m.luces.len(), 1);
        assert_eq!(m.luces[0].carril, "C1");
    }

    #[test]
    fn reparar_toma_inexistente() {
        let mut m = Montaje::default();
        m.luces.clear();
        m.colocar_luz(&equipo("L1"), "C1").unwrap();
        m.luces[0].toma = Some("T-99".to_string());
        let avisos = m.reparar();
        assert_eq!(m.luces[0].toma, None);
        assert_eq!(avisos.len(), 1);
    }

    #[test]
    fn reparar_codigos_vacios() {
        let mut m = Montaje::default();
        m.carriles = vec![
            Carril { id: "C1".to_string(), x: 3.0 },
            Carril { id: "  ".to_string(), x: 1.0 },
        ];
        m.tomas = vec![
            Toma { codigo: "T-01".to_string(), pared: Pared::Sur, pos: 1.0, nivel: NivelToma::Carriles },
            Toma { codigo: String::new(), pared: Pared::Sur, pos: 2.0, nivel: NivelToma::Carriles },
        ];
        m.luces = vec![
            Luz { codigo: "L1".to_string(), marca: String::new(), modelo: String::new(), specs: TipoLuz::PanelLed.specs(), ajustes: AjustesLuz::por_defecto(&TipoLuz::PanelLed.specs(), false), carril: "C1".to_string(), z: 1.0, toma: Some("  ".to_string()) },
            Luz { codigo: "  ".to_string(), marca: String::new(), modelo: String::new(), specs: TipoLuz::PanelLed.specs(), ajustes: AjustesLuz::por_defecto(&TipoLuz::PanelLed.specs(), false), carril: "C1".to_string(), z: 1.0, toma: None },
            Luz { codigo: "L2".to_string(), marca: String::new(), modelo: String::new(), specs: TipoLuz::PanelLed.specs(), ajustes: AjustesLuz::por_defecto(&TipoLuz::PanelLed.specs(), false), carril: String::new(), z: 1.0, toma: Some("T-01".to_string()) },
        ];
        let avisos = m.reparar();
        // carril vacío, toma vacía, luz vacía, toma vacía de L1, carril vacío de L2
        assert_eq!(avisos.len(), 5);
        assert_eq!(m.carriles.len(), 1);
        assert_eq!(m.tomas.len(), 1);
        assert_eq!(m.luces.len(), 2);
        assert_eq!(m.luces[0].toma, None);
        assert_eq!(m.luces[1].carril, "C1");
        assert_eq!(m.luces[1].toma.as_deref(), Some("T-01"));

        // Sin carriles: el carril vacío se sustituye por uno nuevo, nunca con id "".
        let mut m = Montaje::default();
        m.carriles.clear();
        m.luces = vec![Luz { codigo: "L3".to_string(), marca: String::new(), modelo: String::new(), specs: TipoLuz::PanelLed.specs(), ajustes: AjustesLuz::por_defecto(&TipoLuz::PanelLed.specs(), false), carril: String::new(), z: 1.0, toma: None }];
        let avisos = m.reparar();
        assert_eq!(avisos.len(), 1);
        assert_eq!(m.carriles.len(), 1);
        assert_eq!(m.carriles[0].id, "C1");
        assert_eq!(m.luces[0].carril, "C1");
    }

    #[test]
    fn ajustar_limites_minimos() {
        let mut m = Montaje::default();
        m.sala.ancho = 0.2;
        m.ajustar_a_sala();
        assert_eq!(m.sala.ancho, 1.0);

        let mut m = Montaje::default();
        m.sala.altura_carriles = 9.0;
        m.ajustar_a_sala();
        assert_eq!(m.sala.altura_carriles, 4.0);

        let mut m = Montaje::default();
        m.sala.altura_carriles = 0.1;
        m.ajustar_a_sala();
        assert_eq!(m.sala.altura_carriles, 0.5);
    }

    #[test]
    fn ajustar_limita_maximo() {
        let mut m = Montaje::default();
        m.sala.ancho = 1e9;
        m.sala.largo = 1e9;
        m.sala.alto_techo = 1e9;
        m.ajustar_a_sala();
        assert_eq!(m.sala.ancho, 100.0);
        assert_eq!(m.sala.largo, 100.0);
        assert_eq!(m.sala.alto_techo, 100.0);
    }

    #[test]
    fn guardar_y_cargar_igual() {
        let mut m = Montaje::default();
        m.anadir_toma();
        m.colocar_luz(&equipo("L1"), "C1").unwrap();
        m.conectar("L1", Some("T-01")).unwrap();
        let ruta = ruta_temp("igual");
        m.guardar(&ruta).unwrap();
        let (cargado, avisos) = Montaje::cargar(&ruta).unwrap();
        std::fs::remove_file(&ruta).ok();
        assert_eq!(cargado, m);
        assert!(avisos.is_empty());
    }

    #[test]
    fn guardar_en_ruta_invalida_falla() {
        let ruta = std::env::temp_dir()
            .join(format!("no_existe_{}", std::process::id()))
            .join("a.json");
        assert!(Montaje::default().guardar(&ruta).is_err());
    }

    #[test]
    fn cargar_json_roto_falla() {
        let ruta = ruta_temp("roto");
        std::fs::write(&ruta, "{no json").unwrap();
        let r = Montaje::cargar(&ruta);
        std::fs::remove_file(&ruta).ok();
        assert!(r.is_err());
    }

    #[test]
    fn montaje_con_bom_carga() {
        let json = r#"{"sala":{"ancho":6.0,"largo":6.0,"alto_techo":4.0,"altura_carriles":3.8},"carriles":[],"tomas":[],"luces":[]}"#;
        let ruta = ruta_temp("bom");
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(json.as_bytes());
        std::fs::write(&ruta, bytes).unwrap();
        let r = Montaje::cargar(&ruta);
        std::fs::remove_file(&ruta).ok();
        let (m, _) = r.unwrap();
        assert_eq!(m.sala.ancho, 6.0);
        assert!(m.luces.is_empty() && m.carriles.is_empty());
    }

    #[test]
    fn anadir_objeto_nombres_y_medidas() {
        let mut m = Montaje::default();
        m.objetos.clear();
        assert_eq!(m.anadir_objeto(TipoObjeto::Silla), "Silla 1");
        assert_eq!(m.anadir_objeto(TipoObjeto::Silla), "Silla 2");
        let o = &m.objetos[0];
        assert_eq!((o.ancho, o.fondo, o.alto), (0.45, 0.45, 0.90));
        assert_eq!((o.x, o.z, o.rot), (3.0, 3.0, 0.0));
    }

    #[test]
    fn default_trae_objetos_de_ejemplo() {
        let m = Montaje::default();
        let esperados = [
            ("Mesa 1", TipoObjeto::Mesa, 3.0, 3.0, 0.0),
            ("Silla 1", TipoObjeto::Silla, 3.0, 3.75, 180.0),
            ("Maniquí 1", TipoObjeto::Maniqui, 4.2, 2.0, 0.0),
        ];
        assert_eq!(m.objetos.len(), esperados.len());
        for (o, (nombre, tipo, x, z, rot)) in m.objetos.iter().zip(esperados) {
            let (ancho, fondo, alto) = tipo.medidas();
            assert_eq!(o.nombre, nombre);
            assert_eq!(o.tipo, tipo);
            assert_eq!((o.x, o.z, o.rot), (x, z, rot));
            assert_eq!((o.ancho, o.fondo, o.alto), (ancho, fondo, alto));
        }
    }

    #[test]
    fn renombrar_objeto_no_usa_codigo_de_luz() {
        let mut m = Montaje::default();
        m.objetos.clear();
        m.colocar_luz(&equipo("L1"), "C1").unwrap();
        m.anadir_objeto(TipoObjeto::Silla);
        assert!(m.renombrar_objeto("Silla 1", "L1").is_err());
        assert_eq!(m.objetos[0].nombre, "Silla 1");
        // Un código de luz tampoco puede ser el nombre que asigna anadir_objeto.
        m.luces[0].codigo = "Silla 2".to_string();
        assert_eq!(m.anadir_objeto(TipoObjeto::Silla), "Silla 3");
    }

    #[test]
    fn renombrar_objeto_rechaza_vacio_o_duplicado() {
        let mut m = Montaje::default();
        m.anadir_objeto(TipoObjeto::Silla);
        m.anadir_objeto(TipoObjeto::Mesa);
        let antes = m.clone();
        assert!(m.renombrar_objeto("Silla 1", "").is_err());
        assert!(m.renombrar_objeto("Silla 1", "  ").is_err());
        assert!(m.renombrar_objeto("Silla 1", "Mesa 1").is_err());
        assert_eq!(m, antes);
        m.renombrar_objeto("Silla 1", " Banco ").unwrap();
        assert!(m.objetos.iter().any(|o| o.nombre == "Banco"));
    }

    #[test]
    fn ajustar_objeto() {
        let mut m = Montaje::default();
        m.objetos.clear();
        m.anadir_objeto(TipoObjeto::Caja);
        m.objetos[0].x = 9.0;
        m.objetos[0].alto = 9.0;
        m.objetos[0].rot = -90.0;
        m.objetos[0].ancho = 0.0;
        m.ajustar_a_sala();
        let o = &m.objetos[0];
        assert_eq!(o.x, 6.0);
        assert_eq!(o.alto, 4.0);
        assert_eq!(o.rot, 270.0);
        assert_eq!(o.ancho, 0.05);
    }

    #[test]
    fn cargar_montaje_sin_objetos() {
        let json = r#"{"sala":{"ancho":6.0,"largo":6.0,"alto_techo":4.0,"altura_carriles":3.8},"carriles":[],"tomas":[],"luces":[]}"#;
        let ruta = ruta_temp("sin_objetos");
        std::fs::write(&ruta, json).unwrap();
        let r = Montaje::cargar(&ruta);
        std::fs::remove_file(&ruta).ok();
        let (m, avisos) = r.unwrap();
        assert!(m.objetos.is_empty());
        assert!(avisos.is_empty());
    }

    #[test]
    fn reparar_objetos_vacios_y_repetidos() {
        let mut m = Montaje::default();
        m.objetos = vec![
            Objeto { nombre: "Silla 1".to_string(), tipo: TipoObjeto::Silla, x: 1.0, z: 1.0, rot: 0.0, ancho: 0.45, fondo: 0.45, alto: 0.9, marca: String::new(), modelo: String::new(), categoria: "Sillas".to_string(), malla: None, piel: None },
            Objeto { nombre: " Silla 1".to_string(), tipo: TipoObjeto::Mesa, x: 2.0, z: 2.0, rot: 0.0, ancho: 1.2, fondo: 0.7, alto: 0.75, marca: String::new(), modelo: String::new(), categoria: "Mesas".to_string(), malla: None, piel: None },
            Objeto { nombre: "  ".to_string(), tipo: TipoObjeto::Caja, x: 2.0, z: 2.0, rot: 0.0, ancho: 1.0, fondo: 1.0, alto: 1.0, marca: String::new(), modelo: String::new(), categoria: String::new(), malla: None, piel: None },
        ];
        let avisos = m.reparar();
        assert_eq!(avisos.len(), 2);
        assert_eq!(m.objetos.len(), 1);
        assert_eq!(m.objetos[0].nombre, "Silla 1");
        assert_eq!(m.objetos[0].tipo, TipoObjeto::Silla);
    }

    #[test]
    fn guardar_y_cargar_con_objetos() {
        let mut m = Montaje::default();
        m.anadir_objeto(TipoObjeto::Mesa);
        m.anadir_objeto(TipoObjeto::Maniqui);
        m.objetos[1].rot = 45.5;
        let ruta = ruta_temp("objetos");
        m.guardar(&ruta).unwrap();
        let (cargado, avisos) = Montaje::cargar(&ruta).unwrap();
        std::fs::remove_file(&ruta).ok();
        assert_eq!(cargado, m);
        assert!(avisos.is_empty());
    }

    #[test]
    fn colocar_objeto_desde_inventario() {
        let mut m = Montaje::default();
        m.objetos.clear();
        let e = Equipo {
            codigo: " M-01 ".to_string(),
            marca: "Ikea".to_string(),
            modelo: "Lack".to_string(),
            categoria: "Mesas".to_string(),
            specs: None,
        };
        m.colocar_objeto(&e).unwrap();
        let o = &m.objetos[0];
        assert_eq!(o.nombre, "M-01");
        assert_eq!(o.tipo, TipoObjeto::Mesa);
        assert_eq!((o.marca.as_str(), o.modelo.as_str()), ("Ikea", "Lack"));
        assert_eq!((o.ancho, o.fondo, o.alto), TipoObjeto::Mesa.medidas());
        assert_eq!((o.x, o.z, o.rot), (3.0, 3.0, 0.0));
        assert!(m.equipo_colocado("M-01"));
        assert_eq!(tipo_para_categoria("Sillas"), TipoObjeto::Silla);
        assert_eq!(tipo_para_categoria("Maniquíes"), TipoObjeto::Maniqui);
        assert_eq!(tipo_para_categoria("Trípodes"), TipoObjeto::Caja);
    }

    #[test]
    fn no_colocar_dos_veces() {
        let mut m = Montaje::default();
        m.luces.clear();
        m.objetos.clear();
        m.colocar_luz(&equipo("L1"), "C1").unwrap();
        assert!(m.colocar_objeto(&equipo_en("L1", "Mesas")).is_err());
        m.colocar_objeto(&equipo_en("M1", "Mesas")).unwrap();
        assert!(m.colocar_luz(&equipo("M1"), "C1").is_err());
        assert!(m.colocar_objeto(&equipo_en(" M1 ", "Mesas")).is_err());
        assert_eq!(m.luces.len(), 1);
        assert_eq!(m.objetos.len(), 1);
    }

    #[test]
    fn objeto_viejo_sin_marca_carga() {
        let json = r#"{"sala":{"ancho":6.0,"largo":6.0,"alto_techo":4.0,"altura_carriles":3.8},"carriles":[{"id":"C1","x":3.0}],"tomas":[],"luces":[],"objetos":[{"nombre":"Mesa 1","tipo":"Mesa","x":3.0,"z":3.0,"rot":0.0,"ancho":1.2,"fondo":0.7,"alto":0.75}]}"#;
        let ruta = ruta_temp("objeto_viejo");
        std::fs::write(&ruta, json).unwrap();
        let r = Montaje::cargar(&ruta);
        std::fs::remove_file(&ruta).ok();
        let (m, avisos) = r.unwrap();
        assert!(avisos.is_empty());
        assert_eq!(m.objetos[0].marca, "");
        assert_eq!(m.objetos[0].modelo, "");
    }

    #[test]
    fn piel_ida_y_vuelta() {
        let mut m = Montaje::default();
        let i = m.objetos.iter().position(|o| o.tipo == TipoObjeto::Maniqui).unwrap();
        m.objetos[i].piel = Some(TONOS_PIEL[5].1);
        let texto = serde_json::to_string(&m).unwrap();
        let vuelta: Montaje = serde_json::from_str(&texto).unwrap();
        assert_eq!(vuelta.objetos[i].piel, Some(TONOS_PIEL[5].1));
    }

    #[test]
    fn json_sin_piel_carga() {
        let json = r#"{"nombre":"Maniquí 1","tipo":"Maniqui","x":1.0,"z":1.0,"rot":0.0,"ancho":0.5,"fondo":0.3,"alto":1.75}"#;
        let o: Objeto = serde_json::from_str(json).unwrap();
        assert_eq!(o.piel, None);
    }

    #[test]
    fn cambiar_categoria_cambia_forma_y_medidas() {
        let mut m = Montaje::default();
        m.cambiar_categoria_objeto("Mesa 1", "Sillas").unwrap();
        let o = &m.objetos[0];
        assert_eq!((o.tipo, o.categoria.as_str()), (TipoObjeto::Silla, "Sillas"));
        assert_eq!((o.ancho, o.fondo, o.alto), (0.45, 0.45, 0.90));
        m.cambiar_categoria_objeto("Mesa 1", "Trípodes").unwrap();
        let o = &m.objetos[0];
        assert_eq!((o.tipo, o.categoria.as_str()), (TipoObjeto::Caja, "Trípodes"));
        assert_eq!((o.ancho, o.fondo, o.alto), (1.0, 1.0, 1.0));
        m.cambiar_categoria_objeto("Mesa 1", "Sillas").unwrap();
        assert_eq!((m.objetos[0].ancho, m.objetos[0].fondo, m.objetos[0].alto), (0.45, 0.45, 0.90));
    }

    #[test]
    fn cambiar_categoria_misma_forma_conserva_medidas() {
        let mut m = Montaje::default();
        // El ejemplo trae 3 objetos; la caja nueva es el cuarto.
        m.anadir_objeto(TipoObjeto::Caja);
        m.objetos[3].ancho = 2.0;
        m.cambiar_categoria_objeto("Caja 1", "Trípodes").unwrap();
        m.cambiar_categoria_objeto("Caja 1", "Focos").unwrap();
        assert_eq!(m.objetos[3].ancho, 2.0);
        assert_eq!(m.objetos[3].categoria, "Focos");
    }

    #[test]
    fn cambiar_categoria_rechaza_luces_y_vacia() {
        let mut m = Montaje::default();
        assert!(m.cambiar_categoria_objeto("Mesa 1", CATEGORIA_LUCES).is_err());
        assert!(m.cambiar_categoria_objeto("Mesa 1", "  ").is_err());
        assert!(m.cambiar_categoria_objeto("No existe", "Sillas").is_err());
        assert_eq!(m.objetos[0].tipo, TipoObjeto::Mesa);
        assert_eq!(m.objetos[0].categoria, "Mesas");
    }

    #[test]
    fn objeto_viejo_recibe_categoria() {
        let json = r#"{"sala":{"ancho":6.0,"largo":6.0,"alto_techo":4.0,"altura_carriles":3.8},"carriles":[{"id":"C1","x":3.0}],"tomas":[],"luces":[],"objetos":[{"nombre":"Mesa 1","tipo":"Mesa","x":3.0,"z":3.0,"rot":0.0,"ancho":1.2,"fondo":0.7,"alto":0.75}]}"#;
        let ruta = ruta_temp("objeto_categoria");
        std::fs::write(&ruta, json).unwrap();
        let r = Montaje::cargar(&ruta);
        std::fs::remove_file(&ruta).ok();
        let (m, _) = r.unwrap();
        assert_eq!(m.objetos[0].categoria, "Mesas");
    }

    #[test]
    fn catalogo_completo() {
        assert_eq!(TipoObjeto::TODOS.len(), 28);
        let nombres: std::collections::HashSet<&str> = TipoObjeto::TODOS.iter().map(|t| t.nombre()).collect();
        assert_eq!(nombres.len(), 28);
        assert_eq!(TipoObjeto::CStand.nombre(), "C-stand");
        for t in TipoObjeto::TODOS {
            let (a, f, h) = t.medidas();
            assert!(a > 0.0 && f > 0.0 && h > 0.0, "{}", t.nombre());
            assert!(GRUPOS.contains(&t.grupo()), "{}", t.nombre());
        }
        assert_eq!(TipoObjeto::Silla.grupo(), "Básicos");
        assert_eq!(TipoObjeto::Altavoz.grupo(), "Audio y monitores");
    }

    #[test]
    fn cambiar_forma_resetea_medidas() {
        let mut m = Montaje::default();
        m.objetos[0].ancho = 2.0;
        m.cambiar_forma_objeto("Mesa 1", TipoObjeto::Tripode).unwrap();
        let o = &m.objetos[0];
        assert_eq!(o.tipo, TipoObjeto::Tripode);
        assert_eq!((o.ancho, o.fondo, o.alto), TipoObjeto::Tripode.medidas());
        assert_eq!(o.categoria, "Mesas");
        m.objetos[0].ancho = 2.0;
        m.cambiar_forma_objeto("Mesa 1", TipoObjeto::Tripode).unwrap();
        assert_eq!(m.objetos[0].ancho, 2.0);
        assert!(m.cambiar_forma_objeto("No existe", TipoObjeto::Silla).is_err());
    }

    #[test]
    fn categorias_base_nuevas_mapean_formas() {
        assert_eq!(CATEGORIAS_BASE.len(), 9);
        assert_eq!(tipo_para_categoria("Cámaras y soportes"), TipoObjeto::CamaraTripode);
        assert_eq!(tipo_para_categoria("Iluminación de suelo"), TipoObjeto::SoporteLuz);
        assert_eq!(tipo_para_categoria("Escenografía"), TipoObjeto::Fondo);
        assert_eq!(tipo_para_categoria("Audio y monitores"), TipoObjeto::MicrofonoPie);
        assert_eq!(categoria_de_tipo(TipoObjeto::Tripode), "Cámaras y soportes");
        assert_eq!(categoria_de_tipo(TipoObjeto::Altavoz), "Audio y monitores");
    }

    fn malla_triangulo(nombre: &str) -> Malla {
        Malla {
            nombre: nombre.to_string(),
            vertices: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            triangulos: vec![[0, 1, 2]],
            colores: vec![[255, 0, 0]],
            tam: [1.0, 1.0, 0.0],
        }
    }

    #[test]
    fn colocar_malla_crea_objeto() {
        let mut m = Montaje::default();
        m.objetos.clear();
        let malla = malla_triangulo("Silla X");
        assert_eq!(m.colocar_malla(&malla), "Silla X 1");
        assert_eq!(m.colocar_malla(&malla), "Silla X 2");
        assert_eq!(m.modelos.get("Silla X"), Some(&malla));
        let o = &m.objetos[0];
        assert_eq!(o.tipo, TipoObjeto::Caja);
        assert_eq!(o.malla.as_deref(), Some("Silla X"));
        assert_eq!((o.x, o.z, o.rot), (3.0, 3.0, 0.0));
        assert_eq!((o.ancho, o.alto), (1.0, 1.0));
        assert!(o.fondo >= 0.05, "el fondo plano se ajusta al mínimo");
    }

    #[test]
    fn guardar_solo_modelos_usados() {
        let mut m = Montaje::default();
        m.objetos.clear();
        m.colocar_malla(&malla_triangulo("Usado"));
        m.modelos.insert("Sobrante".to_string(), malla_triangulo("Sobrante"));
        let antes = m.clone();
        let ruta = ruta_temp("solo_usados");
        m.guardar(&ruta).unwrap();
        let (cargado, _) = Montaje::cargar(&ruta).unwrap();
        std::fs::remove_file(&ruta).ok();
        assert_eq!(m, antes, "guardar no toca el montaje en memoria");
        assert_eq!(cargado.modelos.keys().collect::<Vec<_>>(), ["Usado"]);
        assert_eq!(cargado.objetos[0].malla.as_deref(), Some("Usado"));
    }

    #[test]
    fn guardar_solo_presets_usados() {
        let mut m = Montaje::default();
        let usado = ACABADOS[1].1;
        *m.sala.acabado_mut(Pared::Sur) = usado;
        m.acabados.insert("Usado".to_string(), usado);
        m.acabados.insert("Sobrante".to_string(), ACABADOS[2].1);
        let ruta = ruta_temp("solo_presets_usados");
        m.guardar(&ruta).unwrap();
        let (cargado, _) = Montaje::cargar(&ruta).unwrap();
        std::fs::remove_file(&ruta).ok();
        assert_eq!(m.acabados.len(), 2, "guardar no toca el montaje en memoria");
        assert_eq!(cargado.acabados.keys().collect::<Vec<_>>(), ["Usado"]);
    }

    #[test]
    fn guardar_solo_ies_usados() {
        let mut m = Montaje::default();
        m.luces_piso[0].specs.ies = Some("Usado".to_string());
        m.fotometrias.insert("Usado".to_string(), "texto".to_string());
        m.fotometrias.insert("Sobrante".to_string(), "otro".to_string());
        let antes = m.clone();
        let ruta = ruta_temp("solo_ies_usados");
        m.guardar(&ruta).unwrap();
        let (cargado, _) = Montaje::cargar(&ruta).unwrap();
        std::fs::remove_file(&ruta).ok();
        assert_eq!(m, antes, "guardar no toca el montaje en memoria");
        assert_eq!(cargado.fotometrias.keys().collect::<Vec<_>>(), ["Usado"]);
        assert_eq!(cargado.luces_piso[0].specs.ies.as_deref(), Some("Usado"));
    }

    #[test]
    fn montaje_viejo_sin_ies() {
        let json = r#"{"sala":{"ancho":6.0,"largo":6.0,"alto_techo":4.0,"altura_carriles":3.8},"carriles":[],"tomas":[],"luces":[{"codigo":"L1","marca":"","modelo":"","carril":"C1","z":2.0,"toma":null,"specs":{"tipo":"FresnelLed","potencia_w":150.0,"lux_1m":20000.0,"apertura_min":15.0,"apertura_max":50.0,"cct_min":3200.0,"cct_max":5600.0,"cri":96.0},"ajustes":{"encendida":true,"dimmer":1.0,"apertura":15.0,"cct":5600.0,"giro":0.0,"inclinacion":-90.0}}],"objetos":[]}"#;
        let m = cargar_texto("viejo_sin_ies", json);
        assert!(m.fotometrias.is_empty());
        assert_eq!(m.luces[0].specs.ies, None);
    }

    #[test]
    fn montaje_viejo_sin_acabados() {
        let json = r#"{"sala":{"ancho":6.0,"largo":6.0,"alto_techo":4.0,"altura_carriles":3.8},"carriles":[],"tomas":[],"luces":[],"objetos":[]}"#;
        let m = cargar_texto("viejo_sin_acabados", json);
        assert!(m.acabados.is_empty());
    }

    #[test]
    fn nombre_preset_valido_reglas() {
        assert_eq!(nombre_preset_valido("  Mi verde "), Some("Mi verde".to_string()));
        assert_eq!(nombre_preset_valido(""), None);
        assert_eq!(nombre_preset_valido("   "), None);
        assert_eq!(nombre_preset_valido("pintura blanca mate"), None);
        assert_eq!(nombre_preset_valido("PINTURA BLANCA MATE"), None);
    }

    #[test]
    fn montaje_viejo_sin_modelos_carga() {
        let json = r#"{"sala":{"ancho":6.0,"largo":6.0,"alto_techo":4.0,"altura_carriles":3.8},"carriles":[],"tomas":[],"luces":[],"objetos":[{"nombre":"Mesa 1","tipo":"Mesa","x":3.0,"z":3.0,"rot":0.0,"ancho":1.2,"fondo":0.7,"alto":0.75}]}"#;
        let ruta = ruta_temp("viejo_sin_modelos");
        std::fs::write(&ruta, json).unwrap();
        let r = Montaje::cargar(&ruta);
        std::fs::remove_file(&ruta).ok();
        let (m, avisos) = r.unwrap();
        assert!(avisos.is_empty());
        assert!(m.modelos.is_empty());
        assert_eq!(m.objetos[0].malla, None);
    }

    #[test]
    fn reparar_descarta_mallas_danadas() {
        let mut m = Montaje::default();
        m.objetos.clear();
        let mut mala = malla_triangulo("Rota");
        mala.triangulos = vec![[0, 1, 9]];
        mala.colores = vec![[1, 2, 3]];
        m.modelos.insert("Rota".to_string(), mala);
        m.modelos.insert("Buena".to_string(), malla_triangulo("Buena"));
        let avisos = m.reparar();
        assert_eq!(avisos.len(), 1);
        assert!(m.modelos.contains_key("Buena"));
        assert!(!m.modelos.contains_key("Rota"));
    }

    fn cerca(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4)
    }

    fn unitario(v: [f32; 3]) -> [f32; 3] {
        let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        v.map(|x| x / n)
    }

    #[test]
    fn tipos_luz_tabla() {
        assert_eq!(TipoLuz::TODOS.len(), 14);
        let nombres: std::collections::HashSet<&str> = TipoLuz::TODOS.iter().map(|t| t.nombre()).collect();
        assert_eq!(nombres.len(), 14);
        assert_eq!(TipoLuz::TODOS.iter().filter(|t| t.es_piso()).count(), 5);
        assert!(!TipoLuz::FresnelLed.es_piso() && TipoLuz::TuboLed.es_piso());
        assert_eq!(
            TipoLuz::FresnelLed.specs(),
            SpecsLuz {
                tipo: TipoLuz::FresnelLed,
                potencia_w: 150.0,
                lux_1m: 20000.0,
                apertura_min: 15.0,
                apertura_max: 50.0,
                cct_min: 3200.0,
                cct_max: 5600.0,
                cri: 96.0,
                rgb: false,
                ies: None,
            }
        );
        assert_eq!(
            TipoLuz::TuboLed.specs(),
            SpecsLuz {
                tipo: TipoLuz::TuboLed,
                potencia_w: 30.0,
                lux_1m: 600.0,
                apertura_min: 120.0,
                apertura_max: 120.0,
                cct_min: 2700.0,
                cct_max: 10000.0,
                cri: 95.0,
                rgb: false,
                ies: None,
            }
        );
    }

    #[test]
    fn ajustes_se_limitan() {
        let s = TipoLuz::FresnelLed.specs();
        let mut a = AjustesLuz { encendida: true, dimmer: 2.0, apertura: 90.0, cct: 9000.0, giro: -90.0, inclinacion: 120.0, tono: 0.0, saturacion: 0.0 };
        a.limitar(&s);
        assert_eq!((a.dimmer, a.apertura, a.cct, a.giro, a.inclinacion), (1.0, 50.0, 5600.0, 270.0, 90.0));
        let mut n = a;
        n.dimmer = f32::NAN;
        n.apertura = f32::NAN;
        n.cct = f32::NAN;
        n.giro = f32::NAN;
        n.inclinacion = f32::NAN;
        n.limitar(&s);
        assert!([n.dimmer, n.apertura, n.cct, n.giro, n.inclinacion].iter().all(|v| v.is_finite()));
    }

    #[test]
    fn limitar_giro_da_la_vuelta() {
        let s = TipoLuz::FresnelLed.specs();
        let mut a = AjustesLuz { encendida: true, dimmer: 1.0, apertura: 90.0, cct: 5600.0, giro: 370.0, inclinacion: 0.0, tono: 0.0, saturacion: 0.0 };
        a.limitar(&s);
        assert_eq!(a.giro, 10.0);
        a.giro = -10.0;
        a.limitar(&s);
        assert_eq!(a.giro, 350.0);
        a.giro = 720.0;
        a.limitar(&s);
        assert_eq!(a.giro, 0.0);
    }

    #[test]
    fn apuntar_y_direccion_inversas() {
        let casos = [
            ([3.0, 3.55, 5.0], [3.0, 0.75, 3.0]),
            ([1.8, 1.8, 3.0], [3.0, 0.75, 3.0]),
            ([0.0, 2.0, 0.0], [-1.0, 0.5, 4.0]),
            ([2.0, 1.0, 2.0], [2.0, 1.0, -3.0]),
        ];
        for (a, b) in casos {
            let (giro, incl) = apuntar(a, b);
            let d = direccion(giro, incl);
            let esperado = unitario([b[0] - a[0], b[1] - a[1], b[2] - a[2]]);
            assert!(cerca(d, esperado), "{d:?} vs {esperado:?}");
        }
    }

    #[test]
    fn luz_techo_solo_toma_carriles() {
        let mut m = Montaje::default();
        m.luces.clear();
        m.anadir_toma();
        m.tomas[0].nivel = NivelToma::Suelo;
        m.colocar_luz(&equipo("L1"), "C1").unwrap();
        assert!(m.conectar("L1", Some("T-01")).is_err());
        m.tomas[0].nivel = NivelToma::Carriles;
        m.conectar("L1", Some("T-01")).unwrap();
    }

    #[test]
    fn luz_piso_solo_toma_suelo() {
        let mut m = Montaje::default();
        m.luces_piso.clear();
        m.anadir_toma();
        m.colocar_luz_piso(&equipo("P1")).unwrap();
        assert!(m.conectar_piso("P1", Some("T-01")).is_err());
        m.tomas[0].nivel = NivelToma::Suelo;
        m.conectar_piso("P1", Some("T-01")).unwrap();
        assert_eq!(m.luces_piso[0].toma.as_deref(), Some("T-01"));
    }

    #[test]
    fn una_luz_por_toma_entre_techo_y_piso() {
        let mut m = Montaje::default();
        m.luces.clear();
        m.anadir_toma(); // T-01: carriles
        m.anadir_toma(); // T-02: suelo
        m.tomas[1].nivel = NivelToma::Suelo;
        m.colocar_luz(&equipo("L1"), "C1").unwrap();
        m.colocar_luz(&equipo("L2"), "C1").unwrap();
        m.colocar_luz_piso(&equipo("P1")).unwrap();
        m.conectar("L1", Some("T-01")).unwrap();
        assert!(m.conectar_piso("P1", Some("T-01")).is_err());
        assert!(m.conectar("L2", Some("T-01")).is_err());
        m.conectar_piso("P1", Some("T-02")).unwrap();
        assert!(m.conectar("L2", Some("T-02")).is_err());
        assert_eq!(m.tomas_libres_para("L2"), Vec::<String>::new());
        assert_eq!(m.tomas_libres_para("P1"), ["T-02"]);
    }

    #[test]
    fn montaje_viejo_sin_luces_piso_carga() {
        let json = r#"{"sala":{"ancho":6.0,"largo":6.0,"alto_techo":4.0,"altura_carriles":3.8},"carriles":[{"id":"C1","x":3.0}],"tomas":[{"codigo":"T-01","pared":"Sur","pos":1.0}],"luces":[{"codigo":"L1","marca":"","modelo":"","carril":"C1","z":2.0,"toma":"T-01"}]}"#;
        let ruta = ruta_temp("viejo_luces");
        std::fs::write(&ruta, json).unwrap();
        let r = Montaje::cargar(&ruta);
        std::fs::remove_file(&ruta).ok();
        let (m, avisos) = r.unwrap();
        assert!(avisos.is_empty());
        assert!(m.luces_piso.is_empty());
        assert_eq!(m.luces[0].specs, TipoLuz::PanelLed.specs());
        assert!(m.luces[0].ajustes.encendida);
        assert_eq!(m.luces[0].ajustes.inclinacion, -90.0);
        assert_eq!(m.tomas[0].nivel, NivelToma::Carriles);
        assert_eq!(m.sala.acabado_de(Superficie::Suelo).reflexion, 0.2);
        assert_eq!(m.sala.acabado_de(Superficie::Techo).reflexion, 0.7);
    }

    fn cargar_texto(nombre: &str, json: &str) -> Montaje {
        let ruta = ruta_temp(nombre);
        std::fs::write(&ruta, json).unwrap();
        let r = Montaje::cargar(&ruta);
        std::fs::remove_file(&ruta).ok();
        r.unwrap().0
    }

    #[test]
    fn json_viejo_migra_paredes() {
        let json = r#"{"sala":{"ancho":6.0,"largo":6.0,"alto_techo":4.0,"altura_carriles":3.8},"carriles":[{"id":"C1","x":3.0}],"tomas":[],"luces":[]}"#;
        let m = cargar_texto("migra_paredes", json);
        assert_eq!(m.sala.paredes.len(), 4);
        for p in [Pared::Norte, Pared::Sur, Pared::Este, Pared::Oeste] {
            assert_eq!(m.sala.acabado(p), Acabado { color: [188, 188, 188], reflexion: 0.5 });
        }
    }

    #[test]
    fn json_viejo_migra_suelo_techo() {
        let json = r#"{"sala":{"ancho":6.0,"largo":6.0,"alto_techo":4.0,"altura_carriles":3.8,"refl_suelo":0.3,"refl_techo":0.6},"carriles":[{"id":"C1","x":3.0}],"tomas":[],"luces":[]}"#;
        let m = cargar_texto("migra_suelo_techo", json);
        assert_eq!(m.sala.acabado_de(Superficie::Suelo), Acabado { color: [188, 188, 188], reflexion: 0.3 });
        assert_eq!(m.sala.acabado_de(Superficie::Techo), Acabado { color: [188, 188, 188], reflexion: 0.6 });
    }

    #[test]
    fn suelo_techo_ida_y_vuelta() {
        let mut m = Montaje::default();
        *m.sala.acabado_de_mut(Superficie::Suelo) = ACABADOS[1].1;
        let ruta = ruta_temp("suelo_techo_ida_y_vuelta");
        m.guardar(&ruta).unwrap();
        let texto = std::fs::read_to_string(&ruta).unwrap();
        let (vuelta, _) = Montaje::cargar(&ruta).unwrap();
        std::fs::remove_file(&ruta).ok();
        assert!(!texto.contains("\"refl_suelo\"") && !texto.contains("\"refl_techo\""), "{texto}");
        assert_eq!(vuelta.sala.acabado_de(Superficie::Suelo), ACABADOS[1].1);
        assert_eq!(vuelta.sala.acabado_de(Superficie::Techo), m.sala.acabado_de(Superficie::Techo));
    }

    #[test]
    fn guardar_preset_usado_en_suelo() {
        let mut m = Montaje::default();
        let usado = ACABADOS[2].1;
        *m.sala.acabado_de_mut(Superficie::Suelo) = usado;
        m.acabados.insert("Usado".to_string(), usado);
        m.acabados.insert("Sobrante".to_string(), ACABADOS[3].1);
        let ruta = ruta_temp("guardar_preset_usado_en_suelo");
        m.guardar(&ruta).unwrap();
        let (cargado, _) = Montaje::cargar(&ruta).unwrap();
        std::fs::remove_file(&ruta).ok();
        assert_eq!(cargado.acabados.keys().collect::<Vec<_>>(), ["Usado"]);
    }

    #[test]
    fn json_con_refl_paredes_migra() {
        let json = r#"{"sala":{"ancho":6.0,"largo":6.0,"alto_techo":4.0,"altura_carriles":3.8,"refl_paredes":0.3},"carriles":[{"id":"C1","x":3.0}],"tomas":[],"luces":[]}"#;
        let m = cargar_texto("migra_refl", json);
        for p in [Pared::Norte, Pared::Sur, Pared::Este, Pared::Oeste] {
            assert_eq!(m.sala.acabado(p).reflexion, 0.3);
        }
    }

    #[test]
    fn paredes_independientes_ida_y_vuelta() {
        let mut m = Montaje::default();
        *m.sala.acabado_mut(Pared::Sur) = ACABADOS[1].1;
        let texto = serde_json::to_string(&m).unwrap();
        assert!(!texto.contains("\"refl_paredes\"") && !texto.contains("\"color_paredes\""), "{texto}");
        let vuelta: Montaje = serde_json::from_str(&texto).unwrap();
        assert_eq!(vuelta.sala.acabado(Pared::Sur), ACABADOS[1].1);
        assert_eq!(vuelta.sala.acabado(Pared::Norte).color, [188, 188, 188]);
    }

    #[test]
    fn ajustar_limita_reflexion() {
        let mut m = Montaje::default();
        m.sala.acabado_mut(Pared::Sur).reflexion = 1.5;
        m.sala.acabado_mut(Pared::Norte).reflexion = -1.0;
        m.ajustar_a_sala();
        assert_eq!(m.sala.acabado(Pared::Sur).reflexion, 0.95);
        assert_eq!(m.sala.acabado(Pared::Norte).reflexion, 0.0);
    }

    #[test]
    fn default_trae_luces_de_ejemplo() {
        let m = Montaje::default();
        assert_eq!(m.luces.len(), 1);
        let l = &m.luces[0];
        assert_eq!((l.codigo.as_str(), l.carril.as_str(), l.toma.as_deref()), ("EJ-T1", "C2", None));
        assert_eq!(l.specs.tipo, TipoLuz::FresnelLed);
        assert_eq!((l.ajustes.encendida, l.ajustes.dimmer, l.ajustes.cct), (true, 0.8, 5600.0));
        // Apuntada a la mesa (3, 0.75, 3) desde su cuerpo en el carril C2.
        let hacia_mesa = unitario([0.0, 0.75 - m.sala.altura_carriles + 0.25, 3.0 - l.z]);
        assert!(cerca(direccion(l.ajustes.giro, l.ajustes.inclinacion), hacia_mesa));

        assert_eq!(m.luces_piso.len(), 1);
        let p = &m.luces_piso[0];
        assert_eq!((p.codigo.as_str(), p.specs.tipo, p.x, p.z, p.altura), ("EJ-P1", TipoLuz::MonoluzSoftbox, 1.8, 3.0, 1.8));
        assert_eq!(p.ajustes.dimmer, 0.7);
        assert!(cerca(direccion(p.ajustes.giro, p.ajustes.inclinacion), unitario([3.0 - p.x, 0.75 - p.altura, 3.0 - p.z])));
    }

    #[test]
    fn guardar_y_cargar_con_luces_piso() {
        let mut m = Montaje::default();
        m.anadir_toma();
        m.tomas[0].nivel = NivelToma::Suelo;
        m.colocar_luz_piso(&equipo("P1")).unwrap();
        m.conectar_piso("P1", Some("T-01")).unwrap();
        let ruta = ruta_temp("luces_piso");
        m.guardar(&ruta).unwrap();
        let (cargado, avisos) = Montaje::cargar(&ruta).unwrap();
        std::fs::remove_file(&ruta).ok();
        assert_eq!(cargado, m);
        assert!(avisos.is_empty());
    }

    #[test]
    fn reparar_luces_piso() {
        let mut m = Montaje::default();
        m.luces.clear();
        m.luces_piso.clear();
        m.anadir_toma(); // T-01: carriles
        m.anadir_toma(); // T-02: suelo
        m.tomas[1].nivel = NivelToma::Suelo;
        m.colocar_luz(&equipo("L1"), "C1").unwrap();
        m.conectar("L1", Some("T-01")).unwrap();
        m.colocar_luz_piso(&equipo("P1")).unwrap();
        m.luces_piso[0].toma = Some("T-01".to_string()); // de carriles: no vale
        m.colocar_luz_piso(&equipo("P2")).unwrap();
        m.luces_piso[1].toma = Some("T-02".to_string());
        let mut con_codigo_techo = m.luces_piso[0].clone();
        con_codigo_techo.codigo = "L1".to_string();
        con_codigo_techo.toma = None;
        m.luces_piso.push(con_codigo_techo);
        let mut repetida = m.luces_piso[1].clone();
        repetida.toma = None;
        m.luces_piso.push(repetida);
        let avisos = m.reparar();
        assert_eq!(avisos.len(), 3, "{avisos:?}");
        assert_eq!(m.luces_piso.len(), 2);
        assert_eq!(m.luces_piso[0].toma, None);
        assert_eq!(m.luces_piso[1].toma.as_deref(), Some("T-02"));
        assert_eq!(m.luces[0].toma.as_deref(), Some("T-01"));
    }

    #[test]
    fn cambiar_nivel_desconecta_luces_de_otro_nivel() {
        let mut m = Montaje::default();
        let t1 = m.anadir_toma();
        m.conectar("EJ-T1", Some(&t1)).unwrap();
        m.cambiar_nivel_toma(&t1, NivelToma::Suelo);
        assert_eq!(m.tomas[0].nivel, NivelToma::Suelo);
        assert_eq!(m.luces[0].toma, None);
        let t2 = m.anadir_toma();
        m.cambiar_nivel_toma(&t2, NivelToma::Suelo);
        m.conectar_piso("EJ-P1", Some(&t2)).unwrap();
        m.cambiar_nivel_toma(&t2, NivelToma::Carriles);
        assert_eq!(m.luces_piso[0].toma, None);
    }

    #[test]
    fn nitidez_y_medio_por_tipo() {
        assert_eq!(TipoLuz::Elipsoidal.nitidez(), 3.0);
        assert_eq!(TipoLuz::FresnelLed.medio_emisor(), [0.0, 0.0]);
        assert_eq!(TipoLuz::KinoFloCeleb850.medio_emisor(), [0.635, 0.355]);
    }

    #[test]
    fn radio_emisor_tabla() {
        let v: Vec<f32> = TipoLuz::TODOS.iter().map(|t| t.radio_emisor()).collect();
        assert_eq!(v, vec![0.08, 0.30, 0.05, 0.10, 0.40, 0.45, 0.20, 0.10, 0.05, 0.20, 0.53, 0.34, 0.345, 0.10]);
    }

    #[test]
    fn luces_fluotec_y_frezno() {
        let f = TipoLuz::FluotecCineLight120Lt;
        let z = TipoLuz::SilverStarFrezno250Hex;
        assert_eq!(f.nombre(), "Fluotec CineLight 120 LT");
        assert_eq!(z.nombre(), "Silver Star Frezno 250 Hex");
        assert!(f.es_difusa());
        assert_eq!(f.medio_emisor(), [0.6035, 0.1545]);
        assert!(!z.es_difusa());
        assert_eq!(z.medio_emisor(), [0.0, 0.0]);
        assert!(!f.specs().rgb);
        assert!(z.specs().rgb);
        assert_eq!(f.specs().lux_1m, 13000.0);
        assert_eq!(z.specs().lux_1m, 83200.0);
        assert!(!f.es_piso() && !z.es_piso());
    }

    #[test]
    fn json_viejo_sin_color() {
        let json = r#"{"sala":{"ancho":6.0,"largo":6.0,"alto_techo":4.0,"altura_carriles":3.8},"carriles":[{"id":"C1","x":3.0}],"tomas":[],"luces":[{"codigo":"L1","marca":"","modelo":"","carril":"C1","z":2.0,"toma":null,"specs":{"tipo":"FresnelLed","potencia_w":150.0,"lux_1m":20000.0,"apertura_min":15.0,"apertura_max":50.0,"cct_min":3200.0,"cct_max":5600.0,"cri":96.0},"ajustes":{"encendida":true,"dimmer":1.0,"apertura":15.0,"cct":5600.0,"giro":0.0,"inclinacion":-90.0}}]}"#;
        let m = cargar_texto("viejo_color", json);
        let l = &m.luces[0];
        assert!(!l.specs.rgb);
        assert_eq!((l.ajustes.tono, l.ajustes.saturacion), (0.0, 0.0));
    }

    #[test]
    fn limitar_color() {
        let s = TipoLuz::KinoFloCeleb850.specs();
        let mut a = AjustesLuz { tono: 370.0, saturacion: 2.0, ..AjustesLuz::por_defecto(&s, false) };
        a.limitar(&s);
        assert_eq!((a.tono, a.saturacion), (10.0, 1.0));
        a.tono = f32::NAN;
        a.saturacion = f32::NAN;
        a.limitar(&s);
        assert_eq!((a.tono, a.saturacion), (0.0, 0.0));
        let p = TipoLuz::PanelLed.specs();
        let mut b = AjustesLuz { tono: 370.0, saturacion: 2.0, ..AjustesLuz::por_defecto(&p, false) };
        b.limitar(&p);
        assert_eq!(b.saturacion, 0.0);
    }

    fn camara_prueba() -> CamaraEscena {
        CamaraEscena::nueva("CAM1".to_string(), &Montaje::default().sala)
    }

    fn cerca_f(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn fov_y_de_focal_sensor_y_formato() {
        let c = CamaraEscena { focal_mm: 50.0, sensor_ancho_mm: 36.0, aspecto: 16.0 / 9.0, ..camara_prueba() };
        // El campo horizontal es 2·atan(0.36); la vertical sale del formato.
        assert!(cerca_f((c.fov_y() / 2.0).tan(), 0.36 / (16.0 / 9.0), 1e-4));
        assert!(cerca_f((2.0 * 0.36f32.atan()).to_degrees(), 39.6, 0.05));
    }

    #[test]
    fn radio_lente_de_focal_y_diafragma() {
        let c = CamaraEscena { focal_mm: 50.0, diafragma: 2.0, ..camara_prueba() };
        assert!(cerca_f(c.radio_lente(), 0.0125, 1e-7));
    }

    #[test]
    fn camaras_json_y_defecto() {
        // Sin el campo, el montaje carga con una cámara; con la lista vacía, con ninguna.
        let mut v = serde_json::to_value(Montaje::default()).unwrap();
        v.as_object_mut().unwrap().remove("camaras");
        let m: Montaje = serde_json::from_value(v).unwrap();
        assert_eq!(m.camaras.len(), 1);
        assert_eq!(m.camaras[0].nombre, "CAM1");
        let mut v = serde_json::to_value(Montaje::default()).unwrap();
        v["camaras"] = serde_json::json!([]);
        let m: Montaje = serde_json::from_value(v).unwrap();
        assert!(m.camaras.is_empty());
        // Ida y vuelta conserva una cámara editada.
        let mut m = Montaje::default();
        m.camaras[0].focal_mm = 85.0;
        m.camaras[0].nombre = "Principal".to_string();
        let ida: Montaje = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
        assert_eq!(ida.camaras, m.camaras);
    }

    #[test]
    fn camara_se_limita() {
        let mut c = camara_prueba();
        c.inclinacion = 120.0;
        c.focal_mm = 1000.0;
        c.iso = 1.0;
        c.limitar();
        assert_eq!((c.inclinacion, c.focal_mm, c.iso), (89.0, 400.0, 50.0));
        let mut n = camara_prueba();
        for v in [
            &mut n.x, &mut n.altura, &mut n.z, &mut n.giro, &mut n.inclinacion, &mut n.focal_mm,
            &mut n.sensor_ancho_mm, &mut n.aspecto, &mut n.iso, &mut n.diafragma, &mut n.obturacion,
            &mut n.balance_k, &mut n.foco_m,
        ] {
            *v = f32::NAN;
        }
        n.limitar();
        assert_eq!((n.altura, n.foco_m, n.focal_mm, n.iso, n.inclinacion), (1.6, 3.0, 35.0, 800.0, -10.0));
        assert!([n.x, n.z, n.giro, n.sensor_ancho_mm, n.aspecto, n.diafragma, n.obturacion, n.balance_k]
            .iter()
            .all(|v| v.is_finite()));
    }

    #[test]
    fn anadir_y_renombrar_camaras() {
        let mut m = Montaje::default();
        assert_eq!(m.anadir_camara(), "CAM2");
        assert_eq!(m.anadir_camara(), "CAM3");
        assert!(m.renombrar_camara("CAM2", "CAM1").is_err());
        assert!(m.renombrar_camara("CAM2", "  ").is_err());
        m.renombrar_camara("CAM2", " Techo ").unwrap();
        assert!(m.camaras.iter().any(|c| c.nombre == "Techo"));
        // Reparar quita las repetidas y las vacías, como con los objetos.
        m.camaras[1].nombre = "CAM1".to_string();
        m.camaras[2].nombre = "  ".to_string();
        let avisos = m.reparar();
        assert_eq!(m.camaras.len(), 1);
        assert_eq!(avisos.iter().filter(|a| a.contains("Cámara")).count(), 2);
    }

    #[test]
    fn ajustar_mete_camara_en_sala() {
        let mut m = Montaje::default();
        m.camaras[0].x = -5.0;
        m.ajustar_a_sala();
        assert_eq!(m.camaras[0].x, 0.0);
    }
}
