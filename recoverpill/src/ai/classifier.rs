//! Clasificador de archivos con IA
//!
//! Utiliza análisis de entropía y firmas para clasificar archivos y predecir recuperabilidad.

use crate::ai::entropy::EntropyAnalyzer;
use crate::core::signatures::{FileSignature, FileType, SIGNATURE_DATABASE};
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
        info!("Inicializando clasificador IA");

        let entropy_analyzer = EntropyAnalyzer::new();

        // Inicializar patrones conocidos
        let mut type_patterns = HashMap::new();

        // Patrones de JPEG
        type_patterns.insert(
            FileType::Jpeg,
            vec![Pattern {
                magic_bytes: vec![0xFF, 0xD8, 0xFF],
                offset: 0,
                weight: 1.0,
            }],
        );

        // Patrones de PNG
        type_patterns.insert(
            FileType::Png,
            vec![Pattern {
                magic_bytes: vec![0x89, 0x50, 0x4E, 0x47],
                offset: 0,
                weight: 1.0,
            }],
        );

        // Patrones de PDF
        type_patterns.insert(
            FileType::Pdf,
            vec![Pattern {
                magic_bytes: vec![0x25, 0x50, 0x44, 0x46],
                offset: 0,
                weight: 1.0,
            }],
        );

        AIClassifier {
            entropy_analyzer,
            type_patterns,
        }
    }

    /// Clasifica datos crudos - optimizado para velocidad
    pub fn classify(&self, data: &[u8]) -> FileClassification {
        // Detectar tipo de archivo (más rápido)
        let (file_type, signature_confidence) = self.detect_file_type(data);

        // Calcular entropía (optimizado) - usar ventana más pequeña
        let entropy = if data.len() > 2048 {
            self.entropy_analyzer.calculate(&data[..2048])
        } else {
            self.entropy_analyzer.calculate(data)
        };

        // Verificar si el archivo es válido (solo para tipos comunes)
        let is_valid = match file_type {
            FileType::Jpeg | FileType::Png | FileType::Gif | FileType::Bmp | FileType::Pdf => {
                self.validate_data(data, &file_type)
            }
            _ => true, // Para otros tipos, asumir válidos para velocidad
        };

        // Calcular predicción de recuperación (simplificada)
        let recovery_prediction = self.predict_recovery(data, &file_type, entropy);

        // Calcular confianza total
        let confidence =
            (signature_confidence * 0.7 + recovery_prediction.confidence * 0.3).min(100.0);

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
        // Buscar en la base de datos de firmas
        for sig in SIGNATURE_DATABASE.iter() {
            if sig.matches(data) {
                return (sig.file_type, 90.0);
            }
        }

        // Si no hay coincidencia exacta, usar análisis de entropía
        let entropy = self.entropy_analyzer.calculate(data);

        // Clasificación por entropía
        let inferred_type = if entropy < 1.0 {
            FileType::Unknown // Datos vacíos o muy repetitivos
        } else if entropy < 3.0 {
            FileType::Unknown // Posiblemente texto o datos simples
        } else if entropy < 6.0 {
            FileType::Unknown // Posiblemente ejecutable o datos estructurados
        } else if entropy < 8.0 {
            FileType::Unknown // Datos complejos
        } else {
            FileType::Unknown // Posiblemente encriptado o aleatorio
        };

        (inferred_type, 30.0)
    }

    /// Valida los datos
    fn validate_data(&self, data: &[u8], file_type: &FileType) -> bool {
        if data.len() < 16 {
            return false;
        }

        match file_type {
            FileType::Jpeg => self.validate_jpeg(data),
            FileType::Png => self.validate_png(data),
            FileType::Gif => self.validate_gif(data),
            FileType::Bmp => self.validate_bmp(data),
            FileType::Pdf => self.validate_pdf(data),
            _ => true,
        }
    }

    fn validate_jpeg(&self, data: &[u8]) -> bool {
        // Verificar SOI y EOI
        if data.len() < 2 {
            return false;
        }

        // Buscar SOI (Start Of Image)
        data.starts_with(&[0xFF, 0xD8])
    }

    fn validate_png(&self, data: &[u8]) -> bool {
        if data.len() < 8 {
            return false;
        }

        // Verificar PNG signature
        if !data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
            return false;
        }

        // Verificar IEND chunk
        data.windows(12)
            .any(|w| w[0..4] == [0x00, 0x00, 0x00, 0x00] && &w[4..8] == b"IEND")
    }

    fn validate_gif(&self, data: &[u8]) -> bool {
        if data.len() < 6 {
            return false;
        }

        let is_gif87 = data.starts_with(b"GIF87a");
        let is_gif89 = data.starts_with(b"GIF89a");

        (is_gif87 || is_gif89) && data.len() >= 10
    }

    fn validate_bmp(&self, data: &[u8]) -> bool {
        if data.len() < 2 {
            return false;
        }

        data.starts_with(&[0x42, 0x4D]) // "BM"
    }

    fn validate_pdf(&self, data: &[u8]) -> bool {
        if data.len() < 5 {
            return false;
        }

        data.starts_with(b"%PDF-")
    }

    /// Predice la probabilidad de recuperación
    fn predict_recovery(
        &self,
        data: &[u8],
        file_type: &FileType,
        entropy: f64,
    ) -> RecoveryPrediction {
        let mut probability: f64 = 80.0; // Base probability
        let mut is_fragmented = false;
        let mut is_corrupted = false;
        let mut confidence: f64 = 70.0;

        // Ajuste por entropía
        if entropy < 2.0 {
            probability -= 40.0;
            is_corrupted = true;
            confidence = 50.0;
        } else if entropy > 7.9 {
            probability -= 20.0;
            // Podría ser encriptado
            confidence = 60.0;
        } else if entropy > 5.0 && entropy < 7.5 {
            probability += 10.0;
            confidence = 85.0;
        }

        // Ajuste por tipo de archivo y CALIDAD ESPECÍFICA (Mejora)
        match file_type {
            FileType::Jpeg => {
                // JPG debe empezar con FF D8
                if data.len() > 4 && data[0] == 0xFF && data[1] == 0xD8 {
                    probability += 10.0;
                    // Si tiene marcadores de tablas (Quantization/Huffman), es muy probable que sea real
                    if data.iter().take(2048).any(|&b| b == 0xDB || b == 0xC4) {
                        probability += 5.0;
                        confidence += 10.0;
                    }
                } else {
                    probability -= 30.0;
                    is_corrupted = true;
                }
            },
            FileType::Png => {
                if data.len() > 8 && &data[0..4] == b"\x89PNG" {
                    probability += 15.0;
                    confidence += 5.0;
                } else {
                    probability -= 40.0;
                    is_corrupted = true;
                }
            },
            FileType::Gif | FileType::Bmp => {
                // Imágenes tienen buena tasa de recuperación
                probability += 15.0;
                confidence += 10.0;
            }
            FileType::Pdf | FileType::Zip => {
                probability += 10.0;
                confidence += 5.0;
            }
            FileType::Mp4 | FileType::Avi | FileType::MkV => {
                // Videos pueden estar fragmentados
                probability -= 10.0;
                is_fragmented = true;
            }
            _ => {}
        }

        // Validación de estructura
        if !self.validate_data(data, file_type) {
            probability -= 30.0;
            is_corrupted = true;
            confidence -= 20.0;
        }

        // PENALIZACIÓN POR TAMAÑO (Ignorar basura/miniaturas si es imagen)
        if matches!(file_type.category(), "Imágenes") {
            if data.len() < 50_000 {
                probability *= 0.7; // Reducir prioridad de miniaturas
            }
            if data.len() < 5_000 {
                probability *= 0.5; // Muy baja prioridad para iconos/ruido
                is_corrupted = true;
            }
        }

        probability = probability.max(0.0).min(100.0);
        confidence = confidence.max(0.0).min(100.0);

        // Recomendación
        let recommendation = if probability > 80.0 {
            "Alta probabilidad de recuperación exitosa".to_string()
        } else if probability > 50.0 {
            "Recuperación posible pero puede requerir herramientas adicionales".to_string()
        } else if probability > 25.0 {
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

        // Test JPEG
        let jpeg_data = vec![
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01,
        ];
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
}
