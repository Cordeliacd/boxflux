//! Utilidades de filesystem.

use std::io::Write;
use std::path::Path;

/// Escribe `bytes` en `path` de forma ATÓMICA: primero escribe a un
/// archivo temporal hermano y luego hace `rename` sobre el destino.
///
/// Con tmp+rename, o el archivo anterior queda intacto, o el nuevo está
/// completo — nunca un estado intermedio (un JSON truncado al recargar
/// caería en defaults y el usuario perdería sus datos). El `rename`
/// dentro del mismo directorio es atómico en POSIX y en NTFS.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        // Flush + sync para que el rename no adelante datos que aún
        // están en el buffer del SO si el proceso muere justo después.
        f.sync_all()?;
    }
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            // Best-effort: no dejar el tmp tirado si el rename falló.
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}
