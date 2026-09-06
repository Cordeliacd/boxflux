//! FormatBackend: la abstracción limpia sobre las librerías de códecs.
//!
//! El motor de inteligencia se comunica con los códecs SOLO a través
//! de este trait; los adaptadores de [`super::backends`] envuelven los
//! handlers de `formats::*`. Esto mantiene el motor agnóstico del
//! codec: cambiar el encoder de un formato no toca el motor.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::engine::formats::Format;
use crate::engine::formats::FormatCapabilities;

use super::candidate::Candidate;
use super::profile::FileProfile;

/// Resultado que devuelve un backend tras procesar un candidato.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendResult {
    /// Ruta del archivo de salida producido.
    pub output_path: PathBuf,
    /// Tamaño medido de la salida, en bytes.
    pub output_size: u64,
    /// Tiempo de proceso (wall-clock), en ms.
    pub processing_time_ms: u64,
    /// Si el backend considera la salida lossless respecto a la entrada.
    pub lossless: bool,
}

/// Errores que puede devolver un backend. El pipeline los trata como
/// fallos de candidato, no de la aplicación: el motor sigue probando
/// otros candidatos.
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("backend cannot handle candidate: {0}")]
    CannotHandle(String),
    #[error("input file not found: {0}")]
    NotFound(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("decoder failure: {0}")]
    DecoderFailure(String),
    #[error("encoder failure: {0}")]
    EncoderFailure(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("image error: {0}")]
    Image(#[from] image::ImageError),
    #[error("cancelled")]
    Cancelled,
    #[error("other: {0}")]
    Other(String),
}

impl From<crate::engine::formats::FormatError> for BackendError {
    fn from(e: crate::engine::formats::FormatError) -> Self {
        use crate::engine::formats::FormatError as E;
        match e {
            E::NotFound(s) => BackendError::NotFound(s),
            E::PermissionDenied(s) => BackendError::PermissionDenied(s),
            E::Corrupted(s) => BackendError::DecoderFailure(s),
            E::DecoderFailure(s) => BackendError::DecoderFailure(s),
            E::EncoderFailure(s) => BackendError::EncoderFailure(s),
            E::Cancelled => BackendError::Cancelled,
            other => BackendError::Other(other.to_string()),
        }
    }
}

/// Trait que implementa cada adaptador de codec.
pub trait FormatBackend: Send + Sync {
    /// Formato que produce este backend.
    fn format(&self) -> Format;

    /// Nombre legible del backend, p.ej. "PngBackend (oxipng 9.1)".
    fn name(&self) -> &str;

    fn capabilities(&self) -> FormatCapabilities;

    /// Devuelve `true` si este backend puede con la combinación
    /// (profile, candidate). El pipeline lo llama antes de `process`
    /// para saltarse backends no aplicables sin spawnear trabajo.
    fn can_handle(&self, profile: &FileProfile, candidate: &Candidate) -> bool;

    /// Ejecuta el candidato: escribe el archivo de salida en
    /// `output_path`.
    ///
    /// Las implementaciones deben:
    /// 1. Validar las entradas (`CannotHandle` si no aplica)
    /// 2. Decodificar la fuente con la librería subyacente
    /// 3. Aplicar resize / operaciones de metadata
    /// 4. Encodar al formato destino con la calidad pedida
    /// 5. Devolver el `BackendResult` medido
    ///
    /// Los errores van como `BackendError`: el pipeline marca el
    /// candidato como fallido y continúa.
    fn process(
        &self,
        input: &Path,
        output: &Path,
        candidate: &Candidate,
        profile: &FileProfile,
    ) -> Result<BackendResult, BackendError>;
}
