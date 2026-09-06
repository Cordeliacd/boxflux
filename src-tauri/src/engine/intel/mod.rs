//! Capa de decisión de BoxFlux sobre los códecs base (oxipng, mozjpeg,
//! webp, ravif, crate `image`): los códecs ejecutan, este módulo decide.
//! Analiza la imagen, planifica la estrategia, genera candidatos, los
//! pasa por el pipeline de backends y puntúa los resultados para elegir
//! el mejor.

pub mod analyzer;
pub mod backend;
pub mod backends;
pub mod batch;
pub mod cancel;
pub mod candidate;
pub mod decision;
pub mod encode_cache;
pub mod goal;
pub mod goal_parser;
pub mod heuristics;
pub mod iterative;
pub mod metrics;
pub mod original_cache;
pub mod output_safety;
pub mod parameter;
pub mod pipeline;
pub mod profile;
pub mod quality;
pub mod scoring;
pub mod strategy;

pub use backend::{BackendError, BackendResult, FormatBackend};
pub use batch::{BatchProcessor, BatchResult, JobScheduler};
pub use cancel::{CancelHandle, CancellationToken, Cancelled};
pub use candidate::{
    Candidate, CandidateGenerator, CandidateId, CandidateResult, RejectedCandidate,
};
pub use decision::OptimizationDecision;
pub use encode_cache::{avif_encode_key, DedupLookup, EncodeDedupCache};
pub use goal::{GoalWeights, OptimizationGoal, UserConstraints};
pub use goal_parser::parse_goal_prompt;
pub use heuristics::OptimizationHeuristics;
pub use iterative::{iterative_quality_search, IterativeSearchResult};
pub use metrics::ResultMetrics;
pub use original_cache::OriginalImageCache;
pub use output_safety::{
    ensure_distinct, publish_atomic, temporary_path_for, with_temp_output, OutputSafetyError,
    PublishedFile,
};
pub use parameter::OptimizationParameter;
pub use pipeline::{OptimizationPipeline, PipelineConfig};
pub use profile::{ColorModel, FileProfile, ImageCategory};
pub use quality::{QualityEvaluator, QualityMetrics};
pub use scoring::CandidateScoringEngine;
pub use strategy::{OptimizationStrategy, QualityRange, StrategyEngine};

pub use crate::engine::formats::Format;
pub use crate::engine::optimization::{MetadataMode, ResizeMode, ResizeOptions};

/// Versión del motor de inteligencia. Subir cuando un cambio en
/// heurísticas o scoring afecte a la selección del resultado final.
pub const INTEL_ENGINE_VERSION: &str = "1.1.0";
