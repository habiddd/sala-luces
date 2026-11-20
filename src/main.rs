#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod escena;
mod fotometria;
mod geom;
mod importar;
mod ies;
mod inventario;
mod quitarruido;
mod modelo;
mod render_gl;
mod trazado;

// Los drivers leen estos símbolos para usar la GPU dedicada en portátiles híbridos. `exports.def` los exporta
// (build.rs); `#[used]` evita que el enlazador los descarte.
#[allow(non_upper_case_globals)]
#[unsafe(no_mangle)]
#[used]
pub static NvOptimusEnablement: u32 = 1;
#[allow(non_upper_case_globals)]
#[unsafe(no_mangle)]
#[used]
pub static AmdPowerXpressRequestHighPerformance: i32 = 1;

fn main() {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Sala de luces")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 600.0])
            .with_maximized(true),
        ..Default::default()
    };
    let resultado = eframe::run_native("Sala de luces", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))));
    // Sin OpenGL utilizable la ventana no abre: en la versión de ventana (sin consola) hay que avisar aquí.
    if let Err(e) = resultado {
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("Sala de luces")
            .set_description(format!(
                "No se pudo iniciar OpenGL. Se necesita el controlador de la tarjeta gráfica con OpenGL 3.3 (o al menos 2.0 para la vista básica). Detalle: {e}"
            ))
            .show();
    }
}
