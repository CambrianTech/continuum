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

use continuum_base::time::EpochMs;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// A rate and WHERE IT CAME FROM — shared with the decode side on purpose.
///
/// "A measured rate with its provenance" is one concept, and the ladder over it (fresh →
/// stale → the box's most conservative curve → nothing) is one rule; a second copy of
/// either here would be the parallel-allocator shape this crate bans. `decode_knee`
/// landed them first (#4283, after a restart left every point untrusted, the rate read
/// 0.0 and an allowance fell to a floor no thinking pass could hold), so the prefill side
/// re-exports rather than re-declares.
pub use crate::inference::decode_knee::{MeasuredRate, RateSource};

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

    /// A TRIPPED BOUND IS A MEASUREMENT. A turn that carried `prompt_tokens` and tripped a
    /// `bound` before the lane sent headers proves this box prefilled at MOST
    /// `prompt_tokens / bound` over that wait — an upper bound on the rate, and the only
    /// evidence such a turn leaves. Feed it as one: the estimate can only come DOWN to
    /// the bound (a failure is evidence of slowness and nothing else), the point counts a
    /// sample and refreshes `last_ms` so the ladder keeps serving it, and the next
    /// completed prefill (a real rate) moves it back up through the ordinary EMA.
    ///
    /// Without this the loop is closed the wrong way (card c30a4757, the IntelMac
    /// 09:15–10:15Z 2026-09-22): the cap admitted 1907 tokens on a stale 63 t/s point
    /// while llama-server measured 26 t/s; 25 of 29 turns tripped the 300 s floor; a
    /// trip yields no `observe`, so the point never moved and the 25th trip changed
    /// exactly as much as the first. The failure destroyed its own measurement.
    pub fn observe_at_most(&mut self, tps_upper: f64, now_ms: u64) {
        if !tps_upper.is_finite() || tps_upper <= 0.0 {
            return;
        }
        if now_ms.saturating_sub(self.last_ms) > FRESH_MS {
            self.samples = 0;
        }
        self.tps_ema = if self.samples == 0 { tps_upper } else { self.tps_ema.min(tps_upper) };
        self.samples = self.samples.saturating_add(1);
        self.last_ms = now_ms;
    }

    pub fn trusted_at(&self, now_ms: u64) -> bool {
        self.samples >= MIN_SAMPLES && now_ms.saturating_sub(self.last_ms) <= FRESH_MS
    }

}

impl crate::inference::decode_knee::OwnRate for PrefillPoint {
    /// Trusted = fresh. Otherwise ANY evidence of this model's own rate (one recent
    /// sample, or an old trusted average) ranks as stale — above every other model's
    /// point, which the ladder reaches only when this model has none. Requiring
    /// `MIN_SAMPLES` here too meant that after any hour's gap the first sample reset the
    /// count and the served model's own 105.7 tokens/s (persisted, 4 samples) lost to a
    /// 30-day-old 575 tokens/s from a model that was not even loaded (the M5,
    /// 2026-10-18 10:41Z on, `rate_source=conservative`): five times the real rate.
    fn own_rate(&self, now_ms: u64) -> Option<(f64, RateSource)> {
        if self.trusted_at(now_ms) {
            return Some((self.tps_ema, RateSource::Fresh));
        }
        (self.samples >= 1 && self.tps_ema > 0.0).then_some((self.tps_ema, RateSource::Stale))
    }
}

/// The prompt (tokens) a turn may cost at `tps` inside the audience's TTFT budget,
/// clamped to `[floor, served]`. Pure — the one rule.
///
/// On slow hardware (~30–40 tps) with an interactive TTFT of 60 s this yields
/// ~1800–2400 tokens; fast GPU nodes (1000+ tps) reach 60 k+. The guard files in
/// `source-hygiene` once described the clip as a product of two constants, but the
/// multiplication is `tps * audience.ttft()` where TPS is observed from hardware.
pub fn affordable_prompt_tokens(tps: f64, floor: u32, served: u32, audience: Audience) -> u32 {
    let raw = (tps * audience.ttft().as_secs_f64()).floor();
    let raw = if raw.is_finite() && raw > 0.0 { raw as u32 } else { 0 };
    raw.max(floor).min(served.max(floor))
}

/// The conversation fill a box that has NEVER measured a prefill may spend, in tokens.
///
/// A FLOOR, and named as one: it is consulted only when the ladder returns
/// [`RateSource::None`], and it never clamps a box that HAS measured. The value is the
/// substrate's own bootstrap working set — the prompt it already assumes a mind needs
/// before anything about that mind has been measured — so the unmeasured case borrows a
/// number the substrate owns instead of inventing a second one. The first completed
/// generation on the box replaces it with a measurement ([`observe`]).
pub const UNMEASURED_FILL_FLOOR_TOKENS: usize =
    crate::inference::planning::serving_plan::BOOTSTRAP_WORKING_SET as usize;

/// The one ladder, shared with the decode axis — see [`crate::inference::decode_knee::rate_from`].
pub use crate::inference::decode_knee::rate_from;

/// This box's prefill rate for `model` WITH its provenance — the ladder above. Prefer
/// this over [`rate_for`] wherever an absence would otherwise become a constant: the
/// difference between the two is a stale-but-real measurement and a guess.
pub fn measured_rate_for(model: &str) -> MeasuredRate {
    // A pre-1970 clock reads 0 here: every point is stale, so the served window rules and nothing is clamped on a lie.
    let now = EpochMs::now().millis();
    rate_from(&RATES.lock(), model, now)
}

static RATES: LazyLock<parking_lot::Mutex<BTreeMap<String, PrefillPoint>>> =
    LazyLock::new(|| parking_lot::Mutex::new(load()));

/// Record one measured prefill for `model`. Skips short prompts and our own transients
/// (a deploy in flight, the boot window) exactly as the decode knee does.
pub fn observe(model: &str, prefill_ms: f64, prefill_tps: f64) {
    if !(prefill_ms >= MIN_SAMPLE.as_millis() as f64) || !prefill_tps.is_finite() || prefill_tps <= 0.0 {
        return;
    }
    let now = EpochMs::now().millis();
    if let Some(why) = crate::inference::decode_knee::own_transient(now) {
        continuum_base::probe!(
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
        continuum_base::probe!(
            class = "serving.prefill_rate.moved",
            model = model,
            prefill_tps = after.unwrap_or(0.0), // unwrap_or: `moved` implies `after` is Some; 0 is unreachable
            samples = p.samples as u64,
            "the measured prefill rate moved — the render budget follows it"
        );
        save_all(&rates);
    }
}

/// Record a TRIPPED pre-first-byte bound for `model` as the upper-bound measurement it
/// is ([`PrefillPoint::observe_at_most`]): `prompt_tokens` sent, no headers inside
/// `bound`. Skips our own transients exactly as [`observe`] does — a relaunch under the
/// request is not the lane's rate. Persists whenever the point moves, so a reboot
/// starts from what the box proved it could NOT do, not from last hour's optimism.
pub fn observe_bound(model: &str, prompt_tokens: usize, bound: std::time::Duration) {
    let secs = bound.as_secs_f64();
    if prompt_tokens == 0 || !(secs > 0.0) {
        return;
    }
    let tps_upper = prompt_tokens as f64 / secs;
    let now = EpochMs::now().millis();
    if let Some(why) = crate::inference::decode_knee::own_transient(now) {
        continuum_base::probe!(
            class = "serving.prefill_rate.sample_skipped",
            model = model,
            prompt_tokens = prompt_tokens as u64,
            bound_secs = bound.as_secs(),
            why,
            "tripped-bound sample skipped — inside our own transient, the wait is not the lane's"
        );
        return;
    }
    let mut rates = RATES.lock();
    let p = rates.entry(model.to_string()).or_default();
    let before = p.tps_ema;
    p.observe_at_most(tps_upper, now);
    continuum_base::probe!(
        class = "serving.prefill_rate.bounded",
        model = model,
        prompt_tokens = prompt_tokens as u64,
        bound_secs = bound.as_secs(),
        tps_upper,
        tps_before = before,
        tps_after = p.tps_ema,
        samples = p.samples as u64,
        "a tripped bound is a prefill measurement — the rate can only come down to it; the next fill cap follows"
    );
    if p.tps_ema != before {
        save_all(&rates);
    }
}

/// The trusted, fresh prefill rate for `model`, if measured.
pub fn rate_for(model: &str) -> Option<f64> {
    let now = EpochMs::now().millis();
    RATES.lock().get(model).filter(|p| p.trusted_at(now)).map(|p| p.tps_ema)
}

fn default_path() -> Option<PathBuf> {
    crate::paths::continuum_home()
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

    fn trusted(tps: f64, at: u64) -> PrefillPoint {
        let mut p = PrefillPoint::default();
        for _ in 0..MIN_SAMPLES {
            p.observe(tps, at);
        }
        p
    }

    // what this catches: an absence being laundered into a constant one rung too early.
    // The decode side learned this the hard way (#4283): the M5 restarted, every point
    // went untrusted, the rate read 0.0 and the allowance fell to a floor no thinking
    // pass could hold. Stale is evidence; a constant is not.
    #[test]
    fn the_prefill_rate_ladder_is_fresh_then_stale_then_conservative_then_absent() {
        let mut rates = BTreeMap::new();
        assert_eq!(rate_from(&rates, "m", T), MeasuredRate::UNKNOWN, "nothing measured, ever");

        rates.insert("m".to_string(), trusted(400.0, T));
        let fresh = rate_from(&rates, "m", T);
        assert_eq!((fresh.source, fresh.tps), (RateSource::Fresh, Some(400.0)));

        // An hour later the same point is stale — still this box's measurement.
        let stale = rate_from(&rates, "m", T + FRESH_MS + 1);
        assert_eq!((stale.source, stale.tps), (RateSource::Stale, Some(400.0)));

        // A model with no point of its own borrows the SLOWEST rate the box holds.
        rates.insert("slow".to_string(), trusted(70.0, T));
        let borrowed = rate_from(&rates, "never-served", T);
        assert_eq!((borrowed.source, borrowed.tps), (RateSource::Conservative, Some(70.0)));

        // The served model's OWN evidence, however thin, outranks every other model's point.
        // This assertion used to read "one sample is not a rate" (UNKNOWN), and that rule is
        // what let a 30-day-old 575 tok/s from a model that was not loaded stand in for the
        // served model's 105.7 after an hour's gap reset its count (the M5, 2026-10-18 10:41Z
        // on): a thin own point is evidence about THIS box; an old point about another model
        // is not evidence about this one.
        let mut thin = BTreeMap::new();
        let mut p = PrefillPoint::default();
        p.observe(105.7, T);
        thin.insert("m".to_string(), p);
        thin.insert("unloaded".to_string(), trusted(575.6, T - 30 * 24 * 60 * 60 * 1000));
        let own = rate_from(&thin, "m", T);
        assert_eq!((own.source, own.tps), (RateSource::Stale, Some(105.7)), "one sample of the served model is evidence, and it wins");
        // A model with NO point of its own still borrows the slowest the box holds.
        let borrowed = rate_from(&thin, "never-served", T);
        assert_eq!((borrowed.source, borrowed.tps), (RateSource::Conservative, Some(105.7)));
    }

    // what this catches (card c30a4757, the IntelMac 09:15–10:15Z 2026-09-22): a
    // tripped header wait leaving the rate exactly where it was. The point held a stale
    // 63 t/s; the cap admitted 1907 tokens; llama-server prefilled at 26; 25 of 29 turns
    // tripped the 300 s floor and — because only a COMPLETED prefill called `observe` —
    // none of them moved the point. The failure destroyed its own measurement. A trip
    // is an upper-bound observation: the rate comes DOWN to prompt/bound, the point
    // stays served (a sample, a fresh stamp), the next cap follows, and a later real
    // prefill lifts it back through the ordinary EMA. It never raises the estimate.
    #[test]
    fn a_tripped_bound_is_a_prefill_measurement_that_can_only_lower_the_rate() {
        let mut p = trusted(63.0, T);
        let before = rate_from(&[("m".to_string(), p.clone())].into(), "m", T).tps;
        assert_eq!(before, Some(63.0), "the stale-but-served rate that sized the failing prompt");

        // 1907 tokens sent, no headers in 300 s: the lane prefilled at most ~6.4 t/s.
        p.observe_at_most(1907.0 / 300.0, T + 1);
        assert!(p.tps_ema < 6.4 && p.tps_ema > 6.3, "the rate came down to the bound: {}", p.tps_ema);
        assert_eq!(p.last_ms, T + 1, "a trip refreshes the point — it is this hour's evidence");
        assert!(p.trusted_at(T + 1), "still a rung: the ladder serves the lowered rate");
        let after = rate_from(&[("m".to_string(), p.clone())].into(), "m", T + 1).tps;
        assert!(after.is_some_and(|tps| tps <= 1907.0 / 300.0), "the next prompt is sized from the trip: {after:?}");

        // A bound ABOVE the estimate is not evidence of speed — it changes nothing.
        let held = p.tps_ema;
        p.observe_at_most(500.0, T + 2);
        assert_eq!(p.tps_ema, held, "a trip can never raise the rate");

        // The next completed prefill — a real rate — lifts it back through the EMA.
        p.observe(26.0, T + 3);
        assert!(p.tps_ema > held && p.tps_ema < 26.0, "recovery is measured, not assumed: {}", p.tps_ema);

        // Garbage is not a measurement.
        let same = p.clone();
        p.observe_at_most(0.0, T + 4);
        p.observe_at_most(f64::NAN, T + 4);
        assert_eq!(p, same);

        // A point with NO history takes the bound as its first sample: an unmeasured box
        // that has just proved it is slow starts from that proof, not from nothing.
        let mut fresh = PrefillPoint::default();
        fresh.observe_at_most(10.0, T);
        assert_eq!((fresh.samples, fresh.tps_ema), (1, 10.0));
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
