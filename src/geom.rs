use crate::modelo::Sala;
use eframe::egui;
use std::ops::{Add, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct V3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl V3 {
    pub fn new(x: f32, y: f32, z: f32) -> V3 {
        V3 { x, y, z }
    }
    pub fn dot(self, o: V3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    pub fn cross(self, o: V3) -> V3 {
        V3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }
    pub fn len(self) -> f32 {
        self.dot(self).sqrt()
    }
    /// Zero (or near-zero) vectors normalize to zero instead of NaN.
    pub fn norm(self) -> V3 {
        let l = self.len();
        if l > 1e-12 { self * (1.0 / l) } else { V3::default() }
    }
    pub fn from_arr(a: [f32; 3]) -> V3 {
        V3::new(a[0], a[1], a[2])
    }
}

impl Add for V3 {
    type Output = V3;
    fn add(self, o: V3) -> V3 {
        V3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl Sub for V3 {
    type Output = V3;
    fn sub(self, o: V3) -> V3 {
        V3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl Mul<f32> for V3 {
    type Output = V3;
    fn mul(self, k: f32) -> V3 {
        V3::new(self.x * k, self.y * k, self.z * k)
    }
}

impl Neg for V3 {
    type Output = V3;
    fn neg(self) -> V3 {
        V3::new(-self.x, -self.y, -self.z)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Camara {
    pub objetivo: V3,
    pub yaw: f32,
    pub pitch: f32,
    pub distancia: f32,
    pub fov_y: f32,
}

impl Camara {
    pub fn para_sala(s: &Sala) -> Camara {
        Camara {
            objetivo: V3::new(s.ancho / 2.0, s.alto_techo / 2.0, s.largo / 2.0),
            yaw: 0.8,
            pitch: 0.6,
            distancia: 1.8 * s.ancho.max(s.largo),
            fov_y: 50f32.to_radians(),
        }
    }

    /// Cámara que mira desde `pos` en la dirección `dir` (unitaria); `ojo()` vuelve a dar `pos`.
    pub fn desde_escena(pos: V3, dir: V3, fov_y: f32) -> Camara {
        // ojo = objetivo + (cos p·sin y, sin p, cos p·cos y): con distancia 1, ese vector ha de ser −dir.
        Camara {
            objetivo: pos + dir,
            yaw: (-dir.x).atan2(-dir.z),
            pitch: (-dir.y).clamp(-1.0, 1.0).asin(),
            distancia: 1.0,
            fov_y,
        }
    }

    pub fn ojo(&self) -> V3 {
        let (cp, sp) = (self.pitch.cos(), self.pitch.sin());
        self.objetivo + V3::new(cp * self.yaw.sin(), sp, cp * self.yaw.cos()) * self.distancia
    }

    /// Camera basis (forward, right, up). Right is forward x world-up, so it stays
    /// horizontal; pitch is clamped in `orbitar` so forward is never parallel to world-up.
    pub fn base(&self) -> (V3, V3, V3) {
        let forward = (self.objetivo - self.ojo()).norm();
        let right = forward.cross(V3::new(0.0, 1.0, 0.0)).norm();
        let up = right.cross(forward);
        (forward, right, up)
    }

    /// Returns the screen position and depth along the view axis; None if behind the near plane.
    pub fn proyectar(&self, p: V3, rect: egui::Rect) -> Option<(egui::Pos2, f32)> {
        let (forward, right, up) = self.base();
        let d = p - self.ojo();
        let depth = d.dot(forward);
        if depth < 0.05 {
            return None;
        }
        let f = 1.0 / (self.fov_y * 0.5).tan();
        let aspect = rect.width() / rect.height();
        let ndc_x = d.dot(right) / depth * f / aspect;
        let ndc_y = d.dot(up) / depth * f;
        let s = rect.center()
            + egui::Vec2::new(ndc_x * rect.width() / 2.0, -ndc_y * rect.height() / 2.0);
        Some((s, depth))
    }

    /// Ray (origin, unit direction) through a screen position; exact inverse of `proyectar`.
    pub fn rayo(&self, s: egui::Pos2, rect: egui::Rect) -> (V3, V3) {
        let (forward, right, up) = self.base();
        let f = 1.0 / (self.fov_y * 0.5).tan();
        let aspect = rect.width() / rect.height();
        let c = rect.center();
        let ndc_x = (s.x - c.x) / (rect.width() / 2.0);
        let ndc_y = -(s.y - c.y) / (rect.height() / 2.0);
        let x = ndc_x * aspect / f;
        let y = ndc_y / f;
        (self.ojo(), (forward + right * x + up * y).norm())
    }

    pub fn orbitar(&mut self, d: egui::Vec2) {
        self.yaw -= d.x * 0.01;
        self.pitch = (self.pitch + d.y * 0.01).clamp(0.05, 1.5);
    }

    pub fn zoom(&mut self, scroll: f32) {
        self.distancia = (self.distancia * (-scroll * 0.002).exp()).clamp(1.0, 100.0);
    }
}

/// Parameter `t` such that `punto + t*eje` is the point of that line closest to the ray
/// `origen + s*dir`. Returns 0.0 when the lines are parallel.
pub fn param_mas_cercano(origen: V3, dir: V3, punto: V3, eje: V3) -> f32 {
    let w = origen - punto;
    let a = dir.dot(dir);
    let b = dir.dot(eje);
    let c = eje.dot(eje);
    let d = dir.dot(w);
    let e = eje.dot(w);
    let denom = a * c - b * b;
    if denom.abs() < 1e-6 {
        return 0.0;
    }
    (a * e - b * d) / denom
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::new(0.0, 0.0), egui::Vec2::new(800.0, 600.0))
    }

    fn camara() -> Camara {
        Camara {
            objetivo: V3::new(1.0, 2.0, 3.0),
            yaw: 0.8,
            pitch: 0.6,
            distancia: 9.0,
            fov_y: 50f32.to_radians(),
        }
    }

    #[test]
    fn objetivo_al_centro() {
        let c = camara();
        let (p, _) = c.proyectar(c.objetivo, rect()).expect("objetivo visible");
        assert!((p.x - rect().center().x).abs() < 1e-3);
        assert!((p.y - rect().center().y).abs() < 1e-3);
    }

    #[test]
    fn detras_es_none() {
        let c = camara();
        let ojo = c.ojo();
        let detras = ojo + (ojo - c.objetivo).norm() * 2.0;
        assert!(c.proyectar(detras, rect()).is_none());
    }

    #[test]
    fn rayo_inverso_de_proyectar() {
        let c = camara();
        for p in [
            V3::new(0.0, 0.0, 0.0),
            V3::new(2.5, 1.2, 4.0),
            V3::new(-1.0, 3.0, 6.0),
        ] {
            let (s, _) = c.proyectar(p, rect()).expect("visible");
            let (o, d) = c.rayo(s, rect());
            let w = p - o;
            let perp = w - d * w.dot(d);
            assert!(perp.len() < 1e-3, "distancia al rayo {}", perp.len());
        }
    }

    #[test]
    fn param_cercano_recupera_punto() {
        // Rayo por (2,1,0) en direccion Z; recta (0,1,0)+t·X: el punto mas cercano es t=2.
        let t = param_mas_cercano(
            V3::new(2.0, 1.0, 0.0),
            V3::new(0.0, 0.0, 1.0),
            V3::new(0.0, 1.0, 0.0),
            V3::new(1.0, 0.0, 0.0),
        );
        assert!((t - 2.0).abs() < 1e-4, "t = {t}");
    }

    #[test]
    fn param_cercano_paralelo_es_cero() {
        let t = param_mas_cercano(
            V3::new(2.0, 1.0, 0.0),
            V3::new(1.0, 0.0, 0.0),
            V3::new(0.0, 5.0, 0.0),
            V3::new(1.0, 0.0, 0.0),
        );
        assert_eq!(t, 0.0);
    }

    #[test]
    fn norm_de_cero_no_es_nan() {
        let n = V3::default().norm();
        assert!(!n.x.is_nan() && !n.y.is_nan() && !n.z.is_nan());
        assert_eq!(n, V3::default());
    }

    #[test]
    fn pitch_limitado() {
        let mut c = camara();
        c.orbitar(egui::Vec2::new(0.0, 1.0e6));
        assert_eq!(c.pitch, 1.5);
    }

    #[test]
    fn desde_escena_ojo_y_direccion() {
        use crate::modelo::direccion;
        let pos = V3::new(1.0, 2.0, 3.0);
        for (giro, incl) in [(180.0, -10.0), (45.0, -60.0), (300.0, 20.0)] {
            let dir = V3::from_arr(direccion(giro, incl));
            let c = Camara::desde_escena(pos, dir, 0.5);
            let (ojo, base) = (c.ojo(), c.base().0);
            assert!((ojo - pos).len() < 1e-4, "ojo {ojo:?}");
            assert!((base - dir).len() < 1e-4, "dir {base:?}");
        }
    }
}
