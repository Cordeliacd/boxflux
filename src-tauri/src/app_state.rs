//! Estado global de la aplicación Tauri.
//!
//! Se construye en `setup` y se registra con `tauri::Builder::manage(...)`.
//! Los commands reciben `State<AppState>` para acceder a los componentes
//! compartidos (engine, queue, profiles, history, etc.).
//!
//! Todos los datos persistentes viven en `~/BoxFlux/`:
//! - `~/BoxFlux/settings.json`
//! - `~/BoxFlux/profiles.json`
//! - `~/BoxFlux/history.jsonl`
//! - `~/BoxFlux/optimized/<session>/*` — imágenes optimizadas
//! - `~/BoxFlux/sessions/` — (reservado para uso futuro)

use std::sync::Arc;

use parking_lot::RwLock;
use tauri::{AppHandle, Manager};

use crate::diagnostics::Diagnostics;
use crate::engine::{Engine, EngineConfig};
use crate::engine::intel::{CancellationToken, OptimizationPipeline, PipelineConfig};
use crate::history::HistoryManager;
use crate::licensing::LicenseManager;
use crate::paths;
use crate::profiles::ProfileManager;
use crate::queue::JobQueue;
use crate::settings::SettingsManager;
use crate::statistics::StatisticsManager;

/// Rutas base de la app. Todo está bajo `~/BoxFlux/`.
#[derive(Clone)]
pub struct AppPaths {
    /// `~/BoxFlux/`
    pub home: std::path::PathBuf,
    /// `~/BoxFlux/optimized/`
    pub optimized_dir: std::path::PathBuf,
    /// `~/BoxFlux/sessions/`
    pub sessions_dir: std::path::PathBuf,
    /// `~/BoxFlux/settings.json`
    pub settings_path: std::path::PathBuf,
    /// `~/BoxFlux/profiles.json`
    pub profiles_path: std::path::PathBuf,
    /// `~/BoxFlux/history.jsonl`
    pub history_path: std::path::PathBuf,
}

pub struct AppState {
    pub engine: Arc<Engine>,
    pub pipeline: Arc<OptimizationPipeline>,
    pub queue: Arc<JobQueue>,
    pub profiles: Arc<ProfileManager>,
    pub history: Arc<HistoryManager>,
    pub statistics: Arc<StatisticsManager>,
    pub settings: Arc<SettingsManager>,
    pub licensing: Arc<LicenseManager>,
    pub diagnostics: Arc<Diagnostics>,
    pub paths: AppPaths,
    /// Token de cancelación del auto-optimize activo, si lo hay.
    pub auto_optimize_cancel: RwLock<Option<CancellationToken>>,
    pub auto_optimize_request_id: std::sync::atomic::AtomicU64,
}

impl AppState {
    /// Construye el estado a partir de un `AppHandle`. Las rutas se
    /// resuelven a `~/BoxFlux/` (ver `crate::paths`).
    pub fn build(_app: &AppHandle) -> Self {
        // Crea los directorios necesarios. Si falla, logueamos pero
        // continuamos — la app puede funcionar sin persistencia.
        if let Err(e) = paths::ensure_dirs() {
            eprintln!("WARN: no se pudo crear ~/BoxFlux/: {e}");
        }

        let home = paths::boxflux_home();
        let settings_path = paths::settings_path();
        let profiles_path = paths::profiles_path();
        let history_path = paths::history_path();
        let optimized_dir = paths::optimized_dir();
        let sessions_dir = paths::sessions_dir();

        let settings = Arc::new(SettingsManager::new(settings_path.clone()));
        let engine_config = EngineConfig {
            max_workers: settings.get().worker_count as usize,
        };
        let engine = Arc::new(Engine::new(engine_config));
        let max_workers = engine.max_workers() as u32;
        let profiles = Arc::new(ProfileManager::new(profiles_path.clone()));
        let history = Arc::new(HistoryManager::new(history_path.clone()));
        let licensing = Arc::new(LicenseManager::with_local_provider());
        let diagnostics = Arc::new(Diagnostics::new());

        let pipeline = Arc::new(OptimizationPipeline::new(PipelineConfig::default()));
        // El queue delega cada job al pipeline: analiza la imagen y
        // decide la optimización según el goal, sin perfiles fijos.
        let queue = Arc::new(JobQueue::new(Arc::clone(&pipeline), max_workers));
        // Aplicar settings de performance ya en el arranque; si no,
        // solo tendrían efecto cuando el usuario guardara Ajustes.
        let s = settings.get();
        queue.set_max_workers(s.worker_count);
        queue.set_cpu_limit_percent(s.cpu_limit_percent);
        queue.set_memory_budget_mb(s.max_memory_mb);
        let statistics = Arc::new(StatisticsManager::new(
            Arc::clone(&engine),
            Arc::clone(&queue),
        ));

        Self {
            engine,
            pipeline,
            queue,
            profiles,
            history,
            statistics,
            settings,
            licensing,
            diagnostics,
            paths: AppPaths {
                home,
                optimized_dir,
                sessions_dir,
                settings_path,
                profiles_path,
                history_path,
            },
            auto_optimize_cancel: RwLock::new(None),
            auto_optimize_request_id: std::sync::atomic::AtomicU64::new(1),
        }
    }

    pub fn next_request_id(&self) -> u64 {
        self.auto_optimize_request_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }
}
