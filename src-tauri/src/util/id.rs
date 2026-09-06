//! Generador de IDs únicos monotónico atómico.

use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Devuelve el siguiente ID único (1-indexado, monotónico creciente).
pub fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}
