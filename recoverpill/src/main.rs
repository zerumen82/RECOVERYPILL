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

    // Load window icon from embedded resource
    let icon_data = load_window_icon();

    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_title(&title)
            .with_icon(icon_data),
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

/// Load window icon from the embedded capsule icon
fn load_window_icon() -> egui::IconData {
    // The icon is embedded via Windows resources, so we load it from file for the window
    let icon_path = "capsule.ico";
    
    // Try to load from file first (for development)
    if let Ok(bytes) = std::fs::read(icon_path) {
        if let Ok(img) = image::load_from_memory(&bytes) {
            let rgba = img.to_rgba8();
            let (width, height) = rgba.dimensions();
            return egui::IconData {
                rgba: rgba.into_raw(),
                width,
                height,
            };
        }
    }
    
    // Fallback: create a simple default icon
    create_default_icon()
}

/// Create a simple default icon as fallback
fn create_default_icon() -> egui::IconData {
    let size = 32u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    
    // Fill with blue color
    for y in 0..size {
        for x in 0..size {
            let idx = ((y * size + x) * 4) as usize;
            rgba[idx] = 30;      // R
            rgba[idx + 1] = 144; // G
            rgba[idx + 2] = 255; // B
            rgba[idx + 3] = 255; // A
        }
    }
    
    egui::IconData {
        rgba,
        width: size,
        height: size,
    }
}
