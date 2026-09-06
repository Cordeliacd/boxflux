//! JpegBackend: adaptador sobre [`crate::engine::formats::JpegHandler`],
//! que usa `mozjpeg` para re-encode.
//!
//! Dos rutas:
//! - **Source-quality** (fuente JPEG, quality `None`): re-encode con
//!   las quant tables del original, jpegtran-style. Su calidad la
//!   MIDE el pipeline (no se etiqueta lossless porque el roundtrip
//!   IDCT→FDCT no es bit-exacto).
//! - **Lossy**: re-encode mozjpeg con chroma 4:4:4 o 4:2:0 según la
//!   categoría de la imagen (fotos → 4:2:0, gráficos → 4:4:4).

use std::path::Path;
use std::time::Instant;

use crate::engine::formats::{Format, FormatCapabilities, FormatHandler, JpegHandler};
use crate::engine::optimization::{OptimizationProfile, ProfileKind};

use crate::engine::intel::backend::{BackendError, BackendResult, FormatBackend};
use crate::engine::intel::candidate::Candidate;
use crate::engine::intel::profile::FileProfile;

pub struct JpegBackend {
    handler: JpegHandler,
}

impl JpegBackend {
    pub fn new() -> Self {
        Self {
            handler: JpegHandler::new(),
        }
    }

    fn build_profile(&self, candidate: &Candidate, profile: &FileProfile) -> OptimizationProfile {
        // Chroma subsampling adaptativo por categoría.
        //
        // La resolución cromática del ojo es ~1/4 de la luminante: en
        // fotografía el 4:2:0 es prácticamente imperceptible y ahorra
        // 10-15%. En imágenes sintéticas (screenshots, UI, texto) el
        // subsampling produce fringe visible → 4:4:4. La lógica vive en
        // `FileProfile::is_photo_like()` y la comparte el iterative
        // search. El scorer mide el resultado real: si el 4:2:0 degrada
        // más de lo esperado en una foto concreta, el candidato pierde
        // contra otro — decisión adaptativa, no un hardcode.
        let photo_like = profile.is_photo_like();
        OptimizationProfile {
            name: candidate.label.clone(),
            kind: ProfileKind::Custom,
            jpeg_quality: candidate.quality.unwrap_or(82),
            jpeg_progressive: true,
            webp_quality: 80,
            webp_lossless: false,
            avif_quality: 60,
            avif_alpha_quality: 80,
            avif_time_budget_ms: 0,
            png_optimization_level: 3,
            metadata_mode: candidate.metadata_mode,
            preserve_color_profile: candidate.preserve_color_profile,
            // Fotos (y desconocidas, que suelen ser fotos) → 4:2:0;
            // screenshots/texto/ilustraciones → 4:4:4.
            jpeg_chroma_444: !photo_like,
            resize: candidate.resize,
        }
    }
}

impl Default for JpegBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatBackend for JpegBackend {
    fn format(&self) -> Format {
        Format::Jpeg
    }

    fn name(&self) -> &str {
        "JpegBackend (mozjpeg 0.10)"
    }

    fn capabilities(&self) -> FormatCapabilities {
        self.handler.capabilities()
    }

    fn can_handle(&self, profile: &FileProfile, candidate: &Candidate) -> bool {
        if candidate.format != Format::Jpeg {
            return false;
        }
        // El re-encode "source-quality" (quality=None) solo tiene
        // sentido cuando la fuente YA es JPEG: reutiliza sus quant
        // tables y sampling. Una conversión PNG→JPEG siempre necesita
        // un quality explícito del ladder.
        if candidate.quality.is_none() && !candidate.lossless {
            return profile.format == Format::Jpeg;
        }
        if candidate.lossless {
            // No existe un JPEG lossless verificable desde fuentes
            // no-JPEG, y desde fuente JPEG el roundtrip IDCT→FDCT de
            // mozjpeg-rs no es bit-exacto → no lo ofrecemos como
            // lossless (honestidad sobre etiquetas).
            return false;
        }
        // Re-encode lossy: JPEG no tiene canal alpha — la fuente debe ser
        // opaca (la capa de estrategia ya lo filtra, defensa extra).
        !profile.has_alpha
    }

    fn process(
        &self,
        input: &Path,
        output: &Path,
        candidate: &Candidate,
        profile: &FileProfile,
    ) -> Result<BackendResult, BackendError> {
        let start = Instant::now();
        let p = self.build_profile(candidate, profile);
        let result = if candidate.quality.is_none() && profile.format == Format::Jpeg {
            // Re-encode con las quant tables del original — jpegtran-style.
            // El pipeline medirá su calidad real (SSIM ≈0.999).
            self.handler.reencode_source_quality(input, output, &p)?
        } else {
            self.handler.optimize(input, output, &p)?
        };
        Ok(BackendResult {
            output_path: output.to_path_buf(),
            output_size: result.output_size,
            processing_time_ms: start.elapsed().as_millis() as u64,
            lossless: false,
        })
    }
}
