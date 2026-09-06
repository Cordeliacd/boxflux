//! AvifBackend: adaptador sobre [`crate::engine::formats::AvifHandler`],
//! que usa `ravif` / `rav1e`.

use std::path::Path;
use std::time::Instant;

use crate::engine::formats::{AvifHandler, Format, FormatCapabilities, FormatHandler};
use crate::engine::optimization::{OptimizationProfile, ProfileKind};

use crate::engine::intel::backend::{BackendError, BackendResult, FormatBackend};
use crate::engine::intel::candidate::Candidate;
use crate::engine::intel::profile::FileProfile;

pub struct AvifBackend {
    handler: AvifHandler,
}

/// Perfil de encode AVIF de un candidato: función libre y compartida
/// — la usa `AvifBackend::process` para codificar y el pipeline para
/// construir la clave de dedupe
/// ([`crate::engine::intel::encode_cache::avif_encode_key`]). Una sola
/// fuente de verdad: si un campo cambia aquí, la clave del dedupe
/// cambia con él y el dedupe deja de coincidir (nunca deduplica de
/// más).
pub(crate) fn avif_candidate_profile(candidate: &Candidate) -> OptimizationProfile {
    OptimizationProfile {
        name: candidate.label.clone(),
        kind: ProfileKind::Custom,
        jpeg_quality: 82,
        jpeg_progressive: true,
        webp_quality: 80,
        webp_lossless: false,
        avif_quality: candidate.quality.unwrap_or(60),
        avif_alpha_quality: 80,
        // Presupuesto de tiempo del candidato → el handler decide el
        // speed de rav1e con él (ver `recommended_speed`).
        avif_time_budget_ms: candidate.backend_time_budget_ms,
        png_optimization_level: 3,
        metadata_mode: candidate.metadata_mode,
        preserve_color_profile: candidate.preserve_color_profile,
        jpeg_chroma_444: true,
        resize: candidate.resize,
    }
}

impl AvifBackend {
    pub fn new() -> Self {
        Self {
            handler: AvifHandler::new(),
        }
    }

    fn build_profile(&self, candidate: &Candidate) -> OptimizationProfile {
        avif_candidate_profile(candidate)
    }
}

impl Default for AvifBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatBackend for AvifBackend {
    fn format(&self) -> Format {
        Format::Avif
    }

    fn name(&self) -> &str {
        "AvifBackend (ravif 0.11 / rav1e)"
    }

    fn capabilities(&self) -> FormatCapabilities {
        self.handler.capabilities()
    }

    fn can_handle(&self, profile: &FileProfile, candidate: &Candidate) -> bool {
        // Fuente AVIF: no soportada como entrada de conversión.
        if profile.format == Format::Avif {
            return false;
        }
        candidate.format == Format::Avif && !candidate.lossless
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
        // AvifHandler.convert() cubre PNG/JPEG/WebP → AVIF.
        let result = self.handler.convert(input, output, Format::Avif, &p)?;
        Ok(BackendResult {
            output_path: output.to_path_buf(),
            output_size: result.output_size,
            processing_time_ms: start.elapsed().as_millis() as u64,
            lossless: false, // el encode AVIF aquí es siempre lossy
        })
    }
}
