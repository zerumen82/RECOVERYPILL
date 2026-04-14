//! Motor de recuperación de archivos
//!
//! Implementa las funciones para recuperar archivos del disco.

use log::{error, info, warn};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::core::scanner::FoundFile;
use crate::disk::access::DiskReader;

/// Calcula un hash simple de los primeros bytes de un archivo para detección de duplicados
/// Usamos los primeros 4KB para un balance entre precisión y velocidad
pub fn calculate_content_hash(data: &[u8]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    // Usar los primeros 4KB o menos si el archivo es pequeño
    let hash_data = if data.len() > 4096 {
        &data[..4096]
    } else {
        data
    };
    
    let mut hasher = DefaultHasher::new();
    hash_data.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

/// Motor de recuperación
pub struct RecoveryEngine {
    output_dir: PathBuf,
}

impl RecoveryEngine {
    /// Crea un nuevo motor de recuperación
    pub fn new(output_dir: &Path) -> Result<Self, String> {
        // Crear directorio de salida si no existe
        if !output_dir.exists() {
            fs::create_dir_all(output_dir)
                .map_err(|e| format!("Error al crear directorio de salida: {}", e))?;
        }

        info!("Motor de recuperación inicializado en: {:?}", output_dir);

        Ok(RecoveryEngine {
            output_dir: output_dir.to_path_buf(),
        })
    }

    /// Recupera un archivo del disco de forma eficiente mediante streaming (bloques de 1MB)
    pub fn recover_file(
        &self,
        reader: &mut DiskReader,
        found_file: &FoundFile,
    ) -> Result<PathBuf, String> {
        let extension = found_file.file_type.extension();
        let file_name = format!(
            "{}_0x{:X}.{}",
            found_file.file_type.extension().to_uppercase(),
            found_file.offset,
            extension
        );

        let category_dir = self.output_dir.join(found_file.file_type.category());
        if !category_dir.exists() {
            let _ = fs::create_dir_all(&category_dir);
        }

        let file_path = category_dir.join(&file_name);

        // Determinar tamaño a leer
        let total_to_read = self.calculate_read_size(found_file) as usize;
        
        // Para archivos pequeños (< 1MB), leer todo de una vez
        if total_to_read <= 1024 * 1024 {
            match reader.read_at(found_file.offset, total_to_read) {
                Ok(data) => {
                    if !data.is_empty() {
                        fs::write(&file_path, &data)
                            .map_err(|e| format!("Error al escribir archivo: {}", e))?;
                        return Ok(file_path);
                    }
                }
                Err(e) => {
                    warn!("Error leyendo archivo pequeño: {}", e);
                }
            }
        }

        // Para archivos grandes, usar streaming con buffer reutilizado
        let mut file = File::create(&file_path)
            .map_err(|e| format!("Error al crear archivo de salida: {}", e))?;

        let mut offset = found_file.offset;
        let mut bytes_written = 0usize;

        // Buffer optimizado de 4MB para mejor rendimiento en discos modernos
        let chunk_size = 4 * 1024 * 1024; // 4MB
        let mut buffer = vec![0u8; chunk_size];

        while bytes_written < total_to_read {
            let remaining = total_to_read - bytes_written;
            let current_read = std::cmp::min(chunk_size, remaining);

            match reader.read_at(offset, current_read) {
                Ok(data) => {
                    if data.is_empty() { break; }

                    file.write_all(&data)
                        .map_err(|e| format!("Error escribiendo datos: {}", e))?;

                    bytes_written += data.len();
                    offset += data.len() as u64;

                    if data.len() < current_read { break; }
                }
                Err(e) => {
                    warn!("Error de lectura en offset {}: {}. Saltando.", offset, e);
                    break;
                }
            }
        }

        info!("Archivo recuperado: {:?} ({} bytes)", file_name, bytes_written);
        Ok(file_path)
    }

    /// Calcula el tamaño de lectura basado en el tipo de archivo
    fn calculate_read_size(&self, found_file: &FoundFile) -> usize {
        use crate::core::signatures::FileType;

        // Si tenemos un tamaño estimado válido, usarlo
        if found_file.estimated_size > 0 {
            return found_file.estimated_size as usize;
        }

        // Tamaños máximos razonables por tipo de archivo
        match found_file.file_type {
            FileType::Jpeg | FileType::Png | FileType::Gif | FileType::Webp => 25 * 1024 * 1024, // 25MB
            FileType::Bmp | FileType::Tiff | FileType::Raw | FileType::Psd => 100 * 1024 * 1024, // 100MB
            
            FileType::Mp4 | FileType::Avi | FileType::MkV | FileType::Mov | FileType::Wmv => 1024 * 1024 * 1024, // 1GB
            
            FileType::Mp3 | FileType::Wav | FileType::Flac | FileType::Ogg => 50 * 1024 * 1024, // 50MB
            
            FileType::Pdf | FileType::Doc | FileType::Docx | FileType::Xls | FileType::Xlsx => 50 * 1024 * 1024, // 50MB
            
            FileType::Zip | FileType::Rar | FileType::SevenZip => 500 * 1024 * 1024, // 500MB (Límite de seguridad)
            
            FileType::Exe | FileType::Msi => 200 * 1024 * 1024, // 200MB
            
            FileType::Unknown => 10 * 1024 * 1024, // 10MB
            _ => 50 * 1024 * 1024,
        }
    }

    /// Busca el final real del archivo para formatos conocidos
    fn find_file_end(&self, data: &[u8], file_type: crate::core::signatures::FileType) -> Vec<u8> {
        use crate::core::signatures::FileType;

        if data.is_empty() {
            return data.to_vec();
        }

        match file_type {
            // JPEG: buscar FFD9 (End of Image)
            FileType::Jpeg => {
                for i in 0..data.len().saturating_sub(1) {
                    if data[i] == 0xFF && data[i + 1] == 0xD9 {
                        return data[..=i + 1].to_vec();
                    }
                }
                // Si no se encuentra EOF, devolver todos los datos
                data.to_vec()
            }

            // PNG: buscar IEND
            FileType::Png => {
                let iend_marker = b"IEND";
                for i in 0..data.len().saturating_sub(4) {
                    if &data[i..i + 4] == iend_marker {
                        // IEND chunk tiene 12 bytes (4 length + 4 type + 4 CRC)
                        let end_pos = std::cmp::min(i + 12, data.len());
                        return data[..end_pos].to_vec();
                    }
                }
                data.to_vec()
            }

            // GIF: buscar 00 3B (Trailer)
            FileType::Gif => {
                for i in 0..data.len().saturating_sub(1) {
                    if data[i] == 0x00 && data[i + 1] == 0x3B {
                        return data[..=i + 1].to_vec();
                    }
                }
                data.to_vec()
            }

            // BMP: buscar final basado en el tamaño del archivo en el header
            FileType::Bmp => {
                if data.len() >= 14 {
                    // El tamaño del archivo está en bytes 2-5 (little endian)
                    let file_size =
                        u32::from_le_bytes([data[2], data[3], data[4], data[5]]) as usize;
                    if file_size > 0 && file_size <= data.len() {
                        return data[..file_size].to_vec();
                    }
                }
                data.to_vec()
            }

            // TIFF: buscar final basado en el último IFD
            FileType::Tiff => {
                // TIFF es complejo, devolver todos los datos por ahora
                data.to_vec()
            }

            // MP4/MOV: buscar moov atom
            FileType::Mp4 | FileType::Mov => {
                let moov_marker = b"moov";
                for i in 0..data.len().saturating_sub(4) {
                    if &data[i..i + 4] == moov_marker {
                        // Buscar el final del átomo moov
                        if i + 8 < data.len() {
                            let atom_size = u32::from_be_bytes([
                                data[i - 4],
                                data[i - 3],
                                data[i - 2],
                                data[i - 1],
                            ]) as usize;
                            let end_pos = std::cmp::min(i - 4 + atom_size, data.len());
                            return data[..end_pos].to_vec();
                        }
                    }
                }
                data.to_vec()
            }

            // AVI: buscar final basado en el header
            FileType::Avi => {
                if data.len() >= 12 {
                    // AVI tiene el tamaño del archivo en bytes 4-7
                    let file_size =
                        u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
                    if file_size > 0 && file_size <= data.len() {
                        return data[..file_size].to_vec();
                    }
                }
                data.to_vec()
            }

            // MKV/WebM: buscar Cluster final
            FileType::MkV | FileType::WebM => {
                // MKV es complejo, buscar el último cluster
                let cluster_marker = &[0x1F, 0x43, 0xB6, 0x75]; // Cluster ID
                let mut last_cluster_pos = None;

                for i in 0..data.len().saturating_sub(4) {
                    if &data[i..i + 4] == cluster_marker {
                        last_cluster_pos = Some(i);
                    }
                }

                if let Some(pos) = last_cluster_pos {
                    // Buscar el siguiente cluster o final
                    for i in (pos + 4)..data.len().saturating_sub(4) {
                        if &data[i..i + 4] == cluster_marker {
                            return data[..i].to_vec();
                        }
                    }
                }
                data.to_vec()
            }

            // MP3: buscar último frame sync
            FileType::Mp3 => {
                let sync_marker = 0xFF;
                let mut last_sync_pos = None;

                for i in 0..data.len().saturating_sub(1) {
                    if data[i] == sync_marker && (data[i + 1] & 0xE0) == 0xE0 {
                        last_sync_pos = Some(i);
                    }
                }

                if let Some(pos) = last_sync_pos {
                    // Calcular el final del último frame (aproximadamente 4KB por frame)
                    let end_pos = std::cmp::min(pos + 4096, data.len());
                    return data[..end_pos].to_vec();
                }
                data.to_vec()
            }

            // WAV: buscar final basado en el header
            FileType::Wav => {
                if data.len() >= 44 {
                    // WAV tiene el tamaño del archivo en bytes 4-7
                    let file_size =
                        u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
                    if file_size > 0 && file_size <= data.len() {
                        return data[..file_size].to_vec();
                    }
                }
                data.to_vec()
            }

            // FLAC: buscar STREAMINFO block
            FileType::Flac => {
                if data.starts_with(b"fLaC") {
                    // FLAC comienza con "fLaC", buscar el final del stream
                    // Por simplicidad, devolver todos los datos
                    data.to_vec()
                } else {
                    data.to_vec()
                }
            }

            // PDF: buscar %%EOF
            FileType::Pdf => {
                let eof_marker = b"%%EOF";
                let start = if data.len() >= 5 { data.len() - 5 } else { 0 };
                for i in (start..data.len()).rev() {
                    if &data[i..i + 5] == eof_marker {
                        return data[..i + 5].to_vec();
                    }
                }
                data.to_vec()
            }

            // Para otros formatos, devolver todos los datos
            _ => data.to_vec(),
        }
    }

    /// Valida que los datos del archivo son coherentes
    fn validate_file_data(
        &self,
        data: &[u8],
        file_type: crate::core::signatures::FileType,
    ) -> bool {
        use crate::core::signatures::FileType;

        if data.len() < 4 {
            return false;
        }

        match file_type {
            FileType::Jpeg => {
                // JPEG debe empezar con FFD8FF
                if !data.starts_with(&[0xFF, 0xD8, 0xFF]) {
                    return false;
                }
                // Intentar decodificar para verificar integridad
                image::load_from_memory(data).is_ok()
            }
            FileType::Png => {
                // PNG debe empezar con 89504E47
                if !data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
                    return false;
                }
                // Intentar decodificar para verificar integridad
                image::load_from_memory(data).is_ok()
            }
            FileType::Gif => {
                // GIF debe empezar con GIF87a o GIF89a
                if !data.starts_with(b"GIF87a") && !data.starts_with(b"GIF89a") {
                    return false;
                }
                // Intentar decodificar para verificar integridad
                image::load_from_memory(data).is_ok()
            }
            FileType::Bmp => {
                // BMP debe empezar con 424D
                if !data.starts_with(&[0x42, 0x4D]) {
                    return false;
                }
                // Intentar decodificar para verificar integridad
                image::load_from_memory(data).is_ok()
            }
            FileType::Webp => {
                // WebP debe tener RIFF en inicio y WEBP en offset 8
                if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"WEBP" {
                    return false;
                }
                // Intentar decodificar para verificar integridad
                image::load_from_memory(data).is_ok()
            }
            // Para otros formatos, validar con magic bytes
            FileType::Pdf => {
                data.starts_with(&[0x25, 0x50, 0x44, 0x46]) // %PDF
            }
            FileType::Zip | FileType::Docx | FileType::Xlsx | FileType::Pptx => {
                // ZIP y Office files: validar signature
                data.starts_with(&[0x50, 0x4B, 0x03, 0x04])
            }
            FileType::Rar => {
                data.starts_with(&[0x52, 0x61, 0x72, 0x21])
            }
            FileType::SevenZip => {
                data.starts_with(&[0x37, 0x7A, 0xBC, 0xAF])
            }
            // Para otros formatos, al menos verificar que no estén vacíos
            _ => !data.iter().all(|&b| b == 0),
        }
    }

    /// Recupera múltiples archivos
    pub fn recover_files(
        &self,
        reader: &mut DiskReader,
        files: &[FoundFile],
    ) -> Vec<(FoundFile, Result<PathBuf, String>)> {
        info!("Iniciando recuperación de {} archivos", files.len());

        // Recuperar archivos secuencialmente (paralelización limitada por acceso al disco)
        files
            .iter()
            .map(|f| {
                let result = self.recover_file(reader, f);
                (f.clone(), result)
            })
            .collect()
    }

    /// Recupera archivos por tipo
    pub fn recover_by_type(
        &self,
        reader: &mut DiskReader,
        files: &[FoundFile],
        file_type: crate::core::signatures::FileType,
    ) -> Vec<(FoundFile, Result<PathBuf, String>)> {
        let filtered: Vec<FoundFile> = files
            .iter()
            .filter(|f| f.file_type == file_type)
            .cloned()
            .collect();

        self.recover_files(reader, &filtered)
    }

    /// Organiza los archivos recuperados en subdirectorios por tipo
    pub fn organize_by_type(
        &self,
        files: &[PathBuf],
    ) -> std::collections::HashMap<String, Vec<PathBuf>> {
        let mut organized: std::collections::HashMap<String, Vec<PathBuf>> =
            std::collections::HashMap::new();

        for file_path in files {
            if let Some(ext) = file_path.extension() {
                let ext_str = ext.to_string_lossy().to_lowercase();
                let category = match ext_str.as_str() {
                    "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tiff" | "webp" | "ico" => "Imágenes",
                    "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" => {
                        "Documentos"
                    }
                    "zip" | "rar" | "7z" | "tar" | "gz" => "Archivos",
                    "mp3" | "wav" | "flac" | "aac" | "ogg" => "Audio",
                    "mp4" | "avi" | "mkv" | "mov" | "wmv" | "webm" => "Video",
                    "exe" | "dll" | "msi" => "Ejecutables",
                    _ => "Otros",
                };

                organized
                    .entry(category.to_string())
                    .or_insert_with(Vec::new)
                    .push(file_path.clone());
            }
        }

        organized
    }

    /// Obtiene el directorio de salida
    pub fn output_dir(&self) -> &Path {
        &self.output_dir
    }

    /// Establece un nuevo directorio de salida
    pub fn set_output_dir(&mut self, path: &Path) -> Result<(), String> {
        if !path.exists() {
            fs::create_dir_all(path).map_err(|e| format!("Error al crear directorio: {}", e))?;
        }

        self.output_dir = path.to_path_buf();
        Ok(())
    }
}

/// Lee datos crudos del disco en un offset específico
pub fn read_raw_data(drive: &str, offset: u64, size: usize) -> Result<Vec<u8>, String> {
    let mut reader = DiskReader::open(drive)?;
    reader.read_at(offset, size)
}

/// Valida si un archivo tiene datos coherentes
pub fn validate_recovered_file(path: &Path) -> Result<bool, String> {
    let mut file = File::open(path).map_err(|e| format!("Error al abrir archivo: {}", e))?;

    let mut header = vec![0u8; 16];
    file.read_exact(&mut header)
        .map_err(|e| format!("Error al leer header: {}", e))?;

    // Verificar si tiene magic bytes válidos
    Ok(!header.iter().all(|&b| b == 0))
}

/// Obtiene información de un archivo recuperado
pub fn get_recovered_file_info(path: &Path) -> Result<RecoveredFileInfo, String> {
    let metadata = fs::metadata(path).map_err(|e| format!("Error al obtener metadatos: {}", e))?;

    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    Ok(RecoveredFileInfo {
        path: path.to_path_buf(),
        size: metadata.len(),
        extension,
        is_valid: validate_recovered_file(path).unwrap_or(false),
    })
}

/// Información de un archivo recuperado
#[derive(Debug)]
pub struct RecoveredFileInfo {
    pub path: PathBuf,
    pub size: u64,
    pub extension: String,
    pub is_valid: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_recovery_engine() {
        let temp_dir = env::temp_dir().join("recoverpill_test");
        let engine = RecoveryEngine::new(&temp_dir);
        assert!(engine.is_ok());

        // Limpiar
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
