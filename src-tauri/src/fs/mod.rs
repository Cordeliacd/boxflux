//! Validación de seguridad de rutas: defensa contra path traversal,
//! sobreescritura de originales y permisos; más descubrimiento
//! recursivo de archivos soportados para el drag&drop de carpetas.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::engine::formats::Format;

/// Problema detectado al validar una ruta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ValidationIssue {
    Ok,
    EmptyPath,
    PathTraversal,
    DoesNotExist,
    NotAFile,
    NotADirectory,
    UnsupportedExtension,
    OutputSameAsInput,
    OutputWouldOverwriteOriginal,
    PermissionDenied,
    InvalidUnicode,
    TooLarge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub issue: ValidationIssue,
    #[serde(default)]
    pub message: String,
}

impl ValidationResult {
    pub fn ok() -> Self {
        Self {
            issue: ValidationIssue::Ok,
            message: String::new(),
        }
    }
    pub fn is_ok(&self) -> bool {
        self.issue == ValidationIssue::Ok
    }
}

/// Extensiones soportadas (case-insensitive).
pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "webp", "avif", "gif", "bmp", "tif", "tiff",
];

/// Comprueba si una extensión está soportada.
pub fn is_supported_extension(ext: &str) -> bool {
    let lower = ext.trim_start_matches('.').to_ascii_lowercase();
    SUPPORTED_EXTENSIONS.contains(&lower.as_str())
}

/// Comprueba si un path es seguro (no contiene `..` para evitar traversal).
pub fn is_path_safe(path: &Path) -> bool {
    let Some(s) = path.to_str() else {
        return false;
    };
    !s.split(std::path::MAIN_SEPARATOR).any(|c| c == "..")
}

/// Valida un archivo de entrada. `max_size_bytes=0` = sin límite.
pub fn validate_input_file(path: &Path, max_size_bytes: u64) -> ValidationResult {
    let Some(s) = path.to_str() else {
        return ValidationResult {
            issue: ValidationIssue::InvalidUnicode,
            message: format!("ruta no es UTF-8 válida: {}", path.display()),
        };
    };
    if s.is_empty() {
        return ValidationResult {
            issue: ValidationIssue::EmptyPath,
            message: "ruta vacía".into(),
        };
    }
    if !is_path_safe(path) {
        return ValidationResult {
            issue: ValidationIssue::PathTraversal,
            message: format!("ruta contiene '..': {}", path.display()),
        };
    }
    if !path.exists() {
        return ValidationResult {
            issue: ValidationIssue::DoesNotExist,
            message: format!("no existe: {}", path.display()),
        };
    }
    if !path.is_file() {
        return ValidationResult {
            issue: ValidationIssue::NotAFile,
            message: format!("no es un archivo regular: {}", path.display()),
        };
    }
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if !is_supported_extension(ext) {
            return ValidationResult {
                issue: ValidationIssue::UnsupportedExtension,
                message: format!("extensión no soportada: .{ext}"),
            };
        }
    } else {
        return ValidationResult {
            issue: ValidationIssue::UnsupportedExtension,
            message: "archivo sin extensión".into(),
        };
    }
    if let Ok(meta) = std::fs::metadata(path) {
        if max_size_bytes > 0 && meta.len() > max_size_bytes {
            return ValidationResult {
                issue: ValidationIssue::TooLarge,
                message: format!(
                    "archivo demasiado grande: {} bytes (max {})",
                    meta.len(),
                    max_size_bytes
                ),
            };
        }
    }
    // Best-effort readability check.
    if let Err(e) = std::fs::File::open(path) {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            return ValidationResult {
                issue: ValidationIssue::PermissionDenied,
                message: format!("permiso denegado: {}", path.display()),
            };
        }
    }
    ValidationResult::ok()
}

/// Valida que `output` no sea igual a `input` y que su directorio padre exista.
pub fn validate_output_path(output: &Path, input: Option<&Path>) -> ValidationResult {
    if let Some(input) = input {
        if let (Ok(in_abs), Ok(out_abs)) =
            (std::fs::canonicalize(input), std::fs::canonicalize(output))
        {
            if in_abs == out_abs {
                return ValidationResult {
                    issue: ValidationIssue::OutputSameAsInput,
                    message: format!("la salida es igual a la entrada: {}", output.display()),
                };
            }
        } else if input == output {
            return ValidationResult {
                issue: ValidationIssue::OutputSameAsInput,
                message: format!("la salida es igual a la entrada: {}", output.display()),
            };
        }
    }
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return ValidationResult {
                    issue: ValidationIssue::PermissionDenied,
                    message: format!(
                        "no se pudo crear el directorio padre {}: {e}",
                        parent.display()
                    ),
                };
            }
        }
    }
    ValidationResult::ok()
}

/// Descubre todos los archivos soportados en `dir` (recursivo por defecto).
/// Omite silenciosamente directorios con permiso denegado.
pub fn discover_supported_files(dir: &Path, recursive: bool) -> Vec<PathBuf> {
    if !dir.exists() || !dir.is_dir() {
        return Vec::new();
    }
    let mut out = Vec::new();
    if recursive {
        let walker = walkdir(dir, &mut out);
        walker;
    } else {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && is_supported_image_file(&path) {
                    if let Ok(abs) = std::fs::canonicalize(&path) {
                        out.push(abs);
                    } else {
                        out.push(path);
                    }
                }
            }
        }
    }
    out.sort();
    out
}

fn walkdir(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if file_type.is_dir() {
            walkdir(&path, out);
        } else if file_type.is_file() && is_supported_image_file(&path) {
            if let Ok(abs) = std::fs::canonicalize(&path) {
                out.push(abs);
            } else {
                out.push(path);
            }
        }
    }
}

/// Comprueba si un path es un archivo de imagen soportado.
pub fn is_supported_image_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(is_supported_extension)
        .unwrap_or(false)
}

/// Detecta el formato desde un path.
pub fn detect_format(path: &Path) -> Format {
    Format::from_path(path)
}

/// Computa la ruta de salida para `input_path` según `mode`.
///
/// Modes: `"same_folder"`, `"output_subfolder"`, `"custom_folder"`.
/// `filename_template` placeholders: `{name}`, `{ext}`, `{format}`.
pub fn compute_output_path(
    input_path: &Path,
    mode: &str,
    custom_folder: Option<&str>,
    preserve_structure: bool,
    filename_template: &str,
    target_extension: Option<&str>,
) -> PathBuf {
    let parent = input_path.parent().unwrap_or_else(|| Path::new("."));
    let stem = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let ext = input_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let new_ext = target_extension.unwrap_or(ext);
    let template = if filename_template.is_empty() {
        "{name}_optimized{format}"
    } else {
        filename_template
    };
    let filename = template
        .replace("{name}", stem)
        .replace("{ext}", &format!(".{}", ext))
        .replace("{format}", &format!(".{}", new_ext));

    match mode {
        "same_folder" => parent.join(&filename),
        "output_subfolder" => parent.join("BoxFlux").join(&filename),
        "custom_folder" => {
            let Some(custom) = custom_folder else {
                return parent.join(&filename);
            };
            let base = PathBuf::from(custom);
            if preserve_structure {
                if let Some(p) = parent.file_name() {
                    base.join(p).join(&filename)
                } else {
                    base.join(&filename)
                }
            } else {
                base.join(&filename)
            }
        }
        _ => parent.join(&filename),
    }
}
