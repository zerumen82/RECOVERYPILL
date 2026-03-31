//! Módulo de acceso a disco
//!
//! Proporciona funciones para detectar unidades y acceder al disco a bajo nivel.

pub mod access;
pub mod drive_info;
pub mod filesystem;

pub use access::DiskReader;
pub use drive_info::{DriveInfo, DriveType};
pub use filesystem::{FileEntry, FileSystemReader, FileSystemType};
