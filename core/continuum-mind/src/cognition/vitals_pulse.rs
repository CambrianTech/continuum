//! The mind's per-persona PULSES: acts executed and faculties fired (reason, recall,
//! focus), stamped at the one seam each event already passes. Mind facts, recorded by the
//! mind; the vitals radiator (`ipc::vitals_emitter`) samples them to light the HUD, so the
//! mind never reaches up into the socket layer to say it acted.

use std::collections::HashMap;
use std::time::Duration;

use uuid::Uuid;

/// The ACT PULSE: per-persona executed-act counter, bumped by the act-observe
/// path each time a tool act completes. A HELD-WORK turn is one long service
/// cycle — the cycle-delta reads 0 exactly while she works hardest (Joel,
/// 2026-08-31: "AIs look still greyed" during a live benchmark round). Acts
/// are the true thinking-right-now signal inside a turn; activity radiates the
/// LOUDER of the two deltas.
static ACT_PULSE: std::sync::LazyLock<std::sync::Mutex<HashMap<Uuid, u64>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

/// Record `n` completed acts for `persona` — called by the act-observe apply
/// path; cheap (one map bump under a short lock, no allocation on the hot path
/// beyond the entry).
pub fn record_acts(persona: Uuid, n: u64) {
    let mut pulse = ACT_PULSE.lock().unwrap_or_else(|e| e.into_inner());
    *pulse.entry(persona).or_insert(0) += n;
}

/// Drain `persona`'s executed-act count since the last sample (the radiator's read).
pub fn take_acts(persona: Uuid) -> u64 {
    let mut pulse = ACT_PULSE.lock().unwrap_or_else(|e| e.into_inner());
    pulse.remove(&persona).unwrap_or(0) // unwrap_or: no entry means no act since the last drain, which IS zero acts
}

/// FACULTY PULSES — the axes the persona HUD names (`reason`, `recall`, `focus`;
/// see `brainRegions` in the semantic layer). Each is a timestamp (+ count) the
/// emitter reads with a freshness window: the region lights when the faculty
/// fires and fades as the window elapses. Recorded at the ONE seam each event
/// already passes — lane acquired (reasoning), recall surfaced (memory), a
/// directed turn admitted (focus). Before 2026-09-04 the emitter radiated
/// `activity/queue/tps/pfx` while the HUD asked for `reason/recall/act/speed/
/// focus`: four of five regions read "awaiting signal" forever.
static FACULTY_PULSE: std::sync::LazyLock<
    std::sync::Mutex<HashMap<(Uuid, &'static str), (u64, std::time::Instant)>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

fn record_faculty(persona: Uuid, axis: &'static str, n: u64) {
    let mut pulse = FACULTY_PULSE.lock().unwrap_or_else(|e| e.into_inner());
    pulse.insert((persona, axis), (n, std::time::Instant::now()));
}

/// A deliberation lane was acquired — she is reasoning now.
pub fn record_reasoning(persona: Uuid) {
    record_faculty(persona, "reason", 1);
}

/// `n` memories surfaced into this turn.
pub fn record_recall(persona: Uuid, n: u64) {
    record_faculty(persona, "recall", n);
}

/// A DIRECTED turn was admitted — someone addressed her (a human, an agent,
/// or a peer naming her).
pub fn record_focus(persona: Uuid) {
    record_faculty(persona, "focus", 1);
}

/// Level for a faculty pulse: full while fresh, fading linearly to 0 over the
/// window; `None` when nothing fired within it (an honest "awaiting").
pub fn faculty_level(
    persona: Uuid,
    axis: &'static str,
    window: Duration,
    full_scale: u64,
) -> Option<u8> {
    let pulse = FACULTY_PULSE.lock().unwrap_or_else(|e| e.into_inner());
    let (n, at) = pulse.get(&(persona, axis))?;
    let age = at.elapsed();
    if age >= window {
        return None;
    }
    let fresh = 1.0 - age.as_secs_f64() / window.as_secs_f64();
    let base = pct_u64(*n, full_scale) as f64;
    Some((base * fresh).round().clamp(0.0, 100.0) as u8)
}

/// `v` as a percentage of `full`, saturating at 100 (a meter level).
pub fn pct_u64(v: u64, full: u64) -> u8 {
    (v.saturating_mul(100) / full.max(1)).min(100) as u8 // head-cut-ok: a meter level clamped to 100, not text
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a faculty pulse lights its axis while fresh and reads
    // ABSENT (not zero) once the window elapses — the HUD's "awaiting signal" is
    // an honest no-signal, never a stale full bar. Recorded at the seams the
    // deliberation/recall/admission paths already pass.
    #[test]
    fn a_faculty_pulse_lights_then_goes_absent() {
        let p = Uuid::from_u128(0x77);
        assert!(faculty_level(p, "reason", Duration::from_secs(60), 1).is_none());
        record_reasoning(p);
        assert_eq!(
            faculty_level(p, "reason", Duration::from_secs(60), 1),
            Some(100)
        );
        record_recall(p, 3);
        assert_eq!(
            faculty_level(p, "recall", Duration::from_secs(60), 6),
            Some(50)
        );
        assert!(faculty_level(p, "recall", Duration::from_millis(0), 6).is_none());
        record_focus(p);
        assert_eq!(
            faculty_level(p, "focus", Duration::from_secs(60), 1),
            Some(100)
        );
    }
}
