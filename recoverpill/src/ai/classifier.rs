//! Clasificador de archivos con IA
//!
//! Utiliza análisis de entropía, firmas, footers, y validación estructural
//! para clasificar archivos y predecir recuperabilidad con alta precisión.

use crate::ai::entropy::EntropyAnalyzer;
use crate::core::signatures::{FileSignature, FileType, SIGNATURE_DATABASE, FOOTER_DATABASE};
use log::{debug, info};
use std::collections::HashMap;

/// Predicción de recuperación de un archivo
#[derive(Debug, Clone)]
pub struct RecoveryPrediction {
    pub probability: f64, // 0-100%
    pub is_fragmented: bool,
    pub is_corrupted: bool,
    pub confidence: f64, // 0-100%
    pub recommendation: String,
}

/// Clasificación de un archivo
#[derive(Debug, Clone)]
pub struct FileClassification {
    pub file_type: FileType,
    pub confidence: f64,
    pub entropy: f64,
    pub is_valid: bool,
    pub recovery_prediction: RecoveryPrediction,
}

/// Clasificador de archivos con IA
pub struct AIClassifier {
    entropy_analyzer: EntropyAnalyzer,
    type_patterns: HashMap<FileType, Vec<Pattern>>,
    footer_patterns: HashMap<FileType, Vec<u8>>,
}

/// Patrón de archivo conocido
#[derive(Debug, Clone)]
struct Pattern {
    magic_bytes: Vec<u8>,
    offset: usize,
    weight: f64,
}

impl AIClassifier {
    /// Crea un nuevo clasificador
    pub fn new() -> Self {
        info!("Inicializando clasificador IA avanzado");

        let entropy_analyzer = EntropyAnalyzer::new();

        let mut type_patterns = HashMap::new();
        let mut footer_patterns = HashMap::new();

        // Patrones de JPEG
        type_patterns.insert(
            FileType::Jpeg,
            vec![Pattern { magic_bytes: vec![0xFF, 0xD8, 0xFF], offset: 0, weight: 1.0 }],
        );
        // Patrones de PNG
        type_patterns.insert(
            FileType::Png,
            vec![Pattern { magic_bytes: vec![0x89, 0x50, 0x4E, 0x47], offset: 0, weight: 1.0 }],
        );
        // Patrones de PDF
        type_patterns.insert(
            FileType::Pdf,
            vec![Pattern { magic_bytes: vec![0x25, 0x50, 0x44, 0x46], offset: 0, weight: 1.0 }],
        );
        // APK (ZIP header)
        type_patterns.insert(
            FileType::Apk,
            vec![Pattern { magic_bytes: vec![0x50, 0x4B, 0x03, 0x04], offset: 0, weight: 1.0 }],
        );
        // DEX
        type_patterns.insert(
            FileType::Dex,
            vec![Pattern { magic_bytes: vec![0x64, 0x65, 0x78, 0x0A, 0x30, 0x33, 0x35, 0x00], offset: 0, weight: 1.0 }],
        );
        // SQLite
        type_patterns.insert(
            FileType::Db,
            vec![Pattern { magic_bytes: vec![0x53, 0x51, 0x4C, 0x69, 0x74, 0x65], offset: 0, weight: 1.0 }],
        );

        // Footers conocidos para validación estructural
        // Iteramos directamente sobre las claves del FOOTER_DATABASE
        for ft in [FileType::Jpeg, FileType::Png, FileType::Gif, FileType::Pdf, FileType::Zip, FileType::Apk] {
            if let Some(footer) = FOOTER_DATABASE.get(&ft) {
                footer_patterns.insert(ft, footer.to_vec());
            }
        }

        AIClassifier {
            entropy_analyzer,
            type_patterns,
            footer_patterns,
        }
    }

    /// Clasifica datos crudos - optimizado para velocidad
    pub fn classify(&self, data: &[u8]) -> FileClassification {
        let (file_type, signature_confidence) = self.detect_file_type(data);

        let entropy = if data.len() > 2048 {
            self.entropy_analyzer.calculate(&data[..2048])
        } else {
            self.entropy_analyzer.calculate(data)
        };

        let is_valid = match file_type {
            FileType::Jpeg | FileType::Png | FileType::Gif | FileType::Bmp |
            FileType::Pdf | FileType::Apk | FileType::Dex | FileType::Db => {
                self.validate_data(data, &file_type)
            }
            _ => true,
        };

        let recovery_prediction = self.predict_recovery(data, &file_type, entropy, is_valid);

        let confidence = (signature_confidence * 0.6 + recovery_prediction.confidence * 0.4).min(100.0);

        FileClassification {
            file_type,
            confidence,
            entropy,
            is_valid,
            recovery_prediction,
        }
    }

    /// Detecta el tipo de archivo
    fn detect_file_type(&self, data: &[u8]) -> (FileType, f64) {
        for sig in SIGNATURE_DATABASE.iter() {
            if sig.matches(data) {
                return (sig.file_type, 90.0);
            }
        }

        // Clasificación por entropía + heurística
        let entropy = self.entropy_analyzer.calculate(data);
        if entropy < 0.5 {
            return (FileType::Unknown, 10.0);
        }

        // Heurística: detectar texto plano
        if data.len() > 20 {
            let printable = data.iter().take(256).filter(|&&b| b.is_ascii_graphic() || b == b' ' || b == b'\n' || b == b'\r' || b == b'\t').count();
            let ratio = printable as f64 / std::cmp::min(data.len(), 256) as f64;
            if ratio > 0.8 && entropy < 5.0 {
                return (FileType::Text, 40.0);
            }
        }

        (FileType::Unknown, 20.0)
    }

    /// Valida datos usando estructura específica por tipo
    fn validate_data(&self, data: &[u8], file_type: &FileType) -> bool {
        if data.len() < 16 { return false; }
        match file_type {
            FileType::Jpeg => self.validate_jpeg(data),
            FileType::Png => self.validate_png(data),
            FileType::Gif => self.validate_gif(data),
            FileType::Bmp => self.validate_bmp(data),
            FileType::Pdf => self.validate_pdf(data),
            FileType::Apk => self.validate_apk(data),
            FileType::Dex => self.validate_dex(data),
            FileType::Db => self.validate_sqlite(data),
            _ => true,
        }
    }

    fn validate_jpeg(&self, data: &[u8]) -> bool {
        if data.len() < 4 { return false; }
        // SOI marker FF D8
        if !data.starts_with(&[0xFF, 0xD8]) { return false; }
        // Buscar EOI marker FF D9
        data.windows(2).any(|w| w == [0xFF, 0xD9])
    }

    fn validate_png(&self, data: &[u8]) -> bool {
        data.len() >= 8 && data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])
            && data.windows(12).any(|w| &w[4..8] == b"IEND")
    }

    fn validate_gif(&self, data: &[u8]) -> bool {
        (data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a")) && data.len() >= 10
    }

    fn validate_bmp(&self, data: &[u8]) -> bool {
        data.len() >= 2 && data.starts_with(&[0x42, 0x4D])
    }

    fn validate_pdf(&self, data: &[u8]) -> bool {
        data.len() >= 5 && data.starts_with(b"%PDF-")
    }

    fn validate_apk(&self, data: &[u8]) -> bool {
        // APK es ZIP: verificar header PK\x03\x04
        if !data.starts_with(&[0x50, 0x4B, 0x03, 0x04]) { return false; }
        // Buscar AndroidManifest.xml dentro (primeros 64KB)
        let search_limit = std::cmp::min(data.len(), 65536);
        data[..search_limit].windows(19).any(|w| {
            w == b"AndroidManifest.xml"
        })
    }

    fn validate_dex(&self, data: &[u8]) -> bool {
        if data.len() < 12 { return false; }
        // Verificar header DEX
        let dex_magic = &data[0..8];
        dex_magic == b"dex\n035\0" || dex_magic == b"dex\n036\0" || dex_magic == b"dex\n037\0"
    }

    fn validate_sqlite(&self, data: &[u8]) -> bool {
        data.len() >= 16 && data.starts_with(b"SQLite format 3\0")
    }

    /// Predice probabilidad de recuperación con heurística avanzada
    fn predict_recovery(
        &self,
        data: &[u8],
        file_type: &FileType,
        entropy: f64,
        is_valid: bool,
    ) -> RecoveryPrediction {
        let mut probability: f64 = 80.0;
        let mut is_fragmented = false;
        let mut is_corrupted = false;
        let mut confidence: f64 = 70.0;

        // Análisis de entropía
        if entropy < 0.5 {
            probability -= 60.0;
            is_corrupted = true;
            confidence = 20.0;
        } else if entropy < 2.0 {
            probability -= 30.0;
            confidence = 50.0;
        } else if entropy > 7.9 {
            probability -= 15.0;
            confidence = 65.0;
        } else if entropy > 5.0 && entropy < 7.5 {
            probability += 5.0;
            confidence = 85.0;
        }

        // Análisis específico por tipo
        match file_type {
            FileType::Jpeg => {
                if data.len() > 4 && data[0] == 0xFF && data[1] == 0xD8 {
                    probability += 10.0;
                    if data.iter().take(2048).any(|&b| b == 0xDB || b == 0xC4) {
                        probability += 10.0;
                        confidence += 10.0;
                    }
                    // Verificar EOI
                    if data.windows(2).any(|w| w == [0xFF, 0xD9]) {
                        probability += 10.0;
                    } else {
                        probability -= 10.0;
                        is_fragmented = true;
                    }
                } else {
                    probability -= 30.0;
                    is_corrupted = true;
                }
            }
            FileType::Png => {
                if data.len() > 8 && &data[0..4] == b"\x89PNG" {
                    probability += 10.0;
                } else {
                    probability -= 40.0;
                    is_corrupted = true;
                }
            }
            FileType::Gif | FileType::Bmp => {
                probability += 15.0;
                confidence += 10.0;
            }
            FileType::Pdf | FileType::Zip | FileType::Apk => {
                probability += 8.0;
                confidence += 5.0;
                if file_type == &FileType::Pdf && !data.windows(5).any(|w| w == b"%%EOF") {
                    probability -= 20.0;
                    is_fragmented = true;
                }
            }
            FileType::Dex => {
                if data.len() > 8 {
                    let dex_magic = &data[0..8];
                    if dex_magic == b"dex\n035\0" || dex_magic == b"dex\n036\0" || dex_magic == b"dex\n037\0" {
                        probability += 15.0;
                        confidence += 10.0;
                    }
                }
            }
            FileType::Db => {
                if data.len() >= 16 && data.starts_with(b"SQLite format 3\0") {
                    probability += 20.0;
                    confidence += 15.0;
                }
            }
            FileType::Mp4 | FileType::Avi | FileType::MkV | FileType::ThreeGp => {
                probability -= 5.0;
                is_fragmented = true;
            }
            FileType::Text => {
                probability += 5.0;
                confidence += 10.0;
            }
            _ => {}
        }

        // Validación estructural
        if !is_valid && *file_type != FileType::Unknown {
            probability -= 20.0;
            is_corrupted = true;
            confidence -= 15.0;
        }

        // Penalización por tamaño sospechoso
        if matches!(file_type.category(), "Imágenes") {
            if data.len() < 5_000 {
                probability *= 0.5;
                is_corrupted = true;
            } else if data.len() < 50_000 {
                probability *= 0.8;
            }
        }

        probability = probability.max(0.0).min(100.0);
        confidence = confidence.max(0.0).min(100.0);

        let recommendation = if probability > 85.0 {
            "Alta probabilidad de recuperación exitosa".to_string()
        } else if probability > 65.0 {
            "Buena probabilidad de recuperación".to_string()
        } else if probability > 40.0 {
            "Recuperación posible pero archivo puede estar fragmentado".to_string()
        } else if probability > 20.0 {
            "Archivo muy probablemente corrupto o fragmentado".to_string()
        } else {
            "Baja probabilidad de recuperación".to_string()
        };

        RecoveryPrediction {
            probability,
            is_fragmented,
            is_corrupted,
            confidence,
            recommendation,
        }
    }

    /// Analiza un archivo y retorna su clasificación
    pub fn analyze(&self, data: &[u8], offset: u64) -> FileClassification {
        debug!("Analizando datos en offset: {}", offset);
        self.classify(data)
    }

    /// Obtiene estadísticas de clasificación
    pub fn get_statistics(&self, classifications: &[FileClassification]) -> ClassificationStats {
        let mut stats = ClassificationStats {
            total: classifications.len(),
            by_type: HashMap::new(),
            average_confidence: 0.0,
            average_recoverability: 0.0,
            valid_files: 0,
            corrupted_files: 0,
        };

        if classifications.is_empty() {
            return stats;
        }

        let mut total_confidence = 0.0;
        let mut total_recoverability = 0.0;

        for c in classifications {
            let type_name = c.file_type.display_name();
            *stats.by_type.entry(type_name.to_string()).or_insert(0) += 1;
            total_confidence += c.confidence;
            total_recoverability += c.recovery_prediction.probability;
            if c.is_valid {
                stats.valid_files += 1;
            } else {
                stats.corrupted_files += 1;
            }
        }

        stats.average_confidence = total_confidence / classifications.len() as f64;
        stats.average_recoverability = total_recoverability / classifications.len() as f64;
        stats
    }

    /// Evalúa si los datos son recuperables mediante carving
    pub fn can_carve(&self, data: &[u8]) -> bool {
        if data.len() < 4 { return false; }
        // Verificar si tiene magic bytes reconocibles
        SIGNATURE_DATABASE.iter().any(|sig| sig.matches(data))
    }

    /// Calcula una puntuación heurística de integridad
    pub fn integrity_score(&self, data: &[u8]) -> f64 {
        if data.is_empty() { return 0.0; }

        let entropy = self.entropy_analyzer.calculate(data);
        let null_ratio = data.iter().filter(|&&b| b == 0).count() as f64 / data.len() as f64;
        let ff_ratio = data.iter().filter(|&&b| b == 0xFF).count() as f64 / data.len() as f64;

        let mut score: f64 = 50.0;

        // Penalizar exceso de ceros o FFs (sectores vacíos/corruptos)
        if null_ratio > 0.9 || ff_ratio > 0.9 {
            score -= 40.0;
        } else if null_ratio > 0.5 || ff_ratio > 0.5 {
            score -= 20.0;
        }

        // Bonificar entropía saludable
        if entropy > 3.0 && entropy < 7.5 {
            score += 20.0;
        }

        // Verificar magic bytes
        if self.can_carve(data) {
            score += 20.0;
        }

        score.max(0.0).min(100.0)
    }
}

/// Estadísticas de clasificación
#[derive(Debug)]
pub struct ClassificationStats {
    pub total: usize,
    pub by_type: HashMap<String, usize>,
    pub average_confidence: f64,
    pub average_recoverability: f64,
    pub valid_files: usize,
    pub corrupted_files: usize,
}

impl Default for AIClassifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classifier() {
        let classifier = AIClassifier::new();
        let jpeg_data = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01];
        let classification = classifier.classify(&jpeg_data);
        assert_eq!(classification.file_type, FileType::Jpeg);
        assert!(classification.confidence > 50.0);
    }

    #[test]
    fn test_pdf() {
        let classifier = AIClassifier::new();
        let pdf_data = b"%PDF-1.4\n1 0 obj\n<<\n>>\nendobj";
        let classification = classifier.classify(pdf_data);
        assert_eq!(classification.file_type, FileType::Pdf);
    }

    #[test]
    fn test_dex() {
        let classifier = AIClassifier::new();
        let dex_data = b"dex\n035\0...";
        let classification = classifier.classify(dex_data);
        assert_eq!(classification.file_type, FileType::Dex);
    }

    #[test]
    fn test_sqlite() {
        let classifier = AIClassifier::new();
        let sqlite_data = b"SQLite format 3\0...";
        let classification = classifier.classify(sqlite_data);
        assert_eq!(classification.file_type, FileType::Db);
    }
}
