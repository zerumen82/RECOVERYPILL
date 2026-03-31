//! Módulo core - Motor de escaneo y recuperación
//!
//! Contains the main scanning engine, file signatures, and recovery logic.

pub mod recovery;
pub mod scanner;
pub mod signatures;

pub use recovery::RecoveryEngine;
pub use scanner::{FoundFile, ScanProgress, ScanResult, Scanner};
pub use signatures::{FileSignature, FileType, SIGNATURE_DATABASE};
