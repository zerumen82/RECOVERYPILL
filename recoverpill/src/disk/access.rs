//! Acceso de bajo nivel al disco
//!
//! Proporciona funciones para leer datos del disco usando Windows API.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::ptr::null_mut;
use winapi::ctypes::c_void;
use winapi::shared::minwindef::{DWORD, FALSE};
use winapi::um::fileapi::GetDiskFreeSpaceExW;
use winapi::um::fileapi::{CreateFileW, ReadFile, OPEN_EXISTING};
use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
use winapi::um::winnt::{FILE_SHARE_READ, FILE_SHARE_WRITE, GENERIC_READ, HANDLE};

// FILE_BEGIN = 0 for SetFilePointer
const FILE_BEGIN: DWORD = 0;

/// Lector de disco de bajo nivel usando Windows API
pub struct DiskReader {
    handle: HANDLE,
    pub total_bytes: u64,
    pub bytes_per_sector: u64,
}

impl DiskReader {
    /// Abre una unidad de disco para lectura usando Windows API
    pub fn open(drive_path: &str) -> Result<Self, String> {
        // Convertir la ruta del drive a formato Windows
        // "C:" -> "\\\\.\\C:"
        // "C:\\" -> "\\\\.\\C:"
        let device_path = if drive_path.ends_with('\\') {
            format!("\\\\.\\{}", drive_path.trim_end_matches('\\'))
        } else if drive_path.len() == 2 && drive_path.chars().nth(1) == Some(':') {
            format!("\\\\.\\{}", drive_path)
        } else if drive_path.starts_with("\\\\.\\") {
            drive_path.to_string()
        } else {
            format!("\\\\.\\{}", drive_path)
        };

        // Convertir a wide string
        let wide_path: Vec<u16> = device_path
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        // Abrir el dispositivo con permisos de lectura
        let handle = unsafe {
            CreateFileW(
                wide_path.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                null_mut(),
                OPEN_EXISTING,
                0,
                null_mut(),
            )
        };

        if handle == INVALID_HANDLE_VALUE {
            let error = unsafe { winapi::um::errhandlingapi::GetLastError() };
            return Err(format!(
                "Error al abrir {}: Código de error {}",
                device_path, error
            ));
        }

        // Obtener el tamaño del dispositivo
        // Primero intentar con GetDiskFreeSpaceEx para volúmenes
        let total_bytes = get_volume_size_from_path(drive_path);

        // Si es 0, intentar con DeviceIoControl
        let total_bytes = if total_bytes == 0 {
            get_disk_size(handle)
        } else {
            total_bytes
        };

        // Si sigue siendo 0, intentar con el tamaño del volumen usando DeviceIoControl
        let total_bytes = if total_bytes == 0 {
            get_volume_size(handle).unwrap_or(0)
        } else {
            total_bytes
        };
        let bytes_per_sector = 512u64; // Valor por defecto

        Ok(DiskReader {
            handle,
            total_bytes,
            bytes_per_sector,
        })
    }

    /// Lee una cantidad específica de bytes desde una posición
    pub fn read_at(&mut self, offset: u64, size: usize) -> Result<Vec<u8>, String> {
        if self.handle == INVALID_HANDLE_VALUE {
            return Err("Handle de disco inválido".to_string());
        }

        // Mover el puntero del archivo al offset deseado
        let mut distance_high: i32 = (offset >> 32) as i32;
        let distance_low = (offset & 0xFFFFFFFF) as i32;

        let new_pos = unsafe {
            winapi::um::fileapi::SetFilePointer(
                self.handle,
                distance_low,
                &mut distance_high,
                FILE_BEGIN,
            )
        };

        if new_pos == winapi::um::fileapi::INVALID_SET_FILE_POINTER {
            let error = unsafe { winapi::um::errhandlingapi::GetLastError() };
            return Err(format!(
                "Error al posicionar en offset {}: {}",
                offset, error
            ));
        }

        // Leer los datos
        let mut buffer = vec![0u8; size];
        let mut bytes_read: DWORD = 0;

        let result = unsafe {
            ReadFile(
                self.handle,
                buffer.as_mut_ptr() as *mut c_void,
                size as DWORD,
                &mut bytes_read as *mut DWORD,
                null_mut(),
            )
        };

        if result == FALSE {
            let error = unsafe { winapi::um::errhandlingapi::GetLastError() };
            return Err(format!("Error al leer en offset {}: {}", offset, error));
        }

        buffer.truncate(bytes_read as usize);
        Ok(buffer)
    }

    /// Cierra el handle del disco
    pub fn close(&mut self) {
        if self.handle != INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(self.handle);
            }
            self.handle = INVALID_HANDLE_VALUE;
        }
    }

    /// Obtiene el tamaño total del disco en bytes
    pub fn total_size(&self) -> u64 {
        self.total_bytes
    }
}

impl Drop for DiskReader {
    fn drop(&mut self) {
        self.close();
    }
}

/// Obtiene el tamaño del disco
fn get_disk_size(handle: HANDLE) -> u64 {
    // Usar un enfoque más simple: intentar obtener el tamaño con DeviceIoControl
    // Primero intentar con GET_DRIVE_GEOMETRY_EX que devuelve el tamaño real
    let mut bytes_returned: DWORD = 0;
    let mut disk_geometry_ex: [u8; 32] = [0; 32]; // DISK_GEOMETRY_EX

    unsafe {
        let result = winapi::um::ioapiset::DeviceIoControl(
            handle,
            0x000700A0, // IOCTL_DISK_GET_DRIVE_GEOMETRY_EX
            null_mut(),
            0,
            disk_geometry_ex.as_mut_ptr() as *mut c_void,
            disk_geometry_ex.len() as DWORD,
            &mut bytes_returned as *mut DWORD,
            null_mut(),
        );

        if result != 0 && bytes_returned >= 24 {
            // En DISK_GEOMETRY_EX, el tamaño está en los primeros 8 bytes (QuadPart)
            let mut size_bytes = [0u8; 8];
            size_bytes.copy_from_slice(&disk_geometry_ex[0..8]);
            return i64::from_le_bytes(size_bytes) as u64;
        }
    }

    // Si falla, intentar con GET_DRIVE_GEOMETRY
    let mut bytes_returned: DWORD = 0;
    let mut disk_geometry: [u8; 64] = [0; 64]; // DISK_GEOMETRY

    unsafe {
        let result = winapi::um::ioapiset::DeviceIoControl(
            handle,
            0x00070000, // IOCTL_DISK_GET_DRIVE_GEOMETRY
            null_mut(),
            0,
            disk_geometry.as_mut_ptr() as *mut c_void,
            disk_geometry.len() as DWORD,
            &mut bytes_returned as *mut DWORD,
            null_mut(),
        );

        if result != 0 && bytes_returned >= 32 {
            // Bytes por sector = 8-11
            let bytes_per_sector = u32::from_le_bytes([
                disk_geometry[8],
                disk_geometry[9],
                disk_geometry[10],
                disk_geometry[11],
            ]) as u64;
            // Sectores por pista = 12-15
            let sectors = u32::from_le_bytes([
                disk_geometry[12],
                disk_geometry[13],
                disk_geometry[14],
                disk_geometry[15],
            ]) as u64;
            // Numero de pistas = 16-19
            let tracks = u32::from_le_bytes([
                disk_geometry[16],
                disk_geometry[17],
                disk_geometry[18],
                disk_geometry[19],
            ]) as u64;
            // Number of Media = 20-23
            let media = u32::from_le_bytes([
                disk_geometry[20],
                disk_geometry[21],
                disk_geometry[22],
                disk_geometry[23],
            ]) as u64;

            return bytes_per_sector * sectors * tracks * media;
        }
    }

    // Valor por defecto si no se puede obtener
    0
}

/// Obtiene el tamaño del volumen
fn get_volume_size(handle: HANDLE) -> Option<u64> {
    let mut bytes_returned: DWORD = 0;
    let mut volume_disk_extents: [u8; 64] = [0; 64]; // VOLUME_DISK_EXTENTS

    unsafe {
        let result = winapi::um::ioapiset::DeviceIoControl(
            handle,
            0x00056000, // IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS
            null_mut(),
            0,
            volume_disk_extents.as_mut_ptr() as *mut c_void,
            volume_disk_extents.len() as DWORD,
            &mut bytes_returned as *mut DWORD,
            null_mut(),
        );

        if result != 0 && bytes_returned >= 24 {
            // Number of disk extents = 0-3
            let num_extents = u32::from_le_bytes([
                volume_disk_extents[0],
                volume_disk_extents[1],
                volume_disk_extents[2],
                volume_disk_extents[3],
            ]);

            if num_extents > 0 {
                // Starting offset = 8-15
                // Partition size = 16-23
                let mut part_size_bytes = [0u8; 8];
                part_size_bytes.copy_from_slice(&volume_disk_extents[16..24]);
                let part_size = u64::from_le_bytes(part_size_bytes);

                return Some(part_size);
            }
        }
    }

    None
}

/// Obtiene el tamaño del volumen usando la ruta (para volúmenes lógicos)
fn get_volume_size_from_path(drive_path: &str) -> u64 {
    // Convertir la ruta del drive a formato Windows: "C:" -> "C:\"
    let path = if drive_path.ends_with('\\') {
        drive_path.to_string()
    } else if drive_path.len() == 2 && drive_path.chars().nth(1) == Some(':') {
        format!("{}\\", drive_path)
    } else {
        drive_path.to_string()
    };

    let wide_path: Vec<u16> = OsStr::new(&path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let mut free_bytes_available: u64 = 0;
    let mut total_bytes: u64 = 0;
    let mut total_free_bytes: u64 = 0;

    unsafe {
        let result = GetDiskFreeSpaceExW(
            wide_path.as_ptr(),
            &mut free_bytes_available as *mut u64 as *mut _,
            &mut total_bytes as *mut u64 as *mut _,
            &mut total_free_bytes as *mut u64 as *mut _,
        );

        if result != 0 {
            return total_bytes;
        }
    }

    0
}
