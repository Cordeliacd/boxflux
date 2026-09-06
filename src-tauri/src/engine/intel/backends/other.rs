//! Adaptadores para GIF / BMP / TIFF: delegan en los handlers de
//! `formats::other` vía el crate `image`.

use std::path::Path;
use std::time::Instant;

use crate::engine::formats::{
    BmpHandler, Format, FormatCapabilities, FormatHandler, GifHandler, TiffHandler,
};
use crate::engine::optimization::{OptimizationProfile, ProfileKind};

use crate::engine::intel::backend::{BackendError, BackendResult, FormatBackend};
use crate::engine::intel::candidate::Candidate;
use crate::engine::intel::profile::FileProfile;

macro_rules! generic_backend {
    ($name:ident, $handler_ty:ty, $format:expr, $lib_name:expr) => {
        pub struct $name {
            handler: $handler_ty,
        }

        impl $name {
            pub fn new() -> Self {
                Self {
                    handler: <$handler_ty>::new(),
                }
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl FormatBackend for $name {
            fn format(&self) -> Format {
                $format
            }

            fn name(&self) -> &str {
                $lib_name
            }

            fn capabilities(&self) -> FormatCapabilities {
                self.handler.capabilities()
            }

            fn can_handle(&self, _profile: &FileProfile, candidate: &Candidate) -> bool {
                // El crate image re-encodea estos formatos: no garantiza
                // preservar el bitstream original ni todas las features
                // específicas del formato. No etiquetarlos lossless.
                candidate.format == $format && !candidate.lossless
            }

            fn process(
                &self,
                input: &Path,
                output: &Path,
                candidate: &Candidate,
                _profile: &FileProfile,
            ) -> Result<BackendResult, BackendError> {
                let start = Instant::now();
                let p = OptimizationProfile {
                    name: candidate.label.clone(),
                    kind: ProfileKind::Custom,
                    jpeg_quality: 82,
                    jpeg_progressive: true,
                    webp_quality: 80,
                    webp_lossless: false,
                    avif_quality: 60,
                    avif_alpha_quality: 80,
                    avif_time_budget_ms: 0,
                    png_optimization_level: 3,
                    metadata_mode: candidate.metadata_mode,
                    preserve_color_profile: candidate.preserve_color_profile,
                    jpeg_chroma_444: true,
                    resize: candidate.resize,
                };
                // "Optimizar" estos formatos es re-encodear: puede
                // encoger el archivo si el input estaba mal codificado.
                let result = self.handler.optimize(input, output, &p)?;
                Ok(BackendResult {
                    output_path: output.to_path_buf(),
                    output_size: result.output_size,
                    processing_time_ms: start.elapsed().as_millis() as u64,
                    lossless: false,
                })
            }
        }
    };
}

generic_backend!(
    GifBackend,
    GifHandler,
    Format::Gif,
    "GifBackend (image 0.25)"
);
generic_backend!(
    BmpBackend,
    BmpHandler,
    Format::Bmp,
    "BmpBackend (image 0.25)"
);
generic_backend!(
    TiffBackend,
    TiffHandler,
    Format::Tiff,
    "TiffBackend (image 0.25)"
);
