//! Metadata inspection and stripping.
//!
//! BoxFlux never silently removes metadata. The user must opt in via
//! [`crate::engine::optimization::MetadataMode`]. This module provides read-only
//! inspection helpers used by the Before/After viewer.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::engine::formats::FormatError;

/// Descripción de metadata de un archivo. Solo se inspecciona EXIF
/// (JPEG/TIFF vía kamadak-exif); otros formatos devuelven info vacía.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetadataInfo {
    pub has_exif: bool,
    pub has_gps: bool,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub software: Option<String>,
    pub datetime: Option<String>,
    pub orientation: Option<u16>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// Inspect a file for metadata. Currently only JPEG/TIFF EXIF is parsed
/// via the `kamadak-exif` crate. Other formats return an empty info.
pub fn inspect(path: &Path) -> Result<MetadataInfo, FormatError> {
    let file = File::open(path)?;
    let mut buf = BufReader::new(&file);
    let reader = exif::Reader::new();
    let mut info = MetadataInfo::default();
    let mut exif_found = false;
    match reader.read_from_container(&mut buf) {
        Ok(exif_data) => {
            exif_found = true;
            for f in exif_data.fields() {
                use exif::Tag;
                match f.tag {
                    Tag::Make => {
                        info.camera_make = Some(f.display_value().with_unit(&exif_data).to_string())
                    }
                    Tag::Model => {
                        info.camera_model =
                            Some(f.display_value().with_unit(&exif_data).to_string())
                    }
                    Tag::Software => info.software = Some(f.display_value().to_string()),
                    Tag::DateTime | Tag::DateTimeOriginal | Tag::DateTimeDigitized => {
                        if info.datetime.is_none() {
                            info.datetime = Some(f.display_value().to_string());
                        }
                    }
                    Tag::Orientation => {
                        if let exif::Value::Short(v) = &f.value {
                            if let Some(o) = v.first() {
                                info.orientation = Some(*o);
                            }
                        }
                    }
                    Tag::PixelXDimension | Tag::ImageWidth => {
                        if let exif::Value::Short(v) = &f.value {
                            if let Some(w) = v.first() {
                                info.width = Some(*w as u32);
                            }
                        }
                    }
                    Tag::PixelYDimension | Tag::ImageLength => {
                        if let exif::Value::Short(v) = &f.value {
                            if let Some(h) = v.first() {
                                info.height = Some(*h as u32);
                            }
                        }
                    }
                    _ => {
                        if f.tag == Tag::GPSInfoIFDPointer {
                            info.has_gps = true;
                        }
                    }
                }
            }
        }
        Err(_) => {
            // Not every file has EXIF — this is fine.
        }
    }
    info.has_exif = exif_found;
    Ok(info)
}
