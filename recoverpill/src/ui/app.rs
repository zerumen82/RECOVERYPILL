//! Aplicación principal de recoverPill
//! 
//! Interfaz gráfica con egui para la recuperación de datos.

use eframe::{egui, App};
use log::info;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::disk::drive_info::{DriveInfo, get_available_drives};
use crate::core::scanner::{Scanner, ScanProgress, FoundFile};
use crate::core::signatures::get_categories;

/// Estado de la aplicación
pub struct RecoverPillApp {
    // Unidades disponibles
    drives: Vec<DriveInfo>,
    selected_drive: Option<usize>,
    
    // Escáner
    scanner: Option<Scanner>,
    is_scanning: bool,
    scan_progress: ScanProgress,
    found_files: Vec<FoundFile>,
    
    // Progreso del escaneo en porcentaje (para la barra de progreso)
    scan_percentage: f64,
    
    // Último múltiplo de 10% mostrado en UI (para notificaciones visuales)
    last_ten_percent: i32,
    
    // Notificación de progreso actual (para mostrar en UI)
    current_notification: Option<String>,
    notification_timer: f32,
    
    // Carpeta de recuperación
    output_folder: String,
    
    // Bandera de parada compartida entre UI y scanner
    should_stop: Arc<AtomicBool>,
    
    // Receiver para resultados del escaneo async
    scan_result_receiver: Option<std::sync::mpsc::Receiver<Result<crate::core::scanner::ScanResult, String>>>,
    
    // Canal para progreso en tiempo real
    progress_receiver: Option<std::sync::mpsc::Receiver<String>>,
    
    // Filtros
    enabled_filters: Vec<String>,
    all_filters_enabled: bool,
    
    // Consola
    console_messages: Vec<ConsoleMessage>,
    
    // Resultados
    selected_file: Option<usize>,
    
    // Vista previa de imagen
    preview_data: Option<Vec<u8>>,
    preview_file_index: Option<usize>,
    
    // Filtro de búsqueda
    filter_text: String,
    
    // Ordenar por
    sort_by: SortOption,
    sort_ascending: bool,
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

/// Opciones de ordenación
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortOption {
    Name,
    Type,
    Size,
    Recoverability,
    Entropy,
}

impl RecoverPillApp {
    /// Crea una nueva aplicación
    pub fn new() -> Self {
        info!("Inicializando recoverPill UI");
        
        let drives = get_available_drives();
        let categories = get_categories();
        let enabled_filters: Vec<String> = categories.iter().map(|s| s.to_string()).collect();
        
        RecoverPillApp {
            drives,
            selected_drive: None,
            scanner: None,
            is_scanning: false,
            scan_progress: ScanProgress::new(0),
            found_files: Vec::new(),
            scan_percentage: 0.0,
            last_ten_percent: -1,
            current_notification: None,
            notification_timer: 0.0,
            should_stop: Arc::new(AtomicBool::new(false)),
            scan_result_receiver: None,
            progress_receiver: None,
            enabled_filters,
            all_filters_enabled: true,
            console_messages: vec![
                ConsoleMessage {
                    text: "recoverPill v1.0.0 listo".to_string(),
                    level: ConsoleLevel::Info,
                },
            ],
            selected_file: None,
            preview_data: None,
            preview_file_index: None,
            filter_text: String::new(),
            output_folder: String::new(),
            sort_by: SortOption::Recoverability,
            sort_ascending: false,
        }
    }

    /// Agrega un mensaje a la consola
    fn add_console_message(&mut self, text: String, level: ConsoleLevel) {
        self.console_messages.push(ConsoleMessage { text, level });
        
        if self.console_messages.len() > 100 {
            self.console_messages.remove(0);
        }
    }

    /// Inicia el escaneo
    fn start_scan(&mut self) {
        if self.selected_drive.is_none() {
            self.add_console_message("Seleccione una unidad primero".to_string(), ConsoleLevel::Warning);
            return;
        }
        
        if self.is_scanning {
            self.add_console_message("Ya hay un escaneo en progreso".to_string(), ConsoleLevel::Warning);
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
        
        self.add_console_message(format!("Iniciando escaneo de {}...", drive_path), ConsoleLevel::Info);
        self.add_console_message(format!("Tamaño de unidad: {} bytes ({})", drive_size, DriveInfo::format_size(drive_size)), ConsoleLevel::Info);
        
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
        
        // Resetear la bandera de parada
        self.should_stop.store(false, Ordering::SeqCst);
        
        // Crear canal de progreso
        let (progress_tx, progress_rx) = std::sync::mpsc::channel();
        self.progress_receiver = Some(progress_rx);
        
        // Crear el scanner y obtener referencia a la bandera de parada
        let should_stop = self.should_stop.clone();
        
        // Canal para obtener el resultado
        let (tx, rx) = std::sync::mpsc::channel();
        
        std::thread::spawn(move || {
            // Crear el scanner dentro del hilo
            match Scanner::new(&drive_path) {
                Ok(mut scanner) => {
                    // Get the scanner's should_stop flag and set it to match
                    let scanner_stop = scanner.get_should_stop();
                    
                    // Sincronizar: copiar el estado inicial de should_stop al scanner
                    scanner_stop.store(should_stop.load(Ordering::SeqCst), Ordering::SeqCst);
                    
                    // Spawn a thread to monitor the UI's stop flag
                    let stop_watcher = std::thread::spawn(move || {
                        while !should_stop.load(Ordering::SeqCst) {
                            std::thread::sleep(std::time::Duration::from_millis(50));
                        }
                        // Signal scanner to stop
                        scanner_stop.store(true, Ordering::SeqCst);
                    });
                    
                    // Wrap progress callback to send through channel
                    let progress_tx_clone = progress_tx.clone();
                    let result = scanner.scan_with_progress(move |msg| {
                        let _ = progress_tx_clone.send(msg);
                    });
                    
                    // Stop the watcher thread
                    let _ = stop_watcher.join();
                    let _ = tx.send(Ok(result));
                }
                Err(e) => {
                    let _ = tx.send(Err(e));
                }
            }
        });
        
        // Guardar el receiver
        self.scan_result_receiver = Some(rx);
    }
    
    /// Procesa resultados del escaneo en background
    fn process_scan_results(&mut self) {
        // Procesar mensajes de progreso en tiempo real - extraer porcentaje
        let progress_msgs: Vec<String> = if let Some(ref rx) = self.progress_receiver {
            let mut msgs = Vec::new();
            while let Ok(msg) = rx.try_recv() {
                msgs.push(msg);
            }
            msgs
        } else {
            Vec::new()
        };
        
        // Extraer porcentaje de los mensajes y actualizar barra de progreso
        // Solo mostrar progreso en la barra, no en consola
        for msg in &progress_msgs {
            // Extraer porcentaje del mensaje "Progreso: X% - Y archivos encontrados"
            if let Some(percent_str) = msg.split("Progreso: ").nth(1) {
                if let Some(percent) = percent_str.split('%').next() {
                    if let Ok(p) = percent.trim().parse::<f64>() {
                        self.scan_percentage = p;
                        
                        // Notificación cada 1% en consola (más detallado)
                        let current_percent = p as i32;
                        if current_percent > self.last_ten_percent {
                            // Siempre mostrar cada 1% en consola
                            let file_count = progress_msgs.iter()
                                .filter_map(|m| m.split("archivos encontrados").next())
                                .filter_map(|s| s.split_whitespace().last())
                                .filter_map(|s| s.parse::<usize>().ok())
                                .max()
                                .unwrap_or(0);
                            
                            self.add_console_message(
                                format!("Progreso: {}% - {} archivos encontrados", current_percent, file_count),
                                ConsoleLevel::Info
                            );
                            self.last_ten_percent = current_percent;
                        }
                        
                        // Mostrar notificación VISUAL cada 10% en la UI
                        let ten_percent = (p / 10.0).floor() as i32;
                        let ten_percent_shown = (self.last_ten_percent as f64 / 10.0).floor() as i32;
                        if ten_percent > ten_percent_shown && ten_percent > 0 && ten_percent <= 10 {
                            // Notificación visual en la UI
                            self.current_notification = Some(format!("📊 Escaneo al {}% - {} archivos encontrados", 
                                ten_percent * 10, 
                                progress_msgs.iter().filter_map(|m| m.split("archivos encontrados").next())
                                    .filter_map(|s| s.split_whitespace().last())
                                    .filter_map(|s| s.parse::<usize>().ok())
                                    .max()
                                    .unwrap_or(0)
                            ));
                            self.notification_timer = 3.0;
                        }
                    }
                }
            }
            
            // NO agregar mensajes a la consola durante el escaneo - solo mostrar en la barra de progreso
            // Esto mantiene la consola limpia
        }
        
        // Actualizar temporizador de notificación
        if self.notification_timer > 0.0 {
            self.notification_timer -= 0.016; // Asumiendo ~60 FPS
            if self.notification_timer <= 0.0 {
                self.current_notification = None;
            }
        }
        
        // Agregar mensaje final de progreso si hay archivos encontrados
        if let Some(last_msg) = progress_msgs.last() {
            if let Some(count_str) = last_msg.split("archivos encontrados").next() {
                if let Ok(count) = count_str.split_whitespace().last().unwrap_or("0").parse::<usize>() {
                    if count > 0 && !self.is_scanning {
                        // Scan completed
                    }
                }
            }
        }
        
        if let Some(ref rx) = self.scan_result_receiver {
            // Intentar recibir resultado sin bloquear
            match rx.try_recv() {
                Ok(Ok(result)) => {
                    // Escaneo completado exitosamente
                    self.found_files = result.files_found;
                    self.is_scanning = false;
                    self.scan_progress.is_running = false;
                    self.scan_result_receiver = None;
                    self.progress_receiver = None;
                    self.add_console_message(
                        format!("Escaneo completado: {} archivos encontrados", self.found_files.len()),
                        ConsoleLevel::Success
                    );
                }
                Ok(Err(e)) => {
                    self.is_scanning = false;
                    self.scan_progress.is_running = false;
                    self.scan_result_receiver = None;
                    self.progress_receiver = None;
                    self.add_console_message(
                        format!("Error en escaneo: {}", e),
                        ConsoleLevel::Error
                    );
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // El escaneo aún está en progreso
                    if self.should_stop.load(Ordering::SeqCst) {
                        // Usuario solicitó parar - no hacer nada, esperar al resultado
                        // El hilo del scanner debería enviar el resultado pronto
                        self.add_console_message(
                            "Escaneo detenido, esperando resultados...".to_string(),
                            ConsoleLevel::Warning
                        );
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // El hilo del scanner terminó pero no recibió resultado
                    // Esto no debería pasar si el scanner envía siempre el resultado
                    self.is_scanning = false;
                    self.scan_progress.is_running = false;
                    self.scan_result_receiver = None;
                    self.progress_receiver = None;
                    self.add_console_message(
                        "Advertencia: no se recibieron archivos encontrados".to_string(),
                        ConsoleLevel::Warning
                    );
                }
            }
        }
    }
    
    /// Detiene el escaneo en progreso
    fn stop_scan(&mut self) {
        // Señalar al scanner que se detenga
        self.should_stop.store(true, Ordering::SeqCst);
        // No desconectar los receivers inmediatamente - esperar a que el hilo termine
        // El resultado se procesará en process_scan_results()
        self.add_console_message("Deteniendo escaneo... (esperando archivos encontrados)".to_string(), ConsoleLevel::Warning);
    }
    
    /// Alterna la selección de un archivo
    fn toggle_file_selection(&mut self, index: usize) {
        if index < self.found_files.len() {
            self.found_files[index].selected = !self.found_files[index].selected;
        }
    }
    
    /// Selecciona todos los archivos
    fn select_all(&mut self) {
        for file in &mut self.found_files {
            file.selected = true;
        }
        self.add_console_message("Todos los archivos seleccionados".to_string(), ConsoleLevel::Info);
    }
    
    /// Deselecciona todos los archivos
    fn deselect_all(&mut self) {
        for file in &mut self.found_files {
            file.selected = false;
        }
        self.add_console_message("Todos los archivos deseleccionados".to_string(), ConsoleLevel::Info);
    }
    
    /// Limpia todos los archivos encontrados
    fn clear_all_files(&mut self) {
        let count = self.found_files.len();
        self.found_files.clear();
        self.selected_file = None;
        if count > 0 {
            self.add_console_message(
                format!("{} archivos eliminados de la lista", count),
                ConsoleLevel::Info
            );
        }
    }
    
    /// Obtiene los archivos seleccionados
    fn get_selected_files(&self) -> Vec<&FoundFile> {
        self.found_files.iter().filter(|f| f.selected).collect()
    }
    
    /// Recupera los archivos seleccionados - marca para recuperación
    fn recover_selected_files(&mut self) {
        let count = self.found_files.iter().filter(|f| f.selected).count();
        if count == 0 {
            self.add_console_message("No hay archivos seleccionados para recuperar".to_string(), ConsoleLevel::Warning);
        } else {
            self.add_console_message(
                format!("{} archivos seleccionados para recuperar", count),
                ConsoleLevel::Success
            );
            if self.output_folder.is_empty() {
                self.add_console_message(
                    "⚠️ Define una carpeta de recuperación primero".to_string(),
                    ConsoleLevel::Warning
                );
            } else {
                self.add_console_message(
                    format!("📁 Carpeta de recuperación: {}", self.output_folder),
                    ConsoleLevel::Info
                );
            }
        }
    }
    
    /// Establece la carpeta de recuperación
    fn set_output_folder(&mut self, folder: String) {
        self.output_folder = folder.clone();
        self.add_console_message(
            format!("📁 Carpeta de recuperación configurada: {}", folder),
            ConsoleLevel::Success
        );
    }
    
    /// Ordena los archivos
    fn sort_files(&mut self, sort_by: SortOption, ascending: bool) {
        self.sort_by = sort_by;
        self.sort_ascending = ascending;
        
        match sort_by {
            SortOption::Name => {
                self.found_files.sort_by(|a, b| {
                    let cmp = a.file_name.to_lowercase().cmp(&b.file_name.to_lowercase());
                    if ascending { cmp } else { cmp.reverse() }
                });
            }
            SortOption::Type => {
                self.found_files.sort_by(|a, b| {
                    let cmp = a.file_type.extension().cmp(&b.file_type.extension());
                    if ascending { cmp } else { cmp.reverse() }
                });
            }
            SortOption::Size => {
                self.found_files.sort_by(|a, b| {
                    let cmp = a.estimated_size.cmp(&b.estimated_size);
                    if ascending { cmp } else { cmp.reverse() }
                });
            }
            SortOption::Recoverability => {
                self.found_files.sort_by(|a, b| {
                    let cmp = a.recoverability.partial_cmp(&b.recoverability).unwrap();
                    if ascending { cmp } else { cmp.reverse() }
                });
            }
            SortOption::Entropy => {
                self.found_files.sort_by(|a, b| {
                    let cmp = a.entropy.partial_cmp(&b.entropy).unwrap();
                    if ascending { cmp } else { cmp.reverse() }
                });
            }
        }
        
        self.add_console_message(
            format!("📋 Archivos ordenados por {:?}", sort_by),
            ConsoleLevel::Info
        );
    }
    
    /// Carga la previsualización de un archivo de imagen
    fn load_preview(&mut self, index: usize) -> bool {
        if index >= self.found_files.len() {
            return false;
        }
        
        let file = &self.found_files[index];
        
        // Solo cargar previsualización para tipos de imagen
        match file.file_type {
            crate::core::signatures::FileType::Jpeg | 
            crate::core::signatures::FileType::Png | 
            crate::core::signatures::FileType::Gif | 
            crate::core::signatures::FileType::Bmp | 
            crate::core::signatures::FileType::Webp | 
            crate::core::signatures::FileType::Ico => {
                // Por ahora, guardamos que este archivo tiene preview
                // La carga real de datos requeriría acceso al DiskReader
                self.preview_file_index = Some(index);
                true
            }
            _ => {
                self.preview_file_index = None;
                false
            }
        }
    }
    
    /// Determina si un archivo puede tener previsualización
    fn can_have_preview(file_type: crate::core::signatures::FileType) -> bool {
        matches!(file_type,
            crate::core::signatures::FileType::Jpeg | 
            crate::core::signatures::FileType::Png | 
            crate::core::signatures::FileType::Gif | 
            crate::core::signatures::FileType::Bmp | 
            crate::core::signatures::FileType::Webp | 
            crate::core::signatures::FileType::Ico |
            crate::core::signatures::FileType::Heic |
            crate::core::signatures::FileType::Raw
        )
    }
}

impl App for RecoverPillApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Procesar resultados del escaneo async
        self.process_scan_results();
        
        // Solicitar repintado si está escaneando
        if self.is_scanning {
            ctx.request_repaint();
        }
        
        // === PANEL LATERAL DERECHO - Análisis IA ===
        egui::SidePanel::right("preview_panel").min_width(250.0).max_width(300.0).show(ctx, |ui| {
            ui.heading("📊 Análisis IA");
            ui.separator();
            
            if let Some(idx) = self.preview_file_index {
                if idx < self.found_files.len() {
                    let file = &self.found_files[idx];
                    
                    ui.label(egui::RichText::new(&file.file_name).strong().size(14.0));
                    ui.separator();
                    
                    ui.label(egui::RichText::new("DETALLES").strong());
                    ui.label(format!("Tipo: {}", file.file_type.extension().to_uppercase()));
                    ui.label(format!("Tamaño: {}", format_size(file.estimated_size)));
                    ui.label(format!("Offset: {}", file.offset));
                    ui.separator();
                    
                    let recover_color = if file.recoverability >= 70.0 {
                        egui::Color32::GREEN
                    } else if file.recoverability >= 40.0 {
                        egui::Color32::YELLOW
                    } else {
                        egui::Color32::RED
                    };
                    ui.colored_label(recover_color, 
                        format!("🎯 Recuperabilidad: {:.0}%", file.recoverability));
                    
                    let entropy_color = if file.entropy < 2.0 {
                        egui::Color32::RED
                    } else if file.entropy > 7.5 {
                        egui::Color32::YELLOW
                    } else {
                        egui::Color32::GREEN
                    };
                    ui.colored_label(entropy_color, 
                        format!("📊 Entropía: {:.2}", file.entropy));
                }
            } else {
                ui.label("Sin selección");
                ui.separator();
                ui.label(egui::RichText::new("Tips:").strong());
                ui.label("• >70% recuperabilidad = bueno");
                ui.label("• Entropía 3-7.5 = normal");
            }
        });
        
        // === PANEL CENTRAL ===
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("recoverPill - Recuperación de Datos");
            ui.separator();
            
            // Selector de unidad
            ui.horizontal(|ui| {
                ui.label("Unidad:");
                
                let mut drive_text = "Seleccionar...".to_string();
                if let Some(idx) = self.selected_drive {
                    if idx < self.drives.len() {
                        let d = &self.drives[idx];
                        drive_text = format!("{} ({})", d.path, DriveInfo::format_size(d.total_bytes));
                    }
                }
                
                egui::ComboBox::from_id_source("drive_selector")
                    .selected_text(drive_text)
                    .show_ui(ui, |ui| {
                        for (i, d) in self.drives.iter().enumerate() {
                            let text = format!("{} ({})", d.path, DriveInfo::format_size(d.total_bytes));
                            ui.selectable_value(&mut self.selected_drive, Some(i), text);
                        }
                    });
                
                if ui.button("🔍 Escanear").clicked() {
                    self.start_scan();
                }
                
                // Botón de parada
                if self.is_scanning {
                    ui.label("⏳ Escaneando...");
                    if ui.button("⏹ Parar").clicked() {
                        self.stop_scan();
                    }
                }
            });
            
            // Mostrar progreso si está escaneando
            if self.is_scanning {
                ui.horizontal(|ui| {
                    ui.label("🔄 Escaneo en progreso...");
                    let progress_percent = self.scan_percentage as f32 / 100.0;
                    ui.add(egui::ProgressBar::new(progress_percent).show_percentage());
                    ui.label(format!("{:.1}%", self.scan_percentage));
                });
            }
            
            // Notificación de progreso cada 10% - EN PANEL SEPARADO
            if let Some(ref notification) = self.current_notification {
                ui.add_space(5.0);
                egui::Frame::group(ui.style())
                    .fill(egui::Color32::from_rgb(50, 100, 150))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(notification)
                            .size(16.0)
                            .color(egui::Color32::WHITE)
                            .strong());
                    });
                ui.add_space(5.0);
            }
            
            ui.separator();
            
            // Título de archivos - en su propia línea
            ui.heading(format!("📁 Archivos Encontrados ({})", self.found_files.len()));
            
            // Selector de carpeta de recuperación - con botón
            ui.horizontal(|ui| {
                ui.label("📂 Carpeta de recuperación:");
                if ui.button("📁 Seleccionar carpeta...").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_directory(".")
                        .pick_folder()
                    {
                        self.set_output_folder(path.to_string_lossy().to_string());
                    }
                }
                if !self.output_folder.is_empty() {
                    ui.label(egui::RichText::new(&self.output_folder).small().italics());
                }
            });
            
            // Barra de controles
            ui.horizontal(|ui| {
                // Ordenar por
                ui.label("🔽 Ordenar:");
                
                let sort_label = match self.sort_by {
                    SortOption::Name => "Nombre",
                    SortOption::Type => "Tipo",
                    SortOption::Size => "Tamaño",
                    SortOption::Recoverability => "Recuperabilidad",
                    SortOption::Entropy => "Entropía",
                };
                
                egui::ComboBox::from_id_source("sort_selector")
                    .selected_text(sort_label)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.sort_by, SortOption::Recoverability, "Recuperabilidad");
                        ui.selectable_value(&mut self.sort_by, SortOption::Name, "Nombre");
                        ui.selectable_value(&mut self.sort_by, SortOption::Type, "Tipo");
                        ui.selectable_value(&mut self.sort_by, SortOption::Size, "Tamaño");
                        ui.selectable_value(&mut self.sort_by, SortOption::Entropy, "Entropía");
                    });
                
                // Botón invertir orden
                if ui.button(if self.sort_ascending { "⬆️" } else { "⬇️" })
                    .on_hover_text("Invertir orden").clicked() {
                    self.sort_ascending = !self.sort_ascending;
                    self.sort_files(self.sort_by, self.sort_ascending);
                }
                
                ui.separator();
                
                // Filtro
                ui.label("🔍 Buscar:");
                ui.text_edit_singleline(&mut self.filter_text);
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("💾 Recuperar Seleccionados").clicked() {
                        self.recover_selected_files();
                    }
                    if ui.button("❌ Deseleccionar Todo").clicked() {
                        self.deselect_all();
                    }
                    if ui.button("📋 Seleccionar Todo").clicked() {
                        self.select_all();
                    }
                    if ui.button("🗑️ Clear All").clicked() {
                        self.clear_all_files();
                    }
                    let selected_count = self.get_selected_files().len();
                    if selected_count > 0 {
                        ui.label(format!("✅ {} seleccionados", selected_count));
                    }
                });
            });
            
            // Contador de archivos por tipo
            if !self.found_files.is_empty() {
                ui.horizontal(|ui| {
                    ui.label("📊 Resumen:");
                    let mut type_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
                    for f in &self.found_files {
                        let type_name = f.file_type.extension().to_uppercase();
                        *type_counts.entry(type_name).or_insert(0) += 1;
                    }
                    for (t, c) in type_counts.iter().take(5) {
                        ui.label(format!("{}: {}", t, c));
                    }
                    if type_counts.len() > 5 {
                        ui.label(format!("... +{}", type_counts.len() - 5));
                    }
                });
            }
            
            // Mostrar archivos encontrados en formato de tabla con columnas
            egui::ScrollArea::vertical().stick_to_bottom(false).show(ui, |ui| {
                // Encabezados de columna - AHORA COINCIDE CON DATOS (8 columnas)
                egui::Grid::new("file_grid_header")
                    .num_columns(8)
                    .spacing([10.0, 6.0])
                    .show(ui, |ui| {
                        ui.style_mut().override_text_style = Some(egui::TextStyle::Heading);
                        ui.label("☑");
                        ui.label("🖼️");
                        ui.label("📝 Nombre");
                        ui.label("📋 Tipo");
                        ui.label("💾 Tamaño");
                        ui.label("📍 Offset");
                        ui.label("🎯 %");
                        ui.label("📊 Ent");
                        ui.end_row();
                    });
                
                ui.separator();
                
                let mut toggle_idx: Option<usize> = None;
                
                // Filtrar y ordenar archivos
                let mut filtered_files: Vec<(usize, &FoundFile)> = if self.filter_text.is_empty() {
                    self.found_files.iter().enumerate().collect()
                } else {
                    let filter_lower = self.filter_text.to_lowercase();
                    self.found_files.iter().enumerate()
                        .filter(|(_, f)| {
                            f.file_name.to_lowercase().contains(&filter_lower) ||
                            f.file_type.extension().to_lowercase().contains(&filter_lower)
                        })
                        .collect()
                };
                
                // Ordenar si hay filtro activo o si el usuario cambió la ordenación
                if !self.filter_text.is_empty() || true {
                    // Crear un vector ordenado basado en la opción
                    let sort_by = self.sort_by;
                    let ascending = self.sort_ascending;
                    filtered_files.sort_by(|(_, a), (_, b)| {
                        let cmp = match sort_by {
                            SortOption::Name => a.file_name.to_lowercase().cmp(&b.file_name.to_lowercase()),
                            SortOption::Type => a.file_type.extension().cmp(&b.file_type.extension()),
                            SortOption::Size => a.estimated_size.cmp(&b.estimated_size),
                            SortOption::Recoverability => a.recoverability.partial_cmp(&b.recoverability).unwrap(),
                            SortOption::Entropy => a.entropy.partial_cmp(&b.entropy).unwrap(),
                        };
                        if ascending { cmp } else { cmp.reverse() }
                    });
                }
                
                // Mostrar contador de archivos filtrados
                if !self.filter_text.is_empty() {
                    ui.label(format!("🔍 Mostrando {} de {} archivos", filtered_files.len(), self.found_files.len()));
                }
                
                for (i, file) in filtered_files {
                    egui::Grid::new("file_grid")
                        .num_columns(8)
                        .spacing([10.0, 6.0])
                        .show(ui, |ui| {
                            let mut check_state = file.selected;
                            
                            // Checkbox para seleccionar
                            if ui.checkbox(&mut check_state, "").clicked() {
                                toggle_idx = Some(i);
                            }
                            
                            // Thumbnail/icono del archivo
                            ui.label(get_file_thumbnail(file.file_type));
                            
                            // Nombre del archivo
                            let can_preview = RecoverPillApp::can_have_preview(file.file_type);
                            let name_color = if can_preview {
                                egui::Color32::from_rgb(100, 149, 237) // Azul para imágenes
                            } else {
                                egui::Color32::WHITE
                            };
                            ui.label(egui::RichText::new(&file.file_name).color(name_color));
                            
                            // Tipo de archivo (extensión)
                            ui.label(file.file_type.extension().to_uppercase());
                            
                            // Tamaño
                            ui.label(format_size(file.estimated_size));
                            
                            // Offset
                            ui.label(format!("{}", file.offset));
                            
                            // Porcentaje de recuperabilidad
                            let recover_color = if file.recoverability >= 70.0 {
                                egui::Color32::GREEN
                            } else if file.recoverability >= 40.0 {
                                egui::Color32::YELLOW
                            } else {
                                egui::Color32::RED
                            };
                            ui.colored_label(recover_color, format!("{:.0}%", file.recoverability));
                            
                            // Entropía
                            ui.label(format!("{:.1}", file.entropy));
                            
                            ui.end_row();
                        });
                }
                
                // Apply toggle after UI is done
                if let Some(idx) = toggle_idx {
                    self.toggle_file_selection(idx);
                }
            });
            
            ui.separator();
            
            // Consola
            ui.heading("🤖 Consola IA");
            egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                for msg in &self.console_messages {
                    let color = match msg.level {
                        ConsoleLevel::Info => egui::Color32::GRAY,
                        ConsoleLevel::Warning => egui::Color32::YELLOW,
                        ConsoleLevel::Error => egui::Color32::RED,
                        ConsoleLevel::Success => egui::Color32::GREEN,
                    };
                    
                    ui.colored_label(color, &msg.text);
                }
            });
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
        format!("{} bytes", bytes)
    }
}

/// Obtiene el icono/thumbnail para un tipo de archivo
fn get_file_thumbnail(file_type: crate::core::signatures::FileType) -> &'static str {
    use crate::core::signatures::FileType;
    match file_type {
        // Imágenes
        FileType::Jpeg | FileType::Png | FileType::Gif | FileType::Bmp | 
        FileType::Tiff | FileType::Webp | FileType::Ico |
        FileType::Heic | FileType::Raw | FileType::Psd | FileType::Ai | FileType::Svg => "🖼️",
        // Videos
        FileType::Mp4 | FileType::Avi | FileType::MkV | FileType::Mov | 
        FileType::Wmv | FileType::WebM | FileType::Flv => "🎬",
        // Audio
        FileType::Mp3 | FileType::Wav | FileType::Flac | FileType::Aac | 
        FileType::Ogg | FileType::Wma => "🎵",
        // Documentos
        FileType::Pdf => "📄",
        FileType::Doc | FileType::Docx => "📝",
        FileType::Xls | FileType::Xlsx => "📊",
        FileType::Ppt | FileType::Pptx => "📽️",
        FileType::Odt => "📃",
        // Comprimidos
        FileType::Zip | FileType::Rar | FileType::SevenZip | 
        FileType::Tar | FileType::Gzip => "📦",
        // Ejecutables
        FileType::Exe | FileType::Dll | FileType::Msi => "⚙️",
        // Unknown
        FileType::Unknown => "❓",
    }
}

impl Default for RecoverPillApp {
    fn default() -> Self {
        Self::new()
    }
}
