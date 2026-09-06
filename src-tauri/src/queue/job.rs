//! Job types.

use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::engine::formats::Format;
use crate::engine::optimization::ResizeOptions;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum JobStatus {
    Queued,
    Processing,
    Completed,
    Failed,
    Cancelled,
    Skipped,
}

impl Default for JobStatus {
    fn default() -> Self {
        Self::Queued
    }
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "Queued",
            Self::Processing => "Processing",
            Self::Completed => "Completed",
            Self::Failed => "Failed",
            Self::Cancelled => "Cancelled",
            Self::Skipped => "Skipped",
        }
    }
}

/// Operación que el motor inteligente debe realizar.
///
/// Simplificado a 2 modos principales:
/// - `Optimize`: el motor analiza la imagen y elige el mejor formato +
///   calidad según el goal. **No usa un perfil predestinado.**
/// - `Resize`: redimensionar manteniendo formato (operación auxiliar).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum JobOperation {
    /// El motor inteligente analiza la imagen y decide qué optimización
    /// aplicar (formato + calidad) según el `optimization_goal` del job.
    Optimize,
    /// Redimensionar manteniendo el formato original.
    Resize,
}

impl Default for JobOperation {
    fn default() -> Self {
        Self::Optimize
    }
}

/// Goal del usuario.
///
/// - `Quality`: el motor busca la mejor calidad posible intentando
///   que el archivo pese menos. Prefiere lossless o lossy de alta
///   calidad (SSIM ≥ 0.95).
/// - `Compress`: el motor busca la compresión más óptima según la
///   imagen. Puede elegir AVIF q40 si la imagen lo permite, o WebP
///   q70 si AVIF no conviene.
/// - `ExtremeLightweight`: búsqueda iterativa guiada por butteraugli
///   para encontrar el punto exacto donde la imagen empieza a
///   degradarse. El archivo más pequeño que sigue siendo visualmente
///   idéntico al original.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub enum OptimizationMode {
    /// Calidad: prioriza mantener la calidad visual alta, intentando
    /// que el archivo pese menos. El motor elegirá lossless si está
    /// cerca del lossy en tamaño; si no, lossy con SSIM ≥ 0.95.
    #[default]
    Quality,
    /// Comprimir: el motor busca la compresión más óptima según la
    /// imagen. Evalúa AVIF, WebP y JPEG en varios niveles de calidad
    /// y elige el que dé el mejor balance según el análisis.
    Compress,
    /// Extremadamente ligero manteniendo calidad: búsqueda iterativa
    /// con butteraugli para encontrar el quality óptimo por imagen.
    ExtremeLightweight,
}

impl OptimizationMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Quality => "Quality",
            Self::Compress => "Compress",
            Self::ExtremeLightweight => "ExtremeLightweight",
        }
    }

    /// Convierte al `OptimizationGoal` interno del pipeline inteligente.
    pub fn to_pipeline_goal(self) -> crate::engine::intel::OptimizationGoal {
        match self {
            Self::Quality => crate::engine::intel::OptimizationGoal::Quality,
            // Compress usa MaximumCompression para priorizar tamaño,
            // pero el motor analiza la imagen y puede elegir calidad
            // media si la imagen lo permite (no es un preset fijo).
            Self::Compress => crate::engine::intel::OptimizationGoal::MaximumCompression,
            // Búsqueda iterativa guiada por butteraugli.
            Self::ExtremeLightweight => crate::engine::intel::OptimizationGoal::ExtremeLightweight,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: u64,
    pub operation: JobOperation,
    pub source_path: String,
    pub destination_path: String,
    pub input_format: Format,
    pub output_format: Format,
    /// Modo de optimización: Quality o Compress.
    /// El motor inteligente usa este modo para decidir el goal interno.
    pub optimization_mode: OptimizationMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resize: Option<ResizeOptions>,
    pub original_size: u64,
    pub final_size: u64,
    pub progress: u8,
    pub status: JobStatus,
    pub processing_time_ms: u64,
    /// Formato seleccionado por el motor inteligente (información
    /// para mostrar al usuario qué decidió el motor).
    #[serde(default)]
    pub selected_format: String,
    /// Calidad seleccionada por el motor (p.ej. "q80", "lossless").
    #[serde(default)]
    pub selected_quality: String,
    /// Explicación humana de por qué el motor eligió esta salida.
    #[serde(default)]
    pub decision_explanation: String,
    #[serde(default)]
    pub error: String,
    /// Formato de salida forzado por el usuario (modo Manual).
    ///
    /// Si es `None`, el motor decide el formato automáticamente (modo Auto).
    /// Si es `Some(format)`, el motor restringe los candidatos a ese formato
    /// y no realiza conversiones cruzadas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub force_format: Option<Format>,
    /// Si es `true`, fuerza salida lossless sin importar el goal.
    #[serde(default)]
    pub force_lossless: bool,
    /// Si es `true`, elimina toda la metadata (EXIF, XMP, ICC).
    #[serde(default)]
    pub strip_all_metadata: bool,
    /// Si es `true`, preserva el perfil de color ICC del original.
    /// Por defecto `true` para evitar alteraciones en tonos oscuros.
    #[serde(default = "default_preserve_color_profile")]
    pub preserve_color_profile: bool,
    // Internal timing — not serialized.
    #[serde(skip)]
    pub queued_at: Option<Instant>,
    #[serde(skip)]
    pub started_at: Option<Instant>,
    #[serde(skip)]
    pub finished_at: Option<Instant>,
}

fn default_preserve_color_profile() -> bool {
    true
}

impl Default for Job {
    fn default() -> Self {
        Self {
            id: 0,
            operation: JobOperation::Optimize,
            source_path: String::new(),
            destination_path: String::new(),
            input_format: Format::Unknown,
            output_format: Format::Unknown,
            optimization_mode: OptimizationMode::Quality,
            resize: None,
            original_size: 0,
            final_size: 0,
            progress: 0,
            status: JobStatus::Queued,
            processing_time_ms: 0,
            selected_format: String::new(),
            selected_quality: String::new(),
            decision_explanation: String::new(),
            error: String::new(),
            force_format: None,
            force_lossless: false,
            strip_all_metadata: false,
            preserve_color_profile: true,
            queued_at: None,
            started_at: None,
            finished_at: None,
        }
    }
}

impl Job {
    pub fn percentage_saved(&self) -> f64 {
        if self.original_size == 0 {
            return 0.0;
        }
        100.0 * (self.original_size as f64 - self.final_size as f64) / self.original_size as f64
    }
}
