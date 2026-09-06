//! Catálogo de perfiles persistido en `profiles.json`: built-ins más
//! custom del usuario. Solo lectura para la UI, más la selección del
//! perfil por defecto.

use std::path::{Path, PathBuf};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::engine::optimization::OptimizationProfile;

/// Catálogo de perfiles: built-ins + custom del usuario.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProfileCatalog {
    #[serde(default)]
    pub profiles: Vec<OptimizationProfile>,
    #[serde(default = "default_profile_name")]
    pub default_name: String,
}

fn default_profile_name() -> String {
    "Web".to_string()
}

impl ProfileCatalog {
    /// Catálogo inicial con los presets built-in.
    pub fn with_builtins() -> Self {
        Self {
            profiles: OptimizationProfile::all_presets(),
            default_name: "Web".to_string(),
        }
    }

    /// Si el default apunta a un perfil que ya no existe, vuelve a "Web".
    fn sanitized(mut self) -> Self {
        if !self.profiles.iter().any(|p| p.name == self.default_name) {
            self.default_name = "Web".to_string();
        }
        self
    }
}

pub struct ProfileManager {
    catalog: RwLock<ProfileCatalog>,
    path: PathBuf,
}

impl ProfileManager {
    pub fn new(path: PathBuf) -> Self {
        let catalog = Self::load_or_init(&path);
        Self {
            catalog: RwLock::new(catalog),
            path,
        }
    }

    fn load_or_init(path: &Path) -> ProfileCatalog {
        match std::fs::read_to_string(path) {
            Ok(contents) => Self::parse_catalog(&contents),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => ProfileCatalog::with_builtins(),
            Err(e) => {
                tracing::warn!("error leyendo profiles.json ({e}); usando builtins");
                ProfileCatalog::with_builtins()
            }
        }
    }

    /// Parseo leniente: ante un perfil con kind desconocido filtramos
    /// ese perfil y conservamos el resto, en vez de descartar todo el
    /// catálogo del usuario por un único error de parseo.
    fn parse_catalog(contents: &str) -> ProfileCatalog {
        match serde_json::from_str::<ProfileCatalog>(contents) {
            Ok(catalog) => catalog.sanitized(),
            Err(_) => {
                let Ok(mut value) = serde_json::from_str::<serde_json::Value>(contents) else {
                    tracing::warn!("profiles.json corrupto; usando builtins");
                    return ProfileCatalog::with_builtins();
                };
                if let Some(profiles) = value.get_mut("profiles").and_then(|p| p.as_array_mut()) {
                    profiles.retain(|p| {
                        p.get("kind")
                            .and_then(|k| k.as_str())
                            .map(|k| matches!(k, "Web" | "Lossless" | "Custom"))
                            .unwrap_or(false)
                    });
                }
                serde_json::from_value(value)
                    .map(|c: ProfileCatalog| c.sanitized())
                    .unwrap_or_else(|_| {
                        tracing::warn!("profiles.json irreparable; usando builtins");
                        ProfileCatalog::with_builtins()
                    })
            }
        }
    }

    fn persist(&self, catalog: &ProfileCatalog) {
        // Escritura atómica: un crash a mitad de write no debe dejar
        // profiles.json truncado.
        match serde_json::to_string_pretty(catalog) {
            Ok(json) => {
                if let Err(e) = crate::util::fs_io::write_atomic(&self.path, json.as_bytes()) {
                    tracing::error!("error escribiendo profiles.json: {e}");
                }
            }
            Err(e) => tracing::error!("error serializando profiles.json: {e}"),
        }
    }

    /// Devuelve una copia de todos los perfiles (built-ins + custom).
    pub fn all(&self) -> Vec<OptimizationProfile> {
        self.catalog.read().profiles.clone()
    }

    /// Devuelve el nombre del perfil por defecto.
    pub fn default_name(&self) -> String {
        self.catalog.read().default_name.clone()
    }

    /// Establece el perfil por defecto. Devuelve `false` si el nombre no existe.
    pub fn set_default(&self, name: &str) -> bool {
        let mut catalog = self.catalog.write();
        if !catalog
            .profiles
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case(name))
        {
            return false;
        }
        catalog.default_name = name.to_string();
        self.persist(&catalog);
        true
    }
}
