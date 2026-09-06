//! Queue & job commands.

use std::path::PathBuf;

use tauri::{AppHandle, Emitter, State};

use crate::app_state::AppState;
use crate::engine::formats::Format;
use crate::fs;
use crate::queue::job::{Job, JobOperation, OptimizationMode};

#[tauri::command]
pub fn get_queue_snapshot(state: State<'_, AppState>) -> Vec<Job> {
    state.queue.snapshot()
}

#[tauri::command]
pub fn get_queue_stats(state: State<'_, AppState>) -> crate::queue::QueueStats {
    state.queue.stats()
}

#[tauri::command]
pub fn start_queue(state: State<'_, AppState>) {
    state.queue.start();
}

#[tauri::command]
pub fn pause_queue(state: State<'_, AppState>) {
    state.queue.pause();
}

#[tauri::command]
pub fn resume_queue(state: State<'_, AppState>) {
    state.queue.resume();
}

/// Emite `job-updated` + `stats-changed` tras cualquier mutación de
/// la cola (completar, cancelar, reintentar, eliminar) para que el
/// frontend no dependa de polling.
fn emit_queue_changed(app: &AppHandle, state: &AppState, job: Option<&Job>) {
    if let Some(job) = job {
        let _ = app.emit("boxflux://job-updated", job.clone());
    }
    let _ = app.emit("boxflux://stats-changed", state.queue.stats());
}

#[tauri::command]
pub fn cancel_job(job_id: u64, app_handle: AppHandle, state: State<'_, AppState>) -> bool {
    let ok = state.queue.cancel(job_id);
    if ok {
        if let Some(job) = state.queue.snapshot().into_iter().find(|j| j.id == job_id) {
            emit_queue_changed(&app_handle, &state, Some(&job));
        } else {
            emit_queue_changed(&app_handle, &state, None);
        }
    }
    ok
}

#[tauri::command]
pub fn retry_job(job_id: u64, app_handle: AppHandle, state: State<'_, AppState>) -> Option<u64> {
    let new_id = state.queue.retry(job_id);
    if new_id.is_some() {
        emit_queue_changed(&app_handle, &state, None);
    }
    new_id
}

#[tauri::command]
pub fn remove_job(job_id: u64, app_handle: AppHandle, state: State<'_, AppState>) -> bool {
    let ok = state.queue.remove(job_id);
    if ok {
        emit_queue_changed(&app_handle, &state, None);
    }
    ok
}

#[tauri::command]
pub fn clear_completed(app_handle: AppHandle, state: State<'_, AppState>) {
    state.queue.clear_completed();
    emit_queue_changed(&app_handle, &state, None);
}

#[derive(Debug, serde::Deserialize)]
pub struct EnqueueOptimizeArgs {
    pub source: String,
    /// Modo de optimización: "Quality" o "Compress".
    /// El motor inteligente usa este modo para decidir el goal interno
    /// y elegir la mejor optimización según el análisis de la imagen.
    pub mode: String,
    /// Formato de salida forzado (modo Manual).
    ///
    /// `None` o string vacío → modo Auto: el motor evalúa todos los formatos
    /// disponibles y selecciona el de menor peso en bytes según el goal.
    ///
    /// `Some("PNG" | "JPEG" | "WebP" | "AVIF")` → modo Manual: el motor
    /// restringe los candidatos a ese formato y no realiza conversiones
    /// cruzadas. La extensión del archivo final coincidirá con el formato
    /// seleccionado.
    #[serde(default)]
    pub force_format: Option<String>,
    /// Si es `true`, fuerza salida lossless sin importar el goal.
    #[serde(default)]
    pub force_lossless: bool,
    /// Si es `true`, elimina toda la metadata (EXIF, XMP, ICC).
    #[serde(default)]
    pub strip_all_metadata: bool,
    /// Si es `true`, preserva el perfil de color ICC del original.
    /// Por defecto `true` para evitar alteraciones en tonos oscuros.
    #[serde(default = "default_preserve_color_profile")]
    pub preserve_color_profile: bool,
}

fn default_preserve_color_profile() -> bool {
    true
}

/// Parsea el string `force_format` del frontend a `Option<Format>`.
fn parse_force_format(s: &Option<String>) -> Option<Format> {
    let s = s.as_deref()?.trim();
    if s.is_empty() {
        return None;
    }
    let s = s.trim_start_matches('.');
    s.parse::<Format>().ok()
}

#[tauri::command]
pub fn enqueue_optimize_job(
    args: EnqueueOptimizeArgs,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<u64, String> {
    let source = PathBuf::from(&args.source);
    let validation = fs::validate_input_file(&source, 0);
    if !validation.is_ok() {
        return Err(validation.message);
    }
    let input_format = Format::from_path(&source);
    let settings = state.settings.get();
    // Si el usuario forzó un formato, usamos su extensión canónica para
    // pre-calcular el destination_path. Esto NO significa que el pipeline
    // no pueda elegir otro formato (puede descartar el forzado si la imagen
    // no es compatible, p.ej. JPEG no soporta alpha) — la validación final
    // por magic bytes garantiza que la extensión siempre coincida con el
    // contenido real.
    let forced_format = parse_force_format(&args.force_format);
    let target_ext = forced_format.map(|f| f.canonical_extension());
    let dest = fs::compute_output_path(
        &source,
        match settings.output_mode {
            crate::settings::OutputMode::SameFolder => "same_folder",
            crate::settings::OutputMode::OutputSubfolder => "output_subfolder",
            crate::settings::OutputMode::CustomFolder => "custom_folder",
        },
        Some(&settings.custom_output_folder),
        settings.preserve_folder_structure,
        &settings.filename_template,
        target_ext,
    );
    if dest == source {
        return Err("la ruta de salida coincide con la de entrada".into());
    }
    // Parsear el modo de optimización.
    let mode = match args.mode.as_str() {
        "Quality" | "quality" => OptimizationMode::Quality,
        "Compress" | "compress" => OptimizationMode::Compress,
        "ExtremeLightweight" | "extremelightweight" => OptimizationMode::ExtremeLightweight,
        _ => OptimizationMode::Quality, // default
    };
    let job = Job {
        operation: JobOperation::Optimize,
        source_path: source.to_string_lossy().to_string(),
        destination_path: dest.to_string_lossy().to_string(),
        input_format,
        output_format: input_format, // se actualizará tras la decisión del motor
        optimization_mode: mode,
        force_format: forced_format,
        force_lossless: args.force_lossless,
        strip_all_metadata: args.strip_all_metadata,
        preserve_color_profile: args.preserve_color_profile,
        ..Default::default()
    };
    let id = state.queue.enqueue(job);
    // Notificar al frontend inmediatamente.
    emit_queue_changed(&app_handle, &state, None);
    Ok(id)
}

/// Recibe una lista de paths arrastrados a la ventana. Para cada uno:
/// - Si es archivo, lo encola como optimize job en modo Quality.
/// - Si es carpeta, descubre todos los archivos soportados
///   recursivamente y los encola.
/// Devuelve el número de jobs encolados.
///
/// El escaneo recursivo corre en spawn_blocking para no congelar
/// la UI.
#[tauri::command]
pub async fn add_dropped_paths(
    paths: Vec<String>,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<u64, String> {
    let queue = std::sync::Arc::clone(&state.queue);
    let settings = state.settings.get();
    let output_mode = match settings.output_mode {
        crate::settings::OutputMode::SameFolder => "same_folder",
        crate::settings::OutputMode::OutputSubfolder => "output_subfolder",
        crate::settings::OutputMode::CustomFolder => "custom_folder",
    };
    let custom_folder = settings.custom_output_folder.clone();
    let preserve_structure = settings.preserve_folder_structure;
    let filename_template = settings.filename_template.clone();

    let enqueued = tauri::async_runtime::spawn_blocking(move || {
        let mut enqueued = 0u64;
        // Modo por defecto para drops: Quality (el usuario puede cambiar el
        // modo en la página Optimizar antes de encolar manualmente).
        let default_mode = OptimizationMode::Quality;
        for path_str in paths {
            let path = PathBuf::from(&path_str);
            if !path.exists() {
                continue;
            }
            if path.is_dir() {
                let files = fs::discover_supported_files(&path, true);
                for file in files {
                    let input_format = Format::from_path(&file);
                    let dest = fs::compute_output_path(
                        &file,
                        output_mode,
                        Some(&custom_folder),
                        preserve_structure,
                        &filename_template,
                        None,
                    );
                    if dest == file {
                        continue;
                    }
                    let job = Job {
                        operation: JobOperation::Optimize,
                        source_path: file.to_string_lossy().to_string(),
                        destination_path: dest.to_string_lossy().to_string(),
                        input_format,
                        output_format: input_format,
                        optimization_mode: default_mode,
                        ..Default::default()
                    };
                    queue.enqueue(job);
                    enqueued += 1;
                }
            } else if path.is_file() {
                let validation = fs::validate_input_file(&path, 0);
                if !validation.is_ok() {
                    continue;
                }
                let input_format = Format::from_path(&path);
                let dest = fs::compute_output_path(
                    &path,
                    output_mode,
                    Some(&custom_folder),
                    preserve_structure,
                    &filename_template,
                    None,
                );
                if dest == path {
                    continue;
                }
                let job = Job {
                    operation: JobOperation::Optimize,
                    source_path: path.to_string_lossy().to_string(),
                    destination_path: dest.to_string_lossy().to_string(),
                    input_format,
                    output_format: input_format,
                    optimization_mode: default_mode,
                    ..Default::default()
                };
                queue.enqueue(job);
                enqueued += 1;
            }
        }
        enqueued
    })
    .await
    .map_err(|e| format!("panic escaneando carpetas: {e}"))?;

    if !state.queue.is_running() {
        state.queue.start();
    }
    emit_queue_changed(&app_handle, &state, None);
    Ok(enqueued)
}
