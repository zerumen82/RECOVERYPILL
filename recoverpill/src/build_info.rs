//! Módulo de información de build
//!
//! Incluye la información generada por build.rs durante la compilación

// Incluir el archivo generado por build.rs
include!(concat!(env!("OUT_DIR"), "/build_info.rs"));
