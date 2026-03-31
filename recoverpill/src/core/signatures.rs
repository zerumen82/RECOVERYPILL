//! Base de datos de firmas de archivos
//!
//! Define las firmas (magic bytes) para detectar diferentes tipos de archivos.

use std::sync::LazyLock;

/// Tipos de archivos soportados
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FileType {
    // Imágenes
    Jpeg,
    Png,
    Gif,
    Bmp,
    Tiff,
    Webp,
    Ico,
    // Imágenes adicionales
    Heic,
    Raw, // RAW de cámaras
    Psd,
    Ai,
    Svg,
    // Documentos
    Pdf,
    Doc,
    Docx,
    Xls,
    Xlsx,
    Ppt,
    Pptx,
    Odt,
    // Archivos comprimidos
    Zip,
    Rar,
    SevenZip,
    Tar,
    Gzip,
    // Audio
    Mp3,
    Wav,
    Flac,
    Aac,
    Ogg,
    Wma,
    // Video
    Mp4,
    Avi,
    MkV,
    Mov,
    Wmv,
    WebM,
    Flv,
    // Ejecutables
    Exe,
    Dll,
    Msi,
    // Otros
    Unknown,
}

impl FileType {
    /// Obtiene la extensión del archivo
    pub fn extension(&self) -> &'static str {
        match self {
            FileType::Jpeg => "jpg",
            FileType::Png => "png",
            FileType::Gif => "gif",
            FileType::Bmp => "bmp",
            FileType::Tiff => "tiff",
            FileType::Webp => "webp",
            FileType::Ico => "ico",
            FileType::Heic => "heic",
            FileType::Raw => "raw",
            FileType::Psd => "psd",
            FileType::Ai => "ai",
            FileType::Svg => "svg",
            FileType::Pdf => "pdf",
            FileType::Doc => "doc",
            FileType::Docx => "docx",
            FileType::Xls => "xls",
            FileType::Xlsx => "xlsx",
            FileType::Ppt => "ppt",
            FileType::Pptx => "pptx",
            FileType::Odt => "odt",
            FileType::Zip => "zip",
            FileType::Rar => "rar",
            FileType::SevenZip => "7z",
            FileType::Tar => "tar",
            FileType::Gzip => "gz",
            FileType::Mp3 => "mp3",
            FileType::Wav => "wav",
            FileType::Flac => "flac",
            FileType::Aac => "aac",
            FileType::Ogg => "ogg",
            FileType::Wma => "wma",
            FileType::Mp4 => "mp4",
            FileType::Avi => "avi",
            FileType::MkV => "mkv",
            FileType::Mov => "mov",
            FileType::Wmv => "wmv",
            FileType::WebM => "webm",
            FileType::Flv => "flv",
            FileType::Exe => "exe",
            FileType::Dll => "dll",
            FileType::Msi => "msi",
            FileType::Unknown => "bin",
        }
    }

    /// Obtiene el nombre legible del tipo
    pub fn display_name(&self) -> &'static str {
        match self {
            FileType::Jpeg => "JPEG Image",
            FileType::Png => "PNG Image",
            FileType::Gif => "GIF Image",
            FileType::Bmp => "Bitmap Image",
            FileType::Tiff => "TIFF Image",
            FileType::Webp => "WebP Image",
            FileType::Ico => "Icon",
            FileType::Heic => "HEIC Image",
            FileType::Raw => "RAW Image",
            FileType::Psd => "Photoshop Doc",
            FileType::Ai => "Adobe Illustrator",
            FileType::Svg => "SVG Image",
            FileType::Pdf => "PDF Document",
            FileType::Doc => "Word Document",
            FileType::Docx => "Word Document",
            FileType::Xls => "Excel Spreadsheet",
            FileType::Xlsx => "Excel Spreadsheet",
            FileType::Ppt => "PowerPoint",
            FileType::Pptx => "PowerPoint",
            FileType::Odt => "OpenDocument",
            FileType::Zip => "ZIP Archive",
            FileType::Rar => "RAR Archive",
            FileType::SevenZip => "7-Zip Archive",
            FileType::Tar => "TAR Archive",
            FileType::Gzip => "GZIP Archive",
            FileType::Mp3 => "MP3 Audio",
            FileType::Wav => "WAV Audio",
            FileType::Flac => "FLAC Audio",
            FileType::Aac => "AAC Audio",
            FileType::Ogg => "OGG Audio",
            FileType::Wma => "WMA Audio",
            FileType::Mp4 => "MP4 Video",
            FileType::Avi => "AVI Video",
            FileType::MkV => "MKV Video",
            FileType::Mov => "MOV Video",
            FileType::Wmv => "WMV Video",
            FileType::WebM => "WebM Video",
            FileType::Flv => "FLV Video",
            FileType::Exe => "Executable",
            FileType::Dll => "Dynamic Library",
            FileType::Msi => "Installer",
            FileType::Unknown => "Unknown",
        }
    }

    /// Obtiene la categoría del archivo
    pub fn category(&self) -> &'static str {
        match self {
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
            | FileType::Svg => "Imágenes",

            FileType::Pdf
            | FileType::Doc
            | FileType::Docx
            | FileType::Xls
            | FileType::Xlsx
            | FileType::Ppt
            | FileType::Pptx
            | FileType::Odt => "Documentos",

            FileType::Zip | FileType::Rar | FileType::SevenZip | FileType::Tar | FileType::Gzip => {
                "Archivos"
            }

            FileType::Mp3
            | FileType::Wav
            | FileType::Flac
            | FileType::Aac
            | FileType::Ogg
            | FileType::Wma => "Audio",

            FileType::Mp4
            | FileType::Avi
            | FileType::MkV
            | FileType::Mov
            | FileType::Wmv
            | FileType::WebM
            | FileType::Flv => "Video",

            FileType::Exe | FileType::Dll | FileType::Msi => "Ejecutables",

            FileType::Unknown => "Otros",
        }
    }
}

/// Firma de archivo conocida
#[derive(Debug, Clone)]
pub struct FileSignature {
    pub file_type: FileType,
    pub magic_bytes: &'static [u8],
    pub offset: usize,
    pub min_size: usize,
    pub max_size: Option<usize>,
}

impl FileSignature {
    /// Verifica si los datos coinciden con esta firma
    pub fn matches(&self, data: &[u8]) -> bool {
        if data.len() < self.offset + self.magic_bytes.len() {
            return false;
        }

        for (i, &byte) in self.magic_bytes.iter().enumerate() {
            if data[self.offset + i] != byte {
                return false;
            }
        }
        true
    }

    /// Estima el tamaño del archivo basado en los datos
    pub fn estimate_size(&self, data: &[u8]) -> Option<u64> {
        match self.file_type {
            FileType::Jpeg => estimate_jpeg_size(data),
            FileType::Png => estimate_png_size(data),
            FileType::Gif => estimate_gif_size(data),
            FileType::Bmp => estimate_bmp_size(data),
            FileType::Pdf => estimate_pdf_size(data),
            FileType::Zip => estimate_zip_size(data),
            FileType::Mp3 => estimate_mp3_size(data),
            FileType::Mp4 => estimate_mp4_size(data),
            _ => None,
        }
    }
}

// Funciones de estimación de tamaño
fn estimate_jpeg_size(data: &[u8]) -> Option<u64> {
    for i in (2..data.len() - 1).rev() {
        if data[i] == 0xFF && data[i + 1] == 0xD9 {
            return Some((i + 2) as u64);
        }
    }
    None
}

fn estimate_png_size(data: &[u8]) -> Option<u64> {
    for i in (8..data.len() - 7).rev() {
        if &data[i..i + 4] == b"IEND"
            && data[i + 4] == 0xAE
            && data[i + 5] == 0x42
            && data[i + 6] == 0x60
            && data[i + 7] == 0x82
        {
            return Some((i + 8) as u64);
        }
    }
    None
}

fn estimate_gif_size(data: &[u8]) -> Option<u64> {
    for i in (2..data.len() - 1).rev() {
        if data[i] == 0x00 && data[i + 1] == 0x3B {
            return Some((i + 2) as u64);
        }
    }
    None
}

fn estimate_bmp_size(data: &[u8]) -> Option<u64> {
    if data.len() >= 14 {
        // En BMP el tamaño total está en el offset 2 (4 bytes LE)
        let size = u32::from_le_bytes([data[2], data[3], data[4], data[5]]) as u64;
        // Validación básica: entre 100 bytes y 1GB
        if size > 100 && size < 1_000_000_000 {
            return Some(size);
        }
    }
    None
}

fn estimate_pdf_size(data: &[u8]) -> Option<u64> {
    // Buscar la marca %%EOF de atrás hacia adelante en los últimos 2048 bytes
    let start_search = if data.len() > 2048 { data.len() - 2048 } else { 0 };
    for i in (start_search..data.len().saturating_sub(5)).rev() {
        if &data[i..i + 5] == b"%%EOF" {
            return Some((i + 5) as u64);
        }
    }
    None
}

fn estimate_zip_size(data: &[u8]) -> Option<u64> {
    // Para archivos ZIP (y Office Open XML), buscamos el End of Central Directory Record (EOCD)
    // El EOCD empieza con 0x06054b50 y mide al menos 22 bytes.
    for i in (0..data.len().saturating_sub(22)).rev() {
        if data[i] == 0x50 && data[i + 1] == 0x4B && data[i + 2] == 0x05 && data[i + 3] == 0x06 {
            // Tamaño del Central Directory + Offset del Central Directory + Tamaño del EOCD
            let cd_size = u32::from_le_bytes([data[i + 12], data[i + 13], data[i + 14], data[i + 15]]) as u64;
            let cd_offset = u32::from_le_bytes([data[i + 16], data[i + 17], data[i + 18], data[i + 19]]) as u64;
            let comment_len = u16::from_le_bytes([data[i + 20], data[i + 21]]) as u64;
            
            let total_size = cd_offset + cd_size + 22 + comment_len;
            if total_size > 0 && total_size < 10_000_000_000 {
                return Some(total_size);
            }
        }
    }
    None
}

fn estimate_mp3_size(data: &[u8]) -> Option<u64> {
    for i in (4..data.len() - 3).rev() {
        if data[i] == 0xFF && (data[i + 1] & 0xE0) == 0xE0 {
            return Some((i + 1) as u64);
        }
    }
    None
}

fn estimate_mp4_size(data: &[u8]) -> Option<u64> {
    for i in (8..data.len() - 7).rev() {
        if &data[i..i + 4] == b"mdat" {
            let box_size =
                u32::from_be_bytes([data[i - 4], data[i - 3], data[i - 2], data[i - 1]]) as u64;
            if box_size > 0 {
                return Some(box_size);
            }
        }
    }
    None
}

/// Base de datos de firmas usando LazyLock
pub static SIGNATURE_DATABASE: LazyLock<Vec<FileSignature>> = LazyLock::new(|| {
    vec![
        // Imágenes
        FileSignature {
            file_type: FileType::Jpeg,
            magic_bytes: &[0xFF, 0xD8, 0xFF],
            offset: 0,
            min_size: 100,
            max_size: Some(500_000_000),
        },
        FileSignature {
            file_type: FileType::Png,
            magic_bytes: &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
            offset: 0,
            min_size: 100,
            max_size: Some(500_000_000),
        },
        FileSignature {
            file_type: FileType::Gif,
            magic_bytes: b"GIF89a",
            offset: 0,
            min_size: 100,
            max_size: Some(100_000_000),
        },
        FileSignature {
            file_type: FileType::Gif,
            magic_bytes: b"GIF87a",
            offset: 0,
            min_size: 100,
            max_size: Some(100_000_000),
        },
        FileSignature {
            file_type: FileType::Bmp,
            magic_bytes: &[0x42, 0x4D],
            offset: 0,
            min_size: 100,
            max_size: Some(1_000_000_000),
        },
        FileSignature {
            file_type: FileType::Tiff,
            magic_bytes: &[0x49, 0x49, 0x2A, 0x00],
            offset: 0,
            min_size: 100,
            max_size: Some(500_000_000),
        },
        FileSignature {
            file_type: FileType::Tiff,
            magic_bytes: &[0x4D, 0x4D, 0x00, 0x2A],
            offset: 0,
            min_size: 100,
            max_size: Some(500_000_000),
        },
        FileSignature {
            file_type: FileType::Webp,
            magic_bytes: b"RIFF",
            offset: 0,
            min_size: 100,
            max_size: Some(500_000_000),
        },
        FileSignature {
            file_type: FileType::Ico,
            magic_bytes: &[0x00, 0x00, 0x01, 0x00],
            offset: 0,
            min_size: 50,
            max_size: Some(10_000_000),
        },
        // Documentos
        FileSignature {
            file_type: FileType::Pdf,
            magic_bytes: &[0x25, 0x50, 0x44, 0x46],
            offset: 0,
            min_size: 100,
            max_size: Some(1_000_000_000),
        },
        FileSignature {
            file_type: FileType::Docx,
            magic_bytes: &[0x50, 0x4B, 0x03, 0x04],
            offset: 0,
            min_size: 100,
            max_size: Some(1_000_000_000),
        },
        FileSignature {
            file_type: FileType::Xlsx,
            magic_bytes: &[0x50, 0x4B, 0x03, 0x04],
            offset: 0,
            min_size: 100,
            max_size: Some(1_000_000_000),
        },
        FileSignature {
            file_type: FileType::Pptx,
            magic_bytes: &[0x50, 0x4B, 0x03, 0x04],
            offset: 0,
            min_size: 100,
            max_size: Some(1_000_000_000),
        },
        // Archivos comprimidos
        FileSignature {
            file_type: FileType::Zip,
            magic_bytes: &[0x50, 0x4B, 0x03, 0x04],
            offset: 0,
            min_size: 100,
            max_size: Some(10_000_000_000),
        },
        FileSignature {
            file_type: FileType::Zip,
            magic_bytes: &[0x50, 0x4B, 0x05, 0x06],
            offset: 0,
            min_size: 100,
            max_size: Some(10_000_000_000),
        },
        FileSignature {
            file_type: FileType::Rar,
            magic_bytes: &[0x52, 0x61, 0x72, 0x21, 0x1A, 0x07],
            offset: 0,
            min_size: 100,
            max_size: Some(10_000_000_000),
        },
        FileSignature {
            file_type: FileType::Rar,
            magic_bytes: &[0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, 0x00],
            offset: 0,
            min_size: 100,
            max_size: Some(10_000_000_000),
        },
        FileSignature {
            file_type: FileType::SevenZip,
            magic_bytes: &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C],
            offset: 0,
            min_size: 100,
            max_size: Some(10_000_000_000),
        },
        FileSignature {
            file_type: FileType::Tar,
            magic_bytes: &[0x75, 0x73, 0x74, 0x61, 0x72],
            offset: 257,
            min_size: 1024,
            max_size: Some(10_000_000_000),
        },
        FileSignature {
            file_type: FileType::Gzip,
            magic_bytes: &[0x1F, 0x8B],
            offset: 0,
            min_size: 100,
            max_size: Some(10_000_000_000),
        },
        // Audio
        FileSignature {
            file_type: FileType::Mp3,
            magic_bytes: &[0xFF, 0xFB],
            offset: 0,
            min_size: 100,
            max_size: Some(500_000_000),
        },
        FileSignature {
            file_type: FileType::Mp3,
            magic_bytes: &[0xFF, 0xF3],
            offset: 0,
            min_size: 100,
            max_size: Some(500_000_000),
        },
        FileSignature {
            file_type: FileType::Mp3,
            magic_bytes: &[0xFF, 0xF2],
            offset: 0,
            min_size: 100,
            max_size: Some(500_000_000),
        },
        FileSignature {
            file_type: FileType::Wav,
            magic_bytes: b"RIFF",
            offset: 0,
            min_size: 100,
            max_size: Some(2_000_000_000),
        },
        FileSignature {
            file_type: FileType::Flac,
            magic_bytes: &[0x66, 0x4C, 0x61, 0x43],
            offset: 0,
            min_size: 100,
            max_size: Some(2_000_000_000),
        },
        FileSignature {
            file_type: FileType::Ogg,
            magic_bytes: &[0x4F, 0x67, 0x67, 0x53],
            offset: 0,
            min_size: 100,
            max_size: Some(2_000_000_000),
        },
        // Video - Firmas específicas para evitar falsos positivos
        FileSignature {
            file_type: FileType::Mp4,
            magic_bytes: &[0x00, 0x00, 0x00, 0x1C, 0x66, 0x74, 0x79, 0x70],  // ft y isom
            offset: 4,
            min_size: 100,
            max_size: Some(50_000_000_000),
        },
        FileSignature {
            file_type: FileType::Mp4,
            magic_bytes: &[0x00, 0x00, 0x00, 0x18, 0x66, 0x74, 0x79, 0x70],  // ft isom
            offset: 4,
            min_size: 100,
            max_size: Some(50_000_000_000),
        },
        FileSignature {
            file_type: FileType::Mp4,
            magic_bytes: &[0x00, 0x00, 0x00, 0x14, 0x66, 0x74, 0x79, 0x70],  // ft isom
            offset: 4,
            min_size: 100,
            max_size: Some(50_000_000_000),
        },
        FileSignature {
            file_type: FileType::Avi,
            magic_bytes: b"RIFF",
            offset: 0,
            min_size: 100,
            max_size: Some(50_000_000_000),
        },
        FileSignature {
            file_type: FileType::MkV,
            magic_bytes: &[0x1A, 0x45, 0xDF, 0xA3],
            offset: 0,
            min_size: 100,
            max_size: Some(50_000_000_000),
        },
        FileSignature {
            file_type: FileType::Mov,
            magic_bytes: &[0x00, 0x00, 0x00, 0x14, 0x66, 0x74, 0x79, 0x70],  // qt   brand
            offset: 4,
            min_size: 100,
            max_size: Some(50_000_000_000),
        },
        FileSignature {
            file_type: FileType::Mov,
            magic_bytes: &[0x00, 0x00, 0x00, 0x18, 0x66, 0x74, 0x79, 0x70],  // qt   brand
            offset: 4,
            min_size: 100,
            max_size: Some(50_000_000_000),
        },
        FileSignature {
            file_type: FileType::Wmv,
            magic_bytes: &[0x30, 0x26, 0xB2, 0x75],
            offset: 0,
            min_size: 100,
            max_size: Some(50_000_000_000),
        },
        FileSignature {
            file_type: FileType::WebM,
            magic_bytes: &[0x1A, 0x45, 0xDF, 0xA3],
            offset: 0,
            min_size: 100,
            max_size: Some(50_000_000_000),
        },
        // Ejecutables
        FileSignature {
            file_type: FileType::Exe,
            magic_bytes: &[0x4D, 0x5A],
            offset: 0,
            min_size: 100,
            max_size: Some(2_000_000_000),
        },
        FileSignature {
            file_type: FileType::Dll,
            magic_bytes: &[0x4D, 0x5A],
            offset: 0,
            min_size: 100,
            max_size: Some(2_000_000_000),
        },
        FileSignature {
            file_type: FileType::Msi,
            magic_bytes: &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1],
            offset: 0,
            min_size: 100,
            max_size: Some(2_000_000_000),
        },
        // Más formatos de video
        FileSignature {
            file_type: FileType::Flv,
            magic_bytes: b"FLV",
            offset: 0,
            min_size: 100,
            max_size: Some(10_000_000_000),
        },
        // Más formatos de audio
        FileSignature {
            file_type: FileType::Wma,
            magic_bytes: &[0x30, 0x26, 0xB2, 0x75],
            offset: 0,
            min_size: 100,
            max_size: Some(1_000_000_000),
        },
        // RAW formatos de cámara
        FileSignature {
            file_type: FileType::Raw,
            magic_bytes: b"II",
            offset: 0,
            min_size: 100,
            max_size: Some(500_000_000),
        },
        FileSignature {
            file_type: FileType::Raw,
            magic_bytes: b"MM",
            offset: 0,
            min_size: 100,
            max_size: Some(500_000_000),
        },
    ]
});

/// Busca una firma en los datos dados
pub fn detect_file_type(data: &[u8]) -> Option<&'static FileSignature> {
    for sig in SIGNATURE_DATABASE.iter() {
        if sig.matches(data) {
            return Some(sig);
        }
    }
    None
}

/// Obtiene las categorías disponibles
pub fn get_categories() -> Vec<&'static str> {
    vec![
        "Imágenes",
        "Documentos",
        "Archivos",
        "Audio",
        "Video",
        "Ejecutables",
        "Otros",
    ]
}
