//! Liveness responsibility of the llama-server process owner.
/// Wall-clock ms of the last REAL generation that produced tokens on the served lane.
///
/// Health is recent DELIVERY, never a synthetic probe. The decode heartbeat
/// (`serving_daemon::spawn_health_heartbeat_if_due`) runs a real multi-token generation
/// through the LIVE slots, so it competes for the same slots as actual work: a lane saturated
/// by long prefills cannot hand the probe a slot, the probe reads that as "no decode", and two
/// misses relaunch a lane that was never wedged — just busy. Glass-boxed on the SWE bench
/// (v13): `serving.health {ok:false} x2 -> {action:"relaunch"}` mid-run, then every downstream
/// generate refused with `serving: <none>`.
///
/// Real tokens are the honest liveness evidence, and they cost nothing to observe — a lane that
/// just decoded for a persona is provably not wedged. So the probe is for QUIET lanes, which is
/// exactly when it is both cheap and meaningful. Same principle as the airc delivery receipts:
/// health is recently-ACKed delivery, never the existence of a connection.
/// [[a-benchmark-zero-is-a-claim-about-the-harness-until-proven-otherwise]]
pub(super) static LAST_REAL_DECODE_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);


/// Consecutive REAL generations that FAILED on the served lane since the last success —
/// the other half of "delivery beats probing" (#363). The 2026-08-07 blackout proved a
/// wedge class the 1-token smoke probe cannot see: an UNDERSIZED slot rejects the
/// fleet's real 12k-token prompts (every citizen turn dies mid-stream) while a tiny
/// synthetic probe still passes — "can generate" is not "can serve the actual working
/// set". Sustained real-turn failure is the honest wedge evidence, symmetric with real
/// tokens being the honest liveness evidence above.
pub(super) static REAL_DECODE_FAILS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);


/// Record that a real generation FAILED terminally on the served lane (stream read
/// error, mid-stream silence, transport death). Called from the adapter's error paths —
/// gated by the caller on the request actually targeting the LOCAL serving lane, so a
/// cloud provider's 529 can never smear the local lane's record.
pub fn note_real_decode_failure() {
    REAL_DECODE_FAILS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}


/// Consecutive real-generation failures since the last success. The serving daemon's
/// heartbeat treats `>= HEALTH_FAILS_TO_RELAUNCH` as wedge evidence that OUTRANKS both
/// the recent-decode trust window and a passing smoke probe.
pub fn consecutive_real_decode_failures() -> u64 {
    REAL_DECODE_FAILS.load(std::sync::atomic::Ordering::Relaxed)
}


/// Reset the failure streak — called when the lane is declared wedged (so the freshly
/// relaunched lane starts clean instead of being instantly re-condemned by its
/// predecessor's record) and implicitly by every successful decode.
pub fn reset_real_decode_failures() {
    REAL_DECODE_FAILS.store(0, std::sync::atomic::Ordering::Relaxed);
}


/// What a FINISHED real generation says about the served lane's health.
///
/// The classification is pure so it can be tested without touching the process-global
/// counters it drives ([[a-daemon-test-owns-its-inputs-process-globals-cross-tests]]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealDecodeOutcome {
    /// Tokens reached the citizen: proof of life, and it ends any failure streak.
    ProofOfLife,
    /// The generation finished CLEANLY and delivered nothing — no content, no reasoning,
    /// no tool call — and the engine said it STOPPED. This is the latched-backend class
    /// (a Metal compute context that has gone bad still answers 200 and streams a
    /// well-formed body with zero deltas), and it is the one wedge the liveness record
    /// could not previously see: the transport never failed, so nothing stamped a
    /// failure, so `REAL_DECODE_FAILS` stayed 0 while every citizen on the lane got an
    /// empty turn. A relaunch IS the cure, so this counts against the lane.
    DeliveredNothing,
    /// Empty, but the engine said the OUTPUT CAP ended it. The lane is healthy — it
    /// generated until the allowance ran out and the whole budget went to a reasoning
    /// channel that never committed an answer. Measured by Cormac on the IntelMac
    /// 2026-09-22: 2 of 42 settle ticks, `finish_reason` Length, on a lane with 27
    /// successful generations in the same window. Relaunching it would bounce a working
    /// backend and fix nothing; the owner is the output allowance
    /// ([[an-output-allowance-for-a-thinking-model-must-hold-its-reasoning-channel]]).
    /// Surfaced, never charged to the lane.
    CappedBeforeOutput,
    /// Not the local serving lane — a cloud provider's empty turn says nothing about our
    /// hardware and must never smear this record.
    NotOurLane,
}


/// Classify a finished real generation for the lane-health record.
///
/// `delivered_something` is the OR of content, reasoning and tool calls: a native tool
/// turn legitimately carries empty text, so text alone is not the test.
///
/// The FINISH REASON separates two empties that read identically in a receipt and have
/// opposite cures (Cormac's review of #4344, measured across two nodes the same hour):
/// an empty that STOPPED is a backend that produced nothing and wants a relaunch; an
/// empty that hit LENGTH is a healthy backend whose output cap ended it, and relaunching
/// it would bounce a working lane and change nothing. Only the first is the lane's fault.
pub fn classify_real_decode(
    local_lane: bool,
    delivered_something: bool,
    finish_reason: crate::ai::types::FinishReason,
) -> RealDecodeOutcome {
    use crate::ai::types::FinishReason;
    if !local_lane {
        return RealDecodeOutcome::NotOurLane;
    }
    if delivered_something {
        return RealDecodeOutcome::ProofOfLife;
    }
    match finish_reason {
        // The cap ended it: the lane generated, the allowance ran out. Healthy hardware.
        FinishReason::Length => RealDecodeOutcome::CappedBeforeOutput,
        // Stop / ToolUse-with-no-call / Error / an unreported end, all empty: nothing was produced.
        FinishReason::Stop | FinishReason::ToolUse | FinishReason::Error | FinishReason::Unreported => {
            RealDecodeOutcome::DeliveredNothing
        }
    }
}


/// Record that a real generation produced tokens on the served lane. Called from the adapter's
/// success path — the one place that knows tokens actually came out.
pub fn note_real_decode() {
    // A success ends the failure streak regardless of clock readability below.
    REAL_DECODE_FAILS.store(0, std::sync::atomic::Ordering::Relaxed);
    // A clock we cannot read must NOT be stored: 0 is this atomic's "never decoded"
    // sentinel, so `.unwrap_or(0)` here would erase a real decode rather than record
    // one. Leave the last good stamp in place and say so.
    let Ok(since_epoch) = continuum_base::time::EpochMs::read() else {
        tracing::warn!(target: "continuum_core::inference::llama_server", "system clock is before the UNIX epoch — cannot stamp this decode; \
             liveness will be judged from the previous stamp or by probing"
        );
        return;
    };
    LAST_REAL_DECODE_MS.store(
        since_epoch.millis(),
        std::sync::atomic::Ordering::Relaxed,
    );
}


/// How a NEVER-STARTED stream timeout should be read. The adapter's queue-budget
/// timeout fires for two very different worlds that look identical from inside one
/// request: the backend is dead (wedge), or the lane is busy serving OTHER work and
/// this request simply never got a slot (starvation). Its own error text has always
/// admitted the ambiguity ("either the backend is dead or the queue is oversubscribed").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NeverStartedClass {
    /// The lane delivered real tokens to SOMEONE within the window we waited —
    /// it is provably alive, just oversubscribed. Not wedge evidence: stamping
    /// it as such relaunches a healthy busy lane, killing the very generations
    /// that prove it healthy. Measured live 2026-08-15 (bench-hard-rs): 4 demand
    /// lanes on 1-2 slots with multi-minute thinking turns → queued turns timed
    /// out never-started → 2 stamps → `serving.health via=real_turn_failures` →
    /// relaunch every 2 minutes, forever — llama's own log showed 11k-token
    /// completions at 17.5 t/s between the kills. The round died to its own
    /// health monitor.
    Starved,
    /// No real decode landed for anyone across the whole window we waited (or
    /// none has ever been observed) — the timeout IS evidence the lane cannot
    /// serve, and it must count toward the relaunch threshold (#363: an
    /// undersized/dead lane rejecting the fleet's real prompts is exactly what
    /// the real-turn counter exists to catch).
    WedgeEvidence,
}


/// Classify a stream timeout (never started, or started and then stalled with the stream still
/// carrying keepalive bytes, card 2caa0de5) using the lane's OWN delivery record —
/// the same "health is recent delivery, never a synthetic probe" principle as
/// [`ms_since_real_decode`], applied to the failure side. Pure so the busy-lane
/// and dead-lane rows are table-testable without a server.
pub fn classify_never_started_timeout(
    ms_since_decode: Option<u64>,
    waited_ms: u64,
) -> NeverStartedClass {
    match ms_since_decode {
        // Tokens came out for someone while we were waiting: alive, oversubscribed.
        Some(age) if age < waited_ms => NeverStartedClass::Starved,
        // No delivery across our whole wait (or never / clock unreadable): the
        // timeout is genuine evidence about the lane, not about the queue.
        _ => NeverStartedClass::WedgeEvidence,
    }
}


/// How a missed decode SMOKE PROBE should be read, judged by the server's own
/// `/slots` activity between two consecutive misses (L11) — the exact sibling of
/// [`classify_never_started_timeout`], one layer up: that one classifies a REQUEST
/// timeout by the lane's delivery record; this one classifies a PROBE miss by the
/// lane's slot-state record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmokeMissVerdict {
    /// `/slots` changed between the two misses: the serve loop is advancing (for
    /// whoever's work — including clients this core never knew about), so the probe
    /// missed because real work held the slots. Contention, never wedge evidence.
    AliveViaSlotProgress,
    /// No proof of progress: first miss (nothing to compare), fingerprint frozen
    /// (the wedge signature), or `/slots` unreadable (no exoneration on a fetch
    /// error). Counts toward the relaunch threshold exactly as before L11.
    CountMiss,
}


/// Judge a smoke-probe miss by comparing `/slots` fingerprints across misses. Pure so
/// the busy-lane / frozen-lane / first-miss / no-endpoint rows are table-testable.
/// The miss threshold a wedge declaration must reach, scaled by what a kill
/// would destroy (2026-08-23, the MirrorCode baseline's three kills): a lane
/// holding a LARGE in-flight prompt (≥ a quarter of its served window —
/// relative, never an absolute constant) earns DOUBLE the base threshold,
/// because relaunching it burns minutes of re-prefill and, when the client
/// retries the same giant turn, can cycle into a kill loop. The patience is
/// priced in the same currency as the false-kill cost. A lane with no large
/// in-flight investment keeps the base threshold — a truly quiet wedge still
/// dies fast.
pub fn wedge_miss_threshold(
    base: u8,
    inflight_prompt_tokens: Option<u64>,
    served_window: u32,
) -> u8 {
    match inflight_prompt_tokens {
        Some(t) if served_window > 0 && t >= served_window as u64 / 4 => base.saturating_mul(2),
        _ => base,
    }
}


pub fn judge_smoke_miss(prev_fp: Option<u64>, cur_fp: Option<u64>) -> SmokeMissVerdict {
    match (prev_fp, cur_fp) {
        (Some(prev), Some(cur)) if prev != cur => SmokeMissVerdict::AliveViaSlotProgress,
        _ => SmokeMissVerdict::CountMiss,
    }
}


/// Fingerprint of the server's own `/slots` activity state — the lane-level work
/// evidence that survives every blind spot the adapter-side stamps have (L11).
///
/// The adapter stamps ([`note_real_decode`] / [`note_real_prefill_progress`]) only see
/// work done for OUR open streams. Two measured cases where that goes blind while the
/// lane works flat out:
///   1. GHOST WORK after a core restart: the predecessor's in-flight turns keep
///      decoding inside llama-server for clients that no longer exist — no stream on
///      this side, no stamps (2026-08-16 boot: 3 wedge declarations + kill-loop,
///      10:13→10:36, on a lane llama's own log showed grinding the whole time).
///   2. A fresh core's atomics start at 0 (`None`), so the L9 "trust busy lanes" gate
///      falls through until the first adapter stream observes progress.
/// `/slots` is the server's OWN account: per slot `id_task` (task turnover),
/// `is_processing`, `n_prompt_tokens_processed` (prefill advance), and the decode
/// counter when the build exposes it. If ANY of that changed between two looks, the
/// serve loop is provably advancing — for whoever's work — and a missed smoke probe
/// is queue contention, not death. A truly wedged backend is FROZEN: same fingerprint
/// every look (the 2026-08-05 4-hour wedge held one task at one impossible progress).
///
/// FOR WHOEVER EXTENDS THIS: hash only fields that change WITH WORK. Never hash
/// timing/params blobs — they'd churn the fingerprint on every poll and exonerate a
/// dead lane forever. `None` (no array) means "cannot say", which the caller must
/// treat as no exoneration, never as evidence of progress.
/// The largest in-flight prompt investment on the lane, in tokens — how much
/// re-prefill a lane kill would DESTROY right now. Read from the same `/slots`
/// control-plane JSON as the activity fingerprint. `None` = no in-flight task
/// (or unparseable), which the caller must read as "nothing to protect", never
/// as an error.
pub fn slots_max_inflight_prompt_tokens_of(slots: &serde_json::Value) -> Option<u64> {
    slots
        .as_array()?
        .iter()
        .filter(|s| s.get("is_processing").and_then(|v| v.as_bool()) == Some(true))
        .filter_map(|s| s.get("n_prompt_tokens_processed").and_then(|v| v.as_u64()))
        .max()
}


pub fn slots_activity_fingerprint_of(slots: &serde_json::Value) -> Option<u64> {
    use std::hash::{Hash, Hasher};
    let arr = slots.as_array()?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    arr.len().hash(&mut h);
    for slot in arr {
        slot.get("id").and_then(|v| v.as_u64()).hash(&mut h);
        slot.get("id_task").and_then(|v| v.as_i64()).hash(&mut h);
        slot.get("is_processing").and_then(|v| v.as_bool()).hash(&mut h);
        slot.get("n_prompt_tokens_processed")
            .and_then(|v| v.as_u64())
            .hash(&mut h);
        // DECODE ADVANCE — the field that was silently never read (2026-08-24 kill):
        // current llama-server builds wrap the per-slot decode state in an ARRAY
        // (`next_token: [{n_decoded: ...}]`); a string-key get on an array is always
        // None, so during a long DECODE every hashed field above is static (prefill
        // done → processed frozen; one task → id_task frozen) and a lane grinding at
        // 31 tok/s fingerprinted as WEDGED. Two smoke misses later the healer killed a
        // 35k-token in-flight turn — the 1-lane kill-loop. Read n_decoded through both
        // shapes: object (older builds) or first element of the array (current).
        let next_token = slot.get("next_token");
        next_token
            .and_then(|nt| {
                nt.get("n_decoded")
                    .or_else(|| nt.get(0).and_then(|first| first.get("n_decoded")))
            })
            .and_then(|v| v.as_u64())
            .hash(&mut h);
        // Belt for the build where next_token vanishes entirely: this llama build also
        // grows `n_prompt_tokens` monotonically as decode extends the context (measured
        // live: 47,211 → 47,704 over 16s of pure decode). Work-driven, monotone — NOT a
        // timing blob (the doc's churn hazard), so hashing it can only exonerate a lane
        // that is provably extending a sequence.
        slot.get("n_prompt_tokens").and_then(|v| v.as_u64()).hash(&mut h);
    }
    Some(h.finish())
}


/// Wall-clock ms of the last real PREFILL advance on the served lane. The decode stamp
/// above is blind to a lane that is 100% mid-prefill: eight 17k prompts saturate the
/// slots for minutes producing ZERO output tokens, the decode stamp goes stale, the
/// health heartbeat's "trust busy lanes" gate falls through, its probe can't get a
/// slot, and two misses relaunch a lane working flat out (round-killer L9, live
/// 2026-08-15 13:32 — 30s after dispatch, reason "sustained decode failure"). The #385
/// per-request detector already learned PREFILL IS PROGRESS; this is the same lesson at
/// the lane level. Stamped from the adapter's prefill-progress site, same local-lane
/// gating as the decode stamp.
pub(super) static LAST_REAL_PREFILL_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);


/// Record that a real generation's PREFILL advanced on the served lane.
pub fn note_real_prefill_progress() {
    let Ok(since_epoch) = continuum_base::time::EpochMs::read() else {
        // Same clock discipline as note_real_decode: 0 is the "never" sentinel,
        // an unreadable clock must not erase a real stamp.
        return;
    };
    LAST_REAL_PREFILL_MS.store(
        since_epoch.millis(),
        std::sync::atomic::Ordering::Relaxed,
    );
}


/// Milliseconds since the lane last did REAL WORK for anyone — a decode that produced
/// tokens OR a prefill that advanced, whichever is fresher. This is the liveness
/// question the health heartbeat and the never-started-timeout classifier actually ask;
/// [`ms_since_real_decode`] remains for callers that specifically need tokens-out.
pub fn ms_since_real_work() -> Option<u64> {
    let decode = ms_since_real_decode();
    let prefill = ms_since_stamp(&LAST_REAL_PREFILL_MS);
    match (decode, prefill) {
        (Some(d), Some(p)) => Some(d.min(p)),
        (one, other) => one.or(other),
    }
}


pub(super) fn ms_since_stamp(stamp: &std::sync::atomic::AtomicU64) -> Option<u64> {
    let last = stamp.load(std::sync::atomic::Ordering::Relaxed);
    if last == 0 {
        return None;
    }
    let now = continuum_base::time::EpochMs::read().ok()?.millis();
    Some(now.saturating_sub(last))
}


/// Milliseconds since the last real token-producing generation, or `None` if none has been
/// observed yet (fresh boot) — the caller must then fall back to probing.
pub fn ms_since_real_decode() -> Option<u64> {
    let last = LAST_REAL_DECODE_MS.load(std::sync::atomic::Ordering::Relaxed);
    if last == 0 {
        return None;
    }
    // `.unwrap_or(0)` here was the dangerous half: `now` = 0 makes the saturating_sub
    // yield 0, i.e. "a token came out 0 ms ago" — an unreadable clock would present a
    // WEDGED lane as perfectly fresh, and the caller's documented fallback (probe it)
    // would never run. `None` already means "no usable evidence, go probe".
    let now = continuum_base::time::EpochMs::read().ok()?.millis();
    Some(now.saturating_sub(last))
}
