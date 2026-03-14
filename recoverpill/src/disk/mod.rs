//! Módulo de acceso a disco
//! 
//! Proporciona funciones para detectar unidades y acceder al disco a bajo nivel.

pub mod drive_info;
pub mod access;

pub use drive_info::{DriveInfo, DriveType};
pub use access::DiskReader;
