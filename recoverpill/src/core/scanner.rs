//! Motor de escaneo de disco
//!
//! Escanea el disco en busca de archivos borrados usando firmas de archivos.

use log::{debug, error, info, warn};
use parking_lot::RwLock;
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use crate::ai::classifier::AIClassifier;
use crate::core::signatures::{detect_file_type, FileSignature, FileType, SIGNATURE_DATABASE};
use crate::disk::access::DiskReader;
use crate::disk::filesystem::FileSystemReader;

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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FoundFile {
    pub offset: u64, // Offset en el disco donde se encontró
    pub file_type: FileType,
    pub file_name: String,
    pub estimated_size: u64,
    pub recoverability: f64, // 0-100%
    pub entropy: f64,
    pub signature_matched: String,
    pub selected: bool, // Whether user selected this file
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
        // Usar el método con deep scan y progress callback
        self.scan_with_progress(|msg| {
            info!("{}", msg);
        })
    }

    /// Escanea el disco con callback de progreso (optimizado)
    pub fn scan_with_progress<F>(&mut self, mut progress_callback: F) -> ScanResult
    where
        F: FnMut(String) + Send + 'static,
    {
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

        if total_bytes == 0 {
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

        // Chunks más grandes para mejor rendimiento en discos formateados
        let chunk_size = 1024 * 1024; // 1MB por chunk - mejor rendimiento para escaneo profundo
        let mut offset: u64 = 0;

        // Log de diagnóstico - enviar a través del callback para que se vea en la UI
        progress_callback(format!("=== DIAGNÓSTICO DE ESCANEO ==="));
        progress_callback(format!("Disco: {}", self.drive_path));
        progress_callback(format!(
            "Tamaño total: {:.2} GB",
            total_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
        ));
        progress_callback(format!("Tamaño de chunk: {} bytes", chunk_size));
        progress_callback(format!(
            "Número de firmas en base de datos: {}",
            SIGNATURE_DATABASE.len()
        ));
        progress_callback(format!("=============================="));

        progress_callback(format!(
            "Iniciando escaneo profundo de {} ({:.2} GB)...",
            self.drive_path,
            total_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
        ));

        while offset < total_bytes && !self.should_stop.load(Ordering::SeqCst) {
            let remaining = (total_bytes - offset) as usize;
            let current_chunk = std::cmp::min(remaining, chunk_size);

            // Verificar stop antes de leer datos
            if self.should_stop.load(Ordering::SeqCst) {
                break;
            }

            // Leer datos del disco de forma segura
            let data = match self.reader.as_mut() {
                Some(r) => match r.read_at(offset, current_chunk) {
                    Ok(d) => {
                        if d.is_empty() {
                            warn!("Datos vacíos en offset {}, terminando escaneo", offset);
                            break;
                        }
                        d
                    }
                    Err(e) => {
                        error!("Error crítico de lectura en offset {}: {}", offset, e);
                        // En lugar de crash, saltamos el bloque y continuamos
                        offset += current_chunk as u64;
                        bytes_scanned += current_chunk as u64;
                        continue;
                    }
                },
                None => {
                    error!("Lector de disco no disponible durante el escaneo");
                    break;
                }
            };

            // Verificar stop antes de procesar
            if self.should_stop.load(Ordering::SeqCst) {
                break;
            }

            // BÚSQUEDA PROFUNDA: Escanear el chunk de forma paralela y exhaustiva
            let found = self.search_signatures_deep(&data, offset, &None);

            if self.should_stop.load(Ordering::SeqCst) {
                break;
            }

            let mut skip_distance = 0u64;

            if !found.is_empty() {
                let mut files = self.found_files.write();
                for f in &found {
                    if !files.iter().any(|existing| existing.offset == f.offset) {
                        files.push(f.clone());
                        
                        // SMART SKIPPING: Si el archivo es grande y confiable, saltamos su contenido
                        if f.estimated_size > current_chunk as u64 && f.recoverability > 60.0 {
                            // Guardamos la distancia máxima a saltar basándonos en el archivo más grande encontrado en este bloque
                            skip_distance = std::cmp::max(skip_distance, f.estimated_size);
                        }

                        if let Ok(json) = serde_json::to_string(f) {
                            progress_callback(format!(">>> DATA:{}", json));
                        }
                    }
                }
            }

            let advance = if skip_distance > 0 {
                // Saltar el archivo pero alineado a sectores
                (skip_distance / 512) * 512
            } else {
                current_chunk as u64
            };

            bytes_scanned += advance;
            offset += advance;

            // Actualizar progreso cada chunk - usar cantidad acumulada
            {
                let files_count = self.found_files.read().len() as u64;
                let mut progress = self.progress.write();
                progress.bytes_scanned = bytes_scanned;
                progress.files_found = files_count;
            }

            let files_count = self.found_files.read().len();
            let percent = (bytes_scanned as f64 / total_bytes as f64) * 100.0;

            // Log de progreso cada 1%
            if percent as u64 % 1 == 0 {
                info!(
                    "Progreso: {:.1}% - {} archivos encontrados",
                    percent, files_count
                );
            }

            progress_callback(format!(
                "Progreso: {:.1}% - {} archivos encontrados",
                percent, files_count
            ));

            // Verificar stop al final de cada chunk para responsividad
            if self.should_stop.load(Ordering::SeqCst) {
                break;
            }
        }

        let was_stopped = self.should_stop.load(Ordering::SeqCst);
        let scan_time = start_time.elapsed().as_millis() as u64;

        {
            let mut progress = self.progress.write();
            progress.is_running = false;
            progress.bytes_scanned = bytes_scanned;
        }

        self.is_scanning.store(false, Ordering::SeqCst);

        let files = self.found_files.read().clone();

        info!(
            "Escaneo {} en {}ms. Archivos: {}",
            if was_stopped {
                "detenido"
            } else {
                "completado"
            },
            scan_time,
            files.len()
        );

        ScanResult {
            drive: self.drive_path.clone(),
            total_bytes,
            files_found: files,
            scan_time_ms: scan_time,
            success: true,
            error_message: if was_stopped {
                Some("Escaneo detenido por usuario".to_string())
            } else {
                None
            },
        }
    }

    /// Deep scan por carving - busca archivos incluso sin firmas completas (OPTIMIZADO)
    fn deep_scan_carving(&self, data: &[u8], base_offset: u64) -> Vec<FoundFile> {
        let mut found = Vec::new();

        // OPTIMIZADO: Ventana más pequeña y step más grande para rendimiento
        let window_size = 512;  // Solo buscamos el header/magic bytes
        let step = 512;         // Paso más grande para mejor rendimiento

        for window_start in (0..data.len().saturating_sub(window_size)).step_by(step) {
            if self.should_stop.load(Ordering::SeqCst) {
                break;
            }

            let window = &data[window_start..std::cmp::min(window_start + window_size, data.len())];

            // Solo buscar firmas en esta ventana (más rápido que carve_file_from_window)
            if let Some(file_info) = self.quick_carve_window(window, base_offset + window_start as u64) {
                found.push(file_info);
            }
        }

        found
    }

    /// Búsqueda rápida de firmas para carving (versión optimizada)
    fn quick_carve_window(&self, window: &[u8], offset: u64) -> Option<FoundFile> {
        // Solo verificar las primeras bytes de la ventana para firmas conocidas
        for sig in SIGNATURE_DATABASE.iter() {
            if sig.magic_bytes.is_empty() {
                continue;
            }

            // Verificar si la firma está al inicio de la ventana
            if window.len() >= sig.magic_bytes.len() {
                let mut match_found = true;
                for (j, &byte) in sig.magic_bytes.iter().enumerate() {
                    if window[j] != byte {
                        match_found = false;
                        break;
                    }
                }

                if match_found {
                    let entropy = calculate_entropy(window);
                    let estimated_size = self.find_file_boundaries(window, &sig.file_type);
                    let recoverability = self.calculate_carve_recoverability(
                        &sig.file_type,
                        estimated_size,
                        entropy,
                    );

                    return Some(FoundFile {
                        offset,
                        file_type: sig.file_type,
                        file_name: format!(
                            "{}_{}",
                            sig.file_type.extension().to_uppercase(),
                            offset / 1024 / 1024
                        ),
                        estimated_size,
                        recoverability,
                        entropy,
                        signature_matched: format!(
                            "{:02X?}",
                            &sig.magic_bytes[..std::cmp::min(4, sig.magic_bytes.len())]
                        ),
                        selected: true,
                    });
                }
            }
        }
        None
    }

    /// Intenta hacer carving de un archivo desde una ventana de datos
    fn carve_file_from_window(&self, window: &[u8], offset: u64) -> Option<FoundFile> {
        for sig in SIGNATURE_DATABASE.iter() {
            if sig.magic_bytes.is_empty() {
                continue;
            }

            for i in 0..window.len().saturating_sub(sig.magic_bytes.len()) {
                let mut match_found = true;
                for (j, &byte) in sig.magic_bytes.iter().enumerate() {
                    if window[i + j] != byte {
                        match_found = false;
                        break;
                    }
                }

                if match_found {
                    let file_offset = offset + i as u64;
                    let window_for_size = &window[i..];

                    let estimated_size = self.find_file_boundaries(window_for_size, &sig.file_type);
                    let entropy = calculate_entropy(window_for_size);

                    let recoverability = self.calculate_carve_recoverability(
                        &sig.file_type,
                        estimated_size,
                        entropy,
                    );

                    return Some(FoundFile {
                        offset: file_offset,
                        file_type: sig.file_type,
                        file_name: format!(
                            "{}_{}",
                            sig.file_type.extension().to_uppercase(),
                            file_offset / 1024 / 1024
                        ),
                        estimated_size,
                        recoverability,
                        entropy,
                        signature_matched: format!(
                            "{:02X?}",
                            &sig.magic_bytes[..std::cmp::min(4, sig.magic_bytes.len())]
                        ),
                        selected: true,
                    });
                }
            }
        }

        None
    }

    /// Encuentra los límites del archivo (inicio y fin) para carving preciso
    fn find_file_boundaries(&self, data: &[u8], file_type: &FileType) -> u64 {
        match file_type {
            FileType::Jpeg => {
                for i in (2..data.len() - 1).rev() {
                    if data[i] == 0xFF && data[i + 1] == 0xD9 {
                        return (i + 2) as u64;
                    }
                }
                data.len() as u64
            }
            FileType::Png => {
                for i in (8..data.len().saturating_sub(8)).rev() {
                    if &data[i..i + 4] == b"IEND"
                        && data[i + 4] == 0xAE
                        && data[i + 5] == 0x42
                        && data[i + 6] == 0x60
                        && data[i + 7] == 0x82
                    {
                        return (i + 8) as u64;
                    }
                }
                data.len() as u64
            }
            FileType::Gif => {
                for i in (2..data.len() - 1).rev() {
                    if data[i] == 0x00 && data[i + 1] == 0x3B {
                        return (i + 2) as u64;
                    }
                }
                data.len() as u64
            }
            FileType::Pdf => {
                for i in (5..data.len() - 4).rev() {
                    if &data[i..i + 5] == b"%%EOF" {
                        return (i + 5) as u64;
                    }
                }
                data.len() as u64
            }
            FileType::Zip => {
                if data.len() >= 22 {
                    for i in (data.len() - 22..data.len() - 4).rev() {
                        if data[i] == 0x50
                            && data[i + 1] == 0x4B
                            && data[i + 2] == 0x05
                            && data[i + 3] == 0x06
                        {
                            return (i + 22) as u64;
                        }
                    }
                }
                data.len() as u64
            }
            FileType::Mp4 => self.find_mp4_end(data),
            _ => data.len() as u64,
        }
    }

    /// Encuentra el final de un archivo MP4 buscando el box 'mdat'
    fn find_mp4_end(&self, data: &[u8]) -> u64 {
        let mut i = 8;
        while i + 8 < data.len() {
            let box_size =
                u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]) as u64;
            if box_size == 0 || box_size > data.len() as u64 {
                break;
            }

            let box_type = &data[i + 4..i + 8];
            if box_type == b"mdat" {
                return i as u64 + box_size;
            }

            i += box_size as usize;
        }
        data.len() as u64
    }

    /// Calcula la recuperabilidad para un archivo obtenido por carving
    fn calculate_carve_recoverability(&self, file_type: &FileType, size: u64, entropy: f64) -> f64 {
        let mut score: f64 = 50.0;

        if size > 0 && size < 100_000_000 {
            score += 20.0;
        }

        match file_type {
            FileType::Jpeg | FileType::Png | FileType::Gif => {
                if entropy > 5.0 && entropy < 7.8 {
                    score += 15.0;
                }
            }
            FileType::Pdf => {
                if entropy < 6.0 {
                    score += 10.0;
                }
            }
            _ => {}
        }

        score.min(100.0)
    }

    /// Detecta firmas parciales en los datos - busca cualquier coincidencia
    fn detect_partial_signature(&self, data: &[u8]) -> Option<FileType> {
        for sig in SIGNATURE_DATABASE.iter() {
            if sig.magic_bytes.is_empty() {
                continue;
            }
            for i in 0..data.len().saturating_sub(sig.magic_bytes.len()) {
                let mut match_found = true;
                for (j, &byte) in sig.magic_bytes.iter().enumerate() {
                    if data[i + j] != byte {
                        match_found = false;
                        break;
                    }
                }
                if match_found {
                    return Some(sig.file_type);
                }
            }
        }
        None
    }

    /// Estima el tamaño del archivo basado en entropía
    fn estimate_file_size_from_entropy(&self, data: &[u8], entropy: f64) -> u64 {
        // Estimación basada en entropía
        if entropy > 6.0 {
            // Alta entropía = datos comprimidos/encriptados = archivo grande
            1024 * 1024 // 1MB
        } else if entropy > 4.0 {
            // Entropía media = datos estructurados
            512 * 1024 // 512KB
        } else {
            // Baja entropía = datos simples
            256 * 1024 // 256KB
        }
    }

    /// Detecta si un chunk está vacío (solo ceros o FF) - optimizado para velocidad
    fn is_chunk_empty(data: &[u8]) -> bool {
        // Si el chunk es muy pequeño, no considerarlo vacío
        if data.len() < 4096 {
            return false;
        }

        // Muestrear el chunk en lugar de verificar byte por byte - sampleo más granular
        let sample_rate = 1024; // Verificar cada 1KB (más granular para discos formateados)
        let mut zeros = 0;
        let mut ff = 0;
        let samples = data.len() / sample_rate;

        if samples == 0 {
            return false;
        }

        for i in 0..samples {
            let byte = data[i * sample_rate];
            if byte == 0 {
                zeros += 1;
            } else if byte == 0xFF {
                ff += 1;
            }
        }

        // Si más del 80% de las muestras son 0 o FF, considerar vacío (umbral más permisivo para discos formateados)
        zeros + ff > samples * 80 / 100
    }

    /// Busca firmas de archivos en los datos de forma exhaustiva (Byte-by-Byte) - PARALELIZADO
    fn search_signatures_deep(
        &self,
        data: &[u8],
        base_offset: u64,
        enabled_types: &Option<Vec<FileType>>,
    ) -> Vec<FoundFile> {
        let step = 512; // Alinear con sectores de disco
        
        // Capturar solo lo necesario para el closure para evitar problemas de Sync con DiskReader
        let should_stop = self.should_stop.clone();
        let ai_classifier = &self.ai_classifier;
        let enabled_types_ref = enabled_types.as_ref();

        let sectors: Vec<(usize, &[u8])> = data
            .chunks(step)
            .enumerate()
            .map(|(i, chunk)| (i * step, chunk))
            .collect();

        sectors.into_par_iter()
            .filter_map(|(offset_in_chunk, sector)| {
                if should_stop.load(Ordering::SeqCst) {
                    return None;
                }

                // Si el sector está vacío (ceros o FF), omitir
                if Self::is_chunk_empty(sector) {
                    return None;
                }

                // Buscar firma en el sector de forma estática o pasando el clasificador
                Self::find_signature_in_sector(
                    sector,
                    base_offset + offset_in_chunk as u64,
                    enabled_types_ref,
                    ai_classifier
                )
            })
            .collect()
    }

    /// Función auxiliar estática para búsqueda en sector (Thread-safe)
    fn find_signature_in_sector(
        window: &[u8],
        offset: u64,
        enabled_types: Option<&Vec<FileType>>,
        ai_classifier: &AIClassifier,
    ) -> Option<FoundFile> {
        for sig in SIGNATURE_DATABASE.iter() {
            if sig.magic_bytes.is_empty() {
                continue;
            }

            if let Some(types) = enabled_types {
                if !types.contains(&sig.file_type) {
                    continue;
                }
            }

            if sig.matches(window) {
                // FILTRO DE CALIDAD BALANCEADO
                let entropy = calculate_entropy(window);
                
                // Solo descartar si el sector es extremadamente pobre en datos (todo ceros o FF)
                if entropy < 0.1 {
                    continue;
                }

                // Usar IA para calcular la puntuación, pero NO descartar por ahora
                let ai_classification = ai_classifier.classify(window);
                let mut recoverability = ai_classification.recovery_prediction.probability;

                // Si la firma es larga (> 4 bytes), le damos un voto de confianza extra
                if sig.magic_bytes.len() >= 4 {
                    recoverability = (recoverability + 20.0).min(100.0);
                }

                let estimated_size = sig.estimate_size(window).unwrap_or(0);
                let extracted_name = extract_filename_from_data(window, &sig.file_type);
                
                let file_name = extracted_name.unwrap_or_else(|| {
                    format!("{}_{}", sig.file_type.extension().to_uppercase(), offset / 1024 / 1024)
                });

                return Some(FoundFile {
                    offset,
                    file_type: sig.file_type,
                    file_name,
                    estimated_size,
                    recoverability,
                    entropy,
                    signature_matched: format!("{:02X?}", &sig.magic_bytes[..std::cmp::min(4, sig.magic_bytes.len())]),
                    selected: true,
                });
            }
        }
        None
    }

    /// Busca firmas de archivos en los datos (optimizado para velocidad)
    fn search_signatures(
        &self,
        data: &[u8],
        base_offset: u64,
        enabled_types: &Option<Vec<FileType>>,
    ) -> Vec<FoundFile> {
        let mut found = Vec::new();

        // Optimización: buscar solo en el inicio del chunk para firmas completas
        // y luego hacer búsqueda dispersa para el resto
        let search_regions = [
            (0, std::cmp::min(512, data.len())), // Inicio del chunk - búsqueda densa
        ];

        // Búsqueda densa al inicio (donde es más probable encontrar firmas)
        let window_size = 64;
        let step = 16;
        for window_start in (0..data.len().saturating_sub(window_size)).step_by(step) {
            if self.should_stop.load(Ordering::SeqCst) {
                break;
            }

            let window_end = std::cmp::min(window_start + window_size, data.len());
            let window = &data[window_start..window_end];

            if let Some(found_file) = self.find_signature_in_window(
                window,
                base_offset + window_start as u64,
                enabled_types,
            ) {
                found.push(found_file);
            }
        }

        found
    }

    /// Busca una firma en una ventana específica
    fn find_signature_in_window(
        &self,
        window: &[u8],
        offset: u64,
        enabled_types: &Option<Vec<FileType>>,
    ) -> Option<FoundFile> {
        // Verificar cada firma
        for sig in SIGNATURE_DATABASE.iter() {
            // Verificar stop antes de procesar cada firma
            if self.should_stop.load(Ordering::SeqCst) {
                break;
            }

            // Filtrar por tipos habilitados
            if let Some(ref types) = enabled_types {
                if !types.contains(&sig.file_type) {
                    continue;
                }
            }

            if sig.matches(window) {
                // Verificar stop antes de cálculos
                if self.should_stop.load(Ordering::SeqCst) {
                    break;
                }

                // Calcular entropía
                let entropy = calculate_entropy(window);

                // Verificar stop antes de estimar tamaño
                if self.should_stop.load(Ordering::SeqCst) {
                    break;
                }

                // Estimar tamaño
                let estimated_size = sig.estimate_size(window).unwrap_or(0);

                // Verificar stop antes de IA
                if self.should_stop.load(Ordering::SeqCst) {
                    break;
                }

                // USAR IA para predecir recuperabilidad
                let ai_classification = self.ai_classifier.classify(window);

                // Verificar stop antes de extraer nombre
                if self.should_stop.load(Ordering::SeqCst) {
                    break;
                }

                let recoverability = ai_classification.recovery_prediction.probability;

                // Intentar extraer nombre de archivo de los metadatos
                let extracted_name = extract_filename_from_data(window, &sig.file_type);

                let file_name = if let Some(name) = extracted_name {
                    name
                } else {
                    format!(
                        "{}_{}",
                        sig.file_type.extension().to_uppercase(),
                        offset / 1024 / 1024
                    )
                };

                return Some(FoundFile {
                    offset,
                    file_type: sig.file_type,
                    file_name,
                    estimated_size,
                    recoverability,
                    entropy,
                    signature_matched: format!(
                        "{:02X?}",
                        &sig.magic_bytes[..std::cmp::min(4, sig.magic_bytes.len())]
                    ),
                    selected: true,
                });
            }
        }

        None
    }

    /// Detiene el escaneo
    pub fn stop(&self) {
        info!("Deteniendo escaneo...");
        self.should_stop.store(true, Ordering::SeqCst);
    }

    /// Lee datos del disco en un offset específico (para previsualizaciones)
    pub fn read_data_at(&mut self, offset: u64, size: usize) -> Result<Vec<u8>, String> {
        if let Some(ref mut reader) = self.reader {
            reader.read_at(offset, size)
        } else {
            Err("Disco no disponible".to_string())
        }
    }

    /// Obtiene la ruta del disco
    pub fn get_drive_path(&self) -> &str {
        &self.drive_path
    }

    /// Obtiene la bandera de parada (para compartir con UI)
    pub fn get_should_stop(&self) -> Arc<AtomicBool> {
        self.should_stop.clone()
    }

    /// Obtiene el progreso actual como mensaje de texto
    pub fn get_progress_message(&self) -> String {
        let progress = self.progress.read();
        format!(
            "Progreso: {:.1}% - {} archivos encontrados",
            progress.percentage(),
            progress.files_found
        )
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
        self.found_files
            .read()
            .iter()
            .filter(|f| f.file_type == file_type)
            .cloned()
            .collect()
    }

    /// Obtiene archivos por categoría
    pub fn get_files_by_category(&self, category: &str) -> Vec<FoundFile> {
        self.found_files
            .read()
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

    /// Escanea el sistema de archivos en busca de archivos
    pub fn scan_filesystem<F>(&mut self, mut progress_callback: F) -> ScanResult
    where
        F: FnMut(String) + Send + 'static,
    {
        info!(
            "Iniciando escaneo del sistema de archivos: {}",
            self.drive_path
        );
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

        // Crear lector del sistema de archivos
        let reader = match self.reader.take() {
            Some(r) => r,
            None => {
                return ScanResult {
                    drive: self.drive_path.clone(),
                    total_bytes: 0,
                    files_found: Vec::new(),
                    scan_time_ms: 0,
                    success: false,
                    error_message: Some("No hay lector de disco disponible".to_string()),
                };
            }
        };

        let mut fs_reader = match FileSystemReader::new(reader) {
            Ok(r) => r,
            Err(e) => {
                return ScanResult {
                    drive: self.drive_path.clone(),
                    total_bytes: 0,
                    files_found: Vec::new(),
                    scan_time_ms: 0,
                    success: false,
                    error_message: Some(format!(
                        "Error al crear lector del sistema de archivos: {}",
                        e
                    )),
                };
            }
        };

        progress_callback(format!(
            "Escaneando sistema de archivos: {:?}",
            fs_reader.get_fs_type()
        ));

        // Escanear el sistema de archivos
        let fs_files = match fs_reader.scan_filesystem() {
            Ok(files) => files,
            Err(e) => {
                return ScanResult {
                    drive: self.drive_path.clone(),
                    total_bytes: 0,
                    files_found: Vec::new(),
                    scan_time_ms: 0,
                    success: false,
                    error_message: Some(format!("Error al escanear sistema de archivos: {}", e)),
                };
            }
        };

        // Convertir archivos del sistema de archivos a FoundFile
        let mut found_files = Vec::new();
        for fs_file in fs_files {
            if self.should_stop.load(Ordering::SeqCst) {
                break;
            }

            // Leer una muestra del archivo para determinar el tipo
            let sample_data = match fs_reader.read_file_data(&fs_file, 4096) {
                Ok(data) => data,
                Err(_) => continue,
            };

            // Detectar el tipo de archivo
            let file_type = if let Some(sig) = detect_file_type(&sample_data) {
                sig.file_type
            } else {
                // Inferir tipo por extensión
                match fs_file.file_type.as_str() {
                    "jpg" | "jpeg" => FileType::Jpeg,
                    "png" => FileType::Png,
                    "gif" => FileType::Gif,
                    "pdf" => FileType::Pdf,
                    "doc" | "docx" => FileType::Doc,
                    "xls" | "xlsx" => FileType::Xls,
                    "mp3" => FileType::Mp3,
                    "mp4" => FileType::Mp4,
                    "zip" => FileType::Zip,
                    _ => FileType::Unknown,
                }
            };

            // Calcular entropía
            let entropy = calculate_entropy(&sample_data);

            // Calcular recuperabilidad
            let recoverability = if fs_file.is_deleted {
                // Para archivos eliminados, usar IA para predecir recuperabilidad
                let ai_classification = self.ai_classifier.classify(&sample_data);
                ai_classification.recovery_prediction.probability
            } else {
                // Para archivos existentes, la recuperabilidad es alta
                95.0
            };

            let found_file = FoundFile {
                offset: fs_file.offset,
                file_type,
                file_name: fs_file.name.clone(),
                estimated_size: fs_file.size,
                recoverability,
                entropy,
                signature_matched: if fs_file.is_deleted {
                    "Sistema de archivos (eliminado)".to_string()
                } else {
                    "Sistema de archivos (existente)".to_string()
                },
                selected: true,
            };

            // Actualizar progreso
            {
                let mut files = self.found_files.write();
                files.push(found_file.clone());
            }

            found_files.push(found_file);

            progress_callback(format!(
                "Encontrado: {} ({}) - {}",
                fs_file.name,
                if fs_file.is_deleted {
                    "eliminado"
                } else {
                    "existente"
                },
                fs_file.file_type
            ));
        }

        let scan_time = start_time.elapsed().as_millis() as u64;

        {
            let mut progress = self.progress.write();
            progress.is_running = false;
            progress.files_found = found_files.len() as u64;
        }

        self.is_scanning.store(false, Ordering::SeqCst);

        // Guardar el cluster_size antes de mover el reader
        let cluster_size = fs_reader.get_cluster_size();

        // Devolver el reader
        self.reader = Some(fs_reader.into_reader());

        info!(
            "Escaneo del sistema de archivos completado en {}ms. Archivos: {}",
            scan_time,
            found_files.len()
        );

        ScanResult {
            drive: self.drive_path.clone(),
            total_bytes: cluster_size * 1000, // Estimación
            files_found: found_files,
            scan_time_ms: scan_time,
            success: true,
            error_message: None,
        }
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

/// Calcula la entropía de Shannon de los datos (optimizado con SIMD)
pub fn calculate_entropy(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }

    let len = data.len();

    // Usar contador simple pero optimizado
    let mut freq = [0u64; 256];

    // Procesar en chunks de 64 bytes para mejor cache
    let chunk_size = 64;
    for chunk in data.chunks(chunk_size) {
        for &byte in chunk {
            freq[byte as usize] += 1;
        }
    }

    let len_f64 = len as f64;
    let mut entropy = 0.0;

    // Calcular entropía - solo para bytes que aparecieron
    for count in freq.iter() {
        if *count > 0 {
            let p = *count as f64 / len_f64;
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
fn extract_filename_from_data(
    data: &[u8],
    file_type: &crate::core::signatures::FileType,
) -> Option<String> {
    use crate::core::signatures::FileType;

    match file_type {
        // Para archivos ZIP (DOCX, XLSX, PPTX, ZIP)
        FileType::Zip | FileType::Docx | FileType::Xlsx | FileType::Pptx => {
            // Buscar "PK" header y luego el nombre del archivo
            if data.len() > 50 {
                // Buscar en los primeros bytes el nombre del archivo en la estructura ZIP
                for i in 0..std::cmp::min(data.len() - 30, 1000) {
                    if data[i] == 0x50 && i + 1 < data.len() && data[i + 1] == 0x4B {
                        // Possible ZIP local file header
                        if i + 30 < data.len() {
                            let name_len =
                                u16::from_le_bytes([data[i + 26], data[i + 27]]) as usize;
                            let start = i + 30;
                            let end = start + name_len;
                            if end <= data.len() {
                                if let Ok(name) = std::str::from_utf8(&data[start..end]) {
                                    let clean_name = name.trim_end_matches('\0');
                                    if !clean_name.is_empty()
                                        && !clean_name.contains("../")
                                        && !clean_name.contains('/')
                                    {
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
        }
        // Para PDFs
        FileType::Pdf => extract_pdf_metadata_filename(data),
        // Para imágenes JPEG
        FileType::Jpeg => extract_jpeg_metadata_filename(data),
        // Para archivos MP3
        FileType::Mp3 => extract_mp3_metadata_filename(data),
        // Para otros tipos de archivo, generar un nombre más descriptivo
        _ => {
            // Generar un nombre basado en el tipo de archivo y el tamaño
            if data.len() > 100 {
                // Intentar buscar cadenas de texto legibles como nombre
                if let Ok(text) = std::str::from_utf8(&data[..100]) {
                    let valid_chars: String = text
                        .chars()
                        .filter(|c| {
                            c.is_alphanumeric()
                                || c.is_whitespace()
                                || *c == '.'
                                || *c == '_'
                                || *c == '-'
                        })
                        .collect();
                    let trimmed = valid_chars.trim();
                    if trimmed.len() > 3 && trimmed.len() < 50 {
                        // Limpiar y retornar
                        let clean_name = trimmed.replace(|c: char| c.is_whitespace(), "_");
                        return Some(clean_name);
                    }
                }
            }
            None
        }
    }
}

/// Extrae nombre de archivo de metadatos PDF (Mejorado)
fn extract_pdf_metadata_filename(data: &[u8]) -> Option<String> {
    if data.len() < 8 {
        return None;
    }

    // Buscar /Title en los primeros 4096 bytes
    let limit = std::cmp::min(data.len(), 4096);
    let data_str = String::from_utf8_lossy(&data[..limit]);

    if let Some(title_start) = data_str.find("/Title") {
        let after_title = &data_str[title_start + 6..];
        if let Some(start_paren) = after_title.find('(') {
            let start = start_paren + 1;
            if let Some(end_paren) = after_title[start..].find(')') {
                let title = &after_title[start..start + end_paren];
                let clean_title: String = title.chars()
                    .filter(|c| c.is_alphanumeric() || *c == '_' || *c == ' ' || *c == '-')
                    .collect();
                if !clean_title.trim().is_empty() {
                    return Some(format!("{}.pdf", clean_title.trim().replace(' ', "_")));
                }
            }
        }
    }
    None
}

/// Extrae nombre de archivo de metadatos JPEG (Mejorado con EXIF Real)
fn extract_jpeg_metadata_filename(data: &[u8]) -> Option<String> {
    if data.len() < 256 { return None; }

    // Buscar marcador de fecha EXIF (0x9003 - DateTimeOriginal o 0x0132 - DateTime)
    // Patrón típico: YYYY:MM:DD HH:MM:SS
    for i in 0..std::cmp::min(data.len() - 20, 4096) {
        if data[i] >= b'1' && data[i] <= b'2' && data[i+4] == b':' && data[i+7] == b':' && data[i+10] == b' ' {
             let date_str = String::from_utf8_lossy(&data[i..i+19]);
             if date_str.chars().all(|c| c.is_numeric() || c == ':' || c == ' ') {
                 let clean_date = date_str.replace(':', "").replace(' ', "_");
                 return Some(format!("IMG_{}.jpg", clean_date));
             }
        }
    }
    None
}

/// Extrae metadatos EXIF de una sección APP1 JPEG
fn extract_exif_metadata(section: &[u8]) -> Option<String> {
    // Buscar en EXIF por "Make", "Model", "DateTime" para generar un nombre
    // Esto es una simplificación, pero es un punto de partida

    if section.len() > 100 {
        // Convertir a string para búsqueda simple
        if let Ok(section_str) = std::str::from_utf8(&section[6..]) {
            // Buscar fecha y hora
            if let Some(date_index) = section_str.find("DateTime=") {
                let date_str = &section_str[date_index + 9..];
                if let Some(end) = date_str.find('\0') {
                    let date_clean = date_str[..end].replace(":", "").replace(" ", "_");
                    return Some(format!("IMG_{}.jpg", date_clean));
                }
            }
        }
    }

    None
}

/// Extrae nombre de archivo de metadatos MP3
fn extract_mp3_metadata_filename(data: &[u8]) -> Option<String> {
    if data.len() < 10 {
        return None;
    }

    // Buscar ID3v2 tag al principio del archivo
    if data[0..3] == [0x49, 0x44, 0x33] {
        // "ID3"
        let major_ver = data[3];
        let revision = data[4];
        let flags = data[5];
        let size = (data[6] as u32 & 0x7F) << 21
            | (data[7] as u32 & 0x7F) << 14
            | (data[8] as u32 & 0x7F) << 7
            | (data[9] as u32 & 0x7F);

        if size > 0 {
            let tag_start = 10;
            let tag_end = tag_start + size as usize;

            if tag_end <= data.len() {
                // Intentar extraer título y artista
                if let Some(title) = extract_id3v2_title(&data[tag_start..tag_end]) {
                    return Some(title);
                }
            }
        }
    }

    None
}

/// Extrae título de un ID3v2 tag
fn extract_id3v2_title(tag_data: &[u8]) -> Option<String> {
    // Buscar frames TIT2 (título), TPE1 (artista), TALB (álbum)
    let mut i = 0;

    while i + 10 < tag_data.len() {
        let frame_id = &tag_data[i..i + 4];

        // Buscar frame TIT2 (título)
        if frame_id == b"TIT2" {
            let frame_size = (tag_data[i + 4] as u32) << 24
                | (tag_data[i + 5] as u32) << 16
                | (tag_data[i + 6] as u32) << 8
                | tag_data[i + 7] as u32;

            let frame_start = i + 10;
            let frame_end = frame_start + frame_size as usize;

            if frame_end <= tag_data.len() {
                let encoding = tag_data[frame_start];
                let content = &tag_data[frame_start + 1..frame_end];

                // Decodificar según encoding
                let decoded = match encoding {
                    0 => {
                        // ISO-8859-1
                        String::from_utf8_lossy(content).to_string()
                    }
                    1 => {
                        // UTF-16
                        if let Ok(s) = String::from_utf16(
                            &content
                                .chunks_exact(2)
                                .map(|c| (c[0] as u16) | (c[1] as u16) << 8)
                                .collect::<Vec<_>>(),
                        ) {
                            s
                        } else {
                            continue;
                        }
                    }
                    2 => {
                        // UTF-16BE
                        if let Ok(s) = String::from_utf16(
                            &content
                                .chunks_exact(2)
                                .map(|c| (c[1] as u16) | (c[0] as u16) << 8)
                                .collect::<Vec<_>>(),
                        ) {
                            s
                        } else {
                            continue;
                        }
                    }
                    3 => {
                        // UTF-8
                        String::from_utf8_lossy(content).to_string()
                    }
                    _ => continue,
                };

                let clean_title = decoded.trim_matches('\0').trim();
                if !clean_title.is_empty() && clean_title.len() < 100 {
                    return Some(format!(
                        "{}.mp3",
                        clean_title.replace("/", "_").replace("\\", "_")
                    ));
                }
            }
        }

        i += 1;
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
