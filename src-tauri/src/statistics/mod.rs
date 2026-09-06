//! Agrega el estado live del `JobQueue` en `LiveStats` para la UI.
//! La cola es la única fuente de la verdad; el `Engine` solo aporta
//! `max_workers`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::engine::Engine;
use crate::queue::JobQueue;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveStats {
    pub files_processed: u64,
    pub files_failed: u64,
    pub bytes_processed_in: u64,
    pub bytes_processed_out: u64,
    pub total_processing_ms: u64,
    pub files_queued: u64,
    pub files_processing: u64,
    pub active_workers: u32,
    pub max_workers: u32,
    pub space_saved_bytes: i64,
    pub compression_percentage: f64,
    pub processing_speed_files_per_sec: f64,
}

pub struct StatisticsManager {
    engine: Arc<Engine>,
    queue: Arc<JobQueue>,
}

impl StatisticsManager {
    pub fn new(engine: Arc<Engine>, queue: Arc<JobQueue>) -> Self {
        Self { engine, queue }
    }

    pub fn snapshot(&self) -> LiveStats {
        // La cola es la única fuente de la verdad: completed/failed son
        // archivos, original/final_bytes son bytes de entrada/salida y
        // total_processing_ms es el tiempo acumulado.
        let queue_stats = self.queue.stats();

        let files_processed = queue_stats.completed;
        let files_failed = queue_stats.failed;
        let bytes_processed_in = queue_stats.original_bytes;
        let bytes_processed_out = queue_stats.final_bytes;
        let total_processing_ms = queue_stats.total_processing_ms;

        let space_saved_bytes = bytes_processed_in as i64 - bytes_processed_out as i64;
        let compression_percentage = if bytes_processed_in == 0 {
            0.0
        } else {
            100.0 * (bytes_processed_in as f64 - bytes_processed_out as f64)
                / bytes_processed_in as f64
        };
        let processing_speed_files_per_sec = if total_processing_ms == 0 {
            0.0
        } else {
            files_processed as f64 / (total_processing_ms as f64 / 1000.0)
        };

        LiveStats {
            files_processed,
            files_failed,
            bytes_processed_in,
            bytes_processed_out,
            total_processing_ms,
            files_queued: queue_stats.queued,
            files_processing: queue_stats.processing,
            active_workers: queue_stats.active_workers,
            max_workers: self.engine.max_workers() as u32,
            space_saved_bytes,
            compression_percentage,
            processing_speed_files_per_sec,
        }
    }

    /// Resumen de batch para mostrar en UI (formato texto multi-línea).
    pub fn format_batch_summary(&self) -> String {
        let s = self.snapshot();
        let saved_label = if s.space_saved_bytes < 0 {
            "Aumentado"
        } else {
            "Ahorrado"
        };
        let saved_abs = s.space_saved_bytes.unsigned_abs();
        format!(
            "Archivos: {}\nOriginal: {}\nOptimizado: {}\n{}: {} ({:.1}%)\nTiempo de procesamiento: {}",
            s.files_processed,
            crate::util::format::format_bytes(s.bytes_processed_in),
            crate::util::format::format_bytes(s.bytes_processed_out),
            saved_label,
            crate::util::format::format_bytes(saved_abs),
            s.compression_percentage,
            crate::util::format::format_duration(s.total_processing_ms),
        )
    }
}
