//! PngBackend: adaptador sobre [`crate::engine::formats::PngHandler`],
//! que usa `oxipng` para recompresión lossless.
//!
//! Sin lógica PNG propia: solo traduce entre la abstracción
//! [`Candidate`] del motor y el [`OptimizationProfile`] del handler.

use std::path::Path;
use std::time::Instant;

use crate::engine::formats::{Format, FormatCapabilities, FormatHandler, PngHandler};
use crate::engine::optimization::{MetadataMode, OptimizationProfile, ProfileKind};

use crate::engine::intel::backend::{BackendError, BackendResult, FormatBackend};
use crate::engine::intel::candidate::Candidate;
use crate::engine::intel::profile::FileProfile;

pub struct PngBackend {
    handler: PngHandler,
}

impl PngBackend {
    pub fn new() -> Self {
        Self {
            handler: PngHandler::new(),
        }
    }

    /// El pipeline fija `candidate.backend_time_budget_ms` (fracción
    /// determinista del presupuesto del goal). Ese valor dimensiona las
    /// ITERACIONES de zopfli (modelo determinista por tamaño raw) en
    /// vez de un timeout de reloj de pared: el resultado no debe
    /// depender de la carga del sistema, y un zopfli largo no debe
    /// comerse el presupuesto y matar de hambre al resto de candidatos.
    fn handler_for(&self, candidate: &Candidate) -> PngHandler {
        if candidate.backend_time_budget_ms > 0 {
            PngHandler::with_zopfli_budget(std::time::Duration::from_millis(
                candidate.backend_time_budget_ms,
            ))
        } else {
            PngHandler::new()
        }
    }

    fn build_profile(&self, candidate: &Candidate) -> OptimizationProfile {
        // Cap por memoria: con `max_memory_mb` configurado, el level
        // baja como máximo a 5 — zopfli en imágenes grandes puede
        // consumir varios cientos de MB. Es lo que da efecto real al
        // ajuste de memoria.
        let mem_constrained = candidate.memory_budget_mb > 0 && candidate.memory_budget_mb < 1024;
        let mut p = OptimizationProfile {
            name: candidate.label.clone(),
            kind: ProfileKind::Custom,
            jpeg_quality: 82,
            jpeg_progressive: true,
            webp_quality: 80,
            webp_lossless: false,
            avif_quality: 60,
            avif_alpha_quality: 80,
            avif_time_budget_ms: 0,
            // PNG level según el effort del candidato:
            // < 0.5 (Balanced, Web) → 4; 0.5-0.8 (Quality, Lossless)
            // → 5; ≥ 0.8 (MaximumCompression) → 6 (zopfli).
            png_optimization_level: if candidate.estimated_effort >= 0.8 && !mem_constrained {
                6 // zopfli + all filters + all reductions
            } else if candidate.estimated_effort >= 0.5 {
                5 // libdeflater-12 + all filters + full eval
            } else {
                4 // libdeflater-12 + standard filter set
            },
            metadata_mode: candidate.metadata_mode,
            preserve_color_profile: candidate.preserve_color_profile,
            jpeg_chroma_444: true,
            resize: candidate.resize,
        };
        // PNG no tiene quality: el flag lossless es el único mando.
        let _ = candidate.quality;
        // Si el usuario pidió eliminar toda la metadata, no conservar
        // el perfil de color: consistente con `MetadataMode::RemoveAll`
        // (oxipng `StripChunks::All` elimina los chunks ICC).
        if matches!(candidate.metadata_mode, MetadataMode::RemoveAll) {
            p.preserve_color_profile = false;
        }
        p.metadata_mode = match candidate.metadata_mode {
            MetadataMode::Keep => MetadataMode::Keep,
            MetadataMode::RemoveSafe => MetadataMode::RemoveSafe,
            MetadataMode::RemoveAll => MetadataMode::RemoveAll,
        };
        p
    }
}

impl Default for PngBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatBackend for PngBackend {
    fn format(&self) -> Format {
        Format::Png
    }

    fn name(&self) -> &str {
        "PngBackend (oxipng 9.1)"
    }

    fn capabilities(&self) -> FormatCapabilities {
        self.handler.capabilities()
    }

    fn can_handle(&self, profile: &FileProfile, candidate: &Candidate) -> bool {
        // PNG puede producirse desde cualquier fuente legible.
        let _ = profile;
        candidate.format == Format::Png
    }

    fn process(
        &self,
        input: &Path,
        output: &Path,
        candidate: &Candidate,
        _profile: &FileProfile,
    ) -> Result<BackendResult, BackendError> {
        let start = Instant::now();
        let p = self.build_profile(candidate);
        let handler = self.handler_for(candidate);
        let result = if _profile.format == Format::Png {
            handler.optimize(input, output, &p)?
        } else {
            // oxipng solo acepta PNG de entrada: los candidatos PNG
            // cruzados van por la ruta de conversión del handler.
            let converted = self.handler.convert(input, output, Format::Png, &p)?;
            crate::engine::optimization::OptimizationResult {
                original_size: converted.original_size,
                output_size: converted.output_size,
                bytes_saved: converted.bytes_saved,
                percentage_saved: converted.percentage_saved,
                processing_time_ms: converted.processing_time_ms,
                format: Format::Png,
                success: converted.success,
                error: converted.error,
            }
        };
        Ok(BackendResult {
            output_path: output.to_path_buf(),
            output_size: result.output_size,
            processing_time_ms: start.elapsed().as_millis() as u64,
            lossless: true, // PNG es siempre lossless
        })
    }
}
