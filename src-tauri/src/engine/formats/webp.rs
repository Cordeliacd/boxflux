//! WebP format handler.
//!
//! Supports both lossy and lossless encoding modes via the `webpx` crate
//! (libwebp bindings with animation + ICC + streaming support).

use std::fs;
use std::path::Path;
use std::time::Instant;

use crate::engine::conversion::ConversionResult;
use crate::engine::formats::{Format, FormatCapabilities, FormatError, FormatHandler, FormatInfo};
use crate::engine::optimization::{OptimizationProfile, OptimizationResult};

pub struct WebpHandler;

impl WebpHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WebpHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatHandler for WebpHandler {
    fn format(&self) -> Format {
        Format::Webp
    }

    fn capabilities(&self) -> FormatCapabilities {
        FormatCapabilities {
            can_read: true,
            can_write: true,
            can_optimize: true,
            can_convert: true,
        }
    }

    fn analyze(&self, path: &Path) -> Result<FormatInfo, FormatError> {
        if !path.exists() {
            return Err(FormatError::NotFound(path.display().to_string()));
        }
        let file_size = fs::metadata(path)?.len();
        let reader = image::ImageReader::open(path)?.with_guessed_format()?;
        let dims = reader.into_dimensions()?;
        Ok(FormatInfo {
            format: Format::Webp,
            width: dims.0,
            height: dims.1,
            file_size,
            has_alpha: true,
            color_type: "RGBA".to_string(),
            bit_depth: 8,
        })
    }

    fn optimize(
        &self,
        input_path: &Path,
        output_path: &Path,
        profile: &OptimizationProfile,
    ) -> Result<OptimizationResult, FormatError> {
        if !input_path.exists() {
            return Err(FormatError::NotFound(input_path.display().to_string()));
        }
        let start = Instant::now();
        let original_size = fs::metadata(input_path)?.len();

        let img = image::open(input_path)?;
        let img = if let Some(resize) = profile.resize.as_ref() {
            crate::engine::optimization::apply_resize(img, Some(resize))?
        } else {
            img
        };

        let quality = profile.webp_quality.clamp(0, 100);
        // `method` 6 = máxima búsqueda de predicción/filtros; el
        // default de libwebp es 4 y deja 2-6% de compresión sin usar.
        // `sharp_yuv` aplica reconstrucción de croma en bordes
        // (Sharp RGB→YUV): reduce el fringe de color en bordes
        // saturados frente al conversor bilineal por defecto.
        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        let mut encoder = webpx::Encoder::new_rgba(rgba.as_raw(), w, h);
        if profile.webp_lossless {
            encoder = encoder.lossless(true).method(6);
        } else {
            encoder = encoder.quality(quality as f32).method(6).sharp_yuv(true);
        }
        let webp_bytes: Vec<u8> = encoder
            .encode(webpx::Unstoppable)
            .map_err(|e| FormatError::EncoderFailure(format!("webpx encode: {e}")))?;
        fs::write(output_path, &webp_bytes)?;

        let output_size = fs::metadata(output_path)?.len();
        let bytes_saved = original_size.saturating_sub(output_size);
        let percentage_saved = if original_size == 0 {
            0.0
        } else {
            100.0 * (original_size as f64 - output_size as f64) / original_size as f64
        };

        Ok(OptimizationResult {
            original_size,
            output_size,
            bytes_saved,
            percentage_saved,
            processing_time_ms: start.elapsed().as_millis() as u64,
            format: Format::Webp,
            success: true,
            error: String::new(),
        })
    }

    fn convert(
        &self,
        input_path: &Path,
        output_path: &Path,
        target_format: Format,
        profile: &OptimizationProfile,
    ) -> Result<ConversionResult, FormatError> {
        crate::engine::conversion::convert_generic(
            input_path,
            output_path,
            target_format,
            profile,
            Format::Webp,
        )
    }
}
