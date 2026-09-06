//! Token de cancelación cooperativa: thread-safe y barato de
//! consultar.
//!
//! Diseño:
//! - `CancellationToken` envuelve un `Arc<AtomicBool>`; consultar es
//!   un único load atómico.
//! - Es `Clone` y `Send + Sync`; los clones comparten el mismo átomo:
//!   cualquiera de ellos puede cancelar a los demás.
//! - La cancelación es **cooperativa**: activar el flag NO interrumpe
//!   un codec en ejecución. El pipeline comprueba el flag en fronteras
//!   significativas (antes de cada candidato, antes de la evaluación de
//!   calidad, antes de publicar) y detiene el trabajo restante. El codec
//!   que ya corría termina y su resultado se descarta (nunca se
//!   publica).
//! - Sin estado global: cada llamada a `optimize` crea un token nuevo
//!   (o acepta uno del caller) y lo suelta al volver.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Token de cancelación cooperativa. Barato de clonar y consultar.
///
/// Todos los clones comparten el mismo estado subyacente: activar la
/// cancelación en cualquier clon es visible para todos los demás.
#[derive(Debug, Clone)]
pub struct CancellationToken {
    inner: Arc<AtomicBool>,
}

impl CancellationToken {
    /// Crea un token nuevo, no cancelado.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Devuelve `true` si se pidió la cancelación (un único load
    /// atómico).
    #[inline]
    pub fn is_cancelled(&self) -> bool {
        self.inner.load(Ordering::Acquire)
    }

    /// Identidad física: `true` si ambos tokens comparten el mismo
    /// estado interno (son clones del mismo token original).
    ///
    /// Útil para limpiar un registro de token activo sin pisar el
    /// token de una ejecución posterior: si `token` fue reemplazado
    /// mientras nuestra tarea terminaba, `ptr_eq` devuelve `false` y
    /// sabemos que no debemos tocar el registro.
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    /// Pide la cancelación. Visible para todos los clones poco después
    /// de que la llamada vuelva.
    ///
    /// NO interrumpe ningún codec en ejecución: solo hace que el
    /// pipeline pare en la siguiente frontera.
    pub fn cancel(&self) {
        self.inner.store(true, Ordering::Release);
    }

    /// Cortocircuito para operaciones largas: `Err(Cancelled)` si ya
    /// se pidió la cancelación.
    pub fn check(&self) -> Result<(), Cancelled> {
        if self.is_cancelled() {
            Err(Cancelled)
        } else {
            Ok(())
        }
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

/// Error devuelto cuando una operación fue cancelada.
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("operation cancelled")]
pub struct Cancelled;

impl serde::Serialize for Cancelled {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str("cancelled")
    }
}

/// Handle opaco para cancelar un pipeline en ejecución desde fuera.
/// Internamente es un `CancellationToken`.
///
/// Es `Send + Sync` y se puede dropear en cualquier momento: dropear
/// el handle NO cancela el token — el pipeline pierde la capacidad de
/// ser cancelado externamente y corre hasta terminar.
pub struct CancelHandle {
    token: CancellationToken,
}

impl CancelHandle {
    pub fn new() -> Self {
        Self {
            token: CancellationToken::new(),
        }
    }

    /// Devuelve el token que debe consultar el pipeline.
    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }

    /// Pide la cancelación del run asociado.
    pub fn cancel(&self) {
        self.token.cancel();
    }

    /// Devuelve `true` si se pidió la cancelación.
    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }
}

impl Default for CancelHandle {
    fn default() -> Self {
        Self::new()
    }
}
