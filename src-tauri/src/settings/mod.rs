//! Settings persistentes de la aplicación, serializadas con serde_json
//! a `settings.json`.

use std::path::{Path, PathBuf};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

/// Tema visual de la aplicación.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub enum Theme {
    #[default]
    Light,
    Dark,
    System,
}

/// Política de salida de archivos optimizados.
///
/// Por defecto usamos `CustomFolder` apuntando a `~/BoxFlux/optimized/`
/// para que todas las imágenes optimizadas se guarden en un único lugar
/// visible para el usuario, en vez de crear subcarpetas `BoxFlux/` en el
/// directorio de cada foto original.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub enum OutputMode {
    /// Misma carpeta que el original.
    ///
    /// ADVERTENCIA: esto puede sobreescribir archivos si la plantilla
    /// de nombre colisiona. Úsalo solo si sabes lo que haces.
    SameFolder,
    /// Subcarpeta `BoxFlux/` dentro del original. Mantenida por
    /// retrocompatibilidad — desaconsejada.
    #[deprecated(note = "usar CustomFolder con ~/BoxFlux/optimized/")]
    OutputSubfolder,
    /// Carpeta personalizada definida por el usuario. Por defecto
    /// `~/BoxFlux/optimized/`.
    #[default]
    CustomFolder,
}

/// Política de manejo de metadata EXIF/XMP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub enum MetadataBehavior {
    #[default]
    Keep,
    /// Elimina campos sensibles (GPS, datos personales) preservando
    /// información técnica (cámara, dimensiones).
    RemoveSafe,
    /// Elimina toda la metadata.
    RemoveAll,
}

/// Configuración completa de la aplicación. Serializada a JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    /// Tema visual.
    #[serde(default)]
    pub theme: Theme,

    /// Idioma de la UI ("es" o "en").
    #[serde(default = "default_language")]
    pub language: String,

    /// Arrancar minimizado.
    #[serde(default)]
    pub start_minimized: bool,

    /// Notificaciones de escritorio activadas.
    #[serde(default = "default_true")]
    pub notifications_enabled: bool,

    /// Workers concurrentes. 0 = auto (`available_parallelism`).
    #[serde(default)]
    pub worker_count: u32,

    /// Procesar automáticamente los archivos arrastrados a la ventana.
    #[serde(default)]
    pub automatic_processing: bool,

    /// Perfil de optimización por defecto (nombre).
    #[serde(default = "default_profile")]
    pub default_profile: String,

    /// Calidad por defecto (1-100) para formatos lossy.
    #[serde(default = "default_quality")]
    pub default_quality: u8,

    /// Política de metadata por defecto.
    #[serde(default)]
    pub metadata_behavior: MetadataBehavior,

    /// Formato de salida por defecto (string vacío = preservar).
    #[serde(default)]
    pub default_output_format: String,

    /// Política de ubicación de salida.
    #[serde(default)]
    pub output_mode: OutputMode,

    /// Carpeta de salida personalizada (solo si `output_mode = CustomFolder`).
    #[serde(default)]
    pub custom_output_folder: String,

    /// Preservar estructura de carpetas en batch processing.
    #[serde(default = "default_true")]
    pub preserve_folder_structure: bool,

    /// Plantilla para nombres de archivo. Placeholders: `{name}`, `{ext}`, `{format}`.
    #[serde(default = "default_filename_template")]
    pub filename_template: String,

    /// Límite de CPU en porcentaje (0 = sin límite).
    #[serde(default)]
    pub cpu_limit_percent: u32,

    /// Memoria máxima en MB (0 = sin límite).
    #[serde(default)]
    pub max_memory_mb: u64,

    /// Logging activado.
    #[serde(default = "default_true")]
    pub logging_enabled: bool,

    /// Diagnósticos detallados (modo debug).
    #[serde(default)]
    pub diagnostics_enabled: bool,

    /// Funcionalidades experimentales (AVIF, etc.).
    #[serde(default)]
    pub experimental_features: bool,
}

fn default_language() -> String {
    "es".to_string()
}
fn default_true() -> bool {
    true
}
fn default_profile() -> String {
    "Web".to_string()
}
fn default_quality() -> u8 {
    85
}
fn default_filename_template() -> String {
    "{name}_optimized{format}".to_string()
}

impl Default for AppSettings {
    fn default() -> Self {
        // Resolvemos `~/BoxFlux/optimized/` de forma lazy. Si `$HOME` no
        // está disponible, dejamos un string vacío que el validador del
        // frontend sustituirá por el directorio del usuario.
        let default_output_folder = crate::paths::optimized_dir().to_string_lossy().to_string();
        Self {
            theme: Theme::Light,
            language: default_language(),
            start_minimized: false,
            notifications_enabled: true,
            worker_count: 0,
            automatic_processing: false,
            default_profile: default_profile(),
            default_quality: default_quality(),
            metadata_behavior: MetadataBehavior::RemoveSafe,
            default_output_format: String::new(),
            output_mode: OutputMode::CustomFolder,
            custom_output_folder: default_output_folder,
            preserve_folder_structure: false,
            filename_template: default_filename_template(),
            cpu_limit_percent: 0,
            max_memory_mb: 0,
            logging_enabled: true,
            diagnostics_enabled: false,
            experimental_features: false,
        }
    }
}

impl AppSettings {
    /// Carga settings desde `path`. Si el archivo no existe o es inválido,
    /// devuelve `Default::default()` y (en caso de error) escribe un log
    /// de advertencia.
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_else(|e| {
                tracing::warn!("settings.json corrupto ({e}), usando defaults");
                Self::default()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => {
                tracing::warn!("error leyendo settings.json ({e}), usando defaults");
                Self::default()
            }
        }
    }

    /// Persiste los settings a `path`. Crea los directorios padres si hace falta.
    ///
    /// Escritura atómica (tmp + rename): un crash a mitad del write no
    /// debe dejar el JSON truncado — al recargar caeríamos en defaults
    /// y el usuario perdería sus ajustes.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        crate::util::fs_io::write_atomic(path, json.as_bytes())
    }
}

/// Manager thread-safe de settings. Mantiene una copia en memoria + path
/// del archivo JSON.
pub struct SettingsManager {
    settings: RwLock<AppSettings>,
    path: PathBuf,
}

impl SettingsManager {
    pub fn new(path: PathBuf) -> Self {
        let settings = AppSettings::load(&path);
        Self {
            settings: RwLock::new(settings),
            path,
        }
    }

    pub fn get(&self) -> AppSettings {
        self.settings.read().clone()
    }

    pub fn update<F>(&self, f: F)
    where
        F: FnOnce(&mut AppSettings),
    {
        let mut s = self.settings.write();
        f(&mut s);
        // Best-effort persistencia.
        if let Err(e) = s.save(&self.path) {
            tracing::error!("error guardando settings.json: {e}");
        }
    }

    pub fn replace(&self, new_settings: AppSettings) {
        let mut s = self.settings.write();
        *s = new_settings;
        if let Err(e) = s.save(&self.path) {
            tracing::error!("error guardando settings.json: {e}");
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}
