//! GIF / BMP / TIFF format handlers.
//!
//! These three formats share a common pattern via the `image` crate:
//! they can be read, can be converted, but do not have a specialized
//! optimization engine. Their "optimize" path is a re-encode that may
//! reduce file size if the input was inefficiently encoded.

use std::fs;
use std::path::Path;
use std::time::Instant;

use crate::engine::conversion::ConversionResult;
use crate::engine::formats::{Format, FormatCapabilities, FormatError, FormatHandler, FormatInfo};
use crate::engine::optimization::{OptimizationProfile, OptimizationResult};

macro_rules! generic_handler {
    ($name:ident, $format:expr, $image_format:expr, $can_optimize:expr) => {
        pub struct $name;

        impl $name {
            pub fn new() -> Self {
                Self
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl FormatHandler for $name {
            fn format(&self) -> Format {
                $format
            }

            fn capabilities(&self) -> FormatCapabilities {
                FormatCapabilities {
                    can_read: true,
                    can_write: true,
                    can_optimize: $can_optimize,
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
                    format: $format,
                    width: dims.0,
                    height: dims.1,
                    file_size,
                    has_alpha: matches!($format, Format::Gif | Format::Bmp),
                    color_type: "RGB".to_string(),
                    bit_depth: 8,
                })
            }

            fn optimize(
                &self,
                input_path: &Path,
                output_path: &Path,
                profile: &OptimizationProfile,
            ) -> Result<OptimizationResult, FormatError> {
                if !$can_optimize {
                    return Err(FormatError::CannotOptimize($format));
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
                img.save_with_format(output_path, $image_format)?;

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
                    format: $format,
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
                    $format,
                )
            }
        }
    };
}

generic_handler!(GifHandler, Format::Gif, image::ImageFormat::Gif, false);
generic_handler!(BmpHandler, Format::Bmp, image::ImageFormat::Bmp, false);
generic_handler!(TiffHandler, Format::Tiff, image::ImageFormat::Tiff, false);
