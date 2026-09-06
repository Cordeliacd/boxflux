//! AVIF format handler.
//!
//! Encoding is performed via the `ravif` crate. AVIF encoding is CPU
//! expensive; the engine caps concurrency to avoid swamping the system.

use std::fs;
use std::path::Path;
use std::time::Instant;

use crate::engine::conversion::ConversionResult;
use crate::engine::formats::{Format, FormatCapabilities, FormatError, FormatHandler, FormatInfo};
use crate::engine::optimization::{OptimizationProfile, OptimizationResult};

pub struct AvifHandler;

impl AvifHandler {
    pub fn new() -> Self {
        Self
    }

    /// Velocidad de rav1e según presupuesto de tiempo y tamaño de
    /// imagen.
    ///
    /// Speed 4 es el sweet spot: 16-21% menor que speed 5/6 a igual
    /// calidad con solo +30% de tiempo. La velocidad sube SOLO cuando
    /// el presupuesto de tiempo del candidato no da para el speed 4
    /// (estimación conservadora de ~2 s/MP a speed 4 con los threads
    /// disponibles).
    pub(crate) fn recommended_speed(
        quality: u8,
        pixels: u64,
        backend_time_budget_ms: u64,
    ) -> u8 {
        if backend_time_budget_ms > 0 {
            // Estimación conservadora del coste a speed 4.
            let mps = (pixels as f64 / 1_000_000.0).max(0.05);
            let est_ms_speed4 = mps * 2000.0;
            if est_ms_speed4 > backend_time_budget_ms as f64 {
                return 8; // no cabe → modo rápido
            }
            return 4; // cabe → máxima compresión
        }
        // Sin presupuesto (conversión manual desde la UI).
        if quality >= 75 {
            4
        } else {
            5
        }
    }
}

impl Default for AvifHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatHandler for AvifHandler {
    fn format(&self) -> Format {
        Format::Avif
    }

    fn capabilities(&self) -> FormatCapabilities {
        FormatCapabilities {
            can_read: false, // la lectura de AVIF va por la feature avif del crate image, aún no habilitada
            can_write: true,
            can_optimize: false,
            can_convert: true,
        }
    }

    fn analyze(&self, _path: &Path) -> Result<FormatInfo, FormatError> {
        // Leer AVIF requiere la feature avif del crate image; mejor
        // CannotRead que devolver datos falsos.
        Err(FormatError::CannotRead(Format::Avif))
    }

    fn optimize(
        &self,
        _input: &Path,
        _output: &Path,
        _profile: &OptimizationProfile,
    ) -> Result<OptimizationResult, FormatError> {
        // Solo se convierte HACIA AVIF; no hay optimización in-place.
        Err(FormatError::CannotOptimize(Format::Avif))
    }

    fn convert(
        &self,
        input_path: &Path,
        output_path: &Path,
        target_format: Format,
        profile: &OptimizationProfile,
    ) -> Result<ConversionResult, FormatError> {
        if target_format != Format::Avif {
            return Err(FormatError::Unsupported(format!(
                "AVIF handler only writes AVIF, got target {target_format}"
            )));
        }
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

        let rgba = img.to_rgba8();
        let width = rgba.width();
        let height = rgba.height();
        let pixels: Vec<rgb::RGBA8> = rgba
            .pixels()
            .map(|p| rgb::RGBA8::new(p[0], p[1], p[2], p[3]))
            .collect();

        let img_rgba = ravif::Img::new(&pixels[..], width as usize, height as usize);

        let quality = profile.avif_quality.clamp(0, 100) as f32;
        let alpha_quality = profile.avif_alpha_quality.clamp(0, 100) as f32;
        // speed (0-10): 0=más lento/mejor. 4 es el sweet spot (ver
        // `AvifHandler::recommended_speed`); sube solo si el presupuesto
        // de tiempo no da para 4. `encode_rgb` para fuentes opacas
        // (sin plano alpha, más pequeño y rápido). `with_num_threads`
        // paraleliza el encode en rav1e.
        let speed = Self::recommended_speed(
            profile.avif_quality,
            u64::from(width) * u64::from(height),
            profile.avif_time_budget_ms,
        );
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
            .min(4); // rav1e escala mal más allá de ~4 threads por imagen
        let config = ravif::Encoder::new()
            .with_quality(quality)
            .with_alpha_quality(alpha_quality)
            .with_speed(speed)
            .with_num_threads(Some(threads));

        // ¿La imagen tiene canal alpha por tipo de píxel? Si no, usamos
        // encode_rgb (sin plano alpha). Un RGBA con alpha=255 en todos
        // los píxeles comprime el plano a casi nada, así que no vale la
        // pena el escaneo píxel a píxel.
        let source_has_alpha = img.color().has_alpha();
        let encoded = if source_has_alpha {
            config
                .encode_rgba(img_rgba)
                .map_err(|e| FormatError::EncoderFailure(format!("ravif: {e}")))?
        } else {
            let rgb_pixels: Vec<rgb::RGB8> = rgba
                .pixels()
                .map(|p| rgb::RGB8::new(p[0], p[1], p[2]))
                .collect();
            let img_rgb = ravif::Img::new(&rgb_pixels[..], width as usize, height as usize);
            config
                .encode_rgb(img_rgb)
                .map_err(|e| FormatError::EncoderFailure(format!("ravif: {e}")))?
        };
        fs::write(output_path, encoded.avif_file.as_slice())?;

        let output_size = fs::metadata(output_path)?.len();
        let bytes_saved = original_size.saturating_sub(output_size);
        let percentage_saved = if original_size == 0 {
            0.0
        } else {
            100.0 * (original_size as f64 - output_size as f64) / original_size as f64
        };

        Ok(ConversionResult {
            original_size,
            output_size,
            bytes_saved,
            percentage_saved,
            processing_time_ms: start.elapsed().as_millis() as u64,
            input_format: Format::from_path(input_path),
            output_format: Format::Avif,
            success: true,
            error: String::new(),
        })
    }
}
