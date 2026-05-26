//! Aplicación principal de recoverPill
//! Interfaz gráfica con egui para la recuperación de datos.
//! Con soporte multi-pestaña: Escaneo, Android, Configuración.

use eframe::{egui, App};
use log::{info, error};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::path::PathBuf;

use crate::ai::entropy::{entropy_description, entropy_emoji, entropy_color};
use crate::build_info::BUILD_DATE;
use crate::core::scanner::{FoundFile, ScanProgress, Scanner};
use crate::core::signatures::get_categories;
use crate::disk::drive_info::{get_available_drives, DriveInfo};
use crate::disk::android::{AndroidDevice, AndroidRecoveryEngine, AndroidScanResult};

const APP_TITLE: &str = "recoverPill - Recuperación de Datos";
const PANEL_BG: egui::Color32 = egui::Color32::from_rgb(30, 32, 40);
const CARD_BG: egui::Color32 = egui::Color32::from_rgb(40, 44, 55);
const CARD_HOVER: egui::Color32 = egui::Color32::from_rgb(50, 55, 70);
const ACCENT_COLOR: egui::Color32 = egui::Color32::from_rgb(66, 135, 245);
const ACCENT_LIGHT: egui::Color32 = egui::Color32::from_rgb(100, 170, 255);
const SUCCESS_COLOR: egui::Color32 = egui::Color32::from_rgb(76, 175, 80);
const WARNING_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 193, 7);
const ERROR_COLOR: egui::Color32 = egui::Color32::from_rgb(244, 67, 54);
const ANDROID_COLOR: egui::Color32 = egui::Color32::from_rgb(60, 185, 96); // Green Android

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MainTab {
    Scan,
    Android,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScanMode {
    Signature,
    FileSystem,
}

pub struct RecoverPillApp {
    // Navegación
    current_tab: MainTab,

    // Escaneo
    drives: Vec<DriveInfo>,
    selected_drive: Option<usize>,
    scanner: Option<Scanner>,
    is_scanning: bool,
    is_recovering: bool,
    scan_progress: ScanProgress,
    recovery_progress: f64,
    found_files: Vec<FoundFile>,
    scan_percentage: f64,
    last_ten_percent: i32,
    current_notification: Option<String>,
    notification_timer: f32,
    output_folder: String,
    should_stop: Arc<AtomicBool>,
    scan_result_receiver:
        Option<std::sync::mpsc::Receiver<Result<crate::core::scanner::ScanResult, String>>>,
    progress_receiver: Option<std::sync::mpsc::Receiver<String>>,
    recovery_receiver: Option<std::sync::mpsc::Receiver<RecoveryResult>>,
    recovery_progress_receiver: Option<std::sync::mpsc::Receiver<String>>,
    enabled_filters: Vec<String>,
    all_filters_enabled: bool,
    type_filter: Option<String>,
    selected_individual_types: std::collections::HashSet<String>,
    console_messages: Vec<ConsoleMessage>,
    selected_file: Option<usize>,
    preview_data: Option<Vec<u8>>,
    preview_file_index: Option<usize>,
    preview_width: u32,
    preview_height: u32,
    preview_loading: bool,
    preview_error: Option<String>,
    preview_texture: Option<egui::TextureHandle>,
    preview_receiver: Option<std::sync::mpsc::Receiver<PreviewResult>>,
    last_drive_path: Option<String>,
    filter_text: String,
    quality_filter_enabled: bool,
    hide_duplicates: bool,
    min_recoverability: f64,
    sort_by: SortOption,
    sort_ascending: bool,
    current_page: usize,
    items_per_page: usize,
    scan_mode: ScanMode,
    disk_map: Vec<u8>, // 0: unread, 1: scanning, 2: scanned, 3: found, 4: error

    // Configuración avanzada
    multi_pass_enabled: bool,
    multi_pass_count: u32,
    footer_detection_enabled: bool,

    // Android
    android_engine: Option<AndroidRecoveryEngine>,
    android_devices: Vec<AndroidDevice>,
    android_selected_device: Option<usize>,
    android_is_scanning: bool,
    android_scan_result: Option<AndroidScanResult>,
    android_scan_progress: String,
    android_output_folder: String,
    android_backup_in_progress: bool,
    adb_available: bool,

    // Sesión
    current_session_file: Option<PathBuf>,
    status_message: String,
    status_timer: f32,

    // Canales para Android
    android_scan_receiver: Option<std::sync::mpsc::Receiver<AndroidScanResult>>,
    android_backup_receiver: Option<std::sync::mpsc::Receiver<Result<Vec<std::path::PathBuf>, String>>>,

    // Timing para ETA
    recovery_start_time: Option<std::time::Instant>,
    recovery_eta: String,

    // Confirmación de detención
    stop_confirm: bool,
}

struct RecoveryResult {
    success_count: usize,
    fail_count: usize,
    first_names: Vec<String>,
    error: Option<String>,
}

struct PreviewResult {
    index: usize,
    data: Option<Vec<u8>>,
    width: u32,
    height: u32,
    error: Option<String>,
}

#[derive(Debug, Clone)]
struct ConsoleMessage {
    text: String,
    level: ConsoleLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConsoleLevel {
    Info,
    Warning,
    Error,
    Success,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortOption {
    Name,
    Type,
    Size,
    Recoverability,
    Entropy,
}

impl RecoverPillApp {
    pub fn new() -> Self {
        info!("Inicializando recoverPill UI");

        let drives = get_available_drives();
        let categories = get_categories();
        let enabled_filters: Vec<String> = categories.iter().map(|s| s.to_string()).collect();

        let adb_available = AndroidRecoveryEngine::is_available();
        if adb_available {
            info!("ADB disponible - módulo Android activo");
            let mut engine = AndroidRecoveryEngine::new();
            let devices = engine.detect_devices();
            RecoverPillApp {
                drives,
                current_tab: MainTab::Scan,
                selected_drive: None,
                scanner: None,
                is_scanning: false,
                is_recovering: false,
                scan_progress: ScanProgress::new(0),
                recovery_progress: 0.0,
                found_files: Vec::new(),
                scan_percentage: 0.0,
                last_ten_percent: -1,
                current_notification: None,
                notification_timer: 0.0,
                should_stop: Arc::new(AtomicBool::new(false)),
                scan_result_receiver: None,
                progress_receiver: None,
                recovery_receiver: None,
                recovery_progress_receiver: None,
                enabled_filters,
                all_filters_enabled: true,
                type_filter: None,
                selected_individual_types: std::collections::HashSet::new(),
                console_messages: vec![ConsoleMessage {
                    text: format!("recoverPill v1.0.0 listo (Compilado: {})", BUILD_DATE),
                    level: ConsoleLevel::Info,
                }],
                selected_file: None,
                preview_data: None,
                preview_file_index: None,
                preview_width: 0,
                preview_height: 0,
                preview_loading: false,
                preview_error: None,
                preview_texture: None,
                preview_receiver: None,
                last_drive_path: None,
                filter_text: String::new(),
                quality_filter_enabled: false,
                hide_duplicates: true,
                min_recoverability: 70.0,
                output_folder: String::new(),
                sort_by: SortOption::Recoverability,
                sort_ascending: false,
                current_page: 0,
                items_per_page: 200,
                scan_mode: ScanMode::Signature,
                disk_map: vec![0u8; 1000],
                multi_pass_enabled: false,
                multi_pass_count: 2,
                footer_detection_enabled: true,
                android_engine: Some(engine),
                android_devices: devices,
                android_selected_device: None,
                android_is_scanning: false,
                android_scan_result: None,
                android_scan_progress: String::new(),
                android_output_folder: String::new(),
                android_backup_in_progress: false,
                adb_available,
                current_session_file: None,
                status_message: String::new(),
                status_timer: 0.0,
                android_scan_receiver: None,
                android_backup_receiver: None,
                recovery_start_time: None,
                recovery_eta: String::new(),
                stop_confirm: false,
            }
        } else {
            RecoverPillApp {
                drives,
                current_tab: MainTab::Scan,
                selected_drive: None,
                scanner: None,
                is_scanning: false,
                is_recovering: false,
                scan_progress: ScanProgress::new(0),
                recovery_progress: 0.0,
                found_files: Vec::new(),
                scan_percentage: 0.0,
                last_ten_percent: -1,
                current_notification: None,
                notification_timer: 0.0,
                should_stop: Arc::new(AtomicBool::new(false)),
                scan_result_receiver: None,
                progress_receiver: None,
                recovery_receiver: None,
                recovery_progress_receiver: None,
                enabled_filters,
                all_filters_enabled: true,
                type_filter: None,
                selected_individual_types: std::collections::HashSet::new(),
                console_messages: vec![ConsoleMessage {
                    text: format!("recoverPill v1.0.0 listo (Compilado: {})", BUILD_DATE),
                    level: ConsoleLevel::Info,
                }],
                selected_file: None,
                preview_data: None,
                preview_file_index: None,
                preview_width: 0,
                preview_height: 0,
                preview_loading: false,
                preview_error: None,
                preview_texture: None,
                preview_receiver: None,
                last_drive_path: None,
                filter_text: String::new(),
                quality_filter_enabled: false,
                hide_duplicates: true,
                min_recoverability: 70.0,
                output_folder: String::new(),
                sort_by: SortOption::Recoverability,
                sort_ascending: false,
                current_page: 0,
                items_per_page: 200,
                scan_mode: ScanMode::Signature,
                disk_map: vec![0u8; 1000],
                multi_pass_enabled: false,
                multi_pass_count: 2,
                footer_detection_enabled: true,
                android_engine: None,
                android_devices: Vec::new(),
                android_selected_device: None,
                android_is_scanning: false,
                android_scan_result: None,
                android_scan_progress: String::new(),
                android_output_folder: String::new(),
                android_backup_in_progress: false,
                adb_available,
                current_session_file: None,
                status_message: String::new(),
                status_timer: 0.0,
                android_scan_receiver: None,
                android_backup_receiver: None,
                recovery_start_time: None,
                recovery_eta: String::new(),
                stop_confirm: false,
            }
        }
    }

    fn add_console_message(&mut self, text: String, level: ConsoleLevel) {
        self.console_messages.push(ConsoleMessage { text, level });
        if self.console_messages.len() > 100 {
            self.console_messages.remove(0);
        }
    }

    fn get_current_type_filters(&self) -> Option<Vec<crate::core::signatures::FileType>> {
        use crate::core::signatures::FileType;
        self.type_filter.as_ref().map(|filter| {
            filter.split(',')
                .filter_map(|ext| {
                    match ext.trim().to_lowercase().as_str() {
                        "jpg" | "jpeg" => Some(FileType::Jpeg),
                        "png" => Some(FileType::Png),
                        "gif" => Some(FileType::Gif),
                        "bmp" => Some(FileType::Bmp),
                        "webp" => Some(FileType::Webp),
                        "heic" => Some(FileType::Heic),
                        "raw" => Some(FileType::Raw),
                        "tiff" => Some(FileType::Tiff),
                        "ico" => Some(FileType::Ico),
                        "psd" => Some(FileType::Psd),
                        "ai" => Some(FileType::Ai),
                        "svg" => Some(FileType::Svg),
                        "mp3" => Some(FileType::Mp3),
                        "wav" => Some(FileType::Wav),
                        "flac" => Some(FileType::Flac),
                        "aac" => Some(FileType::Aac),
                        "ogg" => Some(FileType::Ogg),
                        "wma" => Some(FileType::Wma),
                        "mp4" => Some(FileType::Mp4),
                        "avi" => Some(FileType::Avi),
                        "mkv" => Some(FileType::MkV),
                        "mov" => Some(FileType::Mov),
                        "wmv" => Some(FileType::Wmv),
                        "webm" => Some(FileType::WebM),
                        "flv" => Some(FileType::Flv),
                        "pdf" => Some(FileType::Pdf),
                        "doc" => Some(FileType::Doc),
                        "docx" => Some(FileType::Docx),
                        "xls" => Some(FileType::Xls),
                        "xlsx" => Some(FileType::Xlsx),
                        "ppt" => Some(FileType::Ppt),
                        "pptx" => Some(FileType::Pptx),
                        "odt" => Some(FileType::Odt),
                        "zip" => Some(FileType::Zip),
                        "rar" => Some(FileType::Rar),
                        "7z" => Some(FileType::SevenZip),
                        "tar" => Some(FileType::Tar),
                        "gz" => Some(FileType::Gzip),
                        "exe" => Some(FileType::Exe),
                        "dll" => Some(FileType::Dll),
                        "msi" => Some(FileType::Msi),
                        _ => None,
                    }
                })
                .collect()
        })
    }

    fn start_scan(&mut self) {
        if self.selected_drive.is_none() {
            self.add_console_message(
                "Seleccione una unidad primero".to_string(),
                ConsoleLevel::Warning,
            );
            return;
        }

        if self.is_scanning {
            self.add_console_message(
                "Ya hay un escaneo en progreso".to_string(),
                ConsoleLevel::Warning,
            );
            return;
        }

        let drive_index = self.selected_drive.unwrap();
        if drive_index >= self.drives.len() {
            self.add_console_message("Unidad inválida".to_string(), ConsoleLevel::Error);
            return;
        }

        let drive = &self.drives[drive_index];
        let drive_path = drive.path.clone();
        let drive_size = drive.total_bytes;

        self.last_drive_path = Some(drive_path.clone());

        let scan_mode = self.scan_mode;
        let mode_text = match scan_mode {
            ScanMode::Signature => "escaneo por firmas",
            ScanMode::FileSystem => "escaneo del sistema de archivos",
        };

        self.add_console_message(
            format!("🚀 Iniciando {} de {}...", mode_text, drive_path),
            ConsoleLevel::Info,
        );
        
        let type_filters = self.get_current_type_filters();
        if let Some(ref filters) = type_filters {
            self.add_console_message(
                format!("🔍 Filtrando por {} tipos de archivo", filters.len()),
                ConsoleLevel::Info,
            );
        }

        self.add_console_message(
            format!(
                "💾 Tamaño de unidad: {} ({})",
                DriveInfo::format_size(drive_size),
                drive_size
            ),
            ConsoleLevel::Info,
        );
        self.add_console_message(
            format!(
                "🔍 Modo: {}",
                match scan_mode {
                    ScanMode::Signature => "Escaneo Profundo (busca archivos borrados)",
                    ScanMode::FileSystem => "Escaneo Superficial (sistema de archivos)",
                }
            ),
            ConsoleLevel::Info,
        );
        self.add_console_message(
            "⏳ Procesando... revise la consola para ver el progreso detallado".to_string(),
            ConsoleLevel::Info,
        );

        self.found_files.clear();
        self.selected_file = None;
        self.is_scanning = true;
        self.scan_progress.is_running = true;
        self.scan_percentage = 0.0;
        self.last_ten_percent = -1;
        self.current_notification = None;
        self.notification_timer = 0.0;
        self.preview_data = None;
        self.preview_file_index = None;

        self.should_stop.store(false, Ordering::SeqCst);

        let (progress_tx, progress_rx) = std::sync::mpsc::channel();
        self.progress_receiver = Some(progress_rx);

        let should_stop = self.should_stop.clone();
        let min_recoverability = self.min_recoverability;
        let multi_pass = self.multi_pass_enabled;
        let multi_pass_count = self.multi_pass_count;
        let footer_detection = self.footer_detection_enabled;
        let (tx, rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || match Scanner::new(&drive_path) {
            Ok(mut scanner) => {
                let scanner_stop = scanner.get_should_stop();
                scanner_stop.store(should_stop.load(Ordering::SeqCst), Ordering::SeqCst);

                // Aplicar configuración avanzada (Bug 5)
                scanner.set_footer_detection(footer_detection);
                if multi_pass {
                    scanner.set_scan_passes(multi_pass_count);
                }

                // Copiar referencia del flag del scanner para que stop_watcher pueda detenerlo
                let scanner_stop_flag = scanner_stop.clone();

                let _stop_watcher = std::thread::spawn(move || {
                    while !should_stop.load(Ordering::SeqCst) {
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    scanner_stop_flag.store(true, Ordering::SeqCst);
                });

                let progress_tx_clone = progress_tx.clone();
                let result = match scan_mode {
                    ScanMode::Signature => {
                        if multi_pass {
                            scanner.scan_multi_pass(type_filters, multi_pass_count, min_recoverability, move |msg| {
                                let _ = progress_tx_clone.send(msg);
                            })
                        } else {
                            scanner.scan_with_progress(type_filters, min_recoverability, move |msg| {
                                let _ = progress_tx_clone.send(msg);
                            })
                        }
                    },
                    ScanMode::FileSystem => scanner.scan_filesystem(move |msg| {
                        let _ = progress_tx_clone.send(msg);
                    }),
                };

                // Enviar resultado inmediatamente, sin esperar stop_watcher
                let _ = tx.send(Ok(result));
            }
            Err(e) => {
                let _ = tx.send(Err(e));
            }
        });

        self.scan_result_receiver = Some(rx);
    }

    fn process_scan_results(&mut self) {
        let progress_msgs: Vec<String> = if let Some(ref rx) = self.progress_receiver {
            let mut msgs = Vec::new();
            while let Ok(msg) = rx.try_recv() {
                msgs.push(msg);
            }
            msgs
        } else {
            Vec::new()
        };

        for msg in &progress_msgs {
            // Mostrar todos los mensajes en la consola
            self.add_console_message(msg.clone(), ConsoleLevel::Info);

            // Detectar si el mensaje informa sobre archivos encontrados y actualizar en tiempo real
            // El scanner envía los archivos con el prefijo ">>> DATA:" seguido del JSON del archivo
            if msg.starts_with(">>> DATA:") {
                let json = &msg[9..];
                if let Ok(file) = serde_json::from_str::<FoundFile>(json) {
                    self.found_files.push(file);
                    // Re-aplicar ordenación actual para mantener consistencia
                    if !self.found_files.is_empty() {
                        self.sort_files(self.sort_by, self.sort_ascending);
                    }
                }
                continue;
            }
            
            // Extraer porcentaje y otros mensajes
            if let Some(percent_str) = msg.split("Progreso: ").nth(1) {
                if let Some(percent) = percent_str.split('%').next() {
                    if let Ok(p) = percent.trim().parse::<f64>() {
                        self.scan_percentage = p;
                    }
                }
            }
        }

        if self.notification_timer > 0.0 {
            self.notification_timer -= 0.016;
            if self.notification_timer <= 0.0 {
                self.current_notification = None;
            }
        }

        if let Some(ref rx) = self.scan_result_receiver {
            match rx.try_recv() {
                Ok(Ok(result)) => {
                    // Reemplazar la lista con los archivos encontrados
                    self.found_files = result.files_found;

                    self.is_scanning = false;
                    self.scan_progress.is_running = false;
                    self.scan_percentage = 100.0;
                    self.scan_result_receiver = None;
                    self.progress_receiver = None;

                    // Mensaje de resumen
                    self.add_console_message(
                        format!(
                            "✅ Escaneo completado: {} archivos encontrados",
                            self.found_files.len()
                        ),
                        ConsoleLevel::Success,
                    );
                }
                Ok(Err(e)) => {
                    self.is_scanning = false;
                    self.scan_progress.is_running = false;
                    self.scan_result_receiver = None;
                    self.progress_receiver = None;
                    self.add_console_message(
                        format!("Error en escaneo: {}", e),
                        ConsoleLevel::Error,
                    );
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.is_scanning = false;
                    self.scan_progress.is_running = false;
                    self.scan_result_receiver = None;
                    self.progress_receiver = None;

                    let count = self.found_files.len();
                    if count > 0 {
                        self.add_console_message(
                            format!("Escaneo detenido: {} archivos encontrados", count),
                            ConsoleLevel::Warning,
                        );
                    } else {
                        self.add_console_message(
                            "No se encontraron archivos".to_string(),
                            ConsoleLevel::Warning,
                        );
                    }
                }
            }
        }
    }

    fn process_android_results(&mut self) {
        if let Some(ref rx) = self.android_scan_receiver {
            match rx.try_recv() {
                Ok(result) => {
                    self.android_scan_result = Some(result.clone());
                    self.android_is_scanning = false;
                    self.android_scan_receiver = None;
                    self.add_console_message(
                        format!("✅ Escaneo Android completado: {} archivos en {}ms",
                            result.found_files.len(), result.scan_time_ms),
                        ConsoleLevel::Success,
                    );
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.android_is_scanning = false;
                    self.android_scan_receiver = None;
                }
            }
        }

        if let Some(ref rx) = self.android_backup_receiver {
            match rx.try_recv() {
                Ok(Ok(dirs)) => {
                    self.android_backup_in_progress = false;
                    self.android_backup_receiver = None;
                    self.add_console_message(
                        format!("✅ Backup Android completado: {} directorios", dirs.len()),
                        ConsoleLevel::Success,
                    );
                }
                Ok(Err(e)) => {
                    self.android_backup_in_progress = false;
                    self.android_backup_receiver = None;
                    self.add_console_message(
                        format!("❌ Error en backup Android: {}", e),
                        ConsoleLevel::Error,
                    );
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.android_backup_in_progress = false;
                    self.android_backup_receiver = None;
                }
            }
        }
    }

    fn stop_scan(&mut self) {
        self.should_stop.store(true, Ordering::SeqCst);
        self.stop_confirm = false;
    }

    fn stop_recovery(&mut self) {
        self.is_recovering = false;
        self.recovery_receiver = None;
        self.recovery_progress_receiver = None;
        self.recovery_progress = 0.0;
        self.add_console_message(
            "⏹ Recuperación detenida por el usuario".to_string(),
            ConsoleLevel::Warning,
        );
    }

    fn select_by_category(&mut self, category: &str) {
        let mut count = 0;
        let min_quality = if self.quality_filter_enabled { self.min_recoverability } else { 0.0 };
        
        for file in &mut self.found_files {
            // Deseleccionar todo primero
            file.selected = false;
            
            let file_category = file.file_type.category();
            if file_category == category && file.recoverability >= min_quality {
                file.selected = true;
                count += 1;
            }
        }
        
        if self.quality_filter_enabled {
            self.add_console_message(
                format!("✅ {} archivos de {} con calidad >= {:.0}% seleccionados", count, category, self.min_recoverability),
                ConsoleLevel::Success,
            );
        } else {
            self.add_console_message(
                format!("✅ {} archivos de {} seleccionados", count, category),
                ConsoleLevel::Success,
            );
        }
    }

    fn toggle_file_selection(&mut self, index: usize) {
        if index < self.found_files.len() {
            self.found_files[index].selected = !self.found_files[index].selected;
        }
    }

    fn select_all(&mut self) {
        let mut count = 0;
        let filter_text = self.filter_text.to_lowercase();
        let type_pattern = self.type_filter.clone();
        let min_quality = if self.quality_filter_enabled { self.min_recoverability } else { 0.0 };

        for file in &mut self.found_files {
            // Primero deseleccionar todo
            file.selected = false;
            
            // Verificar si el archivo pasaría los filtros actuales
            let text_match = filter_text.is_empty() 
                || file.file_name.to_lowercase().contains(&filter_text)
                || file.file_type.extension().to_lowercase().contains(&filter_text);
                
            let type_match = if let Some(ref pattern) = type_pattern {
                let ext = file.file_type.extension().to_lowercase();
                pattern.split(',').any(|p| p.trim().to_lowercase() == ext)
            } else {
                true
            };
            
            let quality_match = file.recoverability >= min_quality;

            if text_match && type_match && quality_match {
                file.selected = true;
                count += 1;
            }
        }

        let msg = if count > 0 {
            format!("✅ Seleccionados {} archivos (respetando filtros actuales)", count)
        } else {
            "No hay archivos que coincidan con los filtros para seleccionar".to_string()
        };
        
        self.add_console_message(msg, ConsoleLevel::Success);
    }

    fn deselect_all(&mut self) {
        for file in &mut self.found_files {
            file.selected = false;
        }
        self.add_console_message(
            "Todos los archivos deseleccionados".to_string(),
            ConsoleLevel::Info,
        );
    }

    fn clear_all_files(&mut self) {
        let count = self.found_files.len();
        self.found_files.clear();
        self.selected_file = None;
        if count > 0 {
            self.add_console_message(
                format!("{} archivos eliminados de la lista", count),
                ConsoleLevel::Info,
            );
        }
    }

    fn get_selected_files(&self) -> Vec<&FoundFile> {
        self.found_files.iter().filter(|f| f.selected).collect()
    }

    fn recover_selected_files(&mut self) {
        let output_folder = self.output_folder.clone();
        let drive_idx = self.selected_drive;

        // Obtener filtro de calidad activo
        let min_quality = if self.quality_filter_enabled { self.min_recoverability } else { 0.0 };

        // Filtrar archivos seleccionados por calidad
        let files_to_recover: Vec<_> = self
            .found_files
            .iter()
            .filter(|f| f.selected && f.recoverability >= min_quality)
            .map(|f| (f.offset, f.file_type.clone(), f.estimated_size))
            .collect();

        let selected_count = files_to_recover.len();

        if selected_count == 0 {
            self.add_console_message(
                "No hay archivos seleccionados para recuperar".to_string(),
                ConsoleLevel::Warning,
            );
            return;
        }

        if output_folder.is_empty() {
            self.add_console_message(
                "Define una carpeta de recuperación primero".to_string(),
                ConsoleLevel::Warning,
            );
            return;
        }

        if self.is_recovering {
            self.add_console_message(
                "Ya hay una recuperación en progreso".to_string(),
                ConsoleLevel::Warning,
            );
            return;
        }

        self.add_console_message(
            format!("Iniciando recuperación de {} archivos...", selected_count),
            ConsoleLevel::Info,
        );

        let drive_idx = match drive_idx {
            Some(i) => i,
            None => {
                self.add_console_message(
                    "No hay unidad seleccionada".to_string(),
                    ConsoleLevel::Error,
                );
                return;
            }
        };

        if drive_idx >= self.drives.len() {
            self.add_console_message("Unidad inválida".to_string(), ConsoleLevel::Error);
            return;
        }

        let drive_path = self.drives[drive_idx].path.clone();
        self.is_recovering = true;
        self.recovery_start_time = Some(std::time::Instant::now());
        self.recovery_eta = String::new();

        let (tx, rx) = std::sync::mpsc::channel();
        self.recovery_receiver = Some(rx);

        let (progress_tx, progress_rx) = std::sync::mpsc::channel();
        self.recovery_progress_receiver = Some(progress_rx);

        std::thread::spawn(move || {
            use crate::core::recovery::RecoveryEngine;
            use crate::disk::access::DiskReader;

            let output_dir = std::path::Path::new(&output_folder);
            let mut engine = match RecoveryEngine::new(output_dir) {
                Ok(e) => e,
                Err(e) => {
                    let _ = tx.send(RecoveryResult {
                        success_count: 0,
                        fail_count: 0,
                        first_names: Vec::new(),
                        error: Some(format!("Error: {}", e)),
                    });
                    return;
                }
            };

            let mut reader = match DiskReader::open(&drive_path) {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx.send(RecoveryResult {
                        success_count: 0,
                        fail_count: 0,
                        first_names: Vec::new(),
                        error: Some(format!("Error al abrir disco: {}", e)),
                    });
                    return;
                }
            };

            let total_files = files_to_recover.len();
            let mut success_count = 0;
            let mut fail_count = 0;
            let mut first_names = Vec::new();

            for (idx, (offset, file_type, size)) in files_to_recover.iter().enumerate() {
                let temp_file = FoundFile {
                    offset: *offset,
                    file_type: file_type.clone(),
                    file_name: String::new(),
                    estimated_size: *size,
                    recoverability: 0.0,
                    entropy: 0.0,
                    signature_matched: String::new(),
                    selected: false,
                    is_validated: false,
                    content_hash: None,
                    is_duplicate: false,
                    duplicate_group: None,
                };

                let _ = progress_tx.send(format!(
                    "📄 [{}/{}] Recuperando {} en 0x{:X} ({} bytes)...",
                    idx + 1,
                    total_files,
                    file_type.extension().to_uppercase(),
                    offset,
                    size
                ));

                match engine.recover_file(&mut reader, &temp_file) {
                    Ok(path) => {
                        success_count += 1;
                        let file_name = path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();

                        if success_count <= 5 {
                            first_names.push(file_name.clone());
                        }

                        let _ = progress_tx.send(format!(
                            "✅ [{}/{}] Recuperado: {}",
                            idx + 1,
                            total_files,
                            file_name
                        ));
                    }
                    Err(e) => {
                        fail_count += 1;
                        let _ = progress_tx.send(format!(
                            "❌ [{}/{}] Error recuperando {} en 0x{:X}: {}",
                            idx + 1,
                            total_files,
                            file_type.extension().to_uppercase(),
                            offset,
                            e
                        ));
                    }
                }
            }

            let _ = tx.send(RecoveryResult {
                success_count,
                fail_count,
                first_names,
                error: None,
            });
        });
    }

    fn process_recovery_results(&mut self) {
        let result = if let Some(ref rx) = self.recovery_receiver {
            match rx.try_recv() {
                Ok(result) => Some(result),
                Err(_) => None,
            }
        } else {
            None
        };

        if let Some(result) = result {
            self.recovery_receiver = None;
            self.recovery_progress_receiver = None;
            self.is_recovering = false;

            if let Some(error) = result.error {
                self.add_console_message(error, ConsoleLevel::Error);
            } else {
                for name in &result.first_names {
                    self.add_console_message(
                        format!("✅ Recuperado: {}", name),
                        ConsoleLevel::Success,
                    );
                }

                self.add_console_message(
                    format!(
                        "🎉 Recuperación completada: {} archivos guardados, {} errores",
                        result.success_count, result.fail_count
                    ),
                    ConsoleLevel::Success,
                );
            }
        }
    }

    fn process_recovery_progress(&mut self) {
        let progress_msgs: Vec<String> = if let Some(ref rx) = self.recovery_progress_receiver {
            let mut msgs = Vec::new();
            while let Ok(msg) = rx.try_recv() {
                msgs.push(msg);
            }
            msgs
        } else {
            Vec::new()
        };

        for msg in progress_msgs {
            // Extraer porcentaje de progreso de mensajes como "[1/10] Recuperando..."
            if let Some(bracket_start) = msg.find('[') {
                if let Some(bracket_end) = msg.find(']') {
                    let bracket_content = &msg[bracket_start + 1..bracket_end];
                    if let Some(slash_pos) = bracket_content.find('/') {
                        if let (Ok(current), Ok(total)) = (
                            bracket_content[..slash_pos].parse::<usize>(),
                            bracket_content[slash_pos + 1..].parse::<usize>(),
                        ) {
                            if total > 0 {
                                self.recovery_progress = (current as f64 / total as f64) * 100.0;
                            }
                        }
                    }
                }
            }

            if msg.contains("Recuperado:") {
                self.add_console_message(msg, ConsoleLevel::Success);
            } else if msg.contains("Error") {
                self.add_console_message(msg, ConsoleLevel::Error);
            } else {
                self.add_console_message(msg, ConsoleLevel::Info);
            }
        }

        // Calcular ETA
        if self.recovery_progress > 0.0 && self.recovery_progress < 100.0 {
            if let Some(start) = self.recovery_start_time {
                let elapsed = start.elapsed().as_secs_f64();
                let total_estimated = elapsed * 100.0 / self.recovery_progress;
                let remaining = total_estimated - elapsed;
                if remaining > 0.0 && remaining.is_finite() {
                    let total_secs = remaining as u64;
                    let hours = total_secs / 3600;
                    let mins = (total_secs % 3600) / 60;
                    let secs = total_secs % 60;
                    self.recovery_eta = if hours > 0 {
                        format!("ETA: {}h {:02}m {:02}s", hours, mins, secs)
                    } else if mins > 0 {
                        format!("ETA: {}m {:02}s", mins, secs)
                    } else {
                        format!("ETA: {}s", secs)
                    };
                }
            }
        }
    }

    fn set_output_folder(&mut self, folder: String) {
        self.output_folder = folder.clone();
        self.add_console_message(
            format!("Carpeta de recuperación configurada: {}", folder),
            ConsoleLevel::Success,
        );
    }

    fn sort_files(&mut self, sort_by: SortOption, ascending: bool) {
        self.sort_by = sort_by;
        self.sort_ascending = ascending;

        match sort_by {
            SortOption::Name => {
                self.found_files.sort_by(|a, b| {
                    let cmp = a.file_name.to_lowercase().cmp(&b.file_name.to_lowercase());
                    if ascending {
                        cmp
                    } else {
                        cmp.reverse()
                    }
                });
            }
            SortOption::Type => {
                self.found_files.sort_by(|a, b| {
                    let cmp = a.file_type.extension().cmp(&b.file_type.extension());
                    if ascending {
                        cmp
                    } else {
                        cmp.reverse()
                    }
                });
            }
            SortOption::Size => {
                self.found_files.sort_by(|a, b| {
                    let cmp = a.estimated_size.cmp(&b.estimated_size);
                    if ascending {
                        cmp
                    } else {
                        cmp.reverse()
                    }
                });
            }
            SortOption::Recoverability => {
                self.found_files.sort_by(|a, b| {
                    let cmp = a.recoverability.partial_cmp(&b.recoverability).unwrap();
                    if ascending {
                        cmp
                    } else {
                        cmp.reverse()
                    }
                });
            }
            SortOption::Entropy => {
                self.found_files.sort_by(|a, b| {
                    let cmp = a.entropy.partial_cmp(&b.entropy).unwrap();
                    if ascending {
                        cmp
                    } else {
                        cmp.reverse()
                    }
                });
            }
        }

        self.add_console_message(
            format!("📋 Archivos ordenados por {:?}", sort_by),
            ConsoleLevel::Info,
        );
    }

    fn load_preview(&mut self, index: usize) -> bool {
        if index >= self.found_files.len() {
            return false;
        }

        let file = &self.found_files[index];

        if !RecoverPillApp::can_have_preview(file.file_type) {
            self.preview_file_index = None;
            self.preview_data = None;
            self.preview_texture = None;
            self.preview_error = Some(format!(
                "Tipo {} no soportado para preview",
                file.file_type.extension()
            ));
            return false;
        }

        self.preview_loading = true;
        self.preview_error = None;
        self.preview_texture = None;
        self.preview_file_index = Some(index);

        let file_offset = file.offset;
        let file_type = file.file_type;
        let drive_path = self.last_drive_path.clone();

        let (tx, rx) = std::sync::mpsc::channel();
        self.preview_receiver = Some(rx);

        std::thread::spawn(move || {
            let result = Self::load_preview_thread(file_offset, file_type, drive_path);
            let _ = tx.send(PreviewResult {
                index,
                data: result.0,
                width: result.1,
                height: result.2,
                error: result.3,
            });
        });

        true
    }

    fn load_preview_thread(
        file_offset: u64,
        file_type: crate::core::signatures::FileType,
        drive_path: Option<String>,
    ) -> (Option<Vec<u8>>, u32, u32, Option<String>) {
        if let Some(drive_path) = drive_path {
            if let Ok(mut reader) = crate::disk::access::DiskReader::open(&drive_path) {
                let disk_size = reader.total_size();

                if file_offset >= disk_size {
                    return (None, 0, 0, Some(format!("Offset fuera del disco")));
                }

                for search_offset in 0..2048 {
                    let read_offset = file_offset + (search_offset * 256) as u64;

                    if read_offset >= disk_size {
                        break;
                    }

                    let remaining = disk_size - read_offset;
                    let max_read = std::cmp::min(remaining as usize, 4 * 1024 * 1024);
                    let preview_size = std::cmp::max(4096, max_read);

                    if preview_size == 0 {
                        continue;
                    }

                    match reader.read_at(read_offset, preview_size) {
                        Ok(data) => {
                            if data.len() < 4 {
                                continue;
                            }

                            let magic_ok = match file_type {
                                crate::core::signatures::FileType::Jpeg => {
                                    data.starts_with(&[0xFF, 0xD8, 0xFF])
                                }
                                crate::core::signatures::FileType::Png => {
                                    data.starts_with(&[0x89, 0x50, 0x4E, 0x47])
                                }
                                crate::core::signatures::FileType::Gif => {
                                    data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a")
                                }
                                crate::core::signatures::FileType::Bmp => {
                                    data.starts_with(&[0x42, 0x4D])
                                }
                                crate::core::signatures::FileType::Webp => {
                                    data.len() >= 12
                                        && &data[0..4] == b"RIFF"
                                        && &data[8..12] == b"WEBP"
                                }
                                crate::core::signatures::FileType::Ico => {
                                    data.starts_with(&[0x00, 0x00]) && data.len() >= 4
                                }
                                _ => true,
                            };

                            if !magic_ok {
                                continue;
                            }

                            match image::load_from_memory(&data) {
                                Ok(img) => {
                                    let rgba = img.to_rgba8();
                                    let (width, height) = rgba.dimensions();

                                    if width > 0 && height > 0 {
                                        return (Some(rgba.into_raw()), width, height, None);
                                    }
                                }
                                Err(_e) => continue,
                            }
                        }
                        Err(_) => continue,
                    }
                }
                (None, 0, 0, Some("No se encontró imagen válida".to_string()))
            } else {
                (None, 0, 0, Some("No se pudo abrir disco".to_string()))
            }
        } else {
            (None, 0, 0, Some("No hay disco seleccionado".to_string()))
        }
    }

    fn process_preview_results(&mut self) {
        let result = if let Some(ref rx) = self.preview_receiver {
            match rx.try_recv() {
                Ok(result) => Some(result),
                Err(_) => None,
            }
        } else {
            None
        };

        if let Some(result) = result {
            self.preview_receiver = None;

            if result.index == self.preview_file_index.unwrap_or(usize::MAX) {
                if let Some(data) = result.data {
                    self.preview_data = Some(data);
                    self.preview_width = result.width;
                    self.preview_height = result.height;
                    self.preview_loading = false;
                    self.preview_error = None;

                    self.add_console_message(
                        format!("✅ Preview: {}x{}", result.width, result.height),
                        ConsoleLevel::Success,
                    );
                } else {
                    self.preview_loading = false;
                    self.preview_error = result.error;
                }
            }
        }
    }

    fn can_have_preview(file_type: crate::core::signatures::FileType) -> bool {
        matches!(
            file_type,
            crate::core::signatures::FileType::Jpeg
                | crate::core::signatures::FileType::Png
                | crate::core::signatures::FileType::Gif
                | crate::core::signatures::FileType::Bmp
                | crate::core::signatures::FileType::Webp
                | crate::core::signatures::FileType::Ico
                | crate::core::signatures::FileType::Heic
                | crate::core::signatures::FileType::Raw
        )
    }
}

impl App for RecoverPillApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.process_scan_results();
        self.process_preview_results();
        self.process_recovery_results();
        self.process_recovery_progress();
        self.process_android_results();

        if self.is_scanning || self.preview_loading || self.is_recovering || self.android_is_scanning || self.android_backup_in_progress {
            ctx.request_repaint();
        }

        let mut style = (*ctx.style()).clone();
        style.visuals.panel_fill = PANEL_BG;
        style.visuals.window_fill = PANEL_BG;
        ctx.set_style(style);

        // === ATAJOS DE TECLADO ===
        use egui::Modifiers;
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, egui::Key::Enter)) {
            if self.current_tab == MainTab::Scan && !self.is_scanning && self.selected_drive.is_some() {
                self.start_scan();
            }
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, egui::Key::R)) {
            if self.current_tab == MainTab::Scan && !self.is_recovering && !self.found_files.is_empty() {
                self.recover_selected_files();
            }
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, egui::Key::A)) {
            if self.current_tab == MainTab::Scan && !self.found_files.is_empty() {
                self.select_all();
            }
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, egui::Key::Escape)) {
            if self.is_scanning {
                self.stop_scan();
            } else if self.is_recovering {
                self.stop_recovery();
            }
        }

        // === TAB BAR ===
        egui::TopBottomPanel::top("tab_bar")
            .min_height(36.0)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    let scan_tab = egui::Button::new(
                        egui::RichText::new("🔍 Escaneo").size(14.0)
                    )
                    .fill(if self.current_tab == MainTab::Scan { ACCENT_COLOR } else { egui::Color32::from_rgb(40, 44, 55) })
                    .min_size(egui::vec2(100.0, 28.0));
                    if ui.add(scan_tab).clicked() { self.current_tab = MainTab::Scan; }

                    let android_tab = egui::Button::new(
                        egui::RichText::new("📱 Android").size(14.0)
                    )
                    .fill(if self.current_tab == MainTab::Android { ANDROID_COLOR } else { egui::Color32::from_rgb(40, 44, 55) })
                    .min_size(egui::vec2(100.0, 28.0));
                    if ui.add(android_tab).clicked() { self.current_tab = MainTab::Android; }

                    let settings_tab = egui::Button::new(
                        egui::RichText::new("⚙️ Config").size(14.0)
                    )
                    .fill(if self.current_tab == MainTab::Settings { egui::Color32::from_rgb(80, 80, 100) } else { egui::Color32::from_rgb(40, 44, 55) })
                    .min_size(egui::vec2(100.0, 28.0));
                    if ui.add(settings_tab).clicked() { self.current_tab = MainTab::Settings; }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!("recoverPill v1.0.0 | {}", BUILD_DATE))
                                .size(9.0)
                                .color(egui::Color32::from_gray(120)),
                        );
                    });
                });
                ui.add_space(4.0);
            });

        // === MAIN CONTENT ===
        if self.current_tab == MainTab::Scan {
            self.render_scan_content(ctx);
        } else if self.current_tab == MainTab::Android {
            self.render_android_content(ctx);
        } else if self.current_tab == MainTab::Settings {
            self.render_settings_content(ctx);
        }

        // === NOTIFICACIÓN FLOTANTE (Bug 4) ===
        if let Some(ref notif) = self.current_notification {
            if self.notification_timer > 0.0 {
                egui::Area::new(egui::Id::new("notification_area"))
                    .anchor(egui::Align2::RIGHT_TOP, [-20.0, 50.0])
                    .show(ctx, |ui| {
                        egui::Frame::none()
                            .fill(egui::Color32::from_rgba_premultiplied(0, 0, 0, 200))
                            .rounding(8.0)
                            .inner_margin(egui::Margin::same(12.0))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(notif)
                                        .size(13.0)
                                        .color(ACCENT_LIGHT),
                                );
                            });
                    });
            }
        }

        // === CONSOLA COMPARTIDA (Bug 3) ===
        egui::TopBottomPanel::bottom("console_panel")
            .resizable(true)
            .default_height(120.0)
            .min_height(60.0)
            .show(ctx, |ui| {
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("📟 Consola de Sistema").size(13.0).strong().color(ACCENT_COLOR));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(egui::RichText::new("🗑️ Limpiar").size(10.0)).clicked() {
                            self.console_messages.clear();
                        }
                        ui.label(egui::RichText::new(format!("{} mensajes", self.console_messages.len())).size(9.0).color(egui::Color32::from_gray(120)));
                    });
                });
                ui.add_space(3.0);
                ui.separator();

                egui::Frame::none()
                    .fill(egui::Color32::from_rgb(15, 15, 20))
                    .rounding(4.0)
                    .inner_margin(egui::Margin::same(5.0))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .stick_to_bottom(true)
                            .max_height(200.0)
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                for msg in self.console_messages.iter().rev().take(50) {
                                    let (color, icon) = match msg.level {
                                        ConsoleLevel::Info => (egui::Color32::from_gray(180), "ℹ️"),
                                        ConsoleLevel::Warning => (WARNING_COLOR, "⚠️"),
                                        ConsoleLevel::Error => (ERROR_COLOR, "❌"),
                                        ConsoleLevel::Success => (SUCCESS_COLOR, "✅"),
                                    };
                                    ui.horizontal(|ui| {
                                        ui.label(egui::RichText::new(icon).size(10.0));
                                        ui.label(
                                            egui::RichText::new(&msg.text).color(color).size(10.0).monospace(),
                                        );
                                    });
                                }
                                if self.console_messages.is_empty() {
                                    ui.vertical_centered(|ui| {
                                        ui.add_space(10.0);
                                        ui.label(egui::RichText::new("Esperando actividad...").color(egui::Color32::from_gray(80)).italics());
                                    });
                                }
                            });
                    });
                ui.add_space(5.0);
            });
    }
}

impl RecoverPillApp {
    fn render_scan_content(&mut self, ctx: &egui::Context) {
        // Panel izquierdo
        egui::SidePanel::left("control_panel")
            .min_width(200.0)
            .max_width(240.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.add_space(10.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("🔍 recoverPill")
                            .size(22.0)
                            .strong()
                            .color(ACCENT_COLOR),
                    );
                    ui.label(
                        egui::RichText::new("Recuperación de Datos")
                            .size(13.0)
                            .color(egui::Color32::from_gray(180)),
                    );
                });
                ui.add_space(15.0);
                ui.separator();
                ui.add_space(10.0);

                // Selector de unidad
                ui.label(egui::RichText::new("💾 Unidad").size(14.0).strong());
                ui.add_space(5.0);
                egui::Frame::none()
                    .fill(CARD_BG)
                    .rounding(6.0)
                    .inner_margin(egui::Margin::same(10.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let mut drive_text = "Seleccionar...".to_string();
                            if let Some(idx) = self.selected_drive {
                                if idx < self.drives.len() {
                                    let d = &self.drives[idx];
                                    drive_text = format!("{}", d.path);
                                }
                            }
                            egui::ComboBox::from_id_source("drive_selector")
                                .selected_text(
                                    egui::RichText::new(drive_text)
                                        .size(13.0)
                                        .color(ACCENT_COLOR),
                                )
                                .width(180.0)
                                .show_ui(ui, |ui| {
                                    for (i, d) in self.drives.iter().enumerate() {
                                        let text = format!(
                                            "{} ({})",
                                            d.path,
                                            DriveInfo::format_size(d.total_bytes)
                                        );
                                        ui.selectable_value(
                                            &mut self.selected_drive,
                                            Some(i),
                                            egui::RichText::new(text).size(12.0),
                                        );
                                    }
                                });
                        });
                        if let Some(idx) = self.selected_drive {
                            if idx < self.drives.len() {
                                let d = &self.drives[idx];
                                ui.add_space(5.0);
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Capacidad: {}",
                                        DriveInfo::format_size(d.total_bytes)
                                    ))
                                    .size(11.0)
                                    .color(egui::Color32::from_gray(150)),
                                );
                            }
                        }
                    });

                ui.add_space(12.0);

                // Selector de modo de escaneo
                ui.label(
                    egui::RichText::new("🔍 Modo de escaneo")
                        .size(14.0)
                        .strong(),
                );
                ui.add_space(5.0);
                egui::Frame::none()
                    .fill(CARD_BG)
                    .rounding(6.0)
                    .inner_margin(egui::Margin::same(10.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let sig_btn = egui::Button::new(
                                egui::RichText::new("🔍 Escaneo Profundo").size(12.0),
                            )
                            .fill(
                                if self.scan_mode == ScanMode::Signature {
                                    ACCENT_COLOR
                                } else {
                                    egui::Color32::from_rgb(60, 60, 80)
                                },
                            );
                            if ui.add(sig_btn).clicked() {
                                self.scan_mode = ScanMode::Signature;
                            }

                            let fs_btn = egui::Button::new(
                                egui::RichText::new("📁 Escaneo Superficial").size(12.0),
                            )
                            .fill(
                                if self.scan_mode == ScanMode::FileSystem {
                                    ACCENT_COLOR
                                } else {
                                    egui::Color32::from_rgb(60, 60, 80)
                                },
                            );
                            if ui.add(fs_btn).clicked() {
                                self.scan_mode = ScanMode::FileSystem;
                            }
                        });
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(match self.scan_mode {
                                ScanMode::Signature => {
                                    "Recupera archivos borrados de discos formateados"
                                },
                                ScanMode::FileSystem => "Lista archivos existentes en el sistema",
                            })
                            .size(10.0)
                            .color(egui::Color32::from_gray(150)),
                        );
                    });

                ui.add_space(12.0);

                // Botón de escaneo
                ui.label(egui::RichText::new("⚡ Acciones").size(14.0).strong());
                ui.add_space(5.0);
                egui::Frame::none()
                    .fill(CARD_BG)
                    .rounding(6.0)
                    .inner_margin(egui::Margin::same(10.0))
                    .show(ui, |ui| {
                        if !self.is_scanning {
                            let drive_ready = self.selected_drive.is_some();
                            if ui
                                .add_enabled(drive_ready,
                                    egui::Button::new(
                                        egui::RichText::new("🚀 Iniciar Escaneo").size(14.0),
                                    )
                                    .fill(if drive_ready { ACCENT_COLOR } else { egui::Color32::from_gray(60) })
                                    .min_size(egui::vec2(200.0, 35.0)),
                                )
                                .on_hover_text(if drive_ready { "" } else { "Selecciona una unidad primero" })
                                .clicked()
                            {
                                self.start_scan();
                            }
                        } else {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Escaneando... {:.1}%",
                                        self.scan_percentage
                                    ))
                                    .size(13.0)
                                    .color(WARNING_COLOR),
                                );
                            });
                            ui.add_space(8.0);
                            let stop_label = if self.stop_confirm {
                                "⏹ ¿Confirmar?"
                            } else {
                                "⏹ Detener"
                            };
                            if ui
                                .add(
                                    egui::Button::new(egui::RichText::new(stop_label).size(13.0))
                                        .fill(if self.stop_confirm { egui::Color32::from_rgb(200, 50, 30) } else { ERROR_COLOR })
                                        .min_size(egui::vec2(200.0, 30.0)),
                                )
                                .clicked()
                            {
                                if self.stop_confirm {
                                    self.stop_confirm = false;
                                    self.stop_scan();
                                } else {
                                    self.stop_confirm = true;
                                }
                            }
                            if self.stop_confirm {
                                ui.label(
                                    egui::RichText::new("Presiona de nuevo para confirmar")
                                        .size(9.0)
                                        .color(WARNING_COLOR),
                                );
                            }
                            let progress_percent = self.scan_percentage as f32 / 100.0;
                            ui.add_space(8.0);
                            ui.add(
                                egui::ProgressBar::new(progress_percent)
                                    .desired_width(200.0)
                                    .fill(ACCENT_COLOR),
                            );
                        }
                    });

                ui.add_space(12.0);

                // Carpeta de recuperación
                ui.label(egui::RichText::new("📁 Carpeta").size(14.0).strong());
                ui.add_space(5.0);
                egui::Frame::none()
                    .fill(CARD_BG)
                    .rounding(6.0)
                    .inner_margin(egui::Margin::same(10.0))
                    .show(ui, |ui| {
                        if ui.button("📂 Seleccionar...").clicked() {
                            if let Some(path) =
                                rfd::FileDialog::new().set_directory(".").pick_folder()
                            {
                                self.set_output_folder(path.to_string_lossy().to_string());
                            }
                        }
                        if !self.output_folder.is_empty() {
                            ui.add_space(5.0);
                            ui.label(
                                egui::RichText::new(&self.output_folder)
                                    .size(10.0)
                                    .italics()
                                    .color(egui::Color32::from_gray(150)),
                            );
                        }
                    });

                ui.add_space(12.0);

                // Filtros
                ui.label(egui::RichText::new("🔍 Filtros").size(14.0).strong());
                ui.add_space(5.0);
                egui::Frame::none()
                    .fill(CARD_BG)
                    .rounding(6.0)
                    .inner_margin(egui::Margin::same(10.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let types = [
                                ("ALL", "", "Todos"),
                                ("IMG", "jpg,jpeg,png,gif,bmp,webp,heic,raw", "Imágenes"),
                                ("VID", "mp4,avi,mkv,mov,wmv", "Videos"),
                                ("DOC", "pdf,doc,docx,xls,xlsx", "Docs"),
                                ("ZIP", "zip,rar,7z,tar,gz", "Comprimidos"),
                            ];
                            for (label, pattern, tooltip) in types {
                                let is_active = self.type_filter.as_deref() == Some(pattern);
                                let btn = egui::Button::new(
                                    egui::RichText::new(label).size(11.0).color(if is_active {
                                        egui::Color32::WHITE
                                    } else {
                                        egui::Color32::from_gray(180)
                                    }),
                                )
                                .fill(if is_active {
                                    ACCENT_COLOR
                                } else {
                                    egui::Color32::from_gray(50)
                                })
                                .min_size(egui::vec2(35.0, 22.0));
                                if ui.add(btn).on_hover_text(tooltip).clicked() {
                                    if pattern.is_empty() {
                                        self.type_filter = None;
                                        self.selected_individual_types.clear();
                                    } else {
                                        self.type_filter = Some(pattern.to_string());
                                        self.selected_individual_types.clear();
                                        let exts: Vec<&str> = pattern.split(',').collect();
                                        for ext in exts {
                                            self.selected_individual_types.insert(ext.trim().to_string());
                                        }
                                    }
                                }
                            }
                        });
                        
                        // Selección individual de tipos de archivo
                        {
                            ui.add_space(8.0);
                            ui.separator();
                            ui.add_space(5.0);
                            
                            // Categorías de tipos de archivo
                            let image_types = vec![
                                ("jpg", "JPG"), ("jpeg", "JPEG"), ("png", "PNG"), ("gif", "GIF"),
                                ("bmp", "BMP"), ("webp", "WebP"), ("heic", "HEIC"), ("raw", "RAW"),
                                ("tiff", "TIFF"), ("ico", "ICO"), ("psd", "PSD"), ("ai", "AI"),
                                ("svg", "SVG"),
                            ];
                            let video_types = vec![
                                ("mp4", "MP4"), ("avi", "AVI"), ("mkv", "MKV"), ("mov", "MOV"), 
                                ("wmv", "WMV"), ("webm", "WebM"), ("flv", "FLV"),
                            ];
                            let doc_types = vec![
                                ("pdf", "PDF"), ("doc", "DOC"), ("docx", "DOCX"), 
                                ("xls", "XLS"), ("xlsx", "XLSX"), ("ppt", "PPT"), 
                                ("pptx", "PPTX"), ("odt", "ODT"),
                            ];
                            let archive_types = vec![
                                ("zip", "ZIP"), ("rar", "RAR"), ("7z", "7Z"), ("tar", "TAR"), ("gz", "GZ"),
                            ];
                            let audio_types = vec![
                                ("mp3", "MP3"), ("wav", "WAV"), ("flac", "FLAC"), ("aac", "AAC"),
                                ("ogg", "OGG"), ("wma", "WMA"),
                            ];
                            let executable_types = vec![
                                ("exe", "EXE"), ("dll", "DLL"), ("msi", "MSI"),
                            ];
                            
                            // Función para mostrar checkboxes de una categoría
                            let mut show_category = |ui: &mut egui::Ui, label: &str, emoji: &str, types: &[( &str, &str)]| {
                                ui.label(egui::RichText::new(format!("{} {}", emoji, label)).size(11.0).strong().color(ACCENT_COLOR));
                                ui.add_space(3.0);
                                ui.horizontal_wrapped(|ui| {
                                    for (ext, label) in types {
                                        let selected = self.selected_individual_types.contains(*ext);
                                        let checkbox_text = if selected { "✅" } else { "☐" };
                                        if ui.button(format!("{} {}", checkbox_text, label)).clicked() {
                                            if selected {
                                                self.selected_individual_types.remove(*ext);
                                            } else {
                                                self.selected_individual_types.insert(ext.to_string());
                                            }
                                        }
                                    }
                                });
                                ui.add_space(5.0);
                            };
                            
                            show_category(ui, "Imágenes", "🖼️", &image_types);
                            show_category(ui, "Videos", "🎬", &video_types);
                            show_category(ui, "Documentos", "📄", &doc_types);
                            show_category(ui, "Archivos", "📦", &archive_types);
                            show_category(ui, "Audio", "🎵", &audio_types);
                            show_category(ui, "Ejecutables", "⚙️", &executable_types);
                            
                            // Botones para seleccionar/deseleccionar todo
                            ui.horizontal(|ui| {
                                if ui.button(egui::RichText::new("✅ Todos").size(10.0)).clicked() {
                                    for (ext, _) in image_types.iter().chain(video_types.iter()).chain(doc_types.iter()).chain(archive_types.iter()).chain(audio_types.iter()).chain(executable_types.iter()) {
                                        self.selected_individual_types.insert(ext.to_string());
                                    }
                                }
                                if ui.button(egui::RichText::new("☐ Ninguno").size(10.0)).clicked() {
                                    self.selected_individual_types.clear();
                                }
                                if ui.button(egui::RichText::new("🔍 Aplicar").size(10.0)).clicked() && !self.selected_individual_types.is_empty() {
                                    let pattern: Vec<String> = self.selected_individual_types.iter().cloned().collect();
                                    self.type_filter = Some(pattern.join(","));
                                }
                            });
                        }
                        
                        ui.add_space(8.0);
                        
                        // Filtro de Calidad
                        ui.horizontal(|ui| {
                            let text = if self.quality_filter_enabled { "✅" } else { "☐" };
                            if ui.button(text).on_hover_text("Activar filtro de calidad").clicked() {
                                self.quality_filter_enabled = !self.quality_filter_enabled;
                            }
                            ui.label("Calidad min:");
                            ui.add(
                                egui::Slider::new(&mut self.min_recoverability, 0.0..=100.0)
                                    .show_value(true)
                                    .text("%")
                                    .integer(),
                            );
                        });
                        
                        ui.add_space(5.0);
                        ui.horizontal(|ui| {
                            let dup_text = if self.hide_duplicates { "🔄✅" } else { "🔄☐" };
                            if ui.button(dup_text).on_hover_text("Ocultar duplicados").clicked() {
                                self.hide_duplicates = !self.hide_duplicates;
                            }
                            ui.label(if self.hide_duplicates {
                                "Duplicados ocultos"
                            } else {
                                "Mostrar duplicados"
                            });
                        });
                        
                    });

                ui.add_space(12.0);

                // Tips Rápidos
                ui.label(egui::RichText::new("💡 Tips").size(14.0).strong());
                ui.add_space(5.0);
                egui::Frame::none()
                    .fill(CARD_BG)
                    .rounding(6.0)
                    .inner_margin(egui::Margin::same(10.0))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("• Escaneo Profundo: ").size(11.0).color(ACCENT_COLOR));
                        ui.label(egui::RichText::new("Recupera archivos de discos formateados.").size(10.0));
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("• IA: ").size(11.0).color(ACCENT_COLOR));
                        ui.label(egui::RichText::new("Filtra automáticamente el ruido y sectores vacíos.").size(10.0));
                    });
            });

        // Panel derecho
        egui::SidePanel::right("preview_panel")
            .min_width(200.0)
            .max_width(280.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.add_space(10.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("📋 Detalles")
                            .size(17.0)
                            .strong()
                            .color(ACCENT_COLOR),
                    );
                });
                ui.add_space(10.0);
                ui.separator();

                if let Some(idx) = self.selected_file {
                    if idx < self.found_files.len() {
                        let file = &self.found_files[idx];

                        if RecoverPillApp::can_have_preview(file.file_type) {
                            egui::Frame::none()
                                .fill(CARD_BG)
                                .rounding(6.0)
                                .inner_margin(egui::Margin::same(8.0))
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new("🖼️ Vista Previa").size(13.0).strong(),
                                    );
                                    ui.add_space(6.0);
                                    if self.preview_loading {
                                        ui.horizontal(|ui| {
                                            ui.spinner();
                                            ui.label("Cargando...");
                                        });
                                    } else if let Some(ref error) = self.preview_error {
                                        ui.vertical_centered(|ui| {
                                            let file = &self.found_files[idx];
                                            let thumb = get_thumb(file.file_type);
                                            ui.label(egui::RichText::new(thumb).size(36.0));
                                            ui.label(
                                                egui::RichText::new(format!("{}", file.file_type.extension().to_uppercase()))
                                                    .size(14.0)
                                                    .strong()
                                                    .color(ACCENT_COLOR),
                                            );
                                            ui.add_space(4.0);
                                            ui.label(
                                                egui::RichText::new(format!("{} bytes", format_size(file.estimated_size)))
                                                    .size(10.0)
                                                    .color(egui::Color32::from_gray(160)),
                                            );
                                            ui.label(
                                                egui::RichText::new(format!("Offset: 0x{:X}", file.offset))
                                                    .size(9.0)
                                                    .color(egui::Color32::from_gray(130))
                                                    .monospace(),
                                            );
                                            ui.add_space(4.0);
                                            ui.label(
                                                egui::RichText::new(error)
                                                    .size(9.0)
                                                    .color(egui::Color32::from_gray(120)),
                                            );
                                        });
                                    } else if let Some(ref data) = self.preview_data {
                                        if !data.is_empty() {
                                            if self.preview_texture.is_none() || self.preview_file_index != Some(idx) {
                                                // recreate texture for this index
                                                self.preview_texture = None;
                                                self.preview_file_index = Some(idx);
                                                if let Some(ref data) = self.preview_data {
                                                    if !data.is_empty() {
                                                        let texture = ui.ctx().load_texture(
                                                            format!("preview_{}", idx),
                                                            egui::ColorImage::from_rgba_unmultiplied(
                                                                [
                                                                    self.preview_width as usize,
                                                                    self.preview_height as usize,
                                                                ],
                                                                data,
                                                            ),
                                                            Default::default(),
                                                        );
                                                        self.preview_texture = Some(texture);
                                                    }
                                                }
                                            }
                                            if let Some(ref texture) = self.preview_texture {
                                                let max_w = 240.0;
                                                let max_h = 180.0;
                                                let aspect = self.preview_width as f32
                                                    / self.preview_height as f32;
                                                let (w, h) = if aspect > 1.0 {
                                                    (max_w, max_w / aspect)
                                                } else {
                                                    (max_h * aspect, max_h)
                                                };
                                                ui.vertical_centered(|ui| {
                                                    ui.add(
                                                        egui::Image::new(texture)
                                                            .max_width(w)
                                                            .max_height(h)
                                                            .rounding(4.0),
                                                    );
                                                });
                                            }
                                        }
                                    }
                                });
                            ui.add_space(8.0);
                        }

                        egui::Frame::none()
                            .fill(CARD_BG)
                            .rounding(6.0)
                            .inner_margin(egui::Margin::same(10.0))
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new(&file.file_name).size(14.0).strong());
                                ui.add_space(10.0);
                                ui.separator();
                                ui.add_space(6.0);
                                egui::Grid::new("details_grid")
                                    .num_columns(2)
                                    .spacing([8.0, 6.0])
                                    .show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new("Tipo:")
                                                .color(egui::Color32::from_gray(150)),
                                        );
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{}",
                                                file.file_type.extension().to_uppercase()
                                            ))
                                            .color(ACCENT_COLOR),
                                        );
                                        ui.end_row();
                                        ui.label(
                                            egui::RichText::new("Tamaño:")
                                                .color(egui::Color32::from_gray(150)),
                                        );
                                        ui.label(format_size(file.estimated_size));
                                        ui.end_row();
                                        ui.label(
                                            egui::RichText::new("Offset:")
                                                .color(egui::Color32::from_gray(150)),
                                        );
                                        ui.label(
                                            egui::RichText::new(format!("0x{:X}", file.offset))
                                                .monospace(),
                                        );
                                        ui.end_row();
                                    });
                            });
                        ui.add_space(8.0);

                        egui::Frame::none()
                            .fill(CARD_BG)
                            .rounding(6.0)
                            .inner_margin(egui::Margin::same(10.0))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new("🎯 Recuperación").size(13.0).strong(),
                                );
                                ui.add_space(8.0);
                                let recover_color = if file.recoverability >= 70.0 {
                                    SUCCESS_COLOR
                                } else if file.recoverability >= 40.0 {
                                    WARNING_COLOR
                                } else {
                                    ERROR_COLOR
                                };
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::ProgressBar::new(file.recoverability as f32 / 100.0)
                                            .desired_width(120.0)
                                            .fill(recover_color),
                                    );
                                    ui.add_space(5.0);
                                    ui.label(
                                        egui::RichText::new(format!("{:.0}%", file.recoverability))
                                            .size(14.0)
                                            .strong()
                                            .color(recover_color),
                                    );
                                });
                                ui.add_space(6.0);
                                let ent_desc = entropy_description(file.entropy);
                                let ent_emoji = entropy_emoji(file.entropy);
                                let ent_col = egui::Color32::from_rgb(
                                    (entropy_color(file.entropy)[0] * 255.0) as u8,
                                    (entropy_color(file.entropy)[1] * 255.0) as u8,
                                    (entropy_color(file.entropy)[2] * 255.0) as u8,
                                );
                                ui.horizontal(|ui| {
                                    ui.label(format!("Complejidad {}:", ent_emoji));
                                    ui.add_space(5.0);
                                    ui.label(
                                        egui::RichText::new(format!("{} ({:.1})", ent_desc, file.entropy))
                                            .color(ent_col)
                                            .strong(),
                                    );
                                });
                                ui.add_space(8.0);
                                ui.separator();
                                let rec_text = if file.recoverability >= 80.0 {
                                    "✅ Excelente"
                                } else if file.recoverability >= 60.0 {
                                    "👍 Bueno"
                                } else if file.recoverability >= 40.0 {
                                    "⚠️ Limitado"
                                } else {
                                    "❌ Bajo"
                                };
                                ui.label(
                                    egui::RichText::new(rec_text)
                                        .size(12.0)
                                        .color(recover_color),
                                );
                            });
                        ui.add_space(8.0);

                        egui::Frame::none()
                            .fill(CARD_BG)
                            .rounding(6.0)
                            .inner_margin(egui::Margin::same(10.0))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let _ =
                                        ui.toggle_value(&mut self.found_files[idx].selected, "✓");
                                    if ui.button("🔄").clicked() {
                                        self.preview_texture = None;
                                        self.load_preview(idx);
                                    }
                                });
                            });
                    }
                } else {
                    egui::Frame::none()
                        .fill(CARD_BG)
                        .rounding(6.0)
                        .inner_margin(egui::Margin::same(12.0))
                        .show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.label(egui::RichText::new("💡").size(28.0));
                                ui.add_space(5.0);
                                ui.label(
                                    egui::RichText::new("Selecciona un archivo")
                                        .size(13.0)
                                        .strong(),
                                );
                            });
                            ui.add_space(12.0);
                            ui.separator();
                            ui.add_space(8.0);
                            ui.label(egui::RichText::new("Consejos:").strong().size(12.0));
                            ui.add_space(6.0);
                            ui.label("• Clic en nombre para detalles");
                            ui.add_space(3.0);
                            ui.label("• Imágenes muestran preview");
                            ui.add_space(3.0);
                            ui.label("• >70% = buena calidad");
                            ui.add_space(3.0);
                            ui.label("• 📊 Simple/Complejo = datos intactos");
                        });
                }
            });

        // Panel central
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::Frame::none()
                .fill(CARD_BG)
                .rounding(6.0)
                .inner_margin(egui::Margin::same(10.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("📂 Archivos")
                                .size(15.0)
                                .strong()
                                .color(ACCENT_COLOR),
                        );
                        ui.add_space(15.0);
                        let total = self.found_files.len();
                        let selected = self.get_selected_files().len();
                        ui.label(
                            egui::RichText::new(format!("Total: {}", total))
                                .size(12.0)
                                .color(egui::Color32::from_gray(180)),
                        );
                        if selected > 0 {
                            ui.label(
                                egui::RichText::new(format!(" | Selecc: {}", selected))
                                    .size(12.0)
                                    .color(SUCCESS_COLOR),
                            );
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label("Ordenar:");
                            ui.add_space(5.0);
                            let sort_label = match self.sort_by {
                                SortOption::Name => "Nombre",
                                SortOption::Type => "Tipo",
                                SortOption::Size => "Tamaño",
                                SortOption::Recoverability => "Recup.",
                                SortOption::Entropy => "📊 Complejidad",
                            };
                            egui::ComboBox::from_id_source("sort")
                                .selected_text(sort_label)
                                .width(100.0)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut self.sort_by,
                                        SortOption::Recoverability,
                                        "Recup.",
                                    );
                                    ui.selectable_value(
                                        &mut self.sort_by,
                                        SortOption::Name,
                                        "Nombre",
                                    );
                                    ui.selectable_value(
                                        &mut self.sort_by,
                                        SortOption::Type,
                                        "Tipo",
                                    );
                                    ui.selectable_value(
                                        &mut self.sort_by,
                                        SortOption::Size,
                                        "Tamaño",
                                    );
                                    ui.selectable_value(
                                        &mut self.sort_by,
                                        SortOption::Entropy,
                                        "📊 Complejidad",
                                    );
                                });
                            ui.add_space(3.0);
                            if ui
                                .button(if self.sort_ascending { "⬆" } else { "⬇" })
                                .clicked()
                            {
                                self.sort_ascending = !self.sort_ascending;
                                self.sort_files(self.sort_by, self.sort_ascending);
                            }
                        });
                    });
                });
            ui.add_space(6.0);

            egui::Frame::none()
                .fill(CARD_BG)
                .rounding(6.0)
                .inner_margin(egui::Margin::same(8.0))
                .show(ui, |ui| {
                    if self.is_recovering {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(
                                egui::RichText::new(format!(
                                    "Recuperando... {:.1}%",
                                    self.recovery_progress
                                ))
                                .size(13.0)
                                .color(WARNING_COLOR),
                            );
                            if !self.recovery_eta.is_empty() {
                                ui.label(
                                    egui::RichText::new(&self.recovery_eta)
                                        .size(11.0)
                                        .color(egui::Color32::from_gray(160)),
                                );
                            }
                            ui.add_space(8.0);
                            if ui.button("⏹ Detener").clicked() {
                                self.stop_recovery();
                            }
                        });
                        ui.add_space(8.0);
                        let progress_percent = self.recovery_progress as f32 / 100.0;
                        ui.add(
                            egui::ProgressBar::new(progress_percent)
                                .desired_width(200.0)
                                .fill(SUCCESS_COLOR),
                        );
                    } else {
                        ui.horizontal(|ui| {
                            if ui.button("📥 Recuperar").clicked() {
                                self.recover_selected_files();
                            }
                            ui.add_space(8.0);
                            if ui.button(format!("⭐ >= {:.0}%", self.min_recoverability)).clicked() {
                                let mut count = 0;
                                for f in &mut self.found_files {
                                    f.selected = false;
                                    if f.recoverability >= self.min_recoverability {
                                        f.selected = true;
                                        count += 1;
                                    }
                                }
                                self.add_console_message(
                                    format!("✅ {} archivos con calidad >= {:.0}%", count, self.min_recoverability),
                                    ConsoleLevel::Success,
                                );
                            }
                            ui.add_space(8.0);
                            if ui.button("☑ Todo").clicked() {
                                self.select_all();
                            }
                            if ui.button("☐ Nada").clicked() {
                                self.deselect_all();
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.button("🗑️").clicked() {
                                        self.clear_all_files();
                                    }
                                },
                            );
                        });
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label("Categorías:");
                            ui.add_space(4.0);
                            if ui.button("🖼️ Img").clicked() {
                                self.select_by_category("Imágenes");
                            }
                            if ui.button("🎬 Vid").clicked() {
                                self.select_by_category("Video");
                            }
                            if ui.button("🎵 Aud").clicked() {
                                self.select_by_category("Audio");
                            }
                            if ui.button("📄 Doc").clicked() {
                                self.select_by_category("Documentos");
                            }
                            if ui.button("📦 Arc").clicked() {
                                self.select_by_category("Archivos");
                            }
                            if ui.button("⚙️ Exe").clicked() {
                                self.select_by_category("Ejecutables");
                            }
                        });
                    }
                });
            ui.add_space(6.0);

            // Barra de búsqueda
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(45, 48, 60))
                .rounding(4.0)
                .inner_margin(egui::Margin::same(4.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("🔍").size(12.0));
                        ui.add_space(4.0);
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut self.filter_text)
                                .desired_width(200.0)
                                .hint_text("Filtrar por nombre o tipo..."),
                        );
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            self.filter_text.clear();
                        }
                        if !self.filter_text.is_empty() {
                            if ui.add(
                                egui::Button::new(egui::RichText::new("✕").size(12.0).color(egui::Color32::from_gray(180)))
                                    .min_size(egui::vec2(18.0, 18.0))
                            ).clicked()
                            {
                                self.filter_text.clear();
                            }
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let visible_count = self.found_files.iter().filter(|f| {
                                let text_match = self.filter_text.is_empty()
                                    || f.file_name.to_lowercase().contains(&self.filter_text.to_lowercase())
                                    || f.file_type.extension().to_lowercase().contains(&self.filter_text.to_lowercase());
                                let type_match = if let Some(ref pattern) = self.type_filter {
                                    if pattern.is_empty() { true } else {
                                        let ext = f.file_type.extension().to_lowercase();
                                        pattern.split(',').any(|p| p.trim().to_lowercase() == ext)
                                    }
                                } else { true };
                                let quality_match = if self.quality_filter_enabled { f.recoverability >= self.min_recoverability } else { true };
                                let not_duplicate = if self.hide_duplicates { !f.is_duplicate } else { true };
                                text_match && type_match && quality_match && not_duplicate
                            }).count();
                            ui.label(
                                egui::RichText::new(format!("{} visibles / {}", visible_count, self.found_files.len()))
                                    .size(10.0)
                                    .color(egui::Color32::from_gray(140)),
                            );
                        });
                    });
                });
            ui.add_space(6.0);

            // Calcular anchos de columna una sola vez (antes del scroll)
            let base_width = ui.available_width();
            let col_widths = [
                base_width * 0.03, // ☑ checkbox
                base_width * 0.03, // 📁 icon
                base_width * 0.25, // Nombre
                base_width * 0.07, // Tipo
                base_width * 0.09, // Tamaño
                base_width * 0.12, // Offset
                base_width * 0.10, // Recup.
                base_width * 0.09, // Entropía
                base_width * 0.09, // Estado
            ];

            egui::Frame::none()
                .fill(egui::Color32::from_rgb(50, 55, 70))
                .rounding(6.0)
                .inner_margin(egui::Margin::same(6.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [col_widths[0], 18.0],
                            egui::Label::new(egui::RichText::new("☑").size(13.0)),
                        );
                        ui.add_sized(
                            [col_widths[1], 18.0],
                            egui::Label::new(egui::RichText::new("📁").size(13.0)),
                        );
                        ui.add_sized(
                            [col_widths[2], 18.0],
                            egui::Label::new(egui::RichText::new("Nombre").size(13.0).strong()),
                        );
                        ui.add_sized(
                            [col_widths[3], 18.0],
                            egui::Label::new(egui::RichText::new("Tipo").size(13.0).strong()),
                        );
                        ui.add_sized(
                            [col_widths[4], 18.0],
                            egui::Label::new(egui::RichText::new("Tamaño").size(13.0).strong()),
                        );
                        ui.add_sized(
                            [col_widths[5], 18.0],
                            egui::Label::new(egui::RichText::new("Offset").size(13.0).strong()),
                        );
                        ui.add_sized(
                            [col_widths[6], 18.0],
                            egui::Label::new(egui::RichText::new("Recup.").size(13.0).strong()),
                        );
                        ui.add_sized(
                            [col_widths[7], 18.0],
                            egui::Label::new(egui::RichText::new("📊 Complejidad").size(13.0).strong()),
                        );
                        ui.add_sized(
                            [col_widths[8], 18.0],
                            egui::Label::new(egui::RichText::new("Estado").size(13.0).strong()),
                        );
                    });
                });

            egui::ScrollArea::vertical()
                .stick_to_bottom(false)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let mut filtered_files: Vec<(usize, &FoundFile)> = self
                        .found_files
                        .iter()
                        .enumerate()
                        .filter(|(_, f)| {
                            let text_match = self.filter_text.is_empty()
                                || f.file_name
                                    .to_lowercase()
                                    .contains(&self.filter_text.to_lowercase())
                                || f.file_type
                                    .extension()
                                    .to_lowercase()
                                    .contains(&self.filter_text.to_lowercase());
                            let type_match = if let Some(ref pattern) = self.type_filter {
                                if pattern.is_empty() {
                                    true
                                } else {
                                    let ext = f.file_type.extension().to_lowercase();
                                    pattern.split(',').any(|p| p.trim().to_lowercase() == ext)
                                }
                            } else {
                                true
                            };
                            
                            let quality_match = if self.quality_filter_enabled {
                                f.recoverability >= self.min_recoverability
                            } else {
                                true
                            };
                            
                            let not_duplicate = if self.hide_duplicates {
                                !f.is_duplicate
                            } else {
                                true
                            };

                            text_match && type_match && quality_match && not_duplicate
                        })
                        .collect();

                    let sort_by = self.sort_by;
                    let ascending = self.sort_ascending;
                    filtered_files.sort_by(|(_, a), (_, b)| {
                        let cmp = match sort_by {
                            SortOption::Name => {
                                a.file_name.to_lowercase().cmp(&b.file_name.to_lowercase())
                            }
                            SortOption::Type => {
                                a.file_type.extension().cmp(&b.file_type.extension())
                            }
                            SortOption::Size => a.estimated_size.cmp(&b.estimated_size),
                            SortOption::Recoverability => {
                                a.recoverability.partial_cmp(&b.recoverability).unwrap()
                            }
                            SortOption::Entropy => a.entropy.partial_cmp(&b.entropy).unwrap(),
                        };
                        if ascending {
                            cmp
                        } else {
                            cmp.reverse()
                        }
                    });

                    let total_files = filtered_files.len();
                    let total_pages = (total_files + self.items_per_page - 1) / self.items_per_page;
                    if self.current_page >= total_pages && total_pages > 0 {
                        self.current_page = total_pages - 1;
                    }
                    let start_idx = self.current_page * self.items_per_page;
                    let end_idx = std::cmp::min(start_idx + self.items_per_page, total_files);

                    if total_pages > 1 {
                        ui.horizontal(|ui| {
                            if ui.button("⏮").clicked() {
                                self.current_page = 0;
                            }
                            if ui.button("◀").clicked() && self.current_page > 0 {
                                self.current_page -= 1;
                            }
                            let mut page_num = self.current_page as i32 + 1;
                            let dval = ui.add_sized(
                                [90.0, 20.0],
                                egui::DragValue::new(&mut page_num)
                                    .speed(0.5)
                                    .prefix("Pág: ")
                                    .suffix(&format!("/{}", total_pages)),
                            );
                            if dval.changed() {
                                self.current_page = (page_num - 1).max(0).min(total_pages as i32 - 1) as usize;
                            }
                            if ui.button("▶").clicked() && self.current_page < total_pages - 1 {
                                self.current_page += 1;
                            }
                            if ui.button("⏭").clicked() {
                                self.current_page = total_pages - 1;
                            }
                            ui.label(
                                egui::RichText::new(format!("({} archivos)", total_files))
                                    .size(10.0)
                                    .color(egui::Color32::from_gray(140)),
                            );
                        });
                    }

                    let mut toggle_idx: Option<usize> = None;
                    let mut click_idx: Option<usize> = None;

                    for local_idx in start_idx..end_idx {
                        let (original_idx, file) = &filtered_files[local_idx];
                        let i = *original_idx;
                        let is_selected = self.selected_file == Some(i);
                        let bg = if is_selected {
                            egui::Color32::from_rgb(60, 80, 120)
                        } else if local_idx % 2 == 0 {
                            egui::Color32::from_rgb(35, 38, 48)
                        } else {
                            egui::Color32::from_rgb(30, 32, 40)
                        };

                        egui::Frame::none()
                            .fill(bg)
                            .rounding(3.0)
                            .inner_margin(egui::Margin::same(3.0))
                            .show(ui, |ui| {
                                let can_prev = RecoverPillApp::can_have_preview(file.file_type);
                                let name_color = if can_prev {
                                    ACCENT_COLOR
                                } else {
                                    egui::Color32::WHITE
                                };

                                let rec_col = if file.recoverability >= 70.0 {
                                    SUCCESS_COLOR
                                } else if file.recoverability >= 40.0 {
                                    WARNING_COLOR
                                } else {
                                    ERROR_COLOR
                                };

                                let ent_desc = entropy_description(file.entropy);
                                let ent_emoji = entropy_emoji(file.entropy);
                                let ent_col = egui::Color32::from_rgb(
                                    (entropy_color(file.entropy)[0] * 255.0) as u8,
                                    (entropy_color(file.entropy)[1] * 255.0) as u8,
                                    (entropy_color(file.entropy)[2] * 255.0) as u8,
                                );

                                ui.horizontal(|ui| {
                                    let mut check_state = file.selected;
                                    if ui
                                        .add_sized(
                                            [col_widths[0], 18.0],
                                            egui::Checkbox::without_text(&mut check_state),
                                        )
                                        .clicked()
                                    {
                                        toggle_idx = Some(i);
                                    }
                                    ui.add_sized(
                                        [col_widths[1], 18.0],
                                        egui::Label::new(
                                            egui::RichText::new(get_thumb(file.file_type))
                                                .size(12.0),
                                        ),
                                    );

                                    if ui
                                        .add_sized(
                                            [col_widths[2], 18.0],
                                            egui::Button::new(
                                                egui::RichText::new(&file.file_name)
                                                    .color(name_color)
                                                    .size(10.0),
                                            )
                                            .frame(false),
                                        )
                                        .clicked()
                                    {
                                        click_idx = Some(i);
                                    }
                                    ui.add_sized(
                                        [col_widths[3], 18.0],
                                        egui::Label::new(
                                            egui::RichText::new(
                                                file.file_type.extension().to_uppercase(),
                                            )
                                            .size(9.0)
                                            .color(ACCENT_COLOR),
                                        ),
                                    );
                                    ui.add_sized(
                                        [col_widths[4], 18.0],
                                        egui::Label::new(
                                            egui::RichText::new(format_size(file.estimated_size))
                                                .size(9.0),
                                        ),
                                    );
                                    ui.add_sized(
                                        [col_widths[5], 18.0],
                                        egui::Label::new(
                                            egui::RichText::new(format!("0x{:X}", file.offset))
                                                .size(9.0)
                                                .monospace(),
                                        ),
                                    );

                                    ui.horizontal(|ui| {
                                        ui.add_sized(
                                            [col_widths[6] * 0.6, 18.0],
                                            egui::ProgressBar::new(
                                                file.recoverability as f32 / 100.0,
                                            )
                                            .fill(rec_col),
                                        );
                                        ui.add_sized(
                                            [col_widths[6] * 0.4, 18.0],
                                            egui::Label::new(
                                                egui::RichText::new(format!(
                                                    "{:.0}",
                                                    file.recoverability
                                                ))
                                                .color(rec_col)
                                                .size(8.0),
                                            ),
                                        );
                                    });
                                    ui.add_sized(
                                        [col_widths[7], 18.0],
                                        egui::Label::new(
                                            egui::RichText::new(format!("{} {}", ent_emoji, ent_desc))
                                                .color(ent_col)
                                                .size(8.0),
                                        ),
                                    );
                                    
                                    // Columna de estado: validación y duplicados
                                    let status_text = if file.is_duplicate {
                                        "🔄 Dup"
                                    } else if file.is_validated {
                                        "✓ OK"
                                    } else {
                                        "⏳ Pend."
                                    };
                                    let status_col = if file.is_duplicate {
                                        WARNING_COLOR
                                    } else if file.is_validated {
                                        SUCCESS_COLOR
                                    } else {
                                        egui::Color32::from_gray(150)
                                    };
                                    ui.add_sized(
                                        [col_widths[8], 18.0],
                                        egui::Label::new(
                                            egui::RichText::new(status_text)
                                                .color(status_col)
                                                .size(8.0)
                                                .strong(),
                                        ),
                                    );
                                });
                            });
                        ui.add_space(1.0);
                    }

                    if let Some(idx) = toggle_idx {
                        self.toggle_file_selection(idx);
                    }
                    if let Some(idx) = click_idx {
                        self.selected_file = Some(idx);
                        self.load_preview(idx);
                    }
                });

            ui.add_space(5.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("recoverPill v1.0.0 | {}", BUILD_DATE))
                        .size(9.0)
                        .color(egui::Color32::from_gray(100)),
                );
            });
        });
    }

    fn render_android_content(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("📱 Recuperación Android").size(22.0).strong().color(ANDROID_COLOR));
                if !self.adb_available {
                    ui.label(egui::RichText::new("⚠️ ADB no detectado").size(14.0).color(ERROR_COLOR));
                }
            });
            ui.add_space(10.0);
            ui.separator();

            if !self.adb_available {
                egui::Frame::none()
                    .fill(CARD_BG)
                    .rounding(8.0)
                    .inner_margin(egui::Margin::same(20.0))
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(egui::RichText::new("📱").size(48.0));
                            ui.add_space(10.0);
                            ui.label(egui::RichText::new("ADB no detectado").size(16.0).strong().color(ERROR_COLOR));
                            ui.add_space(5.0);
                            ui.label("Instala Android SDK Platform Tools o conecta un dispositivo con depuración USB activada.");
                            ui.add_space(10.0);
                            if ui.button("🔍 Buscar ADB nuevamente").clicked() {
                                let available = AndroidRecoveryEngine::is_available();
                                self.adb_available = available;
                                if available {
                                    let mut engine = AndroidRecoveryEngine::new();
                                    self.android_devices = engine.detect_devices();
                                    self.android_engine = Some(engine);
                                }
                            }
                        });
                    });
                ui.add_space(10.0);
            } else {
                // Android main content
                egui::SidePanel::left("android_control")
                    .min_width(250.0)
                    .max_width(300.0)
                    .resizable(true)
                    .show(ctx, |ui| {
                        ui.add_space(10.0);
                        ui.label(egui::RichText::new("📱 Dispositivos").size(14.0).strong().color(ANDROID_COLOR));
                        ui.add_space(5.0);

                        if self.android_devices.is_empty() {
                            egui::Frame::none()
                                .fill(CARD_BG)
                                .rounding(6.0)
                                .inner_margin(egui::Margin::same(10.0))
                                .show(ui, |ui| {
                                    ui.label("No se detectaron dispositivos.");
                                    if ui.button("🔄 Escanear").clicked() {
                                        if let Some(ref mut engine) = self.android_engine {
                                            self.android_devices = engine.detect_devices();
                                        }
                                    }
                                });
                        } else {
                            for (i, device) in self.android_devices.iter().enumerate() {
                                let is_selected = self.android_selected_device == Some(i);
                                let bg = if is_selected { ANDROID_COLOR } else { CARD_BG };
                                if egui::Frame::none()
                                    .fill(bg)
                                    .rounding(6.0)
                                    .inner_margin(egui::Margin::same(8.0))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new("📱").size(18.0));
                                            ui.vertical(|ui| {
                                                ui.label(egui::RichText::new(&device.model).size(13.0).strong());
                                                ui.label(egui::RichText::new(&device.serial).size(10.0).color(egui::Color32::from_gray(150)));
                                            });
                                        });
                                        ui.add_space(4.0);
                                        ui.label(format!("{} | Android {}", device.manufacturer, device.android_version));
                                        if device.is_rooted {
                                            ui.label(egui::RichText::new("✅ Rooteado").size(10.0).color(SUCCESS_COLOR));
                                        }
                                    }).response.clicked()
                                {
                                    self.android_selected_device = Some(i);
                                }
                                ui.add_space(4.0);
                            }
                        }

                        ui.add_space(15.0);
                        ui.label(egui::RichText::new("⚡ Acciones").size(14.0).strong());
                        ui.add_space(5.0);
                        egui::Frame::none()
                            .fill(CARD_BG)
                            .rounding(6.0)
                            .inner_margin(egui::Margin::same(10.0))
                            .show(ui, |ui| {
                                if self.android_selected_device.is_none() {
                                    ui.label("Selecciona un dispositivo primero");
                                } else {
                                    if !self.android_is_scanning {
                                        if ui.add(
                                            egui::Button::new(egui::RichText::new("🔍 Escanear Dispositivo").size(13.0))
                                                .fill(ANDROID_COLOR)
                                                .min_size(egui::vec2(220.0, 32.0))
                                        ).clicked() {
                                            self.start_android_scan();
                                        }
                                        ui.add_space(5.0);
                                        if ui.add(
                                            egui::Button::new(egui::RichText::new("💾 Backup Rápido").size(13.0))
                                                .fill(ACCENT_COLOR)
                                                .min_size(egui::vec2(220.0, 32.0))
                                        ).clicked() {
                                            self.start_android_backup();
                                        }
                                    } else {
                                        ui.horizontal(|ui| {
                                            ui.spinner();
                                            ui.label("Escaneando...");
                                        });
                                        if ui.button("⏹ Detener").clicked() {
                                            if let Some(ref engine) = self.android_engine {
                                                engine.stop();
                                            }
                                            self.android_is_scanning = false;
                                        }
                                    }
                                }
                            });

                        ui.add_space(12.0);
                        ui.label(egui::RichText::new("📁 Destino").size(14.0).strong());
                        ui.add_space(5.0);
                        egui::Frame::none()
                            .fill(CARD_BG)
                            .rounding(6.0)
                            .inner_margin(egui::Margin::same(10.0))
                            .show(ui, |ui| {
                                if ui.button("📂 Carpeta...").clicked() {
                                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                        self.android_output_folder = path.to_string_lossy().to_string();
                                    }
                                }
                                if !self.android_output_folder.is_empty() {
                                    ui.label(egui::RichText::new(&self.android_output_folder).size(10.0).italics().color(egui::Color32::from_gray(150)));
                                }
                            });
                    });

                // Android results panel
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add_space(10.0);
                    let scan_result = self.android_scan_result.as_ref().cloned();
                    if let Some(result) = scan_result {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(format!("📁 Archivos encontrados: {}", result.found_files.len())).size(15.0).strong());
                            if !self.android_output_folder.is_empty() {
                                if ui.button("💾 Recuperar Todo").clicked() {
                                    self.recover_android_files();
                                }
                            }
                        });
                        ui.add_space(8.0);
                        ui.separator();

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            for (i, file) in result.found_files.iter().enumerate() {
                                let bg = if i % 2 == 0 { egui::Color32::from_rgb(35, 38, 48) } else { egui::Color32::from_rgb(30, 32, 40) };
                                egui::Frame::none()
                                    .fill(bg)
                                    .rounding(3.0)
                                    .inner_margin(egui::Margin::same(4.0))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            let icon = get_thumb_android(file.file_type);
                                            ui.label(egui::RichText::new(icon).size(14.0));
                                            ui.add_sized([200.0, 18.0], |ui: &mut egui::Ui| {
                                                ui.label(egui::RichText::new(&file.file_name).size(10.0))
                                            });
                                            ui.add_sized([60.0, 18.0], |ui: &mut egui::Ui| {
                                                ui.label(egui::RichText::new(file.file_type.extension().to_uppercase()).size(9.0).color(ACCENT_COLOR))
                                            });
                                            ui.add_sized([80.0, 18.0], |ui: &mut egui::Ui| {
                                                ui.label(format_size(file.size))
                                            });
                                            let rec_col = if file.recoverability >= 70.0 { SUCCESS_COLOR } else if file.recoverability >= 40.0 { WARNING_COLOR } else { ERROR_COLOR };
                                            ui.add_sized([60.0, 18.0], |ui: &mut egui::Ui| {
                                                ui.add(egui::ProgressBar::new(file.recoverability as f32 / 100.0).fill(rec_col).desired_width(50.0))
                                            });
                                            ui.label(egui::RichText::new(format!("{:.0}%", file.recoverability)).size(9.0).color(rec_col));
                                        });
                                    });
                                ui.add_space(2.0);
                            }
                        });
                    } else {
                        ui.vertical_centered(|ui| {
                            ui.add_space(30.0);
                            ui.label(egui::RichText::new("📱").size(48.0));
                            ui.add_space(10.0);
                            if self.android_is_scanning {
                                ui.label("Escaneando dispositivo Android...");
                                ui.spinner();
                            } else {
                                ui.label("Selecciona un dispositivo y escanea para comenzar");
                                ui.label(egui::RichText::new("Archivos compatibles: Fotos, Videos, WhatsApp, APKs, Bases de Datos").size(10.0).color(egui::Color32::from_gray(150)));
                            }
                        });
                    }
                });
            }
        });
    }

    fn render_settings_content(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(10.0);
            ui.label(egui::RichText::new("⚙️ Configuración Avanzada").size(22.0).strong().color(ACCENT_COLOR));
            ui.add_space(10.0);
            ui.separator();

            egui::Grid::new("settings_grid").num_columns(2).spacing([20.0, 10.0]).show(ui, |ui| {
                ui.label(egui::RichText::new("Escaneo Multi-Pass:").strong());
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.multi_pass_enabled, "Activar");
                    if self.multi_pass_enabled {
                        ui.add_space(10.0);
                        ui.label("Pasadas:");
                        ui.add(egui::Slider::new(&mut self.multi_pass_count, 2..=5).integer());
                    }
                });
                ui.end_row();

                ui.label(egui::RichText::new("Detección de Footers:").strong());
                ui.checkbox(&mut self.footer_detection_enabled, "Mejorar precisión en archivos fragmentados");
                ui.end_row();

                ui.label(egui::RichText::new("Items por página:").strong());
                ui.add(egui::Slider::new(&mut self.items_per_page, 50..=500).integer());
                ui.end_row();

                ui.label(egui::RichText::new("Calidad mínima:").strong());
                ui.add(egui::Slider::new(&mut self.min_recoverability, 0.0..=100.0).text("%"));
                ui.end_row();

                ui.label(egui::RichText::new("Ocultar duplicados:").strong());
                ui.checkbox(&mut self.hide_duplicates, "Ocultar archivos duplicados en resultados");
                ui.end_row();
            });

            ui.add_space(15.0);
            ui.separator();
            ui.add_space(10.0);

            ui.label(egui::RichText::new("📊 Estadísticas del Motor").size(16.0).strong());
            ui.add_space(5.0);
            egui::Frame::none()
                .fill(CARD_BG)
                .rounding(6.0)
                .inner_margin(egui::Margin::same(10.0))
                .show(ui, |ui| {
                    ui.label(format!("Firmas de archivos: {}", crate::core::signatures::SIGNATURE_DATABASE.len()));
                    ui.label(format!("Footers conocidos: {}", crate::core::signatures::FOOTER_DATABASE.len()));
                    ui.label(format!("Categorías: {}", get_categories().len()));
                    ui.label(if self.adb_available { "ADB: ✅ Disponible" } else { "ADB: ❌ No disponible" });
                });
        });
    }

    fn start_android_scan(&mut self) {
        let device_idx = match self.android_selected_device {
            Some(i) => i,
            None => return,
        };
        if device_idx >= self.android_devices.len() { return; }

        let device = self.android_devices[device_idx].clone();
        self.android_is_scanning = true;
        self.android_scan_result = None;
        self.add_console_message(format!("📱 Escaneando Android: {} ({})", device.model, device.serial), ConsoleLevel::Info);

        let (tx, rx) = std::sync::mpsc::channel();
        self.android_scan_receiver = Some(rx);

        let progress_tx = tx.clone();
        std::thread::spawn(move || {
            let mut engine = AndroidRecoveryEngine::new();
            let result = engine.scan_data_partition(&device, |msg| {
                info!("Android: {}", msg);
            });
            let _ = tx.send(result);
            drop(progress_tx);
        });
    }

    fn start_android_backup(&mut self) {
        let device_idx = match self.android_selected_device {
            Some(i) => i,
            None => return,
        };
        if device_idx >= self.android_devices.len() { return; }
        if self.android_output_folder.is_empty() { return; }

        let device = self.android_devices[device_idx].clone();
        let output = std::path::PathBuf::from(&self.android_output_folder);
        self.android_backup_in_progress = true;

        let (tx, rx) = std::sync::mpsc::channel();
        self.android_backup_receiver = Some(rx);

        self.add_console_message(format!("💾 Backup Android iniciado en: {}", self.android_output_folder), ConsoleLevel::Info);

        std::thread::spawn(move || {
            let engine = AndroidRecoveryEngine::new();
            let result = engine.backup_device(&device.serial, &output, |msg| {
                info!("Backup: {}", msg);
            });
            let _ = tx.send(result);
        });
    }

    fn recover_android_files(&mut self) {
        let result = match self.android_scan_result.clone() {
            Some(r) => r,
            None => {
                self.add_console_message("No hay resultados de escaneo para recuperar".to_string(), ConsoleLevel::Warning);
                return;
            }
        };

        if self.android_output_folder.is_empty() {
            self.add_console_message("Define una carpeta de destino primero".to_string(), ConsoleLevel::Warning);
            return;
        }

        let serial = result.device.serial.clone();
        let output = std::path::PathBuf::from(&self.android_output_folder);
        let files_to_recover: Vec<(String, String, u64)> = result.found_files.iter()
            .map(|f| (f.path.clone(), f.file_name.clone(), f.size))
            .collect();

        let total = files_to_recover.len();
        if total == 0 {
            self.add_console_message("No hay archivos para recuperar".to_string(), ConsoleLevel::Warning);
            return;
        }

        self.add_console_message(
            format!("💾 Recuperando {} archivos desde Android...", total),
            ConsoleLevel::Info,
        );

        self.android_backup_in_progress = true;

        let (tx, rx) = std::sync::mpsc::channel();
        self.android_backup_receiver = Some(rx);

        std::thread::spawn(move || {
            let engine = AndroidRecoveryEngine::new();
            let mut recovered = Vec::new();
            let mut had_error = false;

            for (i, (remote_path, file_name, _size)) in files_to_recover.iter().enumerate() {
                let rel_path = remote_path.trim_start_matches("/data/media/0/");
                if rel_path.is_empty() { continue; }

                let dest_path = output.join(rel_path);
                if let Some(parent) = dest_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }

                match engine.recover_file(&serial, remote_path, &dest_path) {
                    Ok(s) => {
                        info!("✅ Recuperado: {} ({} bytes)", file_name, s);
                        recovered.push(dest_path);
                    }
                    Err(e) => {
                        error!("❌ Error recuperando {}: {}", file_name, e);
                        had_error = true;
                    }
                }

                if i > 0 && i % 10 == 0 {
                    info!("Progreso Android: {}/{} archivos", i + 1, total);
                }
            }

            if had_error {
                let _ = tx.send(Err(format!("Completado con errores: {}/{} recuperados", recovered.len(), total)));
            } else {
                let _ = tx.send(Ok(recovered));
            }
        });
    }
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

fn get_thumb(t: crate::core::signatures::FileType) -> &'static str {
    use crate::core::signatures::FileType;
    match t {
        FileType::Jpeg
        | FileType::Png
        | FileType::Gif
        | FileType::Bmp
        | FileType::Tiff
        | FileType::Webp
        | FileType::Ico
        | FileType::Heic
        | FileType::Raw
        | FileType::Psd
        | FileType::Ai
        | FileType::Svg => "🖼️",
        FileType::Mp4
        | FileType::Avi
        | FileType::MkV
        | FileType::Mov
        | FileType::Wmv
        | FileType::WebM
        | FileType::Flv => "🎬",
        FileType::Mp3
        | FileType::Wav
        | FileType::Flac
        | FileType::Aac
        | FileType::Ogg
        | FileType::Wma => "🎵",
        FileType::Pdf => "📄",
        FileType::Doc | FileType::Docx => "📝",
        FileType::Xls | FileType::Xlsx => "📊",
        FileType::Ppt | FileType::Pptx => "📽️",
        FileType::Odt => "📃",
        FileType::Zip | FileType::Rar | FileType::SevenZip | FileType::Tar | FileType::Gzip => "📦",
        FileType::Exe | FileType::Dll | FileType::Msi => "⚙️",
        FileType::Apk => "📦",
        FileType::Dex => "⚡",
        FileType::Db => "🗄️",
        FileType::Xml => "📋",
        FileType::ThreeGp => "🎬",
        FileType::Text => "📄",
        FileType::AndroidFile => "📱",
        FileType::Unknown => "❓",
    }
}

fn get_thumb_android(t: crate::core::signatures::FileType) -> &'static str {
    use crate::core::signatures::FileType;
    match t {
        FileType::Jpeg | FileType::Png | FileType::Gif | FileType::Bmp | FileType::Webp | FileType::Heic => "🖼️",
        FileType::Mp4 | FileType::ThreeGp => "🎬",
        FileType::Mp3 | FileType::Ogg => "🎵",
        FileType::Apk => "📦",
        FileType::Dex => "⚡",
        FileType::Db => "🗄️",
        FileType::Pdf | FileType::Docx | FileType::Text => "📄",
        FileType::Xml => "📋",
        FileType::Zip => "📦",
        _ => "📱",
    }
}

impl Default for RecoverPillApp {
    fn default() -> Self {
        Self::new()
    }
}








