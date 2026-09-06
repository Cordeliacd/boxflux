//! Búsqueda de la calidad óptima por imagen usando butteraugli como
//! métrica perceptual: escalera descendente de calidades + bisección
//! opcional entre el último q bueno y el primero malo. El target de
//! butteraugli es adaptativo por imagen (ruido lo relaja, gradientes lo
//! endurecen — ver `adaptive_butteraugli_target`). Coste: ~4-12 encodes
//! por formato (~2-3 s por MP).

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::engine::formats::{
    AvifHandler, Format, FormatError, FormatHandler, JpegHandler, WebpHandler,
};
use crate::engine::optimization::{OptimizationProfile, ProfileKind};

use super::cancel::CancellationToken;
use super::candidate::{Candidate, CandidateId, CandidateResult};
use super::goal::OptimizationGoal;
use super::original_cache::OriginalImageCache;
use super::profile::FileProfile;
use super::quality::{QualityEvaluator, QualityMetrics};

/// Escalera de calidades a probar (de alta a baja). El ganador es el
/// intento de menor tamaño que sigue bajo el target de butteraugli.
const QUALITY_LADDER: &[u8] = &[95, 85, 75, 65, 55, 45, 35];

/// Escalera del search AVIF: corta (rav1e cuesta segundos por encode)
/// y cubre el territorio de compresión extrema. Descendente como
/// QUALITY_LADDER para reutilizar el mismo early-stop: el primer rung
/// que excede el target marca la frontera.
const AVIF_QUALITY_LADDER: &[u8] = &[80, 60, 40, 20];

/// Escalera del search según el formato.
fn ladder_for(format: Format) -> &'static [u8] {
    match format {
        Format::Avif => AVIF_QUALITY_LADDER,
        _ => QUALITY_LADDER,
    }
}

/// De todos los intentos medidos `(quality, size, ba)`, el de MENOR
/// TAMAÑO cuyo butteraugli pasa el target. Desempate: menor ba (misma
/// carga de bytes → mejor calidad). Función pura.
///
/// El criterio "el q más bajo que pasa" solo es válido si la curva
/// tamaño/quality es monótona, y no siempre lo es: en contenido plano
/// la cuantización gruesa crea banding que cuesta bits y un q menor
/// puede producir un archivo MAYOR que un q superior.
fn select_best_passing(measured: &[(u8, u64, f64)], target: f64) -> Option<(u8, u64, f64)> {
    measured
        .iter()
        .filter(|(_, _, ba)| *ba < target)
        .min_by(|a, b| {
            a.1.cmp(&b.1).then(
                a.2.partial_cmp(&b.2)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
        })
        .copied()
}

/// Resultado de la búsqueda iterativa.
#[derive(Debug, Clone)]
pub struct IterativeSearchResult {
    /// CandidateId del candidato iterativo (para encajar en
    /// `CandidateResult`).
    pub candidate_id: CandidateId,
    pub format: Format,
    /// Calidad del intento ganador: el de menor tamaño cuyo butteraugli
    /// pasa el target.
    pub best_quality: u8,
    pub output_size: u64,
    /// butteraugli medido del output óptimo vs original.
    pub butteraugli: f64,
    /// Métricas de calidad completas (incluye SSIM/PSNR para el scorer).
    pub quality: QualityMetrics,
    pub output_path: PathBuf,
    /// Tiempo total de búsqueda (ms).
    pub processing_time_ms: u64,
    /// Número de calidades probadas.
    pub qualities_tried: u32,
    /// Target butteraugli usado (adaptativo por imagen).
    pub target_butteraugli: f64,
    /// `false` = target inalcanzable incluso a la máxima calidad: el
    /// resultado es el intento de máxima calidad con métricas reales.
    pub converged: bool,
}

impl IterativeSearchResult {
    /// Convierte el resultado a un `CandidateResult` para que el
    /// pipeline lo procese como cualquier otro candidato.
    pub fn into_candidate_result(self, candidate: &Candidate) -> CandidateResult {
        let label = if self.converged {
            format!(
                "{} iterative q{} ba={:.2}",
                self.format.as_str(),
                self.best_quality,
                self.butteraugli
            )
        } else {
            format!(
                "{} iterative q{} (target {:.2} no alcanzado) ba={:.2}",
                self.format.as_str(),
                self.best_quality,
                self.target_butteraugli,
                self.butteraugli
            )
        };
        CandidateResult {
            candidate: Candidate {
                quality: Some(self.best_quality),
                label,
                ..candidate.clone()
            },
            success: true,
            error: None,
            output_path: Some(self.output_path),
            output_size: self.output_size,
            processing_time_ms: self.processing_time_ms,
            backend_name: format!("IterativeSearch ({})", self.format.as_str()),
            score: None,
            quality: Some(self.quality),
        }
    }
}

/// Calcula el target de butteraugli **adaptativo por imagen**: el goal
/// define el base y el perfil lo ajusta — ruido alto lo relaja (×1.3,
/// el lossy actúa como denoiser), gradientes suaves (×0.8) y sombras
/// (×0.85) lo endurecen (banding visible), texto/UI (×0.75) también.
/// Clamp final a [0.5, 3.0].
pub fn adaptive_butteraugli_target(goal: &OptimizationGoal, profile: &FileProfile) -> f64 {
    let base = goal.butteraugli_target();
    if base == 0.0 {
        return 0.0; // Lossless: no lossy allowed
    }
    let mut target = base;
    if profile.noise_estimate > 0.4 {
        target *= 1.3;
    }
    if profile.complexity < 0.15 {
        target *= 0.8;
    }
    if profile.dark_region_ratio > 0.4 {
        target *= 0.85;
    }
    if profile.edge_density > 0.12 {
        target *= 0.75;
    }
    target.clamp(0.5, 3.0)
}

/// Ejecuta la búsqueda iterativa para un formato dado (JPEG, WebP, AVIF
/// — todos decodificables para medir butteraugli).
///
/// `output_path` es donde se escribe el archivo FINAL (el óptimo); los
/// intentos intermedios van a temporales hermanos. `dedup` es el cache
/// de encodes AVIF del run: cada rung AVIF consulta el cache antes de
/// codificar (mismos parámetros → mismos bytes) y publica los propios
/// para las anclas de waves posteriores.
///
/// Cada rung decide con butteraugli REAL contra una referencia preparada
/// una sola vez (`prepare_butteraugli_reference`); el SSIM completo se
/// computa UNA vez para el ganador y la publicación es un rename del
/// tmp ganador (el encoder es determinista).
pub fn iterative_quality_search(
    input: &Path,
    output_path: &Path,
    candidate: &Candidate,
    profile: &FileProfile,
    goal: &OptimizationGoal,
    cache: &OriginalImageCache,
    dedup: &super::encode_cache::EncodeDedupCache,
    token: &CancellationToken,
) -> Result<IterativeSearchResult, FormatError> {
    let start = Instant::now();
    let target = adaptive_butteraugli_target(goal, profile);
    let evaluator = QualityEvaluator::new();

    // Perfil base de codificación (chroma, metadata, etc.); la quality
    // se sobrescribe en cada intento. El chroma subsampling usa la
    // misma lógica adaptativa del `JpegBackend` (`is_photo_like`):
    // una foto debe recibir el mismo tratamiento por el path iterativo
    // y por el normal.
    let base_profile = build_encode_profile(candidate, profile);

    // Referencia butteraugli del original, preparada UNA vez (decode
    // full-res vía cache + Lanczos3 a ≤1024px); los rungs la reutilizan.
    let reference = evaluator.prepare_butteraugli_reference(cache)?;

    // Tmps creados durante la búsqueda: se limpian SIEMPRE al salir
    // (éxito o error) — el ganador se renombra, el resto se borra.
    let mut tmp_paths: Vec<PathBuf> = Vec::new();

    let result = (|| -> Result<IterativeSearchResult, FormatError> {
        let ladder = ladder_for(candidate.format);
        // El tmp NO se borra aquí: el ganador se publicará por rename.
        let encode_and_measure = |quality: u8,
                                  tmp_paths: &mut Vec<PathBuf>|
         -> Result<Option<(u64, f64)>, FormatError> {
            if token.is_cancelled() {
                return Ok(None);
            }
            let tmp = output_path.with_extension(format!("q{}.tmp", quality));
            let mut p = base_profile.clone();
            match candidate.format {
                Format::Jpeg => {
                    p.jpeg_quality = quality;
                    let handler = JpegHandler::new();
                    let result = handler.optimize(input, &tmp, &p)?;
                    let size = result.output_size;
                    // No tragarse un fallo del evaluador: aborta la
                    // búsqueda con error visible.
                    let ba = evaluator.butteraugli_against(&reference, &tmp)?;
                    tmp_paths.push(tmp);
                    Ok(Some((size, ba)))
                }
                Format::Webp => {
                    p.webp_quality = quality;
                    let handler = WebpHandler::new();
                    let result = handler.optimize(input, &tmp, &p)?;
                    let size = result.output_size;
                    let ba = evaluator.butteraugli_against(&reference, &tmp)?;
                    tmp_paths.push(tmp);
                    Ok(Some((size, ba)))
                }
                // AVIF: encode vía convert() (AvifHandler es write-only).
                // Dedupe: mismo (quality, budget, alpha, metadata,
                // resize) → mismos bytes que un ancla; si el cache los
                // tiene, copiar y medir; si no, codificar y publicarlos.
                Format::Avif => {
                    p.avif_quality = quality;
                    let key = super::encode_cache::avif_encode_key(&p);
                    let size = match dedup.lookup(&key, token) {
                        super::encode_cache::DedupLookup::Hit(cached, size) => {
                            std::fs::copy(&cached, &tmp)?;
                            std::fs::metadata(&tmp)
                                .map(|m| m.len())
                                .unwrap_or(size)
                        }
                        super::encode_cache::DedupLookup::Cancelled => {
                            return Ok(None);
                        }
                        super::encode_cache::DedupLookup::Produce => {
                            let handler = AvifHandler::new();
                            let result =
                                match handler.convert(input, &tmp, Format::Avif, &p) {
                                    Ok(r) => r,
                                    Err(e) => {
                                        // Liberar la clave: el siguiente
                                        // consumidor debe poder codificar.
                                        dedup.abandon(&key);
                                        return Err(e);
                                    }
                                };
                            let size = result.output_size;
                            // Copia propia del cache: el tmp puede ser
                            // renombrado (si gana) o borrado (si no).
                            dedup.store(&key, &tmp);
                            size
                        }
                    };
                    let ba = evaluator.butteraugli_against(&reference, &tmp)?;
                    tmp_paths.push(tmp);
                    Ok(Some((size, ba)))
                }
                _ => Ok(None),
            }
        };

        // Fase 1: escanear la escalera descendente. El primer q que
        // excede el target marca el límite. Si TODOS pasan, el q más
        // bajo es el óptimo; si NINGUNO pasa, el fallback es el q más
        // alto (mejor calidad) con sus métricas reales.
        let mut best_q: u8 = 95;
        let mut best_ba: f64 = f64::MAX;
        let mut have_best = false;
        // Primer intento fallido (métricas reales), por si ningún q
        // alcanza el target.
        let mut last_bad: Option<(u8, u64, f64)> = None;
        let mut last_bad_q: Option<u8> = None;
        let mut qualities_tried: u32 = 0;

        // El ganador es el intento de MENOR TAMAÑO cuyo ba pasa el
        // target (desempate: menor ba), NO el de menor calidad: la
        // curva tamaño/quality no es monótona en contenido plano. La
        // bisección sigue usando el MENOR q que pasa como frontera
        // (hi) — es lo correcto para explorar el límite de calidad.
        let mut measured: Vec<(u8, u64, f64)> = Vec::new();
        // Menor calidad que ha pasado el target (frontera de la
        // bisección, NO necesariamente el ganador).
        let mut lowest_passing_q: u8 = 95;

        for &q in ladder {
            if token.is_cancelled() {
                break;
            }
            qualities_tried += 1;
            match encode_and_measure(q, &mut tmp_paths)? {
                None => break, // cancelado o formato no soportado
                Some((size, ba)) => {
                    measured.push((q, size, ba));
                    if ba < target {
                        // Este q pasa el gate perceptual: nueva frontera
                        // baja de la bisección. Seguir bajando por si
                        // cabe más.
                        lowest_passing_q = q;
                        have_best = true;
                    } else {
                        // Este q excede el target — la imagen empieza a
                        // degradarse. Marcar y dejar de bajar.
                        if last_bad.is_none() {
                            last_bad = Some((q, size, ba));
                        }
                        last_bad_q = Some(q);
                        break;
                    }
                }
            }
        }

        // Fase 2 (opcional): bisección entre lowest_passing_q (último
        // bueno) y last_bad_q (primero malo) — puede haber un q
        // intermedio que aún pase el target.
        if let (Some(bad_q), true) = (last_bad_q, have_best) {
            // Solo bisectar si hay espacio (frontera > bad_q+5).
            if lowest_passing_q > bad_q + 5 {
                let mut lo = bad_q;
                let mut hi = lowest_passing_q;
                for _ in 0..3 {
                    if token.is_cancelled() {
                        break;
                    }
                    let mid = (lo + hi) / 2;
                    if mid == lo || mid == hi {
                        break;
                    }
                    qualities_tried += 1;
                    // Un fallo del evaluador durante la bisección conserva el best
                    // ya convergado en vez de descartarlo.
                    let attempt = match encode_and_measure(mid, &mut tmp_paths) {
                        Ok(v) => v,
                        Err(_) => break,
                    };
                    match attempt {
                        None => break,
                        Some((size, ba)) => {
                            measured.push((mid, size, ba));
                            if ba < target {
                                // mid pasa: estrechar `hi` (el tracking de
                                // lowest_passing_q vive en Fase 1).
                                hi = mid;
                            } else {
                                lo = mid;
                            }
                        }
                    }
                }
            }
        }

        // Target inalcanzable incluso a la máxima calidad: devolver el
        // intento de máxima calidad con sus métricas reales. El ganador
        // sale de TODOS los rungs medidos por el criterio min-tamaño.
        let converged = have_best;
        if let Some((q, _size, ba)) = select_best_passing(&measured, target) {
            best_q = q;
            best_ba = ba;
        } else if !have_best {
            if let Some((q, _size, ba)) = last_bad {
                best_q = q;
                best_ba = ba;
            }
        }
        // Sin medición disponible (búsqueda cancelada antes del primer
        // intento, o formato no soportado): NO fabricar lossless —
        // fallar honesto. El pipeline cae a su fallback del rango del
        // strategy y el candidato se mide por el path normal.
        if best_ba == f64::MAX || (!have_best && last_bad.is_none()) {
            return Err(FormatError::Unsupported(format!(
                "iterative search no pudo medir ningún quality ({})",
                candidate.format.as_str()
            )));
        }

        if token.is_cancelled() {
            return Err(FormatError::Unsupported("cancelled".into()));
        }

        // Publicar el ganador: el tmp ya contiene los bytes correctos
        // (encoder determinista) — renombrar es gratis; re-encodear
        // costaría un encode completo.
        let winner_tmp = output_path.with_extension(format!("q{}.tmp", best_q));
        // Métricas completas (SSIM/MSE/PSNR) del ganador: UNA sola
        // evaluación al final.
        let mut quality_metrics = evaluator.evaluate_with_cache(cache, &winner_tmp, false)?;
        // Una sola fuente de verdad para butteraugli: el valor medido
        // que decidió la búsqueda.
        quality_metrics.butteraugli = Some(best_ba);

        let rename_ok = winner_tmp != *output_path
            && std::fs::rename(&winner_tmp, output_path).is_ok();
        if !rename_ok {
            // Rename fallido (p. ej. cruzando dispositivos, aunque tmp y
            // output son hermanos): re-encode al best_q.
            let mut p = base_profile.clone();
            match candidate.format {
                Format::Jpeg => {
                    p.jpeg_quality = best_q;
                    JpegHandler::new().optimize(input, output_path, &p)?;
                }
                Format::Webp => {
                    p.webp_quality = best_q;
                    WebpHandler::new().optimize(input, output_path, &p)?;
                }
                Format::Avif => {
                    p.avif_quality = best_q;
                    AvifHandler::new().convert(input, output_path, Format::Avif, &p)?;
                }
                _ => {
                    return Err(FormatError::Unsupported(format!(
                        "iterative search no soporta {}",
                        candidate.format.as_str()
                    )));
                }
            }
        }
        let final_size = std::fs::metadata(output_path).map(|m| m.len()).unwrap_or(0);

        Ok(IterativeSearchResult {
            candidate_id: candidate.id,
            format: candidate.format,
            best_quality: best_q,
            output_size: final_size,
            butteraugli: best_ba,
            quality: quality_metrics,
            output_path: output_path.to_path_buf(),
            processing_time_ms: start.elapsed().as_millis() as u64,
            qualities_tried,
            target_butteraugli: target,
            converged,
        })
    })();
    // Cleanup garantizado: borrar todo tmp no publicado (tras un rename
    // exitoso, winner_tmp ya no existe y el remove se ignora).
    for p in &tmp_paths {
        let _ = std::fs::remove_file(p);
    }
    result
}

/// Perfil base para la codificación iterativa (chroma, metadata, etc.),
/// derivado del candidato; la quality se sobrescribe en cada intento.
/// El chroma subsampling se decide con la misma lógica del
/// `JpegBackend` (fotos → 4:2:0, sintéticas → 4:4:4) vía
/// `FileProfile::is_photo_like()` — una sola fuente de verdad.
///
/// `avif_time_budget_ms` se propaga desde el candidato: el search debe
/// codificar con el mismo budget (y por tanto el mismo speed de rav1e)
/// que el path de producción, o sus rungs no corresponderían a los
/// bytes que el encoder de producción emitiría.
fn build_encode_profile(candidate: &Candidate, profile: &FileProfile) -> OptimizationProfile {
    OptimizationProfile {
        name: candidate.label.clone(),
        kind: ProfileKind::Custom,
        jpeg_quality: 82, // overriden per-attempt
        jpeg_progressive: true,
        webp_quality: 80, // overriden per-attempt
        webp_lossless: false,
        avif_quality: 60,
        avif_alpha_quality: 80,
        avif_time_budget_ms: candidate.backend_time_budget_ms,
        png_optimization_level: 3,
        metadata_mode: candidate.metadata_mode,
        preserve_color_profile: candidate.preserve_color_profile,
        jpeg_chroma_444: !profile.is_photo_like(),
        resize: candidate.resize,
    }
}
