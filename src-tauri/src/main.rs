//! BoxFlux — entry point de la aplicación Tauri 2.
//!
//! Carga los plugins, registra el estado global (`AppState`) y todos los
//! commands. El frontend React se comunica vía `invoke('command_name', args)`.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;

use boxflux_lib::app_state::AppState;
use boxflux_lib::commands::{
    auto_optimize, diagnostics, engine, fs, history, licensing, profiles, queue, settings, stats,
};
use tauri::Manager;

fn main() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,boxflux=debug")),
        )
        .with_target(false)
        .try_init();

    tauri::Builder::default()
        // Mínimo privilegio: solo dialog. Nada de shell/fs/opener —
        // todo I/O pasa por commands validados, no por plugins.
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_handle = app.handle().clone();
            let state = AppState::build(&app_handle);
            // Pool de N worker loops (N = cores, tope 8). El gate de
            // active_workers aplica el límite configurado en runtime.
            let worker_count = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
                .min(8)
                .max(1);
            for _ in 0..worker_count {
                let queue_clone: Arc<boxflux_lib::queue::JobQueue> = Arc::clone(&state.queue);
                let handle_for_worker = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    queue_clone.worker_loop(handle_for_worker).await;
                });
            }
            state.queue.start();
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Engine
            engine::get_engine_info,
            engine::get_format_list,
            // Queue
            queue::get_queue_snapshot,
            queue::get_queue_stats,
            queue::start_queue,
            queue::pause_queue,
            queue::resume_queue,
            queue::cancel_job,
            queue::retry_job,
            queue::remove_job,
            queue::clear_completed,
            queue::enqueue_optimize_job,
            queue::add_dropped_paths,
            // Profiles
            profiles::get_all_profiles,
            profiles::get_default_profile,
            profiles::set_default_profile,
            // History
            history::get_history,
            history::remove_history_entry,
            history::clear_history,
            history::get_history_aggregate,
            // Settings
            settings::get_settings,
            settings::update_settings,
            settings::list_themes,
            // Diagnostics
            diagnostics::get_logs,
            diagnostics::clear_logs,
            diagnostics::export_logs,
            diagnostics::copy_diagnostics,
            diagnostics::log_debug,
            diagnostics::log_info,
            diagnostics::log_warn,
            diagnostics::log_error,
            // Licensing
            licensing::get_license_tier,
            licensing::get_license_status,
            licensing::get_license_entitlement,
            licensing::activate_license,
            licensing::deactivate_license,
            licensing::is_entitled_to,
            licensing::list_license_tiers,
            licensing::list_license_statuses,
            // Stats
            stats::get_live_stats,
            stats::get_batch_summary,
            // FS
            fs::validate_input_file,
            fs::validate_output_path,
            fs::discover_supported_files,
            fs::is_supported_image_file,
            fs::compute_output_path,
            // Auto-optimize
            auto_optimize::start_auto_optimize,
            auto_optimize::cancel_auto_optimize,
            auto_optimize::is_auto_optimize_running,
            auto_optimize::list_intel_backends,
        ])
        .run(tauri::generate_context!())
        .expect("error while running BoxFlux");
}
