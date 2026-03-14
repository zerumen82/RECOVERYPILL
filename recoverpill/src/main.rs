//! recoverPill - Herramienta de Recuperación de Datos con IA
//! 
//! Programa ligero para recuperar archivos borrados de discos duros, USB y tarjetas SD.
//! Utiliza Rust para máximo rendimiento y acceso de bajo nivel al disco.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod disk;
mod core;
mod ai;
mod ui;

use log::{info, error, LevelFilter};
use std::panic;
use std::io::Write;
use eframe::{egui, NativeOptions, App};

fn setup_logging() {
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .format(|buf, record| {
            writeln!(
                buf,
                "[{} {} {}:{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                record.file().unwrap_or("unknown"),
                record.line().unwrap_or(0),
                record.args()
            )
        })
        .init();
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
    info!("Plataforma: Windows");
    
    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_title("recoverPill - Recuperación de Datos con IA"),
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
