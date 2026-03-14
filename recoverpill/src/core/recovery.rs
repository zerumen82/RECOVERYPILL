//! Motor de recuperación de archivos
//! 
//! Implementa las funciones para recuperar archivos del disco.

use std::fs::{self, File};
use std::io::{Write, Read};
use std::path::{Path, PathBuf};
use log::{info, warn, error};

use crate::core::scanner::FoundFile;
use crate::disk::access::DiskReader;

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

    /// Recupera un archivo del disco
    pub fn recover_file(
        &self,
        reader: &mut DiskReader,
        found_file: &FoundFile,
    ) -> Result<PathBuf, String> {
        // Crear nombre de archivo
        let file_name = format!(
            "{}_{}.{}",
            found_file.file_type.extension().to_uppercase(),
            found_file.offset / 1024, // Usar offset como identificador
            found_file.file_type.extension()
        );
        
        // Determinar directorio por tipo
        let category_dir = self.output_dir.join(found_file.file_type.category());
        if !category_dir.exists() {
            fs::create_dir_all(&category_dir)
                .map_err(|e| format!("Error al crear directorio de categoría: {}", e))?;
        }
        
        let file_path = category_dir.join(&file_name);
        
        // Leer datos del disco
        let size = if found_file.estimated_size > 0 {
            found_file.estimated_size as usize
        } else {
            // Tamaño por defecto si no se puede estimar
            64 * 1024 // 64KB
        };
        
        let data = reader.read_at(found_file.offset, size)
            .map_err(|e| format!("Error al leer datos del archivo: {}", e))?;
        
        if data.is_empty() {
            return Err("No se pudieron leer datos del archivo".to_string());
        }
        
        // Escribir archivo
        let mut file = File::create(&file_path)
            .map_err(|e| format!("Error al crear archivo: {}", e))?;
        
        file.write_all(&data)
            .map_err(|e| format!("Error al escribir archivo: {}", e))?;
        
        info!("Archivo recuperado: {:?}", file_path);
        
        Ok(file_path)
    }

    /// Recupera múltiples archivos
    pub fn recover_files(
        &self,
        reader: &mut DiskReader,
        files: &[FoundFile],
    ) -> Vec<(FoundFile, Result<PathBuf, String>)> {
        info!("Iniciando recuperación de {} archivos", files.len());
        
        files.iter()
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
        let filtered: Vec<FoundFile> = files.iter()
            .filter(|f| f.file_type == file_type)
            .cloned()
            .collect();
        
        self.recover_files(reader, &filtered)
    }

    /// Organiza los archivos recuperados en subdirectorios por tipo
    pub fn organize_by_type(&self, files: &[PathBuf]) -> std::collections::HashMap<String, Vec<PathBuf>> {
        let mut organized: std::collections::HashMap<String, Vec<PathBuf>> = std::collections::HashMap::new();
        
        for file_path in files {
            if let Some(ext) = file_path.extension() {
                let ext_str = ext.to_string_lossy().to_lowercase();
                let category = match ext_str.as_str() {
                    "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tiff" | "webp" | "ico" => "Imágenes",
                    "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" => "Documentos",
                    "zip" | "rar" | "7z" | "tar" | "gz" => "Archivos",
                    "mp3" | "wav" | "flac" | "aac" | "ogg" => "Audio",
                    "mp4" | "avi" | "mkv" | "mov" | "wmv" | "webm" => "Video",
                    "exe" | "dll" | "msi" => "Ejecutables",
                    _ => "Otros",
                };
                
                organized.entry(category.to_string())
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
            fs::create_dir_all(path)
                .map_err(|e| format!("Error al crear directorio: {}", e))?;
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
    let mut file = File::open(path)
        .map_err(|e| format!("Error al abrir archivo: {}", e))?;
    
    let mut header = vec![0u8; 16];
    file.read_exact(&mut header)
        .map_err(|e| format!("Error al leer header: {}", e))?;
    
    // Verificar si tiene magic bytes válidos
    Ok(!header.iter().all(|&b| b == 0))
}

/// Obtiene información de un archivo recuperado
pub fn get_recovered_file_info(path: &Path) -> Result<RecoveredFileInfo, String> {
    let metadata = fs::metadata(path)
        .map_err(|e| format!("Error al obtener metadatos: {}", e))?;
    
    let extension = path.extension()
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
