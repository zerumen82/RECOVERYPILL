fn main() {
    // Obtener la fecha y hora actual
    let now = chrono::Local::now();
    let build_date = now.format("%Y-%m-%d").to_string();
    let build_time = now.format("%H:%M:%S").to_string();
    let build_timestamp = now.format("%Y-%m-%d %H:%M:%S").to_string();
    
    // Crear el directorio de salida si no existe
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest_path = std::path::Path::new(&out_dir).join("build_info.rs");
    
    // Generar el código Rust con la información de build
    let content = format!(
        r#"
/// Fecha de compilación
pub const BUILD_DATE: &str = "{}";
/// Hora de compilación
pub const BUILD_TIME: &str = "{}";
/// Timestamp completo de compilación
pub const BUILD_TIMESTAMP: &str = "{}";
"#,
        build_date, build_time, build_timestamp
    );
    
    // Escribir el archivo
    std::fs::write(&dest_path, content).unwrap();
    
    // Indicar a Cargo que re-ejecute si cambia build.rs
    println!("cargo:rerun-if-changed=build.rs");
}
