//! Detección de formato real por magic bytes (firma binaria).
//!
//! La extensión del archivo no garantiza el formato interno de los bytes.
//! Un archivo `.png` puede contener en realidad bytes WebP si el motor
//! decidió convertir pero el llamador conservó la extensión original.
//!
//! Este módulo valida los primeros bytes del archivo para determinar
//! empíricamente qué formato tiene el contenido. Es la fuente de verdad
//! usada por el pipeline y el queue worker para forzar que la extensión
//! del archivo final coincida con su codificación real.
//!
//! Referencias:
//! - PNG:  89 50 4E 47 0D 0A 1A 0A           (8 bytes)
//! - JPEG: FF D8 FF                          (3 bytes)
//! - WebP: 52 49 46 46 ?? ?? ?? ?? 57 45 42 50  (RIFF....WEBP, 12 bytes)
//! - AVIF/MOV/HEIC: usa ftyp box — bytes 4-7 = "ftyp"; para distinguir
//!   AVIF se requiere leer el major brand (bytes 8-11) ∈ {avif, avis, mif1}.
//!   Para ser conservadores con el costo, leemos 12 bytes y comprobamos
//!   el patrón ftyp + brand.
//! - GIF:  47 49 46 38 (37 39 | 39 61)        ("GIF87a" / "GIF89a")
//! - BMP:  42 4D                              ("BM")
//! - TIFF: 49 49 2A 00  ó  4D 4D 00 2A       (little-endian / big-endian)

use std::path::Path;

use crate::engine::formats::Format;

/// Mínimo número de bytes necesarios para identificar cualquier formato
/// soportado. Si el archivo tiene menos bytes que esto, se considera
/// `Format::Unknown`.
pub const MIN_MAGIC_BYTES: usize = 12;

/// Lee los primeros `MIN_MAGIC_BYTES` bytes del archivo. Devuelve `None`
/// si el archivo no existe o no se puede leer.
pub fn read_magic_bytes(path: &Path) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; MIN_MAGIC_BYTES];
    let n = file.read(&mut buf).ok()?;
    buf.truncate(n);
    Some(buf)
}

/// Detecta el formato real de un archivo leyendo sus magic bytes.
///
/// A diferencia de `Format::from_path` (que solo mira la extensión), esta
/// función mira el contenido binario. Es la fuente de verdad usada por
/// el pipeline y el queue worker para garantizar que la extensión del
/// archivo final coincida con su codificación interna.
pub fn detect_format_from_bytes(path: &Path) -> Format {
    let Some(bytes) = read_magic_bytes(path) else {
        return Format::Unknown;
    };
    detect_format(&bytes)
}

/// Detecta el formato a partir de un buffer de bytes.
pub fn detect_format(bytes: &[u8]) -> Format {
    // PNG: 89 50 4E 47 0D 0A 1A 0A
    if bytes.len() >= 8
        && bytes[0] == 0x89
        && &bytes[1..4] == b"PNG"
        && bytes[4] == 0x0D
        && bytes[5] == 0x0A
        && bytes[6] == 0x1A
        && bytes[7] == 0x0A
    {
        return Format::Png;
    }

    // JPEG: FF D8 FF
    if bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF {
        return Format::Jpeg;
    }

    // WebP: RIFF....WEBP
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Format::Webp;
    }

    // AVIF / HEIF: bytes 4-7 = "ftyp", bytes 8-11 ∈ {avif, avis, mif1, heic, heix}
    // AVIF canonical brands: "avif" (single image), "avis" (sequence).
    // "mif1" is generic HEIF (may or may not be AVIF — but we can't decode
    // HEIC anyway, so we map it to AVIF for extension purposes only when
    // the brand is one of the AVIF-family brands).
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        let brand = &bytes[8..12];
        match brand {
            b"avif" | b"avis" => return Format::Avif,
            b"mif1" | b"heic" | b"heix" | b"heim" | b"heis" | b"hevc" | b"hevx" => {
                // These are HEIF/HEIC variants. BoxFlux cannot decode them
                // today (no libheif binding), but the brand is real — we
                // report AVIF because it shares the ISOBMFF container and
                // the user should at least see a sensible extension.
                return Format::Avif;
            }
            _ => {}
        }
    }

    // GIF: 47 49 46 38 (37 39 | 39 61) → "GIF87a" or "GIF89a"
    if bytes.len() >= 6
        && &bytes[0..3] == b"GIF"
        && bytes[3] == b'8'
        && (bytes[4] == b'7' || bytes[4] == b'9')
        && bytes[5] == b'a'
    {
        return Format::Gif;
    }

    // BMP: 42 4D ("BM")
    if bytes.len() >= 2 && bytes[0] == b'B' && bytes[1] == b'M' {
        return Format::Bmp;
    }

    // TIFF: 49 49 2A 00 (little-endian) or 4D 4D 00 2A (big-endian)
    if bytes.len() >= 4 {
        if bytes[0] == 0x49 && bytes[1] == 0x49 && bytes[2] == 0x2A && bytes[3] == 0x00 {
            return Format::Tiff;
        }
        if bytes[0] == 0x4D && bytes[1] == 0x4D && bytes[2] == 0x00 && bytes[3] == 0x2A {
            return Format::Tiff;
        }
    }

    Format::Unknown
}

/// Verifica que la extensión del archivo coincide con su contenido binario.
///
/// Devuelve `true` si la extensión corresponde al formato detectado por
/// magic bytes. Si la extensión no mapea a ningún formato conocido, devuelve
/// `false` (la extensión es ambigua o inexistente).
pub fn extension_matches_content(path: &Path) -> bool {
    let ext_format = Format::from_path(path);
    if ext_format == Format::Unknown {
        return false;
    }
    let real_format = detect_format_from_bytes(path);
    real_format == ext_format && real_format != Format::Unknown
}

/// Devuelve la extensión canónica para el formato detectado en el archivo.
/// Útil para renombrar archivos cuya extensión no coincide con su contenido.
pub fn canonical_extension_for_file(path: &Path) -> &'static str {
    detect_format_from_bytes(path).canonical_extension()
}
