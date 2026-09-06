//! Heurísticas deterministas que recortan el espacio de búsqueda.
//! Reglas simples, no ML, derivadas del comportamiento observado de
//! los códecs: cada una permite que un candidato se pruebe o lo
//! rechaza con una razón registrada. Viven todas aquí, separadas del
//! resto del motor.

use super::candidate::Candidate;
use super::profile::{FileProfile, ImageCategory};
use super::strategy::OptimizationStrategy;

#[derive(Debug, Clone)]
pub struct OptimizationHeuristics {
    /// Tamaño (bytes) por debajo del cual no vale la pena optimizar.
    pub trivially_small_threshold: u64,
}

impl Default for OptimizationHeuristics {
    fn default() -> Self {
        Self {
            trivially_small_threshold: 4 * 1024,
        }
    }
}

impl OptimizationHeuristics {
    /// Devuelve `Ok(())` si el candidato debe probarse o `Err(reason)`
    /// si las heurísticas lo rechazan. Se apoya en el análisis de
    /// píxeles del analyzer (zonos oscuras, ruido, saturación, bordes,
    /// zonas planas).
    pub fn should_test(
        &self,
        profile: &FileProfile,
        candidate: &Candidate,
        strategy: &OptimizationStrategy,
    ) -> Result<(), String> {
        // El goal excluye candidatos lossy (o lossless).
        if !strategy.include_lossy && !candidate.lossless {
            return Err("goal excludes lossy candidates".into());
        }
        if !strategy.include_lossless && candidate.lossless {
            return Err("goal excludes lossless candidates".into());
        }

        // Archivos triviales: el re-encode lossy no compensa.
        if profile.file_size < self.trivially_small_threshold && !candidate.lossless {
            return Err(format!(
                "file is trivially small ({} bytes); lossy re-encode not worthwhile",
                profile.file_size
            ));
        }

        // Alpha: formatos sin alpha no se prueban sobre fuentes con
        // transparencia.
        if profile.has_alpha
            && matches!(
                candidate.format,
                crate::engine::formats::Format::Jpeg | crate::engine::formats::Format::Bmp
            )
        {
            return Err("candidate format cannot preserve source transparency".into());
        }

        // Animado: la fuente sigue siendo GIF.
        if profile.is_animated && candidate.format != crate::engine::formats::Format::Gif {
            return Err("animated source can only be re-encoded as GIF in Phase 2".into());
        }

        // PNG no tiene modo lossy.
        if candidate.format == crate::engine::formats::Format::Png && !candidate.lossless {
            return Err("PNG has no lossy mode".into());
        }

        // JPEG lossless (transcode raw DCT) solo tiene sentido si la
        // fuente ya es JPEG: convertir a JPEG desde píxeles no-JPEG
        // siempre es lossy. El generador ya lo filtra; esto es
        // defensa en profundidad.
        if candidate.format == crate::engine::formats::Format::Jpeg
            && candidate.lossless
            && profile.format != crate::engine::formats::Format::Jpeg
        {
            return Err("JPEG lossless requires a JPEG source (raw DCT transcode)".into());
        }

        // Iconos: lossless, para evitar artifacts.
        if profile.pixel_count() < 4096
            && !candidate.lossless
            && profile.category == ImageCategory::Icon
        {
            return Err("icon-sized image: prefer lossless to avoid artifacts".into());
        }

        // Texto denso: lossy solo en calidad alta (q ≥ 75) — por debajo
        // los artifacts en bordes de texto sí son visibles. La calidad
        // real la sigue midiendo el pipeline; esto solo acota la escalera.
        if profile.category == ImageCategory::TextHeavy
            && !candidate.lossless
            && candidate.quality.map(|q| q < 75).unwrap_or(false)
        {
            return Err("text-heavy image: lossy only at q>=75 (sharpness)".into());
        }

        // Screenshots: AVIF a baja calidad rompe el texto.
        if profile.category == ImageCategory::Screenshot
            && candidate.format == crate::engine::formats::Format::Avif
            && candidate.quality.map(|q| q < 60).unwrap_or(false)
        {
            return Err("screenshot at low AVIF quality risks text artifacts".into());
        }

        // Imágenes predominantemente oscuras: JPEG q < 80 arriesga
        // banding en las sombras (cuantización más gruesa ahí).
        if profile
            .characteristics
            .contains(&"predominantly_dark".to_string())
            && candidate.format == crate::engine::formats::Format::Jpeg
            && candidate.quality.map(|q| q < 80).unwrap_or(false)
        {
            return Err("predominantly dark image: JPEG q<80 risks banding in shadows".into());
        }

        // Ruido: lossless no comprime ruido (entropía alta) y el lossy a
        // calidad moderada hace de denoiser suave.
        if profile.characteristics.contains(&"noisy".to_string())
            && candidate.lossless
            && profile.file_size > 200_000
        {
            return Err("noisy image: lossless would be larger than lossy; prefer lossy".into());
        }

        // Saturación alta: JPEG a q < 85 arriesga desplazamiento de
        // color (mozjpeg convierte a YCbCr internamente aunque pidamos
        // 4:4:4).
        if profile
            .characteristics
            .contains(&"highly_saturated".to_string())
            && candidate.format == crate::engine::formats::Format::Jpeg
            && candidate.quality.map(|q| q < 85).unwrap_or(false)
        {
            return Err("highly saturated image: JPEG q<85 risks color shifts".into());
        }

        // Zonas planas + archivo pequeño (< 120 KB): el ahorro absoluto
        // es trivial y el riesgo de artifacts en zonas planas domina.
        // Por encima de 120 KB, decide la medición de calidad del
        // pipeline.
        if profile
            .characteristics
            .contains(&"large_flat_areas".to_string())
            && !candidate.lossless
            && profile.file_size < 120_000
            && candidate.quality.map(|q| q < 80).unwrap_or(false)
        {
            return Err(
                "large flat areas (small file): lossless would be smaller; skip low-quality lossy".into(),
            );
        }

        // Texto/UI: AVIF a q < 70 produce bloques muy visibles en
        // bordes de texto.
        if profile.characteristics.contains(&"text_or_ui".to_string())
            && candidate.format == crate::engine::formats::Format::Avif
            && candidate.quality.map(|q| q < 70).unwrap_or(false)
        {
            return Err("text/UI image: AVIF q<70 risks block artifacts on text".into());
        }

        // Fotos grandes (≥5 MP): PNG lossless siempre pierde contra
        // WebP lossless (que ya se prueba); evitar que compita sabiendo
        // que pierde. Umbral de 5 MP para cubrir fotos de cámaras
        // modernas sin afectar imágenes web típicas (~2 MP).
        if profile.category == ImageCategory::Photograph
            && profile.pixel_count() > 5_000_000
            && candidate.format == crate::engine::formats::Format::Png
            && candidate.lossless
        {
            return Err(
                "large photograph: PNG lossless would be larger than WebP lossless; skip".into(),
            );
        }

        // Zonas planas (ilustraciones, logos, screenshots): JPEG produce
        // artifacts de bloque muy visibles; solo permitirlo a q ≥ 95.
        if profile
            .characteristics
            .contains(&"large_flat_areas".to_string())
            && candidate.format == crate::engine::formats::Format::Jpeg
            && candidate.quality.map(|q| q < 95).unwrap_or(false)
        {
            return Err(
                "large flat areas: JPEG at q<95 produces visible block artifacts; use PNG/WebP"
                    .into(),
            );
        }

        // Imágenes diminutas (< 32×32): el overhead de AVIF/WebP supera
        // el ahorro; solo lossless.
        if profile.pixel_count() < 1_024
            && !candidate.lossless
            && matches!(
                candidate.format,
                crate::engine::formats::Format::Avif
                    | crate::engine::formats::Format::Webp
                    | crate::engine::formats::Format::Jpeg
            )
        {
            return Err(
                "tiny image (<32×32): lossy overhead exceeds savings; prefer lossless".into(),
            );
        }

        Ok(())
    }

    /// Tras medir todos los candidatos: si el lossless ya es ≤ que el
    /// menor de los lossy aceptables, el lossless gana aunque un lossy
    /// suelto pudiera ser menor.
    pub fn prefer_lossless_if_smaller(&self, lossless_size: u64, smallest_lossy_size: u64) -> bool {
        lossless_size <= smallest_lossy_size
    }
}
