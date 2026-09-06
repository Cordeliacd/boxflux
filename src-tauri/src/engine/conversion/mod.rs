//! Conversion between formats.
//!
//! Conversions are dispatched through a generic helper that decodes the
//! source via the `image` crate and re-encodes using the appropriate
//! format-specific encoder (mozjpeg for JPEG, webp crate for WebP,
//! ravif for AVIF, image crate for the rest).

use std::fs;
use std::path::Path;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::engine::formats::{Format, FormatError};
use crate::engine::optimization::OptimizationProfile;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionResult {
    pub original_size: u64,
    pub output_size: u64,
    pub bytes_saved: u64,
    pub percentage_saved: f64,
    pub processing_time_ms: u64,
    pub input_format: Format,
    pub output_format: Format,
    pub success: bool,
    #[serde(default)]
    pub error: String,
}

/// Generic conversion entry point used by every format handler.
pub fn convert_generic(
    input_path: &Path,
    output_path: &Path,
    target_format: Format,
    profile: &OptimizationProfile,
    source_format: Format,
) -> Result<ConversionResult, FormatError> {
    if !input_path.exists() {
        return Err(FormatError::NotFound(input_path.display().to_string()));
    }
    let start = Instant::now();
    let original_size = fs::metadata(input_path)?.len();

    // Decodificación vía ImageReader para acceder al ICC del fuente
    // (misma lógica que el handler JPEG).
    let reader = image::ImageReader::open(input_path)?
        .with_guessed_format()
        .map_err(|e| FormatError::DecoderFailure(format!("format guess: {e}")))?;
    let mut decoder = reader.into_decoder()?;
    let icc_bytes: Option<Vec<u8>> = if profile.preserve_color_profile {
        use image::ImageDecoder as _;
        decoder.icc_profile().ok().flatten()
    } else {
        None
    };
    let img = image::DynamicImage::from_decoder(decoder)?;
    let img = if let Some(resize) = profile.resize.as_ref() {
        crate::engine::optimization::apply_resize(img, Some(resize))?
    } else {
        img
    };

    match target_format {
        Format::Jpeg => {
            let rgb = img.to_rgb8();
            let quality = profile.jpeg_quality.clamp(1, 100) as f32;
            let mut encoder = mozjpeg::compress::Compress::new(mozjpeg::ColorSpace::JCS_RGB);
            encoder.set_size(rgb.width() as usize, rgb.height() as usize);
            encoder.set_quality(quality);
            if profile.jpeg_progressive {
                encoder.set_progressive_mode();
                encoder.set_optimize_scans(true);
            }
            encoder.set_optimize_coding(true);
            // Subsampling de croma: ver formats/jpeg.rs.
            if profile.jpeg_chroma_444 {
                encoder.set_chroma_sampling_pixel_sizes((1, 1), (1, 1));
            } else {
                encoder.set_chroma_sampling_pixel_sizes((2, 2), (1, 1));
            }
            let writer = std::io::BufWriter::new(fs::File::create(output_path)?);
            let mut comp = encoder.start_compress(writer)?;
            // El ICC del fuente se embedea si se preserva fidelidad y no
            // se strippea todo (misma rationale que formats/jpeg.rs).
            let should_embed_icc = profile.preserve_color_profile
                && !matches!(
                    profile.metadata_mode,
                    crate::engine::optimization::MetadataMode::RemoveAll
                );
            if should_embed_icc {
                if let Some(icc) = icc_bytes.as_ref() {
                    comp.write_icc_profile(icc);
                }
            }
            comp.write_scanlines(&rgb)?;
            comp.finish()?;
        }
        Format::Png => {
            img.save_with_format(output_path, image::ImageFormat::Png)?;
            // Un fallo de oxipng aquí no se traga en silencio: el caller
            // vería `success: true` por un archivo que nunca se optimizó.
            // El PNG recién escrito queda utilizable; solo hay que avisar
            // de que la optimización no corrió.
            let opts = oxipng::Options::from_preset(profile.png_optimization_level.min(6));
            let in_file = oxipng::InFile::from(output_path.to_path_buf());
            let out_file = oxipng::OutFile::from_path(output_path.to_path_buf());
            if let Err(e) = oxipng::optimize(&in_file, &out_file, &opts) {
                return Err(FormatError::EncoderFailure(format!(
                    "oxipng (during conversion to PNG): {e}"
                )));
            }
        }
        Format::Webp => {
            let rgba = img.to_rgba8();
            let (w, h) = (rgba.width(), rgba.height());
            let mut encoder = webpx::Encoder::new_rgba(rgba.as_raw(), w, h);
            if profile.webp_lossless {
                encoder = encoder.lossless(true).method(6);
            } else {
                encoder = encoder
                    .quality(profile.webp_quality.clamp(0, 100) as f32)
                    .method(6)
                    .sharp_yuv(true);
            }
            let encoded = encoder
                .encode(webpx::Unstoppable)
                .map_err(|e| FormatError::EncoderFailure(format!("webpx: {e}")))?;
            fs::write(output_path, &encoded)?;
        }
        Format::Avif => {
            let rgba = img.to_rgba8();
            let width = rgba.width();
            let height = rgba.height();
            let pixels: Vec<rgb::RGBA8> = rgba
                .pixels()
                .map(|p| rgb::RGBA8::new(p[0], p[1], p[2], p[3]))
                .collect();
            let img_rgb = ravif::Img::new(&pixels[..], width as usize, height as usize);
            let config = ravif::Encoder::new()
                .with_quality(profile.avif_quality.clamp(0, 100) as f32)
                .with_alpha_quality(profile.avif_alpha_quality.clamp(0, 100) as f32)
                .with_speed(6);
            let encoded = config
                .encode_rgba(img_rgb)
                .map_err(|e| FormatError::EncoderFailure(format!("ravif: {e}")))?;
            fs::write(output_path, encoded.avif_file.as_slice())?;
        }
        Format::Gif => {
            img.save_with_format(output_path, image::ImageFormat::Gif)?;
        }
        Format::Bmp => {
            img.save_with_format(output_path, image::ImageFormat::Bmp)?;
        }
        Format::Tiff => {
            img.save_with_format(output_path, image::ImageFormat::Tiff)?;
        }
        Format::Unknown => {
            return Err(FormatError::Unsupported(format!(
                "unknown target format for {input_path:?}"
            )));
        }
    }

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
        input_format: source_format,
        output_format: target_format,
        success: true,
        error: String::new(),
    })
}
