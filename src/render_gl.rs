//! Render 3D en GPU (OpenGL 3.3 core): sombras por mapa de profundidad, perfil de haz,
//! luz indirecta, exposición y mapeo de tonos. Se dibuja dentro de la vista de egui con un `PaintCallback`.

use crate::config::{es_integrada, Configuracion, Preset};
use crate::escena::VerticeGpu;
use crate::fotometria::{muestras_ies, FuenteLuz, IES_P, IES_T};
use crate::geom::{Camara, V3};
use crate::trazado::{self, Fallo, Rejilla, SondasCpu, Trazador, Tri, MUESTRAS_SONDAS, PRESUPUESTO_MS};
use eframe::glow::{self, HasContext};
use std::sync::Arc;

/// Matriz 4×4 en orden de columnas (`m[columna][fila]`), tal como la espera GL sin transponer.
pub type Mat4 = [[f32; 4]; 4];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModoVista {
    Realista,
    MapaLux,
    Esquema,
}

/// Luces que llegan al shader (uniform arrays).
/// shortcut: más de 32 luces se ignoran en la GPU, subir MAX_LUCES si hiciera falta.
pub const MAX_LUCES: usize = 32;
/// Mapas de sombra: solo las 16 primeras luces proyectan sombra; las demás iluminan sin sombra.
pub const MAX_SOMBRAS: usize = 16;
const CERCA: f32 = 0.05;
const LEJOS_CAMARA: f32 = 300.0;
const LEJOS_SOMBRA: f32 = 30.0;
/// Longitud máxima del cono de humo (m) cuando la luz apunta hacia arriba o muy lejos.
const LARGO_HAZ: f32 = 8.0;
const SEGMENTOS_CONO: usize = 32;
/// Escala del humo: candelas × esto = radiancia de dispersión (calibrada a ojo; se subió ~7× para que el haz se vea a exposición normal).
const GANANCIA_HAZ: f32 = 0.002;

/// Todo lo que cambia cada fotograma y no requiere volver a subir la malla.
#[derive(Clone)]
pub struct ParamsFrame {
    pub vista: Mat4,
    pub proy: Mat4,
    pub ojo: [f32; 3],
    /// Viewport real en píxeles de GL: [izquierda, desde_abajo, ancho, alto].
    pub viewport_px: [i32; 4],
    pub fuentes: Vec<FuenteLuz>,
    pub indirecta_lux: f32,
    pub color_indirecta: [f32; 3],
    pub exposicion: f32,
    /// Ganancia por canal del balance de blancos (se multiplica a la radiancia con la exposición).
    pub balance: [f32; 3],
    /// Radio de la pupila en m para la lente fina del trazado; 0 = estenopeica.
    pub lente: f32,
    /// Distancia del plano de foco en m (solo con `lente` > 0).
    pub foco: f32,
    pub modo: ModoVista,
    pub humo: bool,
}

fn mul(a: Mat4, b: Mat4) -> Mat4 {
    std::array::from_fn(|c| std::array::from_fn(|r| (0..4).map(|k| a[k][r] * b[c][k]).sum()))
}

fn perspectiva(fovy: f32, aspecto: f32, cerca: f32, lejos: f32) -> Mat4 {
    let f = 1.0 / (fovy * 0.5).tan();
    let k = 1.0 / (cerca - lejos);
    [
        [f / aspecto, 0.0, 0.0, 0.0],
        [0.0, f, 0.0, 0.0],
        [0.0, 0.0, (lejos + cerca) * k, -1.0],
        [0.0, 0.0, 2.0 * lejos * cerca * k, 0.0],
    ]
}

/// Vista con ejes (s, u) y hacia adelante `f` desde `ojo`; la convención OpenGL mira hacia −Z.
fn vista_desde(ojo: V3, f: V3, s: V3, u: V3) -> Mat4 {
    [
        [s.x, u.x, -f.x, 0.0],
        [s.y, u.y, -f.y, 0.0],
        [s.z, u.z, -f.z, 0.0],
        [-s.dot(ojo), -u.dot(ojo), f.dot(ojo), 1.0],
    ]
}

/// Vista de la cámara, con la misma base que `Camara::proyectar`.
pub fn matriz_vista(cam: &Camara) -> Mat4 {
    let (f, s, u) = cam.base();
    vista_desde(cam.ojo(), f, s, u)
}

/// Proyección de la cámara (mismo fov vertical que `Camara::proyectar`).
pub fn matriz_proyeccion(cam: &Camara, aspecto: f32) -> Mat4 {
    perspectiva(cam.fov_y, aspecto, CERCA, LEJOS_CAMARA)
}

/// Proyección·vista de la luz: cono de apertura `min(170°, 1.6·apertura)` mirando en `dir`.
pub fn matriz_luz(f: &FuenteLuz) -> Mat4 {
    let dir = V3::from_arr(f.dir).norm();
    let arriba = if dir.y.abs() > 0.99 { V3::new(0.0, 0.0, 1.0) } else { V3::new(0.0, 1.0, 0.0) };
    let s = dir.cross(arriba).norm();
    let u = s.cross(dir);
    let fov = (1.6 * f.apertura).clamp(1.0, 170.0).to_radians();
    mul(perspectiva(fov, 1.0, CERCA, LEJOS_SOMBRA), vista_desde(V3::from_arr(f.pos), dir, s, u))
}

/// Cono de humo de una luz, en espacio unitario (vértice `z` = 0 en el ápice, 1 en la base).
/// Transforma con: eje `dir`, longitud hasta el suelo (máx. `LARGO_HAZ`), radio = largo·tan(apertura/2).
fn modelo_cono(f: &FuenteLuz) -> Mat4 {
    let dir = V3::from_arr(f.dir).norm();
    let pos = V3::from_arr(f.pos);
    let largo = if dir.y < -1e-3 { (-pos.y / dir.y).clamp(0.0, LARGO_HAZ) } else { LARGO_HAZ };
    let radio = largo * (f.apertura.to_radians() * 0.5).tan();
    let a = if dir.y.abs() < 0.9 { V3::new(0.0, 1.0, 0.0) } else { V3::new(1.0, 0.0, 0.0) };
    let w = dir.cross(a).norm();
    let (u, v) = (w * radio, w.cross(dir) * radio);
    let d = dir * largo;
    [[u.x, u.y, u.z, 0.0], [v.x, v.y, v.z, 0.0], [d.x, d.y, d.z, 0.0], [pos.x, pos.y, pos.z, 1.0]]
}

/// Color medio de las luces ponderado por candelas (color de la luz rebotada).
pub fn color_medio(fs: &[FuenteLuz]) -> [f32; 3] {
    let total: f32 = fs.iter().map(|f| f.cd).sum();
    if !(total > 0.0) {
        return [1.0; 3];
    }
    let mut c = [0.0f32; 3];
    for f in fs {
        for i in 0..3 {
            c[i] += f.color[i] * f.cd / total;
        }
    }
    c
}

/// Anchura de penumbra (m) en el receptor: (d_receptor − d_bloqueador)·radio / d_bloqueador.
/// Es la fórmula que aplica el shader PCSS; la prueba la fija. Sin bloqueador a distancia > 0 se devuelve ∞.
#[allow(dead_code)] // solo referencia: el cálculo real está en `sombra_pcss`
pub fn tamano_penumbra(d_receptor: f32, d_bloqueador: f32, radio: f32) -> f32 {
    if !(d_bloqueador > 0.0) {
        return f32::INFINITY;
    }
    ((d_receptor - d_bloqueador) * radio / d_bloqueador).max(0.0)
}

/// Arrays de uniforms: (x, y, z, w) por luz, rellenados con ceros hasta MAX_LUCES.
pub(crate) struct Paquete {
    pub(crate) pos_cd: Vec<f32>,
    pub(crate) dir_ap: Vec<f32>,
    pub(crate) color: Vec<f32>,
    pub(crate) radio: Vec<f32>,
    /// (u.xyz, nitidez) por luz: base del emisor y forma del borde.
    pub(crate) eje: Vec<f32>,
    /// (hu, hv, 0, 0) por luz: medias del emisor rectangular; 0 para disco.
    pub(crate) emisor: Vec<f32>,
    /// 1 si la luz tiene perfil IES (su tabla está en la textura `uTablaIes`), 0 si usa la campana.
    pub(crate) ies: Vec<f32>,
    pub(crate) n_luces: usize,
    n_sombras: usize,
}

fn empaquetar(fs: &[FuenteLuz]) -> Paquete {
    let n = fs.len().min(MAX_LUCES);
    let mut paq = Paquete {
        pos_cd: vec![0.0; 4 * MAX_LUCES],
        dir_ap: vec![0.0; 4 * MAX_LUCES],
        color: vec![0.0; 4 * MAX_LUCES],
        radio: vec![0.0; MAX_LUCES],
        eje: vec![0.0; 4 * MAX_LUCES],
        emisor: vec![0.0; 4 * MAX_LUCES],
        ies: vec![0.0; MAX_LUCES],
        n_luces: n,
        n_sombras: n.min(MAX_SOMBRAS),
    };
    for (i, f) in fs[..n].iter().enumerate() {
        paq.pos_cd[4 * i..4 * i + 4].copy_from_slice(&[f.pos[0], f.pos[1], f.pos[2], f.cd]);
        paq.dir_ap[4 * i..4 * i + 4].copy_from_slice(&[f.dir[0], f.dir[1], f.dir[2], f.apertura]);
        paq.color[4 * i..4 * i + 4].copy_from_slice(&[f.color[0], f.color[1], f.color[2], 0.0]);
        paq.radio[i] = f.radio;
        paq.eje[4 * i..4 * i + 4].copy_from_slice(&[f.eje_u[0], f.eje_u[1], f.eje_u[2], f.nitidez]);
        paq.emisor[4 * i..4 * i + 4].copy_from_slice(&[f.medio[0], f.medio[1], 0.0, 0.0]);
        paq.ies[i] = if f.ies.is_some() { 1.0 } else { 0.0 };
    }
    paq
}

/// Sube las tablas IES: la luz i ocupa las filas `IES_P·i` … `IES_P·i + IES_P − 1`. Sin IES, ceros.
/// Filtrado lineal (θ se interpola por hardware) y CLAMP_TO_EDGE; φ se envuelve en el shader.
fn subir_ies(gl: &glow::Context, tex: glow::Texture, fs: &[FuenteLuz]) {
    let bloque = IES_T * IES_P;
    let mut datos = vec![0.0f32; bloque * MAX_LUCES];
    for (i, f) in fs.iter().take(MAX_LUCES).enumerate() {
        if let Some(ies) = &f.ies {
            datos[i * bloque..(i + 1) * bloque].copy_from_slice(&muestras_ies(ies));
        }
    }
    unsafe {
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::R32F as i32,
            IES_T as i32,
            (IES_P * MAX_LUCES) as i32,
            0,
            glow::RED,
            glow::FLOAT,
            glow::PixelUnpackData::Slice(Some(&bytes(&datos))),
        );
        for (par, val) in [
            (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
            (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
            (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
            (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
        ] {
            gl.tex_parameter_i32(glow::TEXTURE_2D, par, val as i32);
        }
        gl.bind_texture(glow::TEXTURE_2D, None);
    }
}

const VS_ESCENA: &str = r#"#version 330 core
layout(location = 0) in vec3 aPos;
layout(location = 1) in vec3 aNormal;
layout(location = 2) in vec3 aAlbedo;
layout(location = 3) in float aCarcasa;
uniform mat4 uVista;
uniform mat4 uProy;
out vec3 vPos;
out vec3 vNormal;
out vec3 vAlbedo;
out float vCarcasa;
void main() {
    vPos = aPos;
    vNormal = aNormal;
    vAlbedo = aAlbedo;
    vCarcasa = aCarcasa;
    gl_Position = uProy * uVista * vec4(aPos, 1.0);
}
"#;

const FS_ESCENA: &str = r#"#version 330 core
#define MAX_LUCES 32
#define MAX_SOMBRAS 16
in vec3 vPos;
in vec3 vNormal;
in vec3 vAlbedo;
in float vCarcasa;
out vec4 fragColor;
uniform vec3 uOjo;
uniform vec4 uPosCd[MAX_LUCES];
uniform vec4 uDirAp[MAX_LUCES];
uniform vec4 uEjeLuz[MAX_LUCES];
uniform vec4 uEmisor[MAX_LUCES];
uniform vec4 uColor[MAX_LUCES];
uniform int uNLuces;
uniform int uNSombras;
uniform mat4 uLuz[MAX_SOMBRAS];
uniform sampler2DArray uSombras;
uniform float uRadio[MAX_LUCES];
// Tablas IES (fotometria::muestras_ies): 64 θ × 32 φ por luz; uIes[i] = 1 si la luz tiene una.
uniform sampler2D uTablaIes;
uniform float uIes[MAX_LUCES];
uniform int uPcss;
uniform vec3 uIndirecta;
uniform float uExposicion;
uniform vec3 uBalance;
uniform int uModo;
// Sondas de irradiancia (trazado.rs, MAIN_SONDAS): 6 texeles por sonda (caras +x,−x,+y,−y,+z,−z), una fila por (y, z).
uniform sampler2D uSondas;
uniform int uSondaOk;
uniform ivec3 uSondaN;
uniform vec3 uSondaPaso;

// Planos de la proyección de luz (los mismos CERCA y LEJOS_SOMBRA de matriz_luz).
const float CERCA_L = 0.05;
const float LEJOS_L = 30.0;

// Profundidad lineal (m a lo largo del eje de la luz) de un valor del mapa de profundidad.
float prof_lineal(float d) {
    float ndc = 2.0 * d - 1.0;
    return 2.0 * CERCA_L * LEJOS_L / (LEJOS_L + CERCA_L - ndc * (LEJOS_L - CERCA_L));
}

// Ruido de gradiente intercalado: gira el patrón de muestras de cada píxel.
float ruido_pixel() {
    return fract(52.9829189 * fract(dot(gl_FragCoord.xy, vec2(0.06711056, 0.00583715))));
}

// Disco de Vogel (espiral dorada) de n muestras, girado `giro`: muestras bien repartidas sin tabla.
vec2 muestra_disco(int i, int n, float giro) {
    float r = sqrt((float(i) + 0.5) / float(n));
    float a = float(i) * 2.3999632 + giro;
    return r * vec2(cos(a), sin(a));
}

// Profundidad lineal del plano del receptor (normal n, h = (punto − luz) · n) en la muestra `t` del mapa.
// Comparar cada muestra con la profundidad del centro hace que una superficie inclinada se dé sombra a sí misma
// (vetas); con la del plano no. Acotada a [dr/2, 2·dr] para rasantes y esquinas.
float prof_plano(vec2 t, vec3 Ld, vec3 Ls, vec3 Lu, float invP, vec3 n, float h, float dr) {
    vec2 c = t * 2.0 - 1.0;
    float den = dot(Ld + (Ls * c.x + Lu * c.y) * invP, n);
    if (den * h <= 1e-9) return dr;
    return clamp(h / den, 0.5 * dr, 2.0 * dr);
}

// Sombra suave (PCSS): 16 muestras buscan bloqueadores, la penumbra sale del radio del emisor
// y 32 muestras filtran con ese radio. Todas las distancias son lineales (m) sobre el eje de la luz.
float sombra_pcss(int i, vec3 p, vec3 n, float ndl) {
    if (i >= uNSombras) return 1.0;
    vec4 q = uLuz[i] * vec4(p, 1.0);
    if (q.w <= 0.0) return 1.0;
    // Escala f: la fila 0 de proy·vista es f·s y s es unitario, así que su longitud no depende del giro.
    float P = length(vec3(uLuz[i][0][0], uLuz[i][1][0], uLuz[i][2][0]));
    float res = float(textureSize(uSombras, 0).x);
    // Desplazamiento a lo largo de la normal de ~un texel: evita el acné en superficies rasantes.
    float texel_m = 2.0 * q.w / (P * res);
    vec3 po = p + n * texel_m * (0.5 + (1.0 - ndl));
    q = uLuz[i] * vec4(po, 1.0);
    // Base de la luz, la misma de matriz_luz.
    vec3 Ld = normalize(uDirAp[i].xyz);
    vec3 La = vec3(0.0, 1.0, 0.0);
    if (abs(Ld.y) > 0.99) La = vec3(0.0, 0.0, 1.0);
    vec3 Ls = normalize(cross(Ld, La));
    vec3 Lu = cross(Ls, Ld);
    float hp = dot(po - uPosCd[i].xyz, n);
    if (q.w <= 0.0) return 1.0;
    vec3 ndc = q.xyz / q.w;
    if (abs(ndc.x) > 1.0 || abs(ndc.y) > 1.0 || ndc.z > 1.0) return 1.0;
    float dr = q.w;
    float eps = texel_m * 0.25;
    vec2 uv = ndc.xy * 0.5 + 0.5;
    float giro = 6.2831853 * ruido_pixel();
    // Radio de búsqueda: el emisor visto desde el receptor (acotado a 0.03 uv).
    float radio_busq = min(P * uRadio[i] / dr, 0.03);
    float suma = 0.0;
    float cuenta = 0.0;
    for (int k = 0; k < 16; k++) {
        vec2 t = uv + muestra_disco(k, 16, giro) * radio_busq;
        if (any(lessThan(t, vec2(0.0))) || any(greaterThan(t, vec2(1.0)))) continue;
        float db = prof_lineal(textureLod(uSombras, vec3(t, float(i)), 0.0).r);
        if (db < prof_plano(t, Ld, Ls, Lu, 1.0 / P, n, hp, dr) - eps) {
            suma += db;
            cuenta += 1.0;
        }
    }
    if (cuenta == 0.0) return 1.0;
    float db_medio = suma / cuenta;
    // Penumbra en el receptor (m), pasada a texeles y acotada a [1, 40].
    float penumbra = (dr - db_medio) * uRadio[i] / db_medio;
    float r_tex = clamp(penumbra * 0.5 * P / dr * res, 1.0, 40.0);
    float lit = 0.0;
    for (int k = 0; k < 32; k++) {
        vec2 t = uv + muestra_disco(k, 32, giro) * r_tex / res;
        float db = prof_lineal(textureLod(uSombras, vec3(t, float(i)), 0.0).r);
        lit += (db >= prof_plano(t, Ld, Ls, Lu, 1.0 / P, n, hp, dr) - eps) ? 1.0 : 0.0;
    }
    return lit / 32.0;
}

float sombra_de(int i, vec3 p, float bias) {
    if (i >= uNSombras) return 1.0;
    vec4 q = uLuz[i] * vec4(p, 1.0);
    if (q.w <= 0.0) return 1.0;
    vec3 ndc = q.xyz / q.w;
    if (abs(ndc.x) > 1.0 || abs(ndc.y) > 1.0 || ndc.z > 1.0) return 1.0;
    float z = ndc.z * 0.5 + 0.5 - bias;
    vec2 uv = ndc.xy * 0.5 + 0.5;
    vec2 texel = 1.0 / vec2(textureSize(uSombras, 0).xy);
    float s = 0.0;
    for (int y = -1; y <= 1; y++) {
        for (int x = -1; x <= 1; x++) {
            float d = textureLod(uSombras, vec3(uv + vec2(x, y) * texel, float(i)), 0.0).r;
            s += (z <= d) ? 1.0 : 0.0;
        }
    }
    return s / 9.0;
}

vec3 aces(vec3 x) {
    x = max(x, vec3(0.0));
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
}

vec3 srgb(vec3 c) {
    return mix(c * 12.92, 1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055, step(0.0031308, c));
}

// Misma escala que fotometria::color_lux: negro, azul 50, verde 200, amarillo 500, naranja 1000, rojo 2000+.
vec3 escala_lux(float lux) {
    if (!(lux > 0.0)) return vec3(0.0);
    if (lux < 50.0) return mix(vec3(0.0), vec3(0.0, 0.0, 1.0), lux / 50.0);
    if (lux < 200.0) return mix(vec3(0.0, 0.0, 1.0), vec3(0.0, 1.0, 0.0), log(lux / 50.0) / log(4.0));
    if (lux < 500.0) return mix(vec3(0.0, 1.0, 0.0), vec3(1.0, 1.0, 0.0), log(lux / 200.0) / log(2.5));
    if (lux < 1000.0) return mix(vec3(1.0, 1.0, 0.0), vec3(1.0, 0.5, 0.0), log(lux / 500.0) / log(2.0));
    if (lux < 2000.0) return mix(vec3(1.0, 0.5, 0.0), vec3(1.0, 0.0, 0.0), log(lux / 1000.0) / log(2.0));
    return vec3(1.0, 0.0, 0.0);
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

// Perfil IES de la luz i hacia p: candelas / máximo de la tabla (fotometria::muestras_ies), igual que cd_dir en CPU
// (θ al eje dir, φ desde u hacia v). θ lo interpola la textura; φ se envuelve a mano entre filas, así que no
// hace falta REPEAT ni una franja extra.
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

// Perfil de la luz i: la tabla IES si la tiene, si no la campana del haz.
float perfil_de(int i, vec3 p) {
    return uIes[i] > 0.5 ? perfil_ies(i, p) : perfil_haz(i, p);
}

// Irradiancia de una sonda con normal n: cubo ambiente, E = Σ n_k² · E(cara +k si n_k ≥ 0, si no −k).
vec3 cubo_sonda(ivec3 q, vec3 n) {
    vec3 E = vec3(0.0);
    for (int k = 0; k < 3; k++) {
        int cara = 2 * k + (n[k] < 0.0 ? 1 : 0);
        int fila = q.y + uSondaN.y * q.z;
        vec3 t = texelFetch(uSondas, ivec2(6 * q.x + cara, fila), 0).rgb;
        // Una sonda no finita (0 · NaN = NaN) apagaría todo lo que la interpola: se ignora.
        if (t.x >= 0.0 && t.y >= 0.0 && t.z >= 0.0 && t.x < 1e20 && t.y < 1e20 && t.z < 1e20) E += n[k] * n[k] * t;
    }
    return E;
}

// Interpolación trilineal de las 8 sondas que rodean p (en los bordes se repite la sonda del borde).
vec3 irradiancia_sondas(vec3 p, vec3 n) {
    vec3 g = clamp(p / uSondaPaso - 0.5, vec3(0.0), vec3(uSondaN) - 1.0);
    ivec3 i0 = ivec3(floor(g));
    vec3 f = g - vec3(i0);
    ivec3 i1 = min(i0 + 1, uSondaN - 1);
    vec3 E = vec3(0.0);
    for (int dz = 0; dz < 2; dz++) {
        for (int dy = 0; dy < 2; dy++) {
            for (int dx = 0; dx < 2; dx++) {
                ivec3 q = ivec3(dx == 1 ? i1.x : i0.x, dy == 1 ? i1.y : i0.y, dz == 1 ? i1.z : i0.z);
                float w = (dx == 1 ? f.x : 1.0 - f.x) * (dy == 1 ? f.y : 1.0 - f.y) * (dz == 1 ? f.z : 1.0 - f.z);
                E += w * cubo_sonda(q, n);
            }
        }
    }
    return E;
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

void main() {
    // Normal nula (malla degenerada): se usa "arriba" para no producir NaN.
    vec3 n = dot(vNormal, vNormal) > 1e-12 ? normalize(vNormal) : vec3(0.0, 1.0, 0.0);
    vec3 p = vPos;
    // Carcasa (normal hacia dentro): desde fuera se recorta la cara que mira a la cámara, así se ve la sala.
    bool de_fuera = dot(n, uOjo - p) < 0.0;
    if (vCarcasa > 0.5 && vCarcasa < 1.5) {
        if (de_fuera) discard;
    } else if (de_fuera) {
        n = -n;
    }
    bool piel = vCarcasa > 1.5;
    vec3 Eind = uSondaOk == 1 ? irradiancia_sondas(p, n) : uIndirecta;
    vec3 E = Eind;
    vec3 Lpiel = vec3(0.0);
    vec3 v = normalize(uOjo - p);
    for (int i = 0; i < uNLuces; i++) {
        vec3 C = uPosCd[i].xyz - p;
        float l2 = dot(C, C);
        if (l2 < 1e-6) continue;
        float d2 = max(l2, 0.01);
        float per = perfil_de(i, p);
        vec3 l = C * inversesqrt(l2);
        float nl = dot(n, l);
        // La piel deja pasar un poco la luz más allá del terminador (envoltura).
        if (per <= 0.0 || nl <= (piel ? -0.30 : 0.0)) continue;
        float ndl = max(nl, 0.0);
        float bias = max(0.0015 * (1.0 - ndl), 0.0003);
        float s = uPcss == 1 ? sombra_pcss(i, p, n, ndl) : sombra_de(i, p, bias);
        vec3 Eperp = uColor[i].rgb * (uPosCd[i].w * per / d2 * campo_cercano(i, p, n) * s);
        E += Eperp * ndl;
        if (piel) Lpiel += piel_brdf(n, v, l, vAlbedo, Eperp);
    }
    if (uModo == 1) {
        float lum = dot(E, vec3(0.2126, 0.7152, 0.0722));
        float sh = 0.75 + 0.25 * clamp(n.y, 0.0, 1.0);
        fragColor = vec4(escala_lux(lum) * sh, 1.0);
    } else {
        // 0.972 = 1 − F0: la parte difusa de la piel pierde lo que refleja la capa grasa.
        vec3 rad = piel ? vAlbedo * Eind * (0.972 / 3.14159265) + Lpiel : vAlbedo * E / 3.14159265;
        fragColor = vec4(srgb(aces(rad * uExposicion * uBalance)), 1.0);
    }
}
"#;

const VS_SOMBRA: &str = r#"#version 330 core
layout(location = 0) in vec3 aPos;
uniform mat4 uLuz;
void main() {
    gl_Position = uLuz * vec4(aPos, 1.0);
}
"#;

const FS_SOMBRA: &str = r#"#version 330 core
void main() {}
"#;

const VS_HUMO: &str = r#"#version 330 core
layout(location = 0) in vec3 aPos;
uniform mat4 uVista;
uniform mat4 uProy;
uniform mat4 uModelo;
out float vT;
void main() {
    vT = aPos.z;
    gl_Position = uProy * uVista * uModelo * vec4(aPos, 1.0);
}
"#;

const FS_HUMO: &str = r#"#version 330 core
in float vT;
uniform vec3 uColor;
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
    float fade = (1.0 - vT) * (1.0 - vT);
    fragColor = vec4(srgb(aces(uColor * uExposicion * uBalance * fade)), 1.0);
}
"#;

// Cuadro de la vista: triángulo en banda que cubre el viewport; el color llega premultiplicado.
pub(crate) const VS_CUADRO: &str = r#"#version 330 core
out vec2 vUv;
void main() {
    vec2 p = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));
    vUv = p;
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
"#;

const FS_CUADRO: &str = r#"#version 330 core
in vec2 vUv;
uniform sampler2D uTex;
out vec4 fragColor;
void main() {
    fragColor = texture(uTex, vUv);
}
"#;

pub(crate) fn bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_ne_bytes()).collect()
}

fn plano(m: &Mat4) -> &[f32] {
    m.as_flattened()
}

/// Recursos de GL creados durante una construcción. Si la construcción falla, `Drop` los borra; `desarmar` los
/// suelta cuando todo salió bien (los dueños pasan a ser quien los guarda).
pub(crate) struct Recursos<'a> {
    gl: &'a glow::Context,
    programas: Vec<glow::Program>,
    vaos: Vec<glow::VertexArray>,
    buffers: Vec<glow::Buffer>,
    texturas: Vec<glow::Texture>,
    fbos: Vec<glow::Framebuffer>,
}

impl<'a> Recursos<'a> {
    pub(crate) fn nuevo(gl: &'a glow::Context) -> Recursos<'a> {
        Recursos { gl, programas: Vec::new(), vaos: Vec::new(), buffers: Vec::new(), texturas: Vec::new(), fbos: Vec::new() }
    }

    pub(crate) fn programa(&mut self, vs_src: &str, fs_src: &str) -> Result<glow::Program, String> {
        let p = programa(self.gl, vs_src, fs_src)?;
        self.programas.push(p);
        Ok(p)
    }

    pub(crate) fn vao(&mut self) -> Result<glow::VertexArray, String> {
        let v = unsafe { self.gl.create_vertex_array()? };
        self.vaos.push(v);
        Ok(v)
    }

    pub(crate) fn buffer(&mut self) -> Result<glow::Buffer, String> {
        let b = unsafe { self.gl.create_buffer()? };
        self.buffers.push(b);
        Ok(b)
    }

    pub(crate) fn textura(&mut self) -> Result<glow::Texture, String> {
        let t = unsafe { self.gl.create_texture()? };
        self.texturas.push(t);
        Ok(t)
    }

    /// Registra una textura creada por otra función (si falla la construcción, también se borra).
    pub(crate) fn registrar_textura(&mut self, t: glow::Texture) -> glow::Texture {
        self.texturas.push(t);
        t
    }

    pub(crate) fn fbo(&mut self) -> Result<glow::Framebuffer, String> {
        let f = unsafe { self.gl.create_framebuffer()? };
        self.fbos.push(f);
        Ok(f)
    }

    /// Los recursos pasan a su dueño: al soltar esta copia no se borra nada.
    pub(crate) fn desarmar(mut self) {
        self.programas.clear();
        self.vaos.clear();
        self.buffers.clear();
        self.texturas.clear();
        self.fbos.clear();
    }
}

impl Drop for Recursos<'_> {
    fn drop(&mut self) {
        unsafe {
            for p in self.programas.drain(..) {
                self.gl.delete_program(p);
            }
            for v in self.vaos.drain(..) {
                self.gl.delete_vertex_array(v);
            }
            for b in self.buffers.drain(..) {
                self.gl.delete_buffer(b);
            }
            for t in self.texturas.drain(..) {
                self.gl.delete_texture(t);
            }
            for f in self.fbos.drain(..) {
                self.gl.delete_framebuffer(f);
            }
        }
    }
}

/// Compila y enlaza un programa. Si falla, borra los shaders y el programa creados hasta ese punto.
pub(crate) fn programa(gl: &glow::Context, vs_src: &str, fs_src: &str) -> Result<glow::Program, String> {
    unsafe {
        let prog = gl.create_program()?;
        let mut shaders = Vec::new();
        let mut error = None;
        for (tipo, src, nombre) in [(glow::VERTEX_SHADER, vs_src, "vértice"), (glow::FRAGMENT_SHADER, fs_src, "fragmento")] {
            match gl.create_shader(tipo) {
                Ok(s) => {
                    gl.shader_source(s, src);
                    gl.compile_shader(s);
                    shaders.push(s);
                    gl.attach_shader(prog, s);
                    if !gl.get_shader_compile_status(s) {
                        error = Some(format!("Shader de {nombre} no compila: {}", gl.get_shader_info_log(s)));
                        break;
                    }
                }
                Err(e) => {
                    error = Some(e);
                    break;
                }
            }
        }
        if error.is_none() {
            gl.link_program(prog);
            if !gl.get_program_link_status(prog) {
                error = Some(format!("Programa no enlaza: {}", gl.get_program_info_log(prog)));
            }
        }
        for s in shaders {
            gl.detach_shader(prog, s);
            gl.delete_shader(s);
        }
        match error {
            Some(e) => {
                gl.delete_program(prog);
                Err(e)
            }
            None => Ok(prog),
        }
    }
}

/// Destino fuera de pantalla: la escena se dibuja en `dibujo` (con MSAA si `msaa > 0`) y se resuelve
/// en `fbo`, cuya textura `tex` se pinta sobre la vista. Sin MSAA, `dibujo` es `fbo`.
struct Destino {
    w: i32,
    h: i32,
    msaa: u32,
    tex: glow::Texture,
    fbo: glow::Framebuffer,
    dibujo: glow::Framebuffer,
    rbs: Vec<glow::Renderbuffer>,
}

impl Destino {
    /// Crea el destino; si algo falla, borra lo que ya se creó.
    fn crear(gl: &glow::Context, w: i32, h: i32, msaa: u32) -> Result<Destino, String> {
        let mut r = Recursos::nuevo(gl);
        let tex = r.textura()?;
        let fbo = r.fbo()?;
        r.desarmar();
        let mut d = Destino { w, h, msaa, tex, fbo, dibujo: fbo, rbs: Vec::new() };
        match d.montar(gl) {
            Ok(()) => Ok(d),
            Err(e) => {
                d.borrar(gl);
                Err(e)
            }
        }
    }

    fn montar(&mut self, gl: &glow::Context) -> Result<(), String> {
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(self.tex));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                self.w,
                self.h,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            for (par, val) in [
                (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
                (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
                (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
            ] {
                gl.tex_parameter_i32(glow::TEXTURE_2D, par, val as i32);
            }
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
            gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(self.tex), 0);
        }
        if self.msaa == 0 {
            let prof = self.renderbuffer(gl, glow::DEPTH_COMPONENT24, 0)?;
            unsafe { gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, Some(prof)) };
        } else {
            self.dibujo = unsafe { gl.create_framebuffer()? };
            unsafe { gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.dibujo)) };
            let color = self.renderbuffer(gl, glow::RGBA8, self.msaa as i32)?;
            let prof = self.renderbuffer(gl, glow::DEPTH_COMPONENT24, self.msaa as i32)?;
            unsafe {
                gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::RENDERBUFFER, Some(color));
                gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, Some(prof));
            }
        }
        unsafe {
            for fb in [self.dibujo, self.fbo] {
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fb));
                let estado = gl.check_framebuffer_status(glow::FRAMEBUFFER);
                if estado != glow::FRAMEBUFFER_COMPLETE {
                    return Err(format!("Framebuffer de escena incompleto (estado {estado:#x}, {} muestras)", self.msaa));
                }
            }
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.bind_texture(glow::TEXTURE_2D, None);
        }
        Ok(())
    }

    /// Renderbuffer de `formato` (multimuestra si `muestras > 0`) dentro del destino.
    fn renderbuffer(&mut self, gl: &glow::Context, formato: u32, muestras: i32) -> Result<glow::Renderbuffer, String> {
        unsafe {
            let rb = gl.create_renderbuffer()?;
            self.rbs.push(rb);
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rb));
            if muestras == 0 {
                gl.renderbuffer_storage(glow::RENDERBUFFER, formato, self.w, self.h);
            } else {
                gl.renderbuffer_storage_multisample(glow::RENDERBUFFER, muestras, formato, self.w, self.h);
            }
            Ok(rb)
        }
    }

    fn borrar(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_texture(self.tex);
            gl.delete_framebuffer(self.fbo);
            if self.dibujo != self.fbo {
                gl.delete_framebuffer(self.dibujo);
            }
            for rb in &self.rbs {
                gl.delete_renderbuffer(*rb);
            }
        }
    }
}

/// Destino de `w`×`h` y `msaa` en `slot`, rehecho solo si cambió algo. Devuelve (dibujo, fbo, textura).
fn destino_listo(
    gl: &glow::Context,
    slot: &mut Option<Destino>,
    w: i32,
    h: i32,
    msaa: u32,
) -> Result<(glow::Framebuffer, glow::Framebuffer, glow::Texture), String> {
    if slot.as_ref().is_none_or(|d| (d.w, d.h, d.msaa) != (w, h, msaa)) {
        if let Some(viejo) = slot.take() {
            viejo.borrar(gl);
        }
        *slot = Some(Destino::crear(gl, w, h, msaa)?);
    }
    let d = slot.as_ref().ok_or_else(|| "destino de escena no disponible".to_string())?;
    Ok((d.dibujo, d.fbo, d.tex))
}

/// Por qué no se pudo crear el mapa de sombras: sin memoria (se puede bajar la resolución) u otro error.
enum ErrorSombras {
    SinMemoria,
    Otro(String),
}

impl ErrorSombras {
    fn texto(self) -> String {
        match self {
            ErrorSombras::SinMemoria => "sin memoria de GPU para el mapa de sombras".to_string(),
            ErrorSombras::Otro(e) => e,
        }
    }
}

/// Mapa de profundidad de sombras: `capas` capas de `res`×`res` (una por luz con sombra). Se comprueba con `fbo`.
fn crear_sombras(gl: &glow::Context, fbo: glow::Framebuffer, res: i32, capas: i32) -> Result<glow::Texture, ErrorSombras> {
    unsafe {
        // Se vacían los errores anteriores para que el que se mire sea el de esta reserva.
        for _ in 0..16 {
            if gl.get_error() == glow::NO_ERROR {
                break;
            }
        }
        let tex = gl.create_texture().map_err(ErrorSombras::Otro)?;
        gl.bind_texture(glow::TEXTURE_2D_ARRAY, Some(tex));
        gl.tex_image_3d(
            glow::TEXTURE_2D_ARRAY,
            0,
            glow::DEPTH_COMPONENT24 as i32,
            res,
            res,
            capas,
            0,
            glow::DEPTH_COMPONENT,
            glow::UNSIGNED_INT,
            glow::PixelUnpackData::Slice(None),
        );
        if gl.get_error() == glow::OUT_OF_MEMORY {
            gl.bind_texture(glow::TEXTURE_2D_ARRAY, None);
            gl.delete_texture(tex);
            return Err(ErrorSombras::SinMemoria);
        }
        for (par, val) in [
            (glow::TEXTURE_MIN_FILTER, glow::NEAREST),
            (glow::TEXTURE_MAG_FILTER, glow::NEAREST),
            (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
            (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
        ] {
            gl.tex_parameter_i32(glow::TEXTURE_2D_ARRAY, par, val as i32);
        }
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.framebuffer_texture_layer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, Some(tex), 0, 0);
        // Solo profundidad: sin color de lectura ni de escritura.
        gl.read_buffer(glow::NONE);
        gl.draw_buffers(&[glow::NONE]);
        let estado = gl.check_framebuffer_status(glow::FRAMEBUFFER);
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        gl.bind_texture(glow::TEXTURE_2D_ARRAY, None);
        if estado != glow::FRAMEBUFFER_COMPLETE {
            gl.delete_texture(tex);
            return Err(ErrorSombras::Otro(format!("Framebuffer de sombras incompleto (estado {estado:#x})")));
        }
        Ok(tex)
    }
}

/// Fabricante, renderer y versión de OpenGL de la GPU actual.
pub fn info_gpu(gl: &glow::Context) -> (String, String, String) {
    unsafe {
        (
            gl.get_parameter_string(glow::VENDOR),
            gl.get_parameter_string(glow::RENDERER),
            gl.get_parameter_string(glow::VERSION),
        )
    }
}

pub struct Renderer {
    prog_escena: glow::Program,
    prog_sombra: glow::Program,
    prog_humo: glow::Program,
    prog_cuadro: glow::Program,
    vao_escena: glow::VertexArray,
    vbo_escena: glow::Buffer,
    n_escena: i32,
    vao_sombra: glow::VertexArray,
    vbo_sombra: glow::Buffer,
    n_sombra: i32,
    vao_cono: glow::VertexArray,
    vbo_cono: glow::Buffer,
    n_cono: i32,
    /// Sin atributos: el cuadro se genera con `gl_VertexID`.
    vao_cuadro: glow::VertexArray,
    tex_sombra: glow::Texture,
    /// Tablas IES de las luces (R32F, IES_T × IES_P·MAX_LUCES); se sube con la escena.
    tex_ies: glow::Texture,
    fbo_sombra: glow::Framebuffer,
    /// Resolución de sombras pedida por la configuración y la real (menor si la memoria no alcanzó).
    res_pedida: i32,
    res_sombra: i32,
    /// Capas del mapa de sombras (una por luz con sombra); crecen según hacen falta.
    capas_sombra: i32,
    /// Matrices de luz con las que se dibujó el mapa de sombras vigente; vacío = hay que redibujarlo.
    sombras_dibujadas: Vec<Mat4>,
    /// Sin mapa de sombras por falta de memoria: se dibuja sin sombras hasta cambiar la configuración.
    sin_sombras: bool,
    /// Fracción de la vista que se renderiza (0.5..=1.0) y muestras MSAA del destino.
    escala: f32,
    msaa: u32,
    /// Sombras suaves (PCSS) en lugar del PCF 3×3: preset Alta y Fotorrealista (con mapa de sombras en el haz).
    pcss: bool,
    destino: Option<Destino>,
    preset: Preset,
    muestras_max: u32,
    /// Rebotes del trazado; bajan solos si una tesela sigue tardando demasiado con la escala mínima.
    rebotes: u32,
    /// Escala del trazado: la de la configuración (en GPU integrada, arranca en 0.5); baja sola si hace falta.
    escala_traza: f32,
    /// GPU integrada (según su renderer y vendor).
    integrada: bool,
    /// Trazador (BVH, acumulación y sondas); existe con los presets Alta y Fotorrealista.
    trazador: Option<Trazador>,
    /// Triángulos de la escena para el trazador; se empaquetan en la próxima muestra si cambió el montaje.
    tris_traza: Vec<Tri>,
    /// Oclusores de sombra (como el mapa de sombras: sin cuerpos de luz).
    tris_sombra: Vec<Tri>,
    traza_sucia: bool,
    /// La escena no se puede trazar (no cabe o no tiene triángulos): se dibuja con el raster hasta que cambie.
    escena_invalida: bool,
    /// Medidas de la sala (ancho, alto, largo) para la carcasa.
    sala: [f32; 3],
    destino_traza: Option<Destino>,
    /// Vista, proyección, ojo, exposición y viewport de la última muestra; si cambian, la acumulación se reinicia.
    clave_traza: Option<ClaveTraza>,
    /// El último fotograma se pintó con trazado.
    traza_activa: bool,
    /// Un ajuste automático de escala o rebotes espera a que la app lo guarde.
    ajuste: bool,
    /// Avisos para la app (suavizado desactivado, memoria de sombras, etc.).
    avisos: Vec<String>,
    /// Fallo del trazado; la app lo toma una vez y reacciona según su tipo.
    fallo: Option<Fallo>,
    /// Irradiancia de las sondas leída a CPU al converger (la lectura de lux la usa).
    sondas_cpu: Option<Arc<SondasCpu>>,
    /// Cambia cada vez que `sondas_cpu` cambia: la app invalida lo que dependía de la versión anterior.
    sondas_gen: u64,
    /// Las sondas siguen acumulando: hace falta otro fotograma.
    sondas_activas: bool,
}

/// Lo que, si cambia, reinicia la acumulación: vista, proyección, ojo, exposición (bits), viewport, lente y foco (bits).
/// El balance no entra: se aplica al mostrar.
type ClaveTraza = (Mat4, Mat4, [f32; 3], u32, [i32; 4], u32, u32);
/// Si una tesela tarda más de esto (ms) con la escala y los rebotes ya bajados, el trazado no cabe en esta GPU.
const LIMITE_TESELA_MS: f32 = 60.0;
const ESCALA_MIN_TRAZO: f32 = 0.5;

impl Renderer {
    /// Crea los programas, buffers, mapa de sombras y destino (y el trazador si es Fotorrealista). Si algo falla,
    /// se borra todo lo creado.
    pub fn new(gl: &glow::Context, c: &Configuracion) -> Result<Renderer, String> {
        let (vendor, renderer, _) = info_gpu(gl);
        let integrada = es_integrada(&renderer, &vendor);
        let res = c.res_sombras as i32;
        let mut r = Recursos::nuevo(gl);
        unsafe {
            let prog_escena = r.programa(VS_ESCENA, FS_ESCENA)?;
            let prog_sombra = r.programa(VS_SOMBRA, FS_SOMBRA)?;
            let prog_humo = r.programa(VS_HUMO, FS_HUMO)?;
            let prog_cuadro = r.programa(VS_CUADRO, FS_CUADRO)?;

            // Escena: posición, normal, albedo y marca de carcasa (10 floats por vértice).
            let vao_escena = r.vao()?;
            let vbo_escena = r.buffer()?;
            gl.bind_vertex_array(Some(vao_escena));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo_escena));
            for (i, offset) in [0, 12, 24].into_iter().enumerate() {
                gl.enable_vertex_attrib_array(i as u32);
                gl.vertex_attrib_pointer_f32(i as u32, 3, glow::FLOAT, false, 40, offset);
            }
            gl.enable_vertex_attrib_array(3);
            gl.vertex_attrib_pointer_f32(3, 1, glow::FLOAT, false, 40, 36);

            // Sombras: solo posiciones.
            let vao_sombra = r.vao()?;
            let vbo_sombra = r.buffer()?;
            gl.bind_vertex_array(Some(vao_sombra));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo_sombra));
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 12, 0);

            // Cono unitario del humo: ápice en z = 0, base en z = 1.
            let mut cono = Vec::new();
            for i in 0..SEGMENTOS_CONO {
                let (a0, a1) = (i as f32 / SEGMENTOS_CONO as f32, (i + 1) as f32 / SEGMENTOS_CONO as f32);
                let ang = |a: f32| a * std::f32::consts::TAU;
                cono.extend_from_slice(&[0.0, 0.0, 0.0]);
                cono.extend_from_slice(&[ang(a0).cos(), ang(a0).sin(), 1.0]);
                cono.extend_from_slice(&[ang(a1).cos(), ang(a1).sin(), 1.0]);
            }
            let vao_cono = r.vao()?;
            let vbo_cono = r.buffer()?;
            gl.bind_vertex_array(Some(vao_cono));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo_cono));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &bytes(&cono), glow::STATIC_DRAW);
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 12, 0);

            let vao_cuadro = r.vao()?;
            let tex_ies = r.textura()?;
            let fbo_sombra = r.fbo()?;
            // Una capa para empezar: el mapa crece cuando la escena tiene más luces con sombra.
            let tex_sombra = r.registrar_textura(crear_sombras(gl, fbo_sombra, res, 1).map_err(ErrorSombras::texto)?);
            // Prueba del destino con el MSAA configurado: si el driver no lo admite, falla aquí.
            let destino = Destino::crear(gl, 4, 4, c.msaa)?;
            // Alta y Fotorrealista compilan el trazado (sondas) ya: si falla, Fotorrealista baja a Alta y Alta sigue sin sondas.
            let mut avisos = Vec::new();
            let trazador = if c.preset == Preset::Rapida {
                None
            } else {
                match Trazador::nuevo(gl, integrada) {
                    Ok(t) => Some(t),
                    Err(e) if c.preset == Preset::Fotorrealista => {
                        destino.borrar(gl);
                        return Err(e);
                    }
                    Err(e) => {
                        avisos.push(format!("Sin sondas de luz en esta GPU: se usa iluminación indirecta básica. {e}"));
                        None
                    }
                }
            };

            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            r.desarmar();

            Ok(Renderer {
                prog_escena,
                prog_sombra,
                prog_humo,
                prog_cuadro,
                vao_escena,
                vbo_escena,
                n_escena: 0,
                vao_sombra,
                vbo_sombra,
                n_sombra: 0,
                vao_cono,
                vbo_cono,
                n_cono: (SEGMENTOS_CONO * 3) as i32,
                vao_cuadro,
                tex_sombra,
                tex_ies,
                fbo_sombra,
                res_pedida: res,
                res_sombra: res,
                capas_sombra: 1,
                sombras_dibujadas: Vec::new(),
                sin_sombras: false,
                escala: c.escala,
                msaa: c.msaa,
                pcss: c.preset != Preset::Rapida,
                destino: Some(destino),
                preset: c.preset,
                muestras_max: c.muestras_max,
                rebotes: c.rebotes,
                escala_traza: if integrada { ESCALA_MIN_TRAZO } else { c.escala },
                integrada,
                trazador,
                tris_traza: Vec::new(),
                tris_sombra: Vec::new(),
                traza_sucia: true,
                escena_invalida: false,
                sala: [0.0; 3],
                destino_traza: None,
                clave_traza: None,
                traza_activa: false,
                ajuste: false,
                avisos,
                fallo: None,
                sondas_cpu: None,
                sondas_gen: 0,
                sondas_activas: false,
            })
        }
    }

    /// Aplica escala, MSAA, sombras y preset; el mapa de sombras se rehace solo si cambia su tamaño.
    /// Cualquier cambio reinicia la acumulación del trazado. Si el suavizado no funciona, `c.msaa` pasa a 0.
    pub fn configurar(&mut self, gl: &glow::Context, c: &mut Configuracion) -> Result<(), String> {
        let res = c.res_sombras as i32;
        if res != self.res_pedida {
            self.cambiar_sombras(gl, res, self.capas_sombra)?;
            self.res_pedida = res;
        }
        self.sin_sombras = false;
        if c.msaa > 0 {
            match Destino::crear(gl, 4, 4, c.msaa) {
                Ok(prueba) => prueba.borrar(gl),
                Err(e) => {
                    self.avisos.push(format!("El suavizado {}× no funciona en esta GPU: se desactiva. {e}", c.msaa));
                    c.msaa = 0;
                }
            }
        }
        self.escala = c.escala;
        self.msaa = c.msaa;
        self.pcss = c.preset != Preset::Rapida;
        self.preset = c.preset;
        self.muestras_max = c.muestras_max;
        self.rebotes = c.rebotes;
        self.escala_traza = if self.integrada { ESCALA_MIN_TRAZO } else { c.escala };
        self.clave_traza = None;
        self.soltar_sondas();
        if c.preset == Preset::Rapida {
            if let Some(t) = self.trazador.take() {
                t.destruir(gl);
            }
        } else if self.trazador.is_none() {
            match Trazador::nuevo(gl, self.integrada) {
                Ok(t) => {
                    self.trazador = Some(t);
                    self.traza_sucia = true;
                }
                // Fotorrealista lo comunica como fallo (la app baja a Alta); Alta sigue sin sondas.
                Err(e) if c.preset == Preset::Fotorrealista => self.fallo = Some(Fallo::Gl(e)),
                Err(e) => self.avisos.push(format!("Sin sondas de luz en esta GPU: se usa iluminación indirecta básica. {e}")),
            }
        }
        if let Some(t) = self.trazador.as_mut() {
            t.reiniciar();
            t.reiniciar_sondas();
        }
        Ok(())
    }

    /// Descarta la irradiancia leída a CPU (cambió la escena o la configuración).
    fn soltar_sondas(&mut self) {
        self.sondas_cpu = None;
        self.sondas_gen += 1;
    }

    /// Mapa de sombras de `res` px y `capas` capas. Si la memoria no alcanza, baja la resolución hasta 1024 px.
    fn cambiar_sombras(&mut self, gl: &glow::Context, res: i32, capas: i32) -> Result<(), String> {
        let mut r = res;
        loop {
            match crear_sombras(gl, self.fbo_sombra, r, capas) {
                Ok(tex) => {
                    unsafe { gl.delete_texture(self.tex_sombra) };
                    self.tex_sombra = tex;
                    self.res_sombra = r;
                    self.capas_sombra = capas;
                    self.sombras_dibujadas.clear();
                    return Ok(());
                }
                Err(ErrorSombras::SinMemoria) if r > 1024 => {
                    r /= 2;
                    self.avisos.push(format!("Sin memoria para sombras de {res} px: se usan {r} px."));
                }
                Err(e) => return Err(e.texto()),
            }
        }
    }

    /// Hace sitio para `n` luces con sombra (una capa por luz, como mucho `MAX_SOMBRAS`). False si no hay memoria.
    fn asegurar_sombras(&mut self, gl: &glow::Context, n: usize) -> bool {
        if self.sin_sombras {
            return false;
        }
        let capas = (n as i32).clamp(1, MAX_SOMBRAS as i32);
        if capas <= self.capas_sombra {
            return true;
        }
        match self.cambiar_sombras(gl, self.res_pedida, capas) {
            Ok(()) => true,
            Err(e) => {
                self.avisos.push(format!("No hay memoria para las sombras: se dibuja sin ellas. {e}"));
                self.sin_sombras = true;
                false
            }
        }
    }

    /// Sube la malla de la escena (solo cuando cambia el montaje). El trazado la toma en su próxima muestra.
    pub fn subir_escena(&mut self, gl: &glow::Context, malla: &[VerticeGpu], sombras: &[[f32; 3]], sala: [f32; 3], fs: &[FuenteLuz]) {
        let verts: Vec<f32> =
            malla.iter().flat_map(|v| v.pos.into_iter().chain(v.normal).chain(v.albedo).chain([v.carcasa])).collect();
        let pos: Vec<f32> = sombras.iter().flatten().copied().collect();
        self.n_escena = malla.len() as i32;
        self.n_sombra = sombras.len() as i32;
        self.tris_traza = trazado::triangulos_de(malla);
        self.tris_sombra = trazado::triangulos_sombra(sombras);
        self.traza_sucia = true;
        self.soltar_sondas();
        self.sombras_dibujadas.clear();
        self.escena_invalida = false;
        self.sala = sala;
        subir_ies(gl, self.tex_ies, fs);
        unsafe {
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo_escena));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &bytes(&verts), glow::STATIC_DRAW);
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo_sombra));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &bytes(&pos), glow::STATIC_DRAW);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
        }
    }

    /// Escala efectiva del trazado si quedó por debajo de la configurada (ajuste automático).
    pub fn escala_reducida(&self) -> Option<f32> {
        (self.preset == Preset::Fotorrealista && self.escala_traza < self.escala - 1e-3).then_some(self.escala_traza)
    }

    /// Muestras acumuladas y máximo, si el último fotograma se pintó con trazado.
    pub fn progreso(&self) -> Option<(u32, u32)> {
        if !self.traza_activa {
            return None;
        }
        self.trazador.as_ref().map(|t| (t.muestras, self.muestras_max))
    }

    /// ¿Hace falta otro fotograma? Sí mientras haya muestras pendientes, sondas por acumular o un fallo por comunicar.
    pub fn pide_repintado(&self) -> bool {
        self.fallo.is_some() || self.sondas_activas || self.progreso().is_some_and(|(n, max)| n < max)
    }

    /// Irradiancia de las sondas a CPU, cuando ya han convergido (None hasta entonces o si cambió la escena).
    pub fn sondas_cpu(&self) -> Option<Arc<SondasCpu>> {
        self.sondas_cpu.clone()
    }

    /// Versión de `sondas_cpu`: cambia cada vez que se lee o se descarta.
    pub fn sondas_gen(&self) -> u64 {
        self.sondas_gen
    }

    /// Fallo del trazado en el último fotograma; se devuelve una sola vez.
    pub fn tomar_fallo(&mut self) -> Option<Fallo> {
        self.fallo.take()
    }

    /// Escala y rebotes del trazado si el ajuste automático los cambió desde la última vez (la app los guarda).
    pub fn tomar_ajuste(&mut self) -> Option<(f32, u32)> {
        std::mem::take(&mut self.ajuste).then_some((self.escala_traza, self.rebotes))
    }

    /// Avisos pendientes para la app; se devuelven una sola vez.
    pub fn tomar_avisos(&mut self) -> Vec<String> {
        std::mem::take(&mut self.avisos)
    }

    /// Lee el tiempo del pase anterior (sin bloquear) y ajusta el trazado: teselas por fotograma para llenar el
    /// presupuesto; si una tesela tarda más de `LIMITE_TESELA_MS`, baja la escala hasta 0.5, luego los rebotes hasta 1,
    /// y si aun así tarda, se comunica como `Fallo::Lento`.
    fn comprobar_tiempo(&mut self, gl: &glow::Context) {
        let Some(tz) = self.trazador.as_mut() else { return };
        let Some((ms, n)) = tz.leer_tiempo(gl) else { return };
        let por_tesela = (ms / n.max(1) as f32).max(1e-3);
        #[cfg(debug_assertions)]
        eprintln!("trazo: {ms:.2} ms en {n} teselas ({por_tesela:.2} ms por tesela)");
        let cabe = (PRESUPUESTO_MS / por_tesela).floor() as usize;
        tz.por_fotograma = cabe.clamp(1, tz.teselas_totales());
        if por_tesela <= LIMITE_TESELA_MS {
            return;
        }
        let cambio = if self.escala_traza > ESCALA_MIN_TRAZO + 1e-3 {
            self.escala_traza = (self.escala_traza - 0.1).max(ESCALA_MIN_TRAZO);
            true
        } else if self.rebotes > 1 {
            self.rebotes -= 1;
            true
        } else {
            self.fallo = Some(Fallo::Lento(format!("una tesela tarda {por_tesela:.0} ms")));
            false
        };
        if cambio {
            self.ajuste = true;
            tz.reiniciar();
            tz.reiniciar_sondas();
            tz.olvidar_medida(gl);
            self.soltar_sondas();
        }
    }

    /// Un fotograma del trazado (un presupuesto de teselas) y la imagen promediada, tonemapeada en su destino.
    /// Sube la escena al trazador si cambió (esto reinicia sus acumulaciones de píxeles y de sondas).
    fn subir_traza(&mut self, gl: &glow::Context) -> Result<(), Fallo> {
        if !self.traza_sucia {
            return Ok(());
        }
        let tz = self.trazador.as_mut().ok_or_else(|| Fallo::Gl("el trazado no está preparado".to_string()))?;
        if let Err(e) = tz.subir_escena(gl, &self.tris_traza, &self.tris_sombra, self.sala) {
            if matches!(e, Fallo::Escena(_)) {
                self.escena_invalida = true;
            }
            return Err(e);
        }
        self.traza_sucia = false;
        Ok(())
    }

    /// Fallo de las sondas: en Fotorrealista es un fallo del trazado; en Alta son opcionales, así que se apagan con un aviso.
    fn fallo_sondas(&mut self, gl: &glow::Context, e: Fallo) {
        if self.preset == Preset::Fotorrealista {
            self.fallo = Some(e);
            return;
        }
        if let Some(t) = self.trazador.take() {
            t.destruir(gl);
        }
        self.soltar_sondas();
        let motivo = match e {
            Fallo::Gl(s) | Fallo::Escena(s) | Fallo::Lento(s) => s,
        };
        self.avisos.push(format!("Sin sondas de luz en esta GPU: se usa iluminación indirecta básica. {motivo}"));
    }

    /// Sondas de irradiancia: avanzan mientras no convergen y se ven en Realista o Mapa de lux. Ya convergidas, se leen
    /// a CPU una vez. Devuelve la textura y la rejilla si hay muestras que usar; None si no, o si el trazado falla.
    fn pasar_sondas(&mut self, gl: &glow::Context, p: &ParamsFrame, paq: &Paquete) -> Option<(glow::Texture, Rejilla)> {
        self.sondas_activas = false;
        let util = matches!(self.preset, Preset::Alta | Preset::Fotorrealista)
            && !self.escena_invalida
            && matches!(p.modo, ModoVista::Realista | ModoVista::MapaLux);
        if !util || self.trazador.is_none() {
            return None;
        }
        // Una escena sin triángulos ya está marcada como inválida (la pasada raster la muestra); no es un fallo nuevo.
        if let Err(e) = self.subir_traza(gl) {
            if !matches!(e, Fallo::Escena(_)) {
                self.fallo_sondas(gl, e);
            }
            return None;
        }
        let tz = self.trazador.as_mut()?;
        if tz.sondas_muestras() < MUESTRAS_SONDAS {
            let presupuesto = tz.sondas_por_fotograma();
            if let Err(e) = tz.avanzar_sondas(gl, self.vao_cuadro, paq, self.tex_ies, self.rebotes, self.sala, presupuesto) {
                if !matches!(e, Fallo::Escena(_)) {
                    self.fallo_sondas(gl, e);
                }
                return None;
            }
            self.sondas_activas = tz.sondas_muestras() < MUESTRAS_SONDAS;
        }
        if tz.sondas_muestras() == MUESTRAS_SONDAS && self.sondas_cpu.is_none() {
            self.sondas_cpu = tz.sondas_leer(gl).map(Arc::new);
            self.sondas_gen += 1;
        }
        if tz.sondas_muestras() == 0 {
            return None;
        }
        Some((tz.sondas_textura()?, tz.sondas_rejilla()))
    }

    fn pasar_trazo(&mut self, gl: &glow::Context, p: &ParamsFrame, paq: &Paquete, w: i32, h: i32) -> Result<glow::Texture, Fallo> {
        self.subir_traza(gl)?;
        let tz = self.trazador.as_mut().ok_or_else(|| Fallo::Gl("el trazado no está preparado".to_string()))?;
        let clave = (p.vista, p.proy, p.ojo, p.exposicion.to_bits(), p.viewport_px, p.lente.to_bits(), p.foco.to_bits());
        if self.clave_traza != Some(clave) {
            tz.reiniciar();
            self.clave_traza = Some(clave);
        }
        tz.asegurar_tamano(gl, w, h).map_err(Fallo::Gl)?;
        if tz.muestras < self.muestras_max {
            let presupuesto = tz.por_fotograma;
            tz.avanzar(gl, self.vao_cuadro, p, paq, self.tex_ies, self.rebotes, self.sala, presupuesto, self.muestras_max)?;
        }
        let (_, fbo, tex) = destino_listo(gl, &mut self.destino_traza, w, h, 0).map_err(Fallo::Gl)?;
        tz.mostrar(gl, self.vao_cuadro, fbo, p.exposicion, p.balance);
        self.traza_activa = true;
        Ok(tex)
    }

    /// Escena con la pasada raster (sombras, luz, humo) en el destino con MSAA; devuelve la textura resuelta.
    /// Si el destino con MSAA no se puede crear, se desactiva el suavizado y se avisa.
    fn escena_a_textura(
        &mut self,
        gl: &glow::Context,
        p: &ParamsFrame,
        paq: &Paquete,
        w: i32,
        h: i32,
        sondas: Option<(glow::Texture, Rejilla)>,
    ) -> Option<glow::Texture> {
        let (dibujo, fbo, tex) = match destino_listo(gl, &mut self.destino, w, h, self.msaa) {
            Ok(d) => d,
            Err(e) if self.msaa > 0 => {
                self.avisos.push(format!("El suavizado {}× no funciona en esta GPU: se desactiva. {e}", self.msaa));
                self.msaa = 0;
                match destino_listo(gl, &mut self.destino, w, h, 0) {
                    Ok(d) => d,
                    Err(e) => {
                        self.avisos.push(format!("No se pudo preparar la vista 3D: {e}"));
                        return None;
                    }
                }
            }
            Err(e) => {
                self.avisos.push(format!("No se pudo preparar la vista 3D: {e}"));
                return None;
            }
        };
        self.pasada_sombras(gl, p, paq);
        self.escena(gl, p, paq, dibujo, w, h, sondas);
        if dibujo != fbo {
            unsafe {
                gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(dibujo));
                gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(fbo));
                gl.blit_framebuffer(0, 0, w, h, 0, 0, w, h, glow::COLOR_BUFFER_BIT, glow::NEAREST);
            }
        }
        Some(tex)
    }

    /// Mapa de sombras: una capa por luz. Se omite si las luces y la geometría no cambiaron desde el último dibujo.
    fn pasada_sombras(&mut self, gl: &glow::Context, p: &ParamsFrame, paq: &Paquete) {
        if paq.n_sombras == 0 || self.n_sombra == 0 {
            return;
        }
        let luces: Vec<Mat4> = p.fuentes[..paq.n_sombras].iter().map(matriz_luz).collect();
        if luces == self.sombras_dibujadas {
            return;
        }
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo_sombra));
            gl.viewport(0, 0, self.res_sombra, self.res_sombra);
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LESS);
            gl.depth_mask(true);
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::BLEND);
            gl.use_program(Some(self.prog_sombra));
            gl.bind_vertex_array(Some(self.vao_sombra));
            let loc = gl.get_uniform_location(self.prog_sombra, "uLuz");
            for (i, m) in luces.iter().enumerate() {
                gl.framebuffer_texture_layer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, Some(self.tex_sombra), 0, i as i32);
                gl.clear(glow::DEPTH_BUFFER_BIT);
                gl.uniform_matrix_4_f32_slice(loc.as_ref(), false, plano(m));
                gl.draw_arrays(glow::TRIANGLES, 0, self.n_sombra);
            }
        }
        self.sombras_dibujadas = luces;
    }

    /// Escena completa (sombras, luz, humo) en `dibujo`, de tamaño `w`×`h`.
    fn escena(
        &self,
        gl: &glow::Context,
        p: &ParamsFrame,
        paq: &Paquete,
        dibujo: glow::Framebuffer,
        w: i32,
        h: i32,
        sondas: Option<(glow::Texture, Rejilla)>,
    ) {
        unsafe {
            // Pasada principal, en el destino fuera de pantalla (fondo transparente).
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(dibujo));
            gl.viewport(0, 0, w, h);
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LESS);
            gl.depth_mask(true);
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::BLEND);
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);

            gl.use_program(Some(self.prog_escena));
            gl.bind_vertex_array(Some(self.vao_escena));
            let u = |nombre: &str| gl.get_uniform_location(self.prog_escena, nombre);
            gl.uniform_matrix_4_f32_slice(u("uVista").as_ref(), false, plano(&p.vista));
            gl.uniform_matrix_4_f32_slice(u("uProy").as_ref(), false, plano(&p.proy));
            gl.uniform_3_f32(u("uOjo").as_ref(), p.ojo[0], p.ojo[1], p.ojo[2]);
            gl.uniform_4_f32_slice(u("uPosCd[0]").as_ref(), &paq.pos_cd);
            gl.uniform_4_f32_slice(u("uDirAp[0]").as_ref(), &paq.dir_ap);
            gl.uniform_4_f32_slice(u("uEjeLuz[0]").as_ref(), &paq.eje);
            gl.uniform_4_f32_slice(u("uEmisor[0]").as_ref(), &paq.emisor);
            gl.uniform_4_f32_slice(u("uColor[0]").as_ref(), &paq.color);
            gl.uniform_1_f32_slice(u("uRadio[0]").as_ref(), &paq.radio);
            gl.uniform_1_i32(u("uPcss").as_ref(), self.pcss as i32);
            gl.uniform_1_i32(u("uNLuces").as_ref(), paq.n_luces as i32);
            gl.uniform_1_i32(u("uNSombras").as_ref(), paq.n_sombras as i32);
            let mut luces = vec![0.0f32; 16 * MAX_SOMBRAS];
            for (i, f) in p.fuentes[..paq.n_sombras].iter().enumerate() {
                luces[16 * i..16 * i + 16].copy_from_slice(plano(&matriz_luz(f)));
            }
            gl.uniform_matrix_4_f32_slice(u("uLuz[0]").as_ref(), false, &luces);
            gl.uniform_3_f32(
                u("uIndirecta").as_ref(),
                p.indirecta_lux * p.color_indirecta[0],
                p.indirecta_lux * p.color_indirecta[1],
                p.indirecta_lux * p.color_indirecta[2],
            );
            gl.uniform_1_f32(u("uExposicion").as_ref(), p.exposicion);
            gl.uniform_3_f32(u("uBalance").as_ref(), p.balance[0], p.balance[1], p.balance[2]);
            gl.uniform_1_i32(u("uModo").as_ref(), if p.modo == ModoVista::MapaLux { 1 } else { 0 });
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D_ARRAY, Some(self.tex_sombra));
            gl.uniform_1_i32(u("uSombras").as_ref(), 0);
            // Unidad 5: tablas IES (las unidades 0 a 4 son del trazado).
            gl.active_texture(glow::TEXTURE5);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.tex_ies));
            gl.uniform_1_i32(u("uTablaIes").as_ref(), 5);
            gl.uniform_1_f32_slice(u("uIes[0]").as_ref(), &paq.ies);
            // Unidad 6: sondas de irradiancia (libre: el trazado usa 0 a 5). Sin sondas, uSondaOk = 0 y se usa uIndirecta.
            gl.active_texture(glow::TEXTURE6);
            gl.uniform_1_i32(u("uSondas").as_ref(), 6);
            match sondas {
                Some((tex, rej)) => {
                    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                    gl.uniform_1_i32(u("uSondaOk").as_ref(), 1);
                    gl.uniform_3_i32(u("uSondaN").as_ref(), rej.n[0] as i32, rej.n[1] as i32, rej.n[2] as i32);
                    gl.uniform_3_f32(u("uSondaPaso").as_ref(), rej.paso[0], rej.paso[1], rej.paso[2]);
                }
                None => gl.uniform_1_i32(u("uSondaOk").as_ref(), 0),
            }
            gl.draw_arrays(glow::TRIANGLES, 0, self.n_escena);

            // Humo: conos aditivos, con profundidad pero sin escribirla. El alfa no cambia: el fondo sigue transparente.
            if p.humo && paq.n_luces > 0 {
                gl.enable(glow::BLEND);
                gl.blend_func_separate(glow::ONE, glow::ONE, glow::ZERO, glow::ONE);
                gl.depth_mask(false);
                gl.use_program(Some(self.prog_humo));
                gl.bind_vertex_array(Some(self.vao_cono));
                let h = |nombre: &str| gl.get_uniform_location(self.prog_humo, nombre);
                gl.uniform_matrix_4_f32_slice(h("uVista").as_ref(), false, plano(&p.vista));
                gl.uniform_matrix_4_f32_slice(h("uProy").as_ref(), false, plano(&p.proy));
                for f in &p.fuentes[..paq.n_luces] {
                    let k = GANANCIA_HAZ * f.cd;
                    gl.uniform_matrix_4_f32_slice(h("uModelo").as_ref(), false, plano(&modelo_cono(f)));
                    gl.uniform_3_f32(h("uColor").as_ref(), f.color[0] * k, f.color[1] * k, f.color[2] * k);
                    gl.uniform_1_f32(h("uExposicion").as_ref(), p.exposicion);
                    gl.uniform_3_f32(h("uBalance").as_ref(), p.balance[0], p.balance[1], p.balance[2]);
                    gl.draw_arrays(glow::TRIANGLES, 0, self.n_cono);
                }
                gl.depth_mask(true);
                gl.disable(glow::BLEND);
            }
        }
    }

    /// Pinta la textura resuelta sobre el viewport actual. Es premultiplicada: el alfa 0 deja ver el fondo de egui.
    fn cuadro(&self, gl: &glow::Context, tex: glow::Texture) {
        unsafe {
            gl.disable(glow::DEPTH_TEST);
            gl.enable(glow::BLEND);
            gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
            gl.use_program(Some(self.prog_cuadro));
            gl.bind_vertex_array(Some(self.vao_cuadro));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.uniform_1_i32(gl.get_uniform_location(self.prog_cuadro, "uTex").as_ref(), 0);
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
        }
    }

    /// Escena fuera de pantalla (con MSAA resuelto) y cuadro sobre la vista. Restaura el estado de GL.
    pub fn pintar(&mut self, gl: &glow::Context, p: &ParamsFrame) {
        let mut paq = empaquetar(&p.fuentes);
        if paq.n_sombras > 0 && !self.asegurar_sombras(gl, paq.n_sombras) {
            paq.n_sombras = 0;
        }
        let [_, _, vw, vh] = p.viewport_px;
        // El haz de humo y el mapa de lux se siguen dibujando con la pasada raster.
        let trazo = self.preset == Preset::Fotorrealista && p.modo == ModoVista::Realista && !p.humo && !self.escena_invalida;
        if trazo {
            self.comprobar_tiempo(gl);
        }
        let escala = if trazo { self.escala_traza } else { self.escala };
        let (w, h) = (((vw as f32 * escala).round() as i32).max(1), ((vh as f32 * escala).round() as i32).max(1));
        let (vp_prev, sc_prev, sc_on, fb_prev) = unsafe {
            let mut vp_prev = [0i32; 4];
            gl.get_parameter_i32_slice(glow::VIEWPORT, &mut vp_prev);
            let mut sc_prev = [0i32; 4];
            gl.get_parameter_i32_slice(glow::SCISSOR_BOX, &mut sc_prev);
            let sc_on = gl.is_enabled(glow::SCISSOR_TEST);
            let fb_prev = gl.get_parameter_framebuffer(glow::FRAMEBUFFER_BINDING);
            // El recorte de egui no debe afectar al destino fuera de pantalla ni a su resolución.
            gl.disable(glow::SCISSOR_TEST);
            (vp_prev, sc_prev, sc_on, fb_prev)
        };
        let sondas = self.pasar_sondas(gl, p, &paq);
        let tex = if trazo {
            match self.pasar_trazo(gl, p, &paq, w, h) {
                Ok(tex) => Some(tex),
                Err(e) => {
                    // Fallo: se dibuja la pasada raster este fotograma (con uIndirecta); la app decide qué hacer con el fallo.
                    self.fallo = Some(e);
                    self.traza_activa = false;
                    self.escena_a_textura(gl, p, &paq, w, h, None)
                }
            }
        } else {
            self.traza_activa = false;
            self.escena_a_textura(gl, p, &paq, w, h, sondas)
        };
        unsafe {
            // Vuelta a la ventana de egui, con su recorte y su viewport.
            gl.bind_framebuffer(glow::FRAMEBUFFER, fb_prev);
            // El recorte de egui antes de reactivarlo: las sondas dejan el de su tesela y el cuadro solo se vería ahí.
            gl.scissor(sc_prev[0], sc_prev[1], sc_prev[2], sc_prev[3]);
            if sc_on {
                gl.enable(glow::SCISSOR_TEST);
            }
            gl.viewport(p.viewport_px[0], p.viewport_px[1], p.viewport_px[2], p.viewport_px[3]);
        }
        if let Some(tex) = tex {
            self.cuadro(gl, tex);
        }
        unsafe {
            // Restaurar lo que egui no vuelve a fijar en cada primitiva.
            gl.bind_vertex_array(None);
            gl.use_program(None);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D_ARRAY, None);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::BLEND);
            gl.viewport(vp_prev[0], vp_prev[1], vp_prev[2], vp_prev[3]);
            gl.scissor(sc_prev[0], sc_prev[1], sc_prev[2], sc_prev[3]);
            #[cfg(debug_assertions)]
            {
                let e = gl.get_error();
                if e != glow::NO_ERROR {
                    eprintln!("render GPU: error de GL {e:#x} tras pintar");
                }
            }
        }
    }

    pub fn destruir(&self, gl: &glow::Context) {
        if let Some(d) = &self.destino {
            d.borrar(gl);
        }
        if let Some(d) = &self.destino_traza {
            d.borrar(gl);
        }
        if let Some(t) = &self.trazador {
            t.destruir(gl);
        }
        unsafe {
            gl.delete_program(self.prog_escena);
            gl.delete_program(self.prog_sombra);
            gl.delete_program(self.prog_humo);
            gl.delete_program(self.prog_cuadro);
            gl.delete_vertex_array(self.vao_escena);
            gl.delete_vertex_array(self.vao_sombra);
            gl.delete_vertex_array(self.vao_cono);
            gl.delete_vertex_array(self.vao_cuadro);
            gl.delete_buffer(self.vbo_escena);
            gl.delete_buffer(self.vbo_sombra);
            gl.delete_buffer(self.vbo_cono);
            gl.delete_texture(self.tex_sombra);
            gl.delete_texture(self.tex_ies);
            gl.delete_framebuffer(self.fbo_sombra);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::V3;

    fn rect() -> eframe::egui::Rect {
        eframe::egui::Rect::from_min_size(eframe::egui::Pos2::new(0.0, 0.0), eframe::egui::Vec2::new(800.0, 600.0))
    }

    fn camara() -> Camara {
        Camara { objetivo: V3::new(1.0, 2.0, 3.0), yaw: 0.8, pitch: 0.6, distancia: 9.0, fov_y: 50f32.to_radians() }
    }

    /// Transforma un punto con proy·vista y lo pasa a píxeles del rect (origen arriba-izquierda).
    fn a_pantalla(m: &Mat4, p: [f32; 3]) -> (f32, f32) {
        let v = [p[0], p[1], p[2], 1.0];
        let c: [f32; 4] = std::array::from_fn(|r| (0..4).map(|k| m[k][r] * v[k]).sum());
        let (nx, ny) = (c[0] / c[3], c[1] / c[3]);
        ((nx + 1.0) * 0.5 * 800.0, (1.0 - ny) * 0.5 * 600.0)
    }

    #[test]
    fn matriz_vista_coincide_con_camara() {
        let c = camara();
        let pm = mul(matriz_proyeccion(&c, 800.0 / 600.0), matriz_vista(&c));
        for p in [[0.0, 0.0, 0.0], [2.5, 1.2, 4.0], [-1.0, 3.0, 6.0], [1.0, 2.0, 3.0]] {
            let (sx, sy) = a_pantalla(&pm, p);
            let (s, _) = c.proyectar(V3::from_arr(p), rect()).expect("visible");
            assert!((sx - s.x).abs() < 0.5 && (sy - s.y).abs() < 0.5, "{p:?}: GPU ({sx},{sy}) CPU ({},{})", s.x, s.y);
        }
    }

    #[test]
    fn matriz_luz_centra_el_eje() {
        let f = FuenteLuz { pos: [1.0, 2.5, 0.5], dir: [0.0, -1.0, 0.0], cd: 1000.0, apertura: 30.0, color: [1.0; 3], radio: 0.0, eje_u: [1.0, 0.0, 0.0], medio: [0.0; 2], nitidez: 1.0, ies: None };
        let m = matriz_luz(&f);
        let p = [1.0, 2.5 - 2.0, 0.5];
        let c: [f32; 4] = std::array::from_fn(|r| (0..4).map(|k| m[k][r] * [p[0], p[1], p[2], 1.0][k]).sum());
        assert!((c[0] / c[3]).abs() < 1e-5 && (c[1] / c[3]).abs() < 1e-5, "{:?}", c);
    }

    /// Escala de proyección que usa el shader: longitud de la fila 0 de proy·vista (= f, porque s es unitario).
    fn escala_proyeccion(m: &Mat4) -> f32 {
        (m[0][0] * m[0][0] + m[1][0] * m[1][0] + m[2][0] * m[2][0]).sqrt()
    }

    #[test]
    fn escala_pcss_independiente_del_giro() {
        // Con giro 90/270 el eje X de la luz apunta en Z: m[0][0] = f·s.x ≈ 0. El shader debe usar la longitud de la fila.
        let fov = (1.6 * 90.0f32).clamp(1.0, 170.0).to_radians();
        let esperada = 1.0 / (fov * 0.5).tan();
        for giro in [0.0, 90.0, 180.0, 270.0, 281.0] {
            let f = FuenteLuz { pos: [0.0, 2.0, 0.0], dir: crate::modelo::direccion(giro, -41.0), cd: 1000.0, apertura: 90.0, color: [1.0; 3], radio: 0.0, eje_u: [1.0, 0.0, 0.0], medio: [0.0; 2], nitidez: 1.0, ies: None };
            let m = matriz_luz(&f);
            assert!((escala_proyeccion(&m) - esperada).abs() < 1e-4, "giro {giro}: {} vs {esperada}", escala_proyeccion(&m));
            if giro == 281.0 {
                // Documenta el bug antiguo: m[0][0] (lo que leía el shader) queda muy por debajo de f.
                assert!(m[0][0].abs() < escala_proyeccion(&m) / 2.0, "m[0][0] = {}", m[0][0]);
            }
        }
    }

    #[test]
    fn modelo_cono_llega_al_suelo() {
        // Luz a 2 m apuntando abajo: el cono acaba en (0, 0, 0) y su base mide 2·tan(30°).
        let f = FuenteLuz { pos: [0.0, 2.0, 0.0], dir: [0.0, -1.0, 0.0], cd: 1000.0, apertura: 60.0, color: [1.0; 3], radio: 0.0, eje_u: [1.0, 0.0, 0.0], medio: [0.0; 2], nitidez: 1.0, ies: None };
        let m = modelo_cono(&f);
        let fin = [m[3][0] + m[2][0], m[3][1] + m[2][1], m[3][2] + m[2][2]];
        assert!(fin[1].abs() < 1e-5, "{fin:?}");
        let radio = (m[0][0] * m[0][0] + m[0][1] * m[0][1] + m[0][2] * m[0][2]).sqrt();
        assert!((radio - 2.0 * 30f32.to_radians().tan()).abs() < 1e-4, "{radio}");
    }

    #[test]
    fn params_hasta_16_luces() {
        let luz = |i: usize| FuenteLuz { pos: [i as f32, 2.0, 0.0], dir: [0.0, -1.0, 0.0], cd: 1.0 + i as f32, apertura: 40.0, color: [1.0; 3], radio: 0.0, eje_u: [1.0, 0.0, 0.0], medio: [0.0; 2], nitidez: 1.0, ies: None };
        let pocas: Vec<FuenteLuz> = (0..20).map(luz).collect();
        let paq = empaquetar(&pocas);
        assert_eq!((paq.n_luces, paq.n_sombras), (20, 16), "las luces 17–20 iluminan sin sombra");
        assert_eq!(paq.pos_cd.len(), 4 * MAX_LUCES);
        assert_eq!(&paq.pos_cd[4 * 17..4 * 17 + 4], &[17.0, 2.0, 0.0, 18.0]);
        let muchas: Vec<FuenteLuz> = (0..40).map(luz).collect();
        assert_eq!(empaquetar(&muchas).n_luces, MAX_LUCES);
    }

    #[test]
    fn empaquetar_eje_y_emisor() {
        let f = FuenteLuz { pos: [0.0; 3], dir: [0.0, -1.0, 0.0], cd: 1.0, apertura: 40.0, color: [1.0; 3], radio: 0.0, eje_u: [0.0, 0.0, 1.0], medio: [0.635, 0.355], nitidez: 2.0, ies: None };
        let paq = empaquetar(&[f.clone(), f]);
        assert_eq!(&paq.eje[4..8], &[0.0, 0.0, 1.0, 2.0]);
        assert_eq!(&paq.emisor[4..8], &[0.635, 0.355, 0.0, 0.0]);
    }

    #[test]
    fn color_medio_pondera_por_cd() {
        let rojo = FuenteLuz { pos: [0.0; 3], dir: [0.0, -1.0, 0.0], cd: 3.0, apertura: 40.0, color: [1.0, 0.0, 0.0], radio: 0.0, eje_u: [1.0, 0.0, 0.0], medio: [0.0; 2], nitidez: 1.0, ies: None };
        let azul = FuenteLuz { cd: 1.0, color: [0.0, 0.0, 1.0], ..rojo.clone() };
        let c = color_medio(&[rojo, azul]);
        assert!((c[0] - 0.75).abs() < 1e-6 && (c[2] - 0.25).abs() < 1e-6, "{c:?}");
        assert_eq!(color_medio(&[]), [1.0; 3]);
    }

    #[test]
    fn tamano_penumbra_crece_con_la_distancia() {
        // Bloqueador a 1 m de la luz y receptor a 3 m, emisor de 0.45 m: penumbra (3 − 1)·0.45 / 1 = 0.9 m.
        assert!((tamano_penumbra(3.0, 1.0, 0.45) - 0.9).abs() < 1e-6);
        // Pegado al receptor no hay penumbra; un emisor mayor da más penumbra.
        assert_eq!(tamano_penumbra(2.0, 2.0, 0.45), 0.0);
        assert!(tamano_penumbra(3.0, 1.0, 0.45) > tamano_penumbra(3.0, 1.0, 0.08));
        assert_eq!(tamano_penumbra(3.0, 0.0, 0.45), f32::INFINITY);
    }
}
