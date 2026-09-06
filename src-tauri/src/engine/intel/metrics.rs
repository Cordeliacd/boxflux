//! ResultMetrics: el resultado MEDIDO de un candidato (nunca
//! inventado).

use super::quality::QualityMetrics;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResultMetrics {
    pub original_size: u64,
    pub output_size: u64,
    pub bytes_saved: u64,
    /// Porcentaje ahorrado, 0.0..=100.0. Negativo si la salida crece.
    pub percentage_saved: f64,
    /// output_size / original_size. <1.0 = comprimido, >1.0 = creció.
    pub compression_ratio: f64,
    pub processing_time_ms: u64,
    /// Bytes por segundo de input procesado.
    pub throughput_mbps: f64,
    /// Métricas de calidad (None para candidatos lossless).
    pub quality: Option<QualityMetrics>,
}

impl ResultMetrics {
    pub fn from_sizes(
        original: u64,
        output: u64,
        time_ms: u64,
        quality: Option<QualityMetrics>,
    ) -> Self {
        let bytes_saved = original.saturating_sub(output);
        let percentage_saved = if original == 0 {
            0.0
        } else {
            // `bytes_saved` es u64 y no puede ser negativo, pero el
            // porcentaje sí debe reflejar el crecimiento para que el
            // scorer pueda penalizar candidatos mayores que la entrada.
            100.0 * (original as f64 - output as f64) / original as f64
        };
        let compression_ratio = if original == 0 {
            1.0
        } else {
            output as f64 / original as f64
        };
        let throughput_mbps = if time_ms == 0 {
            0.0
        } else {
            (original as f64 / 1_048_576.0) / (time_ms as f64 / 1000.0)
        };
        Self {
            original_size: original,
            output_size: output,
            bytes_saved,
            percentage_saved,
            compression_ratio,
            processing_time_ms: time_ms,
            throughput_mbps,
            quality,
        }
    }

    /// ¿Salida idéntica byte a byte a la entrada (lossless)?
    pub fn is_lossless(&self) -> bool {
        self.quality
            .as_ref()
            .map(|q| q.is_lossless)
            .unwrap_or(false)
    }
}
