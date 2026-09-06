//! Tauri commands — la frontera entre el frontend React y el backend Rust.
//!
//! Cada `#[tauri::command]` aquí definido se expone al frontend vía
//! `invoke_handler` en `lib.rs`. Los commands se agrupan por dominio:
//! - engine, formats
//! - queue, jobs
//! - profiles
//! - history
//! - settings
//! - diagnostics
//! - licensing
//! - auto_optimize (con events)

pub mod auto_optimize;
pub mod diagnostics;
pub mod engine;
pub mod fs;
pub mod history;
pub mod licensing;
pub mod profiles;
pub mod queue;
pub mod settings;
pub mod stats;
