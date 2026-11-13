//! THE SPEED PULSE: per-persona decode + prefill tokens/sec from the last completed generation
//! (llama-server's own `timings`), recorded by the inference adapter at stream close. A serving
//! measurement, so the engine owns it; the vitals radiator reads it as `tps`/`pfx` while fresh
//! and the roster tile draws them as micro-speedometers (Joel: "speedometers not massive text;
//! thin lines and meters").

use std::collections::HashMap;
use std::time::{Duration, Instant};

use uuid::Uuid;

static SPEED_PULSE: std::sync::LazyLock<std::sync::Mutex<HashMap<Uuid, (f64, f64, Instant)>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

/// Record the last generation's speeds for `persona` (uuid-string; non-uuid
/// callers — CLI, probes — are silently not a tile and skip).
pub fn record_speed(persona: &str, decode_tps: f64, prefill_tps: f64) {
    let Ok(id) = Uuid::parse_str(persona) else {
        return;
    };
    let mut pulse = SPEED_PULSE.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: a poisoned pulse map still records its last speeds
    pulse.insert(id, (decode_tps, prefill_tps, Instant::now()));
}

/// `persona`'s last (decode, prefill) tokens/sec when sampled within `fresh`; `None` when
/// there is none or it is older (a stale needle is a lie, so the reader names the bound).
pub fn fresh_speed(persona: Uuid, fresh: Duration) -> Option<(f64, f64)> {
    let pulse = SPEED_PULSE.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: a poisoned pulse map still reads its last speeds
    pulse.get(&persona).filter(|(_, _, at)| at.elapsed() < fresh).map(|(d, p, _)| (*d, *p))
}
