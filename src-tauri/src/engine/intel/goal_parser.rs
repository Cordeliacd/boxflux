//! GoalParser — interpreta un prompt en lenguaje natural y lo
//! traduce a un [`OptimizationGoal`].
//!
//! El usuario escribe, p.ej., *"lo más ligero posible sin perder
//! calidad visual"* y el motor hace lo correcto. No hay nada mágico:
//! es un clasificador determinístico por palabras clave — predecible
//! y auditable.
//!
//! ## Cómo funciona
//!
//! El prompt se normaliza (minúsculas, sin acentos) y se puntúa en 4
//! ejes:
//!
//! - `light`  — señales hacia "lo más ligero posible"
//! - `quality` — señales hacia "mantén la calidad"
//! - `speed`   — señales hacia "rápido"
//! - `lossless` — señales hacia "sin pérdida"
//!
//! Luego se aplica un árbol de decisión:
//!
//! 1. Si `lossless` domina → `Lossless`
//! 2. Si `light` Y `quality` ambos fuertes → `ExtremeLightweight`
//!    (es exactamente lo que este goal hace: más ligero sin perder)
//! 3. Si `light` solo (sin cuidado por calidad) → `MaximumCompression`
//! 4. Si `quality` solo → `Quality`
//! 5. Si `speed` domina → `Web`
//! 6. Default → `Balanced`
//!
//! El parser reconoce español e inglés. Ejemplos:
//!
//! | Prompt | Goal |
//! |--------|------|
//! | "lo más ligero posible sin perder calidad" | ExtremeLightweight |
//! | "extremely lightweight maintaining quality" | ExtremeLightweight |
//! | "comprimir al máximo" | MaximumCompression |
//! | "tiny file size" | MaximumCompression |
//! | "sin pérdida" | Lossless |
//! | "lossless" | Lossless |
//! | "calidad máxima" | Quality |
//! | "rápido" | Web |
//! | "balanceado" | Balanced |

use crate::engine::formats::Format;
use crate::engine::intel::goal::{GoalWeights, OptimizationGoal};

/// Normaliza un prompt: minúsculas, sin acentos, para matching robusto.
fn normalize(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' => 'a',
            'é' => 'e',
            'í' => 'i',
            'ó' => 'o',
            'ú' => 'u',
            'ü' => 'u',
            'ñ' => 'n',
            _ => c,
        })
        .collect::<String>()
}

/// Cuenta cuántas de `needles` aparecen en `haystack` (como substring).
fn count_matches(haystack: &str, needles: &[&str]) -> u32 {
    needles.iter().filter(|n| haystack.contains(*n)).count() as u32
}

/// Señales hacia el eje "ligero" (tamaño mínimo).
///
/// Nota: "máximo"/"extremo" NO están aquí porque son ambiguos —
/// "máxima calidad" significa calidad alta, "máximo ligero" significa
/// tamaño mínimo. Los tratamos como AMPLIFICADORES, no como señales
/// de eje.
const LIGHT_SIGNALS: &[&str] = &[
    "ligero",
    "ligera",
    "light",
    "small",
    "peque",
    "comprim",
    "compress",
    "tiny",
    "minimo",
    "mínimo",
    "reduce",
    "reduc",
    "ahorra",
    "save bytes",
    "menor",
    "file size",
    "tamaño",
];

/// Señales hacia el eje "calidad" (preservar visual).
const QUALITY_SIGNALS: &[&str] = &[
    "calidad",
    "quality",
    "visual",
    "identico",
    "idéntico",
    "manten",
    "preserve",
    "preserv",
    "look",
    "sharp",
    "fidel",
    "igual",
    "no perder",
    "sin perder",
    "no cambie",
    "no camb",
    "casi igual",
];

/// Señales hacia "sin pérdida" (bit-exacto).
const LOSSLESS_SIGNALS: &[&str] = &[
    "lossless",
    "sin perdida",
    "sin pérdida",
    "sin perder nada",
    "bit exact",
    "bit-exact",
    "exact",
    "perfect",
    "perfecto",
    "100%",
];

/// Señales hacia "rápido" / web.
const SPEED_SIGNALS: &[&str] = &[
    "rapido",
    "rápido",
    "fast",
    "quick",
    "speed",
    "web",
    "internet",
    "online",
    "browser",
    "navegador",
];

/// Señales hacia "balanceado".
const BALANCE_SIGNALS: &[&str] = &[
    "balance",
    "equilibr",
    "balanced",
    "medium",
    "medio",
    "default",
    "por defecto",
];

/// Interpreta un prompt en lenguaje natural y devuelve el goal.
///
/// Ver [`mod`] para la documentación del árbol de decisión.
pub fn parse_goal_prompt(prompt: &str) -> OptimizationGoal {
    let s = normalize(prompt);
    if s.trim().is_empty() {
        return OptimizationGoal::Balanced;
    }

    let light = count_matches(&s, LIGHT_SIGNALS);
    let quality = count_matches(&s, QUALITY_SIGNALS);
    let lossless = count_matches(&s, LOSSLESS_SIGNALS);
    let speed = count_matches(&s, SPEED_SIGNALS);
    let balance = count_matches(&s, BALANCE_SIGNALS);

    // Si el prompt nombra explícitamente un goal, úsalo.
    if balance >= 1 {
        return OptimizationGoal::Balanced;
    }
    if lossless >= 1 {
        return OptimizationGoal::Lossless;
    }

    // "lo más ligero posible SIN perder calidad": pide AMBAS cosas →
    // ExtremeLightweight es exactamente esa combinación (más ligero
    // manteniendo calidad). Basta light >= 1: "lo más ligero posible"
    // tiene un solo match de light y la intención es claramente extrema.
    let extreme_combo = light >= 1 && quality >= 1;
    let extreme_explicit = s.contains("extreme") || s.contains("extremo");
    if (extreme_combo || extreme_explicit) && quality >= 1 {
        return OptimizationGoal::ExtremeLightweight;
    }

    // Si solo pide ligero (sin cuidar calidad) → MaximumCompression.
    if light >= 1 && quality == 0 {
        return OptimizationGoal::MaximumCompression;
    }

    // Si solo pide calidad → Quality.
    if quality >= 1 && light == 0 {
        return OptimizationGoal::Quality;
    }

    // Si hay algo de ambos pero no la combinación extrema → Balanced.
    if light >= 1 && quality >= 1 {
        return OptimizationGoal::Balanced;
    }

    // Speed/web.
    if speed >= 1 {
        return OptimizationGoal::Web;
    }

    // Default.
    OptimizationGoal::Balanced
}

/// Conveniencia: parsea el prompt Y devuelve los `GoalWeights` resultantes.
pub fn parse_goal_weights(prompt: &str) -> GoalWeights {
    parse_goal_prompt(prompt).weights()
}

/// Conveniencia: parsea el prompt Y devuelve los formatos preferidos.
pub fn parse_preferred_formats(prompt: &str) -> Vec<Format> {
    parse_goal_prompt(prompt).weights().preferred_formats
}
