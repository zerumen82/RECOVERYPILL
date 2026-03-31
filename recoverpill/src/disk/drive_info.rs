//! Información de unidades de disco
//!
//! Proporciona funciones para detectar y obtener información de las unidades disponibles.

use log::{info, warn};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use winapi::um::fileapi::{GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives};
use winapi::um::winnt::{FILE_ATTRIBUTE_READONLY, FILE_SHARE_READ, FILE_SHARE_WRITE};

/// Tipos de unidades de disco
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveType {
    Unknown,
    NoRootDir,
    Removable, // USB, SD card
    Fixed,     // Disco duro interno
    Network,
    CDRom,
    RamDisk,
}

impl DriveType {
    pub fn from_winapi(dtype: u32) -> Self {
        match dtype {
            0 => DriveType::Unknown,
            1 => DriveType::NoRootDir,
            2 => DriveType::Removable,
            3 => DriveType::Fixed,
            4 => DriveType::Network,
            5 => DriveType::CDRom,
            6 => DriveType::RamDisk,
            _ => DriveType::Unknown,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            DriveType::Unknown => "Desconocido",
            DriveType::NoRootDir => "Sin directorio raíz",
            DriveType::Removable => "Extraíble (USB/SD)",
            DriveType::Fixed => "Disco Fijo",
            DriveType::Network => "Red",
            DriveType::CDRom => "CD/DVD",
            DriveType::RamDisk => "RAM Disk",
        }
    }
}

/// Información de una unidad de disco
#[derive(Debug, Clone)]
pub struct DriveInfo {
    pub path: String, // ej: "C:"
    pub drive_type: DriveType,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub used_bytes: u64,
    pub volume_label: String,
}

impl DriveInfo {
    /// Formatea el tamaño en bytes a string legible
    pub fn format_size(bytes: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = KB * 1024;
        const GB: u64 = MB * 1024;
        const TB: u64 = GB * 1024;

        if bytes >= TB {
            format!("{:.2} TB", bytes as f64 / TB as f64)
        } else if bytes >= GB {
            format!("{:.2} GB", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            format!("{:.2} MB", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            format!("{:.2} KB", bytes as f64 / KB as f64)
        } else {
            format!("{} bytes", bytes)
        }
    }

    /// Obtiene el nombre del volumen de la unidad
    fn get_volume_label(path: &str) -> String {
        let wide_path: Vec<u16> = OsStr::new(path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut volume_name: [u16; 261] = [0; 261];
        let mut serial: u32 = 0;
        let mut max_component_length: u32 = 0;
        let mut file_system_flags: u32 = 0;

        unsafe {
            let result = winapi::um::fileapi::GetVolumeInformationW(
                wide_path.as_ptr(),
                volume_name.as_mut_ptr(),
                261,
                &mut serial,
                &mut max_component_length,
                &mut file_system_flags,
                std::ptr::null_mut(),
                0,
            );

            if result != 0 {
                // Convertir de UTF-16 a String
                let len = volume_name.iter().position(|&c| c == 0).unwrap_or(260);
                String::from_utf16_lossy(&volume_name[..len])
            } else {
                String::from("Sin etiqueta")
            }
        }
    }

    /// Obtiene el tipo de unidad a partir de la letra
    fn get_drive_type(path: &str) -> DriveType {
        let wide_path: Vec<u16> = OsStr::new(path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let dtype = GetDriveTypeW(wide_path.as_ptr());
            DriveType::from_winapi(dtype)
        }
    }

    /// Obtiene el espacio total y libre de la unidad
    fn get_disk_space(path: &str) -> (u64, u64) {
        let wide_path: Vec<u16> = OsStr::new(path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        // Usar la estructura correcta para ULARGE_INTEGER
        #[repr(C)]
        struct ULARGE_INTEGER {
            LowPart: u32,
            HighPart: u32,
        }

        let mut free_bytes_available: ULARGE_INTEGER = ULARGE_INTEGER {
            LowPart: 0,
            HighPart: 0,
        };
        let mut total_bytes: ULARGE_INTEGER = ULARGE_INTEGER {
            LowPart: 0,
            HighPart: 0,
        };
        let mut total_free_bytes: ULARGE_INTEGER = ULARGE_INTEGER {
            LowPart: 0,
            HighPart: 0,
        };

        unsafe {
            let result = GetDiskFreeSpaceExW(
                wide_path.as_ptr(),
                &mut free_bytes_available as *mut ULARGE_INTEGER as *mut _,
                &mut total_bytes as *mut ULARGE_INTEGER as *mut _,
                &mut total_free_bytes as *mut ULARGE_INTEGER as *mut _,
            );

            if result != 0 {
                let total = ((total_bytes.HighPart as u64) << 32) | (total_bytes.LowPart as u64);
                let free = ((free_bytes_available.HighPart as u64) << 32)
                    | (free_bytes_available.LowPart as u64);
                return (total, free);
            }
        }

        (0, 0)
    }

    /// Crea DriveInfo a partir de una letra de unidad
    pub fn from_drive_letter(letter: char) -> Option<Self> {
        let path = format!("{}:\\", letter);
        let drive_type = Self::get_drive_type(&path);

        if drive_type == DriveType::Unknown || drive_type == DriveType::NoRootDir {
            return None;
        }

        let (total_bytes, free_bytes) = Self::get_disk_space(&path);
        let volume_label = Self::get_volume_label(&path);
        let used_bytes = total_bytes.saturating_sub(free_bytes);

        Some(DriveInfo {
            path: format!("{}:", letter),
            drive_type,
            total_bytes,
            free_bytes,
            used_bytes,
            volume_label,
        })
    }
}

/// Obtiene todas las unidades lógicas disponibles en el sistema
pub fn get_available_drives() -> Vec<DriveInfo> {
    let mut drives = Vec::new();

    unsafe {
        let logical_drives = GetLogicalDrives();

        // Iterar sobre las letras A-Z (26 bits)
        for i in 0..26 {
            if (logical_drives & (1 << i)) != 0 {
                let letter = (b'A' + i as u8) as char;

                if let Some(drive_info) = DriveInfo::from_drive_letter(letter) {
                    info!(
                        "Unidad detectada: {} - {} ({}: {})",
                        drive_info.path,
                        drive_info.volume_label,
                        drive_info.drive_type.display_name(),
                        DriveInfo::format_size(drive_info.total_bytes)
                    );
                    drives.push(drive_info);
                }
            }
        }
    }

    if drives.is_empty() {
        warn!("No se detectaron unidades lógicas");
    }

    drives
}

/// Obtiene solo unidades extraíbles (USB, SD cards)
pub fn get_removable_drives() -> Vec<DriveInfo> {
    get_available_drives()
        .into_iter()
        .filter(|d| d.drive_type == DriveType::Removable)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_drives() {
        let drives = get_available_drives();
        println!("Unidades encontradas: {:?}", drives);
        assert!(!drives.is_empty());
    }

    #[test]
    fn test_drive_info_format() {
        assert_eq!(DriveInfo::format_size(1024), "1.00 KB");
        assert_eq!(DriveInfo::format_size(1048576), "1.00 MB");
        assert_eq!(DriveInfo::format_size(1073741824), "1.00 GB");
    }
}
