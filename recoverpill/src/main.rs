//! recoverPill - Herramienta de Recuperación de Datos con IA
//!
//! Programa ligero para recuperar archivos borrados de discos duros, USB y tarjetas SD.
//! Utiliza Rust para máximo rendimiento y acceso de bajo nivel al disco.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai;
mod build_info;
mod core;
mod disk;
mod ui;

use eframe::{egui, App, NativeOptions};
use log::{error, info, LevelFilter};
use std::io::Write;
use std::panic;

fn setup_logging() {
    // Configurar para que los logs salgan por stderr/stdout siempre
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .target(env_logger::Target::Stdout) // Forzar stdout para que sea visible
        .format(|buf, record| {
            writeln!(
                buf,
                "[{}] {:<5} - {}",
                chrono::Local::now().format("%H:%M:%S"),
                record.level(),
                record.args()
            )
        })
        .init();
    
    info!("Logging inicializado correctamente");
}

fn setup_panic_handler() {
    panic::set_hook(Box::new(|panic_info| {
        let msg = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic".to_string()
        };

        let location = if let Some(loc) = panic_info.location() {
            format!("{}:{}:{}", loc.file(), loc.line(), loc.column())
        } else {
            "unknown location".to_string()
        };

        error!("PANIC at {}: {}", location, msg);
    }));
}

fn main() {
    setup_logging();
    setup_panic_handler();

    info!("Iniciando recoverPill v1.0.0");
    info!("Fecha de compilación: {}", build_info::BUILD_TIMESTAMP);
    info!("Plataforma: Windows");

    let title = format!(
        "recoverPill - v1.0.0 (Build: {})",
        build_info::BUILD_TIMESTAMP
    );

    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_title(&title),
        ..Default::default()
    };

    if let Err(e) = eframe::run_native(
        "recoverPill",
        options,
        Box::new(|_cc| Box::new(ui::app::RecoverPillApp::new()) as Box<dyn App>),
    ) {
        error!("Error al iniciar la aplicación: {}", e);
        std::process::exit(1);
    }
}
