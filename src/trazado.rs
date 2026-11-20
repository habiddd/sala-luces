//! Trazado de caminos progresivo en GPU (preset Fotorrealista). La escena se pasa a una BVH en CPU y se
//! empaqueta en texturas RGBA32F; cada fotograma añade una muestra por píxel a un promedio que vive en dos
//! texturas (ping-pong). Solo OpenGL 3.3 core: texelFetch sobre texturas, sin SSBO ni cómputo.

use crate::escena::VerticeGpu;
use crate::render_gl::{bytes, Paquete, ParamsFrame, Recursos, VS_CUADRO};
use eframe::glow::{self, HasContext};

/// Ancho fijo de las texturas de datos (texeles); la alto se ajusta al contenido.
pub const ANCHO: usize = 1024;
/// Triángulos máximos por hoja de la BVH (salvo en la profundidad límite).
const HOJA_MAX: usize = 4;
/// Profundidad máxima del árbol: la pila de la GPU tiene 32 entradas.
const PROF_MAX: usize = 30;
/// Cubos del SAH binned por eje.
const CUBOS: usize = 12;

/// Triángulo plano de la escena: vértices, normal (de cara) y albedo lineal.
#[derive(Clone, Debug, PartialEq)]
pub struct Tri {
    pub v: [[f32; 3]; 3],
    pub normal: [f32; 3],
    pub albedo: [f32; 3],
    /// Carcasa de la sala: los rayos primarios la saltan cuando el ojo queda al otro lado.
    pub carcasa: bool,
    /// Piel del maniquí: el sombreado usa la capa grasa y la dispersión (`directa_piel`).
    pub piel: bool,
}

impl Tri {
    fn caja(&self) -> Caja {
        self.v.iter().fold(Caja::VACIA, |a, &p| a.une(Caja::de_punto(p)))
    }
}

/// Triángulos de la malla de la GPU (tres vértices seguidos por triángulo, como los genera `escena`).
pub fn triangulos_de(malla: &[VerticeGpu]) -> Vec<Tri> {
    malla
        .chunks_exact(3)
        .map(|c| Tri {
            v: [c[0].pos, c[1].pos, c[2].pos],
            normal: c[0].normal,
            albedo: c[0].albedo,
            carcasa: c[0].carcasa > 0.5 && c[0].carcasa < 1.5,
            piel: c[0].carcasa > 1.5,
        })
        .collect()
}

/// Oclusores para sombras: las posiciones de `escena::malla_sombras` (tres por triángulo).
pub fn triangulos_sombra(puntos: &[[f32; 3]]) -> Vec<Tri> {
    puntos
        .chunks_exact(3)
        .map(|c| Tri { v: [c[0], c[1], c[2]], normal: [0.0; 3], albedo: [0.0; 3], carcasa: false, piel: false })
        .collect()
}

/// Caja alineada con los ejes; vacía cuando min > max.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Caja {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Caja {
    const VACIA: Caja = Caja { min: [f32::INFINITY; 3], max: [f32::NEG_INFINITY; 3] };

    fn de_punto(p: [f32; 3]) -> Caja {
        Caja { min: p, max: p }
    }

    fn une(self, o: Caja) -> Caja {
        Caja {
            min: std::array::from_fn(|i| self.min[i].min(o.min[i])),
            max: std::array::from_fn(|i| self.max[i].max(o.max[i])),
        }
    }

    fn area(&self) -> f32 {
        let e: [f32; 3] = std::array::from_fn(|i| self.max[i] - self.min[i]);
        if e.iter().any(|&x| x < 0.0) {
            return 0.0;
        }
        2.0 * (e[0] * e[1] + e[1] * e[2] + e[2] * e[0])
    }
}

fn centro(c: &Caja) -> [f32; 3] {
    std::array::from_fn(|i| 0.5 * (c.min[i] + c.max[i]))
}

/// Nodo en orden de profundidad: el hijo izquierdo va justo después; el derecho tiene índice `derecha`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Nodo {
    Interno { caja: Caja, derecha: usize, eje: usize },
    Hoja { caja: Caja, primero: usize, cuenta: usize },
}

pub struct Bvh {
    pub nodos: Vec<Nodo>,
    /// Triángulos en el orden que usan las hojas.
    pub tris: Vec<Tri>,
}

/// Construye la BVH por SAH binned (con corte por mediana si el SAH no separa nada).
pub fn construir(tris: &[Tri]) -> Bvh {
    let cajas: Vec<Caja> = tris.iter().map(Tri::caja).collect();
    let centros: Vec<[f32; 3]> = cajas.iter().map(centro).collect();
    let mut idx: Vec<usize> = (0..tris.len()).collect();
    let mut nodos = Vec::new();
    if !idx.is_empty() {
        construir_rec(&mut nodos, &mut idx, 0, 0, &cajas, &centros);
    }
    Bvh { nodos, tris: idx.iter().map(|&i| tris[i].clone()).collect() }
}

fn construir_rec(nodos: &mut Vec<Nodo>, idx: &mut [usize], base: usize, prof: usize, cajas: &[Caja], centros: &[[f32; 3]]) {
    let caja = idx.iter().fold(Caja::VACIA, |a, &i| a.une(cajas[i]));
    let n = idx.len();
    // shortcut: a partir de PROF_MAX la hoja puede pasar de HOJA_MAX triángulos; no ocurre con SAH normal.
    if n <= HOJA_MAX || prof >= PROF_MAX {
        nodos.push(Nodo::Hoja { caja, primero: base, cuenta: n });
        return;
    }
    let cent = idx.iter().fold(Caja::VACIA, |a, &i| a.une(Caja::de_punto(centros[i])));
    let ext: [f32; 3] = std::array::from_fn(|k| cent.max[k] - cent.min[k]);
    let eje = if ext[0] >= ext[1] && ext[0] >= ext[2] {
        0
    } else if ext[1] >= ext[2] {
        1
    } else {
        2
    };
    let mut mid = 0;
    if ext[eje] > 0.0 {
        let escala = CUBOS as f32 / ext[eje];
        let cubo = |i: usize| (((centros[i][eje] - cent.min[eje]) * escala) as usize).min(CUBOS - 1);
        let mut bins = [(0usize, Caja::VACIA); CUBOS];
        for &i in idx.iter() {
            let b = &mut bins[cubo(i)];
            b.0 += 1;
            b.1 = b.1.une(cajas[i]);
        }
        let mut mejor = (f32::INFINITY, 0usize);
        for s in 0..CUBOS - 1 {
            let (nl, cl) = bins[..=s].iter().fold((0usize, Caja::VACIA), |a, b| (a.0 + b.0, a.1.une(b.1)));
            let (nr, cr) = bins[s + 1..].iter().fold((0usize, Caja::VACIA), |a, b| (a.0 + b.0, a.1.une(b.1)));
            let coste = nl as f32 * cl.area() + nr as f32 * cr.area();
            if coste < mejor.0 {
                mejor = (coste, s);
            }
        }
        let s = mejor.1;
        mid = particionar(idx, |i| cubo(i) <= s);
    }
    if mid == 0 || mid == n {
        // Sin separación útil (centros iguales o SAH degenerado): corte por la mediana.
        idx.select_nth_unstable_by(n / 2, |&a, &b| centros[a][eje].total_cmp(&centros[b][eje]));
        mid = n / 2;
    }
    let (izq, der) = idx.split_at_mut(mid);
    let pos = nodos.len();
    nodos.push(Nodo::Interno { caja, derecha: 0, eje });
    construir_rec(nodos, izq, base, prof + 1, cajas, centros);
    let derecha = nodos.len();
    construir_rec(nodos, der, base + mid, prof + 1, cajas, centros);
    nodos[pos] = Nodo::Interno { caja, derecha, eje };
}

/// Reordena `idx` dejando delante los que cumplen `izq`; devuelve cuántos son.
fn particionar(idx: &mut [usize], izq: impl Fn(usize) -> bool) -> usize {
    let mut m = 0;
    for k in 0..idx.len() {
        if izq(idx[k]) {
            idx.swap(k, m);
            m += 1;
        }
    }
    m
}

/// Dimensiones (ancho, alto) de la textura que aloja `texeles` píxeles con ancho `ANCHO`.
pub fn dimension(texeles: usize) -> (usize, usize) {
    (ANCHO, texeles.div_ceil(ANCHO).max(1))
}

/// Datos RGBA32F listos para subir: nodos (2 texeles cada uno) y triángulos (5 texeles cada uno), rellenados.
pub struct Texturas {
    pub nodos: Vec<f32>,
    pub tris: Vec<f32>,
}

/// Texel de nodo: (min.xyz, a) y (max.xyz, b). Interno: a = índice del derecho, b = −(eje + 1) (−1..−3).
/// Hoja: a = primer triángulo, b = cuántos (> 0). Triángulo: v0 (+carcasa), v1, v2, normal, albedo (+piel).
pub fn empaquetar(bvh: &Bvh) -> Texturas {
    let mut nodos: Vec<[f32; 4]> = Vec::with_capacity(2 * bvh.nodos.len());
    for nodo in &bvh.nodos {
        let (caja, a, b) = match *nodo {
            Nodo::Interno { caja, derecha, eje } => (caja, derecha as f32, -(eje as f32 + 1.0)),
            Nodo::Hoja { caja, primero, cuenta } => (caja, primero as f32, cuenta as f32),
        };
        nodos.push([caja.min[0], caja.min[1], caja.min[2], a]);
        nodos.push([caja.max[0], caja.max[1], caja.max[2], b]);
    }
    let mut tris: Vec<[f32; 4]> = Vec::with_capacity(5 * bvh.tris.len());
    for t in &bvh.tris {
        let carcasa = if t.carcasa { 1.0 } else { 0.0 };
        tris.push([t.v[0][0], t.v[0][1], t.v[0][2], carcasa]);
        tris.push([t.v[1][0], t.v[1][1], t.v[1][2], 0.0]);
        tris.push([t.v[2][0], t.v[2][1], t.v[2][2], 0.0]);
        tris.push([t.normal[0], t.normal[1], t.normal[2], 0.0]);
        let piel = if t.piel { 1.0 } else { 0.0 };
        tris.push([t.albedo[0], t.albedo[1], t.albedo[2], piel]);
    }
    Texturas { nodos: relleno(nodos), tris: relleno(tris) }
}

fn relleno(texeles: Vec<[f32; 4]>) -> Vec<f32> {
    let (w, h) = dimension(texeles.len());
    let mut v: Vec<f32> = texeles.into_iter().flatten().collect();
    v.resize(w * h * 4, 0.0);
    v
}

/// Números pseudoaleatorios PCG (RXS-M-XS) de 32 bits: mismo algoritmo que `rnd` del shader.
/// Solo sirve de referencia en las pruebas; el shader lo calcula por su cuenta.
#[cfg(test)]
pub fn pcg(estado: &mut u32) -> u32 {
    *estado = estado.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    let palabra = ((*estado >> ((*estado >> 28) + 4)) ^ *estado).wrapping_mul(277_803_737);
    (palabra >> 22) ^ palabra
}

/// Muestras por sonda hasta converger (cada una, una dirección con distribución coseno alrededor de un eje).
pub const MUESTRAS_SONDAS: u32 = 1024;
/// Separación objetivo de las sondas (m) y tope por eje (si la sala es mayor, el paso crece).
const PASO_SONDAS: f32 = 0.5;
const SONDAS_MAX_EJE: usize = 32;

/// Rejilla de sondas sobre la sala (origen en una esquina): `n` sondas por eje, en el centro de celdas de `paso` m.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rejilla {
    pub n: [usize; 3],
    pub paso: [f32; 3],
}

/// Rejilla de una sala de `sala` = (ancho X, alto Y, largo Z) m: una sonda cada 0.5 m, como mucho 32 por eje.
pub fn rejilla(sala: [f32; 3]) -> Rejilla {
    let n = sala.map(|d| ((d / PASO_SONDAS).ceil() as usize).clamp(1, SONDAS_MAX_EJE));
    Rejilla { n, paso: std::array::from_fn(|i| sala[i] / n[i] as f32) }
}

/// Tamaño (ancho, alto) de la textura de sondas: 6 texeles por sonda (una columna por cara) y una fila por (y, z).
pub fn dimension_sondas(r: &Rejilla) -> (usize, usize) {
    (6 * r.n[0], r.n[1] * r.n[2])
}

/// Irradiancias de las sondas leídas a CPU, en el orden de la textura (RGBA32F; 6 texeles por sonda).
pub struct SondasCpu {
    pub rej: Rejilla,
    pub datos: Vec<f32>,
}

impl SondasCpu {
    /// Irradiancia RGB de la cara `cara` (0..6: +x, −x, +y, −y, +z, −z) de la sonda (ix, iy, iz).
    fn cara(&self, q: [usize; 3], cara: usize) -> [f32; 3] {
        let ancho = 6 * self.rej.n[0];
        let i = ((q[1] + self.rej.n[1] * q[2]) * ancho + 6 * q[0] + cara) * 4;
        [self.datos[i], self.datos[i + 1], self.datos[i + 2]]
    }

    /// Cubo ambiente: E = Σ n_k² · E(cara +k si n_k ≥ 0, si no −k). Los pesos n_k² suman 1 con n unitario.
    fn cubo(&self, q: [usize; 3], n: [f32; 3]) -> [f32; 3] {
        let mut e = [0.0f32; 3];
        for (k, nk) in n.into_iter().enumerate() {
            let c = self.cara(q, 2 * k + usize::from(nk < 0.0));
            for i in 0..3 {
                e[i] += nk * nk * c[i];
            }
        }
        e
    }

    /// Irradiancia RGB en `p` con normal `n`: trilineal sobre las 8 sondas que rodean a `p` (en los bordes se
    /// repite la sonda del borde), cada una con su cubo ambiente.
    pub fn irradiancia(&self, p: [f32; 3], n: [f32; 3]) -> [f32; 3] {
        let r = self.rej;
        let g: [f32; 3] = std::array::from_fn(|i| (p[i] / r.paso[i] - 0.5).clamp(0.0, (r.n[i] - 1) as f32));
        let i0: [usize; 3] = std::array::from_fn(|i| g[i].floor() as usize);
        let f: [f32; 3] = std::array::from_fn(|i| g[i] - i0[i] as f32);
        let i1: [usize; 3] = std::array::from_fn(|i| (i0[i] + 1).min(r.n[i] - 1));
        let mut e = [0.0f32; 3];
        for c in 0..8usize {
            let alto = |i: usize| (c >> i) & 1 == 1;
            let q: [usize; 3] = std::array::from_fn(|i| if alto(i) { i1[i] } else { i0[i] });
            let w: f32 = (0..3).map(|i| if alto(i) { f[i] } else { 1.0 - f[i] }).product();
            let v = self.cubo(q, n);
            for k in 0..3 {
                e[k] += w * v[k];
            }
        }
        e
    }
}

const FS_PREFIJO: &str = r#"#version 330 core
#define MAX_LUCES 32
#define MAX_PILA 32
#define MAX_VISITAS 4096
#define MAX_REBOTES 32
#define PI 3.14159265
#define EPS 0.001
uniform sampler2D uNodos;
uniform sampler2D uTris;
uniform sampler2D uNodosS;
uniform sampler2D uTrisS;
uniform sampler2D uPrev;
uniform int uW;
uniform vec2 uDim;
uniform vec3 uSala;
uniform vec3 uOjo;
uniform vec3 uEje;
uniform vec3 uArriba;
uniform vec3 uAdelante;
uniform float uTan;
uniform float uAspecto;
uniform vec4 uPosCd[MAX_LUCES];
uniform vec4 uDirAp[MAX_LUCES];
uniform vec4 uEjeLuz[MAX_LUCES];
uniform vec4 uEmisor[MAX_LUCES];
uniform vec4 uColor[MAX_LUCES];
uniform float uRadio[MAX_LUCES];
// Tablas IES (fotometria::muestras_ies) en la unidad 5; uIes[i] = 1 si la luz tiene una.
uniform sampler2D uTablaIes;
uniform float uIes[MAX_LUCES];
uniform int uNLuces;
uniform int uRebotes;
uniform int uMuestra;
layout(location = 0) out vec4 fragColor;

// Texel i de la BVH de la escena (`s` = false) o de la BVH de sombras (`s` = true).
vec4 nodo(int i, bool s) {
    ivec2 c = ivec2(i % uW, i / uW);
    return s ? texelFetch(uNodosS, c, 0) : texelFetch(uNodos, c, 0);
}

vec4 tri(int i, bool s) {
    ivec2 c = ivec2(i % uW, i / uW);
    return s ? texelFetch(uTrisS, c, 0) : texelFetch(uTris, c, 0);
}

// PCG RXS-M-XS: números en [0, 1) a partir del estado de 32 bits.
float rnd(inout uint s) {
    s = s * 747796405u + 2891336453u;
    uint w = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
    w = (w >> 22u) ^ w;
    return min(float(w) * 2.3283064e-10, 0.99999994);
}

// Carcasa: el rayo primario salta la cara si el ojo está al otro lado de su plano (igual que fotometria).
bool oculta(vec3 o, vec3 p) {
    for (int i = 0; i < 3; i++) {
        if (abs(p[i]) < 1e-3 && o[i] < 0.0) return true;
        if (abs(p[i] - uSala[i]) < 1e-3 && o[i] > uSala[i]) return true;
    }
    return false;
}

// Último impacto de `recorrer` es piel del maniquí (lo lee `camino` justo después).
bool gPiel = false;

// Recorrido de la BVH con pila fija. Con `cualquiera`, basta un impacto (rayos de sombra). Con `sombra`,
// recorre la BVH de sombras (sin cuerpos de luz, como el mapa de sombras del raster).
bool recorrer(vec3 o, vec3 d, float tmax, bool primario, bool cualquiera, bool sombra,
              out float tHit, out vec3 nHit, out vec3 albHit) {
    float dx = abs(d.x) < 1e-8 ? 1e-8 : d.x;
    float dy = abs(d.y) < 1e-8 ? 1e-8 : d.y;
    float dz = abs(d.z) < 1e-8 ? 1e-8 : d.z;
    vec3 inv = 1.0 / vec3(dx, dy, dz);
    int pila[MAX_PILA];
    int sp = 1;
    pila[0] = 0;
    float mejor = tmax;
    bool hit = false;
    tHit = tmax;
    nHit = vec3(0.0, 0.0, 1.0);
    albHit = vec3(0.0);
    gPiel = false;
    int tiHit = -1;
    for (int it = 0; it < MAX_VISITAS; it++) {
        if (sp <= 0) break;
        sp--;
        int ni = pila[sp];
        vec4 a = nodo(2 * ni, sombra);
        vec4 b = nodo(2 * ni + 1, sombra);
        vec3 t0 = (a.xyz - o) * inv;
        vec3 t1 = (b.xyz - o) * inv;
        vec3 lo = min(t0, t1);
        vec3 hi = max(t0, t1);
        float tn = max(max(lo.x, lo.y), max(lo.z, 0.0));
        float tf = min(min(hi.x, hi.y), min(hi.z, mejor));
        if (tn > tf) continue;
        if (b.w < 0.5) {
            // Interno: el izquierdo está en ni + 1 y el derecho en a.w; b.w = -(eje + 1).
            if (sp + 2 > MAX_PILA) continue;
            int eje = int(-b.w - 0.5);
            int cerca = ni + 1;
            int lejos = int(a.w + 0.5);
            if (d[eje] < 0.0) {
                cerca = int(a.w + 0.5);
                lejos = ni + 1;
            }
            // Se apila el lejano primero: el cercano se visita antes y acorta `mejor`.
            pila[sp] = lejos;
            sp++;
            pila[sp] = cerca;
            sp++;
        } else {
            int primero = int(a.w + 0.5);
            int cuenta = int(b.w + 0.5);
            for (int k = 0; k < cuenta; k++) {
                int ti = 5 * (primero + k);
                vec4 v0 = tri(ti, sombra);
                vec3 v1 = tri(ti + 1, sombra).xyz;
                vec3 v2 = tri(ti + 2, sombra).xyz;
                vec3 e1 = v1 - v0.xyz;
                vec3 e2 = v2 - v0.xyz;
                vec3 pv = cross(d, e2);
                float det = dot(e1, pv);
                if (abs(det) < 1e-12) continue;
                float idet = 1.0 / det;
                vec3 tv = o - v0.xyz;
                float bu = dot(tv, pv) * idet;
                if (bu < 0.0 || bu > 1.0) continue;
                vec3 qv = cross(tv, e1);
                float bv = dot(d, qv) * idet;
                if (bv < 0.0 || bu + bv > 1.0) continue;
                float t = dot(e2, qv) * idet;
                if (t < 1e-6 || t >= mejor) continue;
                if (primario && v0.w > 0.5 && oculta(o, o + d * t)) continue;
                mejor = t;
                hit = true;
                tHit = t;
                tiHit = ti;
                if (cualquiera) return true;
            }
        }
    }
    if (hit) {
        vec4 nn = tri(tiHit + 3, sombra);
        vec4 ab = tri(tiHit + 4, sombra);
        nHit = nn.xyz;
        albHit = ab.xyz;
        gPiel = ab.w > 0.5;
    }
    return hit;
}

// Sombra: ¿hay algo entre p y la luz (distancia dist)?
bool ocluido(vec3 p, vec3 l, float dist) {
    float t;
    vec3 n;
    vec3 a;
    return recorrer(p, l, dist - EPS, false, true, true, t, n, a);
}

// Factor de campo cercano k, igual que fotometria::campo_cercano (CPU): E exacta / E puntual de un emisor
// lambertiano de radiancia 1, desde el centro. Base u = eje_u, v = cross(u, dir). k = 1 para discos pequeños o
// si el receptor no ve la cara frontal; como mucho 50.
float campo_cercano(int i, vec3 p, vec3 n) {
    vec2 h;
    if (uEmisor[i].x > 0.0 && uEmisor[i].y > 0.0) {
        h = uEmisor[i].xy;
    } else {
        if (uRadio[i] <= 0.05) return 1.0;
        h = vec2(uRadio[i] * 0.886227);
    }
    vec3 dir = normalize(uDirAp[i].xyz);
    vec3 u = uEjeLuz[i].xyz;
    vec3 v = cross(u, dir);
    vec3 c = uPosCd[i].xyz - p;
    float l2 = dot(c, c);
    if (!(l2 > 1e-12)) return 1.0;
    float l = sqrt(l2);
    float ce = -dot(dir, c) / l;
    float cr = dot(n, c) / l;
    if (ce <= 0.0 || cr <= 0.0) return 1.0;
    // Esquinas relativas a p en orden (+,+), (-,+), (-,-), (+,-).
    vec3 esq[4];
    esq[0] = c + u * h.x + v * h.y;
    esq[1] = c - u * h.x + v * h.y;
    esq[2] = c - u * h.x - v * h.y;
    esq[3] = c + u * h.x - v * h.y;
    // Sutherland–Hodgman contra n · x > 1e-5 (como mucho 5 vértices).
    vec3 q[8];
    int m = 0;
    for (int j = 0; j < 4; j++) {
        vec3 a = esq[j];
        vec3 b = esq[(j + 1) % 4];
        float da = dot(n, a) - 1e-5;
        float db = dot(n, b) - 1e-5;
        if (da > 0.0) {
            q[m] = a;
            m++;
        }
        if ((da > 0.0) != (db > 0.0)) {
            q[m] = a + (b - a) * (da / (da - db));
            m++;
        }
    }
    // Fórmula de Lambert del polígono: ½ Σ β_i (n · γ_i).
    float suma = 0.0;
    for (int j = 0; j < 8; j++) {
        if (j >= m) break;
        vec3 a = q[j];
        vec3 b = q[(j + 1) % m];
        if (dot(a, a) < 1e-12 || dot(b, b) < 1e-12) continue;
        vec3 x = cross(a, b);
        float lx = length(x);
        if (lx >= 1e-9) suma += atan(lx, dot(a, b)) * dot(n, x) / lx;
    }
    float ep = 4.0 * h.x * h.y * ce * cr / max(l2, 0.01);
    float k = 0.5 * suma / ep;
    // Guarda final: NaN, Inf o negativo dan k = 1 (sin factor) en vez de un píxel blanco.
    if (!(k >= 0.0) || k > 1e30) return 1.0;
    return min(k, 50.0);
}

// Forma del haz, igual que fotometria::perfil_fuente (CPU): gaussiana de nitidez n sobre t, con cola a 0 entre 1.2 y 1.6.
float g_haz(float t, float n) {
    float a = abs(t);
    return exp(-0.693147 * pow(a, 2.0 * n)) * (1.0 - smoothstep(1.2, 1.6, a));
}

// Perfil de la luz i hacia p: emisor rectangular si uEmisor tiene medias > 0, si no disco de radio uRadio.
float perfil_haz(int i, vec3 p) {
    vec3 dir = normalize(uDirAp[i].xyz);
    vec3 u = uEjeLuz[i].xyz;
    vec3 v = cross(u, dir);
    vec3 q = p - uPosCd[i].xyz;
    float z = dot(q, dir);
    if (!(z > 0.0)) return 0.0;
    float m = max(radians(uDirAp[i].w) * 0.5, 1e-3);
    float x = dot(q, u);
    float y = dot(q, v);
    float n = uEjeLuz[i].w;
    if (uEmisor[i].x > 0.0 && uEmisor[i].y > 0.0) {
        float xp = max(abs(x) - uEmisor[i].x, 0.0);
        float yp = max(abs(y) - uEmisor[i].y, 0.0);
        return g_haz(atan(xp, z) / m, n) * g_haz(atan(yp, z) / m, n);
    }
    float r = max(length(vec2(x, y)) - uRadio[i], 0.0);
    return g_haz(atan(r, z) / m, n);
}

// Perfil IES de la luz i (igual que en FS_ESCENA): candelas / máximo, θ por el filtrado de la textura y φ envuelto
// a mano entre las filas de la luz. Con IES, uPosCd.w ya es el máximo × dimmer, así que el peso no cambia.
float perfil_ies(int i, vec3 p) {
    vec3 dir = normalize(uDirAp[i].xyz);
    vec3 u = uEjeLuz[i].xyz;
    vec3 v = cross(u, dir);
    vec3 q = p - uPosCd[i].xyz;
    float l = length(q);
    if (!(l > 1e-6)) return 0.0;
    float theta = degrees(acos(clamp(dot(q, dir) / l, -1.0, 1.0)));
    float phi = degrees(atan(dot(q, v), dot(q, u)));
    if (phi < 0.0) phi += 360.0;
    float fu = (theta / 180.0 * 63.0 + 0.5) / 64.0;
    float fp = phi / 11.25;
    float j0 = mod(floor(fp), 32.0);
    float j1 = mod(j0 + 1.0, 32.0);
    float t = fract(fp);
    float base = 32.0 * float(i);
    float a = texture(uTablaIes, vec2(fu, (base + j0 + 0.5) / float(32 * MAX_LUCES))).r;
    float b = texture(uTablaIes, vec2(fu, (base + j1 + 0.5) / float(32 * MAX_LUCES))).r;
    return mix(a, b, t);
}

float perfil_de(int i, vec3 p) {
    return uIes[i] > 0.5 ? perfil_ies(i, p) : perfil_haz(i, p);
}

// Perfil de la luz i hacia el punto p (0 si queda fuera del haz) por el factor de campo cercano, y la distancia
// al centro d2 (suelo 0.01 m²). El peso de muestreo es w = cd · perfil · k / d2 (ver `directa`).
float perfil_luz(int i, vec3 p, vec3 n, out float d2) {
    vec3 c = uPosCd[i].xyz - p;
    float l2 = dot(c, c);
    d2 = max(l2, 0.01);
    if (l2 < 1e-8) return 0.0;
    return perfil_de(i, p) * campo_cercano(i, p, n);
}

// Piel: capa grasa dieléctrica (n ≈ 1.44, F0 = 0.028; GGX α = 0.35, Weyrich 2006) sobre un difuso con dispersión
// subsuperficial aproximada por envoltura normalizada por canal (recorrido medio R > G > B, Jensen 2001).
// Radiancia hacia v por una luz en dirección l (unitarias) cuya irradiancia normal es Eperp (color incluido).
vec3 piel_brdf(vec3 n, vec3 v, vec3 l, vec3 alb, vec3 Eperp) {
    const vec3 W = vec3(0.30, 0.12, 0.06);
    const float F0 = 0.028;
    const float A = 0.35;
    float nl = dot(n, l);
    float nv = max(dot(n, v), 1e-3);
    vec3 dif = max(vec3(0.0), (vec3(nl) + W) / ((1.0 + W) * (1.0 + W)));
    // pow con base negativa es NaN: n·v puede pasar de 1 por redondeo.
    float fv = F0 + (1.0 - F0) * pow(clamp(1.0 - nv, 0.0, 1.0), 5.0);
    vec3 r = alb * dif * ((1.0 - fv) / 3.14159265);
    vec3 hs = l + v;
    if (nl > 0.0 && dot(hs, hs) > 1e-8) {
        vec3 h = normalize(hs);
        float nh = max(dot(n, h), 0.0);
        float a2 = A * A;
        float dd = nh * nh * (a2 - 1.0) + 1.0;
        float D = a2 / (3.14159265 * dd * dd);
        float k = A * 0.5;
        float G = nl / (nl * (1.0 - k) + k) * nv / (nv * (1.0 - k) + k);
        float F = F0 + (1.0 - F0) * pow(clamp(1.0 - dot(h, v), 0.0, 1.0), 5.0);
        r += vec3(D * G * F / (4.0 * nv));
    }
    return Eperp * r;
}

// Iluminancia directa en p (normal n) con muestreo de una luz por vértice (next-event estimation): la luz k
// se elige con probabilidad w_k / Σw y su contribución se divide por esa probabilidad. Como
// f_k = color_k · cd_k · perfil · k_campo · max(0, n·l) · vis / max(d², 0.01) = color_k · max(0, n·l) · vis · w_k
// (con w_k = cd_k · perfil · k_campo / max(d², 0.01)), el estimador queda color_k · max(0, n·l) · vis · Σw: el
// mismo valor esperado que sumar todas las luces. El disco solo aporta la visibilidad: un punto muestreado en él
// da la sombra suave.
vec3 directa(vec3 p, vec3 n, inout uint estado) {
    float pesos[MAX_LUCES];
    float W = 0.0;
    for (int i = 0; i < MAX_LUCES; i++) {
        pesos[i] = 0.0;
        if (i >= uNLuces) break;
        float d2;
        float per = perfil_luz(i, p, n, d2);
        if (per <= 0.0) continue;
        pesos[i] = uPosCd[i].w * per / d2;
        W += pesos[i];
    }
    // Sin peso (ninguna luz llega) la contribución es cero, igual que con probabilidad uniforme.
    if (W <= 0.0) return vec3(0.0);
    float u = rnd(estado) * W;
    int k = -1;
    float acc = 0.0;
    for (int i = 0; i < MAX_LUCES; i++) {
        if (i >= uNLuces) break;
        if (pesos[i] <= 0.0) continue;
        k = i;
        acc += pesos[i];
        if (u < acc) break;
    }
    if (k < 0) return vec3(0.0);
    vec3 pos = uPosCd[k].xyz;
    vec3 dir = normalize(uDirAp[k].xyz);
    float x1 = rnd(estado);
    float x2 = rnd(estado);
    vec3 q;
    if (uEmisor[k].x > 0.0 && uEmisor[k].y > 0.0) {
        // Emisor rectangular (Kino Flo): el punto muestreado está sobre el panel.
        vec3 ue = uEjeLuz[k].xyz;
        vec3 ve = cross(ue, dir);
        q = pos + ue * (uEmisor[k].x * (2.0 * x1 - 1.0)) + ve * (uEmisor[k].y * (2.0 * x2 - 1.0));
    } else {
        vec3 arr = abs(dir.y) > 0.99 ? vec3(0.0, 0.0, 1.0) : vec3(0.0, 1.0, 0.0);
        vec3 s = normalize(cross(dir, arr));
        vec3 u2 = cross(s, dir);
        float ang = 6.2831853 * x2;
        q = pos + (s * cos(ang) + u2 * sin(ang)) * (uRadio[k] * sqrt(x1));
    }
    vec3 Lq = q - p;
    float dq2 = dot(Lq, Lq);
    // Un punto del disco al otro lado del plano de la superficie no ilumina.
    if (dq2 <= 1e-8 || dot(n, Lq) <= 0.0) return vec3(0.0);
    float dq = sqrt(dq2);
    if (ocluido(p, Lq / dq, dq)) return vec3(0.0);
    vec3 Le = uPosCd[k].xyz - p;
    float ndl = max(0.0, dot(n, Le / sqrt(dot(Le, Le))));
    return uColor[k].rgb * (ndl * W);
}

// Como `directa`, pero para la piel: la envoltura deja pasar luz hasta −0.30 · distancia bajo el plano y la
// radiancia sale de piel_brdf (la misma muestra y el mismo peso W que en `directa`).
vec3 directa_piel(vec3 p, vec3 n, vec3 v, vec3 alb, inout uint estado) {
    float pesos[MAX_LUCES];
    float W = 0.0;
    for (int i = 0; i < MAX_LUCES; i++) {
        pesos[i] = 0.0;
        if (i >= uNLuces) break;
        float d2;
        float per = perfil_luz(i, p, n, d2);
        if (per <= 0.0) continue;
        pesos[i] = uPosCd[i].w * per / d2;
        W += pesos[i];
    }
    if (W <= 0.0) return vec3(0.0);
    float u = rnd(estado) * W;
    int k = -1;
    float acc = 0.0;
    for (int i = 0; i < MAX_LUCES; i++) {
        if (i >= uNLuces) break;
        if (pesos[i] <= 0.0) continue;
        k = i;
        acc += pesos[i];
        if (u < acc) break;
    }
    if (k < 0) return vec3(0.0);
    vec3 pos = uPosCd[k].xyz;
    vec3 dir = normalize(uDirAp[k].xyz);
    float x1 = rnd(estado);
    float x2 = rnd(estado);
    vec3 q;
    if (uEmisor[k].x > 0.0 && uEmisor[k].y > 0.0) {
        vec3 ue = uEjeLuz[k].xyz;
        vec3 ve = cross(ue, dir);
        q = pos + ue * (uEmisor[k].x * (2.0 * x1 - 1.0)) + ve * (uEmisor[k].y * (2.0 * x2 - 1.0));
    } else {
        vec3 arr = abs(dir.y) > 0.99 ? vec3(0.0, 0.0, 1.0) : vec3(0.0, 1.0, 0.0);
        vec3 s = normalize(cross(dir, arr));
        vec3 u2 = cross(s, dir);
        float ang = 6.2831853 * x2;
        q = pos + (s * cos(ang) + u2 * sin(ang)) * (uRadio[k] * sqrt(x1));
    }
    vec3 Lq = q - p;
    float dq2 = dot(Lq, Lq);
    if (dq2 <= 1e-8) return vec3(0.0);
    float dq = sqrt(dq2);
    // Envoltura: la piel recibe luz hasta un poco por debajo del plano de la superficie.
    if (dot(n, Lq) <= -0.30 * dq) return vec3(0.0);
    if (ocluido(p, Lq / dq, dq)) return vec3(0.0);
    return piel_brdf(n, v, Lq / dq, alb, uColor[k].rgb * W);
}

// Camino de un píxel: vértice 0 = impacto primario; hasta `uRebotes` rebotes difusos con ruleta rusa desde el 2.º.
// Devuelve la radiancia (L/π incluido), la cobertura (1 si el primario acierta) y el albedo y la normal del primario
// (0 sin impacto).
vec3 camino(vec3 o, vec3 d, inout uint estado, out float cov, out vec3 alb0, out vec3 n0) {
    vec3 L = vec3(0.0);
    vec3 beta = vec3(1.0);
    cov = 0.0;
    alb0 = vec3(0.0);
    n0 = vec3(0.0);
    for (int b = 0; b <= MAX_REBOTES; b++) {
        if (b > uRebotes) break;
        float t;
        vec3 n;
        vec3 alb;
        if (!recorrer(o, d, 1e30, b == 0, false, false, t, n, alb)) break;
        if (b == 0) cov = 1.0;
        bool esPiel = gPiel;
        vec3 p = o + d * t;
        // Normal nula (malla degenerada): se toma de cara al rayo para no producir NaN.
        if (dot(n, n) < 1e-12) n = -d;
        if (dot(n, d) > 0.0) n = -n;
        if (b == 0) { alb0 = alb; n0 = n; }
        vec3 pn = p + n * EPS;
        // if en vez de ?: algunos drivers evalúan las dos ramas y un NaN de una contamina el resultado.
        if (esPiel) L += beta * directa_piel(pn, n, -d, alb, estado);
        else L += beta * alb * directa(pn, n, estado) / PI;
        if (b == uRebotes) break;
        beta *= alb;
        if (b >= 2) {
            float pr = clamp(max(beta.x, max(beta.y, beta.z)), 0.05, 0.95);
            float rr = rnd(estado);
            if (rr > pr) break;
            beta /= pr;
        }
        float x1 = rnd(estado);
        float x2 = rnd(estado);
        float r = sqrt(x1);
        float phi = 6.2831853 * x2;
        vec3 ta = abs(n.x) > 0.5 ? vec3(0.0, 1.0, 0.0) : vec3(1.0, 0.0, 0.0);
        vec3 s = normalize(cross(ta, n));
        vec3 u = cross(n, s);
        o = pn;
        d = normalize(s * (r * cos(phi)) + u * (r * sin(phi)) + n * sqrt(max(0.0, 1.0 - x1)));
    }
    return L;
}

"#;

// Pase de píxeles: un píxel por muestra (FS_PREFIJO + este main). Tres salidas: color, albedo y normal del primario,
// cada una con la cobertura en alfa y su media incremental.
const MAIN_TRAZA: &str = r#"layout(location = 1) out vec4 fragAlb;
layout(location = 2) out vec4 fragNor;
uniform sampler2D uPrevAlb;
uniform sampler2D uPrevNor;
uniform float uLente;
uniform float uFoco;
void main() {
    uint estado = uint(gl_FragCoord.x) * 1973u + uint(gl_FragCoord.y) * 9277u + uint(uMuestra) * 26699u + 1u;
    rnd(estado);
    float jx = rnd(estado);
    float jy = rnd(estado);
    vec2 pix = floor(gl_FragCoord.xy) + vec2(jx, jy);
    vec2 ndc = pix / uDim * 2.0 - 1.0;
    vec3 dir = normalize(uAdelante + uEje * (ndc.x * uAspecto * uTan) + uArriba * (ndc.y * uTan));
    // Lente fina: el rayo sale de un punto al azar de la pupila y apunta al punto del plano de foco que el primario veía.
    vec3 o = uOjo;
    if (uLente > 0.0) {
        vec3 pf = uOjo + dir * (uFoco / dot(dir, uAdelante));
        float rl = uLente * sqrt(rnd(estado));
        float al = 6.2831853 * rnd(estado);
        o = uOjo + (uEje * cos(al) + uArriba * sin(al)) * rl;
        dir = normalize(pf - o);
    }
    float cov;
    vec3 alb0;
    vec3 n0;
    vec3 L = camino(o, dir, estado, cov, alb0, n0);
    // Una muestra no finita (NaN o Inf) se descarta: un solo píxel así contaminaría el promedio para siempre.
    // isnan no es fiable en todos los drivers: una comparación con NaN siempre da falso.
    if (!(L.x >= 0.0 && L.y >= 0.0 && L.z >= 0.0 && L.x < 1e20 && L.y < 1e20 && L.z < 1e20)) L = vec3(0.0);
    if (!(dot(alb0, alb0) < 1e20)) alb0 = vec3(0.0);
    if (!(dot(n0, n0) < 1e20)) n0 = vec3(0.0);
    vec4 prev = vec4(0.0);
    vec4 prevAlb = vec4(0.0);
    vec4 prevNor = vec4(0.0);
    if (uMuestra > 0) {
        ivec2 c = ivec2(gl_FragCoord.xy);
        prev = texelFetch(uPrev, c, 0);
        prevAlb = texelFetch(uPrevAlb, c, 0);
        prevNor = texelFetch(uPrevNor, c, 0);
    }
    fragColor = prev + (vec4(L, cov) - prev) / float(uMuestra + 1);
    fragAlb = prevAlb + (vec4(alb0, cov) - prevAlb) / float(uMuestra + 1);
    fragNor = prevNor + (vec4(n0, cov) - prevNor) / float(uMuestra + 1);
}
"#;

// Pase de sondas (FS_PREFIJO + este main): un texel por (sonda, cara). Cada muestra es una dirección con
// distribución coseno alrededor de la cara; la irradiancia E = π · media(L) se acumula como el píxel.
// Cara 2k = +eje k, 2k+1 = −eje k.
const MAIN_SONDAS: &str = r#"uniform ivec3 uSondaN;
uniform vec3 uSondaPaso;
void main() {
    int col = int(gl_FragCoord.x);
    int fila = int(gl_FragCoord.y);
    int ix = col / 6;
    int cara = col - 6 * ix;
    int iy = fila % uSondaN.y;
    int iz = fila / uSondaN.y;
    vec3 p = (vec3(float(ix), float(iy), float(iz)) + 0.5) * uSondaPaso;
    int k = cara / 2;
    float signo = cara % 2 == 0 ? 1.0 : -1.0;
    vec3 a = vec3(float(k == 0), float(k == 1), float(k == 2)) * signo;
    // La semilla no depende de la sonda: todas usan la misma secuencia, así el ruido que quede varía suave en el
    // espacio en vez de dar manchas de color de sonda en sonda. La dirección sale de la secuencia R2 (baja
    // discrepancia y progresiva): reparte el hemisferio mucho mejor que el azar puro con las mismas muestras.
    uint estado = uint(cara) * 1973u + uint(uMuestra) * 26699u + 7u;
    rnd(estado);
    float x1 = fract(0.5 + float(uMuestra) * 0.7548776662);
    float x2 = fract(0.5 + float(uMuestra) * 0.5698402910);
    vec3 ta = abs(a.x) > 0.5 ? vec3(0.0, 1.0, 0.0) : vec3(1.0, 0.0, 0.0);
    vec3 s = normalize(cross(ta, a));
    vec3 u = cross(a, s);
    float r = sqrt(x1);
    float phi = 6.2831853 * x2;
    vec3 d = normalize(s * (r * cos(phi)) + u * (r * sin(phi)) + a * sqrt(max(0.0, 1.0 - x1)));
    float cov;
    vec3 alb0;
    vec3 n0;
    vec3 L = camino(p, d, estado, cov, alb0, n0);
    // isnan no es fiable en todos los drivers: una comparación con NaN siempre da falso.
    if (!(L.x >= 0.0 && L.y >= 0.0 && L.z >= 0.0 && L.x < 1e20 && L.y < 1e20 && L.z < 1e20)) L = vec3(0.0);
    vec4 prev = vec4(0.0);
    if (uMuestra > 0) prev = texelFetch(uPrev, ivec2(col, fila), 0);
    fragColor = prev + (vec4(PI * L, 1.0) - prev) / float(uMuestra + 1);
}
"#;

const FS_TONO: &str = r#"#version 330 core
uniform sampler2D uAcum;
uniform float uExposicion;
uniform vec3 uBalance;
out vec4 fragColor;
vec3 aces(vec3 x) {
    x = max(x, vec3(0.0));
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
}
vec3 srgb(vec3 c) {
    return mix(c * 12.92, 1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055, step(0.0031308, c));
}
void main() {
    vec4 c = texelFetch(uAcum, ivec2(gl_FragCoord.xy), 0);
    float a = clamp(c.a, 0.0, 1.0);
    if (a <= 0.0) {
        fragColor = vec4(0.0);
        return;
    }
    vec3 rad = c.rgb / a;
    fragColor = vec4(srgb(aces(rad * uExposicion * uBalance)) * a, a);
}
"#;

/// Base de la cámara (s, u, f) y tangente del semiángulo vertical con la razón de aspecto, sacadas de las matrices.
fn base_camara(p: &ParamsFrame) -> ([f32; 3], [f32; 3], [f32; 3], f32, f32) {
    let v = &p.vista;
    let s = [v[0][0], v[1][0], v[2][0]];
    let u = [v[0][1], v[1][1], v[2][1]];
    let f = [-v[0][2], -v[1][2], -v[2][2]];
    let tan = 1.0 / p.proy[1][1];
    let aspecto = p.proy[1][1] / p.proy[0][0];
    (s, u, f, tan, aspecto)
}

/// Capas de la acumulación de píxeles: color, albedo y normal del primario (COLOR_ATTACHMENT0..2).
const CAPAS_PIXELES: usize = 3;
/// Unidades de las capas anteriores del pase de píxeles (uPrev, uPrevAlb, uPrevNor), relativas a TEXTURE0.
/// 3 y 4 son de la BVH de sombras; 5 de las tablas IES.
const UNIDAD_PREVIA: [u32; 3] = [2, 6, 7];

/// Par de texturas de acumulación (ping-pong): `actual` guarda el último promedio completo.
/// `tex[lado][capa]` con `capas` texturas RGBA32F por lado; cada lado tiene su propio destino (`fbo`).
struct Acumulo {
    w: i32,
    h: i32,
    capas: usize,
    tex: Vec<Vec<glow::Texture>>,
    fbo: Vec<glow::Framebuffer>,
    actual: usize,
}

impl Acumulo {
    fn crear(gl: &glow::Context, w: i32, h: i32, capas: usize) -> Result<Acumulo, String> {
        let mut a = Acumulo { w, h, capas, tex: Vec::new(), fbo: Vec::new(), actual: 0 };
        match a.montar(gl) {
            Ok(()) => Ok(a),
            Err(e) => {
                a.borrar(gl);
                Err(e)
            }
        }
    }

    fn montar(&mut self, gl: &glow::Context) -> Result<(), String> {
        for _ in 0..2 {
            unsafe {
                let fb = gl.create_framebuffer()?;
                self.fbo.push(fb);
                self.tex.push(Vec::new());
                let lado = self.tex.len() - 1;
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fb));
                let mut adjuntos = Vec::new();
                for capa in 0..self.capas {
                    let tex = gl.create_texture()?;
                    self.tex[lado].push(tex);
                    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                    gl.tex_image_2d(
                        glow::TEXTURE_2D,
                        0,
                        glow::RGBA32F as i32,
                        self.w,
                        self.h,
                        0,
                        glow::RGBA,
                        glow::FLOAT,
                        glow::PixelUnpackData::Slice(None),
                    );
                    // NEAREST: sin mipmaps la textura sería incompleta y texelFetch devolvería negro.
                    for (par, val) in [
                        (glow::TEXTURE_MIN_FILTER, glow::NEAREST),
                        (glow::TEXTURE_MAG_FILTER, glow::NEAREST),
                        (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                        (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
                    ] {
                        gl.tex_parameter_i32(glow::TEXTURE_2D, par, val as i32);
                    }
                    let att = glow::COLOR_ATTACHMENT0 + capa as u32;
                    gl.framebuffer_texture_2d(glow::FRAMEBUFFER, att, glow::TEXTURE_2D, Some(tex), 0);
                    adjuntos.push(att);
                }
                gl.draw_buffers(&adjuntos);
                // Se comprueba antes de limpiar: un destino incompleto no se toca.
                let estado = gl.check_framebuffer_status(glow::FRAMEBUFFER);
                if estado != glow::FRAMEBUFFER_COMPLETE {
                    return Err(format!("Framebuffer de acumulación incompleto (estado {estado:#x}, RGBA32F)"));
                }
                gl.clear_color(0.0, 0.0, 0.0, 0.0);
                gl.clear(glow::COLOR_BUFFER_BIT);
            }
        }
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.bind_texture(glow::TEXTURE_2D, None);
        }
        Ok(())
    }

    fn borrar(&self, gl: &glow::Context) {
        unsafe {
            for t in self.tex.iter().flatten() {
                gl.delete_texture(*t);
            }
            for f in &self.fbo {
                gl.delete_framebuffer(*f);
            }
        }
    }
}

/// Lo que se lee del trazado para quitar el ruido: RGB compacto (w·h·3) y la cobertura por píxel (w·h).
#[allow(dead_code)] // shortcut: pendiente de la Tarea C del quitarruido.
pub struct LecturaRuido {
    pub w: usize,
    pub h: usize,
    pub color: Vec<f32>,
    pub albedo: Vec<f32>,
    pub normal: Vec<f32>,
    pub cobertura: Vec<f32>,
}

/// Promedio RGBA de una capa de acumulación → RGB compacto: rgb / cobertura (alfa), 0 sin cobertura. Con `normal`,
/// además se normaliza cada terna (las de longitud casi nula se quedan en cero).
#[allow(dead_code)] // shortcut: pendiente de la Tarea C del quitarruido.
fn rgba_a_rgb(rgba: &[f32], normal: bool) -> Vec<f32> {
    let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);
    for p in rgba.chunks_exact(4) {
        let a = p[3];
        let mut v = if a > 0.0 { [p[0] / a, p[1] / a, p[2] / a] } else { [0.0; 3] };
        if normal {
            let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            if l > 1e-6 {
                v = [v[0] / l, v[1] / l, v[2] / l];
            }
        }
        rgb.extend_from_slice(&v);
    }
    rgb
}

/// Sube un vector de texeles RGBA32F de `ANCHO` columnas (la altura sale de su longitud).
fn subir_textura(gl: &glow::Context, tex: glow::Texture, datos: &[f32]) -> Result<(), String> {
    let b = bytes(datos);
    unsafe {
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA32F as i32,
            ANCHO as i32,
            (datos.len() / (4 * ANCHO)) as i32,
            0,
            glow::RGBA,
            glow::FLOAT,
            glow::PixelUnpackData::Slice(Some(&b)),
        );
        for (par, val) in [
            (glow::TEXTURE_MIN_FILTER, glow::NEAREST),
            (glow::TEXTURE_MAG_FILTER, glow::NEAREST),
            (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
            (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
        ] {
            gl.tex_parameter_i32(glow::TEXTURE_2D, par, val as i32);
        }
        gl.bind_texture(glow::TEXTURE_2D, None);
    }
    Ok(())
}

/// Fallo del trazado. `Gl` (compilar, destino, memoria) baja de preset; `Escena` solo se avisa; `Lento` baja a Alta.
#[derive(Clone, Debug, PartialEq)]
pub enum Fallo {
    Gl(String),
    Escena(String),
    Lento(String),
}

/// Texturas de una BVH subida a la GPU.
struct TexBvh {
    nodos: glow::Texture,
    tris: glow::Texture,
    n_tris: usize,
}

impl TexBvh {
    /// Sube la BVH de `tris` (vacía no sirve: el recorrido necesita al menos un nodo).
    fn subir(&mut self, gl: &glow::Context, tris: &[Tri]) -> Result<(), Fallo> {
        self.n_tris = 0;
        if tris.is_empty() {
            return Err(Fallo::Escena("la escena no tiene triángulos que trazar".to_string()));
        }
        let bvh = construir(tris);
        let datos = empaquetar(&bvh);
        let max = unsafe { gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE) } as usize;
        for d in [&datos.nodos, &datos.tris] {
            let alto = d.len() / (4 * ANCHO);
            if alto > max {
                return Err(Fallo::Escena(format!("la escena no cabe en una textura de GPU ({alto} filas, máximo {max})")));
            }
        }
        subir_textura(gl, self.nodos, &datos.nodos).map_err(Fallo::Gl)?;
        subir_textura(gl, self.tris, &datos.tris).map_err(Fallo::Gl)?;
        self.n_tris = bvh.tris.len();
        Ok(())
    }

    fn borrar(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_texture(self.nodos);
            gl.delete_texture(self.tris);
        }
    }
}

/// Lado de cada tesela (px): un pase dibuja una tesela, así ningún envío se acerca al límite de la GPU.
pub const LADO_TESELA: i32 = 256;
/// Tiempo de GPU por fotograma que se intenta no superar con el trazado (ms).
pub const PRESUPUESTO_MS: f32 = 30.0;
/// Tiempo de GPU por fotograma para las sondas: pequeño, para que la vista Realista siga a 60 fps mientras convergen.
const PRESUPUESTO_SONDAS_MS: f32 = 8.0;

/// Región de un destino que se dibuja de una vez (coordenadas de GL: origen abajo a la izquierda).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tesela {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Teselas de `lado` px que cubren exactamente un destino de `w`×`h` (las de los bordes son más pequeñas).
pub fn teselas(w: i32, h: i32, lado: i32) -> Vec<Tesela> {
    let mut v = Vec::new();
    let mut y = 0;
    while y < h {
        let th = lado.min(h - y);
        let mut x = 0;
        while x < w {
            v.push(Tesela { x, y, w: lado.min(w - x), h: th });
            x += lado;
        }
        y += lado;
    }
    v
}

/// Sondas de irradiancia: su rejilla, la acumulación (ping-pong) y el presupuesto de teselas, como el trazado de píxeles.
struct Sondas {
    prog: glow::Program,
    rej: Rejilla,
    acumulo: Option<Acumulo>,
    /// Muestras completas desde el último reinicio.
    muestras: u32,
    teselas: Vec<Tesela>,
    siguiente: usize,
    por_fotograma: usize,
    consulta: Option<(glow::Query, usize)>,
}

impl Sondas {
    /// Rejilla de `sala` y su destino (solo se rehace si cambian sus dimensiones); reinicia la acumulación.
    fn preparar(&mut self, gl: &glow::Context, sala: [f32; 3]) -> Result<(), String> {
        self.rej = rejilla(sala);
        let (w, h) = dimension_sondas(&self.rej);
        let (w, h) = (w as i32, h as i32);
        if self.acumulo.as_ref().is_none_or(|a| (a.w, a.h) != (w, h)) {
            if let Some(viejo) = self.acumulo.take() {
                viejo.borrar(gl);
            }
            self.acumulo = Some(Acumulo::crear(gl, w, h, 1)?);
            self.teselas = teselas(w, h, LADO_TESELA);
        }
        self.reiniciar();
        Ok(())
    }

    fn reiniciar(&mut self) {
        self.muestras = 0;
        self.siguiente = 0;
    }

    /// Lee sin bloquear el tiempo del pase anterior y ajusta cuántas teselas caben en el presupuesto.
    fn medir(&mut self, gl: &glow::Context) {
        let Some((q, n)) = self.consulta else { return };
        let ns = unsafe {
            if gl.get_query_parameter_u32(q, glow::QUERY_RESULT_AVAILABLE) == 0 {
                return;
            }
            let ns = gl.get_query_parameter_u32(q, glow::QUERY_RESULT);
            gl.delete_query(q);
            ns
        };
        self.consulta = None;
        let por_tesela = (ns as f32 * 1e-6 / n.max(1) as f32).max(1e-3);
        // Varias muestras por fotograma si caben: con una sola, 128 muestras tardaban 128 fotogramas (~2 s).
        let todas = self.teselas.len().max(1) * MUESTRAS_SONDAS as usize;
        self.por_fotograma = ((PRESUPUESTO_SONDAS_MS / por_tesela).floor() as usize).clamp(1, todas);
    }
}

/// Estado de GPU del trazado: programas, BVH de la escena y de sombras, acumulación y medición de tiempo.
pub struct Trazador {
    prog_traza: glow::Program,
    prog_tono: glow::Program,
    /// Escena completa (primarios y rebotes), con los cuerpos de luz.
    escena: TexBvh,
    /// Sombras: la misma geometría que el mapa de sombras del raster (sin cuerpos de luz).
    sombra: TexBvh,
    acumulo: Option<Acumulo>,
    /// Muestras completas desde el último reinicio.
    pub muestras: u32,
    /// Teselas del destino y la siguiente por dibujar de la muestra en curso.
    teselas: Vec<Tesela>,
    siguiente: usize,
    /// Teselas por fotograma que caben en el presupuesto (1 hasta tener una medida).
    pub por_fotograma: usize,
    /// Consulta de tiempo pendiente de leer y las teselas que cubrió.
    consulta: Option<(glow::Query, usize)>,
    sondas: Sondas,
    /// Resultado limpio subido (quitarruido). Solo se muestra si `limpia_visible`; `reiniciar` lo oculta.
    limpia: Option<glow::Texture>,
    limpia_visible: bool,
    /// Sube en cada `reiniciar`: el quitarruido descarta los resultados de una acumulación anterior.
    pub generacion: u64,
}

impl Trazador {
    /// Compila los programas y crea las texturas y el destino. Si algo falla, borra lo creado. `integrada` fija el
    /// arranque conservador (una tesela por fotograma).
    pub fn nuevo(gl: &glow::Context, integrada: bool) -> Result<Trazador, String> {
        let mut r = Recursos::nuevo(gl);
        let prog_traza = r.programa(VS_CUADRO, &[FS_PREFIJO, MAIN_TRAZA].concat())?;
        let prog_sondas = r.programa(VS_CUADRO, &[FS_PREFIJO, MAIN_SONDAS].concat())?;
        let prog_tono = r.programa(VS_CUADRO, FS_TONO)?;
        let escena = TexBvh { nodos: r.textura()?, tris: r.textura()?, n_tris: 0 };
        let sombra = TexBvh { nodos: r.textura()?, tris: r.textura()?, n_tris: 0 };
        let sondas = Sondas {
            prog: prog_sondas,
            rej: rejilla([1.0; 3]),
            acumulo: None,
            muestras: 0,
            teselas: Vec::new(),
            siguiente: 0,
            por_fotograma: 1,
            consulta: None,
        };
        let mut t = Trazador {
            prog_traza,
            prog_tono,
            escena,
            sombra,
            acumulo: None,
            muestras: 0,
            teselas: Vec::new(),
            siguiente: 0,
            por_fotograma: if integrada { 1 } else { 4 },
            consulta: None,
            sondas,
            limpia: None,
            limpia_visible: false,
            generacion: 0,
        };
        t.asegurar_tamano(gl, 4, 4)?;
        r.desarmar();
        Ok(t)
    }

    /// Sube las BVH de la escena (`tris`) y de sombras (`sombra`), y reinicia la acumulación de píxeles y de sondas
    /// (`sala` fija la rejilla de sondas).
    pub fn subir_escena(&mut self, gl: &glow::Context, tris: &[Tri], sombra: &[Tri], sala: [f32; 3]) -> Result<(), Fallo> {
        self.escena.subir(gl, tris)?;
        self.sombra.subir(gl, sombra)?;
        self.reiniciar();
        self.sondas.preparar(gl, sala).map_err(Fallo::Gl)
    }

    /// Vuelve a empezar la acumulación de píxeles (cambió la vista, la exposición, la escena o la configuración).
    pub fn reiniciar(&mut self) {
        self.muestras = 0;
        self.siguiente = 0;
        self.limpia_visible = false;
        self.generacion += 1;
    }

    /// Vuelve a empezar la acumulación de sondas (cambió la escena o los rebotes; la vista no les afecta).
    pub fn reiniciar_sondas(&mut self) {
        self.sondas.reiniciar();
    }

    /// Muestras completas de las sondas desde el último reinicio.
    pub fn sondas_muestras(&self) -> u32 {
        self.sondas.muestras
    }

    /// Teselas por fotograma de las sondas, según el presupuesto medido.
    pub fn sondas_por_fotograma(&self) -> usize {
        self.sondas.por_fotograma
    }

    /// Rejilla de las sondas de la escena subida.
    pub fn sondas_rejilla(&self) -> Rejilla {
        self.sondas.rej
    }

    /// Textura de sondas con la última muestra completa (RGBA32F, una irradiancia por texel).
    pub fn sondas_textura(&self) -> Option<glow::Texture> {
        let a = self.sondas.acumulo.as_ref()?;
        Some(a.tex[a.actual][0])
    }

    /// Irradiancias de las sondas a CPU (lectura bloqueante: solo al converger). None si no hay destino.
    pub fn sondas_leer(&self, gl: &glow::Context) -> Option<SondasCpu> {
        let a = self.sondas.acumulo.as_ref()?;
        let mut bytes_leidos = vec![0u8; (a.w * a.h) as usize * 16];
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(a.fbo[a.actual]));
            gl.read_pixels(0, 0, a.w, a.h, glow::RGBA, glow::FLOAT, glow::PixelPackData::Slice(Some(bytes_leidos.as_mut_slice())));
        }
        let datos = bytes_leidos.chunks_exact(4).map(|c| f32::from_ne_bytes([c[0], c[1], c[2], c[3]])).collect();
        Some(SondasCpu { rej: self.sondas.rej, datos })
    }

    /// Enlaza las texturas de la BVH y de las tablas IES, y los uniforms de luces, rebotes y sala que comparten el
    /// pase de píxeles y el de sondas. El programa `prog` ya está en uso.
    fn enlazar_escena(&self, gl: &glow::Context, prog: glow::Program, paq: &Paquete, ies: glow::Texture, rebotes: u32, sala: [f32; 3]) {
        unsafe {
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.escena.nodos));
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.escena.tris));
            gl.active_texture(glow::TEXTURE3);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.sombra.nodos));
            gl.active_texture(glow::TEXTURE4);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.sombra.tris));
            gl.active_texture(glow::TEXTURE5);
            gl.bind_texture(glow::TEXTURE_2D, Some(ies));
            let loc = |n: &str| gl.get_uniform_location(prog, n);
            gl.uniform_1_i32(loc("uNodos").as_ref(), 0);
            gl.uniform_1_i32(loc("uTris").as_ref(), 1);
            gl.uniform_1_i32(loc("uPrev").as_ref(), 2);
            gl.uniform_1_i32(loc("uNodosS").as_ref(), 3);
            gl.uniform_1_i32(loc("uTrisS").as_ref(), 4);
            gl.uniform_1_i32(loc("uTablaIes").as_ref(), 5);
            gl.uniform_1_f32_slice(loc("uIes[0]").as_ref(), &paq.ies);
            gl.uniform_1_i32(loc("uW").as_ref(), ANCHO as i32);
            gl.uniform_3_f32(loc("uSala").as_ref(), sala[0], sala[1], sala[2]);
            gl.uniform_4_f32_slice(loc("uPosCd[0]").as_ref(), &paq.pos_cd);
            gl.uniform_4_f32_slice(loc("uDirAp[0]").as_ref(), &paq.dir_ap);
            gl.uniform_4_f32_slice(loc("uEjeLuz[0]").as_ref(), &paq.eje);
            gl.uniform_4_f32_slice(loc("uEmisor[0]").as_ref(), &paq.emisor);
            gl.uniform_4_f32_slice(loc("uColor[0]").as_ref(), &paq.color);
            gl.uniform_1_f32_slice(loc("uRadio[0]").as_ref(), &paq.radio);
            gl.uniform_1_i32(loc("uNLuces").as_ref(), paq.n_luces as i32);
            gl.uniform_1_i32(loc("uRebotes").as_ref(), rebotes as i32);
        }
    }

    /// Un pase de sondas: dibuja hasta `presupuesto` teselas de la muestra en curso (ping-pong como el de píxeles).
    /// Llamar solo mientras `sondas_muestras() < MUESTRAS_SONDAS`.
    pub fn avanzar_sondas(
        &mut self,
        gl: &glow::Context,
        vao: glow::VertexArray,
        paq: &Paquete,
        ies: glow::Texture,
        rebotes: u32,
        sala: [f32; 3],
        presupuesto: usize,
    ) -> Result<(), Fallo> {
        if self.escena.n_tris == 0 || self.sombra.n_tris == 0 {
            return Err(Fallo::Escena("la escena no tiene triángulos que trazar".to_string()));
        }
        self.sondas.medir(gl);
        let (w, h) = match &self.sondas.acumulo {
            Some(a) => (a.w, a.h),
            None => return Err(Fallo::Gl("sin destino de sondas".to_string())),
        };
        let prog = self.sondas.prog;
        let rej = self.sondas.rej;
        let medir = self.sondas.consulta.is_none();
        let mut dibujadas = 0usize;
        let consulta = unsafe {
            let consulta = if medir {
                let q = gl.create_query().map_err(Fallo::Gl)?;
                gl.begin_query(glow::TIME_ELAPSED, q);
                Some(q)
            } else {
                None
            };
            gl.disable(glow::BLEND);
            gl.disable(glow::DEPTH_TEST);
            gl.enable(glow::SCISSOR_TEST);
            gl.use_program(Some(prog));
            gl.bind_vertex_array(Some(vao));
            consulta
        };
        self.enlazar_escena(gl, prog, paq, ies, rebotes, sala);
        unsafe {
            let loc = |n: &str| gl.get_uniform_location(prog, n);
            gl.uniform_3_i32(loc("uSondaN").as_ref(), rej.n[0] as i32, rej.n[1] as i32, rej.n[2] as i32);
            gl.uniform_3_f32(loc("uSondaPaso").as_ref(), rej.paso[0], rej.paso[1], rej.paso[2]);
            while dibujadas < presupuesto && self.sondas.muestras < MUESTRAS_SONDAS {
                let s = &mut self.sondas;
                let Some(a) = s.acumulo.as_mut() else { break };
                let otro = 1 - a.actual;
                let (origen, fbo) = (a.tex[a.actual][0], a.fbo[otro]);
                let t = s.teselas[s.siguiente];
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                gl.viewport(0, 0, w, h);
                gl.scissor(t.x, t.y, t.w, t.h);
                gl.active_texture(glow::TEXTURE2);
                gl.bind_texture(glow::TEXTURE_2D, Some(origen));
                gl.uniform_1_i32(loc("uMuestra").as_ref(), s.muestras as i32);
                gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
                gl.flush();
                dibujadas += 1;
                s.siguiente += 1;
                if s.siguiente == s.teselas.len() {
                    s.siguiente = 0;
                    s.muestras += 1;
                    a.actual = otro;
                }
            }
            gl.disable(glow::SCISSOR_TEST);
            if let Some(q) = consulta {
                gl.end_query(glow::TIME_ELAPSED);
                if dibujadas > 0 {
                    self.sondas.consulta = Some((q, dibujadas));
                } else {
                    gl.delete_query(q);
                }
            }
        }
        Ok(())
    }

    /// Número de teselas de un fotograma completo (al menos una).
    pub fn teselas_totales(&self) -> usize {
        self.teselas.len().max(1)
    }

    /// Destino de `w`×`h`; si cambia el tamaño se rehace y la acumulación empieza de cero.
    pub fn asegurar_tamano(&mut self, gl: &glow::Context, w: i32, h: i32) -> Result<(), String> {
        if self.acumulo.as_ref().is_some_and(|a| (a.w, a.h) == (w, h)) {
            return Ok(());
        }
        if let Some(viejo) = self.acumulo.take() {
            viejo.borrar(gl);
        }
        self.acumulo = Some(Acumulo::crear(gl, w, h, CAPAS_PIXELES)?);
        self.teselas = teselas(w, h, LADO_TESELA);
        self.reiniciar();
        Ok(())
    }

    /// Dibuja hasta `presupuesto` teselas de la muestra en curso. Al completar una muestra, su resultado pasa a ser
    /// el promedio visible (ping-pong) y cuenta. Llamar solo si quedan muestras por hacer.
    pub fn avanzar(
        &mut self,
        gl: &glow::Context,
        vao: glow::VertexArray,
        p: &ParamsFrame,
        paq: &Paquete,
        ies: glow::Texture,
        rebotes: u32,
        sala: [f32; 3],
        presupuesto: usize,
        max: u32,
    ) -> Result<(), Fallo> {
        if self.escena.n_tris == 0 || self.sombra.n_tris == 0 {
            return Err(Fallo::Escena("la escena no tiene triángulos que trazar".to_string()));
        }
        let (w, h) = match &self.acumulo {
            Some(a) => (a.w, a.h),
            None => return Err(Fallo::Gl("sin destino de acumulación".to_string())),
        };
        let (s, u, f, tan, aspecto) = base_camara(p);
        let medir = self.consulta.is_none();
        let prog = self.prog_traza;
        let mut dibujadas = 0usize;
        let consulta = unsafe {
            let consulta = if medir {
                let q = gl.create_query().map_err(Fallo::Gl)?;
                gl.begin_query(glow::TIME_ELAPSED, q);
                Some(q)
            } else {
                None
            };
            gl.disable(glow::BLEND);
            gl.disable(glow::DEPTH_TEST);
            gl.enable(glow::SCISSOR_TEST);
            gl.use_program(Some(prog));
            gl.bind_vertex_array(Some(vao));
            consulta
        };
        self.enlazar_escena(gl, prog, paq, ies, rebotes, sala);
        unsafe {
            let loc = |n: &str| gl.get_uniform_location(prog, n);
            gl.uniform_2_f32(loc("uDim").as_ref(), w as f32, h as f32);
            gl.uniform_3_f32(loc("uOjo").as_ref(), p.ojo[0], p.ojo[1], p.ojo[2]);
            gl.uniform_1_f32(loc("uLente").as_ref(), p.lente);
            gl.uniform_1_f32(loc("uFoco").as_ref(), p.foco);
            gl.uniform_1_i32(loc("uPrevAlb").as_ref(), UNIDAD_PREVIA[1] as i32);
            gl.uniform_1_i32(loc("uPrevNor").as_ref(), UNIDAD_PREVIA[2] as i32);
            gl.uniform_3_f32(loc("uEje").as_ref(), s[0], s[1], s[2]);
            gl.uniform_3_f32(loc("uArriba").as_ref(), u[0], u[1], u[2]);
            gl.uniform_3_f32(loc("uAdelante").as_ref(), f[0], f[1], f[2]);
            gl.uniform_1_f32(loc("uTan").as_ref(), tan);
            gl.uniform_1_f32(loc("uAspecto").as_ref(), aspecto);
            while dibujadas < presupuesto && self.muestras < max {
                let Some(a) = self.acumulo.as_mut() else { break };
                let otro = 1 - a.actual;
                let fbo = a.fbo[otro];
                let t = self.teselas[self.siguiente];
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                gl.viewport(0, 0, w, h);
                gl.scissor(t.x, t.y, t.w, t.h);
                for (unidad, previo) in UNIDAD_PREVIA.iter().zip(&a.tex[a.actual]) {
                    gl.active_texture(glow::TEXTURE0 + unidad);
                    gl.bind_texture(glow::TEXTURE_2D, Some(*previo));
                }
                gl.uniform_1_i32(loc("uMuestra").as_ref(), self.muestras as i32);
                gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
                // Cada tesela entra en la cola de la GPU al momento: así el driver no acumula trabajo de golpe.
                gl.flush();
                dibujadas += 1;
                self.siguiente += 1;
                if self.siguiente == self.teselas.len() {
                    self.siguiente = 0;
                    self.muestras += 1;
                    a.actual = otro;
                }
            }
            gl.disable(glow::SCISSOR_TEST);
            if let Some(q) = consulta {
                gl.end_query(glow::TIME_ELAPSED);
                if dibujadas > 0 {
                    self.consulta = Some((q, dibujadas));
                } else {
                    gl.delete_query(q);
                }
            }
        }
        Ok(())
    }

    /// Tiempo (ms) de la última consulta y las teselas que cubrió, si ya tiene resultado. No bloquea.
    pub fn leer_tiempo(&mut self, gl: &glow::Context) -> Option<(f32, usize)> {
        let (q, n) = self.consulta?;
        unsafe {
            if gl.get_query_parameter_u32(q, glow::QUERY_RESULT_AVAILABLE) == 0 {
                return None;
            }
            let ns = gl.get_query_parameter_u32(q, glow::QUERY_RESULT);
            gl.delete_query(q);
            self.consulta = None;
            Some((ns as f32 * 1e-6, n))
        }
    }

    /// Descarta la medida pendiente: ya no describe el trazado con la configuración actual.
    pub fn olvidar_medida(&mut self, gl: &glow::Context) {
        if let Some((q, _)) = self.consulta.take() {
            unsafe { gl.delete_query(q) };
        }
    }

    /// Lee el promedio actual (color, albedo y normal del primario) y lo deja en RGB compacto `w`×`h` para el
    /// quitarruido. Color y albedo = rgb / cobertura (0 sin cobertura); normal normalizada. None sin muestras.
    #[allow(dead_code)] // shortcut: pendiente de la Tarea C del quitarruido.
    pub fn leer_para_quitar_ruido(&self, gl: &glow::Context) -> Option<LecturaRuido> {
        let a = self.acumulo.as_ref()?;
        if self.muestras == 0 {
            return None;
        }
        let (w, h) = (a.w as usize, a.h as usize);
        // Se recuerda el destino de lectura que había y se restaura al final.
        let previo = unsafe { gl.get_parameter_i32(glow::READ_FRAMEBUFFER_BINDING) };
        let mut capas = Vec::with_capacity(CAPAS_PIXELES);
        for capa in 0..CAPAS_PIXELES {
            let mut bytes_leidos = vec![0u8; w * h * 16];
            unsafe {
                gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(a.fbo[a.actual]));
                gl.read_buffer(glow::COLOR_ATTACHMENT0 + capa as u32);
                gl.read_pixels(0, 0, a.w, a.h, glow::RGBA, glow::FLOAT, glow::PixelPackData::Slice(Some(bytes_leidos.as_mut_slice())));
            }
            let datos: Vec<f32> = bytes_leidos.chunks_exact(4).map(|c| f32::from_ne_bytes([c[0], c[1], c[2], c[3]])).collect();
            capas.push(datos);
        }
        let restaurar = std::num::NonZeroU32::new(previo as u32).map(glow::NativeFramebuffer);
        unsafe { gl.bind_framebuffer(glow::READ_FRAMEBUFFER, restaurar) };
        let color = rgba_a_rgb(&capas[0], false);
        let albedo = rgba_a_rgb(&capas[1], false);
        let normal = rgba_a_rgb(&capas[2], true);
        let cobertura = capas[0].chunks_exact(4).map(|p| p[3].clamp(0.0, 1.0)).collect();
        Some(LecturaRuido { w, h, color, albedo, normal, cobertura })
    }

    /// Sube el resultado limpio (RGB compacto `w`×`h`, radiancia sin exposición) y lo muestra `mostrar` hasta el
    /// próximo reinicio. `cobertura` (w·h, la de la lectura) se vuelve a aplicar como en la acumulación (rgb·a, a):
    /// sin ella, el fondo alrededor de la sala saldría negro opaco.
    #[allow(dead_code)] // shortcut: pendiente de la Tarea C del quitarruido.
    pub fn subir_limpia(&mut self, gl: &glow::Context, w: usize, h: usize, rgb: &[f32], cobertura: &[f32]) -> Result<(), String> {
        if rgb.len() != w * h * 3 || cobertura.len() != w * h {
            return Err(format!("Resultado limpio de {} valores; se esperaban {}", rgb.len(), w * h * 3));
        }
        let rgba: Vec<f32> =
            rgb.chunks_exact(3).zip(cobertura).flat_map(|(p, &a)| [p[0] * a, p[1] * a, p[2] * a, a]).collect();
        let b = bytes(&rgba);
        let tex = match self.limpia {
            Some(t) => t,
            None => {
                let t = unsafe { gl.create_texture()? };
                unsafe {
                    gl.bind_texture(glow::TEXTURE_2D, Some(t));
                    for (par, val) in [
                        (glow::TEXTURE_MIN_FILTER, glow::NEAREST),
                        (glow::TEXTURE_MAG_FILTER, glow::NEAREST),
                        (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                        (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
                    ] {
                        gl.tex_parameter_i32(glow::TEXTURE_2D, par, val as i32);
                    }
                }
                self.limpia = Some(t);
                t
            }
        };
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA32F as i32,
                w as i32,
                h as i32,
                0,
                glow::RGBA,
                glow::FLOAT,
                glow::PixelUnpackData::Slice(Some(&b)),
            );
            gl.bind_texture(glow::TEXTURE_2D, None);
        }
        self.limpia_visible = true;
        Ok(())
    }

    /// Pinta el promedio actual (o el resultado limpio, si hay uno vigente) con exposición, ACES y sRGB en `destino`
    /// (de las mismas dimensiones).
    pub fn mostrar(&self, gl: &glow::Context, vao: glow::VertexArray, destino: glow::Framebuffer, exposicion: f32, balance: [f32; 3]) {
        let Some(a) = &self.acumulo else { return };
        let prog = self.prog_tono;
        let tex = match self.limpia {
            Some(l) if self.limpia_visible => l,
            _ => a.tex[a.actual][0],
        };
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(destino));
            gl.viewport(0, 0, a.w, a.h);
            gl.disable(glow::BLEND);
            gl.disable(glow::DEPTH_TEST);
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.use_program(Some(prog));
            gl.bind_vertex_array(Some(vao));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.uniform_1_i32(gl.get_uniform_location(prog, "uAcum").as_ref(), 0);
            gl.uniform_1_f32(gl.get_uniform_location(prog, "uExposicion").as_ref(), exposicion);
            gl.uniform_3_f32(gl.get_uniform_location(prog, "uBalance").as_ref(), balance[0], balance[1], balance[2]);
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
        }
    }

    pub fn destruir(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_program(self.prog_traza);
            gl.delete_program(self.prog_tono);
            gl.delete_program(self.sondas.prog);
            self.escena.borrar(gl);
            self.sombra.borrar(gl);
            if let Some(l) = self.limpia {
                gl.delete_texture(l);
            }
            if let Some((q, _)) = self.consulta {
                gl.delete_query(q);
            }
            if let Some((q, _)) = self.sondas.consulta {
                gl.delete_query(q);
            }
        }
        if let Some(a) = &self.acumulo {
            a.borrar(gl);
        }
        if let Some(a) = &self.sondas.acumulo {
            a.borrar(gl);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generador LCG para escenas y rayos de prueba (determinista).
    struct Azar(u64);

    impl Azar {
        fn f(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            ((self.0 >> 40) as f32) / (1u64 << 24) as f32
        }
        fn punto(&mut self, escala: f32) -> [f32; 3] {
            [self.f() * escala, self.f() * escala, self.f() * escala]
        }
    }

    fn escena_aleatoria(n: usize, seed: u64) -> Vec<Tri> {
        let mut az = Azar(seed);
        (0..n)
            .map(|_| {
                let c = az.punto(10.0);
                let mut v = |_: usize| {
                    let o = az.punto(3.0);
                    [c[0] + o[0] - 1.5, c[1] + o[1] - 1.5, c[2] + o[2] - 1.5]
                };
                Tri { v: [v(0), v(1), v(2)], normal: [0.0, 1.0, 0.0], albedo: [0.5; 3], carcasa: false, piel: false }
            })
            .collect()
    }

    /// Möller–Trumbore, igual que el shader (sin el filtro de carcasa).
    fn golpe(o: [f32; 3], d: [f32; 3], t: &Tri) -> Option<f32> {
        let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
        let cr = |a: [f32; 3], b: [f32; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
        let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let (e1, e2) = (sub(t.v[1], t.v[0]), sub(t.v[2], t.v[0]));
        let pv = cr(d, e2);
        let det = dot(e1, pv);
        if det.abs() < 1e-12 {
            return None;
        }
        let idet = 1.0 / det;
        let tv = sub(o, t.v[0]);
        let u = dot(tv, pv) * idet;
        if !(0.0..=1.0).contains(&u) {
            return None;
        }
        let qv = cr(tv, e1);
        let v = dot(d, qv) * idet;
        if v < 0.0 || u + v > 1.0 {
            return None;
        }
        let t = dot(e2, qv) * idet;
        (t >= 1e-6).then_some(t)
    }

    fn fuerza_bruta(tris: &[Tri], o: [f32; 3], d: [f32; 3]) -> Option<f32> {
        tris.iter().filter_map(|t| golpe(o, d, t)).min_by(|a, b| a.total_cmp(b))
    }

    /// Recorrido de la BVH con pila, igual que el shader (visita primero el hijo cercano según el signo de d[eje]).
    fn recorrido_bvh(bvh: &Bvh, o: [f32; 3], d: [f32; 3]) -> Option<f32> {
        let inv: [f32; 3] = std::array::from_fn(|i| 1.0 / if d[i].abs() < 1e-8 { 1e-8 } else { d[i] });
        let mut pila = vec![0usize];
        let mut mejor = f32::INFINITY;
        let mut visitas = 0;
        while let Some(ni) = pila.pop() {
            visitas += 1;
            assert!(visitas <= 4096, "recorrido demasiado largo");
            let caja = match bvh.nodos[ni] {
                Nodo::Interno { caja, .. } | Nodo::Hoja { caja, .. } => caja,
            };
            let (mut tn, mut tf) = (0.0f32, mejor);
            for i in 0..3 {
                let (a, b) = ((caja.min[i] - o[i]) * inv[i], (caja.max[i] - o[i]) * inv[i]);
                tn = tn.max(a.min(b));
                tf = tf.min(a.max(b));
            }
            if tn > tf {
                continue;
            }
            match bvh.nodos[ni] {
                Nodo::Interno { derecha, eje, .. } => {
                    let (cerca, lejos) = if d[eje] < 0.0 { (derecha, ni + 1) } else { (ni + 1, derecha) };
                    pila.push(lejos);
                    pila.push(cerca);
                }
                Nodo::Hoja { primero, cuenta, .. } => {
                    for t in &bvh.tris[primero..primero + cuenta] {
                        if let Some(th) = golpe(o, d, t) {
                            mejor = mejor.min(th);
                        }
                    }
                }
            }
        }
        mejor.is_finite().then_some(mejor)
    }

    /// Hojas y raíz: cada triángulo aparece una vez y su caja está dentro de la del nodo que lo contiene.
    fn revisar_nodo(bvh: &Bvh, ni: usize, visto: &mut [bool]) -> Caja {
        match bvh.nodos[ni] {
            Nodo::Hoja { caja, primero, cuenta } => {
                assert!(cuenta > 0, "hoja vacía");
                for k in primero..primero + cuenta {
                    assert!(!visto[k], "triángulo {k} repetido");
                    visto[k] = true;
                    let tc = bvh.tris[k].caja();
                    for i in 0..3 {
                        assert!(tc.min[i] >= caja.min[i] - 1e-4 && tc.max[i] <= caja.max[i] + 1e-4);
                    }
                }
                caja
            }
            Nodo::Interno { caja, derecha, .. } => {
                let a = revisar_nodo(bvh, ni + 1, visto);
                let b = revisar_nodo(bvh, derecha, visto);
                for i in 0..3 {
                    assert!(a.min[i] >= caja.min[i] - 1e-4 && b.max[i] <= caja.max[i] + 1e-4);
                }
                caja
            }
        }
    }

    #[test]
    fn bvh_contiene_todos_los_triangulos() {
        let tris = escena_aleatoria(300, 7);
        let bvh = construir(&tris);
        assert_eq!(bvh.tris.len(), tris.len());
        let mut visto = vec![false; tris.len()];
        revisar_nodo(&bvh, 0, &mut visto);
        assert!(visto.iter().all(|&v| v), "algún triángulo no está en ninguna hoja");
        // Las hojas tienen como mucho HOJA_MAX triángulos (la escena aleatoria no llega a la profundidad límite).
        for n in &bvh.nodos {
            if let Nodo::Hoja { cuenta, .. } = n {
                assert!(*cuenta <= HOJA_MAX, "hoja con {cuenta} triángulos");
            }
        }
        assert!(construir(&[]).nodos.is_empty());
    }

    #[test]
    fn bvh_intersecta_como_fuerza_bruta() {
        let tris = escena_aleatoria(400, 11);
        let bvh = construir(&tris);
        let mut az = Azar(99);
        let mut aciertos = 0;
        for _ in 0..1500 {
            let o = az.punto(14.0).map(|c| c - 2.0);
            let d = {
                let v = az.punto(2.0).map(|c| c - 1.0);
                let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
                v.map(|c| c / l)
            };
            let ref_t = fuerza_bruta(&tris, o, d);
            let bvh_t = recorrido_bvh(&bvh, o, d);
            match (ref_t, bvh_t) {
                (None, None) => {}
                (Some(a), Some(b)) => {
                    aciertos += 1;
                    assert!((a - b).abs() < 1e-4, "fuerza bruta {a} vs BVH {b}");
                }
                (a, b) => panic!("acierto distinto: fuerza bruta {a:?}, BVH {b:?}"),
            }
        }
        assert!(aciertos > 100, "pocos aciertos para una prueba útil: {aciertos}");
    }

    #[test]
    fn empaquetado_texturas_tamano() {
        let tris = escena_aleatoria(37, 3);
        let bvh = construir(&tris);
        let datos = empaquetar(&bvh);
        let (w, h) = dimension(2 * bvh.nodos.len());
        assert_eq!(w, ANCHO);
        assert!(w * h >= 2 * bvh.nodos.len());
        assert_eq!(datos.nodos.len(), w * h * 4);
        let (w5, h5) = dimension(5 * tris.len());
        assert_eq!(datos.tris.len(), w5 * h5 * 4);
        assert_eq!(dimension(0), (ANCHO, 1));
        assert_eq!(dimension(ANCHO + 1), (ANCHO, 2));
    }

    #[test]
    fn tri_piel_empaqueta() {
        let mut t = escena_aleatoria(1, 5).remove(0);
        t.piel = true;
        let datos = empaquetar(&construir(&[t]));
        // Triángulo: cinco texeles de cuatro floats; el albedo es el 5.º (índice 4), su 4.º float es la marca.
        assert_eq!(datos.tris[4 * 4 + 3], 1.0);
    }

    #[test]
    fn empaquetar_codifica_el_eje() {
        let bvh = construir(&escena_aleatoria(300, 7));
        let datos = empaquetar(&bvh);
        for (i, nodo) in bvh.nodos.iter().enumerate() {
            // Texel 2·i+1 del nodo: (max.xyz, b); b = −(eje+1) en internos, cuenta en hojas.
            let b = datos.nodos[(2 * i + 1) * 4 + 3];
            match *nodo {
                Nodo::Interno { eje, .. } => assert_eq!(b, -(eje as f32 + 1.0), "nodo {i}"),
                Nodo::Hoja { cuenta, .. } => assert_eq!(b, cuenta as f32, "nodo {i}"),
            }
        }
    }

    #[test]
    fn pcg_determinista() {
        let (mut a, mut b) = (42u32, 42u32);
        let secuencia_a: Vec<u32> = (0..16).map(|_| pcg(&mut a)).collect();
        let secuencia_b: Vec<u32> = (0..16).map(|_| pcg(&mut b)).collect();
        assert_eq!(secuencia_a, secuencia_b, "misma semilla, misma secuencia");
        let mut c = 43u32;
        let otra: Vec<u32> = (0..16).map(|_| pcg(&mut c)).collect();
        assert_ne!(secuencia_a, otra);
        let distintos: std::collections::HashSet<u32> = secuencia_a.iter().copied().collect();
        assert_eq!(distintos.len(), 16, "sin repeticiones cortas");
    }

    /// Cuántas veces cubre cada píxel el reparto en teselas: debe ser exactamente una.
    #[test]
    fn teselas_cubren_cada_pixel_una_vez() {
        for (w, h) in [(1000, 700), (256, 256), (257, 1), (1, 300), (4, 4), (1920, 1080)] {
            let lista = teselas(w, h, LADO_TESELA);
            let mut cuenta = vec![0u8; (w * h) as usize];
            for t in &lista {
                assert!(t.w > 0 && t.h > 0 && t.w <= LADO_TESELA && t.h <= LADO_TESELA, "{t:?}");
                assert!(t.x >= 0 && t.y >= 0 && t.x + t.w <= w && t.y + t.h <= h, "{t:?} fuera de {w}x{h}");
                for y in t.y..t.y + t.h {
                    for x in t.x..t.x + t.w {
                        cuenta[(y * w + x) as usize] += 1;
                    }
                }
            }
            assert!(cuenta.iter().all(|&c| c == 1), "{w}x{h}: algún píxel no se cubre una sola vez");
            assert_eq!(lista.len() as i32, ((w + LADO_TESELA - 1) / LADO_TESELA) * ((h + LADO_TESELA - 1) / LADO_TESELA));
        }
    }

    /// Probabilidad de elegir cada luz: pesos normalizados, o uniforme si no suman nada (espejo de `directa`).
    fn probabilidades(pesos: &[f32]) -> Vec<f32> {
        let total: f32 = pesos.iter().sum();
        if total > 0.0 {
            pesos.iter().map(|w| w / total).collect()
        } else {
            vec![1.0 / pesos.len() as f32; pesos.len()]
        }
    }

    /// Índice elegido con `u` en [0, Σp): primera luz cuyo acumulado supera `u` (espejo del bucle del shader).
    fn elegir(probs: &[f32], u: f32) -> usize {
        let mut acc = 0.0;
        let mut ultimo = 0;
        for (i, p) in probs.iter().enumerate() {
            if *p <= 0.0 {
                continue;
            }
            ultimo = i;
            acc += p;
            if u < acc {
                return i;
            }
        }
        ultimo
    }

    #[test]
    fn probabilidades_de_luz_suman_uno_y_son_proporcionales() {
        // cd·perfil/max(d², 0.01): tres luces, una apagada fuera del cono.
        let pesos = [2.0 * 0.5 / 4.0, 1.0 / 0.005, 0.0, 3.0 * 1.0 / 9.0];
        let p = probabilidades(&pesos);
        assert!((p.iter().sum::<f32>() - 1.0).abs() < 1e-6, "{p:?}");
        assert_eq!(p[2], 0.0);
        let total: f32 = pesos.iter().sum();
        for (pi, wi) in p.iter().zip(pesos) {
            assert!((pi - wi / total).abs() < 1e-6);
        }
        // Sin peso en ninguna luz, la probabilidad es uniforme (la contribución es cero de todos modos).
        let u = probabilidades(&[0.0; 4]);
        assert!(u.iter().all(|&x| (x - 0.25).abs() < 1e-6));
    }

    /// El estimador de una luz (f_k / P_k cuando se elige k) tiene el mismo valor esperado que sumar todas las luces.
    #[test]
    fn seleccion_de_luz_es_insesgada() {
        // Una luz sin peso (fuera del cono) no aporta nada: su contribución es cero.
        let pesos = [0.3f32, 2.0, 0.0, 0.7, 5.0];
        let contribucion = [0.4f32, 1.5, 0.0, 0.2, 0.05];
        let p = probabilidades(&pesos);
        let esperado: f32 = (0..pesos.len()).filter(|&k| p[k] > 0.0).map(|k| p[k] * contribucion[k] / p[k]).sum();
        let suma: f32 = contribucion.iter().sum();
        assert!((esperado - suma).abs() < 1e-4, "{esperado} vs {suma}");
        // La elección por `u` reparte según p: se cuenta sobre una rejilla fina de u.
        let n = 100_000;
        let mut cuenta = vec![0usize; pesos.len()];
        for m in 0..n {
            let u = (m as f32 + 0.5) / n as f32 * p.iter().sum::<f32>();
            cuenta[elegir(&p, u)] += 1;
        }
        for k in 0..pesos.len() {
            let frec = cuenta[k] as f32 / n as f32;
            assert!((frec - p[k]).abs() < 1e-3, "luz {k}: {frec} vs {}", p[k]);
        }
    }

    /// Sondas con irradiancia `f(sonda, cara)` (alfa 1), en el mismo diseño de texel que la textura de la GPU.
    fn sondas_de(n: [usize; 3], f: impl Fn([usize; 3], usize) -> [f32; 3]) -> SondasCpu {
        let rej = Rejilla { n, paso: [1.0; 3] };
        let ancho = 6 * n[0];
        let mut datos = vec![0.0f32; ancho * n[1] * n[2] * 4];
        for iz in 0..n[2] {
            for iy in 0..n[1] {
                for ix in 0..n[0] {
                    for c in 0..6 {
                        let i = ((iy + n[1] * iz) * ancho + 6 * ix + c) * 4;
                        datos[i..i + 3].copy_from_slice(&f([ix, iy, iz], c));
                        datos[i + 3] = 1.0;
                    }
                }
            }
        }
        SondasCpu { rej, datos }
    }

    #[test]
    fn rejilla_de_sondas() {
        // Sala de 6 × 4 × 6 m (ancho, alto, largo): una sonda cada 0.5 m, 12 × 8 × 12 = 1152 sondas.
        let r = rejilla([6.0, 4.0, 6.0]);
        assert_eq!(r.n, [12, 8, 12]);
        assert!(r.paso.iter().all(|&p| (p - 0.5).abs() < 1e-6));
        assert_eq!(dimension_sondas(&r), (72, 96));
        // Tope de 32 por eje: en una sala de 40 m el paso crece.
        let grande = rejilla([40.0, 2.0, 1.0]);
        assert_eq!(grande.n, [32, 4, 2]);
        assert!((grande.paso[0] - 1.25).abs() < 1e-6);
    }

    #[test]
    fn cubo_ambiente_pondera_por_n_cuadrado() {
        // Solo la cara +x emite: con normal (0.6, 0.8, 0) pesa 0.6² = 0.36 de esa cara.
        let s = sondas_de([1, 1, 1], |_, c| if c == 0 { [1.0, 0.5, 0.0] } else { [0.0; 3] });
        let e = s.irradiancia([0.5; 3], [0.6, 0.8, 0.0]);
        assert!((e[0] - 0.36).abs() < 1e-5 && (e[1] - 0.18).abs() < 1e-5, "{e:?}");
        // Con normal −x no ve la cara +x.
        assert!(s.irradiancia([0.5; 3], [-1.0, 0.0, 0.0])[0].abs() < 1e-6);
        // Los pesos n² suman 1: con todas las caras iguales, la irradiancia no depende de la normal.
        let igual = sondas_de([1, 1, 1], |_, _| [0.3; 3]);
        let e = igual.irradiancia([0.5; 3], [0.48, -0.6, 0.64]);
        assert!((e[0] - 0.3).abs() < 1e-5, "{e:?}");
    }

    #[test]
    fn trilineal_interpola_entre_sondas() {
        // Dos sondas en x (centros 0.5 y 1.5 m), con irradiancia 0 y 1 en todas sus caras.
        let s = sondas_de([2, 1, 1], |q, _| [q[0] as f32; 3]);
        let n = [0.0, 1.0, 0.0];
        assert!((s.irradiancia([1.0, 0.5, 0.5], n)[0] - 0.5).abs() < 1e-5);
        assert!(s.irradiancia([0.5, 0.5, 0.5], n)[0].abs() < 1e-5);
        // Fuera de la rejilla se repite la sonda del borde.
        assert!(s.irradiancia([0.0, 0.5, 0.5], n)[0].abs() < 1e-5);
        assert!((s.irradiancia([2.0, 0.5, 0.5], n)[0] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn rgba_a_rgb_divide_por_cobertura_y_normaliza() {
        // Color y albedo: rgb / cobertura; sin cobertura, cero.
        let rgba = [0.5, 1.0, 0.25, 0.5, 0.0, 0.0, 0.0, 0.0];
        assert_eq!(rgba_a_rgb(&rgba, false), vec![1.0, 2.0, 0.5, 0.0, 0.0, 0.0]);
        // Normal: dividida y normalizada; una terna nula sigue en cero.
        let nor = [0.0, 0.0, 2.0, 1.0, 0.0, 0.0, 0.0, 0.0];
        assert_eq!(rgba_a_rgb(&nor, true), vec![0.0, 0.0, 1.0, 0.0, 0.0, 0.0]);
    }
}
