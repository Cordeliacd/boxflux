//! Publicación atómica de archivos optimizados.
//!
//! Invariante estricta: un codec NUNCA escribe directamente en la ruta
//! final del usuario. Escribe en un temporal del MISMO directorio, el
//! pipeline valida el temporal y solo entonces se renombra atómicamente.
//!
//! Mismo directorio porque `rename(2)` solo es atómico dentro de un
//! filesystem; un rename cruzado degrada a copy+remove, no atómico y
//! capaz de dejar un destino parcial si el proceso muere a medias.
//!
//! Temporales: `.<dest-stem>.boxflux-<pid>-<counter>.tmp` — punto
//! inicial (oculto en Unix) + PID + contador para unicidad entre runs
//! concurrentes y llamadas del mismo proceso.
//!
//! Fallos: si el encode, la validación o el rename fallan, el temporal
//! se elimina (best-effort) y el destino previo se conserva. Si el
//! destino ya existe, la publicación se RECHAZA: nunca se sobrescribe
//! en silencio un archivo del usuario.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Errores posibles durante la publicación atómica.
#[derive(Debug, thiserror::Error)]
pub enum OutputSafetyError {
    #[error("destination already exists: {0}")]
    DestinationExists(PathBuf),
    #[error("input and output resolve to the same file: {0}")]
    SameAsInput(PathBuf),
    #[error("temporary file already exists: {0}")]
    TempExists(PathBuf),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("validation failed: {0}")]
    ValidationFailed(String),
    #[error("rename failed (cross-filesystem?): {0}")]
    RenameFailed(String),
}

/// Resultado de una publicación atómica exitosa.
#[derive(Debug, Clone)]
pub struct PublishedFile {
    /// Ruta final, pública.
    pub path: PathBuf,
    /// Tamaño en disco tras la publicación (bytes).
    pub size: u64,
}

/// Genera una ruta temporal única para `destination`.
///
/// El temporal vive en el MISMO directorio que el destino (para que
/// `rename` sea atómico) y usa punto inicial + PID + contador para
/// evitar colisiones con runs concurrentes.
pub fn temporary_path_for(destination: &Path) -> PathBuf {
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let stem = destination
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let ext = destination
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("tmp");
    let tmp_name = format!(".{stem}.boxflux-{pid}-{counter}.{ext}.tmp");
    destination
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(tmp_name)
}

/// Comprueba que `input` y `destination` no resuelvan al mismo archivo.
///
/// Defensa en profundidad: la API pública es llamable directamente y
/// no puede fiarse de validaciones externas.
pub fn ensure_distinct(input: &Path, destination: &Path) -> Result<(), OutputSafetyError> {
    if input == destination {
        return Err(OutputSafetyError::SameAsInput(destination.to_path_buf()));
    }
    // Canonicalizar ambos; si falla (p.ej. el destino aún no existe),
    // cae a la comparación léxica de arriba.
    if let (Ok(in_abs), Ok(out_abs)) = (
        std::fs::canonicalize(input),
        std::fs::canonicalize(destination),
    ) {
        if in_abs == out_abs {
            return Err(OutputSafetyError::SameAsInput(destination.to_path_buf()));
        }
    }
    Ok(())
}

/// Publica `source` en `destination` atómicamente.
///
/// `source` debe existir ya (es el temporal que escribió el codec) y
/// `destination` NO debe existir (nunca se sobrescribe en silencio).
/// Ambos deben estar en el mismo filesystem — garantizado porque el
/// temporal se genera en el directorio padre del destino, salvo que el
/// caller se salte `temporary_path_for`.
///
/// En éxito devuelve la ruta publicada y su tamaño en disco. En fallo,
/// el temporal se elimina (best-effort) y el destino queda intacto.
pub fn publish_atomic(
    source: &Path,
    destination: &Path,
) -> Result<PublishedFile, OutputSafetyError> {
    if source == destination {
        // Misma ruta: nada que hacer; verificar que existe y devolver
        // el tamaño.
        let size = std::fs::metadata(source)?.len();
        return Ok(PublishedFile {
            path: destination.to_path_buf(),
            size,
        });
    }
    if destination.exists() {
        return Err(OutputSafetyError::DestinationExists(
            destination.to_path_buf(),
        ));
    }
    // El fuente debe ser un archivo regular (uno vacío explícito vale
    // para algunos formatos).
    let meta = std::fs::metadata(source)?;
    if !meta.is_file() {
        // Limpiar el temporal inválido.
        let _ = std::fs::remove_file(source);
        return Err(OutputSafetyError::ValidationFailed(format!(
            "source is not a regular file: {}",
            source.display()
        )));
    }

    // rename(2) es atómico dentro del mismo filesystem, y el temporal
    // está en el padre del destino (salvo symlink exótico a otro
    // filesystem).
    if let Err(e) = std::fs::rename(source, destination) {
        // Rename cruzado devuelve EXDEV: fallback a copy + remove, solo
        // si el destino sigue sin existir (otro thread pudo crearlo
        // entre el check y el rename).
        if destination.exists() {
            let _ = std::fs::remove_file(source);
            return Err(OutputSafetyError::DestinationExists(
                destination.to_path_buf(),
            ));
        }
        // copy + remove NO es atómico: un crash a medias deja un destino
        // parcial. Se acepta el riesgo solo porque el rename cruzado es
        // exótico aquí (el temporal está en el padre del destino).
        if let Err(copy_err) = std::fs::copy(source, destination) {
            let _ = std::fs::remove_file(source);
            return Err(OutputSafetyError::RenameFailed(format!(
                "rename failed ({e}); copy fallback also failed ({copy_err})"
            )));
        }
        let _ = std::fs::remove_file(source);
        let size = std::fs::metadata(destination)?.len();
        return Ok(PublishedFile {
            path: destination.to_path_buf(),
            size,
        });
    }
    let size = std::fs::metadata(destination)?.len();
    Ok(PublishedFile {
        path: destination.to_path_buf(),
        size,
    })
}

/// Ejecuta una operación de codec que produce un temporal y publica el
/// resultado atómicamente en `destination`.
///
/// `producer` recibe la ruta temporal donde debe escribir. Si devuelve
/// `Ok(())`, el temporal se valida y se renombra atómicamente a
/// `destination`. Si devuelve `Err`, el temporal se elimina
/// (best-effort) y el error se propaga.
///
/// El caller es responsable del check input≠destino (`ensure_distinct`)
/// ANTES de llamar aquí: la ruta de input es específica del codec.
pub fn with_temp_output<P, E>(
    destination: &Path,
    producer: P,
) -> Result<PublishedFile, OutputSafetyError>
where
    P: FnOnce(&Path) -> Result<(), E>,
    E: std::fmt::Display,
{
    let tmp = temporary_path_for(destination);
    // Si existe un temporal huérfano de un run anterior que crashó,
    // intentar eliminarlo y seguir; si no se puede, rechazar — mejor
    // que sobrescribir en silencio y enmascarar un problema real de
    // filesystem.
    if tmp.exists() {
        // Intento best-effort de eliminar el temporal viejo.
        if std::fs::remove_file(&tmp).is_err() {
            return Err(OutputSafetyError::TempExists(tmp));
        }
    }
    match producer(&tmp) {
        Ok(()) => publish_atomic(&tmp, destination),
        Err(e) => {
            // El codec falló: limpiar el temporal parcial.
            let _ = std::fs::remove_file(&tmp);
            Err(OutputSafetyError::ValidationFailed(format!(
                "codec failed before publication: {e}"
            )))
        }
    }
}

/// Limpieza best-effort de temporales `.boxflux-*` huérfanos en `dir`.
/// La llama el pipeline al final de un run. Los errores se loguean a
/// stderr y se ignoran: un fallo de limpieza no debe invalidar una
/// decisión exitosa.
pub fn sweep_stale_temporaries(dir: &Path) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        if name_str.starts_with(".") && name_str.contains(".boxflux-") && name_str.ends_with(".tmp")
        {
            if let Err(e) = std::fs::remove_file(entry.path()) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    eprintln!(
                        "boxflux: failed to sweep stale temporary {}: {e}",
                        entry.path().display()
                    );
                }
            }
        }
    }
}
