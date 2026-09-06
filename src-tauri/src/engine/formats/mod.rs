//! Formatos de imagen soportados y trait `FormatHandler`.
//!
//! La arquitectura es pluggable: cada formato implementa [`FormatHandler`] y se
//! registra en el `Engine`. Nuevos formatos se añaden sin tocar el core.

use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Formatos que BoxFlux sabe leer o escribir.
///
/// `repr(u8)` con discriminantes estables; la serialización JSON usa
/// strings legibles (`"Png"`, `"Jpeg"`, ...) vía los impl manuales de
/// Serialize/Deserialize de abajo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Format {
    Png = 0,
    Jpeg = 1,
    Webp = 2,
    Avif = 3,
    Gif = 4,
    Bmp = 5,
    Tiff = 6,
    Unknown = 255,
}

impl Format {
    /// Extensiones estándar para cada formato. La primera es la canónica
    /// usada al generar nombres de archivo de salida.
    pub fn extensions(&self) -> &'static [&'static str] {
        match self {
            Format::Png => &["png"],
            Format::Jpeg => &["jpg", "jpeg"],
            Format::Webp => &["webp"],
            Format::Avif => &["avif"],
            Format::Gif => &["gif"],
            Format::Bmp => &["bmp"],
            Format::Tiff => &["tif", "tiff"],
            Format::Unknown => &[],
        }
    }

    /// Detecta formato desde una extensión. Case-insensitive.
    pub fn from_extension(ext: &str) -> Self {
        let lower = ext.to_ascii_lowercase();
        let lower = lower.trim_start_matches('.');
        for f in [
            Format::Png,
            Format::Jpeg,
            Format::Webp,
            Format::Avif,
            Format::Gif,
            Format::Bmp,
            Format::Tiff,
        ] {
            if f.extensions().iter().any(|e| *e == lower) {
                return f;
            }
        }
        Format::Unknown
    }

    /// Detecta formato desde la extensión de un path.
    pub fn from_path(path: &Path) -> Self {
        path.extension()
            .and_then(|e| e.to_str())
            .map(Self::from_extension)
            .unwrap_or(Format::Unknown)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Format::Png => "PNG",
            Format::Jpeg => "JPEG",
            Format::Webp => "WebP",
            Format::Avif => "AVIF",
            Format::Gif => "GIF",
            Format::Bmp => "BMP",
            Format::Tiff => "TIFF",
            Format::Unknown => "UNKNOWN",
        }
    }

    /// Extensión canónica para generar archivos de salida.
    pub fn canonical_extension(&self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpeg => "jpg",
            Format::Webp => "webp",
            Format::Avif => "avif",
            Format::Gif => "gif",
            Format::Bmp => "bmp",
            Format::Tiff => "tif",
            Format::Unknown => "",
        }
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for Format {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "png" => Ok(Format::Png),
            "jpg" | "jpeg" => Ok(Format::Jpeg),
            "webp" => Ok(Format::Webp),
            "avif" => Ok(Format::Avif),
            "gif" => Ok(Format::Gif),
            "bmp" => Ok(Format::Bmp),
            "tif" | "tiff" => Ok(Format::Tiff),
            _ => Err(format!("formato desconocido: {s}")),
        }
    }
}

impl Serialize for Format {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Format {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse::<Format>().map_err(serde::de::Error::custom)
    }
}

/// Qué puede hacer un handler particular.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct FormatCapabilities {
    pub can_read: bool,
    pub can_write: bool,
    pub can_optimize: bool,
    pub can_convert: bool,
}

/// Información ligera retornada por [`FormatHandler::analyze`] sin decodificar
/// los píxeles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatInfo {
    pub format: Format,
    pub width: u32,
    pub height: u32,
    pub file_size: u64,
    pub has_alpha: bool,
    pub color_type: String,
    pub bit_depth: u8,
}

mod avif;
mod jpeg;
mod other;
mod png;
mod webp;

pub use avif::AvifHandler;
pub use jpeg::JpegHandler;
pub use other::{BmpHandler, GifHandler, TiffHandler};
pub use png::PngHandler;
pub use webp::WebpHandler;

/// Trait pluggable para handlers de formato.
///
/// Las implementaciones viven en submódulos y se componen en el engine.
pub trait FormatHandler: Send + Sync {
    fn format(&self) -> Format;
    fn capabilities(&self) -> FormatCapabilities;

    /// Analiza un archivo en disco. NO debe decodificar píxeles.
    fn analyze(&self, path: &Path) -> Result<FormatInfo, FormatError>;

    /// Optimiza un archivo. Escribe en `output_path`.
    fn optimize(
        &self,
        input_path: &Path,
        output_path: &Path,
        profile: &crate::engine::optimization::OptimizationProfile,
    ) -> Result<crate::engine::optimization::OptimizationResult, FormatError>;

    /// Convierte un archivo de un formato a otro.
    fn convert(
        &self,
        input_path: &Path,
        output_path: &Path,
        target_format: Format,
        profile: &crate::engine::optimization::OptimizationProfile,
    ) -> Result<crate::engine::conversion::ConversionResult, FormatError>;
}

/// Errores de handlers de formato.
#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("formato no soportado: {0}")]
    Unsupported(String),
    #[error("no se puede leer el formato {0}")]
    CannotRead(Format),
    #[error("no se puede escribir el formato {0}")]
    CannotWrite(Format),
    #[error("no se puede optimizar el formato {0}")]
    CannotOptimize(Format),
    #[error("archivo no encontrado: {0}")]
    NotFound(String),
    #[error("la ruta de salida no debe ser igual a la de entrada: {0}")]
    OutputSameAsInput(String),
    #[error("permiso denegado: {0}")]
    PermissionDenied(String),
    #[error("archivo corrupto o inválido: {0}")]
    Corrupted(String),
    #[error("fallo del decoder: {0}")]
    DecoderFailure(String),
    #[error("fallo del encoder: {0}")]
    EncoderFailure(String),
    #[error("error de IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("error de imagen: {0}")]
    Image(#[from] image::ImageError),
    #[error("cancelado por el llamador")]
    Cancelled,
    #[error("otro: {0}")]
    Other(String),
}

impl FormatError {
    /// Categoría del error para el frontend (mapeo a códigos de UI).
    pub fn category(&self) -> &'static str {
        match self {
            FormatError::Unsupported(_) => "unsupported",
            FormatError::CannotRead(_) => "cannot_read",
            FormatError::CannotWrite(_) => "cannot_write",
            FormatError::CannotOptimize(_) => "cannot_optimize",
            FormatError::NotFound(_) => "not_found",
            FormatError::OutputSameAsInput(_) => "output_same_as_input",
            FormatError::PermissionDenied(_) => "permission_denied",
            FormatError::Corrupted(_) => "corrupted",
            FormatError::DecoderFailure(_) => "decoder_failure",
            FormatError::EncoderFailure(_) => "encoder_failure",
            FormatError::Io(_) => "io",
            FormatError::Image(_) => "image",
            FormatError::Cancelled => "cancelled",
            FormatError::Other(_) => "other",
        }
    }
}

impl Serialize for FormatError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("FormatError", 2)?;
        st.serialize_field("category", self.category())?;
        st.serialize_field("message", &self.to_string())?;
        st.end()
    }
}
