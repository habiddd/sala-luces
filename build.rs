//! Exporta los símbolos que leen los drivers de GPU híbrida (NVIDIA Optimus, AMD PowerXpress).
//! Con rustc estable y enlazador GNU, `#[no_mangle]` no basta en un binario: el .def los exporta.

fn main() {
    println!("cargo:rerun-if-changed=exports.def");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let def = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("exports.def");
        println!("cargo:rustc-link-arg-bins={}", def.display());
    }
}
