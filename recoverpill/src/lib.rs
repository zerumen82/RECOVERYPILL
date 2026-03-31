//! recoverPill - Librería principal
//! 
//! Módulos para recuperación de datos:
//! - disk: Acceso a disco y unidades
//! - core: Motor de escaneo y recuperación
//! - ai: Clasificación con IA
//! - ui: Interfaz gráfica

pub mod disk;
pub mod core;
pub mod ai;
pub mod ui;
pub mod build_info;

pub use disk::drive_info::DriveInfo;
pub use core::scanner::ScanResult;
pub use ai::classifier::FileClassification;
