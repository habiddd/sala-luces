use crate::escena::{centro_luz, ejes_luz};
use crate::geom::V3;
use crate::ies::{self, Ies};
use crate::modelo::{direccion, AjustesLuz, Montaje, Pared, Sala, SpecsLuz, Superficie};
use eframe::egui;
use std::f32::consts::{LN_2, PI};
use std::sync::Arc;

/// Fuente puntual con perfil de haz: intensidad `cd` en el eje `dir`, apertura en grados.
/// Con perfil IES, `cd` es `max_cd · dimmer` y la intensidad en cada dirección sale de la tabla.
#[derive(Clone, Debug, PartialEq)]
pub struct FuenteLuz {
    pub pos: [f32; 3],
    pub dir: [f32; 3],
    pub cd: f32,
    pub apertura: f32,
    pub color: [f32; 3],
    /// Radio del emisor (m), para la penumbra de las sombras suaves.
    pub radio: f32,
    /// Eje u de la base de la luz (`ejes_luz`); v = u × dir.
    pub eje_u: [f32; 3],
    /// Medias del emisor rectangular (u, v); [0, 0] es un disco de radio `radio`.
    pub medio: [f32; 2],
    /// Nitidez del borde del haz (1 suave, 3 corte duro).
    pub nitidez: f32,
    /// Perfil IES del fabricante; None usa la campana del tipo (`perfil_fuente`).
    pub ies: Option<Arc<Ies>>,
}

/// Cromaticidad de Planck (XYZ con Y = 1): radiancia del cuerpo negro integrada con el observador CIE 1931
/// (ajuste analítico multilóbulo de Wyman et al. 2013), así cubre todo 1000–40000 K.
pub fn planck_xyz(k: f64) -> [f64; 3] {
    let t = k.max(1000.0).min(40000.0);
    let mut xyz = [0.0f64; 3];
    for nm in 360..=830 {
        let l = nm as f64;
        let b = 1.0 / (l.powi(5) * ((1.438_776_9e7 / (l * t)).exp() - 1.0));
        let c = cmf(l);
        for i in 0..3 {
            xyz[i] += b * c[i];
        }
    }
    [xyz[0] / xyz[1], 1.0, xyz[2] / xyz[1]]
}

/// Lobulo gaussiano asimétrico: σ1 a la izquierda de μ, σ2 a la derecha.
fn lobulo(l: f64, mu: f64, s1: f64, s2: f64) -> f64 {
    let s = if l < mu { s1 } else { s2 };
    let t = (l - mu) / s;
    (-0.5 * t * t).exp()
}

/// Funciones de color CIE 1931 (x̄, ȳ, z̄) en nm.
fn cmf(l: f64) -> [f64; 3] {
    [
        1.056 * lobulo(l, 599.8, 37.9, 31.0) + 0.362 * lobulo(l, 442.0, 16.0, 26.7) - 0.065 * lobulo(l, 501.1, 20.4, 26.2),
        0.821 * lobulo(l, 568.8, 46.9, 40.5) + 0.286 * lobulo(l, 530.9, 16.3, 31.1),
        1.217 * lobulo(l, 437.0, 11.8, 36.0) + 0.681 * lobulo(l, 459.0, 26.0, 13.8),
    ]
}

/// RGB lineal sRGB (D65) del cuerpo negro a `k`, normalizado a luminancia 1 (método: Planck → XYZ → sRGB).
pub fn cct_a_rgb(k: f32) -> [f32; 3] {
    let [x, y, z] = planck_xyz(k as f64);
    let rgb = [
        3.2406 * x - 1.5372 * y - 0.4986 * z,
        -0.9689 * x + 1.8758 * y + 0.0415 * z,
        0.0557 * x - 0.2040 * y + 1.0570 * z,
    ]
    .map(|c| c.max(0.0));
    let lum = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
    if !(lum > 0.0) {
        return [1.0; 3];
    }
    rgb.map(|c| (c / lum) as f32)
}

/// Ganancia por canal que pasa una luz de `k` K a blanco de 6500 K; luminancia 1.
pub fn balance_blancos(k: f32) -> [f32; 3] {
    if !k.is_finite() {
        return [1.0; 3];
    }
    let (ref_rgb, rgb) = (cct_a_rgb(6500.0), cct_a_rgb(k));
    let g = [ref_rgb[0] / rgb[0], ref_rgb[1] / rgb[1], ref_rgb[2] / rgb[2]];
    let lum = 0.2126 * g[0] + 0.7152 * g[1] + 0.0722 * g[2];
    g.map(|c| c / lum)
}

/// Intensidad en candelas: lux a 1 m × dimmer × (tan(apertura_mín/2) / tan(apertura/2))^k, k = `exponente_zoom` (2 salvo excepción).
/// En las fuentes difusas (Kino Flo) la apertura no cambia la intensidad: la rejilla solo recorta el derrame.
/// Con perfil IES, es el máximo de la tabla × dimmer (la apertura no cambia nada).
pub fn intensidad_cd(s: &SpecsLuz, a: &AjustesLuz, ies: Option<&Ies>) -> f32 {
    if !a.encendida {
        return 0.0;
    }
    if let Some(ies) = ies {
        return ies.max_cd * a.dimmer.max(0.0).min(1.0);
    }
    // Medio ángulo acotado para que la tangente nunca sea 0 ni infinita.
    let medio = |deg: f32| (deg * 0.5).max(0.05).min(89.5).to_radians().tan();
    let r = if s.tipo.es_difusa() { 1.0 } else { medio(s.apertura_min) / medio(a.apertura) };
    s.lux_1m * a.dimmer.max(0.0).min(1.0) * r.powf(s.tipo.exponente_zoom())
}

/// Media apertura en radianes, acotada para que nunca sea 0. Los shaders usan la misma fórmula.
fn media_rad(apertura_deg: f32) -> f32 {
    (apertura_deg.to_radians() * 0.5).max(1e-3)
}

/// Gaussiana de nitidez `nitidez` sobre t = ángulo / media apertura (1 en el eje, 0.5 en t = 1), que se
/// desvanece entre |t| = 1.2 y 1.6 para no tener un corte en seco. Los shaders tienen la misma función.
fn g_haz(t: f32, nitidez: f32) -> f32 {
    let a = t.abs();
    let s = ((a - 1.2) / 0.4).clamp(0.0, 1.0);
    (-LN_2 * a.powf(2.0 * nitidez)).exp() * (1.0 - s * s * (3.0 - 2.0 * s))
}

/// Perfil de revolución (θ al eje, en grados); cero detrás de la luz (θ > 90°): las carcasas no emiten hacia atrás.
pub fn perfil_n(theta_deg: f32, apertura_deg: f32, nitidez: f32) -> f32 {
    if !(theta_deg.abs() <= 90.0) {
        return 0.0;
    }
    g_haz(theta_deg.to_radians() / media_rad(apertura_deg), nitidez)
}

/// Perfil de la fuente hacia `p` (el mismo que `perfil_haz` en los shaders). Emisor rectangular: la mancha
/// se ensancha con el panel, g(atan(x'/z)) · g(atan(y'/z)). Emisor de disco: g(atan(r'/z)). Cero si z ≤ 0.
pub fn perfil_fuente(f: &FuenteLuz, p: [f32; 3]) -> f32 {
    let q = resta(p, f.pos);
    let z = dot(q, f.dir);
    if !(z > 0.0) {
        return 0.0;
    }
    let m = media_rad(f.apertura);
    let (x, y) = (dot(q, f.eje_u), dot(q, cruz(f.eje_u, f.dir)));
    if f.medio[0] > 0.0 && f.medio[1] > 0.0 {
        let xp = (x.abs() - f.medio[0]).max(0.0);
        let yp = (y.abs() - f.medio[1]).max(0.0);
        g_haz(xp.atan2(z) / m, f.nitidez) * g_haz(yp.atan2(z) / m, f.nitidez)
    } else {
        let r = ((x * x + y * y).sqrt() - f.radio).max(0.0);
        g_haz(r.atan2(z) / m, f.nitidez)
    }
}

/// Semilados (u, v) del emisor para el campo cercano: el rectángulo, o el cuadrado de igual área si es un disco
/// de radio > 0.05 m. None si el disco es tan pequeño que la forma puntual basta.
fn semilados(f: &FuenteLuz) -> Option<[f32; 2]> {
    if f.medio[0] > 0.0 && f.medio[1] > 0.0 {
        Some(f.medio)
    } else if f.radio > 0.05 {
        let h = f.radio * 0.886_227; // √π / 2: lado del cuadrado de área πR²
        Some([h, h])
    } else {
        None
    }
}

/// Irradiancia (radiancia 1) de un emisor lambertiano de semilados `h` sobre `p` con normal `n`, por la fórmula
/// de Lambert del polígono: ½ Σ β_i (n · γ_i). El rectángulo se recorta antes al semiespacio frente a `p`.
/// Los shaders tienen la misma fórmula y base (u = eje_u, v = u × dir).
pub fn lambert_exacta(f: &FuenteLuz, p: [f32; 3], n: [f32; 3], h: [f32; 2]) -> f32 {
    let v = cruz(f.eje_u, f.dir);
    let c = resta(f.pos, p);
    // Esquinas relativas a p en orden (+,+), (−,+), (−,−), (+,−): el orden da signo positivo.
    let signos = [(1.0f32, 1.0f32), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)];
    let esquina: [[f32; 3]; 4] = signos.map(|(a, b)| std::array::from_fn(|i| c[i] + f.eje_u[i] * a * h[0] + v[i] * b * h[1]));
    // Sutherland–Hodgman contra n · x > 1e-5 (un cuadrilátero convexo da como mucho 5 vértices).
    let mut q = [[0.0f32; 3]; 8];
    let mut m = 0;
    for j in 0..4 {
        let (a, b) = (esquina[j], esquina[(j + 1) % 4]);
        let (da, db) = (dot(n, a) - 1e-5, dot(n, b) - 1e-5);
        if da > 0.0 {
            q[m] = a;
            m += 1;
        }
        if (da > 0.0) != (db > 0.0) {
            q[m] = std::array::from_fn(|i| a[i] + (b[i] - a[i]) * (da / (da - db)));
            m += 1;
        }
    }
    let mut suma = 0.0;
    for j in 0..m {
        let (a, b) = (q[j], q[(j + 1) % m]);
        // Vértices casi en p o aristas (casi) paralelas a la visual: el ángulo no está definido, se omite.
        if dot(a, a) < 1e-12 || dot(b, b) < 1e-12 {
            continue;
        }
        let x = cruz(a, b);
        let lx = dot(x, x).sqrt();
        if lx >= 1e-9 {
            suma += lx.atan2(dot(a, b)) * dot(n, x) / lx;
        }
    }
    0.5 * suma
}

/// Factor de campo cercano k: E exacta / E puntual (A · cosθe · cosθr / d²) del emisor, ambas de radiancia 1 y
/// desde el centro. Multiplica a la forma de campo lejano; lejos k ≈ 1 y pegado al panel la E queda finita.
/// k = 1 si el emisor es un disco pequeño o el receptor no ve su cara frontal; como mucho 50.
pub fn campo_cercano(f: &FuenteLuz, p: [f32; 3], n: [f32; 3]) -> f32 {
    let Some(h) = semilados(f) else {
        return 1.0;
    };
    let c = resta(f.pos, p);
    let l2 = dot(c, c);
    if !(l2 > 1e-12) {
        return 1.0;
    }
    let l = l2.sqrt();
    let ce = -dot(f.dir, c) / l;
    let cr = dot(n, c) / l;
    if ce <= 0.0 || cr <= 0.0 {
        return 1.0;
    }
    // Mismo suelo de 0.01 m² en la distancia que `iluminancia`: así se cancela en el producto.
    let ep = 4.0 * h[0] * h[1] * ce * cr / l2.max(0.01);
    let k = lambert_exacta(f, p, n, h) / ep;
    // Guarda final: NaN, Inf o negativo dan k = 1 (sin factor) en vez de un píxel blanco.
    if !(k >= 0.0) || k > 1e30 { 1.0 } else { k.min(50.0) }
}

/// Tono (grados) a RGB lineal: HSV con saturación y valor 1, la componente máxima en 1.
pub fn tono_rgb(tono: f32) -> [f32; 3] {
    let h = tono.rem_euclid(360.0) / 60.0;
    let f = h - h.floor();
    match h.floor() as u32 % 6 {
        0 => [1.0, f, 0.0],
        1 => [1.0 - f, 1.0, 0.0],
        2 => [0.0, 1.0, f],
        3 => [0.0, 1.0 - f, 1.0],
        4 => [f, 0.0, 1.0],
        _ => [1.0, 0.0, 1.0 - f],
    }
}

/// Luminancia (Y) de un color lineal sRGB.
pub fn luminancia(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// Color lineal de una luz: blanco con su temperatura, mezclado con el tono según la saturación.
pub fn color_luz(a: &AjustesLuz) -> [f32; 3] {
    let (blanco, tono, s) = (cct_a_rgb(a.cct), tono_rgb(a.tono), a.saturacion);
    std::array::from_fn(|i| (1.0 - s) * blanco[i] + s * tono[i])
}

/// Columnas (θ, 0..180°) y filas (φ, 0..360°) de la tabla IES que sube la GPU.
pub const IES_T: usize = 64;
pub const IES_P: usize = 32;

/// Remuestreo de la tabla a la rejilla fija de la GPU: candelas / máximo, fila j = φ j·360/IES_P, columna k = θ k·180/(IES_T−1).
pub fn muestras_ies(ies: &Ies) -> Vec<f32> {
    let mut v = Vec::with_capacity(IES_T * IES_P);
    for j in 0..IES_P {
        let phi = j as f32 * 360.0 / IES_P as f32;
        for k in 0..IES_T {
            let theta = k as f32 * 180.0 / (IES_T - 1) as f32;
            v.push(ies.intensidad(theta, phi) / ies.max_cd);
        }
    }
    v
}

/// Apertura (°) del perfil IES: el doble del ángulo donde la tabla cae al 10 % del máximo (en algún φ), entre 1 y 170.
/// Es el cono del mapa de sombras y del humo; la intensidad no depende de ella.
pub fn apertura_ies(ies: &Ies) -> f32 {
    let corte = 0.1 * ies.max_cd;
    let mut theta = 0.0f32;
    while theta < 180.0 {
        let pico = (0..36).map(|j| ies.intensidad(theta, j as f32 * 10.0)).fold(0.0f32, f32::max);
        if pico < corte {
            break;
        }
        theta += 1.0;
    }
    (2.0 * theta).clamp(1.0, 170.0)
}

thread_local! {
    /// Perfiles IES ya leídos: (nombre, hash del texto) → perfil y su apertura. Así `fuentes` no relee cada fotograma.
    static IES_LEIDOS: std::cell::RefCell<std::collections::HashMap<(String, u64), (Arc<Ies>, f32)>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Perfil y apertura del texto IES `texto` de la luz `nombre`, desde la caché. None si no se puede leer.
fn ies_leido(nombre: &str, texto: &str) -> Option<(Arc<Ies>, f32)> {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut h = DefaultHasher::new();
    texto.hash(&mut h);
    let clave = (nombre.to_string(), h.finish());
    IES_LEIDOS.with(|c| {
        if let Some(v) = c.borrow().get(&clave) {
            return Some(v.clone());
        }
        let ies = Arc::new(ies::leer(texto).ok()?);
        let v = (ies.clone(), apertura_ies(&ies));
        let mut c = c.borrow_mut();
        // shortcut: la caché no expulsa nada salvo al pasar de 64 perfiles distintos, cuando se vacía entera.
        if c.len() >= 64 {
            c.clear();
        }
        c.insert(clave, v.clone());
        Some(v)
    })
}

fn fuente_de(pos: [f32; 3], s: &SpecsLuz, a: &AjustesLuz, ies: Option<(Arc<Ies>, f32)>) -> FuenteLuz {
    let dir = direccion(a.giro, a.inclinacion);
    let (u, _) = ejes_luz(V3::from_arr(dir));
    FuenteLuz {
        pos,
        dir,
        cd: intensidad_cd(s, a, ies.as_ref().map(|x| &*x.0)),
        apertura: ies.as_ref().map_or(a.apertura, |x| x.1),
        color: color_luz(a),
        radio: s.tipo.radio_emisor(),
        eje_u: [u.x, u.y, u.z],
        medio: s.tipo.medio_emisor(),
        nitidez: s.tipo.nitidez(),
        ies: ies.map(|x| x.0),
    }
}

/// Todas las luces del montaje (techo, en su carril; piso, en su (x, altura, z)).
/// Un perfil IES que no se puede leer deja la luz con la campana del tipo.
pub fn fuentes(m: &Montaje) -> Vec<FuenteLuz> {
    let ies_de = |s: &SpecsLuz| {
        let nombre = s.ies.as_deref()?;
        ies_leido(nombre, m.fotometrias.get(nombre)?)
    };
    let mut v = Vec::with_capacity(m.luces.len() + m.luces_piso.len());
    for l in &m.luces {
        let c = centro_luz(m, l);
        v.push(fuente_de([c.x, c.y, c.z], &l.specs, &l.ajustes, ies_de(&l.specs)));
    }
    for l in &m.luces_piso {
        v.push(fuente_de([l.x, l.altura, l.z], &l.specs, &l.ajustes, ies_de(&l.specs)));
    }
    v
}

/// Ángulos (θ al eje `dir`, φ desde u hacia v, 0..360) de `p` desde la fuente, en grados.
fn angulos_ies(f: &FuenteLuz, p: [f32; 3]) -> (f32, f32) {
    let q = resta(p, f.pos);
    let theta = (dot(q, f.dir) / dot(q, q).sqrt()).max(-1.0).min(1.0).acos().to_degrees();
    let phi = dot(q, cruz(f.eje_u, f.dir)).atan2(dot(q, f.eje_u)).to_degrees().rem_euclid(360.0);
    (theta, phi)
}

/// Candelas hacia `p`: la tabla IES × dimmer (`cd / max_cd`), o la campana del tipo.
fn cd_dir(f: &FuenteLuz, p: [f32; 3]) -> f32 {
    match &f.ies {
        Some(ies) => {
            let (theta, phi) = angulos_ies(f, p);
            ies.intensidad(theta, phi) * f.cd / ies.max_cd
        }
        None => f.cd * perfil_fuente(f, p),
    }
}

/// Iluminancia directa (lux) en `p` con normal `n`, sin sombras. Campo lejano desde el centro del emisor,
/// corregido por el factor de campo cercano.
pub fn iluminancia(p: [f32; 3], n: [f32; 3], f: &FuenteLuz) -> f32 {
    let c = resta(f.pos, p);
    let l2 = dot(c, c);
    if l2 < 1e-6 {
        return 0.0;
    }
    // Suelo de 0.01 m² en la distancia, igual que el shader.
    let d2 = l2.max(0.01);
    let coseno = dot(n, c) / l2.sqrt();
    if coseno <= 0.0 {
        return 0.0;
    }
    // Con luz de color bajan los lux, como en un RGBW real: el azul puro rinde mucho menos que el blanco.
    cd_dir(f, p) * coseno / d2 * campo_cercano(f, p, n) * luminancia(f.color)
}

/// Flujo total (lm) = ∫ I(θ) dΩ con el perfil de revolución. Aproximado para el emisor rectangular
/// (el panel lo ensancha); solo se usa en la luz indirecta.
pub fn flujo_lm(f: &FuenteLuz) -> f32 {
    if let Some(ies) = &f.ies {
        // Suma de I · sinθ · Δθ · Δφ sobre una rejilla de 1° × 5° (la simetría la pone `intensidad`).
        const NT: usize = 180;
        const NP: usize = 72;
        let (dt, dp) = (PI / NT as f32, 2.0 * PI / NP as f32);
        let mut suma = 0.0;
        for i in 0..NT {
            let th = (i as f32 + 0.5) * dt;
            for j in 0..NP {
                let ph = ((j as f32 + 0.5) * dp).to_degrees();
                suma += ies.intensidad(th.to_degrees(), ph) * th.sin();
            }
        }
        return suma * dt * dp * f.cd / ies.max_cd * luminancia(f.color);
    }
    const N: usize = 512;
    let tmax = (1.6 * (f.apertura * 0.5).max(1e-3)).min(90.0).to_radians();
    let h = tmax / N as f32;
    let mut suma = 0.0;
    for i in 0..N {
        let th = (i as f32 + 0.5) * h;
        suma += perfil_n(th.to_degrees(), f.apertura, f.nitidez) * th.sin() * h;
    }
    2.0 * PI * f.cd * suma * luminancia(f.color)
}

/// Luz rebotada en la sala (lux): Φ · ρ / (A · (1 − ρ)), con ρ media ponderada por superficie.
pub fn indirecta_lux(m: &Montaje, fs: &[FuenteLuz]) -> f32 {
    let s = &m.sala;
    let (suelo, techo) = (s.ancho * s.largo, s.ancho * s.largo);
    let paredes: f32 = [(Pared::Norte, s.ancho), (Pared::Sur, s.ancho), (Pared::Este, s.largo), (Pared::Oeste, s.largo)]
        .iter()
        .map(|&(p, largo)| s.acabado(p).reflexion * largo * s.alto_techo)
        .sum();
    let area = suelo + techo + 2.0 * (s.ancho + s.largo) * s.alto_techo;
    if !(area > 0.0) {
        return 0.0;
    }
    let rho_suelo = s.acabado_de(Superficie::Suelo).reflexion;
    let rho_techo = s.acabado_de(Superficie::Techo).reflexion;
    let rho = ((rho_suelo * suelo + paredes + rho_techo * techo) / area).clamp(0.0, 0.95);
    let phi: f32 = fs.iter().map(flujo_lm).sum();
    phi * rho / (area * (1.0 - rho))
}

/// EV100 = log2(N² / t · 100 / ISO).
pub fn ev100(iso: f32, f_num: f32, t_s: f32) -> f32 {
    (f_num * f_num / t_s * 100.0 / iso).log2()
}

/// Valores de los desplegables de la cámara: ISO, diafragma f/N y obturación en segundos.
pub const ISOS: [f32; 8] = [100.0, 200.0, 400.0, 800.0, 1600.0, 3200.0, 6400.0, 12800.0];
pub const DIAFRAGMAS: [f32; 9] = [1.4, 2.0, 2.8, 4.0, 5.6, 8.0, 11.0, 16.0, 22.0];
pub const OBTURACIONES: [f32; 12] = [
    1.0 / 1000.0,
    1.0 / 500.0,
    1.0 / 250.0,
    1.0 / 125.0,
    1.0 / 60.0,
    1.0 / 50.0,
    1.0 / 30.0,
    1.0 / 15.0,
    1.0 / 8.0,
    1.0 / 4.0,
    1.0 / 2.0,
    1.0,
];

/// Obturación como se escribe en la cámara: "1/125", o "1 s" a partir de un segundo.
pub fn etiqueta_obturacion(t_s: f32) -> String {
    if t_s >= 1.0 { "1 s".to_string() } else { format!("1/{}", (1.0 / t_s).round()) }
}

/// Factor de exposición (multiplica la radiancia antes del mapeo de tonos).
pub fn exposicion(ev100: f32) -> f32 {
    1.0 / (1.2 * 2f32.powf(ev100))
}

/// EV100 automático a partir de la luminancia media de la escena.
/// Nunca baja de EV 3 (≈ 17 lux en el suelo): una escena casi a oscuras se ve oscura en vez de amplificar el ruido.
pub fn ev100_auto(lum_media: f32) -> f32 {
    (lum_media * 100.0 / 12.5).log2().max(3.0)
}

/// Escala de falso color (lux): negro, azul 50, verde 200, amarillo 500, naranja 1000, rojo 2000+.
const ESCALA_LUX: [(f32, [f32; 3]); 6] = [
    (0.0, [0.0, 0.0, 0.0]),
    (50.0, [0.0, 0.0, 1.0]),
    (200.0, [0.0, 1.0, 0.0]),
    (500.0, [1.0, 1.0, 0.0]),
    (1000.0, [1.0, 0.5, 0.0]),
    (2000.0, [1.0, 0.0, 0.0]),
];

/// Color de una temperatura de color para mostrarla en pantalla (sRGB).
pub fn color_cct(k: f32) -> egui::Color32 {
    color_lineal(cct_a_rgb(k))
}

/// Color lineal (RGB) a sRGB para mostrarlo en pantalla.
pub fn color_lineal(c: [f32; 3]) -> egui::Color32 {
    let c = c.map(|x| {
        let x = x.clamp(0.0, 1.0);
        let s = if x <= 0.003_130_8 { 12.92 * x } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 };
        (s * 255.0).round() as u8
    });
    egui::Color32::from_rgb(c[0], c[1], c[2])
}

pub fn color_lux(lux: f32) -> egui::Color32 {
    let c: [f32; 3] = if !(lux > 0.0) {
        [0.0; 3]
    } else {
        ESCALA_LUX
            .windows(2)
            .find(|w| lux < w[1].0)
            .map_or(ESCALA_LUX[5].1, |w| {
                let (a, b) = (w[0].0, w[1].0);
                // Entre 0 y 50 lux lineal; luego interpolación logarítmica (las luces se perciben así).
                let k = if a == 0.0 { lux / b } else { (lux / a).ln() / (b / a).ln() };
                std::array::from_fn(|i| w[0].1[i] + (w[1].1[i] - w[0].1[i]) * k)
            })
    };
    egui::Color32::from_rgb((c[0] * 255.0).round() as u8, (c[1] * 255.0).round() as u8, (c[2] * 255.0).round() as u8)
}

/// Iluminancia total en `p` (lux): luces con sombra por rayo contra `tris`, más la luz rebotada. La rebotada es
/// la de las sondas (irradiancia RGB de `Renderer::sondas_cpu`) si las hay, o la cifra uniforme de `indirecta_lux`.
pub fn lux_en(m: &Montaje, p: [f32; 3], n: [f32; 3], tris: &[[[f32; 3]; 3]], sondas: Option<[f32; 3]>) -> f32 {
    let fs = fuentes(m);
    let mut e = match sondas {
        Some(c) => luminancia(c),
        None => indirecta_lux(m, &fs),
    };
    for f in &fs {
        let l = resta(f.pos, p);
        let dist = dot(l, l).sqrt();
        if !(dist > 1e-3) {
            continue;
        }
        let d = l.map(|c| c / dist);
        if tris.iter().any(|t| golpea(p, d, dist, t)) {
            continue;
        }
        e += iluminancia(p, n, f);
    }
    e
}

/// Superficie más cercana que corta el rayo o + t·d: punto de impacto y normal de cara al observador.
/// Las caras de la carcasa que miran al observador desde fuera no cuentan: la GPU tampoco las dibuja.
pub fn sobre_superficie(o: [f32; 3], d: [f32; 3], tris: &[[[f32; 3]; 3]], sala: &Sala) -> Option<([f32; 3], [f32; 3])> {
    let (dt, tri) = tris
        .iter()
        .filter_map(|t| golpe(o, d, t).map(|dt| (dt, t)))
        .filter(|(dt, _)| !oculta_desde_fuera(o, [o[0] + d[0] * dt, o[1] + d[1] * dt, o[2] + d[2] * dt], sala))
        .min_by(|a, b| a.0.total_cmp(&b.0))?;
    let p = [o[0] + d[0] * dt, o[1] + d[1] * dt, o[2] + d[2] * dt];
    let n = cruz(resta(tri[1], tri[0]), resta(tri[2], tri[0]));
    let largo = dot(n, n).sqrt();
    if !(largo > 0.0) {
        return None;
    }
    let mut n = n.map(|c| c / largo);
    if dot(n, d) > 0.0 {
        n = n.map(|c| -c);
    }
    Some((p, n))
}

/// Cara del cerramiento (suelo, paredes, techo) con el ojo al otro lado de su plano: el render la recorta.
fn oculta_desde_fuera(o: [f32; 3], p: [f32; 3], s: &Sala) -> bool {
    const TOL: f32 = 1e-3;
    let lim = [s.ancho, s.alto_techo, s.largo];
    (0..3).any(|i| (p[i].abs() < TOL && o[i] < 0.0) || ((p[i] - lim[i]).abs() < TOL && o[i] > lim[i]))
}

/// Möller–Trumbore: ¿el rayo o + t·d (t entre 1 cm y `tmax`) corta el triángulo?
fn golpea(o: [f32; 3], d: [f32; 3], tmax: f32, t: &[[f32; 3]; 3]) -> bool {
    golpe(o, d, t).is_some_and(|dt| dt < tmax)
}

/// Distancia a lo largo del rayo o + t·d hasta el triángulo (desde 1 cm), si lo corta.
fn golpe(o: [f32; 3], d: [f32; 3], t: &[[f32; 3]; 3]) -> Option<f32> {
    let e1 = resta(t[1], t[0]);
    let e2 = resta(t[2], t[0]);
    let h = cruz(d, e2);
    let a = dot(e1, h);
    if a.abs() < 1e-9 {
        return None;
    }
    let f = 1.0 / a;
    let s = resta(o, t[0]);
    let u = f * dot(s, h);
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = cruz(s, e1);
    let v = f * dot(d, q);
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let dt = f * dot(e2, q);
    (dt > 0.01).then_some(dt)
}

fn resta(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cruz(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modelo::{Carril, Luz, LuzPiso, Pared, Superficie, TipoLuz};

    fn cerca(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    fn fuente(pos: [f32; 3], dir: [f32; 3], cd: f32, apertura: f32) -> FuenteLuz {
        let (u, _) = ejes_luz(V3::from_arr(dir));
        FuenteLuz { pos, dir, cd, apertura, color: [1.0; 3], radio: 0.0, eje_u: [u.x, u.y, u.z], medio: [0.0; 2], nitidez: 1.0, ies: None }
    }

    /// Sala de 4 × 4 × 3 m sin luces ni objetos, con la reflectancia indicada en todas las superficies.
    fn sala(refl: f32) -> Montaje {
        let mut m = Montaje::default();
        m.sala.ancho = 4.0;
        m.sala.largo = 4.0;
        m.sala.alto_techo = 3.0;
        m.sala.altura_carriles = 2.5;
        for s in Superficie::TODAS {
            m.sala.acabado_de_mut(s).reflexion = refl;
        }
        m.carriles = vec![Carril { id: "C1".into(), x: 1.0 }];
        m.luces.clear();
        m.luces_piso.clear();
        m.objetos.clear();
        m
    }

    /// Luz de piso de Monoluz apuntando hacia abajo.
    fn piso_abajo(codigo: &str, x: f32, z: f32, altura: f32) -> LuzPiso {
        let specs = TipoLuz::MonoluzSoftbox.specs();
        let mut ajustes = AjustesLuz::por_defecto(&specs, true);
        ajustes.inclinacion = -90.0;
        LuzPiso { codigo: codigo.into(), marca: "M".into(), modelo: "m".into(), specs, ajustes, x, z, altura, toma: None }
    }

    #[test]
    fn difusa_no_cambia_con_apertura() {
        let s = TipoLuz::KinoFloCeleb850.specs();
        let mut a = AjustesLuz::por_defecto(&s, false);
        a.apertura = 90.0;
        let i90 = intensidad_cd(&s, &a, None);
        a.apertura = 160.0;
        assert_eq!(intensidad_cd(&s, &a, None), i90);
    }

    #[test]
    fn perfil_cero_detras() {
        assert_eq!(perfil_n(100.0, 160.0, 1.0), 0.0);
        assert!(perfil_n(80.0, 160.0, 1.0) > 0.0);
    }

    #[test]
    fn color_baja_lux() {
        let s = TipoLuz::KinoFloCeleb850.specs();
        // Luz en el eje, justo encima del punto.
        let en_eje = |sat: f32, tono: f32| {
            let a = AjustesLuz { saturacion: sat, tono, ..AjustesLuz::por_defecto(&s, false) };
            iluminancia([0.0; 3], [0.0, 1.0, 0.0], &fuente_de([0.0, 2.0, 0.0], &s, &a, None))
        };
        let blanco = en_eje(0.0, 0.0);
        assert!(en_eje(1.0, 240.0) < 0.15 * blanco, "azul {} vs blanco {blanco}", en_eje(1.0, 240.0));
        assert!(en_eje(1.0, 120.0) > 0.5 * blanco, "verde {} vs blanco {blanco}", en_eje(1.0, 120.0));
    }

    #[test]
    fn tono_rgb_primarios() {
        for (tono, esperado) in [(0.0, [1.0, 0.0, 0.0]), (120.0, [0.0, 1.0, 0.0]), (240.0, [0.0, 0.0, 1.0])] {
            let c = tono_rgb(tono);
            assert!(c.iter().zip(esperado).all(|(a, b)| (a - b).abs() <= 1e-4), "{tono}: {c:?}");
        }
    }

    #[test]
    fn inverso_del_cuadrado() {
        let f = fuente([0.0, 2.0, 0.0], [0.0, -1.0, 0.0], 1000.0, 60.0);
        let e = iluminancia([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], &f);
        assert!(cerca(e, 250.0, 0.25), "{e}");
    }

    #[test]
    fn coseno_de_incidencia() {
        // Punto a 2 m de la luz, con la normal inclinada 60° respecto al rayo: la mitad que en el eje.
        let a = 60f32.to_radians();
        let f = fuente([2.0 * a.sin(), 2.0 * a.cos(), 0.0], [-a.sin(), -a.cos(), 0.0], 1000.0, 60.0);
        let e = iluminancia([0.0; 3], [0.0, 1.0, 0.0], &f);
        assert!(cerca(e, 125.0, 0.125), "{e}");
    }

    #[test]
    fn perfil_mitad_en_half_peak() {
        assert!(cerca(perfil_n(30.0, 60.0, 1.0), 0.5, 1e-5));
        assert!(cerca(perfil_n(0.0, 60.0, 1.0), 1.0, 1e-6));
        assert_eq!(perfil_n(50.0, 60.0, 1.0), 0.0, "más allá de 1.6 × media apertura");
    }

    #[test]
    fn perfil_regresion_disco() {
        // Disco de radio 0 y nitidez 1: la gaussiana de siempre (θ = 20° con apertura 60).
        let f = fuente([0.0, 2.0, 0.0], [0.0, -1.0, 0.0], 1000.0, 60.0);
        let p = [2.0 * 20f32.to_radians().tan(), 0.0, 0.0];
        assert!(cerca(perfil_fuente(&f, p), (-LN_2 * (20.0f32 / 30.0).powi(2)).exp(), 1e-4));
    }

    /// Celeb 850 apuntando abajo desde 2 m; `punto(x, y)` = punto a 2 m bajo la luz con (x, y) en su base (u, v).
    fn celeb_abajo() -> FuenteLuz {
        let s = TipoLuz::KinoFloCeleb850.specs();
        let a = AjustesLuz { inclinacion: -90.0, ..AjustesLuz::por_defecto(&s, false) };
        fuente_de([0.0, 2.0, 0.0], &s, &a, None)
    }

    fn en_base(f: &FuenteLuz, x: f32, y: f32) -> [f32; 3] {
        let v = cruz(f.eje_u, f.dir);
        std::array::from_fn(|i| f.pos[i] + 2.0 * f.dir[i] + x * f.eje_u[i] + y * v[i])
    }

    /// Las dos luminarias de catálogo dan en el eje los lux que publica el fabricante (tolerancia 10 %).
    #[test]
    fn fluotec_y_frezno_como_el_fabricante() {
        let lux = |tipo: TipoLuz, apertura: Option<f32>, d: f32| {
            let s = tipo.specs();
            let mut a = AjustesLuz { inclinacion: -90.0, dimmer: 1.0, ..AjustesLuz::por_defecto(&s, false) };
            if let Some(ap) = apertura {
                a.apertura = ap;
            }
            iluminancia([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], &fuente_de([0.0, d, 0.0], &s, &a, None))
        };
        // Fluotec CineLight Studio 120 Long Throw: 3887 lx a 1.8 m, 1734 a 2.74 m, 972 a 3.66 m.
        for (d, esperado) in [(1.8, 3887.0), (2.74, 1734.0), (3.66, 972.0)] {
            let e = lux(TipoLuz::FluotecCineLight120Lt, None, d);
            assert!((e - esperado).abs() < 0.10 * esperado, "Fluotec a {d} m: {e} vs {esperado}");
        }
        // Silver Star Frezno 250 Hex con zoom mínimo: 3328 lx a 5 m.
        let e = lux(TipoLuz::SilverStarFrezno250Hex, Some(10.0), 5.0);
        assert!((e - 3328.0).abs() < 0.10 * 3328.0, "Frezno a 5 m: {e}");
        // Con el zoom abierto del todo (55°): 243 lx a 5 m.
        let e = lux(TipoLuz::SilverStarFrezno250Hex, Some(55.0), 5.0);
        assert!((e - 243.0).abs() < 0.10 * 243.0, "Frezno abierto a 5 m: {e}");
    }

    #[test]
    fn kinoflo_mancha_alargada() {
        let f = celeb_abajo();
        // El lado largo del panel (hu) va a lo largo de u: a igual distancia la mancha cae más lento en u.
        assert!(perfil_fuente(&f, en_base(&f, 1.2, 0.0)) > perfil_fuente(&f, en_base(&f, 0.0, 1.2)));
    }

    #[test]
    fn kinoflo_mancha_cuadrada() {
        // Medio casi nulo (no exacto cero: cero usaría la forma de disco, circular) para aislar la nitidez.
        let f = FuenteLuz { medio: [1e-3; 2], apertura: 60.0, ..celeb_abajo() };
        let a = 2.0 * 50f32.to_radians().tan();
        let diagonal = perfil_fuente(&f, en_base(&f, a / 2f32.sqrt(), a / 2f32.sqrt()));
        let eje = perfil_fuente(&f, en_base(&f, a, 0.0));
        assert!(diagonal > eje, "diagonal {diagonal} <= eje {eje}");
    }

    #[test]
    fn borde_suave() {
        assert!(perfil_n(1.58 * 30.0, 60.0, 1.0) < 0.01);
        assert!(cerca(perfil_n(1.0 * 30.0, 60.0, 1.0), 0.5, 1e-3));
    }

    #[test]
    fn kinoflo_meseta_bajo_panel() {
        let f = celeb_abajo();
        let v = cruz(f.eje_u, f.dir);
        // Receptor a 0.8 m del suelo, horizontal; la luz está a 2 m.
        let en = |x: f32, y: f32| {
            let p: [f32; 3] = std::array::from_fn(|i| [0.0, 0.8, 0.0][i] + x * f.eje_u[i] + y * v[i]);
            iluminancia(p, [0.0, 1.0, 0.0], &f)
        };
        // Con el campo cercano exacto la meseta baja algo cerca del borde: 0.893 a 0.5 m en u (la fórmula
        // puntual daba 0.95+), sin caída brusca.
        let centro = en(0.0, 0.0);
        assert!(en(0.5, 0.0) >= 0.88 * centro, "u 0.5: {} vs {centro}", en(0.5, 0.0));
        assert!(en(0.0, 0.3) >= 0.92 * centro, "v 0.3: {} vs {centro}", en(0.0, 0.3));
        // Fuera del borde del panel (hv = 0.355) la luz ya cae.
        assert!(en(0.0, 1.0) < 0.6 * centro, "v 1.0: {} vs {centro}", en(0.0, 1.0));
    }

    #[test]
    fn lejos_como_puntual() {
        let f = celeb_abajo();
        let p = [0.0, 2.0 - 20.0, 0.0];
        let e = iluminancia(p, [0.0, 1.0, 0.0], &f);
        let esperada = f.cd * perfil_fuente(&f, p) / 400.0;
        assert!(cerca(e, esperada, 0.01 * esperada), "{e} vs {esperada}");
    }

    /// Panel de 1 × 1 m a 1 m sobre el origen, mirando hacia abajo.
    fn panel_1m() -> FuenteLuz {
        FuenteLuz { medio: [0.5, 0.5], ..fuente([0.0, 1.0, 0.0], [0.0, -1.0, 0.0], 1.0, 60.0) }
    }

    #[test]
    fn lambert_cuadrado_analitico() {
        // Cuadrado de 1 m a 1 m en el eje: la suma de las cuatro esquinas da 0.75238.
        let e = lambert_exacta(&panel_1m(), [0.0; 3], [0.0, 1.0, 0.0], [0.5, 0.5]);
        assert!((e - 0.7522).abs() <= 0.005 * 0.7522, "{e}");
    }

    #[test]
    fn campo_cercano_lejos_uno() {
        let s = TipoLuz::KinoFloCeleb850.specs();
        let a = AjustesLuz { inclinacion: -90.0, ..AjustesLuz::por_defecto(&s, false) };
        let f = fuente_de([0.0, 10.0, 0.0], &s, &a, None);
        let k = campo_cercano(&f, [0.0; 3], [0.0, 1.0, 0.0]);
        assert!((k - 1.0).abs() <= 0.01, "{k}");
    }

    #[test]
    fn celeb_pegado_finito() {
        // Receptor a 1 cm bajo el centro del panel: E tiende a π·L, no a 1/d².
        let f = celeb_abajo();
        let e = iluminancia([0.0, 1.99, 0.0], [0.0, 1.0, 0.0], &f);
        let a = 4.0 * f.medio[0] * f.medio[1];
        assert!(e > 0.0 && e <= 1.05 * PI * f.cd / a, "{e} vs {}", PI * f.cd / a);
    }

    #[test]
    fn recorte_pared_perpendicular() {
        // Pared perpendicular al panel que pasa por su borde (u = hu), con el receptor un poco por debajo.
        let f = panel_1m();
        let p: [f32; 3] = std::array::from_fn(|i| f.pos[i] + 0.5 * f.eje_u[i] - [0.0, 0.3, 0.0][i]);
        let n: [f32; 3] = std::array::from_fn(|i| -f.eje_u[i]);
        let e = lambert_exacta(&f, p, n, [0.5, 0.5]);
        let k = campo_cercano(&f, p, n);
        assert!(e.is_finite() && e >= 0.0, "E {e}");
        assert!(k.is_finite() && k >= 0.0, "k {k}");
        assert!(iluminancia(p, n, &f).is_finite());
    }

    #[test]
    fn campo_cercano_sin_nan() {
        // Celeb (rectángulo) y disco pequeño (R = 0.08): puntos degenerados y 200 aleatorios en un cubo de 1 m.
        let celeb = celeb_abajo();
        let disco = FuenteLuz { radio: 0.08, ..fuente([0.0, 2.0, 0.0], [0.0, -1.0, 0.0], 1000.0, 60.0) };
        let mut casos: Vec<([f32; 3], [f32; 3])> = Vec::new();
        let u = celeb.eje_u;
        let v = cruz(celeb.eje_u, celeb.dir);
        let (hu, hv) = (celeb.medio[0], celeb.medio[1]);
        let arriba = [0.0, 1.0, 0.0];
        // Sobre el plano del emisor, en el borde y en la recta de una arista (ce = 0).
        for s in [-hv, 0.0, hv] {
            let p: [f32; 3] = std::array::from_fn(|i| celeb.pos[i] + hu * u[i] + s * v[i]);
            casos.push((p, arriba));
            casos.push((p, u));
        }
        // Bajo el panel, alineado con una arista (la arista y el receptor comparten plano).
        let bajo: [f32; 3] = std::array::from_fn(|i| celeb.pos[i] + hu * u[i] - 0.5 * celeb.dir[i]);
        casos.push((bajo, arriba));
        casos.push((bajo, u));
        // Esquina exacta sobre el plano del emisor.
        let esquina: [f32; 3] = std::array::from_fn(|i| celeb.pos[i] + hu * u[i] + hv * v[i]);
        casos.push((esquina, arriba));
        // 200 puntos aleatorios en un cubo de 1 m alrededor de la luz (LCG de semilla fija).
        let mut semilla: u32 = 12345;
        let mut rnd = || {
            semilla = semilla.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (semilla >> 8) as f32 / (1u32 << 24) as f32
        };
        for _ in 0..200 {
            let p: [f32; 3] = std::array::from_fn(|i| celeb.pos[i] + rnd() - 0.5);
            let nv: [f32; 3] = std::array::from_fn(|_| 2.0 * rnd() - 1.0);
            let l = dot(nv, nv).sqrt();
            let n = if l > 1e-3 { nv.map(|x| x / l) } else { arriba };
            casos.push((p, n));
        }
        for f in [&celeb, &disco] {
            for &(p, n) in &casos {
                let k = campo_cercano(f, p, n);
                let e = iluminancia(p, n, f);
                assert!(k.is_finite() && k >= 0.0, "k {k} en {p:?} n {n:?}");
                assert!(e.is_finite() && e >= 0.0, "E {e} en {p:?} n {n:?}");
            }
        }
    }

    #[test]
    fn detras_cero() {
        let f = fuente([0.0, 2.0, 0.0], [0.0, -1.0, 0.0], 1000.0, 60.0);
        assert_eq!(perfil_fuente(&f, [0.0, 3.0, 0.0]), 0.0);
    }

    #[test]
    fn abrir_haz_baja_intensidad() {
        let s = TipoLuz::FresnelLed.specs();
        let mut a = AjustesLuz::por_defecto(&s, false);
        let i15 = intensidad_cd(&s, &a, None);
        assert!(cerca(i15, 20000.0, 0.5), "{i15}");
        a.apertura = 30.0;
        let i30 = intensidad_cd(&s, &a, None);
        let esperada = 20000.0 * (7.5f32.to_radians().tan() / 15f32.to_radians().tan()).powi(2);
        assert!(cerca(i30, esperada, 1.0), "{i30} vs {esperada}");
        assert!(i30 < 0.25 * i15);
    }

    #[test]
    fn apagada_es_cero() {
        let s = TipoLuz::FresnelLed.specs();
        let mut a = AjustesLuz::por_defecto(&s, false);
        a.dimmer = 0.5;
        assert!(cerca(intensidad_cd(&s, &a, None), 10000.0, 0.5));
        a.encendida = false;
        assert_eq!(intensidad_cd(&s, &a, None), 0.0);
    }

    #[test]
    fn cct_calida_y_fria() {
        let c = cct_a_rgb(3200.0);
        assert!(c[0] > c[1] && c[1] > c[2], "{c:?}");
        assert!(cerca(luminancia(c), 1.0, 1e-3));
        let f = cct_a_rgb(6500.0);
        assert!((f[0] - f[2]).abs() < 0.1, "{f:?}");
        assert!(cerca(luminancia(f), 1.0, 1e-3));
    }

    #[test]
    fn cromaticidad_planckiana_6500k() {
        // Referencia: locus de Planck (Kim et al.) a 6500 K, x ≈ 0.3135, y ≈ 0.3236.
        let v = planck_xyz(6504.0);
        let s = v[0] + v[1] + v[2];
        let (x, y) = (v[0] / s, v[1] / s);
        assert!((x - 0.3135).abs() < 0.004 && (y - 0.3236).abs() < 0.004, "{x} {y}");
    }

    #[test]
    fn indirecta_sube_con_una_pared_blanca() {
        let mut oscura = sala(0.2);
        oscura.luces_piso.push(piso_abajo("P1", 2.0, 2.0, 2.0));
        let mut blanca = oscura.clone();
        blanca.sala.acabado_mut(Pared::Norte).reflexion = 0.85;
        let fs = fuentes(&oscura);
        assert!(indirecta_lux(&blanca, &fs) > indirecta_lux(&oscura, &fs));
    }

    #[test]
    fn ev100_auto_con_suelo() {
        assert_eq!(ev100_auto(1e-3), 3.0);
        assert!(cerca(ev100_auto(12.5), 6.64, 0.01));
    }

    #[test]
    fn ev100_valores() {
        assert!(cerca(ev100(100.0, 1.0, 1.0), 0.0, 1e-4));
        assert!(cerca(ev100(400.0, 2.8, 1.0 / 50.0), 6.61, 0.02));
    }

    #[test]
    fn balance_blancos_valores() {
        assert!(balance_blancos(6500.0).iter().all(|g| cerca(*g, 1.0, 1e-3)));
        // Luz cálida (3200 K): se corrige quitando rojo y subiendo azul.
        let calido = balance_blancos(3200.0);
        assert!(calido[2] > calido[0]);
        for k in [2000.0, 5600.0, 12000.0] {
            let g = balance_blancos(k);
            assert!(cerca(0.2126 * g[0] + 0.7152 * g[1] + 0.0722 * g[2], 1.0, 1e-4), "{k} K");
        }
        assert_eq!(balance_blancos(f32::NAN), [1.0; 3]);
    }

    #[test]
    fn etiquetas_de_obturacion() {
        let etiquetas: Vec<String> = OBTURACIONES.iter().map(|&t| etiqueta_obturacion(t)).collect();
        assert_eq!(etiquetas, ["1/1000", "1/500", "1/250", "1/125", "1/60", "1/50", "1/30", "1/15", "1/8", "1/4", "1/2", "1 s"]);
    }

    #[test]
    fn superficie_mas_cercana_de_cara_al_observador() {
        // Sala de 2 m de alto: suelo (y = 0) y techo (y = 2) cubren el origen; el observador está dentro, en y = 1.
        let s = Sala { ancho: 2.0, largo: 2.0, alto_techo: 2.0, ..Montaje::default().sala };
        let suelo = [[-1.0, 0.0, -1.0], [1.0, 0.0, -1.0], [0.0, 0.0, 1.0]];
        let techo = suelo.map(|v| [v[0], 2.0, v[2]]);
        let (p, n) = sobre_superficie([0.0, 1.0, 0.0], [0.0, -1.0, 0.0], &[techo, suelo], &s).expect("hay impacto");
        assert!(cerca(p[1], 0.0, 1e-4) && cerca(p[0], 0.0, 1e-4), "{p:?}");
        assert!(n[1] > 0.99, "la normal debe mirar al observador: {n:?}");
        assert!(sobre_superficie([5.0, 1.0, 0.0], [0.0, -1.0, 0.0], &[suelo], &s).is_none());
    }

    #[test]
    fn escala_color_lux_extremos() {
        assert_eq!(color_lux(0.0), egui::Color32::BLACK);
        assert_eq!(color_lux(f32::NAN), egui::Color32::BLACK);
        assert_eq!(color_lux(50.0), egui::Color32::from_rgb(0, 0, 255));
        assert_eq!(color_lux(5000.0), egui::Color32::from_rgb(255, 0, 0));
    }

    #[test]
    fn sombra_bloquea() {
        let mut m = sala(0.5);
        m.luces_piso.push(piso_abajo("P1", 2.0, 2.0, 2.0));
        let fs = fuentes(&m);
        let p = [2.0, 0.0, 2.0];
        let n = [0.0, 1.0, 0.0];
        // Cuadrado horizontal entre la luz (y = 2) y el punto (y = 0).
        let tris = [
            [[1.5, 1.0, 1.5], [2.5, 1.0, 1.5], [2.5, 1.0, 2.5]],
            [[1.5, 1.0, 1.5], [2.5, 1.0, 2.5], [1.5, 1.0, 2.5]],
        ];
        let indirecta = indirecta_lux(&m, &fs);
        let con_sombra = lux_en(&m, p, n, &tris, None);
        let sin_sombra = lux_en(&m, p, n, &[], None);
        assert!(cerca(con_sombra, indirecta, 0.01), "{con_sombra} vs {indirecta}");
        assert!(sin_sombra - indirecta > 900.0, "directa sin sombra: {sin_sombra}");
    }

    #[test]
    fn lux_en_sobre_mesa_coincide_con_directa() {
        let m = Montaje::default();
        let tris = crate::escena::triangulos(&m);
        // Altura de la tapa de la mesa por defecto (x 3, z 3): el vértice más alto de su cara superior.
        let top_y = tris
            .iter()
            .flatten()
            .filter(|v| (v[0] - 3.0).abs() < 0.7 && (v[2] - 3.0).abs() < 0.4 && v[1] > 0.6 && v[1] < 0.9)
            .map(|v| v[1])
            .fold(f32::MIN, f32::max);
        let p = [3.0, top_y + 0.001, 3.0];
        let n = [0.0, 1.0, 0.0];
        let fs = fuentes(&m);
        let directa: f32 = fs.iter().map(|f| iluminancia(p, n, f)).sum();
        let esperada = directa + indirecta_lux(&m, &fs);
        let total = lux_en(&m, p, n, &tris, None);
        assert!(directa > 1000.0, "la luz de techo debe llegar a la mesa: {directa}");
        assert!((total - esperada).abs() <= 0.05 * esperada, "lux_en {total} vs directa + indirecta {esperada}");
    }

    #[test]
    fn lux_bajo_el_cursor_sobre_mesa() {
        use crate::geom::{Camara, V3};
        let m = Montaje::default();
        let tris = crate::escena::triangulos(&m);
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1000.0, 800.0));
        let mut cam = Camara::para_sala(&m.sala);
        for _ in 0..4 {
            cam.zoom(120.0);
        }
        let objetivo = V3::new(3.0, 0.75, 3.0);
        let (s, _) = cam.proyectar(objetivo, rect).expect("visible");
        let (o, d) = cam.rayo(s, rect);
        let (p, n) = sobre_superficie([o.x, o.y, o.z], [d.x, d.y, d.z], &tris, &m.sala).expect("impacto");
        assert!((p[1] - 0.75).abs() < 0.01 && n[1] > 0.99, "impacto {p:?} normal {n:?}");
        let fs = fuentes(&m);
        let esperada = fs.iter().map(|f| iluminancia(p, n, f)).sum::<f32>() + indirecta_lux(&m, &fs);
        let total = lux_en(&m, p, n, &tris, None);
        assert!(total > 1000.0, "cursor sobre la mesa: {total} lux (esperado {esperada})");
    }

    #[test]
    fn indirecta_positiva_y_crece_con_reflectancia() {
        let f = fuente([2.0, 2.0, 2.0], [0.0, -1.0, 0.0], 1000.0, 60.0);
        let bajo = indirecta_lux(&sala(0.2), &[f.clone()]);
        let alto = indirecta_lux(&sala(0.6), &[f]);
        assert!(bajo > 0.0);
        assert!(alto > bajo, "{alto} <= {bajo}");
    }

    #[test]
    fn flujo_crece_con_la_apertura() {
        let estrecho = flujo_lm(&fuente([0.0; 3], [0.0, -1.0, 0.0], 1000.0, 20.0));
        let ancho = flujo_lm(&fuente([0.0; 3], [0.0, -1.0, 0.0], 1000.0, 60.0));
        assert!(estrecho > 0.0 && ancho > estrecho, "{estrecho} {ancho}");
    }

    #[test]
    fn iluminancia_con_ies() {
        // Tipo C, simetría 0: 1000 cd en el nadir, 800 a 30°, 300 a 60°, 0 a 90°.
        let texto = "IESNA:LM-63-2002\nTILT=NONE\n1 1000 1 4 1 1 2 0.1 0.1 0.1\n1 1 100\n0 30 60 90\n0\n1000 800 300 0\n";
        let mut m = sala(0.5);
        let mut piso = piso_abajo("P1", 0.0, 0.0, 2.0);
        // Fresnel: emisor pequeño, así el campo cercano vale ≈ 1 a 2 m.
        piso.specs = TipoLuz::FresnelLed.specs();
        piso.specs.ies = Some("ej".to_string());
        piso.ajustes.dimmer = 0.5;
        m.luces_piso.push(piso);
        m.fotometrias.insert("ej".to_string(), texto.to_string());
        let fs = fuentes(&m);
        assert!(fs[0].ies.is_some());
        let e = iluminancia([0.0; 3], [0.0, 1.0, 0.0], &fs[0]);
        assert!(cerca(e, 1000.0 / 4.0 * 0.5, 1.25), "{e}");
    }

    #[test]
    fn fuentes_reusa_el_perfil_ies_leido() {
        // Dos llamadas con el mismo texto comparten el mismo Arc (no se relee). Un texto distinto se lee de nuevo.
        let texto = "IESNA:LM-63-2002\nTILT=NONE\n1 1000 1 4 1 1 2 0.1 0.1 0.1\n1 1 100\n0 30 60 90\n0\n1000 800 300 0\n";
        let mut m = sala(0.5);
        let mut piso = piso_abajo("P1", 0.0, 0.0, 2.0);
        piso.specs = TipoLuz::FresnelLed.specs();
        piso.specs.ies = Some("cache_ies".to_string());
        m.luces_piso.push(piso);
        m.fotometrias.insert("cache_ies".to_string(), texto.to_string());
        let (a, b) = (fuentes(&m), fuentes(&m));
        let (ia, ib) = (a[0].ies.as_ref().unwrap(), b[0].ies.as_ref().unwrap());
        assert!(Arc::ptr_eq(ia, ib));
        assert_eq!(a[0].apertura, b[0].apertura);
        let otro = texto.replace("1000 800 300 0", "500 400 150 0");
        m.fotometrias.insert("cache_ies".to_string(), otro);
        let c = fuentes(&m);
        assert!(!Arc::ptr_eq(ia, c[0].ies.as_ref().unwrap()));
        assert_eq!(c[0].ies.as_ref().unwrap().max_cd, 500.0);
    }

    #[test]
    fn fuentes_posiciones() {
        let mut m = sala(0.5);
        m.luces.push(Luz {
            codigo: "T1".into(),
            marca: "M".into(),
            modelo: "m".into(),
            carril: "C1".into(),
            z: 1.2,
            toma: None,
            specs: TipoLuz::FresnelLed.specs(),
            ajustes: AjustesLuz::por_defecto(&TipoLuz::FresnelLed.specs(), false),
        });
        m.luces_piso.push(piso_abajo("P1", 1.8, 3.0, 1.8));
        let fs = fuentes(&m);
        assert_eq!(fs.len(), 2);
        assert!(cerca(fs[0].pos[0], 1.0, 1e-5) && cerca(fs[0].pos[1], 2.25, 1e-5) && cerca(fs[0].pos[2], 1.2, 1e-5));
        assert!(fs[0].dir[1] < -0.99, "{:?}", fs[0].dir);
        assert_eq!(fs[1].pos, [1.8, 1.8, 3.0]);
    }

    #[test]
    fn muestras_ies_ejemplo() {
        let ies = crate::ies::leer(crate::ies::EJEMPLO).unwrap();
        let v = muestras_ies(&ies);
        assert_eq!(v.len(), IES_T * IES_P);
        for j in 0..IES_P {
            let fila = &v[j * IES_T..(j + 1) * IES_T];
            assert_eq!(fila[0], 1.0, "θ = 0 vale 1");
            assert!(fila[31] > 0.0 && fila[31] < 0.1, "θ ≈ 88.6°: {}", fila[31]);
            assert!(fila[32..].iter().all(|&c| c == 0.0), "θ ≥ 90° vale 0");
        }
    }

    #[test]
    fn apertura_ies_al_diez_por_ciento() {
        // 10 % de 1000 cd = 100: la tabla cae de 300 a 0 entre 60° y 90°, así que llega a 100 hacia 80°.
        let ies = crate::ies::leer(crate::ies::EJEMPLO).unwrap();
        let a = apertura_ies(&ies);
        assert!(cerca(a, 160.0, 2.0), "{a}");
    }
}
