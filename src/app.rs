use crate::escena::{arrastrar, centro_luz, construir, dibujar, dibujar_superpuesto, elegir, malla_gpu, malla_sombras, reflexion_de_color, triangulos, Prim, Sel};
use crate::fotometria::{
    balance_blancos, color_cct, color_lineal, color_lux, color_luz, etiqueta_obturacion, ev100, ev100_auto, exposicion, fuentes,
    indirecta_lux, intensidad_cd, lux_en, sobre_superficie, DIAFRAGMAS, ISOS, OBTURACIONES,
};
use crate::config::{
    cargar, cargar_acabados, es_integrada, es_software, guardar, guardar_acabados, limitar_a_gpu, por_defecto_para,
    ruta_acabados, ruta_config, ruta_ies, Configuracion, Preset,
};
use crate::trazado::Fallo;
use crate::geom::{Camara, V3};
use crate::ies::{self, Ies};
use crate::importar::{cargar_biblioteca, guardar_en_biblioteca, importar, nombre_libre, ruta_biblioteca};
use crate::inventario::{ruta_por_defecto, Inventario};
use crate::modelo::{
    apuntar, es_categoria_luz, nombre_preset_valido, Acabado, AjustesLuz, CamaraEscena, Equipo, Malla, Montaje, NivelToma, Pared,
    Superficie, SpecsLuz, TipoLuz, TipoObjeto, ACABADOS, ALTURA_PISO_MAX, ALTURA_PISO_MIN, CATEGORIA_LUCES, CATEGORIA_LUCES_PISO,
    FORMATOS, GRUPOS, SENSORES, TONOS_PIEL,
};
use crate::render_gl::{color_medio, info_gpu, matriz_proyeccion, matriz_vista, ModoVista, ParamsFrame, Renderer};
use eframe::{egui, egui_glow, glow};
use std::collections::{BTreeMap, HashMap};
use std::ops::RangeInclusive;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Acción que espera a que se resuelvan los cambios sin guardar.
#[derive(Clone, Copy)]
enum Accion {
    Nuevo,
    Abrir,
    Cerrar,
}

#[derive(Clone, Copy)]
enum Opcion {
    Guardar,
    Descartar,
    Cancelar,
}

enum FilaInv {
    Editar(usize),
    Borrar(usize),
    Anadir(String),
    AnadirCategoria,
    BorrarCategoria(String),
}

/// Clave de la lectura de lux bajo el cursor: versión de la escena, puntero y cámara (bits).
type LuxClave = (u64, u64, egui::Pos2, [u32; 6]);

pub struct App {
    montaje: Montaje,
    ruta: Option<PathBuf>,
    sucio: bool,
    inventario: Inventario,
    /// Modelos importados guardados en la biblioteca del usuario.
    biblioteca: Vec<Malla>,
    camara: Camara,
    seleccion: Option<Sel>,
    arrastrando: bool,
    pendiente: Option<Accion>,
    mensaje: Option<String>,
    ver_inventario: bool,
    ver_selector: bool,
    ver_selector_piso: bool,
    ver_objetos: bool,
    /// Texto del campo de id (carril, toma u objeto) de la selección `edicion_de`.
    edicion: String,
    edicion_de: Option<Sel>,
    /// Copia editable de las filas del inventario (se rehace tras cada cambio).
    inv_filas: Vec<Equipo>,
    /// Fila "añadir" de cada categoría, creada al mostrarla.
    inv_nuevos: HashMap<String, Equipo>,
    nueva_categoria: String,
    /// Categoría de la pestaña seleccionada en el inventario.
    inv_pestana: String,
    /// Último título enviado a la ventana; evita un comando (y un repintado) por fotograma.
    titulo_puesto: String,
    /// Ventanas y modales abiertos en el fotograma anterior; si cambian, se repinta.
    ultimo_abiertos: Option<[bool; 8]>,
    gl: Option<Arc<glow::Context>>,
    /// Renderer GPU; None si no hay OpenGL o no compiló (entonces se usa Esquema).
    render: Option<Arc<Mutex<Renderer>>>,
    modo: ModoVista,
    /// Calidad de render (preset y ajustes), tal como está aplicada.
    cfg: Configuracion,
    /// Fabricante, renderer y versión de OpenGL.
    gpu: (String, String, String),
    ver_config: bool,
    ver_superficies: bool,
    /// Superficie seleccionada en la ventana Superficies.
    superficie_editada: Superficie,
    /// Presets de pared del usuario (biblioteca en %APPDATA%\SalaLuces\acabados.json).
    acabados_usuario: BTreeMap<String, Acabado>,
    /// true si la biblioteca no se pudo leer: no se sobrescribe el archivo del usuario.
    acabados_bloqueados: bool,
    /// Perfiles IES de la biblioteca (%APPDATA%\SalaLuces\ies): nombre → texto del archivo.
    ies_biblioteca: BTreeMap<String, String>,
    /// Nombre escrito para el preset nuevo de la ventana Paredes.
    nombre_preset: String,
    humo: bool,
    /// Sube a cada cambio del montaje (`marcar_cambio`); la malla GPU se rehace solo si cambió.
    version: u64,
    subida: Option<u64>,
    /// Cámara: exposición automática, o ISO, f/N y obturación (s) elegidos a mano.
    expo_auto: bool,
    iso: f32,
    diafragma: f32,
    obturacion: f32,
    /// EV100 automático de la escena; se recalcula al cambiar el montaje o el objetivo (`clave_expo`).
    ev_auto: f32,
    clave_expo: Option<(u64, [u32; 3])>,
    /// Triángulos de la escena con la versión con que se hicieron (rayos de sombra y de lux).
    tris: Option<(u64, Vec<[[f32; 3]; 3]>)>,
    /// Última lectura de lux bajo el cursor y la clave con que se calculó.
    lux_cursor: Option<(LuxClave, Option<f32>)>,
    /// Cámara de escena en la que se mira (vista de cámara); None con la vista libre.
    vista_camara: Option<String>,
}

fn vacio() -> Equipo {
    Equipo {
        codigo: String::new(),
        marca: String::new(),
        modelo: String::new(),
        categoria: CATEGORIA_LUCES.to_string(),
        specs: None,
    }
}

/// Equipos de una categoría que aún no están colocados en el montaje.
fn sin_colocar<'a>(equipos: &'a [Equipo], montaje: &Montaje, categoria: &str) -> Vec<&'a Equipo> {
    equipos.iter().filter(|e| e.categoria == categoria && !montaje.equipo_colocado(&e.codigo)).collect()
}

/// Número con coma o punto como separador decimal; None si no es finito.
fn analiza(s: &str) -> Option<f64> {
    s.trim().replace(',', ".").parse::<f64>().ok().filter(|x| x.is_finite())
}

/// Campo numérico en metros; acepta coma o punto como separador decimal.
/// Texto sobre fondo oscuro semitransparente, legible sobre la escena.
fn texto_con_fondo(pintor: &egui::Painter, pos: egui::Pos2, ancla: egui::Align2, texto: impl ToString, tam: f32, color: egui::Color32) {
    let galley = pintor.layout_no_wrap(texto.to_string(), egui::FontId::proportional(tam), color);
    // El texto va en la caja sin margen; el fondo la rodea con 6 px a los lados y 3 arriba y abajo.
    let caja = ancla.anchor_size(pos, galley.size());
    pintor.rect_filled(caja.expand2(egui::vec2(6.0, 3.0)), 4.0, egui::Color32::from_black_alpha(150));
    pintor.galley(caja.min, galley, color);
}

fn numero(ui: &mut egui::Ui, v: &mut f32, rango: RangeInclusive<f32>) -> bool {
    ui.add(egui::DragValue::new(v).speed(0.01).max_decimals(2).range(rango).suffix(" m").custom_parser(analiza))
        .changed()
}

/// Cambio hecho por el usuario en un deslizador o en su casilla numérica (escrita y confirmada con
/// Enter o al salir del campo). Un deslizador marca cambio al redondear un valor fraccionario (p. ej.
/// el ángulo de «Apuntar a…») aunque nadie lo toque; eso no cuenta.
fn tocado(r: &egui::Response) -> bool {
    r.changed() && (r.dragged() || r.clicked() || r.drag_stopped() || r.has_focus() || r.lost_focus())
}

/// Rectángulo de proporción `aspecto` (ancho/alto) centrado en `rect`: el mayor que cabe (letterbox).
fn encuadre(rect: egui::Rect, aspecto: f32) -> egui::Rect {
    let (w, h) = (rect.width(), rect.height());
    let tamano = if w > h * aspecto { egui::vec2(h * aspecto, h) } else { egui::vec2(w, w / aspecto) };
    egui::Rect::from_center_size(rect.center(), tamano)
}

/// Rango `min..=max` ampliado para incluir `v`.
fn ampliar_rango(min: f32, max: f32, v: f32) -> (f32, f32) {
    (min.min(v), max.max(v))
}

/// Desplegable de valores de la cámara; `etiqueta` dice cómo se escribe cada uno.
fn combo_valores(ui: &mut egui::Ui, id: &str, v: &mut f32, valores: &[f32], etiqueta: impl Fn(f32) -> String) {
    egui::ComboBox::from_id_salt(id).selected_text(etiqueta(*v)).show_ui(ui, |ui| {
        for &x in valores {
            ui.selectable_value(v, x, etiqueta(x));
        }
    });
}

fn etiqueta_nivel(n: NivelToma) -> &'static str {
    match n {
        NivelToma::Carriles => "Carriles",
        NivelToma::Suelo => "Suelo",
    }
}

/// Especificaciones típicas de una luz nueva de techo (`piso` falso) o de piso.
fn specs_por_defecto(piso: bool) -> SpecsLuz {
    if piso { TipoLuz::PanelPie.specs() } else { TipoLuz::PanelLed.specs() }
}

/// Fila vacía de la pestaña `cat`; las de luces traen especificaciones típicas.
fn fila_nueva(cat: &str) -> Equipo {
    let mut e = vacio();
    e.categoria = cat.to_string();
    if es_categoria_luz(cat) {
        e.specs = Some(specs_por_defecto(cat == CATEGORIA_LUCES_PISO));
    }
    e
}

/// Un campo de especificación; solo se puede cambiar si `en`. Velocidad: 1/200 del rango.
fn campo_specs(ui: &mut egui::Ui, v: &mut f32, rango: RangeInclusive<f32>, en: bool) -> bool {
    let velocidad = f64::from(*rango.end() - *rango.start()) / 200.0;
    ui.add_enabled(en, egui::DragValue::new(v).speed(velocidad).max_decimals(0).range(rango).custom_parser(analiza))
        .changed()
}

/// Celdas de una fila de luces del inventario: tipo (rellena las especificaciones típicas) y valores.
/// Con `editable` falso se ven, pero no se cambian. Devuelve true si cambió algo.
fn celdas_specs(ui: &mut egui::Ui, id: (&str, usize), s: &mut SpecsLuz, piso: bool, editable: bool) -> bool {
    let mut cambio = false;
    let mut tipo = None;
    ui.add_enabled_ui(editable, |ui| {
        egui::ComboBox::from_id_salt(id).selected_text(s.tipo.nombre()).show_ui(ui, |ui| {
            for t in TipoLuz::TODOS.into_iter().filter(|t| t.es_piso() == piso) {
                ui.selectable_value(&mut tipo, Some(t), t.nombre());
            }
        });
    });
    if let Some(t) = tipo {
        *s = t.specs();
        cambio = true;
    }
    cambio |= campo_specs(ui, &mut s.potencia_w, 0.0..=100_000.0, editable);
    cambio |= campo_specs(ui, &mut s.lux_1m, 0.0..=1_000_000.0, editable);
    cambio |= campo_specs(ui, &mut s.apertura_min, 1.0..=179.0, editable);
    cambio |= campo_specs(ui, &mut s.apertura_max, 1.0..=179.0, editable);
    cambio |= campo_specs(ui, &mut s.cct_min, 1000.0..=40_000.0, editable);
    cambio |= campo_specs(ui, &mut s.cct_max, 1000.0..=40_000.0, editable);
    cambio |= campo_specs(ui, &mut s.cri, 0.0..=100.0, editable);
    if cambio {
        // El máximo nunca baja del mínimo.
        s.apertura_max = s.apertura_max.max(s.apertura_min);
        s.cct_max = s.cct_max.max(s.cct_min);
    }
    cambio
}

/// Especificaciones de fabricante de una luz, editables. `piso` elige los tipos de piso o de techo.
/// `perfiles` son los nombres de la biblioteca IES; con perfil, lux a 1 m y apertura quedan fijos (según IES).
/// Pone `importar` a true si se pidió «Importar .ies…». Devuelve true si cambió algo.
fn editar_specs(ui: &mut egui::Ui, id: &str, s: &mut SpecsLuz, piso: bool, perfiles: &[String], importar: &mut bool) -> bool {
    let mut cambio = false;
    let mut tipo = None;
    let con_ies = s.ies.is_some();
    egui::Grid::new(("specs", id)).show(ui, |ui| {
        ui.label("Tipo");
        egui::ComboBox::from_id_salt(("tipo_specs", id)).height(600.0).selected_text(s.tipo.nombre()).show_ui(ui, |ui| {
            for t in TipoLuz::TODOS.into_iter().filter(|t| t.es_piso() == piso) {
                ui.selectable_value(&mut tipo, Some(t), t.nombre());
            }
        });
        ui.end_row();
        ui.label("Fotometría (IES)");
        let antes = s.ies.clone();
        let etiqueta = antes.clone().unwrap_or_else(|| "(ninguna)".to_string());
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt(("ies_specs", id)).selected_text(etiqueta).show_ui(ui, |ui| {
                ui.selectable_value(&mut s.ies, None, "(ninguna)");
                for n in perfiles {
                    ui.selectable_value(&mut s.ies, Some(n.clone()), n);
                }
            });
            if ui.button("Importar .ies…").clicked() {
                *importar = true;
            }
        });
        cambio |= s.ies != antes;
        ui.end_row();
        let SpecsLuz { potencia_w, lux_1m, apertura_min, apertura_max, cct_min, cct_max, cri, .. } = s;
        // Lo que la tabla IES fija: lux a 1 m y apertura (el tercer campo marca esas filas).
        let filas: [(&str, &mut f32, RangeInclusive<f32>, bool); 7] = [
            ("Potencia (W)", potencia_w, 0.0..=100_000.0, false),
            ("Lux a 1 m", lux_1m, 0.0..=1_000_000.0, true),
            ("Apertura mín (°)", apertura_min, 1.0..=179.0, true),
            ("Apertura máx (°)", apertura_max, 1.0..=179.0, true),
            ("Temperatura mín (K)", cct_min, 1000.0..=40_000.0, false),
            ("Temperatura máx (K)", cct_max, 1000.0..=40_000.0, false),
            ("CRI", cri, 0.0..=100.0, false),
        ];
        for (etiqueta, v, rango, de_tabla) in filas {
            let fijo = de_tabla && con_ies;
            ui.label(if fijo { format!("{etiqueta} (según IES)") } else { etiqueta.to_string() });
            cambio |= campo_specs(ui, v, rango, !fijo);
            ui.end_row();
        }
        ui.label("Color (RGB)");
        cambio |= ui.checkbox(&mut s.rgb, "").changed();
        ui.end_row();
    });
    if let Some(t) = tipo {
        *s = t.specs();
        cambio = true;
    }
    if cambio {
        // El máximo nunca baja del mínimo.
        s.apertura_max = s.apertura_max.max(s.apertura_min);
        s.cct_max = s.cct_max.max(s.cct_min);
    }
    cambio
}

/// Perfil IES de una luz, leído del texto del montaje; None si no tiene perfil o no se lee.
fn ies_de(m: &Montaje, s: &SpecsLuz) -> Option<Ies> {
    s.ies.as_deref().and_then(|n| m.fotometrias.get(n)).and_then(|t| ies::leer(t).ok())
}

/// Objetos a los que puede apuntar una luz: nombre y centro.
fn destinos_apuntar(m: &Montaje) -> Vec<(String, [f32; 3])> {
    m.objetos.iter().map(|o| (o.nombre.clone(), [o.x, o.alto / 2.0, o.z])).collect()
}

/// Ajustes de una luz: encendida, dimmer, apertura y temperatura (en su rango; fijas si no hay rango),
/// giro, inclinación y «Apuntar a…». `origen` es el foco y `ies` el perfil de la luz, si lo tiene.
/// Devuelve true si cambió algo.
fn controles_ajustes(
    ui: &mut egui::Ui,
    s: &mut SpecsLuz,
    a: &mut AjustesLuz,
    origen: [f32; 3],
    destinos: &[(String, [f32; 3])],
    id: &str,
    ies: Option<&Ies>,
) -> bool {
    let mut cambio = ui.checkbox(&mut a.encendida, "Encendida").changed();
    ui.horizontal(|ui| {
        ui.label("Dimmer");
        let mut pct = a.dimmer * 100.0;
        let r = ui.add(egui::Slider::new(&mut pct, 0.0..=100.0).suffix(" %").fixed_decimals(0));
        if tocado(&r) {
            a.dimmer = pct / 100.0;
            cambio = true;
        }
    });
    // Los deslizadores van siempre libres; si el valor sale del rango de las specs, el rango se amplía.
    // Al escribir se confirma al salir del campo: si no, «120» pasaría por «1» y ampliaría el rango mal.
    ui.horizontal(|ui| {
        ui.label("Apertura");
        let r = ui.add(egui::Slider::new(&mut a.apertura, 1.0..=179.0).suffix("°").fixed_decimals(1).update_while_editing(false));
        if tocado(&r) {
            (s.apertura_min, s.apertura_max) = ampliar_rango(s.apertura_min, s.apertura_max, a.apertura);
            cambio = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label("Temperatura");
        let r = ui.add(egui::Slider::new(&mut a.cct, 1000.0..=20_000.0).suffix(" K").fixed_decimals(0).update_while_editing(false));
        if tocado(&r) {
            (s.cct_min, s.cct_max) = ampliar_rango(s.cct_min, s.cct_max, a.cct);
            cambio = true;
        }
        let (franja, _) = ui.allocate_exact_size(egui::vec2(28.0, 14.0), egui::Sense::hover());
        ui.painter().rect_filled(franja, 2.0, color_cct(a.cct));
    });
    if s.rgb {
        ui.horizontal(|ui| {
            ui.label("Saturación");
            let mut pct = a.saturacion * 100.0;
            let r = ui.add(egui::Slider::new(&mut pct, 0.0..=100.0).suffix(" %").fixed_decimals(0));
            if tocado(&r) {
                a.saturacion = pct / 100.0;
                cambio = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Tono");
            let r = ui.add(egui::Slider::new(&mut a.tono, 0.0..=360.0).suffix("°").fixed_decimals(0));
            cambio |= tocado(&r);
            let (franja, _) = ui.allocate_exact_size(egui::vec2(28.0, 14.0), egui::Sense::hover());
            ui.painter().rect_filled(franja, 2.0, color_lineal(color_luz(a)));
        });
    }
    ui.horizontal(|ui| {
        ui.label("Giro");
        cambio |= ui
            .add(egui::DragValue::new(&mut a.giro).speed(1.0).max_decimals(0).suffix("°").custom_parser(analiza))
            .changed();
    });
    ui.horizontal(|ui| {
        ui.label("Inclinación");
        let r = ui.add(egui::Slider::new(&mut a.inclinacion, -90.0..=90.0).suffix("°").fixed_decimals(0));
        cambio |= tocado(&r);
    });
    ui.horizontal(|ui| {
        ui.label("Apuntar a");
        let mut elegido: Option<usize> = None;
        let texto = if destinos.is_empty() { "(no hay objetos)" } else { "elegir…" };
        egui::ComboBox::from_id_salt(("apuntar", id)).selected_text(texto).show_ui(ui, |ui| {
            for (k, (nombre, _)) in destinos.iter().enumerate() {
                ui.selectable_value(&mut elegido, Some(k), nombre.as_str());
            }
        });
        if let Some((_, p)) = elegido.and_then(|k| destinos.get(k)) {
            let (giro, inclinacion) = apuntar(origen, *p);
            a.giro = giro;
            a.inclinacion = inclinacion;
            cambio = true;
        }
    });
    if cambio {
        a.limitar(s);
    }
    let potencia = if a.encendida { s.potencia_w * a.dimmer } else { 0.0 };
    ui.label(format!("Intensidad: {:.0} cd · Potencia: {:.0} W", intensidad_cd(s, a, ies), potencia));
    cambio
}

/// Preset un escalón por debajo, si lo hay: Fotorrealista → Alta → Rápida (sin MSAA, sombras 1024, escala 1).
fn bajar_preset(c: &Configuracion) -> Option<Configuracion> {
    match c.preset {
        Preset::Fotorrealista => Some(Configuracion { preset: Preset::Alta, ..c.clone() }),
        Preset::Alta => Some(Configuracion { preset: Preset::Rapida, msaa: 0, res_sombras: 1024, escala: 1.0, ..c.clone() }),
        Preset::Rapida => None,
    }
}

/// Crea el renderer con `cfg`. Si falla, baja de preset (`bajar_preset`) y avisa de cada escalón.
fn iniciar_render(gl: &glow::Context, cfg: Configuracion) -> (Option<Renderer>, Configuracion, Option<String>) {
    let mut actual = cfg.clone();
    let mut avisos = Vec::new();
    loop {
        match Renderer::new(gl, &actual) {
            Ok(r) => return (Some(r), actual, (!avisos.is_empty()).then(|| avisos.join("\n"))),
            Err(e) => {
                eprintln!("render GPU: {e}");
                match bajar_preset(&actual) {
                    Some(menor) => {
                        avisos.push(format!("Tu GPU no admite {}: se usa {}.", actual.preset.nombre(), menor.preset.nombre()));
                        actual = menor;
                    }
                    None => {
                        let msg = format!("No se pudo iniciar el render 3D; se usa la vista esquemática.\n{e}");
                        return (None, cfg, Some(msg));
                    }
                }
            }
        }
    }
}

/// Preset de la variable de entorno SALA_PRESET (rapida, alta, fotorrealista), para pruebas. No se guarda.
fn preset_de_entorno() -> Option<Preset> {
    match std::env::var("SALA_PRESET").ok()?.to_lowercase().as_str() {
        "rapida" | "rápida" => Some(Preset::Rapida),
        "alta" => Some(Preset::Alta),
        "fotorrealista" => Some(Preset::Fotorrealista),
        _ => None,
    }
}

fn pedir_modelo() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Modelos 3D", &["glb", "gltf", "obj"])
        .pick_file()
}

fn pedir_ruta_guardar() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Montaje de sala", &["json"])
        .set_file_name("montaje.sala.json")
        .save_file()
}

impl App {
    pub fn new(cc: &eframe::CreationContext) -> App {
        cc.egui_ctx.global_style_mut(|s| {
            s.spacing.item_spacing = egui::vec2(8.0, 6.0);
            s.spacing.button_padding = egui::vec2(8.0, 4.0);
        });
        let (inventario, aviso) = Inventario::cargar(ruta_por_defecto());
        let (biblioteca, avisos_biblioteca) = cargar_biblioteca(&ruta_biblioteca());
        let mut avisos: Vec<String> = aviso.into_iter().collect();
        avisos.extend(avisos_biblioteca);
        let (acabados_usuario, acabados_bloqueados) = match cargar_acabados(&ruta_acabados()) {
            Ok(a) => (a, false),
            Err(e) => {
                avisos.push(format!("No se pudieron leer los presets de pared: {e}"));
                (BTreeMap::new(), true)
            }
        };
        let (ies_biblioteca, avisos_ies) = ies::cargar_biblioteca(&ruta_ies());
        avisos.extend(avisos_ies);
        let montaje = Montaje::default();
        let camara = Camara::para_sala(&montaje.sala);
        let gpu = cc.gl.as_deref().map(info_gpu).unwrap_or_default();
        let ruta_cfg = ruta_config();
        let guardada = cargar(&ruta_cfg);
        if guardada.is_none() && ruta_cfg.exists() {
            avisos.push("La configuración guardada no se pudo leer; se usan los valores recomendados para la GPU.".to_string());
        }
        let cfg = limitar_a_gpu(guardada.unwrap_or_else(|| por_defecto_para(&gpu.1, &gpu.0)), &gpu.1, &gpu.0);
        // Con SALA_PRESET el preset cambia solo en esta sesión: no se guarda en la configuración.
        let forzado = preset_de_entorno();
        let cfg = Configuracion { preset: forzado.unwrap_or(cfg.preset), ..cfg };
        let inicial = cfg.clone();
        let (gl, render, modo, cfg) = match cc.gl.clone() {
            Some(gl) => {
                let (render, cfg, aviso) = iniciar_render(&gl, cfg);
                avisos.extend(aviso);
                // Si hubo que bajar de preset, la nueva configuración queda guardada.
                if cfg != inicial && forzado.is_none() {
                    if let Err(e) = guardar(&ruta_cfg, &cfg) {
                        avisos.push(e);
                    }
                }
                let modo = if render.is_some() { ModoVista::Realista } else { ModoVista::Esquema };
                (Some(gl), render.map(|r| Arc::new(Mutex::new(r))), modo, cfg)
            }
            None => {
                avisos.push("No hay contexto OpenGL; se usa la vista esquemática.".to_string());
                (None, None, ModoVista::Esquema, cfg)
            }
        };
        let mut app = App {
            gl,
            render,
            modo,
            cfg,
            gpu,
            ver_config: false,
            ver_superficies: false,
            superficie_editada: Superficie::Pared(Pared::Sur),
            acabados_usuario,
            acabados_bloqueados,
            ies_biblioteca,
            nombre_preset: String::new(),
            humo: false,
            version: 0,
            subida: None,
            expo_auto: true,
            iso: 400.0,
            diafragma: 2.8,
            obturacion: 1.0 / 50.0,
            ev_auto: 0.0,
            clave_expo: None,
            tris: None,
            lux_cursor: None,
            vista_camara: None,
            montaje,
            ruta: None,
            sucio: false,
            inventario,
            biblioteca,
            camara,
            seleccion: None,
            arrastrando: false,
            pendiente: None,
            mensaje: (!avisos.is_empty()).then(|| avisos.join("\n")),
            ver_inventario: false,
            ver_selector: false,
            ver_selector_piso: false,
            ver_objetos: false,
            edicion: String::new(),
            edicion_de: None,
            inv_filas: Vec::new(),
            inv_nuevos: HashMap::new(),
            nueva_categoria: String::new(),
            inv_pestana: CATEGORIA_LUCES.to_string(),
            titulo_puesto: String::new(),
            ultimo_abiertos: None,
        };
        app.sincronizar_inventario();
        app
    }

    /// Montaje modificado: sin guardar, y la escena GPU se vuelve a subir en la próxima vista.
    fn marcar_cambio(&mut self) {
        self.sucio = true;
        self.version += 1;
    }

    /// Muestra un error en el modal; devuelve el valor si fue Ok.
    fn avisar<T>(&mut self, r: Result<T, String>) -> Option<T> {
        match r {
            Ok(v) => Some(v),
            Err(e) => {
                self.aviso(e);
                None
            }
        }
    }

    /// Añade un mensaje; si ya hay uno sin leer, va en una línea nueva debajo.
    fn aviso(&mut self, texto: String) {
        self.mensaje = Some(match self.mensaje.take() {
            Some(previo) => format!("{previo}\n{texto}"),
            None => texto,
        });
    }

    /// Guarda la biblioteca de presets; si no se pudo leer, no la pisa.
    fn guardar_presets(&mut self) {
        if self.acabados_bloqueados {
            return;
        }
        if let Err(e) = guardar_acabados(&ruta_acabados(), &self.acabados_usuario) {
            self.aviso(e);
        }
    }

    fn reiniciar_vista(&mut self) {
        self.seleccion = None;
        self.arrastrando = false;
        self.camara = Camara::para_sala(&self.montaje.sala);
        self.vista_camara = None;
    }

    fn sincronizar_inventario(&mut self) {
        self.inv_filas = self.inventario.equipos.clone();
    }

    /// Marca, modelo y especificaciones de la luz colocada `codigo` (de techo o de piso).
    fn datos_luz(&self, codigo: &str) -> Option<(String, String, SpecsLuz)> {
        let datos = |marca: &String, modelo: &String, specs: &SpecsLuz| (marca.clone(), modelo.clone(), specs.clone());
        self.montaje
            .luces
            .iter()
            .find(|l| l.codigo == codigo)
            .map(|l| datos(&l.marca, &l.modelo, &l.specs))
            .or_else(|| self.montaje.luces_piso.iter().find(|l| l.codigo == codigo).map(|l| datos(&l.marca, &l.modelo, &l.specs)))
    }

    /// Lleva marca, modelo y especificaciones de la luz colocada `codigo` a su ficha del inventario, si existe.
    fn llevar_al_inventario(&mut self, codigo: &str) {
        let Some((marca, modelo, specs)) = self.datos_luz(codigo) else { return };
        let Some(ficha) = self.inventario.equipos.iter().find(|e| e.codigo == codigo).cloned() else { return };
        // Sin espacios, como las guarda el inventario: evita guardar en disco por nada.
        let nuevo = Equipo { marca: marca.trim().to_string(), modelo: modelo.trim().to_string(), specs: Some(specs), ..ficha.clone() };
        if nuevo == ficha {
            return;
        }
        let r = self.inventario.editar(codigo, nuevo);
        self.avisar(r);
        self.sincronizar_inventario();
    }

    /// Los campos de texto siguen a la selección: al cambiarla se recargan.
    fn sincronizar_edicion(&mut self) {
        if self.edicion_de == self.seleccion {
            return;
        }
        self.edicion_de = self.seleccion.clone();
        self.edicion = match &self.seleccion {
            Some(Sel::Carril(id)) | Some(Sel::Toma(id)) | Some(Sel::Objeto(id)) | Some(Sel::Camara(id)) => id.clone(),
            _ => String::new(),
        };
    }

    fn titulo(&mut self, ctx: &egui::Context) {
        let nombre = self
            .ruta
            .as_ref()
            .and_then(|r| r.file_name())
            .map_or_else(|| "Sin título".to_string(), |n| n.to_string_lossy().into_owned());
        let marca = if self.sucio { " *" } else { "" };
        let nuevo = format!("Sala de luces — {nombre}{marca}");
        // Solo se envía si cambia: enviar el comando cada fotograma fuerza repintados continuos.
        if nuevo != self.titulo_puesto {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(nuevo.clone()));
            self.titulo_puesto = nuevo;
        }
    }

    /// Cierre de la ventana con cambios sin guardar: se cancela y se pregunta.
    fn cierre(&mut self, ctx: &egui::Context) {
        if self.sucio && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pendiente = Some(Accion::Cerrar);
        }
    }

    /// Nuevo o Abrir: si hay cambios sin guardar, primero se pregunta.
    fn pedir(&mut self, ctx: &egui::Context, accion: Accion) {
        if self.sucio {
            self.pendiente = Some(accion);
        } else {
            self.ejecutar(ctx, accion);
        }
    }

    fn ejecutar(&mut self, ctx: &egui::Context, accion: Accion) {
        match accion {
            Accion::Nuevo => {
                self.montaje = Montaje::default();
                self.version += 1;
                self.ruta = None;
                self.sucio = false;
                self.reiniciar_vista();
            }
            Accion::Abrir => self.abrir(),
            Accion::Cerrar => {
                self.sucio = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn abrir(&mut self) {
        let Some(ruta) = rfd::FileDialog::new()
            .add_filter("Montaje de sala", &["json"])
            .pick_file()
        else {
            return;
        };
        match Montaje::cargar(&ruta) {
            Ok((m, avisos)) => {
                self.montaje = m;
                self.version += 1;
                self.ruta = Some(ruta);
                self.sucio = false;
                self.reiniciar_vista();
                self.incorporar_modelos();
                self.incorporar_acabados();
                self.incorporar_ies();
                if !avisos.is_empty() {
                    // El montaje reparado difiere del archivo: marcarlo para que se pida guardar.
                    self.marcar_cambio();
                    self.aviso(avisos.join("\n"));
                }
            }
            Err(e) => self.aviso(e),
        }
    }

    /// Montaje recién abierto: sus presets que no están en la biblioteca se añaden (si el nombre ya existe, gana la biblioteca).
    fn incorporar_acabados(&mut self) {
        let nuevos: Vec<(String, Acabado)> = self
            .montaje
            .acabados
            .iter()
            .filter_map(|(n, a)| nombre_preset_valido(n).map(|t| (t, *a)))
            .filter(|(n, _)| !self.acabados_usuario.contains_key(n))
            .collect();
        if nuevos.is_empty() {
            return;
        }
        self.acabados_usuario.extend(nuevos);
        self.guardar_presets();
    }

    /// Montaje recién abierto: sus perfiles IES que no están en la biblioteca se copian a ella.
    fn incorporar_ies(&mut self) {
        let nuevos: Vec<(String, String)> = self
            .montaje
            .fotometrias
            .iter()
            .filter(|(n, t)| !self.ies_biblioteca.contains_key(*n) && ies::leer(t).is_ok())
            .map(|(n, t)| (n.clone(), t.clone()))
            .collect();
        for (nombre, texto) in nuevos {
            match ies::guardar_en_biblioteca(&ruta_ies(), &nombre, &texto) {
                Ok(()) => {
                    self.ies_biblioteca.insert(nombre, texto);
                }
                Err(e) => self.aviso(e),
            }
        }
    }

    /// Importar .ies…: valida el archivo y lo copia a la biblioteca. Devuelve su nombre en ella, o None si se
    /// canceló o falló (el fallo va al aviso).
    fn importar_ies(&mut self) -> Option<String> {
        let ruta = rfd::FileDialog::new().add_filter("Perfil IES", &["ies"]).pick_file()?;
        let texto = match std::fs::read(&ruta) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(e) => {
                self.aviso(format!("No se pudo leer {}: {e}", ruta.display()));
                return None;
            }
        };
        if let Err(e) = ies::leer(&texto) {
            self.aviso(format!("{}: {e}", ruta.display()));
            return None;
        }
        // El mismo perfil ya en la biblioteca se reutiliza; uno distinto con el mismo nombre se renombra.
        if let Some(n) = self.ies_biblioteca.iter().find(|(_, t)| **t == texto).map(|(n, _)| n.clone()) {
            return Some(n);
        }
        let base = ruta.file_stem().map(|s| s.to_string_lossy().trim().to_string()).filter(|s| !s.is_empty());
        let usados: Vec<String> = self.ies_biblioteca.keys().cloned().collect();
        let nombre = nombre_libre(&base.unwrap_or_else(|| "perfil".to_string()), &usados);
        if let Err(e) = ies::guardar_en_biblioteca(&ruta_ies(), &nombre, &texto) {
            self.aviso(e);
            return None;
        }
        self.ies_biblioteca.insert(nombre.clone(), texto);
        Some(nombre)
    }

    /// Copia el texto del perfil de la luz desde la biblioteca a `montaje.fotometrias`, para que se guarde con el montaje.
    fn adoptar_ies(&mut self, s: &SpecsLuz) {
        if let Some(n) = &s.ies {
            if let Some(texto) = self.ies_biblioteca.get(n) {
                self.montaje.fotometrias.insert(n.clone(), texto.clone());
            }
        }
    }

    /// Montaje recién abierto: sus modelos nuevos van a la biblioteca y los que le faltan se toman de ella.
    fn incorporar_modelos(&mut self) {
        let nuevos: Vec<Malla> = self
            .montaje
            .modelos
            .values()
            .filter(|m| !self.biblioteca.iter().any(|b| b.nombre == m.nombre))
            .cloned()
            .collect();
        for m in nuevos {
            match guardar_en_biblioteca(&ruta_biblioteca(), &m) {
                Ok(()) => self.biblioteca.push(m),
                Err(e) => self.aviso(e),
            }
        }
        let mut faltan: Vec<String> = Vec::new();
        let necesarios: Vec<String> = self.montaje.objetos.iter().filter_map(|o| o.malla.clone()).collect();
        for nombre in necesarios {
            if self.montaje.modelos.contains_key(&nombre) {
                continue;
            }
            match self.biblioteca.iter().find(|b| b.nombre == nombre) {
                Some(b) => {
                    self.montaje.modelos.insert(nombre, b.clone());
                }
                None if !faltan.contains(&nombre) => faltan.push(nombre),
                None => {}
            }
        }
        if !faltan.is_empty() {
            self.aviso(format!("Modelos no encontrados (se dibujan como caja): {}", faltan.join(", ")));
        }
    }

    /// Importa un modelo a la biblioteca y lo pone en la sala.
    fn importar_modelo(&mut self) {
        let Some(ruta) = pedir_modelo() else {
            return;
        };
        let (mut m, avisos) = match importar(&ruta) {
            Ok(r) => r,
            Err(e) => {
                self.aviso(e);
                return;
            }
        };
        if !avisos.is_empty() {
            self.aviso(avisos.join("\n"));
        }
        // Un nombre ya usado en la biblioteca o en el montaje se numera; si no, el modelo viejo se dibujaría.
        let usados: Vec<String> = self
            .biblioteca
            .iter()
            .map(|b| b.nombre.clone())
            .chain(self.montaje.modelos.keys().cloned())
            .collect();
        m.nombre = nombre_libre(&m.nombre, &usados);
        if let Err(e) = guardar_en_biblioteca(&ruta_biblioteca(), &m) {
            self.aviso(e);
            return;
        }
        let nombre = self.montaje.colocar_malla(&m);
        self.biblioteca.push(m);
        self.seleccion = Some(Sel::Objeto(nombre));
        self.marcar_cambio();
    }

    /// Guarda en la ruta actual, o pregunta una si no hay. Devuelve true si se guardó.
    fn guardar(&mut self) -> bool {
        let ruta = match self.ruta.clone().or_else(pedir_ruta_guardar) {
            Some(r) => r,
            None => return false,
        };
        self.guardar_en(ruta)
    }

    fn guardar_en(&mut self, ruta: PathBuf) -> bool {
        match self.montaje.guardar(&ruta) {
            Ok(()) => {
                self.ruta = Some(ruta);
                self.sucio = false;
                true
            }
            Err(e) => {
                self.aviso(e);
                false
            }
        }
    }

    fn carril_para_luz(&self) -> Option<String> {
        match &self.seleccion {
            Some(Sel::Carril(c)) => Some(c.clone()),
            _ => self.montaje.carriles.first().map(|c| c.id.clone()),
        }
    }

    fn colocar_luz(&mut self, e: &Equipo) {
        let Some(carril) = self.carril_para_luz() else {
            self.aviso("No hay carriles: añade uno antes".to_string());
            return;
        };
        let r = self.montaje.colocar_luz(e, &carril);
        if self.avisar(r).is_some() {
            self.seleccion = Some(Sel::Luz(e.codigo.clone()));
            self.marcar_cambio();
            self.ver_selector = false;
        }
    }

    /// Pone una luz de piso en el centro de la sala y la selecciona para editarla.
    fn colocar_luz_piso(&mut self, e: &Equipo) {
        let r = self.montaje.colocar_luz_piso(e);
        if self.avisar(r).is_some() {
            self.seleccion = Some(Sel::LuzPiso(e.codigo.trim().to_string()));
            self.marcar_cambio();
            self.ver_selector_piso = false;
        }
    }

    fn renombrar_carril(&mut self, viejo: &str) {
        let nuevo = self.edicion.trim().to_string();
        if nuevo != viejo {
            let r = self.montaje.renombrar_carril(viejo, &nuevo);
            if self.avisar(r).is_some() {
                self.marcar_cambio();
                self.seleccion = Some(Sel::Carril(nuevo));
                return;
            }
        }
        self.edicion = viejo.to_string();
    }

    fn renombrar_toma(&mut self, viejo: &str) {
        let nuevo = self.edicion.trim().to_string();
        if nuevo != viejo {
            let r = self.montaje.renombrar_toma(viejo, &nuevo);
            if self.avisar(r).is_some() {
                self.marcar_cambio();
                self.seleccion = Some(Sel::Toma(nuevo));
                return;
            }
        }
        self.edicion = viejo.to_string();
    }

    fn renombrar_camara(&mut self, viejo: &str) {
        let nuevo = self.edicion.trim().to_string();
        if nuevo != viejo {
            let r = self.montaje.renombrar_camara(viejo, &nuevo);
            if self.avisar(r).is_some() {
                self.marcar_cambio();
                if self.vista_camara.as_deref() == Some(viejo) {
                    self.vista_camara = Some(nuevo.clone());
                }
                self.seleccion = Some(Sel::Camara(nuevo));
                return;
            }
        }
        self.edicion = viejo.to_string();
    }

    /// La cámara de escena de la vista de cámara, si la hay y existe.
    fn camara_vista(&self) -> Option<&CamaraEscena> {
        let nombre = self.vista_camara.as_deref()?;
        self.montaje.camaras.iter().find(|c| c.nombre == nombre)
    }

    /// Cámara a la que lleva la tecla 0: la seleccionada o, si no hay ninguna, la primera.
    fn camara_de_atajo(&self) -> Option<String> {
        match &self.seleccion {
            Some(Sel::Camara(n)) if self.montaje.camaras.iter().any(|c| &c.nombre == n) => Some(n.clone()),
            _ => self.montaje.camaras.first().map(|c| c.nombre.clone()),
        }
    }

    /// Entra en la vista de cámara o sale de ella (tecla 0 y botón).
    fn alternar_vista_camara(&mut self) {
        self.vista_camara = if self.vista_camara.is_some() { None } else { self.camara_de_atajo() };
    }

    /// Ctrl+N, Ctrl+O, Ctrl+S y Ctrl+Mayús+S, igual que los botones. No actúan con un campo de texto
    /// enfocado ni con un modal abierto. Ctrl+Mayús+S va antes que Ctrl+S: `consume_key` ignora el Mayús extra.
    fn atajos_archivo(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || self.pendiente.is_some() || self.mensaje.is_some() {
            return;
        }
        let pulsada = |mods: egui::Modifiers, tecla: egui::Key| ctx.input_mut(|i| i.consume_key(mods, tecla));
        let ctrl = egui::Modifiers::COMMAND;
        if pulsada(ctrl | egui::Modifiers::SHIFT, egui::Key::S) {
            if let Some(ruta) = pedir_ruta_guardar() {
                self.guardar_en(ruta);
            }
        } else if pulsada(ctrl, egui::Key::S) {
            self.guardar();
        } else if pulsada(ctrl, egui::Key::N) {
            self.pedir(ctx, Accion::Nuevo);
        } else if pulsada(ctrl, egui::Key::O) {
            self.pedir(ctx, Accion::Abrir);
        }
    }

    /// Tecla 0 entra o sale de la vista de cámara; Escape sale. No actúan con un campo de texto enfocado.
    fn atajos_camara(&mut self, ctx: &egui::Context) {
        if self.camara_vista().is_none() {
            self.vista_camara = None;
        }
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let (cero, esc) = ctx.input(|i| (i.key_pressed(egui::Key::Num0), i.key_pressed(egui::Key::Escape)));
        if cero {
            self.alternar_vista_camara();
        } else if esc {
            self.vista_camara = None;
        }
    }

    /// Cámara con la que se pinta, elige y arrastra, y el rectángulo donde cae la escena.
    /// En vista de cámara, la de escena encuadrada a su formato; si no, la libre sobre todo `rect`.
    fn camara_y_rect(&self, rect: egui::Rect) -> (Camara, egui::Rect) {
        match self.camara_vista() {
            Some(c) => {
                let pos = V3::new(c.x, c.altura, c.z);
                (Camara::desde_escena(pos, V3::from_arr(c.direccion()), c.fov_y()), encuadre(rect, c.aspecto))
            }
            None => (self.camara, rect),
        }
    }

    /// Primitivas de la escena; en vista de cámara, sin las de esa cámara (no se ve a sí misma).
    fn primitivas(&self) -> Vec<Prim> {
        let propia = self.vista_camara.clone().map(Sel::Camara);
        // Seleccionada la propia cámara, sus cotas tampoco se ven (no llevan `sel`, no las quita el filtro).
        let sel = self.seleccion.as_ref().filter(|s| Some(*s) != propia.as_ref());
        let mut prims = construir(&self.montaje, sel);
        if let Some(propia) = propia {
            prims.retain(|p| p.sel.as_ref() != Some(&propia));
        }
        prims
    }

    /// Cambia la cámara de la vista de cámara con `f`, la acota y marca el cambio.
    fn editar_camara_vista(&mut self, f: impl FnOnce(&mut CamaraEscena)) {
        let Some(nombre) = self.vista_camara.clone() else { return };
        let Some(c) = self.montaje.camaras.iter_mut().find(|c| c.nombre == nombre) else { return };
        f(c);
        c.limitar();
        self.marcar_cambio();
    }

    fn renombrar_objeto(&mut self, viejo: &str) {
        let nuevo = self.edicion.trim().to_string();
        if nuevo != viejo {
            let r = self.montaje.renombrar_objeto(viejo, &nuevo);
            if self.avisar(r).is_some() {
                self.marcar_cambio();
                self.seleccion = Some(Sel::Objeto(nuevo));
                return;
            }
        }
        self.edicion = viejo.to_string();
    }

    fn barra(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            if ui.button("Nuevo").on_hover_text("Ctrl+N").clicked() {
                self.pedir(ctx, Accion::Nuevo);
            }
            if ui.button("Abrir…").on_hover_text("Ctrl+O").clicked() {
                self.pedir(ctx, Accion::Abrir);
            }
            if ui.button("Guardar").on_hover_text("Ctrl+S").clicked() {
                self.guardar();
            }
            if ui.button("Guardar como…").on_hover_text("Ctrl+Mayús+S").clicked() {
                if let Some(ruta) = pedir_ruta_guardar() {
                    self.guardar_en(ruta);
                }
            }
            ui.separator();
            if ui.add(egui::Button::new("Inventario").selected(self.ver_inventario)).clicked() {
                self.ver_inventario = !self.ver_inventario;
                if self.ver_inventario {
                    self.sincronizar_inventario();
                }
            }
            if ui.add(egui::Button::new("Configuración").selected(self.ver_config)).clicked() {
                self.ver_config = !self.ver_config;
            }
            if ui.add(egui::Button::new("Superficies").selected(self.ver_superficies)).clicked() {
                self.ver_superficies = !self.ver_superficies;
            }
            if self.render.is_some() {
                ui.separator();
                ui.selectable_value(&mut self.modo, ModoVista::Realista, "Realista");
                ui.selectable_value(&mut self.modo, ModoVista::MapaLux, "Mapa de lux");
                ui.selectable_value(&mut self.modo, ModoVista::Esquema, "Esquema");
                ui.toggle_value(&mut self.humo, "Haz").on_hover_text("Muestra el haz de las luces (humo)");
            }
        });
    }

    /// Triángulos de la escena (sombras y lux), rehechos solo si cambió el montaje.
    fn asegurar_triangulos(&mut self) {
        if self.tris.as_ref().is_none_or(|(v, _)| *v != self.version) {
            self.tris = Some((self.version, triangulos(&self.montaje)));
        }
    }

    /// EV100 de la cámara: el elegido a mano, o el automático (luminancia media de 9 puntos del suelo
    /// alrededor del objetivo), que solo se recalcula al cambiar la escena o el objetivo.
    fn ev100_actual(&mut self) -> f32 {
        // Para pruebas, como SALA_PRESET: fija la exposición sin tocar la interfaz (p. ej. para ver las sombras).
        if let Some(ev) = std::env::var("SALA_EV").ok().and_then(|s| s.parse().ok()) {
            return ev;
        }
        if !self.expo_auto {
            return ev100(self.iso, self.diafragma, self.obturacion);
        }
        let t = self.camara.objetivo;
        let clave = (self.version, [t.x.to_bits(), t.y.to_bits(), t.z.to_bits()]);
        if self.clave_expo != Some(clave) {
            self.clave_expo = Some(clave);
            self.asegurar_triangulos();
            let m = &self.montaje;
            let tris = self.tris.as_ref().map_or(&[][..], |(_, v)| v.as_slice());
            let s = &m.sala;
            let mut suma = 0.0;
            for i in -1..=1 {
                for j in -1..=1 {
                    let x = (t.x + i as f32 * 0.75).clamp(0.0, s.ancho);
                    let z = (t.z + j as f32 * 0.75).clamp(0.0, s.largo);
                    suma += lux_en(m, [x, 0.0, z], [0.0, 1.0, 0.0], tris, None);
                }
            }
            let lum = (suma / 9.0 * 0.18 / std::f32::consts::PI).max(1e-3);
            self.ev_auto = ev100_auto(lum);
        }
        self.ev_auto
    }

    /// Lux bajo el cursor: iluminancia en la superficie que ve la cámara en ese punto.
    /// Solo se recalcula cuando cambian el puntero, la cámara o la escena.
    fn lux_bajo_cursor(&mut self, cam: &Camara, rect: egui::Rect, raton: Option<egui::Pos2>) -> Option<(egui::Pos2, f32)> {
        let Some(p) = raton else {
            self.lux_cursor = None;
            return None;
        };
        // Con las sondas ya leídas, la luz rebotada es la de la rejilla; su versión entra en la clave.
        let (sondas, ver) =
            self.render.as_ref().and_then(|r| r.lock().ok()).map_or((None, 0), |r| (r.sondas_cpu(), r.sondas_gen()));
        let clave: LuxClave =
            (self.version, ver, p, [cam.yaw, cam.pitch, cam.distancia, cam.objetivo.x, cam.objetivo.y, cam.objetivo.z].map(f32::to_bits));
        if self.lux_cursor.as_ref().is_none_or(|(k, _)| *k != clave) {
            self.asegurar_triangulos();
            let (o, d) = cam.rayo(p, rect);
            let tris = self.tris.as_ref().map_or(&[][..], |(_, v)| v.as_slice());
            let lux = sobre_superficie([o.x, o.y, o.z], [d.x, d.y, d.z], tris, &self.montaje.sala).map(|(pt, n)| {
                let ind = sondas.as_ref().map(|s| s.irradiancia(pt, n));
                lux_en(&self.montaje, pt, n, tris, ind)
            });
            self.lux_cursor = Some((clave, lux));
        }
        let (_, lux) = self.lux_cursor.as_ref()?;
        lux.map(|v| (p, v))
    }

    /// Pinta la escena en GPU dentro de `rect` y luego las líneas, textos y contornos encima.
    /// `cam` y `vista` (la cámara de escena, si se mira por ella) dan la vista, la exposición, el balance y la lente.
    fn pintar_gpu(
        &mut self,
        pintor: &egui::Painter,
        rect: egui::Rect,
        prims: &[Prim],
        ctx: &egui::Context,
        cam: Camara,
        vista: Option<CamaraEscena>,
    ) {
        let (Some(render), Some(gl)) = (self.render.clone(), self.gl.clone()) else { return };
        let fs = fuentes(&self.montaje);
        if self.subida != Some(self.version) {
            let malla = malla_gpu(&self.montaje);
            let sombras = malla_sombras(&self.montaje);
            let s = &self.montaje.sala;
            if let Ok(mut r) = render.lock() {
                r.subir_escena(&gl, &malla, &sombras, [s.ancho, s.alto_techo, s.largo], &fs);
            }
            self.subida = Some(self.version);
        }
        let ctx = ctx.clone();
        let indirecta = indirecta_lux(&self.montaje, &fs);
        let color_indirecta = color_medio(&fs);
        let (exposicion, balance, lente, foco) = match &vista {
            Some(c) => (exposicion(ev100(c.iso, c.diafragma, c.obturacion)), balance_blancos(c.balance_k), c.radio_lente(), c.foco_m),
            None => (exposicion(self.ev100_actual()), [1.0; 3], 0.0, 1.0),
        };
        let aspecto = rect.width() / rect.height().max(1.0);
        let o = cam.ojo();
        let params = Arc::new(ParamsFrame {
            vista: matriz_vista(&cam),
            proy: matriz_proyeccion(&cam, aspecto),
            ojo: [o.x, o.y, o.z],
            viewport_px: [0; 4],
            fuentes: fs,
            indirecta_lux: indirecta,
            color_indirecta,
            exposicion,
            balance,
            lente,
            foco,
            modo: self.modo,
            humo: self.humo,
        });
        let cb = egui::PaintCallback {
            rect,
            callback: Arc::new(egui_glow::CallbackFn::new(move |info, painter| {
                let vp = info.viewport_in_pixels();
                let mut p = (*params).clone();
                p.viewport_px = [vp.left_px, vp.from_bottom_px, vp.width_px, vp.height_px];
                if let Ok(mut r) = render.lock() {
                    let ver = r.sondas_gen();
                    r.pintar(painter.gl(), &p);
                    // Cada muestra del trazado es un fotograma: se pide el siguiente hasta llegar al máximo.
                    // Si las sondas acaban de leerse, el cursor de lux necesita un fotograma más para usarlas.
                    if r.pide_repintado() || r.sondas_gen() != ver {
                        ctx.request_repaint();
                    }
                }
            })),
        };
        pintor.add(egui::Shape::Callback(cb));
        dibujar_superpuesto(pintor, &cam, rect, prims);
    }

    fn panel_izquierdo(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::CollapsingHeader::new(egui::RichText::new("Sala").strong())
                .id_salt("sec_sala")
                .default_open(true)
                .show(ui, |ui| {
                    let mut cambio = false;
                    {
                        let sala = &mut self.montaje.sala;
                        egui::Grid::new("sala").show(ui, |ui| {
                            ui.label("Ancho (X)");
                            cambio |= numero(ui, &mut sala.ancho, 1.0..=100.0);
                            ui.end_row();
                            ui.label("Largo (Z)");
                            cambio |= numero(ui, &mut sala.largo, 1.0..=100.0);
                            ui.end_row();
                            ui.label("Altura del techo");
                            cambio |= numero(ui, &mut sala.alto_techo, 1.0..=100.0);
                            ui.end_row();
                            ui.label("Altura de carriles");
                            cambio |= numero(ui, &mut sala.altura_carriles, 0.5..=100.0);
                            ui.end_row();
                        });
                    }
                    if cambio {
                        self.montaje.ajustar_a_sala();
                        self.marcar_cambio();
                    }
                });

            egui::CollapsingHeader::new(egui::RichText::new("Exposición de la vista").strong())
                .id_salt("sec_exposicion")
                .default_open(true)
                .show(ui, |ui| {
                    ui.checkbox(&mut self.expo_auto, "Exposición automática");
                    if !self.expo_auto {
                        egui::Grid::new("camara").show(ui, |ui| {
                            ui.label("ISO");
                            combo_valores(ui, "iso", &mut self.iso, &ISOS, |v| format!("{v}"));
                            ui.end_row();
                            ui.label("Diafragma");
                            combo_valores(ui, "diafragma", &mut self.diafragma, &DIAFRAGMAS, |v| format!("f/{v}"));
                            ui.end_row();
                            ui.label("Obturación");
                            combo_valores(ui, "obturacion", &mut self.obturacion, &OBTURACIONES, etiqueta_obturacion);
                            ui.end_row();
                        });
                    }
                    ui.label(format!("EV100: {:.1}", self.ev100_actual()));
                });

            let titulo_carriles = format!("Carriles ({})", self.montaje.carriles.len());
            egui::CollapsingHeader::new(egui::RichText::new(titulo_carriles.as_str()).strong())
                .id_salt("sec_carriles")
                .default_open(true)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Añadir").clicked() {
                            let id = self.montaje.anadir_carril();
                            self.seleccion = Some(Sel::Carril(id));
                            self.marcar_cambio();
                        }
                        let hay = matches!(self.seleccion, Some(Sel::Carril(_)));
                        if ui.add_enabled(hay, egui::Button::new("Borrar")).clicked() {
                            if let Some(Sel::Carril(id)) = self.seleccion.clone() {
                                let r = self.montaje.borrar_carril(&id);
                                if self.avisar(r).is_some() {
                                    self.seleccion = None;
                                    self.marcar_cambio();
                                }
                            }
                        }
                });
                let mut clic = None;
                for c in &self.montaje.carriles {
                    let sel = Sel::Carril(c.id.clone());
                    if ui.selectable_label(self.seleccion.as_ref() == Some(&sel), c.id.as_str()).clicked() {
                        clic = Some(sel);
                    }
                }
                if clic.is_some() {
                    self.seleccion = clic;
                }
                });

            let titulo_tomas = format!("Tomas ({})", self.montaje.tomas.len());
            egui::CollapsingHeader::new(egui::RichText::new(titulo_tomas.as_str()).strong())
                .id_salt("sec_tomas")
                .default_open(true)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Añadir").clicked() {
                            let codigo = self.montaje.anadir_toma();
                            self.seleccion = Some(Sel::Toma(codigo));
                            self.marcar_cambio();
                        }
                        let hay = matches!(self.seleccion, Some(Sel::Toma(_)));
                        if ui.add_enabled(hay, egui::Button::new("Borrar")).clicked() {
                            if let Some(Sel::Toma(codigo)) = self.seleccion.clone() {
                                self.montaje.borrar_toma(&codigo);
                                self.seleccion = None;
                                self.marcar_cambio();
                            }
                        }
                });
                let mut clic = None;
                for t in &self.montaje.tomas {
                    let sel = Sel::Toma(t.codigo.clone());
                    let texto = if t.nivel == NivelToma::Suelo { format!("{} (suelo)", t.codigo) } else { t.codigo.clone() };
                    if ui.selectable_label(self.seleccion.as_ref() == Some(&sel), texto).clicked() {
                        clic = Some(sel);
                    }
                }
                if clic.is_some() {
                    self.seleccion = clic;
                }
                });

            let titulo_luces = format!("Luces ({})", self.montaje.luces.len());
            egui::CollapsingHeader::new(egui::RichText::new(titulo_luces.as_str()).strong())
                .id_salt("sec_luces")
                .default_open(true)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Añadir").clicked() {
                            if self.montaje.carriles.is_empty() {
                                self.aviso("No hay carriles: añade uno antes".to_string());
                            } else {
                                self.ver_selector = true;
                            }
                        }
                        let hay = matches!(self.seleccion, Some(Sel::Luz(_)));
                        if ui.add_enabled(hay, egui::Button::new("Borrar")).clicked() {
                            if let Some(Sel::Luz(codigo)) = self.seleccion.clone() {
                                self.montaje.quitar_luz(&codigo);
                                self.seleccion = None;
                                self.marcar_cambio();
                            }
                        }
                });
                let mut clic = None;
                for l in &self.montaje.luces {
                    let sel = Sel::Luz(l.codigo.clone());
                    let texto = match &l.toma {
                        Some(t) => format!("{} ({}) → {t}", l.codigo, l.carril),
                        None => format!("{} ({})", l.codigo, l.carril),
                    };
                    if ui.selectable_label(self.seleccion.as_ref() == Some(&sel), texto).clicked() {
                        clic = Some(sel);
                    }
                }
                if clic.is_some() {
                    self.seleccion = clic;
                }
                });

            let titulo_luces_piso = format!("Luces de piso ({})", self.montaje.luces_piso.len());
            egui::CollapsingHeader::new(egui::RichText::new(titulo_luces_piso.as_str()).strong())
                .id_salt("sec_luces_piso")
                .default_open(true)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Añadir").clicked() {
                            self.ver_selector_piso = true;
                        }
                        let hay = matches!(self.seleccion, Some(Sel::LuzPiso(_)));
                        if ui.add_enabled(hay, egui::Button::new("Borrar")).clicked() {
                            if let Some(Sel::LuzPiso(codigo)) = self.seleccion.clone() {
                                self.montaje.quitar_luz_piso(&codigo);
                                self.seleccion = None;
                                self.marcar_cambio();
                            }
                        }
                });
                let mut clic = None;
                for l in &self.montaje.luces_piso {
                    let sel = Sel::LuzPiso(l.codigo.clone());
                    let texto = match &l.toma {
                        Some(t) => format!("{} → {t}", l.codigo),
                        None => l.codigo.clone(),
                    };
                    if ui.selectable_label(self.seleccion.as_ref() == Some(&sel), texto).clicked() {
                        clic = Some(sel);
                    }
                }
                if clic.is_some() {
                    self.seleccion = clic;
                }
                });

            let titulo_objetos = format!("Objetos ({})", self.montaje.objetos.len());
            egui::CollapsingHeader::new(egui::RichText::new(titulo_objetos.as_str()).strong())
                .id_salt("sec_objetos")
                .default_open(true)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Añadir").clicked() {
                            self.ver_objetos = true;
                        }
                        let hay = matches!(self.seleccion, Some(Sel::Objeto(_)));
                        if ui.add_enabled(hay, egui::Button::new("Borrar")).clicked() {
                            if let Some(Sel::Objeto(nombre)) = self.seleccion.clone() {
                                self.montaje.borrar_objeto(&nombre);
                                self.seleccion = None;
                                self.marcar_cambio();
                            }
                        }
                });
                if ui.button("Importar modelo…").clicked() {
                    self.importar_modelo();
                }
                let mut clic = None;
                for o in &self.montaje.objetos {
                    let sel = Sel::Objeto(o.nombre.clone());
                    let texto = format!("{} ({})", o.nombre, o.malla.as_deref().unwrap_or(o.tipo.nombre()));
                    if ui.selectable_label(self.seleccion.as_ref() == Some(&sel), texto).clicked() {
                        clic = Some(sel);
                    }
                }
                if clic.is_some() {
                    self.seleccion = clic;
                }
                });

            let titulo_camaras = format!("Cámaras ({})", self.montaje.camaras.len());
            egui::CollapsingHeader::new(egui::RichText::new(titulo_camaras.as_str()).strong())
                .id_salt("sec_camaras")
                .default_open(true)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Añadir").clicked() {
                            let nombre = self.montaje.anadir_camara();
                            self.seleccion = Some(Sel::Camara(nombre));
                            self.marcar_cambio();
                        }
                        let hay = matches!(self.seleccion, Some(Sel::Camara(_)));
                        if ui.add_enabled(hay, egui::Button::new("Borrar")).clicked() {
                            if let Some(Sel::Camara(nombre)) = self.seleccion.clone() {
                                self.montaje.borrar_camara(&nombre);
                                self.seleccion = None;
                                self.marcar_cambio();
                            }
                        }
                });
                let texto = if self.vista_camara.is_some() { "Salir de la cámara (0)" } else { "Ver por la cámara (0)" };
                let habilitado = self.vista_camara.is_some() || self.camara_de_atajo().is_some();
                if ui.add_enabled(habilitado, egui::Button::new(texto)).clicked() {
                    self.alternar_vista_camara();
                }
                let mut clic = None;
                for c in &self.montaje.camaras {
                    let sel = Sel::Camara(c.nombre.clone());
                    if ui.selectable_label(self.seleccion.as_ref() == Some(&sel), c.nombre.as_str()).clicked() {
                        clic = Some(sel);
                    }
                }
                if clic.is_some() {
                    self.seleccion = clic;
                }
                });
        });
    }

    fn panel_derecho(&mut self, ui: &mut egui::Ui) {
        // Los textos de id/código siguen a la selección (se recargan al cambiarla).
        let Some(sel) = self.seleccion.clone() else {
            ui.heading("Propiedades");
            ui.label(egui::RichText::new("Nada seleccionado. Haz clic en un elemento de la vista 3D o de las listas para editarlo.").weak());
            ui.add_space(8.0);
            let m = &self.montaje;
            ui.label(egui::RichText::new(format!("Carriles: {}", m.carriles.len())).weak());
            ui.label(egui::RichText::new(format!("Tomas: {}", m.tomas.len())).weak());
            ui.label(egui::RichText::new(format!("Luces: {} techo + {} piso", m.luces.len(), m.luces_piso.len())).weak());
            ui.label(egui::RichText::new(format!("Objetos: {}", m.objetos.len())).weak());
            ui.label(egui::RichText::new(format!("Cámaras: {}", m.camaras.len())).weak());
            return;
        };
        egui::ScrollArea::vertical().show(ui, |ui| match sel {
            Sel::Carril(id) => self.editor_carril(ui, &id),
            Sel::Toma(codigo) => self.editor_toma(ui, &codigo),
            Sel::Luz(codigo) => self.editor_luz(ui, &codigo),
            Sel::LuzPiso(codigo) => self.editor_luz_piso(ui, &codigo),
            Sel::Objeto(nombre) => self.editor_objeto(ui, &nombre),
            Sel::Camara(nombre) => self.editor_camara(ui, &nombre),
        });
    }

    fn editor_carril(&mut self, ui: &mut egui::Ui, id: &str) {
        ui.heading("Carril");
        let Some(i) = self.montaje.carriles.iter().position(|c| c.id == id) else {
            ui.label("Este carril ya no existe");
            return;
        };
        let mut cambio = false;
        ui.horizontal(|ui| {
            ui.label("Id");
            let r = ui.add(egui::TextEdit::singleline(&mut self.edicion).desired_width(120.0));
            if r.lost_focus() {
                self.renombrar_carril(id);
            }
        });
        let ancho = self.montaje.sala.ancho;
        ui.horizontal(|ui| {
            ui.label("Posición X");
            cambio |= numero(ui, &mut self.montaje.carriles[i].x, 0.0..=ancho);
        });
        if cambio {
            self.montaje.ajustar_a_sala();
            self.marcar_cambio();
        }
    }

    fn editor_toma(&mut self, ui: &mut egui::Ui, codigo: &str) {
        ui.heading("Toma");
        let Some(i) = self.montaje.tomas.iter().position(|t| t.codigo == codigo) else {
            ui.label("Esta toma ya no existe");
            return;
        };
        let mut cambio = false;
        ui.horizontal(|ui| {
            ui.label("Código");
            let r = ui.add(egui::TextEdit::singleline(&mut self.edicion).desired_width(120.0));
            if r.lost_focus() {
                self.renombrar_toma(codigo);
            }
        });
        let mut pared = self.montaje.tomas[i].pared;
        ui.horizontal(|ui| {
            ui.label("Pared");
            egui::ComboBox::from_id_salt("pared_toma")
                .selected_text(pared.nombre())
                .show_ui(ui, |ui| {
                    for p in Pared::TODAS {
                        ui.selectable_value(&mut pared, p, p.nombre());
                    }
                });
        });
        if pared != self.montaje.tomas[i].pared {
            self.montaje.tomas[i].pared = pared;
            cambio = true;
        }
        let mut nivel = self.montaje.tomas[i].nivel;
        ui.horizontal(|ui| {
            ui.label("Nivel");
            egui::ComboBox::from_id_salt("nivel_toma").selected_text(etiqueta_nivel(nivel)).show_ui(ui, |ui| {
                for n in [NivelToma::Carriles, NivelToma::Suelo] {
                    ui.selectable_value(&mut nivel, n, etiqueta_nivel(n));
                }
            });
        });
        if nivel != self.montaje.tomas[i].nivel {
            self.montaje.cambiar_nivel_toma(codigo, nivel);
            cambio = true;
        }
        let largo = self.montaje.tomas[i].pared.largo(&self.montaje.sala);
        ui.horizontal(|ui| {
            ui.label("Posición");
            cambio |= numero(ui, &mut self.montaje.tomas[i].pos, 0.0..=largo);
        });
        if cambio {
            self.montaje.ajustar_a_sala();
            self.marcar_cambio();
        }
    }

    fn editor_luz(&mut self, ui: &mut egui::Ui, codigo: &str) {
        ui.heading("Luz");
        let Some(i) = self.montaje.luces.iter().position(|l| l.codigo == codigo) else {
            ui.label("Esta luz ya no existe");
            return;
        };
        let antes = self.datos_luz(codigo);
        let mut cambio = false;
        ui.label(format!("Código: {}", self.montaje.luces[i].codigo));
        ui.horizontal(|ui| {
            ui.label("Marca");
            cambio |= ui.text_edit_singleline(&mut self.montaje.luces[i].marca).changed();
        });
        ui.horizontal(|ui| {
            ui.label("Modelo");
            cambio |= ui.text_edit_singleline(&mut self.montaje.luces[i].modelo).changed();
        });
        let mut specs = self.montaje.luces[i].specs.clone();
        let perfiles: Vec<String> = self.ies_biblioteca.keys().cloned().collect();
        let mut importar = false;
        let mut cambio_specs = editar_specs(ui, codigo, &mut specs, false, &perfiles, &mut importar);
        if importar {
            if let Some(n) = self.importar_ies() {
                specs.ies = Some(n);
                cambio_specs = true;
            }
        }
        if cambio_specs {
            self.adoptar_ies(&specs);
            self.montaje.luces[i].specs = specs.clone();
            self.montaje.luces[i].ajustes.limitar(&specs);
            cambio = true;
        }

        let mut carril = self.montaje.luces[i].carril.clone();
        ui.horizontal(|ui| {
            ui.label("Carril");
            egui::ComboBox::from_id_salt("carril_luz")
                .selected_text(carril.as_str())
                .show_ui(ui, |ui| {
                    for c in &self.montaje.carriles {
                        ui.selectable_value(&mut carril, c.id.clone(), c.id.as_str());
                    }
                });
        });
        if carril != self.montaje.luces[i].carril {
            self.montaje.luces[i].carril = carril;
            cambio = true;
        }

        let largo = self.montaje.sala.largo;
        ui.horizontal(|ui| {
            ui.label("Z");
            cambio |= numero(ui, &mut self.montaje.luces[i].z, 0.0..=largo);
        });

        let codigo = self.montaje.luces[i].codigo.clone();
        let actual = self.montaje.luces[i].toma.clone();
        let mut elegida = actual.clone();
        let libres = self.montaje.tomas_libres_para(&codigo);
        let texto = actual.clone().unwrap_or_else(|| "(sin toma)".to_string());
        ui.horizontal(|ui| {
            ui.label("Toma");
            egui::ComboBox::from_id_salt("toma_luz")
                .selected_text(texto.as_str())
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut elegida, None, "(sin toma)");
                    for t in &libres {
                        ui.selectable_value(&mut elegida, Some(t.clone()), t.as_str());
                    }
                });
        });
        if elegida != actual {
            let r = self.montaje.conectar(&codigo, elegida.as_deref());
            if self.avisar(r).is_some() {
                cambio = true;
            }
        }

        let mut s = self.montaje.luces[i].specs.clone();
        let c = centro_luz(&self.montaje, &self.montaje.luces[i]);
        let mut a = self.montaje.luces[i].ajustes;
        let destinos = destinos_apuntar(&self.montaje);
        let ies = ies_de(&self.montaje, &s);
        if controles_ajustes(ui, &mut s, &mut a, [c.x, c.y, c.z], &destinos, &codigo, ies.as_ref()) {
            self.montaje.luces[i].specs = s;
            self.montaje.luces[i].ajustes = a;
            cambio = true;
        }

        if cambio {
            self.montaje.ajustar_a_sala();
            self.marcar_cambio();
        }
        if self.datos_luz(&codigo) != antes {
            self.llevar_al_inventario(&codigo);
        }
    }

    /// Editor de una luz de piso: posición, altura, toma de suelo y ajustes.
    fn editor_luz_piso(&mut self, ui: &mut egui::Ui, codigo: &str) {
        ui.heading("Luz de piso");
        let Some(i) = self.montaje.luces_piso.iter().position(|l| l.codigo == codigo) else {
            ui.label("Esta luz de piso ya no existe");
            return;
        };
        let antes = self.datos_luz(codigo);
        let mut cambio = false;
        ui.label(format!("Código: {}", self.montaje.luces_piso[i].codigo));
        ui.horizontal(|ui| {
            ui.label("Marca");
            cambio |= ui.text_edit_singleline(&mut self.montaje.luces_piso[i].marca).changed();
        });
        ui.horizontal(|ui| {
            ui.label("Modelo");
            cambio |= ui.text_edit_singleline(&mut self.montaje.luces_piso[i].modelo).changed();
        });
        let mut specs = self.montaje.luces_piso[i].specs.clone();
        let perfiles: Vec<String> = self.ies_biblioteca.keys().cloned().collect();
        let mut importar = false;
        let mut cambio_specs = editar_specs(ui, codigo, &mut specs, true, &perfiles, &mut importar);
        if importar {
            if let Some(n) = self.importar_ies() {
                specs.ies = Some(n);
                cambio_specs = true;
            }
        }
        if cambio_specs {
            self.adoptar_ies(&specs);
            self.montaje.luces_piso[i].specs = specs.clone();
            self.montaje.luces_piso[i].ajustes.limitar(&specs);
            cambio = true;
        }
        let (ancho, largo) = (self.montaje.sala.ancho, self.montaje.sala.largo);
        let l = &mut self.montaje.luces_piso[i];
        ui.horizontal(|ui| {
            ui.label("X");
            cambio |= numero(ui, &mut l.x, 0.0..=ancho);
        });
        ui.horizontal(|ui| {
            ui.label("Z");
            cambio |= numero(ui, &mut l.z, 0.0..=largo);
        });
        ui.horizontal(|ui| {
            ui.label("Altura");
            cambio |= numero(ui, &mut l.altura, ALTURA_PISO_MIN..=ALTURA_PISO_MAX);
        });

        let actual = self.montaje.luces_piso[i].toma.clone();
        let mut elegida = actual.clone();
        let libres = self.montaje.tomas_libres_para(codigo);
        let texto = actual.clone().unwrap_or_else(|| "(sin toma)".to_string());
        ui.horizontal(|ui| {
            ui.label("Toma");
            egui::ComboBox::from_id_salt("toma_luz_piso").selected_text(texto.as_str()).show_ui(ui, |ui| {
                ui.selectable_value(&mut elegida, None, "(sin toma)");
                for t in &libres {
                    ui.selectable_value(&mut elegida, Some(t.clone()), t.as_str());
                }
            });
        });
        if elegida != actual {
            let r = self.montaje.conectar_piso(codigo, elegida.as_deref());
            if self.avisar(r).is_some() {
                cambio = true;
            }
        }

        let mut s = self.montaje.luces_piso[i].specs.clone();
        let p = &self.montaje.luces_piso[i];
        let origen = [p.x, p.altura, p.z];
        let mut a = p.ajustes;
        let destinos = destinos_apuntar(&self.montaje);
        let ies = ies_de(&self.montaje, &s);
        if controles_ajustes(ui, &mut s, &mut a, origen, &destinos, codigo, ies.as_ref()) {
            self.montaje.luces_piso[i].specs = s;
            self.montaje.luces_piso[i].ajustes = a;
            cambio = true;
        }

        if cambio {
            self.montaje.ajustar_a_sala();
            self.marcar_cambio();
        }
        if self.datos_luz(codigo) != antes {
            self.llevar_al_inventario(codigo);
        }
    }

    fn editor_objeto(&mut self, ui: &mut egui::Ui, nombre: &str) {
        ui.heading("Objeto");
        let Some(i) = self.montaje.objetos.iter().position(|o| o.nombre == nombre) else {
            ui.label("Este objeto ya no existe");
            return;
        };
        let mut cambio = false;
        ui.horizontal(|ui| {
            ui.label("Nombre");
            let r = ui.add(egui::TextEdit::singleline(&mut self.edicion).desired_width(120.0));
            if r.lost_focus() {
                self.renombrar_objeto(nombre);
            }
        });
        let nombre = self.montaje.objetos[i].nombre.clone();
        let cats: Vec<String> = self.inventario.categorias.iter().filter(|c| !es_categoria_luz(c)).cloned().collect();
        let actual = self.montaje.objetos[i].categoria.clone();
        let mut elegida = actual.clone();
        ui.horizontal(|ui| {
            ui.label("Categoría");
            let texto = if actual.is_empty() { "(sin categoría)" } else { actual.as_str() };
            egui::ComboBox::from_id_salt("categoria_objeto")
                .selected_text(texto)
                .show_ui(ui, |ui| {
                    for c in &cats {
                        ui.selectable_value(&mut elegida, c.clone(), c.as_str());
                    }
                });
        });
        if elegida != actual {
            let r = self.montaje.cambiar_categoria_objeto(&nombre, &elegida);
            if self.avisar(r).is_some() {
                self.marcar_cambio();
            }
        }
        let forma_actual = self.montaje.objetos[i].tipo;
        let malla_actual = self.montaje.objetos[i].malla.clone();
        let texto = malla_actual.clone().unwrap_or_else(|| forma_actual.nombre().to_string());
        // Ok: forma del catálogo; Err: posición del modelo en la biblioteca.
        let mut elegido: Option<Result<TipoObjeto, usize>> = None;
        ui.horizontal(|ui| {
            ui.label("Forma");
            egui::ComboBox::from_id_salt("forma_objeto")
                .selected_text(texto)
                .show_ui(ui, |ui| {
                    for grupo in GRUPOS {
                        ui.strong(grupo);
                        for t in TipoObjeto::TODOS.into_iter().filter(|t| t.grupo() == grupo) {
                            if ui.selectable_label(malla_actual.is_none() && t == forma_actual, t.nombre()).clicked() {
                                elegido = Some(Ok(t));
                            }
                        }
                    }
                    if !self.biblioteca.is_empty() {
                        ui.strong("Importados");
                        for (k, b) in self.biblioteca.iter().enumerate() {
                            let activo = malla_actual.as_deref() == Some(b.nombre.as_str());
                            if ui.selectable_label(activo, b.nombre.as_str()).clicked() {
                                elegido = Some(Err(k));
                            }
                        }
                    }
                });
        });
        match elegido {
            Some(Ok(tipo)) if tipo != forma_actual || malla_actual.is_some() => {
                // Una forma del catálogo sustituye al modelo importado.
                self.montaje.objetos[i].malla = None;
                let r = self.montaje.cambiar_forma_objeto(&nombre, tipo);
                if self.avisar(r).is_some() {
                    self.marcar_cambio();
                }
            }
            Some(Err(k)) => {
                if let Some(b) = self.biblioteca.get(k) {
                    self.montaje.modelos.entry(b.nombre.clone()).or_insert_with(|| b.clone());
                    let o = &mut self.montaje.objetos[i];
                    o.malla = Some(b.nombre.clone());
                    (o.ancho, o.fondo, o.alto) = (b.tam[0], b.tam[2], b.tam[1]);
                    self.montaje.ajustar_a_sala();
                    self.marcar_cambio();
                }
            }
            _ => {}
        }
        // Solo los objetos que vienen del inventario tienen marca o modelo.
        if !self.montaje.objetos[i].marca.is_empty() || !self.montaje.objetos[i].modelo.is_empty() {
            ui.horizontal(|ui| {
                ui.label("Marca");
                ui.label(self.montaje.objetos[i].marca.as_str());
            });
            ui.horizontal(|ui| {
                ui.label("Modelo");
                ui.label(self.montaje.objetos[i].modelo.as_str());
            });
        }
        let (ancho, largo, techo) = (self.montaje.sala.ancho, self.montaje.sala.largo, self.montaje.sala.alto_techo);
        let o = &mut self.montaje.objetos[i];
        ui.horizontal(|ui| {
            ui.label("X");
            cambio |= numero(ui, &mut o.x, 0.0..=ancho);
        });
        ui.horizontal(|ui| {
            ui.label("Z");
            cambio |= numero(ui, &mut o.z, 0.0..=largo);
        });
        ui.horizontal(|ui| {
            ui.label("Ancho");
            cambio |= numero(ui, &mut o.ancho, 0.05..=20.0);
        });
        ui.horizontal(|ui| {
            ui.label("Fondo");
            cambio |= numero(ui, &mut o.fondo, 0.05..=20.0);
        });
        ui.horizontal(|ui| {
            ui.label("Alto");
            cambio |= numero(ui, &mut o.alto, 0.05..=techo);
        });
        ui.horizontal(|ui| {
            ui.label("Rotación");
            // Mismo filtro que `numero`: sin NaN ni infinitos.
            let campo = egui::DragValue::new(&mut o.rot)
                .speed(1.0)
                .max_decimals(0)
                .suffix("°")
                .custom_parser(|s| s.trim().replace(',', ".").parse::<f64>().ok().filter(|x| x.is_finite()));
            cambio |= ui.add(campo).changed();
        });
        if o.tipo == TipoObjeto::Maniqui && o.malla.is_none() {
            let base = TONOS_PIEL[1].1;
            ui.horizontal(|ui| {
                ui.label("Piel");
                let actual = o.piel.unwrap_or(base);
                let texto = TONOS_PIEL.iter().find(|(_, c)| *c == actual).map_or("Personalizado", |(n, _)| *n);
                egui::ComboBox::from_id_salt("piel_maniqui").selected_text(texto).show_ui(ui, |ui| {
                    for (nombre, c) in TONOS_PIEL {
                        if ui.selectable_label(actual == c, nombre).clicked() {
                            o.piel = Some(c);
                            cambio = true;
                        }
                    }
                });
                let mut color = o.piel.unwrap_or(base);
                if egui::color_picker::color_edit_button_srgb(ui, &mut color).changed() {
                    o.piel = Some(color);
                    cambio = true;
                }
            });
            let refl = reflexion_de_color(o.piel.unwrap_or(base));
            ui.label(format!("Reflexión {:.0} % · Absorción {:.0} %", refl * 100.0, (1.0 - refl) * 100.0));
        }
        if cambio {
            self.montaje.ajustar_a_sala();
            self.marcar_cambio();
        }
    }

    fn editor_camara(&mut self, ui: &mut egui::Ui, nombre: &str) {
        ui.heading("Cámara");
        let Some(i) = self.montaje.camaras.iter().position(|c| c.nombre == nombre) else {
            ui.label("Esta cámara ya no existe");
            return;
        };
        ui.horizontal(|ui| {
            ui.label("Nombre");
            let r = ui.add(egui::TextEdit::singleline(&mut self.edicion).desired_width(120.0));
            if r.lost_focus() {
                self.renombrar_camara(nombre);
            }
        });
        let (ancho, alto, largo) = (self.montaje.sala.ancho, self.montaje.sala.alto_techo, self.montaje.sala.largo);
        let destinos = destinos_apuntar(&self.montaje);
        // «Colocar en la vista actual» toma la vista libre, no la de cámara.
        let ojo = self.camara.ojo();
        let fwd = self.camara.base().0;
        let mut cambio = false;
        let c = &mut self.montaje.camaras[i];
        let origen = [c.x, c.altura, c.z];
        ui.horizontal(|ui| {
            ui.label("X");
            cambio |= numero(ui, &mut c.x, 0.0..=ancho);
        });
        ui.horizontal(|ui| {
            ui.label("Z");
            cambio |= numero(ui, &mut c.z, 0.0..=largo);
        });
        ui.horizontal(|ui| {
            ui.label("Altura");
            cambio |= numero(ui, &mut c.altura, 0.1..=alto);
        });
        ui.horizontal(|ui| {
            ui.label("Giro");
            cambio |= ui
                .add(egui::DragValue::new(&mut c.giro).speed(1.0).max_decimals(0).suffix("°").custom_parser(analiza))
                .changed();
        });
        ui.horizontal(|ui| {
            ui.label("Inclinación");
            let r = ui.add(egui::Slider::new(&mut c.inclinacion, -89.0..=89.0).suffix("°").fixed_decimals(0));
            cambio |= tocado(&r);
        });
        ui.horizontal(|ui| {
            ui.label("Apuntar a");
            let mut elegido: Option<usize> = None;
            let texto = if destinos.is_empty() { "(no hay objetos)" } else { "elegir…" };
            egui::ComboBox::from_id_salt(("apuntar_camara", nombre)).selected_text(texto).show_ui(ui, |ui| {
                for (k, (n, _)) in destinos.iter().enumerate() {
                    ui.selectable_value(&mut elegido, Some(k), n.as_str());
                }
            });
            if let Some((_, p)) = elegido.and_then(|k| destinos.get(k)) {
                let (giro, inclinacion) = apuntar(origen, *p);
                c.giro = giro;
                c.inclinacion = inclinacion;
                c.foco_m = (0..3).map(|k| (p[k] - origen[k]).powi(2)).sum::<f32>().sqrt();
                cambio = true;
            }
        });

        // Sensor: un preset de SENSORES, o «Personalizado» (el ancho se escribe a mano).
        let preset = SENSORES.iter().position(|s| (s.1 - c.sensor_ancho_mm).abs() < 0.01);
        let clave = ui.make_persistent_id(("sensor_personal", nombre));
        let mut personal = preset.is_none() || ui.ctx().data(|d| d.get_temp::<bool>(clave)).unwrap_or(false);
        ui.horizontal(|ui| {
            ui.label("Sensor");
            let actual = if personal { None } else { preset };
            let mut elegido = actual;
            let texto = actual.map_or("Personalizado", |k| SENSORES[k].0);
            egui::ComboBox::from_id_salt(("sensor", nombre)).selected_text(texto).show_ui(ui, |ui| {
                for (k, s) in SENSORES.iter().enumerate() {
                    ui.selectable_value(&mut elegido, Some(k), s.0);
                }
                ui.selectable_value(&mut elegido, None, "Personalizado");
            });
            if elegido != actual {
                if let Some(k) = elegido {
                    c.sensor_ancho_mm = SENSORES[k].1;
                    cambio = true;
                }
                personal = elegido.is_none();
                ui.ctx().data_mut(|d| d.insert_temp(clave, personal));
            }
        });
        if personal {
            ui.horizontal(|ui| {
                ui.label("Ancho del sensor");
                let campo = egui::DragValue::new(&mut c.sensor_ancho_mm).speed(0.1).max_decimals(1).range(5.0..=70.0);
                cambio |= ui.add(campo.suffix(" mm").custom_parser(analiza)).changed();
            });
        }

        ui.horizontal(|ui| {
            ui.label("Formato");
            let antes = c.aspecto;
            let texto = FORMATOS.iter().find(|f| f.1 == antes).map_or(format!("{antes:.2}:1"), |f| f.0.to_string());
            egui::ComboBox::from_id_salt(("formato", nombre)).selected_text(texto).show_ui(ui, |ui| {
                for (n, a) in FORMATOS {
                    ui.selectable_value(&mut c.aspecto, a, n);
                }
            });
            cambio |= c.aspecto != antes;
        });
        ui.horizontal(|ui| {
            ui.label("Focal");
            let r = ui.add(egui::Slider::new(&mut c.focal_mm, 8.0..=400.0).logarithmic(true).suffix(" mm").fixed_decimals(0));
            cambio |= tocado(&r);
        });
        let campo_h = 2.0 * (c.sensor_ancho_mm / (2.0 * c.focal_mm)).atan();
        ui.label(format!("Campo horizontal: {:.1}°", campo_h.to_degrees()));
        ui.horizontal(|ui| {
            ui.label("ISO");
            let antes = c.iso;
            combo_valores(ui, "iso_camara", &mut c.iso, &ISOS, |v| format!("{v}"));
            cambio |= c.iso != antes;
        });
        ui.horizontal(|ui| {
            ui.label("Diafragma");
            let antes = c.diafragma;
            combo_valores(ui, "diafragma_camara", &mut c.diafragma, &DIAFRAGMAS, |v| format!("f/{v}"));
            cambio |= c.diafragma != antes;
        });
        ui.horizontal(|ui| {
            ui.label("Obturación");
            let antes = c.obturacion;
            combo_valores(ui, "obturacion_camara", &mut c.obturacion, &OBTURACIONES, etiqueta_obturacion);
            cambio |= c.obturacion != antes;
        });
        ui.horizontal(|ui| {
            ui.label("Balance");
            let r = ui.add(egui::Slider::new(&mut c.balance_k, 2000.0..=12000.0).suffix(" K").fixed_decimals(0).update_while_editing(false));
            cambio |= tocado(&r);
            let (franja, _) = ui.allocate_exact_size(egui::vec2(28.0, 14.0), egui::Sense::hover());
            ui.painter().rect_filled(franja, 2.0, color_cct(c.balance_k));
        });
        ui.horizontal(|ui| {
            ui.label("Foco");
            cambio |= numero(ui, &mut c.foco_m, 0.1..=100.0);
        });
        ui.weak("Desenfoque solo en Fotorrealista");
        ui.label(format!("EV100: {:.1}", ev100(c.iso, c.diafragma, c.obturacion)));
        if ui.button("Colocar en la vista actual").clicked() {
            c.x = ojo.x;
            c.altura = ojo.y;
            c.z = ojo.z;
            let hacia = [ojo.x + fwd.x, ojo.y + fwd.y, ojo.z + fwd.z];
            (c.giro, c.inclinacion) = apuntar([ojo.x, ojo.y, ojo.z], hacia);
            cambio = true;
        }

        if cambio {
            c.limitar();
            self.montaje.ajustar_a_sala();
            self.marcar_cambio();
        }
    }

    fn ventana_objetos(&mut self, ctx: &egui::Context) {
        if !self.ver_objetos {
            return;
        }
        // Equipos de inventario que no son luces, por categoría; se omiten las categorías sin libres.
        let mut grupos: Vec<(String, Vec<Equipo>)> = Vec::new();
        for c in &self.inventario.categorias {
            if es_categoria_luz(c) {
                continue;
            }
            let libres: Vec<Equipo> = sin_colocar(&self.inventario.equipos, &self.montaje, c).into_iter().cloned().collect();
            if !libres.is_empty() {
                grupos.push((c.clone(), libres));
            }
        }
        let mut abierta = true;
        let mut elegido = None;
        let mut elegido_equipo = None;
        let mut elegido_modelo = None;
        let biblioteca = &self.biblioteca;
        egui::Window::new("Añadir objeto").open(&mut abierta).collapsible(false).show(ctx, |ui| {
            for (n, grupo) in GRUPOS.into_iter().enumerate() {
                egui::CollapsingHeader::new(grupo).default_open(n == 0).show(ui, |ui| {
                    for tipo in TipoObjeto::TODOS.into_iter().filter(|t| t.grupo() == grupo) {
                        if ui.button(tipo.nombre()).clicked() {
                            elegido = Some(tipo);
                        }
                    }
                });
            }
            if !biblioteca.is_empty() {
                egui::CollapsingHeader::new("Importados").default_open(true).show(ui, |ui| {
                    for (k, b) in biblioteca.iter().enumerate() {
                        if ui.button(b.nombre.as_str()).clicked() {
                            elegido_modelo = Some(k);
                        }
                    }
                });
            }
            for (cat, libres) in &grupos {
                ui.separator();
                ui.strong(cat.as_str());
                for e in libres {
                    let texto = format!("{} — {} {}", e.codigo, e.marca, e.modelo);
                    if ui.button(texto).clicked() {
                        elegido_equipo = Some(e.clone());
                    }
                }
            }
        });
        if !abierta {
            self.ver_objetos = false;
        }
        if let Some(tipo) = elegido {
            let nombre = self.montaje.anadir_objeto(tipo);
            self.seleccion = Some(Sel::Objeto(nombre));
            self.marcar_cambio();
            self.ver_objetos = false;
            self.montaje.ajustar_a_sala();
        }
        if let Some(e) = elegido_equipo {
            let r = self.montaje.colocar_objeto(&e);
            if self.avisar(r).is_some() {
                self.seleccion = Some(Sel::Objeto(e.codigo.clone()));
                self.marcar_cambio();
                self.ver_objetos = false;
                self.montaje.ajustar_a_sala();
            }
        }
        if let Some(b) = elegido_modelo.and_then(|k| self.biblioteca.get(k)) {
            let nombre = self.montaje.colocar_malla(b);
            self.seleccion = Some(Sel::Objeto(nombre));
            self.marcar_cambio();
            self.ver_objetos = false;
        }
    }

    fn vista_3d(&mut self, ui: &mut egui::Ui) {
        let area = ui.max_rect();
        let resp = ui.allocate_rect(area, egui::Sense::click_and_drag());
        // En vista de cámara, la escena cae en el encuadre (letterbox) y lo demás queda gris.
        let (cam, rect) = self.camara_y_rect(area);
        let prims = self.primitivas();
        let raton = resp.interact_pointer_pos();
        let primario = egui::PointerButton::Primary;

        // Un clic fuera del encuadre no elige nada: ahí no se ve la escena.
        if resp.clicked_by(primario) {
            self.seleccion = raton.filter(|p| rect.contains(*p)).and_then(|p| elegir(&cam, rect, &prims, p));
        }
        if resp.drag_started_by(primario) {
            // Se elige en el punto donde empezó el arrastre, no donde va ya el puntero.
            let origen = ui.input(|i| i.pointer.press_origin());
            let hit = origen.filter(|p| rect.contains(*p)).and_then(|p| elegir(&cam, rect, &prims, p));
            self.arrastrando = hit.is_some();
            if hit.is_some() {
                self.seleccion = hit;
            }
        }
        if self.arrastrando && resp.dragged_by(primario) && resp.drag_delta() != egui::Vec2::ZERO {
            if let (Some(p), Some(sel)) = (raton, self.seleccion.clone()) {
                let (origen, dir) = cam.rayo(p, rect);
                arrastrar(&mut self.montaje, &sel, origen, dir);
                self.marcar_cambio();
            }
        }
        if resp.drag_stopped_by(primario) {
            self.arrastrando = false;
        }
        let delta = resp.drag_delta();
        if resp.dragged_by(egui::PointerButton::Secondary) {
            if self.vista_camara.is_none() {
                self.camara.orbitar(delta);
            } else if delta != egui::Vec2::ZERO {
                self.editar_camara_vista(|c| {
                    c.giro -= delta.x * 0.2;
                    c.inclinacion -= delta.y * 0.2;
                });
            }
        }
        if resp.hovered() {
            let rueda = ui.input(|i| i.smooth_scroll_delta.y);
            if rueda != 0.0 {
                if self.vista_camara.is_none() {
                    self.camara.zoom(rueda);
                } else {
                    self.editar_camara_vista(|c| c.focal_mm *= (rueda * 0.002).exp());
                }
            }
        }

        // Se reconstruye tras los cambios de este frame para no dibujar un fotograma atrasado.
        let (cam, rect) = self.camara_y_rect(area);
        let prims = self.primitivas();
        let pintor = ui.painter_at(area);
        if self.vista_camara.is_some() {
            pintor.rect_filled(area, 0.0, egui::Color32::from_gray(18));
        }
        // La escena y sus etiquetas no salen del encuadre de la cámara.
        let escena = ui.painter_at(rect);
        if self.modo != ModoVista::Esquema && self.render.is_some() {
            let vista = self.camara_vista().cloned();
            self.pintar_gpu(&escena, rect, &prims, ui.ctx(), cam, vista);
            if self.modo == ModoVista::Realista {
                let progreso = self.render.as_ref().and_then(|r| r.lock().ok().and_then(|r| r.progreso()));
                if let Some((n, max)) = progreso {
                    texto_con_fondo(
                        &pintor,
                        rect.right_top() + egui::vec2(-10.0, 10.0),
                        egui::Align2::RIGHT_TOP,
                        format!("Muestras: {n} / {max}"),
                        13.0,
                        egui::Color32::from_gray(230),
                    );
                }
            }
        } else {
            dibujar(&escena, &cam, rect, &prims);
        }
        let ayuda = if self.vista_camara.is_some() {
            "Vista de cámara · Botón derecho: girar la cámara · Rueda: zoom · Esc: salir"
        } else {
            "Clic: seleccionar · Arrastrar: mover · Botón derecho: girar · Rueda: zoom"
        };
        texto_con_fondo(
            &pintor,
            area.left_bottom() + egui::vec2(10.0, -10.0),
            egui::Align2::LEFT_BOTTOM,
            ayuda,
            13.0,
            egui::Color32::from_gray(200),
        );
        if let Some(c) = self.camara_vista() {
            let texto = format!(
                "{} · {:.0} mm · f/{} · {} · ISO {:.0} · {:.0} K",
                c.nombre,
                c.focal_mm,
                c.diafragma,
                etiqueta_obturacion(c.obturacion),
                c.iso,
                c.balance_k
            );
            texto_con_fondo(
                &pintor,
                rect.left_top() + egui::vec2(10.0, 10.0),
                egui::Align2::LEFT_TOP,
                texto,
                13.0,
                egui::Color32::from_gray(230),
            );
        }
        let hover = resp.hover_pos().filter(|p| rect.contains(*p));
        if let Some((p, lux)) = self.lux_bajo_cursor(&cam, rect, hover) {
            pintor.circle_filled(p + egui::vec2(10.0, 14.0), 5.0, color_lux(lux));
            pintor.text(
                p + egui::vec2(20.0, 14.0),
                egui::Align2::LEFT_CENTER,
                format!("≈ {lux:.0} lux"),
                egui::FontId::proportional(14.0),
                egui::Color32::WHITE,
            );
        }
    }

    /// Guarda la configuración y la aplica al renderer (escala, MSAA y sombras).
    /// Si el renderer corrige la configuración (p. ej. desactiva el suavizado), la corrección queda guardada.
    fn aplicar_config(&mut self) {
        self.cfg.explicita = true;
        let mut avisos = Vec::new();
        let aplicado = match (self.render.clone(), self.gl.clone()) {
            (Some(r), Some(gl)) => match r.lock() {
                Ok(mut r) => {
                    let res = r.configurar(&gl, &mut self.cfg);
                    avisos = r.tomar_avisos();
                    res
                }
                Err(_) => Err("El renderer 3D no está disponible".to_string()),
            },
            _ => Ok(()),
        };
        for a in avisos {
            self.aviso(a);
        }
        self.avisar(aplicado);
        let guardado = guardar(&ruta_config(), &self.cfg);
        self.avisar(guardado);
    }

    /// Fallos y avisos del renderer: el trazado ajustado solo, o un error que obliga a bajar de preset.
    fn atender_render(&mut self) {
        let Some(r) = self.render.clone() else { return };
        let (fallo, ajuste, avisos) = match r.lock() {
            Ok(mut r) => (r.tomar_fallo(), r.tomar_ajuste(), r.tomar_avisos()),
            Err(_) => return,
        };
        for a in avisos {
            self.aviso(a);
        }
        if let Some((escala, rebotes)) = ajuste {
            self.cfg.escala = escala;
            self.cfg.rebotes = rebotes;
            self.aplicar_config();
            self.aviso(format!(
                "Trazado ajustado a tu GPU: escala {:.0} %, {rebotes} rebotes.",
                escala * 100.0
            ));
        }
        match fallo {
            Some(Fallo::Gl(e)) => {
                self.cfg.preset = Preset::Alta;
                self.aplicar_config();
                self.aviso(format!("Tu GPU no admite Fotorrealista: se usa Alta.\n{e}"));
            }
            Some(Fallo::Lento(e)) => {
                self.cfg.preset = Preset::Alta;
                self.aplicar_config();
                self.aviso(format!("Fotorrealista es demasiado lento para esta GPU ({e}): se usa Alta."));
            }
            Some(Fallo::Escena(e)) => {
                self.aviso(format!("Fotorrealista no puede trazar esta escena: {e}. Se muestra la vista rápida."));
            }
            None => {}
        }
    }

    /// Acabado de cada superficie: plano de la sala para elegir una pared o el suelo, la lista para el techo, y su material, color, reflexión y absorción.
    fn ventana_superficies(&mut self, ctx: &egui::Context) {
        let mut abierta = self.ver_superficies;
        let mut cambio = false;
        egui::Window::new("Superficies").open(&mut abierta).resizable(false).show(ctx, |ui| {
            let (ancho, largo) = (self.montaje.sala.ancho, self.montaje.sala.largo);
            let (area, _) = ui.allocate_exact_size(egui::vec2(220.0, 160.0), egui::Sense::hover());
            // Norte arriba (z = largo), Oeste a la izquierda (x = 0): el plano se ve como desde arriba.
            let escala = (area.width() - 36.0) / ancho;
            let escala = escala.min((area.height() - 32.0) / largo);
            let plano = egui::Rect::from_center_size(area.center(), egui::vec2(ancho * escala, largo * escala));
            let [r, g, bl] = self.montaje.sala.acabado_de(Superficie::Suelo).color;
            ui.painter().rect_filled(plano, 0.0, egui::Color32::from_rgba_unmultiplied(r, g, bl, 102));
            let mut elegida = None;
            // Antes que las paredes: donde se tocan, gana la pared (se registra después).
            if ui.interact(plano, ui.scope_id().with("suelo"), egui::Sense::click()).clicked() {
                elegida = Some(Superficie::Suelo);
            }
            if self.superficie_editada == Superficie::Suelo {
                ui.painter().rect_stroke(plano, 0.0, egui::Stroke::new(2.0, egui::Color32::WHITE), egui::StrokeKind::Inside);
            }
            let lados = [
                (Pared::Norte, "N", [plano.left_top(), plano.right_top()], egui::vec2(0.0, -12.0)),
                (Pared::Sur, "S", [plano.left_bottom(), plano.right_bottom()], egui::vec2(0.0, 12.0)),
                (Pared::Oeste, "O", [plano.left_top(), plano.left_bottom()], egui::vec2(-12.0, 0.0)),
                (Pared::Este, "E", [plano.right_top(), plano.right_bottom()], egui::vec2(12.0, 0.0)),
            ];
            for (i, (p, letra, [a, b], desplazo)) in lados.into_iter().enumerate() {
                let sel = self.superficie_editada == Superficie::Pared(p);
                let grosor = if sel { 10.0 } else { 6.0 };
                let [r, g, bl] = self.montaje.sala.acabado(p).color;
                let pintor = ui.painter();
                if sel {
                    pintor.line_segment([a, b], egui::Stroke::new(grosor + 4.0, egui::Color32::WHITE));
                }
                pintor.line_segment([a, b], egui::Stroke::new(grosor, egui::Color32::from_rgb(r, g, bl)));
                let medio = a + (b - a) * 0.5 + desplazo;
                pintor.text(medio, egui::Align2::CENTER_CENTER, letra, egui::FontId::proportional(13.0), ui.visuals().text_color());
                let zona = egui::Rect::from_two_pos(a, b).expand(6.0);
                if ui.interact(zona, ui.scope_id().with(i), egui::Sense::click()).clicked() {
                    elegida = Some(Superficie::Pared(p));
                }
            }
            if let Some(s) = elegida {
                self.superficie_editada = s;
            }
            ui.horizontal(|ui| {
                for s in [Superficie::Suelo, Superficie::Techo] {
                    if ui.selectable_label(self.superficie_editada == s, s.nombre()).clicked() {
                        self.superficie_editada = s;
                    }
                }
            });

            let s = self.superficie_editada;
            ui.heading(s.nombre());
            let a = self.montaje.sala.acabado_de_mut(s);
            let actual = *a;
            let texto = ACABADOS
                .iter()
                .find(|(_, x)| *x == actual)
                .map(|(n, _)| n.to_string())
                .or_else(|| self.acabados_usuario.iter().find(|(_, x)| **x == actual).map(|(n, _)| n.clone()))
                .unwrap_or_else(|| "Personalizado".to_string());
            let mut material = None;
            // Nombre del preset del usuario elegido, para que el montaje lo guarde.
            let mut de_usuario = None;
            egui::Grid::new("grid_paredes").num_columns(2).show(ui, |ui| {
                ui.label("Material");
                egui::ComboBox::from_id_salt("material_pared").selected_text(texto).show_ui(ui, |ui| {
                    for (nombre, x) in ACABADOS {
                        if ui.selectable_label(x == actual, nombre).clicked() {
                            material = Some(x);
                        }
                    }
                    if !self.acabados_usuario.is_empty() {
                        ui.separator();
                        for (nombre, x) in &self.acabados_usuario {
                            if ui.selectable_label(*x == actual, nombre).clicked() {
                                material = Some(*x);
                                de_usuario = Some(nombre.clone());
                            }
                        }
                    }
                });
                ui.end_row();
                ui.label("Color");
                cambio |= egui::color_picker::color_edit_button_srgb(ui, &mut a.color).changed();
                ui.end_row();
                ui.label("Reflexión");
                cambio |= tocado(&ui.add(egui::Slider::new(&mut a.reflexion, 0.0..=0.95).fixed_decimals(2)));
                ui.end_row();
                ui.label("Absorción");
                let mut absorcion = 1.0 - a.reflexion;
                if tocado(&ui.add(egui::Slider::new(&mut absorcion, 0.05..=1.0).fixed_decimals(2))) {
                    a.reflexion = 1.0 - absorcion;
                    cambio = true;
                }
                ui.end_row();
            });
            if let Some(x) = material {
                *a = x;
                cambio = true;
                if let Some(nombre) = de_usuario {
                    self.montaje.acabados.insert(nombre, x);
                }
            }

            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.nombre_preset).hint_text("Nombre del preset"));
                let nombre = nombre_preset_valido(&self.nombre_preset);
                if ui.add_enabled(nombre.is_some(), egui::Button::new("Guardar como preset")).clicked() {
                    if let Some(nombre) = nombre {
                        let acabado = self.montaje.sala.acabado_de(s);
                        self.acabados_usuario.insert(nombre.clone(), acabado);
                        self.montaje.acabados.insert(nombre, acabado);
                        self.guardar_presets();
                        self.nombre_preset.clear();
                        cambio = true;
                    }
                }
            });
            if !self.nombre_preset.trim().is_empty() && nombre_preset_valido(&self.nombre_preset).is_none() {
                ui.weak("Ese nombre es de un preset de fábrica");
            }
        });
        self.ver_superficies = abierta;
        if cambio {
            self.marcar_cambio();
        }
    }

    /// Calidad de render: preset, muestras, rebotes, sombras, suavizado y escala, con los datos de la GPU.
    fn ventana_config(&mut self, ctx: &egui::Context) {
        let mut abierta = self.ver_config;
        let mut cambio = false;
        let mut recomendar = false;
        egui::Window::new("Configuración").open(&mut abierta).resizable(false).show(ctx, |ui| {
            ui.strong("Calidad de render");
            // Sin GPU (renderer de software) el trazado no es viable: Fotorrealista queda deshabilitado.
            let software = es_software(&self.gpu.1);
            ui.horizontal(|ui| {
                for p in Preset::TODOS {
                    let bloqueado = software && p == Preset::Fotorrealista;
                    let r = ui.add_enabled(!bloqueado, egui::RadioButton::new(self.cfg.preset == p, p.nombre()));
                    if r.clicked() {
                        self.cfg.preset = p;
                        cambio = true;
                    }
                    if bloqueado {
                        r.on_disabled_hover_text("Tu GPU no tiene aceleración 3D (renderer de software): Fotorrealista no es viable.");
                    }
                }
            });
            egui::Grid::new("config_grid").num_columns(2).show(ui, |ui| {
                ui.label("Muestras máx.");
                cambio |= tocado(&ui.add(egui::Slider::new(&mut self.cfg.muestras_max, 64..=2048).logarithmic(true)));
                ui.end_row();
                ui.label("Rebotes");
                cambio |= tocado(&ui.add(egui::Slider::new(&mut self.cfg.rebotes, 1..=32)));
                ui.end_row();
                ui.label("Resolución de sombras");
                egui::ComboBox::from_id_salt("res_sombras")
                    .selected_text(format!("{} px", self.cfg.res_sombras))
                    .show_ui(ui, |ui| {
                        // 4096 px ≈ 1 GB de VRAM: solo con GPU dedicada.
                        for r in [1024, 2048, 4096].into_iter().filter(|r| *r < 4096 || !es_integrada(&self.gpu.1, &self.gpu.0)) {
                            cambio |= ui.selectable_value(&mut self.cfg.res_sombras, r, format!("{r} px")).changed();
                        }
                    });
                ui.end_row();
                ui.label("Suavizado");
                let etiqueta = |m: u32| if m == 0 { "Desactivado".to_string() } else { format!("{m}×") };
                egui::ComboBox::from_id_salt("msaa").selected_text(etiqueta(self.cfg.msaa)).show_ui(ui, |ui| {
                    for m in [0, 2, 4, 8] {
                        cambio |= ui.selectable_value(&mut self.cfg.msaa, m, etiqueta(m)).changed();
                    }
                });
                ui.end_row();
                ui.label("Escala de resolución");
                let r = ui.add(
                    egui::Slider::new(&mut self.cfg.escala, 0.5..=1.0).custom_formatter(|v, _| format!("{:.0} %", v * 100.0)),
                );
                cambio |= tocado(&r);
                ui.end_row();
            });
            ui.separator();
            ui.strong("GPU");
            let (vendor, renderer, version) = &self.gpu;
            ui.label(format!("Fabricante: {vendor}"));
            ui.label(format!("Renderer: {renderer}"));
            ui.label(format!("OpenGL: {version}"));
            let origen = if self.cfg.explicita { "guardada" } else { "recomendada para esta GPU" };
            ui.label(format!("Valores: {origen}"));
            let reducida = self.render.as_ref().and_then(|r| r.lock().ok().and_then(|r| r.escala_reducida()));
            if let Some(e) = reducida {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 200, 80),
                    format!("Trazado: escala reducida automáticamente a {:.0} %", e * 100.0),
                );
            }
            if ui.button("Valores recomendados para esta GPU").clicked() {
                recomendar = true;
            }
        });
        self.ver_config = abierta;
        if recomendar {
            self.cfg = por_defecto_para(&self.gpu.1, &self.gpu.0);
            cambio = true;
        }
        if cambio {
            self.aplicar_config();
        }
    }

    fn ventana_inventario(&mut self, ctx: &egui::Context) {
        let mut abierta = self.ver_inventario;
        let mut accion: Option<FilaInv> = None;
        let categorias = self.inventario.categorias.clone();
        // Si la pestaña elegida ya no existe (p. ej. se borró su categoría), se pasa a la primera.
        if !categorias.contains(&self.inv_pestana) {
            self.inv_pestana = categorias.first().cloned().unwrap_or_default();
        }
        egui::Window::new("Inventario").open(&mut abierta).default_width(780.0).show(ctx, |ui| {
            let solo = self.inventario.solo_lectura;
            if solo {
                ui.colored_label(egui::Color32::RED, "No se pudo leer el inventario; no se guardarán cambios");
            }
            ui.horizontal_wrapped(|ui| {
                for cat in &categorias {
                    let n = self.inventario.de_categoria(cat).len();
                    if ui.selectable_label(self.inv_pestana == *cat, format!("{cat} ({n})")).clicked() {
                        self.inv_pestana = cat.clone();
                    }
                }
            });
            ui.separator();
            if let Some(cat) = categorias.iter().find(|c| **c == self.inv_pestana).cloned() {
                let base = crate::modelo::CATEGORIAS_BASE.contains(&cat.as_str());
                if !solo && !base && ui.button("Borrar categoría").clicked() {
                    accion = Some(FilaInv::BorrarCategoria(cat.clone()));
                }
                // Las filas se identifican por su posición en `inv_filas`, que es la del inventario en este fotograma.
                // Las pestañas de luces añaden tipo y especificaciones; con solo lectura se ven deshabilitadas.
                let luces = es_categoria_luz(&cat);
                let piso = cat == CATEGORIA_LUCES_PISO;
                egui::ScrollArea::horizontal().show(ui, |ui| {
                    egui::Grid::new(("tabla_inventario", cat.as_str())).striped(true).show(ui, |ui| {
                        ui.strong("Código");
                        ui.strong("Marca");
                        ui.strong("Modelo");
                        if luces {
                            for c in ["Tipo", "W", "lux@1m", "Ap. mín°", "Ap. máx°", "CCT mín", "CCT máx", "CRI"] {
                                ui.strong(c);
                            }
                        }
                        ui.end_row();
                        for (i, fila) in self.inv_filas.iter_mut().enumerate() {
                            if fila.categoria != cat {
                                continue;
                            }
                            if solo {
                                ui.label(fila.codigo.as_str());
                                ui.label(fila.marca.as_str());
                                ui.label(fila.modelo.as_str());
                            } else {
                                let h = ui.spacing().interact_size.y;
                                let r1 = ui.add_sized([90.0, h], egui::TextEdit::singleline(&mut fila.codigo));
                                let r2 = ui.add_sized([140.0, h], egui::TextEdit::singleline(&mut fila.marca));
                                let r3 = ui.add_sized([140.0, h], egui::TextEdit::singleline(&mut fila.modelo));
                                if r1.lost_focus() || r2.lost_focus() || r3.lost_focus() {
                                    accion = Some(FilaInv::Editar(i));
                                }
                            }
                            if luces {
                                let mut s = fila.specs.clone().unwrap_or_else(|| specs_por_defecto(piso));
                                if celdas_specs(ui, ("fila", i), &mut s, piso, !solo) {
                                    fila.specs = Some(s);
                                    accion = Some(FilaInv::Editar(i));
                                }
                            }
                            if !solo && ui.button("Borrar").clicked() {
                                accion = Some(FilaInv::Borrar(i));
                            }
                            ui.end_row();
                        }
                        if !solo {
                            let nuevo = self.inv_nuevos.entry(cat.clone()).or_insert_with(|| fila_nueva(&cat));
                            let h = ui.spacing().interact_size.y;
                            ui.add_sized([90.0, h], egui::TextEdit::singleline(&mut nuevo.codigo).hint_text("Código"));
                            ui.add_sized([140.0, h], egui::TextEdit::singleline(&mut nuevo.marca).hint_text("Marca"));
                            ui.add_sized([140.0, h], egui::TextEdit::singleline(&mut nuevo.modelo).hint_text("Modelo"));
                            if let Some(s) = nuevo.specs.as_mut() {
                                celdas_specs(ui, ("nuevo", 0), s, piso, true);
                            }
                            if ui.button("Añadir").clicked() {
                                accion = Some(FilaInv::Anadir(cat.clone()));
                            }
                            ui.end_row();
                        }
                    });
                });
            }
            if !solo {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.nueva_categoria).hint_text("Nueva categoría").desired_width(120.0));
                    if ui.button("Añadir categoría").clicked() {
                        accion = Some(FilaInv::AnadirCategoria);
                    }
                });
            }
        });
        self.ver_inventario = abierta;

        // Las filas editadas se aplican fuera del bucle; con error vuelven a su valor anterior.
        let r = match accion {
            None => return,
            Some(FilaInv::Editar(i)) => {
                let Some(actual) = self.inventario.equipos.get(i).cloned() else { return };
                if self.inv_filas[i] == actual {
                    return;
                }
                let r = self.inventario.editar(&actual.codigo, self.inv_filas[i].clone());
                // Con el código anterior: la luz colocada conserva el suyo.
                if r.is_ok() && self.montaje.actualizar_equipo(&actual.codigo, &self.inv_filas[i]) {
                    self.marcar_cambio();
                }
                r
            }
            Some(FilaInv::Borrar(i)) => match self.inventario.equipos.get(i).map(|e| e.codigo.clone()) {
                Some(orig) => self.inventario.borrar(&orig),
                None => return,
            },
            Some(FilaInv::Anadir(cat)) => {
                let mut e = self.inv_nuevos.get(&cat).cloned().unwrap_or_else(|| fila_nueva(&cat));
                e.categoria = cat.clone();
                let r = self.inventario.anadir(e);
                if r.is_ok() {
                    self.inv_nuevos.remove(&cat);
                }
                r
            }
            Some(FilaInv::AnadirCategoria) => {
                let nombre = self.nueva_categoria.trim().to_string();
                let r = self.inventario.anadir_categoria(&self.nueva_categoria);
                if r.is_ok() {
                    self.nueva_categoria.clear();
                    self.inv_pestana = nombre;
                }
                r
            }
            Some(FilaInv::BorrarCategoria(cat)) => self.inventario.borrar_categoria(&cat),
        };
        self.avisar(r);
        self.sincronizar_inventario();
    }

    /// Selector de equipos para luces de techo o de piso: lista los del inventario que no están colocados.
    fn ventana_selector(&mut self, ctx: &egui::Context) {
        for piso in [false, true] {
            let abierta_ahora = if piso { self.ver_selector_piso } else { self.ver_selector };
            if !abierta_ahora {
                continue;
            }
            let (categoria, titulo) =
                if piso { (CATEGORIA_LUCES_PISO, "Añadir luz de piso") } else { (CATEGORIA_LUCES, "Añadir luz") };
            let libres: Vec<Equipo> =
                sin_colocar(&self.inventario.equipos, &self.montaje, categoria).into_iter().cloned().collect();
            let mut abierta = true;
            let mut elegido = None;
            egui::Window::new(titulo).open(&mut abierta).collapsible(false).show(ctx, |ui| {
                if libres.is_empty() {
                    ui.label("No hay equipos disponibles. Dalos de alta en Inventario.");
                }
                for e in &libres {
                    let texto = format!("{} — {} {}", e.codigo, e.marca, e.modelo);
                    if ui.button(texto).clicked() {
                        elegido = Some(e.clone());
                    }
                }
            });
            if !abierta {
                if piso {
                    self.ver_selector_piso = false;
                } else {
                    self.ver_selector = false;
                }
            }
            if let Some(e) = elegido {
                if piso {
                    self.colocar_luz_piso(&e);
                } else {
                    self.colocar_luz(&e);
                }
            }
        }
    }

    fn modales(&mut self, ctx: &egui::Context) {
        if let Some(accion) = self.pendiente {
            let mut opcion = None;
            let r = egui::Modal::new(egui::Id::unique("cambios_sin_guardar")).show(ctx, |ui| {
                ui.heading("Hay cambios sin guardar");
                ui.horizontal(|ui| {
                    if ui.button("Guardar").clicked() {
                        opcion = Some(Opcion::Guardar);
                    }
                    if ui.button("Descartar").clicked() {
                        opcion = Some(Opcion::Descartar);
                    }
                    if ui.button("Cancelar").clicked() {
                        opcion = Some(Opcion::Cancelar);
                    }
                });
            });
            match opcion {
                Some(Opcion::Guardar) => {
                    // Si el guardado falla o se cancela, la acción no se hace y el modal sigue.
                    if self.guardar() {
                        self.pendiente = None;
                        self.ejecutar(ctx, accion);
                    }
                }
                Some(Opcion::Descartar) => {
                    self.pendiente = None;
                    self.ejecutar(ctx, accion);
                }
                Some(Opcion::Cancelar) => self.pendiente = None,
                None if r.should_close() => self.pendiente = None,
                None => {}
            }
        }

        // Se muestra después del de cambios: si hay los dos, el de error queda encima.
        if let Some(texto) = self.mensaje.clone() {
            let mut aceptar = false;
            let r = egui::Modal::new(egui::Id::unique("mensaje")).show(ctx, |ui| {
                ui.label(texto.as_str());
                if ui.button("Aceptar").clicked() {
                    aceptar = true;
                }
            });
            if aceptar || r.should_close() {
                self.mensaje = None;
            }
        }
    }
}

impl eframe::App for App {
    fn on_exit(&mut self, gl: Option<&glow::Context>) {
        if let (Some(r), Some(gl)) = (self.render.take(), gl) {
            if let Ok(r) = r.lock() {
                r.destruir(gl);
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.atender_render();
        self.cierre(&ctx);
        self.sincronizar_edicion();
        self.atajos_camara(&ctx);
        self.atajos_archivo(&ctx);
        self.titulo(&ctx);

        egui::Panel::top("barra").show(ui, |ui| self.barra(ui, &ctx));
        egui::Panel::left("izquierda").default_size(250.0).show(ui, |ui| self.panel_izquierdo(ui));
        egui::Panel::right("derecha").default_size(300.0).show(ui, |ui| self.panel_derecho(ui));
        egui::CentralPanel::default().show(ui, |ui| self.vista_3d(ui));

        self.ventana_inventario(&ctx);
        self.ventana_selector(&ctx);
        self.ventana_objetos(&ctx);
        self.ventana_config(&ctx);
        self.ventana_superficies(&ctx);
        self.modales(&ctx);

        // Una ventana recién abierta no se dibuja hasta el siguiente evento si la app está ociosa.
        let abiertos = [
            self.mensaje.is_some(),
            self.pendiente.is_some(),
            self.ver_inventario,
            self.ver_selector,
            self.ver_selector_piso,
            self.ver_objetos,
            self.ver_config,
            self.ver_superficies,
        ];
        if self.ultimo_abiertos != Some(abiertos) {
            self.ultimo_abiertos = Some(abiertos);
            ctx.request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn equipo(codigo: &str, categoria: &str) -> Equipo {
        Equipo {
            codigo: codigo.to_string(),
            marca: "Marca".to_string(),
            modelo: "Modelo".to_string(),
            categoria: categoria.to_string(),
            specs: None,
        }
    }

    #[test]
    fn sin_colocar_excluye_lo_colocado_y_otras_categorias() {
        let equipos = vec![equipo("L1", "Luces"), equipo("M1", "Mesas"), equipo("L2", "Luces")];
        let mut m = Montaje::default();
        let carril = m.anadir_carril();
        m.colocar_luz(&equipos[0], &carril).unwrap();
        let codigos = |v: Vec<&Equipo>| v.iter().map(|e| e.codigo.clone()).collect::<Vec<_>>();
        assert_eq!(codigos(sin_colocar(&equipos, &m, "Luces")), ["L2"]);
        assert_eq!(codigos(sin_colocar(&equipos, &m, "Mesas")), ["M1"]);
        m.colocar_objeto(&equipos[1]).unwrap();
        assert!(sin_colocar(&equipos, &m, "Mesas").is_empty());
    }

    #[test]
    fn ampliar_rango_incluye_valor() {
        assert_eq!(ampliar_rango(90.0, 90.0, 120.0), (90.0, 120.0));
        assert_eq!(ampliar_rango(90.0, 90.0, 40.0), (40.0, 90.0));
        assert_eq!(ampliar_rango(30.0, 60.0, 45.0), (30.0, 60.0));
    }

    #[test]
    fn encuadre_letterbox() {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 900.0));
        let cine = encuadre(rect, 2.39);
        assert!((cine.width() - 1600.0).abs() < 0.01 && (cine.height() - 669.5).abs() < 0.1, "{cine:?}");
        assert!((cine.center() - rect.center()).length() < 0.01);
        let cuadro = encuadre(rect, 1.0);
        assert!((cuadro.width() - 900.0).abs() < 0.01 && (cuadro.height() - 900.0).abs() < 0.01, "{cuadro:?}");
        assert!((cuadro.center() - rect.center()).length() < 0.01);
    }

    #[test]
    fn controles_ajustes_amplia_specs() {
        let mut s = TipoLuz::PanelLed.specs();
        s.apertura_min = 90.0;
        s.apertura_max = 90.0;
        s.cct_min = 5600.0;
        s.cct_max = 5600.0;
        let mut a = AjustesLuz::por_defecto(&s, false);
        let ctx = egui::Context::default();
        let mut salida = ctx.run_pass(egui::RawInput::default(), |ctx| {
            egui::Area::new(egui::Id::unique("prueba")).show(ctx, |ui| {
                controles_ajustes(ui, &mut s, &mut a, [0.0, 0.0, 0.0], &[], "prueba", None);
            });
        });
        // Sin pantalla no se aplican las texturas de fuentes; hay que vaciarlas o epaint entra en pánico al soltarlas.
        salida.textures_delta.clear();
    }
}
