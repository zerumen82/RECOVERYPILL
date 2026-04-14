//! Analizador de entropía
//!
//! Calcula la entropía de Shannon de los datos para analizar su complejidad.

use log::debug;

/// Analizador de entropía
pub struct EntropyAnalyzer {
    block_size: usize,
}

impl EntropyAnalyzer {
    /// Crea un nuevo analizador de entropía
    pub fn new() -> Self {
        EntropyAnalyzer {
            block_size: 4096, // 4KB blocks
        }
    }

    /// Calcula la entropía de Shannon - optimizado para velocidad
    pub fn calculate(&self, data: &[u8]) -> f64 {
        if data.is_empty() {
            return 0.0;
        }

        // Usar una ventana más pequeña para cálculo rápido (2KB)
        let calc_data = if data.len() > 2048 {
            &data[..2048]
        } else {
            data
        };

        self.calculate_entropy(calc_data)
    }

    /// Calcula la entropía usando el método de Shannon
    fn calculate_entropy(&self, data: &[u8]) -> f64 {
        // Contar frecuencia de cada byte
        let mut freq = [0u64; 256];
        for &byte in data {
            freq[byte as usize] += 1;
        }

        let len = data.len() as f64;
        let mut entropy = 0.0;

        for &count in &freq {
            if count > 0 {
                let p = count as f64 / len;
                entropy -= p * p.log2();
            }
        }

        entropy
    }

    /// Calcula la entropía por bloques
    pub fn calculate_block_entropy(&self, data: &[u8]) -> Vec<f64> {
        let mut entropies = Vec::new();

        for chunk in data.chunks(self.block_size) {
            entropies.push(self.calculate_entropy(chunk));
        }

        entropies
    }

    /// Analiza la entropía y retorna información detallada
    pub fn analyze(&self, data: &[u8]) -> EntropyAnalysis {
        let overall_entropy = self.calculate(data);
        let block_entropies = self.calculate_block_entropy(data);

        let avg_block_entropy = if !block_entropies.is_empty() {
            block_entropies.iter().sum::<f64>() / block_entropies.len() as f64
        } else {
            0.0
        };

        let variance = if block_entropies.len() > 1 {
            let mean = avg_block_entropy;
            block_entropies
                .iter()
                .map(|e| (e - mean).powi(2))
                .sum::<f64>()
                / block_entropies.len() as f64
        } else {
            0.0
        };

        // Clasificación basada en entropía
        let classification = classify_entropy(overall_entropy);

        EntropyAnalysis {
            overall_entropy,
            avg_block_entropy,
            variance,
            classification,
            block_count: block_entropies.len(),
        }
    }

    /// Estima si los datos están encriptados o comprimidos
    pub fn is_encrypted_or_compressed(&self, data: &[u8]) -> bool {
        let entropy = self.calculate(data);

        // Alta entropía (> 7.5) sugiere datos encriptados o muy comprimidos
        entropy > 7.5
    }

    /// Detecta si hay datos aleatorios (posiblemente encriptados)
    pub fn detect_random_data(&self, data: &[u8]) -> f64 {
        let entropy = self.calculate(data);

        // Comparar con entropía máxima teórica
        let max_entropy = 8.0; // log2(256)

        // Porcentaje de "aleatoriedad"
        (entropy / max_entropy) * 100.0
    }
}

/// Análisis detallado de entropía
#[derive(Debug, Clone)]
pub struct EntropyAnalysis {
    pub overall_entropy: f64,
    pub avg_block_entropy: f64,
    pub variance: f64,
    pub classification: EntropyClass,
    pub block_count: usize,
}

/// Clasificación de entropía
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntropyClass {
    VeryLow,  // < 2.0 - Datos muy repetitivos
    Low,      // 2.0 - 4.0 - Datos simples o texto
    Medium,   // 4.0 - 6.0 - Datos estructurados
    High,     // 6.0 - 7.5 - Datos complejos
    VeryHigh, // > 7.5 - Encriptado o muy comprimido
}

impl EntropyClass {
    pub fn description(&self) -> &'static str {
        match self {
            EntropyClass::VeryLow => "Muy repetitivo (posiblemente vacío o muy comprimido)",
            EntropyClass::Low => "Datos simples (texto, código fuente)",
            EntropyClass::Medium => "Datos estructurados (ejecutables, documentos)",
            EntropyClass::High => "Datos complejos (imágenes, audio)",
            EntropyClass::VeryHigh => "Posiblemente encriptado o muy comprimido",
        }
    }
    
    /// Retorna una etiqueta clara y amigable para la UI
    pub fn ui_label(&self) -> &'static str {
        match self {
            EntropyClass::VeryLow => "Muy Simple",
            EntropyClass::Low => "Simple",
            EntropyClass::Medium => "Estructurado",
            EntropyClass::High => "Complejo",
            EntropyClass::VeryHigh => "Comprimido/Cifrado",
        }
    }
    
    /// Retorna un emoji descriptivo para la UI
    pub fn ui_emoji(&self) -> &'static str {
        match self {
            EntropyClass::VeryLow => "📄",
            EntropyClass::Low => "📝",
            EntropyClass::Medium => "📦",
            EntropyClass::High => "🖼️",
            EntropyClass::VeryHigh => "🔒",
        }
    }
    
    /// Retorna un color sugerido para la UI (formato egui)
    pub fn ui_color(&self) -> [f32; 3] {
        match self {
            EntropyClass::VeryLow => [0.6, 0.6, 0.6], // Gris
            EntropyClass::Low => [0.3, 0.7, 0.3],     // Verde
            EntropyClass::Medium => [0.9, 0.7, 0.2],  // Amarillo
            EntropyClass::High => [0.2, 0.6, 1.0],    // Azul
            EntropyClass::VeryHigh => [0.9, 0.3, 0.3], // Rojo
        }
    }
}

/// Retorna una descripción clara de la entropía para mostrar en la UI
/// Reemplaza el valor numérico confuso con lenguaje descriptivo
pub fn entropy_description(entropy_value: f64) -> &'static str {
    let class = classify_entropy(entropy_value);
    class.ui_label()
}

/// Retorna el emoji correspondiente al valor de entropía
pub fn entropy_emoji(entropy_value: f64) -> &'static str {
    let class = classify_entropy(entropy_value);
    class.ui_emoji()
}

/// Retorna el color sugerido para el valor de entropía
pub fn entropy_color(entropy_value: f64) -> [f32; 3] {
    let class = classify_entropy(entropy_value);
    class.ui_color()
}

/// Clasifica la entropía
fn classify_entropy(entropy: f64) -> EntropyClass {
    if entropy < 2.0 {
        EntropyClass::VeryLow
    } else if entropy < 4.0 {
        EntropyClass::Low
    } else if entropy < 6.0 {
        EntropyClass::Medium
    } else if entropy < 7.5 {
        EntropyClass::High
    } else {
        EntropyClass::VeryHigh
    }
}

impl Default for EntropyAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty() {
        let analyzer = EntropyAnalyzer::new();
        let entropy = analyzer.calculate(&[]);
        assert_eq!(entropy, 0.0);
    }

    #[test]
    fn test_uniform() {
        let analyzer = EntropyAnalyzer::new();
        // Todos los bytes iguales - entropía 0
        let data = vec![0u8; 1000];
        let entropy = analyzer.calculate(&data);
        assert_eq!(entropy, 0.0);
    }

    #[test]
    fn test_random() {
        let analyzer = EntropyAnalyzer::new();
        // Datos aleatorios - alta entropía
        let data: Vec<u8> = (0..1000).map(|_| rand_byte()).collect();
        let entropy = analyzer.calculate(&data);
        assert!(entropy > 7.0);
    }

    #[test]
    fn test_entropy_analysis() {
        let analyzer = EntropyAnalyzer::new();

        // Datos simples
        let data = b"Hello World! Hello World! Hello World!";
        let analysis = analyzer.analyze(data);

        assert_eq!(analysis.classification, EntropyClass::Low);
    }

    fn rand_byte() -> u8 {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        (nanos % 256) as u8
    }
}
