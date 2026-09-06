//! BatchProcessor + JobScheduler: aplican el pipeline de optimización
//! a muchos archivos con concurrencia controlada.
//!
//! El scheduler limita los workers a `available_parallelism()` por
//! defecto. Cada worker ejecuta el pipeline completo de forma
//! independiente y los resultados se recogen en orden de entrada.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use super::decision::OptimizationDecision;
use super::goal::{OptimizationGoal, UserConstraints};
use super::pipeline::{OptimizationPipeline, PipelineConfig};

/// Resultado de un job del batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResult {
    pub input_path: PathBuf,
    pub decision: OptimizationDecision,
}

impl BatchResult {
    pub fn bytes_saved(&self) -> u64 {
        self.decision
            .original_size
            .saturating_sub(self.decision.final_size)
    }
    pub fn success(&self) -> bool {
        self.decision.selected.is_some() || self.decision.kept_original
    }
}

/// Estadísticas agregadas del batch (atómicas: varios workers).
#[derive(Debug, Default)]
pub struct BatchStats {
    pub total_files: AtomicU64,
    pub succeeded: AtomicU64,
    pub failed: AtomicU64,
    pub kept_original: AtomicU64,
    pub total_original_bytes: AtomicU64,
    pub total_final_bytes: AtomicU64,
    pub total_processing_ms: AtomicU64,
}

impl BatchStats {
    pub fn snapshot(&self) -> BatchStatsSnapshot {
        BatchStatsSnapshot {
            total_files: self.total_files.load(Ordering::Relaxed),
            succeeded: self.succeeded.load(Ordering::Relaxed),
            failed: self.failed.load(Ordering::Relaxed),
            kept_original: self.kept_original.load(Ordering::Relaxed),
            total_original_bytes: self.total_original_bytes.load(Ordering::Relaxed),
            total_final_bytes: self.total_final_bytes.load(Ordering::Relaxed),
            total_processing_ms: self.total_processing_ms.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct BatchStatsSnapshot {
    pub total_files: u64,
    pub succeeded: u64,
    pub failed: u64,
    pub kept_original: u64,
    pub total_original_bytes: u64,
    pub total_final_bytes: u64,
    pub total_processing_ms: u64,
}

/// Controla la concurrencia del batch: número máximo de workers.
pub struct JobScheduler {
    max_workers: usize,
}

impl JobScheduler {
    pub fn new(max_workers: usize) -> Self {
        let max_workers = if max_workers == 0 {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
        } else {
            max_workers
        };
        Self { max_workers }
    }

    pub fn max_workers(&self) -> usize {
        self.max_workers
    }
}

/// El procesador de batch. Posee la configuración del pipeline.
pub struct BatchProcessor {
    pipeline: Arc<OptimizationPipeline>,
    scheduler: JobScheduler,
    stats: Arc<BatchStats>,
}

impl BatchProcessor {
    pub fn new(config: PipelineConfig, max_workers: usize) -> Self {
        Self {
            pipeline: Arc::new(OptimizationPipeline::new(config)),
            scheduler: JobScheduler::new(max_workers),
            stats: Arc::new(BatchStats::default()),
        }
    }

    pub fn stats(&self) -> Arc<BatchStats> {
        Arc::clone(&self.stats)
    }

    pub fn max_workers(&self) -> usize {
        self.scheduler.max_workers()
    }

    /// Procesa un batch de archivos. Devuelve resultados en orden de
    /// entrada.
    pub fn process_batch(
        &self,
        inputs: Vec<PathBuf>,
        output_dir: &Path,
        goal: OptimizationGoal,
        constraints: &UserConstraints,
    ) -> Vec<BatchResult> {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(self.scheduler.max_workers())
            .build()
            .unwrap_or_else(|_| rayon::ThreadPoolBuilder::new().build().unwrap());

        let goal = Arc::new(goal);
        let constraints = Arc::new(constraints.clone());
        let output_dir = Arc::new(output_dir.to_path_buf());
        let stats = Arc::clone(&self.stats);
        let pipeline = Arc::clone(&self.pipeline);

        pool.install(move || {
            inputs
                .into_par_iter()
                .map(|input| {
                    let file_out_dir = output_dir.join(format!(
                        "job_{}",
                        input
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown")
                            .replace('.', "_")
                    ));
                    let decision =
                        pipeline.optimize(&input, &file_out_dir, (*goal).clone(), &constraints);
                    stats.total_files.fetch_add(1, Ordering::Relaxed);
                    stats
                        .total_original_bytes
                        .fetch_add(decision.original_size, Ordering::Relaxed);
                    stats
                        .total_final_bytes
                        .fetch_add(decision.final_size, Ordering::Relaxed);
                    if decision.kept_original {
                        stats.kept_original.fetch_add(1, Ordering::Relaxed);
                    } else if decision.selected.is_some() {
                        stats.succeeded.fetch_add(1, Ordering::Relaxed);
                    } else {
                        stats.failed.fetch_add(1, Ordering::Relaxed);
                    }
                    BatchResult {
                        input_path: input,
                        decision,
                    }
                })
                .collect()
        })
    }
}
