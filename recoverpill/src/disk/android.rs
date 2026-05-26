//! Módulo de recuperación para dispositivos Android
//!
//! Proporciona detección de dispositivos Android via ADB,
//! acceso MTP, y firmas específicas del ecosistema Android.

use log::{info, warn, error, debug};
use std::collections::HashMap;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::core::signatures::FileType;

/// Dispositivo Android detectado
#[derive(Debug, Clone)]
pub struct AndroidDevice {
    pub serial: String,
    pub model: String,
    pub manufacturer: String,
    pub android_version: String,
    pub is_rooted: bool,
    pub storage_size: u64,
    pub storage_free: u64,
    pub is_recovery_mode: bool,
    pub is_fastboot_mode: bool,
}

/// Resultado de escaneo Android
#[derive(Debug, Clone)]
pub struct AndroidScanResult {
    pub device: AndroidDevice,
    pub partitions: Vec<AndroidPartition>,
    pub found_files: Vec<AndroidFileEntry>,
    pub scan_time_ms: u64,
}

/// Partición de Android
#[derive(Debug, Clone)]
pub struct AndroidPartition {
    pub name: String,
    pub offset: u64,
    pub size: u64,
    pub fs_type: String, // ext4, f2fs, etc.
    pub is_mounted: bool,
    pub mount_point: Option<String>,
}

/// Archivo encontrado en dispositivo Android
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AndroidFileEntry {
    pub path: String,
    pub file_name: String,
    pub file_type: FileType,
    pub size: u64,
    pub offset: u64,
    pub is_deleted: bool,
    pub is_media: bool,
    pub mime_type: String,
    pub modified: u64,
    pub recoverability: f64,
    pub selected: bool,
    pub source_partition: String,
}

/// Motor de recuperación Android
pub struct AndroidRecoveryEngine {
    adb_path: String,
    devices: Vec<AndroidDevice>,
    should_stop: Arc<AtomicBool>,
}

impl AndroidRecoveryEngine {
    pub fn new() -> Self {
        AndroidRecoveryEngine {
            adb_path: Self::find_adb(),
            devices: Vec::new(),
            should_stop: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Encuentra ADB en el sistema
    fn find_adb() -> String {
        let candidates = vec![
            "adb".to_string(),
            "adb.exe".to_string(),
            r"C:\Program Files\Android\Android Studio\platform-tools\adb.exe".to_string(),
            r"C:\Program Files (x86)\Android\android-sdk\platform-tools\adb.exe".to_string(),
            r"C:\Android\platform-tools\adb.exe".to_string(),
            r"%LOCALAPPDATA%\Android\Sdk\platform-tools\adb.exe".to_string(),
        ];

        for candidate in candidates {
            let expanded = std::process::Command::new("where")
                .arg(&candidate)
                .output();
            if let Ok(output) = expanded {
                if output.status.success() {
                    info!("ADB encontrado: {}", candidate);
                    return candidate;
                }
            }
            if std::path::Path::new(&candidate).exists() {
                info!("ADB encontrado: {}", candidate);
                return candidate;
            }
        }

        warn!("ADB no encontrado en el sistema. La recuperación Android no estará disponible.");
        "adb".to_string()
    }

    /// Detecta dispositivos Android conectados via ADB
    pub fn detect_devices(&mut self) -> Vec<AndroidDevice> {
        info!("Detectando dispositivos Android...");
        let mut devices = Vec::new();

        let output = self.run_adb_command(&["devices", "-l"]);
        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                for line in stdout.lines().skip(1) {
                    if line.trim().is_empty() || line.contains("daemon") {
                        continue;
                    }
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let serial = parts[0].to_string();
                        let state = parts[1];

                        if state == "device" {
                            let model = self.get_device_property(&serial, "ro.product.model");
                            let manufacturer = self.get_device_property(&serial, "ro.product.manufacturer");
                            let android_ver = self.get_device_property(&serial, "ro.build.version.release");
                            let is_rooted = self.check_root(&serial);

                            let device = AndroidDevice {
                                serial,
                                model: model.unwrap_or_else(|| "Desconocido".to_string()),
                                manufacturer: manufacturer.unwrap_or_else(|| "Desconocido".to_string()),
                                android_version: android_ver.unwrap_or_else(|| "Desconocido".to_string()),
                                is_rooted,
                                storage_size: 0,
                                storage_free: 0,
                                is_recovery_mode: false,
                                is_fastboot_mode: false,
                            };
                            info!("Android detectado: {} {} (Serial: {})", device.manufacturer, device.model, device.serial);
                            devices.push(device);
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Error detectando dispositivos Android: {}", e);
            }
        }

        self.devices = devices.clone();
        devices
    }

    fn get_device_property(&self, serial: &str, prop: &str) -> Option<String> {
        let output = self.run_adb_serial_command(serial, &["shell", "getprop", prop]);
        match output {
            Ok(out) => {
                let val = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !val.is_empty() && val != "\n" {
                    Some(val)
                } else {
                    None
                }
            }
            Err(_) => None,
        }
    }

    fn check_root(&self, serial: &str) -> bool {
        let output = self.run_adb_serial_command(serial, &["shell", "su", "-c", "id"]);
        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                stdout.contains("uid=0")
            }
            Err(_) => false,
        }
    }

    /// Ejecuta comando ADB con serial específico
    fn run_adb_serial_command(&self, serial: &str, args: &[&str]) -> Result<Output, String> {
        let mut cmd_args = vec!["-s", serial];
        cmd_args.extend_from_slice(args);
        self.run_adb_command(&cmd_args)
    }

    fn run_adb_command(&self, args: &[&str]) -> Result<Output, String> {
        let start = Instant::now();
        let result = Command::new(&self.adb_path)
            .args(args)
            .output()
            .map_err(|e| format!("Error ejecutando ADB: {}", e))?;

        debug!("ADB {:?} completado en {:?}ms", args, start.elapsed().as_millis());
        Ok(result)
    }

    /// Escanea la partición de datos de Android
    pub fn scan_data_partition(
        &mut self,
        device: &AndroidDevice,
        progress_callback: impl Fn(String) + Send + 'static,
    ) -> AndroidScanResult {
        let start_time = Instant::now();
        let serial = &device.serial;
        let mut files = Vec::new();
        let mut partitions = Vec::new();

        info!("Iniciando escaneo Android en {} ({})", device.model, serial);

        // Obtener particiones
        partitions = self.get_partitions(serial);

        // Escanear áreas clave de Android
        let scan_paths = vec![
            "/data/media/0/DCIM",
            "/data/media/0/Pictures",
            "/data/media/0/Download",
            "/data/media/0/Documents",
            "/data/media/0/WhatsApp",
            "/data/media/0/Telegram",
            "/data/media/0/Android/media",
            "/data/media/0/Movies",
            "/data/media/0/Music",
            "/data/media/0/Recordings",
        ];

        for path in scan_paths {
            if self.should_stop.load(Ordering::SeqCst) {
                break;
            }
            progress_callback(format!("Escaneando {}...", path));

            let ls_output = if device.is_rooted {
                self.run_adb_serial_command(serial, &["shell", "su", "-c", &format!("ls -la {} 2>/dev/null", path)])
            } else {
                self.run_adb_serial_command(serial, &["shell", "ls", "-la", path])
            };

            if let Ok(output) = ls_output {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines().skip(1) {
                    if self.should_stop.load(Ordering::SeqCst) {
                        break;
                    }
                    if let Some(entry) = self.parse_file_entry(line, path, device) {
                        files.push(entry);
                    }
                }
            }
        }

        // Escaneo profundo de archivos borrados en /data
        if device.is_rooted {
            progress_callback("Escaneo profundo de archivos borrados (requiere root)...".to_string());
            if let Ok(output) = self.run_adb_serial_command(serial, &["shell", "su", "-c", "debugfs -R 'ls -l /' /dev/block/bootdevice/by-name/userdata 2>/dev/null"]) {
                // Parse deleted file entries
                // ...
            }
        }

        // Escaneo de base de datos SQLite de WhatsApp/media
        self.scan_whatsapp_databases(serial, device.is_rooted, &mut files);

        let scan_time = start_time.elapsed().as_millis() as u64;

        AndroidScanResult {
            device: device.clone(),
            partitions,
            found_files: files,
            scan_time_ms: scan_time,
        }
    }

    /// Escanea bases de datos de WhatsApp para recuperar multimedia
    fn scan_whatsapp_databases(&self, serial: &str, is_rooted: bool, files: &mut Vec<AndroidFileEntry>) {
        let db_paths = vec![
            "/data/data/com.whatsapp/databases/msgstore.db",
            "/data/data/com.whatsapp/databases/media.db",
            "/data/data/com.whatsapp/databases/wa.db",
            "/data/data/com.whatsapp/databases/axolotl.db",
        ];

        for db_path in db_paths {
            let cmd = if is_rooted {
                format!("su -c 'cat {}'", db_path)
            } else {
                format!("cat {}", db_path)
            };

            if let Ok(output) = self.run_adb_serial_command(serial, &["shell", &cmd]) {
                if output.status.success() && !output.stdout.is_empty() {
                    files.push(AndroidFileEntry {
                        path: db_path.to_string(),
                        file_name: db_path.rsplit('/').next().unwrap_or(db_path).to_string(),
                        file_type: FileType::Db,
                        size: output.stdout.len() as u64,
                        offset: 0,
                        is_deleted: false,
                        is_media: false,
                        mime_type: "application/x-sqlite3".to_string(),
                        modified: 0,
                        recoverability: 85.0,
                        selected: true,
                        source_partition: "data".to_string(),
                    });
                }
            }
        }
    }

    /// Obtiene particiones del dispositivo Android
    fn get_partitions(&self, serial: &str) -> Vec<AndroidPartition> {
        let mut partitions = Vec::new();

        // Intentar obtener particiones vía /proc/partitions
        if let Ok(output) = self.run_adb_serial_command(serial, &["shell", "cat", "/proc/partitions"]) {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines().skip(2) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    if let (Ok(blocks), Ok(size)) = (parts[2].parse::<u64>(), parts[3].parse::<u64>()) {
                        let name = parts[3].to_string();
                        partitions.push(AndroidPartition {
                            name,
                            offset: 0,
                            size: blocks * 1024,
                            fs_type: "ext4".to_string(), // asumimos ext4
                            is_mounted: false,
                            mount_point: None,
                        });
                    }
                }
            }
        }

        // Intentar obtener particiones montadas
        if let Ok(output) = self.run_adb_serial_command(serial, &["shell", "cat", "/proc/mounts"]) {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 && parts[0].contains("/dev/") {
                    partitions.push(AndroidPartition {
                        name: parts[0].to_string(),
                        offset: 0,
                        size: 0,
                        fs_type: parts[2].to_string(),
                        is_mounted: true,
                        mount_point: Some(parts[1].to_string()),
                    });
                }
            }
        }

        partitions
    }

    /// Parsea una línea de `ls -la` a un AndroidFileEntry
    fn parse_file_entry(&self, line: &str, base_path: &str, _device: &AndroidDevice) -> Option<AndroidFileEntry> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 8 {
            return None;
        }

        // Saltar directorios y enlaces
        if parts[0].starts_with('d') || parts[0].starts_with('l') {
            return None;
        }

        let size = parts[4].parse::<u64>().ok()?;
        if size == 0 {
            return None;
        }

        let name = *parts.last()?;
        if name == "." || name == ".." {
            return None;
        }

        let full_path = format!("{}/{}", base_path, name);
        let (file_type, mime) = self.detect_android_file_type(name);

        Some(AndroidFileEntry {
            path: full_path.clone(),
            file_name: name.to_string(),
            file_type,
            size,
            offset: 0,
            is_deleted: false,
            is_media: matches!(file_type, FileType::Jpeg | FileType::Png | FileType::Gif | FileType::Mp4 | FileType::Mp3),
            mime_type: mime,
            modified: 0,
            recoverability: 95.0,
            selected: true,
            source_partition: "data".to_string(),
        })
    }

    /// Detecta tipo de archivo Android por extensión
    fn detect_android_file_type(&self, name: &str) -> (FileType, String) {
        let lower = name.to_lowercase();
        if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
            (FileType::Jpeg, "image/jpeg".to_string())
        } else if lower.ends_with(".png") {
            (FileType::Png, "image/png".to_string())
        } else if lower.ends_with(".gif") {
            (FileType::Gif, "image/gif".to_string())
        } else if lower.ends_with(".mp4") {
            (FileType::Mp4, "video/mp4".to_string())
        } else if lower.ends_with(".mp3") {
            (FileType::Mp3, "audio/mpeg".to_string())
        } else if lower.ends_with(".apk") {
            (FileType::Apk, "application/vnd.android.package-archive".to_string())
        } else if lower.ends_with(".dex") {
            (FileType::Dex, "application/x-dex".to_string())
        } else if lower.ends_with(".db") || lower.ends_with(".sqlite") {
            (FileType::Db, "application/x-sqlite3".to_string())
        } else if lower.ends_with(".pdf") {
            (FileType::Pdf, "application/pdf".to_string())
        } else if lower.ends_with(".doc") || lower.ends_with(".docx") {
            (FileType::Docx, "application/msword".to_string())
        } else if lower.ends_with(".xml") {
            (FileType::Xml, "application/xml".to_string())
        } else if lower.ends_with(".ogg") {
            (FileType::Ogg, "audio/ogg".to_string())
        } else if lower.ends_with(".webp") {
            (FileType::Webp, "image/webp".to_string())
        } else if lower.ends_with(".heic") {
            (FileType::Heic, "image/heic".to_string())
        } else if lower.ends_with(".3gp") {
            (FileType::ThreeGp, "video/3gpp".to_string())
        } else if lower.ends_with(".avi") {
            (FileType::Avi, "video/x-msvideo".to_string())
        } else if lower.ends_with(".mkv") {
            (FileType::MkV, "video/x-matroska".to_string())
        } else if lower.ends_with(".zip") || lower.ends_with(".jar") {
            (FileType::Zip, "application/zip".to_string())
        } else if lower.ends_with(".txt") {
            (FileType::Text, "text/plain".to_string())
        } else {
            (FileType::AndroidFile, "application/octet-stream".to_string())
        }
    }

    /// Recupera un archivo de dispositivo Android via ADB
    pub fn recover_file(
        &self,
        serial: &str,
        remote_path: &str,
        local_path: &std::path::Path,
    ) -> Result<u64, String> {
        info!("Recuperando {} desde {} a {:?}", remote_path, serial, local_path);

        let output = self.run_adb_serial_command(serial, &["pull", remote_path, &local_path.to_string_lossy()])?;

        if output.status.success() {
            let size = std::fs::metadata(local_path)
                .map(|m| m.len())
                .unwrap_or(0);
            info!("Archivo recuperado: {} ({} bytes)", remote_path, size);
            Ok(size)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("Error ADB pull: {}", stderr))
        }
    }

    /// Hace backup completo del dispositivo Android
    pub fn backup_device(
        &self,
        serial: &str,
        output_dir: &std::path::Path,
        progress_callback: impl Fn(String),
    ) -> Result<Vec<std::path::PathBuf>, String> {
        info!("Iniciando backup Android de {}", serial);
        let mut recovered = Vec::new();

        let backup_paths = vec![
            "/data/media/0/DCIM",
            "/data/media/0/Pictures",
            "/data/media/0/Download",
            "/data/media/0/Documents",
        ];

        for path in backup_paths {
            if self.should_stop.load(Ordering::SeqCst) {
                break;
            }

            let dest_dir = output_dir.join(
                path.trim_start_matches("/data/media/0/")
            );
            std::fs::create_dir_all(&dest_dir)
                .map_err(|e| format!("Error creando directorio: {}", e))?;

            progress_callback(format!("Respaldando {}...", path));

            let output = self.run_adb_serial_command(serial, &[
                "pull", path, &dest_dir.to_string_lossy()
            ])?;

            if output.status.success() {
                recovered.push(dest_dir);
            }
        }

        Ok(recovered)
    }

    pub fn stop(&self) {
        self.should_stop.store(true, Ordering::SeqCst);
    }

    pub fn is_available() -> bool {
        let output = Command::new("adb")
            .arg("version")
            .output();
        matches!(output, Ok(o) if o.status.success())
    }
}

impl Default for AndroidRecoveryEngine {
    fn default() -> Self {
        Self::new()
    }
}
