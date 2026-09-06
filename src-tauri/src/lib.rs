//! BoxFlux — capa Rust de la aplicación Tauri 2.
//!
//! Este crate es la unión entre el engine de procesamiento (en `engine/`)
//! y los commands expuestos al frontend de React (en `commands/`).

pub mod app_state;
pub mod commands;
pub mod diagnostics;
pub mod engine;
pub mod fs;
pub mod history;
pub mod licensing;
pub mod paths;
pub mod profiles;
pub mod queue;
pub mod settings;
pub mod statistics;
pub mod util;
