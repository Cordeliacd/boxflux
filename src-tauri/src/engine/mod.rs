//! Motor de optimización de BoxFlux.
//!
//! `formats/` abstrae los codecs, `optimization/` y `conversion/` definen
//! los resultados, `intel/` decide qué aplicar a cada imagen.

pub mod conversion;
pub mod formats;
pub mod intel;
pub mod metadata;
pub mod optimization;

// Re-exports para uso interno de los commands.
pub use conversion::ConversionResult;
pub use formats::{Format, FormatCapabilities, FormatError, FormatHandler, FormatInfo};
pub use optimization::{
    MetadataMode, OptimizationProfile, OptimizationResult, ResizeMode, ResizeOptions,
};

pub use intel::{
    BackendError, BackendResult, BatchProcessor, BatchResult, CancelHandle, CancellationToken,
    Cancelled, Candidate, CandidateGenerator, CandidateId, CandidateResult, FileProfile,
    FormatBackend, GoalWeights, JobScheduler, OptimizationDecision, OptimizationGoal,
    OptimizationHeuristics, OptimizationPipeline, OptimizationStrategy, OriginalImageCache,
    OutputSafetyError, PipelineConfig, PublishedFile, QualityEvaluator, QualityMetrics,
    RejectedCandidate, ResultMetrics, StrategyEngine, INTEL_ENGINE_VERSION,
};

/// Versión de la crate, expuesta al frontend vía `get_engine_info`.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

use serde::{Deserialize, Serialize};

use crate::engine::formats::{
    AvifHandler, BmpHandler, Format, FormatHandler, GifHandler, JpegHandler, PngHandler,
    TiffHandler, WebpHandler,
};

/// Configuración del engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    /// Número máximo de workers concurrentes para batch processing.
    /// 0 = usar `available_parallelism`.
    #[serde(default)]
    pub max_workers: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self { max_workers: 0 }
    }
}

pub struct Engine {
    handlers: Vec<Box<dyn FormatHandler>>,
    config: EngineConfig,
}

impl Engine {
    pub fn new(config: EngineConfig) -> Self {
        let handlers: Vec<Box<dyn FormatHandler>> = vec![
            Box::new(PngHandler::new()),
            Box::new(JpegHandler::new()),
            Box::new(WebpHandler::new()),
            Box::new(AvifHandler::new()),
            Box::new(GifHandler::new()),
            Box::new(BmpHandler::new()),
            Box::new(TiffHandler::new()),
        ];
        Self { handlers, config }
    }

    pub fn version() -> &'static str {
        crate::engine::ENGINE_VERSION
    }

    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    pub fn max_workers(&self) -> usize {
        if self.config.max_workers == 0 {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
        } else {
            self.config.max_workers
        }
    }

    /// Lista los format handlers cargados y sus capacidades.
    pub fn list_formats(&self) -> Vec<(Format, crate::engine::formats::FormatCapabilities)> {
        self.handlers
            .iter()
            .map(|h| (h.format(), h.capabilities()))
            .collect()
    }
}
