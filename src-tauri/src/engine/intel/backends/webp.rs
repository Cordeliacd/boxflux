//! WebpBackend: adaptador sobre [`crate::engine::formats::WebpHandler`],
//! que usa el crate `webp`.

use std::path::Path;
use std::time::Instant;

use crate::engine::formats::{Format, FormatCapabilities, FormatHandler, WebpHandler};
use crate::engine::optimization::{OptimizationProfile, ProfileKind};

use crate::engine::intel::backend::{BackendError, BackendResult, FormatBackend};
use crate::engine::intel::candidate::Candidate;
use crate::engine::intel::profile::FileProfile;

pub struct WebpBackend {
    handler: WebpHandler,
}

impl WebpBackend {
    pub fn new() -> Self {
        Self {
            handler: WebpHandler::new(),
        }
    }

    fn build_profile(&self, candidate: &Candidate) -> OptimizationProfile {
        OptimizationProfile {
            name: candidate.label.clone(),
            kind: ProfileKind::Custom,
            jpeg_quality: 82,
            jpeg_progressive: true,
            webp_quality: candidate.quality.unwrap_or(80),
            webp_lossless: candidate.lossless,
            avif_quality: 60,
            avif_alpha_quality: 80,
            avif_time_budget_ms: 0,
            png_optimization_level: 3,
            metadata_mode: candidate.metadata_mode,
            preserve_color_profile: candidate.preserve_color_profile,
            jpeg_chroma_444: true,
            resize: candidate.resize,
        }
    }
}

impl Default for WebpBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatBackend for WebpBackend {
    fn format(&self) -> Format {
        Format::Webp
    }

    fn name(&self) -> &str {
        "WebpBackend (webp 0.3)"
    }

    fn capabilities(&self) -> FormatCapabilities {
        self.handler.capabilities()
    }

    fn can_handle(&self, _profile: &FileProfile, candidate: &Candidate) -> bool {
        candidate.format == Format::Webp
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
        let result = self.handler.optimize(input, output, &p)?;
        Ok(BackendResult {
            output_path: output.to_path_buf(),
            output_size: result.output_size,
            processing_time_ms: start.elapsed().as_millis() as u64,
            lossless: candidate.lossless,
        })
    }
}
