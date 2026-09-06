//! Handler de PNG: recompresión lossless delegada en [`oxipng`]
//! (selección de filtros + DEFLATE/Zopfli).

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use crate::engine::conversion::ConversionResult;
use crate::engine::formats::{Format, FormatCapabilities, FormatError, FormatHandler, FormatInfo};
use crate::engine::optimization::{OptimizationProfile, OptimizationResult};

/// Handler de PNG.
///
/// Las iteraciones de zopfli se dimensionan con una función pura de
/// (raw_size × budget): un deadline de reloj de pared cortaría entre
/// trials según la carga y haría los bytes de salida no reproducibles.
/// El coste de zopfli escala con los bytes RAW del scanline, no con el
/// tamaño comprimido del archivo.
pub struct PngHandler {
    /// Presupuesto (ms) para dimensionar las iteraciones de zopfli.
    /// `None` = default interno (30 s). El pipeline lo fija con el
    /// presupuesto de tiempo del candidato.
    zopfli_budget: Option<std::time::Duration>,
}

impl PngHandler {
    pub fn new() -> Self {
        Self { zopfli_budget: None }
    }

    /// Construye un handler cuyo zopfli se dimensiona al presupuesto
    /// dado. Lo usa el `PngBackend` para respetar el presupuesto de
    /// tiempo del candidato (`Candidate::backend_time_budget_ms`).
    pub fn with_zopfli_budget(budget: std::time::Duration) -> Self {
        Self {
            zopfli_budget: Some(budget),
        }
    }
}

impl Default for PngHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Coste de zopfli por MB raw y iteración (peor caso medido con las
/// opciones exactas del engine: level 6, 10 filtros, todas las
/// reducciones). El coste NO es lineal en iteraciones — hay un coste
/// fijo por bloque (~40%) y la 1ª iteración es la más cara, así que la
/// constante debe cubrir el caso de 1 iteración. Varía ~2.3× con la
/// compresibilidad del dato; usamos el peor caso para que el plan
/// nunca exceda el presupuesto.
///
/// El raw del IHDR es PRE-reducción: imágenes que oxipng reduce a
/// paleta procesan menos bytes de los estimados y se les niega zopfli
/// antes de tiempo. Preferimos subestimar zopfli (3-8% peor deflate)
/// a arriesgar minutos por imagen.
const ZOPFLI_MS_PER_MB_ITER: u64 = 8_000;

/// Tope de iteraciones de zopfli.
const ZOPFLI_MAX_ITERATIONS: u8 = 15;

/// Presupuesto por defecto cuando nadie especifica uno: 30 s.
const ZOPFLI_DEFAULT_BUDGET_MS: u64 = 30_000;

/// Iteraciones de zopfli que caben en el presupuesto. Función pura de
/// (raw_size, budget_ms): nunca consulta el reloj. `0` = zopfli no
/// cabe → el caller mantiene libdeflater-12 (rápido, determinista,
/// 3-8% peor).
///
/// Modelo: wall_ms ≈ ZOPFLI_MS_PER_MB_ITER × iters × raw_mb.
fn zopfli_iterations_for(raw_size: u64, budget_ms: u64) -> u8 {
    let budget_ms = if budget_ms == 0 {
        ZOPFLI_DEFAULT_BUDGET_MS
    } else {
        budget_ms
    };
    let raw_mb = (raw_size as f64 / 1_000_000.0).max(0.01);
    let iters = budget_ms as f64 / (ZOPFLI_MS_PER_MB_ITER as f64 * raw_mb);
    iters.floor().clamp(0.0, ZOPFLI_MAX_ITERATIONS as f64) as u8
}

/// Tamaño raw (bytes de scanline sin comprimir) de un PNG, leído
/// directamente del IHDR. El coste de zopfli escala con ESTE tamaño
/// (no con el tamaño comprimido del archivo).
///
/// Formato: 8 B firma + 4 B longitud + 4 B "IHDR" + 4 B width BE +
/// 4 B height BE + 1 B bit depth + 1 B color type.
fn png_raw_size_from_ihdr(path: &Path) -> Option<u64> {
    use std::io::Read;
    let mut f = fs::File::open(path).ok()?;
    let mut header = [0u8; 26];
    f.read_exact(&mut header).ok()?;
    // Firma PNG
    if header[0..8] != [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        return None;
    }
    if &header[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes([header[16], header[17], header[18], header[19]]) as u64;
    let height = u32::from_be_bytes([header[20], header[21], header[22], header[23]]) as u64;
    let bit_depth = header[24] as u64;
    let channels: u64 = match header[25] {
        0 => 1, // grayscale
        2 => 3, // RGB
        3 => 1, // palette index
        4 => 2, // gray+alpha
        6 => 4, // RGBA
        _ => return None,
    };
    if width == 0 || height == 0 || bit_depth == 0 || bit_depth > 16 {
        return None;
    }
    Some(width * height * channels * ((bit_depth + 7) / 8))
}

impl FormatHandler for PngHandler {
    fn format(&self) -> Format {
        Format::Png
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
            format: Format::Png,
            width: dims.0,
            height: dims.1,
            file_size,
            has_alpha: true, // asumido; una sonda del IHDR lo afinaría
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

        // Path lossy opcional (imagequant, el motor de pngquant):
        // cuantiza la paleta 24→8 bit antes del deflate de oxipng. El
        // resultado lossy compite contra el LOSSLESS (que se escribe
        // abajo en `output_path`), no contra el original — uno menor
        // que el original pero mayor que el oxipng no debe ganar.
        //
        // LICENCIA: imagequant es GPL-3.0; solo se compila con
        // `--features png-lossy`. Distribución cerrada requiere
        // licencia comercial.
        #[cfg(feature = "png-lossy")]
        {
            if should_try_lossy_png(profile) {
                // La competición real ocurre tras el paso lossless, abajo.
            }
        }

        let needs_resize = profile
            .resize
            .as_ref()
            .map(|r| r.target_width.is_some() || r.target_height.is_some())
            .unwrap_or(false);

        // Con resize, la imagen reescalada se estaciona en un `.tmp.png`
        // hermano antes de que oxipng la recomprima a `output_path`.
        // Hay que eliminarlo incluso si oxipng falla: el barrido de
        // temporales del engine NO cubre este patrón y un descuido lo
        // dejaría huérfano. El `ScopedRemove` cubre todos los caminos
        // de salida (éxito, error y `?`).
        let staging_path: Option<std::path::PathBuf> = if needs_resize {
            // Extensión PNG para que `image::save` elija encoder PNG;
            // `*.png.tmp` se interpretaría como formato `tmp` desconocido.
            let tmp = output_path.with_extension("tmp.png");
            let img = image::open(input_path)?;
            let resized = crate::engine::optimization::apply_resize(img, profile.resize.as_ref())?;
            resized.save(&tmp)?;
            Some(tmp)
        } else if input_path != output_path {
            fs::copy(input_path, output_path)?;
            None
        } else {
            None
        };

        // El guard se bindea antes de llamar a oxipng: cualquier `?`
        // intermedio también lo dropea. En éxito también borra el staging
        // — oxipng lee de `working_path` y escribe a `output_path`, así
        // que el staging no se consume solo.
        let _staging_guard = ScopedRemove::new(staging_path.as_deref());

        let working_path = staging_path.as_deref().unwrap_or_else(|| {
            if input_path != output_path {
                output_path
            } else {
                input_path
            }
        });

        // Recompresión lossless con oxipng. El raw size que dimensiona el
        // zopfli es el del archivo que oxipng va a leer (el staging si hay
        // resize). IHDR ilegible → fallback al tamaño comprimido; oxipng
        // fallará poco después con su propio error de parseo.
        let raw_size = png_raw_size_from_ihdr(working_path).unwrap_or(original_size);
        let opts = build_oxipng_options(
            profile,
            raw_size,
            self.zopfli_budget.map(|d| d.as_millis() as u64),
        );
        let in_file = oxipng::InFile::from(working_path.to_path_buf());
        let out_file = oxipng::OutFile::from_path(output_path.to_path_buf());
        oxipng::optimize(&in_file, &out_file, &opts)
            .map_err(|e| FormatError::EncoderFailure(format!("oxipng: {e}")))?;

        // El metadata strip ya quedó configurado en build_oxipng_options.

        // `mut` solo se usa en el path `png-lossy` (feature opcional).
        #[cfg_attr(not(feature = "png-lossy"), allow(unused_mut))]
        let mut output_size = fs::metadata(output_path)?.len();

        // Path lossy (solo con `png-lossy`): se publica únicamente si
        // supera al lossless; el empate favorece al lossless.
        #[cfg(feature = "png-lossy")]
        {
            if should_try_lossy_png(profile) {
                let lossy_tmp = output_path.with_extension("liq.tmp.png");
                match try_lossy_png_optimize(input_path, &lossy_tmp, profile) {
                    Ok(lossy_result) if lossy_result.output_size < output_size => {
                        // El lossy ganó — lo publicamos atómicamente.
                        if std::fs::rename(&lossy_tmp, output_path).is_ok() {
                            output_size = lossy_result.output_size;
                        } else {
                            let _ = std::fs::remove_file(&lossy_tmp);
                        }
                    }
                    Ok(_) => {
                        let _ = std::fs::remove_file(&lossy_tmp);
                    }
                    Err(_) => {
                        let _ = std::fs::remove_file(&lossy_tmp);
                    }
                }
            }
        }
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
            format: Format::Png,
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
            Format::Png,
        )
    }
}

fn build_oxipng_options(
    profile: &OptimizationProfile,
    raw_size: u64,
    zopfli_budget_ms: Option<u64>,
) -> oxipng::Options {
    let mut opts = oxipng::Options::from_preset(profile.png_optimization_level.min(6));
    match profile.metadata_mode {
        crate::engine::optimization::MetadataMode::Keep => {
            opts.strip = oxipng::StripChunks::None;
        }
        crate::engine::optimization::MetadataMode::RemoveSafe => {
            // Keep color management, strip everything else.
            opts.strip = oxipng::StripChunks::Safe;
        }
        crate::engine::optimization::MetadataMode::RemoveAll => {
            opts.strip = oxipng::StripChunks::All;
        }
    }

    // La feature `zopfli` de oxipng solo añade el enum variant; no lo
    // selecciona. `from_preset(N)` siempre usa Libdeflater, así que hay
    // que fijar Zopfli explícitamente. Solo en nivel 6: zopfli es
    // 10-100× más lento.
    //
    // Las iteraciones son función pura de (raw_size, budget) para que
    // el resultado sea reproducible y no exceda el presupuesto. Si ni
    // 1 iteración cabe, mantenemos el Libdeflater{12} del preset.
    if profile.png_optimization_level >= 6 {
        let budget_ms = zopfli_budget_ms.unwrap_or(0);
        let iterations = zopfli_iterations_for(raw_size, budget_ms);
        if iterations > 0 {
            opts.deflate = oxipng::Deflaters::Zopfli {
                iterations: std::num::NonZeroU8::new(iterations).unwrap(),
            };
        }
        // iterations == 0 → mantiene el Libdeflater{compression: 12}
        // del preset.
    }

    // Reducciones explícitas: son `true` por defecto en oxipng 9, pero
    // las fijamos para que un cambio de default futuro no degrade la
    // compresión en silencio. Son la fuente de los ahorros grandes
    // (bit depth 8→1/2/4, RGBA→RGB/Palette, dedupe de paleta,
    // re-deflate del IDAT).
    opts.bit_depth_reduction = true;
    opts.color_type_reduction = true;
    opts.palette_reduction = true;
    opts.grayscale_reduction = true;
    opts.idat_recoding = true;

    // Reescribe el RGB de los píxeles con alpha=0 para favorecer el row
    // filtering: no cambia ningún píxel visible y comprime mejor.
    opts.optimize_alpha = true;

    // from_preset(5/6) ya prueba todos los filtros, pero lo fijamos
    // explícitamente para perfiles custom con level cambiado.
    if profile.png_optimization_level >= 5 {
        opts.filter = [
            oxipng::RowFilter::None,
            oxipng::RowFilter::Sub,
            oxipng::RowFilter::Up,
            oxipng::RowFilter::Average,
            oxipng::RowFilter::Paeth,
            oxipng::RowFilter::MinSum,
            oxipng::RowFilter::Entropy,
            oxipng::RowFilter::Bigrams,
            oxipng::RowFilter::BigEnt,
            oxipng::RowFilter::Brute,
        ]
        .iter()
        .copied()
        .collect();
        opts.fast_evaluation = false;
    }

    // Válvula de emergencia: las iteraciones ya caben en el presupuesto
    // por diseño, así que este timeout solo corta ante contenido mucho
    // peor de lo modelado. 3× el presupuesto — nunca dispara en
    // operación normal y evita tirar trabajo ya terminado.
    if profile.png_optimization_level >= 6 {
        let budget_ms = zopfli_budget_ms.unwrap_or(ZOPFLI_DEFAULT_BUDGET_MS);
        opts.timeout = Some(Duration::from_millis(
            budget_ms.saturating_mul(3).max(ZOPFLI_DEFAULT_BUDGET_MS),
        ));
        // Escala 16→8 bit por canal: lossy en profundidad, visualmente
        // lossless en monitores 8-bit, mitad de tamaño. Solo en
        // MaximumCompression — el resto de modos conserva 16-bit.
        opts.scale_16 = true;
    }

    opts
}

// Path lossy PNG: cuantización de paleta con imagequant (GPL-3.0,
// solo con `--features png-lossy`). Si falla, el caller cae al path
// lossless de oxipng.

/// Decide si intentar el path lossy PNG: Lossless nunca; el resto,
/// según quality.
#[cfg(feature = "png-lossy")]
fn should_try_lossy_png(profile: &OptimizationProfile) -> bool {
    use crate::engine::optimization::ProfileKind;
    match profile.kind {
        ProfileKind::Lossless => false,
        ProfileKind::Web | ProfileKind::Custom => profile.jpeg_quality < 90,
    }
}

/// Intenta optimizar el PNG via cuantización de paleta (pngquant-style).
///
/// Decodifica el PNG a RGBA8, cuantiza a ≤256 colores con imagequant,
/// escribe un PNG indexed, lo pasa por oxipng, y devuelve el resultado.
/// Si falla en cualquier paso, devuelve Err (el caller cae a oxipng
/// lossless normal).
#[cfg(feature = "png-lossy")]
fn try_lossy_png_optimize(
    input_path: &Path,
    output_path: &Path,
    profile: &OptimizationProfile,
) -> Result<crate::engine::optimization::OptimizationResult, FormatError> {
    use crate::engine::optimization::OptimizationResult;
    use imagequant::RGBA;
    use std::io::BufWriter;
    use std::time::Instant;

    let start = Instant::now();
    let original_size = std::fs::metadata(input_path)?.len();

    let img = image::open(input_path)?;
    let rgba8 = img.to_rgba8();
    let (w, h) = (rgba8.width() as usize, rgba8.height() as usize);
    // Si algún píxel tiene alpha < 255, la paleta debe preservar el
    // canal alpha (chunk tRNS); si no, los semi-transparentes se
    // aplanan.
    let has_real_alpha = rgba8.pixels().any(|p| p[3] != 255);
    // imagequant requiere un Vec<RGBA> (no &[u8]).
    let pixels: Vec<RGBA> = rgba8
        .pixels()
        .map(|p| RGBA::new(p[0], p[1], p[2], p[3]))
        .collect();

    let mut liq = imagequant::new();
    liq.set_speed(5) // 1=best, 11=fastest. 5=balanced.
        .map_err(|e| FormatError::EncoderFailure(format!("imagequant speed: {e}")))?;
    // set_quality(MIN, MAX): bajo MIN → error (demasiada pérdida);
    // sobre MAX → para temprano. Min = q (no aceptar peor), max = 100
    // (buscar la mejor paleta). `jpeg_quality` es el dial de calidad
    // general del perfil.
    let q = profile.jpeg_quality.clamp(1, 100) as i32;
    liq.set_quality(q, 100)
        .map_err(|e| FormatError::EncoderFailure(format!("imagequant quality: {e}")))?;
    let mut img_liq = liq
        .new_image(&pixels[..], w, h, 0.0)
        .map_err(|e| FormatError::EncoderFailure(format!("imagequant new_image: {e}")))?;
    let mut res = liq
        .quantize(&mut img_liq)
        .map_err(|e| FormatError::EncoderFailure(format!("imagequant quantize: {e}")))?;
    res.set_dithering_level(1.0)
        .map_err(|e| FormatError::EncoderFailure(format!("imagequant dithering: {e}")))?;
    let (palette, indices) = res
        .remapped(&mut img_liq)
        .map_err(|e| FormatError::EncoderFailure(format!("imagequant remapped: {e}")))?;

    let tmp_path = output_path.with_extension("tmp.png");
    {
        let file = std::fs::File::create(&tmp_path)?;
        let ref mut writer = BufWriter::new(file);
        let mut encoder = png::Encoder::new(writer, w as u32, h as u32);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(png::BitDepth::Eight);
        // Con transparencia real, el chunk tRNS lleva el alpha de cada
        // entrada de la paleta; sin él los semi-transparentes se
        // aplanan y la imagen sale corrupta.
        let palette_rgb: Vec<u8> = palette.iter().flat_map(|c| [c.r, c.g, c.b]).collect();
        encoder.set_palette(palette_rgb);
        if has_real_alpha {
            let trns: Vec<u8> = palette.iter().map(|c| c.a).collect();
            encoder.set_trns(trns);
        }
        let mut writer = encoder
            .write_header()
            .map_err(|e| FormatError::EncoderFailure(format!("png write_header: {e}")))?;
        writer
            .write_image_data(&indices)
            .map_err(|e| FormatError::EncoderFailure(format!("png write_image_data: {e}")))?;
    }

    // El raw size para el zopfli es el del PNG indexed recién escrito
    // (el coste escala con el scanline, no con el original).
    let raw_size = png_raw_size_from_ihdr(&tmp_path).unwrap_or(original_size);
    let opts = build_oxipng_options(profile, raw_size, None);
    let in_file = oxipng::InFile::from(tmp_path.clone());
    let out_file = oxipng::OutFile::from_path(output_path.to_path_buf());
    let _ = oxipng::optimize(&in_file, &out_file, &opts);
    let _ = std::fs::remove_file(&tmp_path);

    let output_size = std::fs::metadata(output_path)?.len();
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
        format: Format::Png,
        success: true,
        error: String::new(),
    })
}

/// Guard RAII que borra un archivo al dropearse.
///
/// Asegura la limpieza del staging de resize incluso cuando oxipng
/// falla o un `?` cortocircuita. Los fallos de `remove_file` se
/// tragan: NotFound es esperado; el resto se loguea sin enmascarar
/// el error real. Es un conserje defensivo, no un sustituto de la
/// capa de salida atómica del engine.
struct ScopedRemove {
    path: Option<std::path::PathBuf>,
}

impl ScopedRemove {
    /// Guard para `path`; con `None` es un no-op.
    fn new(path: Option<&Path>) -> Self {
        Self {
            path: path.map(|p| p.to_path_buf()),
        }
    }
}

impl Drop for ScopedRemove {
    fn drop(&mut self) {
        if let Some(p) = self.path.as_ref() {
            if let Err(e) = fs::remove_file(p) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    eprintln!("boxflux: scoped remove failed for {}: {e}", p.display());
                }
            }
        }
    }
}
