//! Acceso al sistema de archivos
//!
//! Proporciona funciones para leer archivos del sistema de archivos (FAT32/NTFS).

use log::{error, info, warn};
use std::io::{Read, Seek, SeekFrom};

use super::access::DiskReader;

/// Tipo de sistema de archivos
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileSystemType {
    Fat32,
    Ntfs,
    Unknown,
}

/// Entrada de archivo del sistema de archivos
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub size: u64,
    pub offset: u64,
    pub is_deleted: bool,
    pub file_type: String,
    pub created: Option<u64>,
    pub modified: Option<u64>,
}

/// Lector del sistema de archivos
pub struct FileSystemReader {
    reader: DiskReader,
    fs_type: FileSystemType,
    cluster_size: u64,
    mft_offset: Option<u64>,
    fat_offset: Option<u64>,
}

impl FileSystemReader {
    /// Crea un nuevo lector del sistema de archivos
    pub fn new(mut reader: DiskReader) -> Result<Self, String> {
        let fs_type = Self::detect_filesystem(&mut reader)?;

        info!("Sistema de archivos detectado: {:?}", fs_type);

        Ok(FileSystemReader {
            reader,
            fs_type,
            cluster_size: 4096, // Valor por defecto
            mft_offset: None,
            fat_offset: None,
        })
    }

    /// Detecta el tipo de sistema de archivos
    fn detect_filesystem(reader: &mut DiskReader) -> Result<FileSystemType, String> {
        // Leer los primeros bytes del disco para detectar el sistema de archivos
        let data = reader.read_at(0, 512)?;

        if data.len() < 512 {
            return Err("No se pudo leer el sector de arranque".to_string());
        }

        // Verificar si es NTFS (busca "NTFS" en el sector de arranque)
        if data.len() >= 52 && &data[3..7] == b"NTFS" {
            return Ok(FileSystemType::Ntfs);
        }

        // Verificar si es FAT32 (busca "FAT32" en el sector de arranque)
        if data.len() >= 82 && &data[82..90] == b"FAT32   " {
            return Ok(FileSystemType::Fat32);
        }

        // Verificar FAT16
        if data.len() >= 54 && &data[54..59] == b"FAT16" {
            return Ok(FileSystemType::Fat32); // Tratamos FAT16 como FAT32 para simplificar
        }

        Ok(FileSystemType::Unknown)
    }

    /// Escanea el sistema de archivos en busca de archivos
    pub fn scan_filesystem(&mut self) -> Result<Vec<FileEntry>, String> {
        info!("Escaneando sistema de archivos: {:?}", self.fs_type);

        match self.fs_type {
            FileSystemType::Fat32 => self.scan_fat32(),
            FileSystemType::Ntfs => self.scan_ntfs(),
            FileSystemType::Unknown => {
                warn!("Sistema de archivos desconocido, intentando escaneo genérico");
                self.scan_generic()
            }
        }
    }

    /// Escanea FAT32
    fn scan_fat32(&mut self) -> Result<Vec<FileEntry>, String> {
        info!("Escaneando FAT32...");

        let mut files = Vec::new();

        // Leer el sector de arranque para obtener información del sistema de archivos
        let boot_sector = self.reader.read_at(0, 512)?;

        if boot_sector.len() < 512 {
            return Err("No se pudo leer el sector de arranque FAT32".to_string());
        }

        // Extraer información del sector de arranque FAT32
        let bytes_per_sector = u16::from_le_bytes([boot_sector[11], boot_sector[12]]) as u64;
        let sectors_per_cluster = boot_sector[13] as u64;
        let reserved_sectors = u16::from_le_bytes([boot_sector[14], boot_sector[15]]) as u64;
        let num_fats = boot_sector[16] as u64;
        let root_cluster = u32::from_le_bytes([
            boot_sector[44],
            boot_sector[45],
            boot_sector[46],
            boot_sector[47],
        ]) as u64;

        self.cluster_size = bytes_per_sector * sectors_per_cluster;
        self.fat_offset = Some(reserved_sectors * bytes_per_sector);

        info!(
            "FAT32: bytes_per_sector={}, sectors_per_cluster={}, cluster_size={}",
            bytes_per_sector, sectors_per_cluster, self.cluster_size
        );

        // Leer el directorio raíz
        let root_offset = self.cluster_to_offset(root_cluster);
        self.read_fat32_directory(root_offset, &mut files, 0)?;

        Ok(files)
    }

    /// Lee un directorio FAT32
    fn read_fat32_directory(
        &mut self,
        offset: u64,
        files: &mut Vec<FileEntry>,
        depth: usize,
    ) -> Result<(), String> {
        if depth > 10 {
            return Ok(()); // Evitar recursión infinita
        }

        let mut current_offset = offset;
        let mut long_name = String::new();

        loop {
            // Leer una entrada de directorio (32 bytes)
            let entry_data = match self.reader.read_at(current_offset, 32) {
                Ok(d) => d,
                Err(_) => break,
            };

            if entry_data.len() < 32 {
                break;
            }

            // Verificar si es el final del directorio
            if entry_data[0] == 0x00 {
                break;
            }

            // Verificar si es una entrada válida
            if entry_data[0] != 0xE5 && entry_data[11] != 0x0F {
                // Entrada normal de archivo/directorio
                let name_bytes = &entry_data[0..11];
                let mut name = String::new();

                // Decodificar nombre (formato 8.3)
                for i in 0..8 {
                    if name_bytes[i] != 0x20 {
                        name.push(name_bytes[i] as char);
                    }
                }

                if name_bytes[8] != 0x20 {
                    name.push('.');
                    for i in 8..11 {
                        if name_bytes[i] != 0x20 {
                            name.push(name_bytes[i] as char);
                        }
                    }
                }

                // Si tenemos un nombre largo, usarlo
                if !long_name.is_empty() {
                    name = long_name.clone();
                    long_name.clear();
                }

                let attributes = entry_data[11];
                let is_directory = (attributes & 0x10) != 0;
                let is_deleted = entry_data[0] == 0xE5;

                // Obtener tamaño del archivo
                let size = u32::from_le_bytes([
                    entry_data[28],
                    entry_data[29],
                    entry_data[30],
                    entry_data[31],
                ]) as u64;

                // Obtener primer cluster
                let first_cluster = (u16::from_le_bytes([entry_data[26], entry_data[27]]) as u64)
                    << 16
                    | u16::from_le_bytes([entry_data[20], entry_data[21]]) as u64;

                let file_offset = self.cluster_to_offset(first_cluster);

                // Crear entrada de archivo
                let file_entry = FileEntry {
                    name: name.clone(),
                    size,
                    offset: file_offset,
                    is_deleted,
                    file_type: if is_directory {
                        "Directorio".to_string()
                    } else {
                        Self::get_file_extension(&name)
                    },
                    created: None,
                    modified: None,
                };

                files.push(file_entry);

                // Si es un directorio, escanear recursivamente
                if is_directory && !is_deleted && first_cluster >= 2 {
                    let dir_offset = self.cluster_to_offset(first_cluster);
                    self.read_fat32_directory(dir_offset, files, depth + 1)?;
                }
            } else if entry_data[11] == 0x0F {
                // Entrada de nombre largo (VFAT)
                let sequence = entry_data[0];
                if sequence & 0x40 != 0 {
                    // Primera entrada de nombre largo
                    long_name.clear();
                }

                // Leer caracteres del nombre largo
                let mut chars = Vec::new();

                // Primera parte (5 caracteres)
                for i in (1..11).step_by(2) {
                    if i + 1 < entry_data.len() {
                        let ch = u16::from_le_bytes([entry_data[i], entry_data[i + 1]]);
                        if ch != 0x0000 && ch != 0xFFFF {
                            chars.push(ch);
                        }
                    }
                }

                // Segunda parte (6 caracteres)
                for i in (14..26).step_by(2) {
                    if i + 1 < entry_data.len() {
                        let ch = u16::from_le_bytes([entry_data[i], entry_data[i + 1]]);
                        if ch != 0x0000 && ch != 0xFFFF {
                            chars.push(ch);
                        }
                    }
                }

                // Tercera parte (2 caracteres)
                for i in (28..32).step_by(2) {
                    if i + 1 < entry_data.len() {
                        let ch = u16::from_le_bytes([entry_data[i], entry_data[i + 1]]);
                        if ch != 0x0000 && ch != 0xFFFF {
                            chars.push(ch);
                        }
                    }
                }

                // Convertir a String
                if let Ok(s) = String::from_utf16(&chars) {
                    long_name = s + &long_name;
                }
            }

            current_offset += 32;
        }

        Ok(())
    }

    /// Convierte un cluster a offset en el disco
    fn cluster_to_offset(&self, cluster: u64) -> u64 {
        if cluster < 2 {
            return 0;
        }

        // Para FAT32, el primer cluster de datos es el cluster 2
        // El offset del primer cluster de datos depende de la estructura del sistema de archivos
        if let Some(fat_offset) = self.fat_offset {
            // Calcular el inicio del área de datos
            // FAT32: área de datos comienza después de las FATs
            // Asumimos 2 FATs, cada una ocupa 1 sector (512 bytes)
            // En un sistema real, esto se calcularía del sector de arranque
            let data_start = fat_offset + 2 * 512; // 2 FATs de 512 bytes cada una
            data_start + (cluster - 2) * self.cluster_size
        } else {
            0
        }
    }

    /// Escanea NTFS
    fn scan_ntfs(&mut self) -> Result<Vec<FileEntry>, String> {
        info!("Escaneando NTFS...");

        let mut files = Vec::new();

        // Leer el sector de arranque para obtener información del MFT
        let boot_sector = self.reader.read_at(0, 512)?;

        if boot_sector.len() < 512 {
            return Err("No se pudo leer el sector de arranque NTFS".to_string());
        }

        // Extraer información del sector de arranque NTFS
        let mft_cluster = u64::from_le_bytes([
            boot_sector[48],
            boot_sector[49],
            boot_sector[50],
            boot_sector[51],
            boot_sector[52],
            boot_sector[53],
            boot_sector[54],
            boot_sector[55],
        ]);

        let bytes_per_sector = u16::from_le_bytes([boot_sector[11], boot_sector[12]]) as u64;
        let sectors_per_cluster = boot_sector[13] as u64;

        self.cluster_size = bytes_per_sector * sectors_per_cluster;
        self.mft_offset = Some(mft_cluster * self.cluster_size);

        info!(
            "NTFS: mft_cluster={}, cluster_size={}",
            mft_cluster, self.cluster_size
        );

        // Leer el MFT
        if let Some(mft_offset) = self.mft_offset {
            self.read_ntfs_mft(mft_offset, &mut files)?;
        }

        Ok(files)
    }

    /// Lee el MFT de NTFS
    fn read_ntfs_mft(&mut self, offset: u64, files: &mut Vec<FileEntry>) -> Result<(), String> {
        // Leer los primeros registros del MFT
        // El MFT tiene un tamaño fijo de 1024 bytes por registro
        let mft_record_size: usize = 1024;

        for i in 0..100u64 {
            // Leer los primeros 100 registros
            let record_offset = offset + (i * mft_record_size as u64);

            let record_data = match self.reader.read_at(record_offset, mft_record_size) {
                Ok(d) => d,
                Err(_) => break,
            };

            if record_data.len() < mft_record_size {
                break;
            }

            // Verificar firma "FILE"
            if &record_data[0..4] != b"FILE" {
                continue;
            }

            // Leer atributos del registro MFT
            let mut file_name = String::new();
            let mut file_size = 0u64;
            let mut is_directory = false;
            let mut data_offset = 0u64;

            // Buscar atributos
            let mut attr_offset = u16::from_le_bytes([record_data[20], record_data[21]]) as usize;

            while attr_offset < record_data.len() - 4 {
                let attr_type = u32::from_le_bytes([
                    record_data[attr_offset],
                    record_data[attr_offset + 1],
                    record_data[attr_offset + 2],
                    record_data[attr_offset + 3],
                ]);

                let attr_length = u32::from_le_bytes([
                    record_data[attr_offset + 4],
                    record_data[attr_offset + 5],
                    record_data[attr_offset + 6],
                    record_data[attr_offset + 7],
                ]) as usize;

                if attr_length == 0 {
                    break;
                }

                match attr_type {
                    0x30 => {
                        // $FILE_NAME - nombre del archivo
                        if attr_offset + 24 < record_data.len() {
                            let name_length = record_data[attr_offset + 64] as usize;
                            let name_offset = attr_offset + 66;

                            if name_offset + name_length * 2 <= record_data.len() {
                                let name_bytes =
                                    &record_data[name_offset..name_offset + name_length * 2];
                                if let Ok(name) = String::from_utf16(
                                    &name_bytes
                                        .chunks_exact(2)
                                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                                        .collect::<Vec<_>>(),
                                ) {
                                    file_name = name;
                                }
                            }
                        }
                    }
                    0x80 => {
                        // $DATA - datos del archivo
                        if attr_offset + 16 < record_data.len() {
                            let content_size = u32::from_le_bytes([
                                record_data[attr_offset + 16],
                                record_data[attr_offset + 17],
                                record_data[attr_offset + 18],
                                record_data[attr_offset + 19],
                            ]) as u64;

                            let content_offset = u32::from_le_bytes([
                                record_data[attr_offset + 20],
                                record_data[attr_offset + 21],
                                record_data[attr_offset + 22],
                                record_data[attr_offset + 23],
                            ]) as u64;

                            file_size = content_size;
                            data_offset = content_offset;
                        }
                    }
                    0x90 => {
                        // $INDEX_ROOT - indica que es un directorio
                        is_directory = true;
                    }
                    _ => {}
                }

                attr_offset += attr_length;
            }

            // Crear entrada de archivo si tenemos un nombre
            if !file_name.is_empty() {
                let file_entry = FileEntry {
                    name: file_name.clone(),
                    size: file_size,
                    offset: data_offset,
                    is_deleted: false, // Los registros MFT activos no están eliminados
                    file_type: if is_directory {
                        "Directorio".to_string()
                    } else {
                        Self::get_file_extension(&file_name)
                    },
                    created: None,
                    modified: None,
                };

                files.push(file_entry);
            }
        }

        Ok(())
    }

    /// Escaneo genérico para sistemas de archivos desconocidos
    fn scan_generic(&mut self) -> Result<Vec<FileEntry>, String> {
        info!("Realizando escaneo genérico...");

        // Para sistemas de archivos desconocidos, simplemente devolvemos una lista vacía
        // En una implementación más completa, podríamos intentar detectar archivos
        // basándonos en patrones de datos
        Ok(Vec::new())
    }

    /// Obtiene la extensión de un archivo
    fn get_file_extension(name: &str) -> String {
        if let Some(pos) = name.rfind('.') {
            name[pos + 1..].to_lowercase()
        } else {
            "desconocido".to_string()
        }
    }

    /// Lee los datos de un archivo del sistema de archivos
    pub fn read_file_data(
        &mut self,
        entry: &FileEntry,
        max_size: usize,
    ) -> Result<Vec<u8>, String> {
        if entry.size == 0 {
            return Ok(Vec::new());
        }

        let read_size = std::cmp::min(entry.size as usize, max_size);

        match self.reader.read_at(entry.offset, read_size) {
            Ok(data) => Ok(data),
            Err(e) => Err(format!("Error leyendo datos del archivo: {}", e)),
        }
    }

    /// Obtiene el tipo de sistema de archivos
    pub fn get_fs_type(&self) -> FileSystemType {
        self.fs_type
    }

    /// Obtiene el tamaño del cluster
    pub fn get_cluster_size(&self) -> u64 {
        self.cluster_size
    }

    /// Consume el FileSystemReader y devuelve el DiskReader
    pub fn into_reader(self) -> DiskReader {
        self.reader
    }
}
