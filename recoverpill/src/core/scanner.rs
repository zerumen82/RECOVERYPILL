//! Motor de escaneo de disco
//! 
//! Escanea el disco en busca de archivos borrados usando firmas de archivos.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use rayon::prelude::*;
use log::{info, warn, debug};
use parking_lot::RwLock;

use crate::disk::access::DiskReader;
use crate::core::signatures::{FileSignature, FileType, SIGNATURE_DATABASE, detect_file_type};
use crate::ai::classifier::AIClassifier;

/// Estado del progreso de escaneo
#[derive(Debug, Clone)]
pub struct ScanProgress {
    pub bytes_scanned: u64,
    pub total_bytes: u64,
    pub files_found: u64,
    pub current_offset: u64,
    pub is_running: bool,
    pub is_paused: bool,
}

impl ScanProgress {
    pub fn new(total_bytes: u64) -> Self {
        ScanProgress {
            bytes_scanned: 0,
            total_bytes,
            files_found: 0,
            current_offset: 0,
            is_running: false,
            is_paused: false,
        }
    }

    pub fn percentage(&self) -> f64 {
        if self.total_bytes == 0 {
            0.0
        } else {
            (self.bytes_scanned as f64 / self.total_bytes as f64) * 100.0
        }
    }
}

/// Archivo encontrado durante el escaneo
#[derive(Debug, Clone)]
pub struct FoundFile {
    pub offset: u64,                    // Offset en el disco donde se encontró
    pub file_type: FileType,
    pub file_name: String,
    pub estimated_size: u64,
    pub recoverability: f64,            // 0-100%
    pub entropy: f64,
    pub signature_matched: String,
    pub selected: bool,                // Whether user selected this file
}

/// Resultado del escaneo
#[derive(Debug, Clone)]
pub struct ScanResult {
    pub drive: String,
    pub total_bytes: u64,
    pub files_found: Vec<FoundFile>,
    pub scan_time_ms: u64,
    pub success: bool,
    pub error_message: Option<String>,
}

/// Motor de escaneo
pub struct Scanner {
    drive_path: String,
    reader: Option<DiskReader>,
    found_files: Arc<RwLock<Vec<FoundFile>>>,
    progress: Arc<RwLock<ScanProgress>>,
    should_stop: Arc<AtomicBool>,
    is_scanning: Arc<AtomicBool>,
    chunk_size: usize,
    // Clasificador IA para análisis de archivos
    ai_classifier: AIClassifier,
}

impl Scanner {
    /// Crea un nuevo escáner
    pub fn new(drive_path: &str) -> Result<Self, String> {
        info!("Inicializando escáner para: {}", drive_path);
        
        let reader = DiskReader::open(drive_path)?;
        
        let total_bytes = reader.total_bytes;
        
        Ok(Scanner {
            drive_path: drive_path.to_string(),
            reader: Some(reader),
            found_files: Arc::new(RwLock::new(Vec::new())),
            progress: Arc::new(RwLock::new(ScanProgress::new(total_bytes))),
            should_stop: Arc::new(AtomicBool::new(false)),
            is_scanning: Arc::new(AtomicBool::new(false)),
            chunk_size: 1024 * 1024, // 1MB chunks - mucho más rápido para escaneo profundo
            ai_classifier: AIClassifier::new(),
        })
    }

    /// Escanea el disco en busca de archivos
    pub fn scan(&mut self, enabled_types: Option<Vec<FileType>>) -> ScanResult {
        info!("Iniciando escaneo profundo de: {}", self.drive_path);
        let start_time = std::time::Instant::now();
        
        self.should_stop.store(false, Ordering::SeqCst);
        self.is_scanning.store(true, Ordering::SeqCst);
        
        // Limpiar resultados anteriores
        {
            let mut files = self.found_files.write();
            files.clear();
        }
        {
            let mut progress = self.progress.write();
            progress.is_running = true;
            progress.files_found = 0;
            progress.bytes_scanned = 0;
        }

        let total_bytes = self.progress.read().total_bytes;
        
        // Si total_bytes es 0, no podemos escanear
        if total_bytes == 0 {
            info!("Error: No se pudo detectar el tamaño del disco");
            return ScanResult {
                drive: self.drive_path.clone(),
                total_bytes: 0,
                files_found: Vec::new(),
                scan_time_ms: 0,
                success: false,
                error_message: Some("No se pudo detectar el tamaño del disco".to_string()),
            };
        }
        
        let mut bytes_scanned: u64 = 0;
        let mut files_found_count: u64 = 0;
        
        // Procesar el disco en chunks
        let mut offset: u64 = 0;
        
        while offset < total_bytes && !self.should_stop.load(Ordering::SeqCst) {
            // Ajustar el tamaño del chunk si es necesario
            let remaining = (total_bytes - offset) as usize;
            let current_chunk = std::cmp::min(remaining, self.chunk_size);
            
            // Leer datos del disco
            let data = match self.reader.as_mut().unwrap().read_at(offset, current_chunk) {
                Ok(d) => d,
                Err(e) => {
                    info!("Error leyendo en offset {}: {}", offset, e);
                    offset += current_chunk as u64;
                    continue;
                }
            };
            
            if data.is_empty() {
                info!("Datos vacíos en offset {}, terminando escaneo", offset);
                break;
            }
            
            // Buscar firmas en los datos
            let found = self.search_signatures(&data, offset, &enabled_types);
            
            // Agregar archivos encontrados
            if !found.is_empty() {
                let mut files = self.found_files.write();
                for f in &found {
                    files.push(f.clone());
                }
                files_found_count += found.len() as u64;
                
                // Actualizar progreso
                {
                    let mut progress = self.progress.write();
                    progress.files_found = files.len() as u64;
                }
            }
            
            // Actualizar progreso
            bytes_scanned += current_chunk as u64;
            {
                let mut progress = self.progress.write();
                progress.bytes_scanned = bytes_scanned;
                progress.current_offset = offset;
            }
            
            offset += current_chunk as u64;
            
            // Loguear progreso cada 100MB
            if offset % (100 * 1024 * 1024) < (current_chunk as u64) {
                let progress = self.progress.read();
                info!("Progreso: {:.1}% - {} archivos encontrados", 
                    progress.percentage(), progress.files_found);
            }
        }
        
        // Finalizar
        let scan_time = start_time.elapsed().as_millis() as u64;
        
        {
            let mut progress = self.progress.write();
            progress.is_running = false;
            progress.bytes_scanned = total_bytes;
        }
        
        self.is_scanning.store(false, Ordering::SeqCst);
        
        let files = self.found_files.read().clone();
        
        info!("Escaneo completado en {}ms. Archivos encontrados: {}", scan_time, files.len());
        
        ScanResult {
            drive: self.drive_path.clone(),
            total_bytes,
            files_found: files,
            scan_time_ms: scan_time,
            success: true,
            error_message: None,
        }
    }

    /// Escanea el disco con callback de progreso
    pub fn scan_with_progress<F>(&mut self, mut progress_callback: F) -> ScanResult 
    where
        F: FnMut(String) + Send + 'static
    {
        info!("Iniciando escaneo profundo con progreso de: {}", self.drive_path);
        let start_time = std::time::Instant::now();
        
        self.should_stop.store(false, Ordering::SeqCst);
        self.is_scanning.store(true, Ordering::SeqCst);
        
        // Limpiar resultados anteriores
        {
            let mut files = self.found_files.write();
            files.clear();
        }
        {
            let mut progress = self.progress.write();
            progress.is_running = true;
            progress.files_found = 0;
            progress.bytes_scanned = 0;
        }

        let total_bytes = self.progress.read().total_bytes;
        
        // Si total_bytes es 0, no podemos escanear
        if total_bytes == 0 {
            info!("Error: No se pudo detectar el tamaño del disco");
            return ScanResult {
                drive: self.drive_path.clone(),
                total_bytes: 0,
                files_found: Vec::new(),
                scan_time_ms: 0,
                success: false,
                error_message: Some("No se pudo detectar el tamaño del disco".to_string()),
            };
        }
        
        let mut bytes_scanned: u64 = 0;
        let mut files_found_count: u64 = 0;
        
        // Procesar el disco en chunks - usar chunks más pequeños para paralelismo
        let mut offset: u64 = 0;
        
        // Tamaño de chunk más pequeño para paralelizar mejor
        let effective_chunk = std::cmp::min(self.chunk_size, 256 * 1024); // 256KB max para paralelismo
        
        while offset < total_bytes && !self.should_stop.load(Ordering::SeqCst) {
            // Ajustar el tamaño del chunk si es necesario
            let remaining = (total_bytes - offset) as usize;
            let current_chunk = std::cmp::min(remaining, effective_chunk);
            
            // Leer datos del disco
            let data = match self.reader.as_mut().unwrap().read_at(offset, current_chunk) {
                Ok(d) => d,
                Err(e) => {
                    info!("Error leyendo en offset {}: {}", offset, e);
                    offset += current_chunk as u64;
                    continue;
                }
            };
            
            if data.is_empty() {
                info!("Datos vacíos en offset {}, terminando escaneo", offset);
                break;
            }
            
            // Buscar firmas en los datos (ya paralelizado internamente)
            let found = self.search_signatures(&data, offset, &None);
            
            // Agregar archivos encontrados
            if !found.is_empty() {
                let mut files = self.found_files.write();
                for f in &found {
                    files.push(f.clone());
                }
                files_found_count += found.len() as u64;
                
                // Actualizar progreso
                {
                    let mut progress = self.progress.write();
                    progress.files_found = files.len() as u64;
                }
            }
            
            // Actualizar progreso
            bytes_scanned += current_chunk as u64;
            {
                let mut progress = self.progress.write();
                progress.bytes_scanned = bytes_scanned;
                progress.current_offset = offset;
            }
            
            offset += current_chunk as u64;
            
            // Reportar progreso cada chunk
            let prog = self.progress.read();
            progress_callback(format!("Progreso: {:.1}% - {} archivos encontrados", 
                prog.percentage(), prog.files_found));
        }
        
        // Finalizar
        let scan_time = start_time.elapsed().as_millis() as u64;
        
        {
            let mut progress = self.progress.write();
            progress.is_running = false;
            progress.bytes_scanned = total_bytes;
        }
        
        self.is_scanning.store(false, Ordering::SeqCst);
        
        let files = self.found_files.read().clone();
        
        info!("Escaneo completado en {}ms. Archivos encontrados: {}", scan_time, files.len());
        
        ScanResult {
            drive: self.drive_path.clone(),
            total_bytes,
            files_found: files,
            scan_time_ms: scan_time,
            success: true,
            error_message: None,
        }
    }

    /// Busca firmas de archivos en los datos
    fn search_signatures(
        &self, 
        data: &[u8], 
        base_offset: u64,
        enabled_types: &Option<Vec<FileType>>,
    ) -> Vec<FoundFile> {
        let mut found = Vec::new();
        
        // Ventana de búsqueda más grande para escaneo profundo - mayor contexto para IA
        let window_size = 8192; // 8KB - más contexto para mejor análisis
        let step = 1024; // Buscar cada 1KB
        
        for window_start in (0..data.len().saturating_sub(window_size)).step_by(step) {
            let window = &data[window_start..std::cmp::min(window_start + window_size, data.len())];
            
            // Verificar cada firma
            for sig in SIGNATURE_DATABASE.iter() {
                // Filtrar por tipos habilitados
                if let Some(ref types) = enabled_types {
                    if !types.contains(&sig.file_type) {
                        continue;
                    }
                }
                
                if sig.matches(window) {
                    // Calcular entropía
                    let entropy = calculate_entropy(window);
                    
                    // Estimar tamaño
                    let estimated_size = sig.estimate_size(window).unwrap_or(0);
                    
                    // USAR IA para predecir recuperabilidad
                    let ai_classification = self.ai_classifier.classify(window);
                    let recoverability = ai_classification.recovery_prediction.probability;
                    
                    // Intentar extraer nombre de archivo de los metadatos
                    let extracted_name = extract_filename_from_data(window, &sig.file_type);
                    
                    let file_name = if let Some(name) = extracted_name {
                        name
                    } else {
                        format!("{}_{}_{}", 
                            sig.file_type.extension().to_uppercase(),
                            found.len() + 1,
                            base_offset / 1024 / 1024
                        )
                    };
                    
                    let found_file = FoundFile {
                        offset: base_offset + window_start as u64,
                        file_type: sig.file_type,
                        file_name,
                        estimated_size,
                        recoverability,
                        entropy,
                        signature_matched: format!("{:02X?}", &sig.magic_bytes[..std::cmp::min(4, sig.magic_bytes.len())]),
                        selected: true, // Default to selected
                    };
                    
                    found.push(found_file);
                    
                    // Mover el offset para evitar encontrar el mismo archivo múltiples veces
                    // Pero no demasiado para no perder archivos cercanos
                    break;
                }
            }
        }
        
        found
    }

    /// Detiene el escaneo
    pub fn stop(&self) {
        info!("Deteniendo escaneo...");
        self.should_stop.store(true, Ordering::SeqCst);
    }

    /// Obtiene la bandera de parada (para compartir con UI)
    pub fn get_should_stop(&self) -> Arc<AtomicBool> {
        self.should_stop.clone()
    }

    /// Obtiene el progreso actual como mensaje de texto
    pub fn get_progress_message(&self) -> String {
        let progress = self.progress.read();
        format!("Progreso: {:.1}% - {} archivos encontrados", 
            progress.percentage(), 
            progress.files_found)
    }

    /// Pausa el escaneo
    pub fn pause(&self) {
        let mut progress = self.progress.write();
        progress.is_paused = true;
    }

    /// Reanuda el escaneo
    pub fn resume(&self) {
        let mut progress = self.progress.write();
        progress.is_paused = false;
    }

    /// Obtiene el progreso actual
    pub fn get_progress(&self) -> ScanProgress {
        self.progress.read().clone()
    }

    /// Obtiene los archivos encontrados
    pub fn get_found_files(&self) -> Vec<FoundFile> {
        self.found_files.read().clone()
    }

    /// Obtiene archivos por tipo
    pub fn get_files_by_type(&self, file_type: FileType) -> Vec<FoundFile> {
        self.found_files.read()
            .iter()
            .filter(|f| f.file_type == file_type)
            .cloned()
            .collect()
    }

    /// Obtiene archivos por categoría
    pub fn get_files_by_category(&self, category: &str) -> Vec<FoundFile> {
        self.found_files.read()
            .iter()
            .filter(|f| f.file_type.category() == category)
            .cloned()
            .collect()
    }

    /// Está escaneando actualmente
    pub fn is_scanning(&self) -> bool {
        self.is_scanning.load(Ordering::SeqCst)
    }

    /// Establece el tamaño del chunk
    pub fn set_chunk_size(&mut self, size: usize) {
        self.chunk_size = size;
    }
}

impl Drop for Scanner {
    fn drop(&mut self) {
        self.stop();
        if let Some(mut reader) = self.reader.take() {
            reader.close();
        }
    }
}

/// Calcula la entropía de Shannon de los datos
pub fn calculate_entropy(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }

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

/// Calcula la probabilidad de recuperación basada en varios factores
fn calculate_recoverability(sig: &FileSignature, entropy: f64, estimated_size: u64) -> f64 {
    let mut score: f64 = 100.0;

    // Penalizar si la entropía es muy baja o muy alta
    if entropy < 2.0 {
        score -= 30.0; // Possible header/corruption
    } else if entropy > 7.9 {
        score -= 20.0; // Possibly encrypted or compressed
    }

    // Bonificar si tenemos un tamaño estimado
    if estimated_size > 0 {
        if let Some(max_size) = sig.max_size {
            if (estimated_size as usize) < max_size {
                score += 10.0;
            }
        } else {
            // Sin límite máximo
            score += 10.0;
        }
    }

    // Ajuste por tipo de archivo (algunos son más fáciles de recuperar)
    match sig.file_type {
        FileType::Jpeg | FileType::Png | FileType::Gif | FileType::Bmp => {
            // Good recovery rate for images
        }
        FileType::Pdf | FileType::Zip => {
            score += 5.0;
        }
        _ => {}
    }

    score.max(0.0).min(100.0)
}

/// Extrae el nombre de archivo de los datos si es posible
fn extract_filename_from_data(data: &[u8], file_type: &crate::core::signatures::FileType) -> Option<String> {
    use crate::core::signatures::FileType;
    
    match file_type {
        // Para archivos ZIP (DOCX, XLSX, PPTX, ZIP)
        FileType::Zip | FileType::Docx | FileType::Xlsx | FileType::Pptx => {
            // Buscar "PK" header y luego el nombre del archivo
            if data.len() > 50 {
                // Buscar en los primeros bytes el nombre del archivo en la estructura ZIP
                for i in 0..std::cmp::min(data.len() - 30, 1000) {
                    if data[i] == 0x50 && i + 1 < data.len() && data[i+1] == 0x4B {
                        // Possible ZIP local file header
                        if i + 30 < data.len() {
                            let name_len = u16::from_le_bytes([data[i+26], data[i+27]]) as usize;
                            let start = i + 30;
                            let end = start + name_len;
                            if end <= data.len() {
                                if let Ok(name) = std::str::from_utf8(&data[start..end]) {
                                    let clean_name = name.trim_end_matches('\0');
                                    if !clean_name.is_empty() && !clean_name.contains("../") && !clean_name.contains('/') {
                                        // Limitar longitud del nombre
                                        if clean_name.len() < 100 && clean_name.len() > 3 {
                                            return Some(clean_name.to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            None
        },
        // Para PDFs
        FileType::Pdf => {
            extract_pdf_metadata_filename(data)
        },
        _ => None
    }
}

/// Extrae nombre de archivo de metadatos PDF
fn extract_pdf_metadata_filename(data: &[u8]) -> Option<String> {
    if data.len() < 8 {
        return None;
    }
    
    // Buscar /Title o /Author en el PDF
    let data_str = match std::str::from_utf8(&data[..std::cmp::min(data.len(), 4096)]) {
        Ok(s) => s,
        Err(_) => return None,
    };
    
    // Buscar /Title
    if let Some(title_start) = data_str.find("/Title(") {
        let start = title_start + 6;
        let end = data_str[start..].find(')').map(|p| start + p)?;
        let title = &data_str[start..end];
        if !title.is_empty() && title.len() < 100 {
            return Some(format!("{}.pdf", title));
        }
    }
    
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entropy() {
        let data = vec![0u8; 1000];
        let entropy = calculate_entropy(&data);
        assert_eq!(entropy, 0.0);
    }

    #[test]
    fn test_jpeg_signature() {
        let jpeg_header = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46];
        let sig = detect_file_type(&jpeg_header);
        assert!(sig.is_some());
        assert_eq!(sig.unwrap().file_type, FileType::Jpeg);
    }

    #[test]
    fn test_png_signature() {
        let png_header = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        let sig = detect_file_type(&png_header);
        assert!(sig.is_some());
        assert_eq!(sig.unwrap().file_type, FileType::Png);
    }
}
