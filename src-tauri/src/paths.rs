//! Layout del directorio de datos de BoxFlux.
//!
//! Todo vive en `~/BoxFlux/` (visible para el usuario) en lugar del
//! `app_data_dir()` de cada SO.
//!
//! Estructura:
//!
//! ```text
//! ~/BoxFlux/
//! ├── settings.json          # Preferencias
//! ├── profiles.json         # Perfiles built-in + custom
//! ├── history.jsonl         # Historial de sesiones (1 línea por entrada)
//! ├── optimized/            # Todas las imágenes optimizadas
//! │   ├── 2024-01-15_142530_photo/
//! │   │   ├── photo_optimized.png
//! │   │   ├── photo_optimized.webp
//! │   │   └── ...
//! │   └── 2024-01-15_143021_screenshot/
//! │       └── screenshot_optimized.png
//! └── sessions/             # (Futuro: snapshot por sesión para re-abrir)
//! ```
//!
//! En macOS y Windows se respeta el directorio home del usuario (`~`).

use std::path::PathBuf;

/// Resuelve `~/BoxFlux/` de forma multiplataforma.
///
/// - Linux: `$HOME/BoxFlux/`
/// - macOS: `$HOME/BoxFlux/`
/// - Windows: `%USERPROFILE%\BoxFlux\`
///
/// Si `$HOME` no está definido, devuelve `./BoxFlux/` como fallback.
pub fn boxflux_home() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join("BoxFlux")
    } else if let Some(userprofile) = std::env::var_os("USERPROFILE") {
        // Windows: HOME normalmente no está definida.
        PathBuf::from(userprofile).join("BoxFlux")
    } else {
        PathBuf::from("./BoxFlux")
    }
}

/// `~/BoxFlux/optimized/` — donde se guardan las imágenes optimizadas.
pub fn optimized_dir() -> PathBuf {
    boxflux_home().join("optimized")
}

/// `~/BoxFlux/sessions/` — snapshot por sesión (reservado para uso futuro).
pub fn sessions_dir() -> PathBuf {
    boxflux_home().join("sessions")
}

/// `~/BoxFlux/settings.json`.
pub fn settings_path() -> PathBuf {
    boxflux_home().join("settings.json")
}

/// `~/BoxFlux/profiles.json`.
pub fn profiles_path() -> PathBuf {
    boxflux_home().join("profiles.json")
}

/// `~/BoxFlux/history.jsonl`.
pub fn history_path() -> PathBuf {
    boxflux_home().join("history.jsonl")
}

/// Crea todos los subdirectorios necesarios. Llamar al arranque.
pub fn ensure_dirs() -> std::io::Result<()> {
    std::fs::create_dir_all(boxflux_home())?;
    std::fs::create_dir_all(optimized_dir())?;
    std::fs::create_dir_all(sessions_dir())?;
    Ok(())
}
