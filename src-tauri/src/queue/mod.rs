//! Cola de jobs: encola, ejecuta N workers en paralelo y publica
//! resultados vía eventos de Tauri.
//!
//! Cada job se delega al `OptimizationPipeline`, que analiza la imagen,
//! genera candidatos según el goal (Quality/Compress), los evalúa,
//! selecciona el mejor y publica atómicamente. Sin perfiles fijos: la
//! decisión depende del análisis de cada imagen.

pub mod job;

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::engine::formats::Format;
use crate::engine::intel::{CancellationToken, OptimizationPipeline, UserConstraints};
use crate::util::id;
use crate::util::magic_bytes;

pub use job::{Job, JobOperation, JobStatus, OptimizationMode};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueueStats {
    pub total_jobs: u64,
    pub queued: u64,
    pub processing: u64,
    pub completed: u64,
    pub failed: u64,
    pub cancelled: u64,
    pub skipped: u64,
    pub original_bytes: u64,
    pub final_bytes: u64,
    pub total_processing_ms: u64,
    pub active_workers: u32,
}

struct QueueInner {
    jobs: HashMap<u64, Job>,
    queued_ids: VecDeque<u64>,
    stats: QueueStats,
    /// Tokens de cancelación per-job: se registran al pasar a
    /// Processing y se eliminan al terminar. `cancel()` los señala
    /// para que el pipeline pare y no publique.
    cancel_tokens: HashMap<u64, CancellationToken>,
}

pub struct JobQueue {
    /// El pipeline inteligente que analiza cada imagen y decide la
    /// mejor optimización según el goal del job.
    pipeline: Arc<OptimizationPipeline>,
    inner: Mutex<QueueInner>,
    running: AtomicBool,
    paused: AtomicBool,
    stop_requested: AtomicBool,
    active_workers: AtomicU32,
    /// Workers máximos configurados por el usuario (0 = automático).
    /// Se puede cambiar en runtime via `set_max_workers()`.
    max_workers: parking_lot::RwLock<u32>,
    /// Límite de CPU en % (0 = sin límite, 1..=100). Se puede cambiar
    /// en runtime via `set_cpu_limit_percent()`.
    cpu_limit_percent: parking_lot::RwLock<u32>,
    /// Presupuesto de memoria en MB para todos los jobs (0 = sin límite).
    /// Se propaga a cada `UserConstraints` construido desde el worker_loop.
    /// Los backends consultan este valor para elegir presets.
    memory_budget_mb: parking_lot::RwLock<u64>,
    notify: Arc<tokio::sync::Notify>,
}

impl JobQueue {
    /// La cola solo consume el pipeline; el `Engine` queda en
    /// `AppState` para `get_engine_info` / `get_format_list` y para
    /// `StatisticsManager`.
    pub fn new(pipeline: Arc<OptimizationPipeline>, max_workers: u32) -> Self {
        Self {
            pipeline,
            inner: Mutex::new(QueueInner {
                jobs: HashMap::new(),
                queued_ids: VecDeque::new(),
                stats: QueueStats::default(),
                cancel_tokens: HashMap::new(),
            }),
            running: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            stop_requested: AtomicBool::new(false),
            active_workers: AtomicU32::new(0),
            max_workers: parking_lot::RwLock::new(max_workers),
            cpu_limit_percent: parking_lot::RwLock::new(0),
            memory_budget_mb: parking_lot::RwLock::new(0),
            notify: Arc::new(tokio::sync::Notify::new()),
        }
    }

    /// Workers efectivos: el menor entre `max_workers` y el límite
    /// derivado de `cpu_limit_percent`. En 8 cores: 50% → 4 workers,
    /// 25% → 2, 0/100 → sin límite.
    pub fn max_workers(&self) -> u32 {
        let user_max = *self.max_workers.read();
        let cpu_pct = *self.cpu_limit_percent.read();
        let available = std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(4);
        // Base: si el usuario configuró workers, usar ese valor.
        // Si no (0), usar available_parallelism.
        let base = if user_max == 0 {
            available
        } else {
            user_max.min(available)
        };
        // Aplicar límite de CPU: el porcentaje se traduce a fracción de
        // cores disponibles. cpu_pct=0 → sin límite. cpu_pct=50 → 50%.
        let effective = if cpu_pct == 0 {
            base
        } else {
            // ceil para no redondear hacia abajo y dejar workers sin usar
            // cuando el usuario pide 25% en una máquina de 4 cores (1 worker).
            let pct = cpu_pct.min(100) as f64 / 100.0;
            ((available as f64) * pct).ceil() as u32
        };
        // El límite efectivo es el MENOR entre el user_max y el derivado
        // del cpu_pct. Así ambos settings cooperan.
        effective.min(base).max(1)
    }

    /// Cambia en runtime el número máximo de workers. El cambio toma
    /// efecto inmediatamente — el worker_loop consulta `max_workers()`
    /// en cada iteración antes de empezar un job nuevo.
    pub fn set_max_workers(&self, n: u32) {
        *self.max_workers.write() = n;
        // Despertar al worker_loop por si estaba esperando con 0
        // workers activos y ahora puede iniciar más.
        self.notify.notify_one();
    }

    /// Cambia en runtime el límite de CPU en %. 0 = sin límite.
    pub fn set_cpu_limit_percent(&self, pct: u32) {
        *self.cpu_limit_percent.write() = pct.min(100);
        self.notify.notify_one();
    }

    /// Cambia en runtime el presupuesto de memoria en MB. 0 = sin límite.
    /// Los backends consultan este valor via `UserConstraints.memory_budget_mb`.
    pub fn set_memory_budget_mb(&self, mb: u64) {
        *self.memory_budget_mb.write() = mb;
    }

    /// Devuelve el presupuesto de memoria actual en MB.
    pub fn memory_budget_mb(&self) -> u64 {
        *self.memory_budget_mb.read()
    }

    /// Encola un job. Devuelve su ID.
    pub fn enqueue(&self, mut job: Job) -> u64 {
        if job.id == 0 {
            job.id = id::next_id();
        }
        job.status = JobStatus::Queued;
        job.queued_at = Some(Instant::now());
        let id = job.id;
        let mut inner = self.inner.lock();
        inner.jobs.insert(id, job);
        inner.queued_ids.push_back(id);
        // Recount en vez de contadores incrementales: inmune a razas.
        recount_from_jobs(&mut inner);
        drop(inner);
        self.notify.notify_one();
        id
    }

    pub fn snapshot(&self) -> Vec<Job> {
        let inner = self.inner.lock();
        let mut jobs: Vec<Job> = inner.jobs.values().cloned().collect();
        jobs.sort_by_key(|j| j.id);
        jobs
    }

    pub fn stats(&self) -> QueueStats {
        let mut inner = self.inner.lock();
        recount_from_jobs(&mut inner);
        let mut s = inner.stats.clone();
        s.active_workers = self.active_workers.load(Ordering::Relaxed);
        s
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    pub fn start(&self) {
        self.stop_requested.store(false, Ordering::Relaxed);
        self.running.store(true, Ordering::Relaxed);
        self.paused.store(false, Ordering::Relaxed);
        self.notify.notify_one();
    }

    pub fn pause(&self) {
        self.paused.store(true, Ordering::Relaxed);
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::Relaxed);
        self.notify.notify_one();
    }

    pub fn stop(&self) {
        self.stop_requested.store(true, Ordering::Relaxed);
        self.running.store(false, Ordering::Relaxed);
        self.notify.notify_waiters();
    }

    pub fn cancel(&self, job_id: u64) -> bool {
        let mut inner = self.inner.lock();
        // Si el job está Processing, señalamos su token: el pipeline
        // para en el siguiente boundary y no publica el resultado.
        if let Some(token) = inner.cancel_tokens.get(&job_id) {
            token.cancel();
        }
        if let Some(job) = inner.jobs.get_mut(&job_id) {
            match job.status {
                JobStatus::Queued => {
                    job.status = JobStatus::Cancelled;
                    job.finished_at = Some(Instant::now());
                    inner.queued_ids.retain(|&id| id != job_id);
                    recount_from_jobs(&mut inner);
                    true
                }
                JobStatus::Processing => {
                    // El estado final (Cancelled) lo fija el worker al
                    // terminar; aquí solo señalamos la cancelación y
                    // marcamos el estado para que la UI lo refleje ya.
                    job.status = JobStatus::Cancelled;
                    true
                }
                _ => false,
            }
        } else {
            false
        }
    }

    pub fn retry(&self, job_id: u64) -> Option<u64> {
        let job = {
            let mut inner = self.inner.lock();
            let job = inner.jobs.get(&job_id)?.clone();
            match job.status {
                JobStatus::Failed | JobStatus::Cancelled => {}
                _ => return None,
            }
            // El token ya no aplica (el job terminó); limpiamos restos.
            inner.cancel_tokens.remove(&job_id);
            inner.jobs.remove(&job_id);
            job
        };
        let new_job = Job {
            id: 0,
            operation: job.operation,
            source_path: job.source_path,
            destination_path: job.destination_path,
            input_format: job.input_format,
            output_format: job.output_format,
            optimization_mode: job.optimization_mode,
            resize: job.resize,
            original_size: 0,
            final_size: 0,
            progress: 0,
            status: JobStatus::Queued,
            processing_time_ms: 0,
            selected_format: String::new(),
            selected_quality: String::new(),
            decision_explanation: String::new(),
            error: String::new(),
            // En retry se conservan las elecciones de formato/perfil
            // del usuario.
            force_format: job.force_format,
            force_lossless: job.force_lossless,
            strip_all_metadata: job.strip_all_metadata,
            preserve_color_profile: job.preserve_color_profile,
            queued_at: Some(Instant::now()),
            started_at: None,
            finished_at: None,
        };
        Some(self.enqueue(new_job))
    }

    pub fn remove(&self, job_id: u64) -> bool {
        let mut inner = self.inner.lock();
        if inner.jobs.remove(&job_id).is_some() {
            inner.queued_ids.retain(|&id| id != job_id);
            inner.cancel_tokens.remove(&job_id);
            recount_from_jobs(&mut inner);
            true
        } else {
            false
        }
    }

    pub fn clear_completed(&self) {
        let mut inner = self.inner.lock();
        let completed_ids: Vec<u64> = inner
            .jobs
            .iter()
            .filter(|(_, j)| {
                matches!(
                    j.status,
                    JobStatus::Completed
                        | JobStatus::Failed
                        | JobStatus::Cancelled
                        | JobStatus::Skipped
                )
            })
            .map(|(id, _)| *id)
            .collect();
        for id in completed_ids {
            inner.jobs.remove(&id);
            inner.cancel_tokens.remove(&id);
        }
        inner.queued_ids.retain(|id| inner.jobs.contains_key(id));
        recount_from_jobs(&mut inner);
    }

    /// Loop principal del worker; `main.rs` lo instancia N veces
    /// (pool real). El gate `active >= max_workers()` aplica el
    /// límite en runtime. Cada job se delega al
    /// `OptimizationPipeline`.
    pub async fn worker_loop(self: Arc<Self>, app_handle: AppHandle) {
        loop {
            if self.stop_requested.load(Ordering::Relaxed) {
                break;
            }
            if self.paused.load(Ordering::Relaxed) {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                continue;
            }
            let active = self.active_workers.load(Ordering::Relaxed);
            if active >= self.max_workers() {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                continue;
            }
            let job_id = {
                let mut inner = self.inner.lock();
                inner.queued_ids.pop_front()
            };
            let Some(job_id) = job_id else {
                let _ = tokio::time::timeout(
                    std::time::Duration::from_millis(500),
                    self.notify.notified(),
                )
                .await;
                continue;
            };
            // El token se registra antes de marcar Processing: sin esa
            // ventana, cancel() podría no encontrar token a quien señalar.
            let cancel_token = CancellationToken::new();
            let job = {
                let mut guard = self.inner.lock();
                let inner: &mut QueueInner = &mut *guard;
                let Some(job) = inner.jobs.get_mut(&job_id) else {
                    continue;
                };
                if job.status == JobStatus::Cancelled {
                    let cancelled_job = job.clone();
                    recount_from_jobs(inner);
                    let _ = app_handle.emit("boxflux://job-updated", cancelled_job);
                    let _ = app_handle.emit("boxflux://stats-changed", self.stats_snapshot(inner));
                    continue;
                }
                job.status = JobStatus::Processing;
                job.started_at = Some(Instant::now());
                inner.cancel_tokens.insert(job_id, cancel_token.clone());
                recount_from_jobs(inner);
                let job_clone = job.clone();
                let _ = app_handle.emit("boxflux://job-updated", job_clone.clone());
                job_clone
            };
            self.active_workers.fetch_add(1, Ordering::Relaxed);
            // El job se procesa a través del `OptimizationPipeline`.
            //
            // Staging dir único por job: el pipeline publica en
            // `selected.<ext>` y dos jobs hacia la misma carpeta
            // colisionarían en ese nombre fijo; cada job usa
            // `.boxflux-stage-<job_id>` y se elimina al mover al destino.
            //
            // El `destination_path` original conserva la extensión de
            // entrada; tras la decisión se recalcula el path final con la
            // extensión canónica del formato seleccionado (validado por
            // magic bytes).
            let pipeline = Arc::clone(&self.pipeline);
            let history = {
                // Historial para registrar la ejecución.
                let state: tauri::State<'_, crate::app_state::AppState> = app_handle.state();
                Arc::clone(&state.history)
            };
            let memory_budget = self.memory_budget_mb();
            let token_for_job = cancel_token.clone();
            let result = tauri::async_runtime::spawn_blocking(move || {
                let input = PathBuf::from(&job.source_path);
                let dest_dir = PathBuf::from(&job.destination_path)
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("."));
                // Staging dir único por job: evita que dos jobs hacia la
                // misma carpeta colisionen en el nombre fijo de salida.
                let staging_dir = dest_dir.join(format!(".boxflux-stage-{}", job.id));
                let goal = job.optimization_mode.to_pipeline_goal();
                let constraints = build_constraints(&job, memory_budget);
                // Con el token, cancelar un job en Processing detiene
                // de verdad el pipeline.
                let decision = pipeline.optimize_with_cancel(
                    &input,
                    &staging_dir,
                    goal,
                    &constraints,
                    &token_for_job,
                );
                // Localizar el archivo publicado por el pipeline.
                let selected_format = decision.selected.as_ref().map(|s| s.candidate.format);
                let selected_ext = selected_format
                    .map(|f| f.canonical_extension())
                    .unwrap_or("png");
                let selected_path = staging_dir.join(format!("selected.{}", selected_ext));
                // Recalcula el `final_path` reemplazando la extensión del
                // `destination_path` con la extensión canónica del formato
                // seleccionado. Esto garantiza que el archivo final tenga
                // una extensión consistente con sus bytes.
                let destination = PathBuf::from(&job.destination_path);
                let final_path = adjust_extension_to_match_format(&destination, selected_format);
                // Mover del staging al destino final. Si el archivo no
                // se puede publicar, el job FALLA con mensaje claro —
                // nunca un "Completed" apuntando a un archivo que no
                // existe.
                let mut publish_error: Option<String> = None;
                if decision.selected.is_some() {
                    if !selected_path.exists() {
                        publish_error = Some(format!(
                            "el pipeline no publicó la salida esperada: {}",
                            selected_path.display()
                        ));
                    } else if selected_path != final_path {
                        // Sobrescribir atómicamente vía rename.
                        if final_path.exists() {
                            let _ = std::fs::remove_file(&final_path);
                        }
                        if let Err(e) = std::fs::rename(&selected_path, &final_path) {
                            publish_error = Some(format!(
                                "no se pudo mover la salida a {}: {e}",
                                final_path.display()
                            ));
                        }
                    }
                }
                // Limpieza del staging dir (best-effort).
                let _ = std::fs::remove_dir_all(&staging_dir);
                // Verificación por magic bytes: si el archivo publicado
                // tiene una extensión que no coincide con su contenido
                // real, lo corregimos. Red de seguridad ante bugs en el
                // pipeline o en los backends.
                let corrected_path = if publish_error.is_none() && decision.selected.is_some() {
                    verify_and_fix_extension(&final_path)
                } else {
                    final_path
                };
                (decision, corrected_path, publish_error)
            })
            .await;
            // Apply result.
            let mut guard = self.inner.lock();
            let inner: &mut QueueInner = &mut *guard;
            self.active_workers.fetch_sub(1, Ordering::Relaxed);
            inner.cancel_tokens.remove(&job_id);
            let Some(j) = inner.jobs.get_mut(&job_id) else {
                continue;
            };
            let now = Instant::now();
            j.finished_at = Some(now);
            if j.status == JobStatus::Cancelled {
                // El pipeline fue cancelado vía token: limpiar posibles
                // restos. La extensión real puede diferir de la del
                // destination_path — limpiamos ambos (best-effort).
                if j.destination_path != j.source_path {
                    let _ = std::fs::remove_file(&j.destination_path);
                }
                let cancelled_job = j.clone();
                drop(j);
                recount_from_jobs(inner);
                let _ = app_handle.emit("boxflux://job-updated", cancelled_job);
                let _ = app_handle.emit("boxflux://stats-changed", self.stats_snapshot(inner));
                continue;
            }
            match result {
                Ok((decision, final_path, publish_error)) => {
                    // Un fallo del pipeline (archivo corrupto, ilegible,
                    // directorio inaccesible) marca el job como FAILED,
                    // nunca como "Completed" con 0% de ahorro.
                    if decision.failed || publish_error.is_some() {
                        j.error = publish_error.unwrap_or_else(|| decision.explanation.clone());
                        j.status = JobStatus::Failed;
                        j.original_size = decision.original_size;
                        j.processing_time_ms =
                            now.duration_since(j.started_at.unwrap_or(now)).as_millis() as u64;
                        let failed_job = j.clone();
                        drop(j);
                        recount_from_jobs(inner);
                        let _ = app_handle.emit("boxflux://job-updated", failed_job);
                        let _ =
                            app_handle.emit("boxflux://stats-changed", self.stats_snapshot(inner));
                        continue;
                    }
                    let final_size = std::fs::metadata(&final_path)
                        .map(|m| m.len())
                        .unwrap_or(decision.final_size);
                    j.original_size = decision.original_size;
                    j.final_size = final_size;
                    j.processing_time_ms =
                        now.duration_since(j.started_at.unwrap_or(now)).as_millis() as u64;
                    if let Some(sel) = &decision.selected {
                        j.selected_format = sel.candidate.format.as_str().to_string();
                        j.selected_quality = if sel.candidate.lossless {
                            "lossless".to_string()
                        } else {
                            sel.candidate
                                .quality
                                .map(|q| format!("q{}", q))
                                .unwrap_or_else(|| "auto".to_string())
                        };
                        j.output_format = sel.candidate.format;
                    }
                    j.decision_explanation = decision.explanation;
                    j.status = JobStatus::Completed;
                    let processing_time_ms = j.processing_time_ms;
                    let completed_job = j.clone();
                    drop(j);
                    recount_from_jobs(inner);
                    inner.stats.original_bytes = inner
                        .stats
                        .original_bytes
                        .saturating_add(decision.original_size);
                    inner.stats.final_bytes = inner.stats.final_bytes.saturating_add(final_size);
                    inner.stats.total_processing_ms = inner
                        .stats
                        .total_processing_ms
                        .saturating_add(processing_time_ms);
                    // Registrar en el historial: la página solo tiene
                    // datos si el queue los escribe aquí.
                    if !decision.kept_original {
                        let entry = crate::history::HistoryEntry {
                            id: 0,
                            timestamp: String::new(),
                            files_processed: 1,
                            files_failed: 0,
                            profile_name: format!(
                                "queue:{}",
                                completed_job.optimization_mode.as_str()
                            ),
                            original_size: decision.original_size,
                            output_size: final_size,
                            duration_ms: processing_time_ms,
                        };
                        let _ = history.append(entry);
                    }
                    let _ = app_handle.emit("boxflux://job-updated", completed_job);
                    let _ = app_handle.emit("boxflux://stats-changed", self.stats_snapshot(inner));
                    continue;
                }
                Err(e) => {
                    j.error = format!("panic del worker: {e}");
                    j.status = JobStatus::Failed;
                    let failed_job = j.clone();
                    drop(j);
                    recount_from_jobs(inner);
                    let _ = app_handle.emit("boxflux://job-updated", failed_job);
                    let _ = app_handle.emit("boxflux://stats-changed", self.stats_snapshot(inner));
                }
            }
        }
    }

    fn stats_snapshot(&self, inner: &mut QueueInner) -> QueueStats {
        // Recalcular contadores desde el map de jobs: los contadores
        // incrementales desincronizan con razas cancel/worker.
        recount_from_jobs(inner);
        let mut s = inner.stats.clone();
        s.active_workers = self.active_workers.load(Ordering::Relaxed);
        s
    }
}

// Helpers de publicación
//
// Viven en el queue y no en el pipeline porque decidir dónde acaba
// el archivo final es responsabilidad del QUEUE. El pipeline publica
// `selected.<ext>`; el queue lo mueve al `destination_path` del job,
// ajustando la extensión si el formato seleccionado difiere del de
// entrada.

/// Recalcula los CONTADORES de stats desde el map de jobs. Evita las
/// razas de los contadores incrementales (cancel durante pop, remove
/// durante processing, retry sin decrementar total_jobs).
///
/// Los campos acumulativos (bytes y tiempos) NO se recalculan — son
/// append-only y por construcción no pueden desincronizarse.
fn recount_from_jobs(inner: &mut QueueInner) {
    let mut s = &mut inner.stats;
    s.total_jobs = inner.jobs.len() as u64;
    s.queued = 0;
    s.processing = 0;
    s.completed = 0;
    s.failed = 0;
    s.cancelled = 0;
    s.skipped = 0;
    for j in inner.jobs.values() {
        match j.status {
            JobStatus::Queued => s.queued += 1,
            JobStatus::Processing => s.processing += 1,
            JobStatus::Completed => s.completed += 1,
            JobStatus::Failed => s.failed += 1,
            JobStatus::Cancelled => s.cancelled += 1,
            JobStatus::Skipped => s.skipped += 1,
        }
    }
}

/// Construye `UserConstraints` para el pipeline a partir del job.
///
/// El job no guarda todavía `force_format` ni `preserve_color_profile`
/// como campos first-class; los lee de los settings globales si el job
/// los trae en `optimization_mode`. En el futuro se pueden agregar campos
/// específicos al `Job`.
///
/// `memory_budget_mb` se pasa explícitamente desde el JobQueue — viene
/// del slider "Memoria máxima" en Ajustes. 0 = sin límite.
fn build_constraints(job: &Job, memory_budget_mb: u64) -> UserConstraints {
    let mut c = UserConstraints::default();
    // El job puede venir con force_format y preserve_color_profile si
    // el comando `enqueue_optimize_job` los recibió del frontend.
    c.force_format = job.force_format;
    c.force_lossless = job.force_lossless;
    c.strip_all_metadata = job.strip_all_metadata;
    c.preserve_color_profile = job.preserve_color_profile;
    c.memory_budget_mb = memory_budget_mb;
    c
}

/// Ajusta la extensión del `destination_path` para que coincida con el
/// formato seleccionado por el motor.
///
/// Si el formato seleccionado es `None` (el motor no eligió ningún
/// candidato y se mantuvo el original), se devuelve el `destination_path`
/// sin cambios.
///
/// Ejemplo:
///   destination = `/foo/photo_optimized.png`
///   selected_format = Some(Format::Webp)
///   → devuelve `/foo/photo_optimized.webp`
fn adjust_extension_to_match_format(
    destination: &Path,
    selected_format: Option<Format>,
) -> PathBuf {
    let Some(format) = selected_format else {
        return destination.to_path_buf();
    };
    let target_ext = format.canonical_extension();
    if target_ext.is_empty() {
        return destination.to_path_buf();
    }
    // Si la extensión actual ya coincide, no hacemos nada.
    if let Some(current_ext) = destination.extension().and_then(|e| e.to_str()) {
        let current_format = Format::from_extension(current_ext);
        if current_format == format {
            return destination.to_path_buf();
        }
    }
    // Reemplaza la extensión. `set_extension` maneja correctamente los
    // casos en que el path no tiene extensión.
    let mut new_path = destination.to_path_buf();
    new_path.set_extension(target_ext);
    new_path
}

/// Verifica que la extensión del archivo publicado coincida con sus magic
/// bytes. Si no coincide, renombra el archivo a la extensión correcta.
///
/// Esto es la red de seguridad final: si algún bug futuro en el pipeline
/// o en los backends produce bytes de un formato distinto al anunciado,
/// este renombrado silencioso evita que el usuario vea un archivo con
/// extensión engañosa.
///
/// Devuelve el path final (que puede ser el mismo o uno renombrado).
fn verify_and_fix_extension(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let real_format = magic_bytes::detect_format_from_bytes(path);
    if real_format == Format::Unknown {
        // No pudimos determinar el formato real. No tocamos el archivo
        // para no romper nada — preferimos ser conservadores.
        return path.to_path_buf();
    }
    let real_ext = real_format.canonical_extension();
    let current_ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if current_ext.eq_ignore_ascii_case(real_ext) {
        return path.to_path_buf();
    }
    // La extensión no coincide con los bytes. Renombramos.
    let mut corrected = path.to_path_buf();
    corrected.set_extension(real_ext);
    // Si el corrected_path ya existe (caso raro), lo eliminamos primero.
    if corrected.exists() && corrected != path {
        let _ = std::fs::remove_file(&corrected);
    }
    if let Err(e) = std::fs::rename(path, &corrected) {
        // Si el rename falla (p.ej. permisos), devolvemos el path original.
        tracing::warn!(
            "boxflux: no se pudo renombrar {} → {}: {}. La extensión no coincide con los bytes.",
            path.display(),
            corrected.display(),
            e
        );
        return path.to_path_buf();
    }
    tracing::info!(
        "boxflux: extensión corregida {} → {} (bytes reales: {})",
        path.display(),
        corrected.display(),
        real_format.as_str()
    );
    corrected
}
