//! Módulo de Inteligencia Artificial
//! 
//! Proporciona clasificación y análisis de archivos usando técnicas de IA.

pub mod classifier;
pub mod entropy;

pub use classifier::{AIClassifier, FileClassification, RecoveryPrediction};
pub use entropy::EntropyAnalyzer;
