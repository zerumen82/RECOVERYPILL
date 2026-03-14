//! Módulo core - Motor de escaneo y recuperación
//! 
//! Contains the main scanning engine, file signatures, and recovery logic.

pub mod scanner;
pub mod signatures;
pub mod recovery;

pub use scanner::{Scanner, ScanResult, FoundFile, ScanProgress};
pub use signatures::{FileSignature, FileType, SIGNATURE_DATABASE};
pub use recovery::RecoveryEngine;
