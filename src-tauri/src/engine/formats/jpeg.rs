//! Handler de JPEG: re-encode con [`mozjpeg`] (mejor compresión que
//! los defaults de libjpeg-turbo a igual calidad configurada).
//!
//! Además del re-encode a calidad configurada existe
//! `reencode_source_quality`: transcode estilo jpegtran que reutiliza
//! las quant tables del original. El subsampling de croma lo decide el
//! backend según la categoría de la imagen, independiente del perfil ICC.

use std::fs;
use std::io::BufWriter;
use std::path::Path;
use std::time::Instant;

use image::ImageDecoder;
use mozjpeg::CompInfoExt;

use crate::engine::conversion::ConversionResult;
use crate::engine::formats::{Format, FormatCapabilities, FormatError, FormatHandler, FormatInfo};
use crate::engine::optimization::{MetadataMode, OptimizationProfile, OptimizationResult};

pub struct JpegHandler;

impl JpegHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for JpegHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatHandler for JpegHandler {
    fn format(&self) -> Format {
        Format::Jpeg
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
            format: Format::Jpeg,
            width: dims.0,
            height: dims.1,
            file_size,
            has_alpha: false, // JPEG has no alpha
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
        if !input_path.exists() {
            return Err(FormatError::NotFound(input_path.display().to_string()));
        }
        let start = Instant::now();
        let original_size = fs::metadata(input_path)?.len();

        // Se decodifica vía `ImageReader` para leer el perfil ICC del
        // fuente ANTES de consumir el decoder: `image::open` descarta
        // el chunk ICC y los Display P3 salían oscurecidos/desviados.
        let reader = image::ImageReader::open(input_path)?
            .with_guessed_format()
            .map_err(|e| FormatError::DecoderFailure(format!("format guess: {e}")))?;
        let mut decoder = reader.into_decoder()?;
        // El ICC se extrae antes de entregar el decoder a from_decoder;
        // `icc_profile()` devuelve Ok(None) si el fuente no trae perfil.
        let icc_bytes: Option<Vec<u8>> = if profile.preserve_color_profile {
            decoder.icc_profile().ok().flatten()
        } else {
            None
        };
        let img = image::DynamicImage::from_decoder(decoder)?;
        let rgb = img.to_rgb8();

        let rgb = if let Some(resize) = profile.resize.as_ref() {
            crate::engine::optimization::apply_resize_rgb(&rgb, resize)?
        } else {
            rgb
        };

        let quality = profile.jpeg_quality.clamp(1, 100) as f32;
        let mut encoder = mozjpeg::compress::Compress::new(mozjpeg::ColorSpace::JCS_RGB);
        encoder.set_size(rgb.width() as usize, rgb.height() as usize);
        encoder.set_quality(quality);
        if profile.jpeg_progressive {
            encoder.set_progressive_mode();
        }
        encoder.set_optimize_coding(true);
        // El subsampling lo decide el backend inteligente (flag
        // `jpeg_chroma_444`), independiente de preservar el perfil ICC:
        // se pueden conservar el ICC y el 4:2:0 a la vez.
        if profile.jpeg_chroma_444 {
            // 4:4:4 — preserva el detalle de color en bordes finos,
            // texto y colores saturados. Coste: ~10-15% más grande.
            encoder.set_chroma_sampling_pixel_sizes((1, 1), (1, 1));
        } else {
            // 4:2:0 — descarta 75% del croma, ~10-15% más pequeño.
            // Prácticamente imperceptible en fotografía (la resolución
            // cromática del ojo humano es ~1/4 de la luminante).
            encoder.set_chroma_sampling_pixel_sizes((2, 2), (1, 1));
        }
        // Scans optimizadas para JPEG progresivo: mejor renderizado
        // progresivo al cargar por red.
        if profile.jpeg_progressive {
            encoder.set_optimize_scans(true);
        }

        let mut comp = encoder.start_compress(BufWriter::new(fs::File::create(output_path)?))?;

        // El ICC del fuente se re-embedea como APP2 (troceado si supera
        // 65533 bytes) para que el viewer aplique la transformación
        // correcta en vez de asumir sRGB. Con RemoveSafe se conserva: el
        // ICC es gestión de color, no metadata sensible (sin GPS ni
        // autor), igual que StripChunks::Safe en PNG. Con RemoveAll se
        // omite.
        let should_embed_icc = profile.preserve_color_profile
            && !matches!(
                profile.metadata_mode,
                crate::engine::optimization::MetadataMode::RemoveAll
            );
        if should_embed_icc {
            if let Some(icc) = icc_bytes.as_ref() {
                // `write_icc_profile` está en `CompressStarted`, no en
                // `Compress`, y debe llamarse antes de `write_scanlines`:
                // los markers APP preceden al SOF/SOS.
                comp.write_icc_profile(icc);
            }
        }

        comp.write_scanlines(&rgb)?;
        comp.finish()?;

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
            format: Format::Jpeg,
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
            Format::Jpeg,
        )
    }
}

impl JpegHandler {
    /// Re-encode JPEG→JPEG **a la calidad del original** (estilo
    /// `jpegtran -optimize -progressive`, con un matiz).
    ///
    /// Lee los planos de píxeles por componente (a su resolución
    /// submuestreada, sin re-escalar croma) y los re-encodea con las
    /// MISMAS quant tables y sampling del original, más Huffman
    /// óptimo y (opcional) scans progresivas.
    ///
    /// El resultado conserva la calidad perceptual del original
    /// (PSNR del roundtrip típicamente >65 dB, delta máximo ≤5 en
    /// ~0.2% de los píxeles) y recupera el espacio desperdiciado por
    /// entropy coding subóptimo — el caso más común en JPEGs de
    /// cámara y exports de editores.
    ///
    /// NO es bit-exacto: mozjpeg-rs 0.10 no expone
    /// `jpeg_read_coefficients`, así que el roundtrip pasa por
    /// IDCT→FDCT. El pipeline lo trata como candidato lossy (medido,
    /// no etiquetado).
    ///
    /// Limitaciones conocidas (devuelve `Err` y el caller cae al
    /// re-encode lossy normal):
    /// - Más de 3 componentes (CMYK/YCCK de algunos editores).
    /// - Componentes de croma con quant tables distintas entre sí
    ///   (raro; la API de mozjpeg solo expone tabla luma + chroma).
    pub fn reencode_source_quality(
        &self,
        input_path: &Path,
        output_path: &Path,
        profile: &OptimizationProfile,
    ) -> Result<OptimizationResult, FormatError> {
        if !input_path.exists() {
            return Err(FormatError::NotFound(input_path.display().to_string()));
        }
        // Defensa crítica: mozjpeg-rs convierte los errores de
        // libjpeg en panics que cruzan la frontera FFI; sobre un
        // archivo no-JPEG el proceso ABORTA silenciosamente. El
        // pipeline nunca enruta fuentes no-JPEG aquí, pero defensa
        // en profundidad: todo JPEG empieza con FF D8 FF.
        {
            let mut magic = [0u8; 3];
            let mut f = fs::File::open(input_path)?;
            use std::io::Read;
            f.read_exact(&mut magic)?;
            if magic != [0xFF, 0xD8, 0xFF] {
                return Err(FormatError::Unsupported(format!(
                    "reencode_source_quality requiere fuente JPEG; magic bytes no son JPEG"
                )));
            }
        }
        let start = Instant::now();
        let original_size = fs::metadata(input_path)?.len();

        // 1. Abrir el JPEG guardando los markers APP/COM para poder
        //    preservarlos según el metadata_mode.
        let dec = mozjpeg::Decompress::with_markers(mozjpeg::ALL_MARKERS)
            .from_path(input_path)
            .map_err(|e| FormatError::DecoderFailure(format!("transcode open: {e}")))?;

        // 2. Dimensiones/colorspace/markers se capturan ANTES de consumir
        //    `dec`. Las quant tables NO están disponibles todavía: libjpeg
        //    solo popula `comp_info[i].quant_table` cuando lee el SOS
        //    (dentro de start_decompress), así que se leen DESPUÉS de
        //    `.raw()` desde el DecompressStarted.
        let width = dec.width();
        let height = dec.height();
        let in_color_space = dec.color_space();

        // 3. Seleccionar markers a preservar según el metadata_mode.
        //    - Keep      → EXIF/XMP (APP1) + ICC (APP2) + COM.
        //    - RemoveSafe → solo ICC (APP2) — gestión de color, no
        //                   metadata sensible (sin GPS ni autor).
        //    - RemoveAll → nada.
        //    APP0 (JFIF) no se copia: libjpeg lo regenera solo al
        //    escribir, y duplicarlo corrompería el header.
        let keep_metadata = matches!(profile.metadata_mode, MetadataMode::Keep);
        let keep_color = !matches!(profile.metadata_mode, MetadataMode::RemoveAll);
        let markers_to_copy: Vec<(mozjpeg::Marker, Vec<u8>)> = dec
            .markers()
            .filter(|m| match m.marker {
                mozjpeg::Marker::APP(1) => keep_metadata,
                mozjpeg::Marker::APP(2) => keep_color,
                mozjpeg::Marker::COM => keep_metadata,
                _ => false,
            })
            .map(|m| (m.marker, m.data.to_vec()))
            .collect();

        // 4. Arrancar la descompresión raw (lee SOS → quant tables ya
        //    asociadas a cada componente).
        let mut raw = dec
            .raw()
            .map_err(|e| FormatError::DecoderFailure(format!("transcode raw: {e}")))?;

        // 5. Capturar quant tables + sampling + shapes del original. El
        //    transcode debe reutilizar las MISMAS tablas o los
        //    coeficientes cambiarían de significado. `qtable()` devuelve
        //    un QTable OWNED (copia los 64 coeficientes), así que el
        //    borrow de `raw.components()` termina inmediatamente.
        let component_info: Vec<(Option<mozjpeg::qtable::QTable>, (u8, u8), (usize, usize))> = raw
            .components()
            .iter()
            .map(|c| (c.qtable(), c.sampling(), (c.row_stride(), c.col_stride())))
            .collect();
        let src_component_count = component_info.len();
        if src_component_count == 0 || src_component_count > 3 {
            return Err(FormatError::Unsupported(
                "transcode lossless: número de componentes no soportado".into(),
            ));
        }
        let sampling: Vec<(u8, u8)> = component_info.iter().map(|(_, s, _)| *s).collect();
        let buffer_shapes: Vec<(usize, usize)> =
            component_info.iter().map(|(_, _, sh)| *sh).collect();
        // QTable no implementa Clone, pero `qtable()` ya devolvió un valor
        // OWNED — lo movemos fuera del tuple con into_iter().
        let qtables: Vec<Option<mozjpeg::qtable::QTable>> =
            component_info.into_iter().map(|(q, _, _)| q).collect();
        if qtables.iter().any(|q| q.is_none()) {
            return Err(FormatError::Unsupported(
                "transcode lossless: quant table no disponible".into(),
            ));
        }

        // Si Cb y Cr usan tablas distintas no podemos representarlo con
        // set_luma_qtable/set_chroma_qtable → fallback.
        if src_component_count == 3 {
            let (a, b) = (qtables[1].as_ref(), qtables[2].as_ref());
            if a != b {
                return Err(FormatError::Unsupported(
                    "transcode lossless: chroma con quant tables distintas".into(),
                ));
            }
        }

        // 6. Leer los coeficientes DCT crudos (todo el archivo).
        // `read_raw_data` hace push sobre los Vecs y debug-asserta que
        // la capacidad es suficiente — pre-reservamos el tamaño exacto
        // (row_stride × col_stride por componente).
        let mut buffers: Vec<Vec<u8>> = buffer_shapes
            .iter()
            .map(|(rs, cs)| Vec::with_capacity(rs * cs))
            .collect();
        {
            let mut buf_refs: Vec<&mut Vec<u8>> = buffers.iter_mut().collect();
            raw.read_raw_data(&mut buf_refs);
        }
        // Sanity: cada buffer debe tener exactamente el tamaño esperado.
        for ((rs, cs), buf) in buffer_shapes.iter().zip(buffers.iter()) {
            if buf.len() != rs * cs {
                return Err(FormatError::Corrupted(format!(
                    "transcode: coeficientes leídos {} != esperados {}",
                    buf.len(),
                    rs * cs
                )));
            }
        }

        // 7. Configurar el compresor raw con la MISMA estructura que el
        //    original: color space, sampling, quant tables.
        let mut comp = mozjpeg::Compress::new(in_color_space);
        comp.set_size(width, height);
        for (dst, (h, v)) in comp.components_mut().iter_mut().zip(sampling.iter()) {
            dst.h_samp_factor = i32::from(*h);
            dst.v_samp_factor = i32::from(*v);
        }
        if let Some(qt) = qtables[0].as_ref() {
            comp.set_luma_qtable(qt);
        }
        if src_component_count > 1 {
            if let Some(qt) = qtables[1].as_ref() {
                comp.set_chroma_qtable(qt);
            }
        }
        // Entropy coding óptimo: Huffman optimizado + (si el perfil lo
        // pide) progresivo con scans optimizadas por mozjpeg.
        comp.set_optimize_coding(true);
        if profile.jpeg_progressive {
            comp.set_progressive_mode();
            comp.set_optimize_scans(true);
        }
        comp.set_raw_data_in(true);

        let writer = BufWriter::new(fs::File::create(output_path)?);
        let mut started = comp.start_compress(writer)?;
        for (marker, data) in &markers_to_copy {
            started.write_marker(*marker, data);
        }
        let slices: Vec<&[u8]> = buffers.iter().map(|b| &b[..]).collect();
        if !started.write_raw_data(&slices) {
            return Err(FormatError::EncoderFailure(
                "transcode: write_raw_data falló".into(),
            ));
        }
        started.finish()?;

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
            format: Format::Jpeg,
            success: true,
            error: String::new(),
        })
    }
}
