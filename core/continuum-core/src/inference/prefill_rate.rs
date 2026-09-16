//! THE MEASURED PREFILL RATE — per model, fresh, durable — and the prompt it can afford.
//!
//! Joel, 2026-09-16: "0.5B is insanely too small … a better fit even if slower"; Cormac's
//! measurement the same hour on the Intel Mac: a 4B decodes there at ~3 t/s — the same
//! per-stream rate the 0.5B was delivering — but prefills at 20.8 t/s, so the 18k-token
//! prompt every render carried (9.3k of tool registry, 9k of conversation) costs a
//! quarter of an hour before the first token. The blocker for a bigger mind on that tier
//! is not the model; it is a prompt sized for a box that prefills at 500 t/s.
//!
//! So the render budget is DERIVED, not chosen by size class (card eff9dad0: perception
//! scales with the model — never cripple the frontier for the 0.5B, never hand the 0.5B
//! the frontier's prompt): `[`TTFT_BUDGET`] × measured prefill t/s` is the prompt a turn
//! may cost, clamped below by the serving stack's own floor and above by the served
//! window. On the M5 (Ornith, ~400–500 t/s) sixty seconds buys ~25–30k and nothing
//! changes; on a 1.5B at ~80 t/s it buys ~5k, a working mind; a 4B at 20 t/s buys 1.2k,
//! below the floor — which is the honest verdict on that pairing.
//!
//! The rate is what the adapter already measures on every generation
//! (`prompt_per_second` from the server's timings, `GenerationTiming::prefill_tokens_per_second`),
//! kept here as one EMA per model with the knee's discipline: samples inside our own
//! transients are skipped, points go stale after an hour, the record survives a reboot.
//! `ContextBudget::from_window` reads it, so every bound in cognition scales with it —
//! one owner, no second profile type.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// Who is waiting on the first token. The TTFT budget is the last CHOSEN number in the
/// render-budget chain (card 7496ed9d, Cormac's review of #4124): 60 s is justified by
/// what the person on the other side feels — and a turn with nobody on the other side (a
/// detached benchmark solve, a self-tick, a consolidation) would gladly spend three
/// minutes of prefill for a materially better prompt. Same shape as the decode knee:
/// keep the derivation, make the input typed. The audience is chosen at the turn seam
/// (`cognition::audience`), never inferred from the prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Audience {
    /// Someone is waiting: a room turn (a human, a peer, a citizen's question).
    Interactive,
    /// Nobody is waiting: a detached solve, the self-cycle, a dream consolidation.
    Unattended,
}

impl Audience {
    /// Seconds of prefill a turn may cost before its first token, for this audience.
    pub const fn ttft(self) -> std::time::Duration {
        match self {
            Audience::Interactive => INTERACTIVE_TTFT,
            Audience::Unattended => UNATTENDED_TTFT,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Audience::Interactive => "interactive",
            Audience::Unattended => "unattended",
        }
    }
}

/// The latency law's line: time-to-act is what the person on the other side feels, and
/// a minute is already the p90 on the M5 today; this bound keeps a smaller box from
/// making it fifteen.
pub const INTERACTIVE_TTFT: std::time::Duration = std::time::Duration::from_secs(60);
/// Three times the interactive line: an unattended turn buys a fuller prompt with time
/// nobody is spending — bounded, because an act still has a tick deadline behind it.
pub const UNATTENDED_TTFT: std::time::Duration = std::time::Duration::from_secs(180);
/// The interactive line, kept under its historical name for the doc references.
pub const TTFT_BUDGET: std::time::Duration = INTERACTIVE_TTFT;
/// A prefill shorter than this measures the request overhead, not the lane — a time floor,
/// deliberately not a token count (a token floor is window-shaped; the noise is in the clock).
pub const MIN_SAMPLE: std::time::Duration = std::time::Duration::from_millis(250);
pub const MIN_SAMPLES: u32 = 4;
pub const FRESH_MS: u64 = 60 * 60 * 1000;
const EMA_ALPHA: f64 = 0.2;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct PrefillPoint {
    pub samples: u32,
    pub tps_ema: f64,
    pub last_ms: u64,
}

impl PrefillPoint {
    pub fn observe(&mut self, tps: f64, now_ms: u64) {
        if !tps.is_finite() || tps <= 0.0 {
            return;
        }
        if now_ms.saturating_sub(self.last_ms) > FRESH_MS {
            self.samples = 0;
        }
        self.tps_ema = if self.samples == 0 { tps } else { self.tps_ema + EMA_ALPHA * (tps - self.tps_ema) };
        self.samples = self.samples.saturating_add(1);
        self.last_ms = now_ms;
    }

    pub fn trusted_at(&self, now_ms: u64) -> bool {
        self.samples >= MIN_SAMPLES && now_ms.saturating_sub(self.last_ms) <= FRESH_MS
    }
}

/// The prompt (tokens) a turn may cost at `tps` inside the audience's TTFT budget,
/// clamped to `[floor, served]`. Pure — the one rule.
pub fn affordable_prompt_tokens(tps: f64, floor: u32, served: u32, audience: Audience) -> u32 {
    let raw = (tps * audience.ttft().as_secs_f64()).floor();
    let raw = if raw.is_finite() && raw > 0.0 { raw as u32 } else { 0 };
    raw.max(floor).min(served.max(floor))
}

static RATES: LazyLock<parking_lot::Mutex<BTreeMap<String, PrefillPoint>>> =
    LazyLock::new(|| parking_lot::Mutex::new(load()));

/// Record one measured prefill for `model`. Skips short prompts and our own transients
/// (a deploy in flight, the boot window) exactly as the decode knee does.
pub fn observe(model: &str, prefill_ms: f64, prefill_tps: f64) {
    if !(prefill_ms >= MIN_SAMPLE.as_millis() as f64) || !prefill_tps.is_finite() || prefill_tps <= 0.0 {
        return;
    }
    let now = now_ms();
    if let Some(why) = crate::inference::decode_knee::own_transient(now) {
        crate::probe!(
            class = "serving.prefill_rate.sample_skipped",
            model = model,
            prefill_ms,
            prefill_tps,
            why,
            "prefill sample skipped — inside our own transient, the rate is not the lane's"
        );
        return;
    }
    let mut rates = RATES.lock();
    let p = rates.entry(model.to_string()).or_default();
    let before = p.trusted_at(now).then_some(p.tps_ema);
    p.observe(prefill_tps, now);
    let after = p.trusted_at(now).then_some(p.tps_ema);
    // Persist when the rate becomes trusted or moves by more than a fifth: a reboot
    // starts from the measured rate, not from the served window.
    let moved = match (before, after) {
        (None, Some(_)) => true,
        (Some(b), Some(a)) => (a - b).abs() > b * 0.2,
        _ => false,
    };
    if moved {
        crate::probe!(
            class = "serving.prefill_rate.moved",
            model = model,
            prefill_tps = after.unwrap_or(0.0), // unwrap_or: `moved` implies `after` is Some; 0 is unreachable
            samples = p.samples as u64,
            "the measured prefill rate moved — the render budget follows it"
        );
        save_all(&rates);
    }
}

/// The trusted, fresh prefill rate for `model`, if measured.
pub fn rate_for(model: &str) -> Option<f64> {
    let now = now_ms();
    RATES.lock().get(model).filter(|p| p.trusted_at(now)).map(|p| p.tps_ema)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0) // unwrap_or: a clock before 1970 makes every point stale — the served window rules, nothing is clamped on a lie
}

fn default_path() -> Option<PathBuf> {
    crate::commands::benchmark::continuum_home()
        .ok()
        .map(|h| h.join("state").join("prefill-rate.json"))
}

fn load() -> BTreeMap<String, PrefillPoint> {
    default_path().map(|p| load_from(&p)).unwrap_or_default() // unwrap_or_default: no home = nothing remembered; the served window rules until measured
}

pub fn load_from(path: &Path) -> BTreeMap<String, PrefillPoint> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(), // unwrap_or_default: a corrupt record is forgotten, never trusted — the rate re-measures
        Err(_) => BTreeMap::new(),
    }
}

fn save_all(rates: &BTreeMap<String, PrefillPoint>) {
    if let Some(p) = default_path() {
        save_to(&p, rates);
    }
}

pub fn save_to(path: &Path, rates: &BTreeMap<String, PrefillPoint>) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(rates) {
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: u64 = 10_000_000_000;

    // what this catches (Cormac's Intel Mac, 2026-09-16): the budget is a rule over a
    // measured rate, not a size table — 500 t/s buys the M5 its whole prompt, 80 t/s buys a
    // 1.5B a working ~5k, 20 t/s buys a 4B less than the floor (the honest verdict); the
    // served window is the ceiling, the serving floor the ground, and a rate that is not
    // trusted (thin, stale) leaves the served window in charge.
    #[test]
    fn the_prompt_a_turn_may_cost_follows_the_measured_prefill_rate() {
        let live = Audience::Interactive;
        assert_eq!(affordable_prompt_tokens(500.0, 4096, 66_000, live), 30_000);
        assert_eq!(affordable_prompt_tokens(80.0, 4096, 32_768, live), 4_800);
        assert_eq!(affordable_prompt_tokens(20.8, 4096, 32_768, live), 4096, "below the floor: the floor");
        assert_eq!(affordable_prompt_tokens(5_000.0, 4096, 66_000, live), 66_000, "never above the served window");
        assert_eq!(affordable_prompt_tokens(f64::NAN, 4096, 66_000, live), 4096);
        // card 7496ed9d: nobody waiting buys a fuller prompt on the same lane — the 4B
        // that could not afford the floor interactively renders ~3.7k unattended; the
        // ceiling and the floor are unchanged.
        let alone = Audience::Unattended;
        assert_eq!(affordable_prompt_tokens(20.8, 4096, 32_768, alone), 4096, "still under the floor");
        assert_eq!(affordable_prompt_tokens(80.0, 4096, 32_768, alone), 14_400);
        assert_eq!(affordable_prompt_tokens(5_000.0, 4096, 66_000, alone), 66_000);
        assert!(Audience::Unattended.ttft() > Audience::Interactive.ttft());
        let mut p = PrefillPoint::default();
        for _ in 0..MIN_SAMPLES - 1 {
            p.observe(80.0, T);
        }
        assert!(!p.trusted_at(T), "three samples are not a rate");
        p.observe(80.0, T);
        assert!(p.trusted_at(T));
        assert!(!p.trusted_at(T + FRESH_MS + 1), "an hour-old rate is not this hour's");
        p.observe(200.0, T + FRESH_MS + 1);
        assert_eq!((p.samples, p.tps_ema), (1, 200.0), "a stale point restarts");
    }

    // what this catches: the record survives a reboot and a corrupt file is forgotten.
    #[test]
    fn the_rate_is_remembered_across_a_reboot_and_a_corrupt_record_is_forgotten() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("prefill-rate.json");
        let mut rates = BTreeMap::new();
        let mut p = PrefillPoint::default();
        for _ in 0..MIN_SAMPLES {
            p.observe(80.0, T);
        }
        rates.insert("m".to_string(), p);
        save_to(&path, &rates);
        assert_eq!(load_from(&path), rates);
        std::fs::write(&path, b"{nope").expect("write");
        assert!(load_from(&path).is_empty());
    }
}
