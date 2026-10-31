//! What a mind's turn actually COSTS — measured, so the serving window can be
//! provisioned for it instead of guessed at.
//!
//! # Why this exists
//!
//! `serving_plan` sizes the served window as `window_for(lanes).min(DEMAND).max(FLOOR)`.
//! `window_for` computes what genuinely fits on this host (94k on a roomy one) and the
//! model's own trained ceiling bounds it (128k for Devstral-Small-2507, and far more for
//! the MoEs this substrate exists to serve). The DEMAND term is what decides how much of
//! that a citizen actually gets — and until this module it was a constant:
//! `BOOTSTRAP_WORKING_SET = MIN_SERVE_CTX * 8 = 16384`, split across lanes, which is why
//! two resident personas each thought in **8192 tokens** on a machine that could serve
//! them 94k of a 128k-capable model.
//!
//! That constant was never meant to survive. Its own doc said so:
//! *"the conservative PRIOR … used until live per-persona working-set telemetry (p95
//! observed + gen headroom) refines it UP toward measured demand (task #234)"*. The
//! telemetry is this module; the prior is now superseded the moment there is one
//! observation.
//!
//! # DEMAND, not USAGE — the trap this module is built to avoid
//!
//! The obvious implementation measures the prompt we actually sent and takes its p95.
//! That measures **the clamp**, not the mind: a citizen held at 8192 fills ~8192, so a
//! p95 of what-was-sent re-derives the cap that produced it and freezes it forever. It
//! is a thermometer inside the thermostat.
//!
//! So what is recorded here is what the turn WOULD have used with no budget at all:
//! framing + the FULL conversation before newest-first trimming + EVERY grounding
//! contribution offered (including the ones assembly had to drop) + the generation
//! reserve. That number is free to exceed the current window — which is exactly the
//! signal that the window is too small, and the only signal that can ever grow it.
//!
//! Measured 2026-08-06, this is not hypothetical: the work board alone offered a median
//! 5,364 tokens into a context budget with a median of 55, and was dropped 495 times out
//! of 495. Under a usage-based metric that board is invisible demand forever.
//!
//! # Peak, not average
//!
//! A working set is the high-water mark of the activity, because that is the size at
//! which the activity stops being strangled. Averaging a coding turn with idle chatter
//! produces a window that serves neither. The peak is safe to provision against because
//! it is bounded twice downstream and never applied directly: `serving_plan` takes
//! `min(what the host fits, this demand)` and floors it at `MIN_SERVE_CTX`, so a single
//! enormous turn can ask for more than the machine has and simply receive what fits.
//!
//! # Ownership
//!
//! One registry per core, held by the caller and passed in — NOT read from a process
//! global inside a decision. A global read inside a decision is what makes tests
//! order-dependent ([[a-process-global-read-inside-a-decision-makes-tests-order-dependent]]),
//! and this value feeds a decision (`plan_serving`) whose whole purpose is to be
//! testable against synthetic hosts.

use std::sync::Arc;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The process's registry handle — the ONE place a spawning mind and the serving
/// daemon on the same core meet.
///
/// This is a WIRING accessor, in the same spirit as
/// [`crate::cognition::persona_workspace::global`]: it hands out the shared handle at
/// construction time. It is deliberately never called from inside a decision —
/// `plan_serving` takes the measured ceiling as a parameter precisely so a synthetic
/// host can be planned against a synthetic demand, and so the test suite cannot become
/// order-dependent through a global read
/// ([[a-process-global-read-inside-a-decision-makes-tests-order-dependent]]).
pub fn global() -> WorkingSetRegistry {
    static GLOBAL: std::sync::OnceLock<WorkingSetRegistry> = std::sync::OnceLock::new();
    GLOBAL.get_or_init(WorkingSetRegistry::new).clone()
}

/// Where one mind's measured demand lives across restarts — beside the rest of
/// her durable state, because it IS her property and should travel with her.
/// Mirrors `persona_workspace::volatile_path`'s layout exactly.
fn personas_root() -> std::io::Result<std::path::PathBuf> {
    crate::paths::home_dir()
        .map(|home| home.join(".continuum/personas"))
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "cannot resolve persistent home directory",
            )
        })
}

fn demand_path(persona: Uuid) -> std::io::Result<std::path::PathBuf> {
    Ok(personas_root()?
        .join(persona.to_string())
        .join("working-set.json"))
}

fn emission_path(persona: Uuid) -> std::io::Result<std::path::PathBuf> {
    Ok(personas_root()?
        .join(persona.to_string())
        .join("emission.json"))
}

/// One mind's observed demand.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonaDemand {
    /// High-water mark, in tokens, of a full unclamped turn for this persona.
    pub peak_tokens: u32,
    /// The most recent observation's value — the peak's honest companion, so the
    /// glass box can show "peaked at 47k, currently running 9k" rather than only
    /// the extreme.
    pub last_tokens: u32,
    /// Wall clock of the most recent observation.
    pub last_seen_ms: u64,
    /// Wall clock of the turn BEFORE `last_seen_ms` (0 = fewer than two turns seen). The
    /// gap between them is how long she takes to come back, which is what the prompt
    /// cache must hold her state across (`serving_daemon::rotation_window_ms`, card
    /// 56d3c76f). Legacy files read 0: no return measured yet.
    #[serde(default)]
    pub prev_seen_ms: u64,
    /// How many turns have been observed. One observation is a measurement; the
    /// count is what lets a reader judge how much to trust the peak.
    pub turns: u64,
    /// Largest growth sample: post-fit size, or untrimmed demand when the window
    /// starved the turn (`sent_sample_for_plan`). The legacy name is retained for
    /// persisted compatibility; this is NOT proof of successful engine service.
    /// Legacy files without it read 0.
    #[serde(default)]
    pub sent_peak: u32,
    /// The last [`RECENT_TURNS`] untrimmed demands, newest overwriting oldest — what
    /// her turns TYPICALLY assemble, as opposed to `peak_tokens` (once, ever). The
    /// lane REQUIREMENT is the median of these with headroom (`typical_tokens`):
    /// Kimi's peak on the 5090 was 176k, which no lane on a 32 GB card can hold,
    /// while her turns run ~30–60k — sizing to the peak would shed her model for
    /// nothing (Joel, 2026-09-20: "fix it, this is the bug"). Legacy files read empty.
    #[serde(default)]
    pub recent: [u32; RECENT_TURNS],
    /// Next slot to overwrite in `recent`.
    #[serde(default)]
    pub recent_next: u8,
    /// The last [`RECENT_TURNS`] post-fit sizes of turns the fit HELD — her NEED, as
    /// opposed to `recent` (her WISH, the untrimmed assembly). A turn counts here only
    /// when `fit_messages` kept the source's truthful minimum and something survived
    /// the fit: dropping optional history is the fit working, not starvation. The
    /// allocator GATES on the median of this ring (no headroom — she was served at
    /// that size) and TARGETS the wish (card 70706a9e: Kimi's wish 99k × 1.25 read as
    /// "over the grid" at a 75,776 seat her turns were fitting into every time).
    /// Legacy files read empty → no need known → the wish stands in, as before.
    #[serde(default)]
    pub need_recent: [u32; RECENT_TURNS],
    /// Next slot to overwrite in `need_recent`.
    #[serde(default)]
    pub need_next: u8,
    /// The last [`RECENT_TURNS`] growth samples (what `sent_peak` takes the max of, over a
    /// lifetime): the lane floor reads their max, so one historic giant turn stops sizing
    /// every plan forever. On the M5, 2026-10-28, all-time `sent_peak`s of 188-370k
    /// (recent turns 20-43k) set a 423,945-token requirement no plan could meet, every
    /// runnable plan was refused as below the floor, and four residents shared one
    /// 27,904-token slot. Legacy files read empty.
    #[serde(default)]
    pub sent_recent: [u32; RECENT_TURNS],
    /// Next slot to overwrite in `sent_recent`.
    #[serde(default)]
    pub sent_next: u8,
}

/// How many recent turns define "typical". Sixteen: a working session, not a
/// lifetime; a change of task shows within a dozen turns.
pub const RECENT_TURNS: usize = 16;

/// Push one untrimmed demand into her recent ring (newest overwrites oldest).
fn push_recent(d: &mut PersonaDemand, demand_tokens: u32) {
    let i = (d.recent_next as usize) % RECENT_TURNS;
    d.recent[i] = demand_tokens;
    d.recent_next = ((i + 1) % RECENT_TURNS) as u8;
}

/// Push one HELD turn's post-fit size into her need ring (newest overwrites oldest).
fn push_need(d: &mut PersonaDemand, need_tokens: u32) {
    let i = (d.need_next as usize) % RECENT_TURNS;
    d.need_recent[i] = need_tokens;
    d.need_next = ((i + 1) % RECENT_TURNS) as u8;
}

/// Push one growth sample into her sent ring (newest overwrites oldest).
fn push_sent(d: &mut PersonaDemand, sent_tokens: u32) {
    let i = (d.sent_next as usize) % RECENT_TURNS;
    d.sent_recent[i] = sent_tokens;
    d.sent_next = ((i + 1) % RECENT_TURNS) as u8;
}

/// Her growth sample for the lane floor: the largest of her RECENT growth samples, never a
/// lifetime peak. A mind that never sent a growth sample has none, so she sets no floor. A
/// record written before the ring existed holds only its lifetime peak: it stands on her
/// typical demand, the same rule [`requirement_tokens`] states, else on that peak.
pub fn growth_sample(d: &PersonaDemand) -> Option<u32> {
    if let Some(recent) = d.sent_recent.iter().copied().max().filter(|&t| t > 0) {
        return Some(recent);
    }
    if d.sent_peak == 0 {
        return None;
    }
    typical_tokens(d).or(Some(d.sent_peak))
}

fn median_of(ring: &[u32; RECENT_TURNS]) -> Option<u32> {
    let mut v: Vec<u32> = ring.iter().copied().filter(|t| *t > 0).collect();
    if v.is_empty() {
        return None;
    }
    v.sort_unstable();
    Some(v[v.len() / 2])
}

/// The median of her recent untrimmed demands — her TYPICAL turn. `None` until one
/// is recorded (a legacy file carries only the peak).
pub fn typical_tokens(d: &PersonaDemand) -> Option<u32> {
    median_of(&d.recent)
}

/// The median post-fit size of her recent HELD turns — what a lane provably held for
/// her. `None` until a held turn is recorded; the allocator then gates on the wish.
pub fn need_tokens(d: &PersonaDemand) -> Option<u32> {
    median_of(&d.need_recent)
}

/// The size a HELD turn contributes to her need ring — `None` when the turn was not
/// held, so the ring never learns a starved window's echo (the 5090's self-sealed
/// 2048, 2026-09-20). A turn is held when the fit kept the source's truthful minimum
/// (`fit_ok`) AND something survived it (`sent > framing`): a fit that dropped every
/// message and "succeeded" is the window measuring itself, not the turn. Pure.
pub fn need_sample(sent_tokens: u32, framing_tokens: u32, fit_ok: bool) -> Option<u32> {
    (fit_ok && sent_tokens > framing_tokens).then_some(sent_tokens)
}

/// What a lane must hold for her: the typical turn, else the last one — one honest
/// sample, never the peak.
pub fn requirement_tokens(d: &PersonaDemand) -> u32 {
    typical_tokens(d).unwrap_or(d.last_tokens) // unwrap_or: no ring yet (a legacy record) = her last measured turn, a measurement, never the peak or a constant
}

/// One mind's observed REPLY size — the output-side twin of [`PersonaDemand`].
///
/// Demand measures what a turn's PROMPT wanted; emission measures what its
/// generation actually PRODUCED (the server's own `usage.output_tokens`). The
/// completion reserve was a bare `window/2` ratio — at a 29k window that
/// reserved 14,720 tokens for replies measuring 0.2–2.5k, squeezing grounding
/// to 195 tokens and dropping the room board for want of 137 (measured
/// 2026-08-31, the meta-loop spiral). Both reserve comments named this exact
/// registry pattern as the honest endgame; this is it.
/// How many recent turns the reasoning / answer NEED is read from — a p90 over this
/// many is the mind's measured demand per channel; a peak alone is one loud turn.
const NEED_SAMPLES: usize = 16;
/// Turns observed before the need is trusted to size an allowance — the same bar the
/// reserve asks (`turns >= 3`) before one reply sizes every turn after it.
const MIN_NEED_TURNS: u32 = 3;
/// Headroom over the p90 need (5/4): the p90 is what she USUALLY needs; the allowance
/// must hold the turn that runs a little longer. An integer ratio, like the decay.
const NEED_HEADROOM_NUM: u32 = 5;
const NEED_HEADROOM_DEN: u32 = 4;

/// What ONE turn of this mind needs to generate, per channel: her measured reasoning
/// (the `<think>` / `reasoning_content` channel) and her measured answer — each the p90
/// of recent turns with headroom. Measured on the M5 (2026-09-20, 20:58–21:14Z) because
/// #4194 sized the allowance in TIME alone and two turns ended inside the reasoning
/// channel (`persona.act.think_only reasoning_len=2730`, `2112`): no answer, no act. An
/// allowance that does not hold the think never reaches the answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputNeed {
    /// Tokens of reasoning a turn needs (p90 × headroom).
    pub reasoning: u32,
    /// Tokens of answer a turn needs (p90 × headroom).
    pub answer: u32,
    /// Turns the need was read from — how much to trust it.
    pub turns: u32,
}

impl OutputNeed {
    /// The whole turn: think, then answer.
    pub fn total(self) -> u32 {
        self.reasoning.saturating_add(self.answer)
    }
}

/// The p90 of `samples` (ascending sort, the value at the 90th percentile position)
/// with the need headroom applied. Empty = 0.
fn p90_with_headroom(samples: &[u32]) -> u32 {
    if samples.is_empty() {
        return 0;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let n = sorted.len();
    let idx = (n * 9).div_ceil(10).saturating_sub(1);
    sorted[idx.min(n - 1)]
        .saturating_mul(NEED_HEADROOM_NUM)
        .div_ceil(NEED_HEADROOM_DEN)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonaEmission {
    /// High-water mark, in tokens, of a completed generation. A turn that hit
    /// its output cap records DOUBLE its emission — the observation is a floor
    /// on true demand, not a measurement of it, and doubling is the growth
    /// path that keeps a measured reserve from freezing itself too small
    /// (the same measure-the-clamp trap `demand_tokens` documents).
    pub peak_tokens: u32,
    /// The most recent observation — the peak's honest companion.
    pub last_tokens: u32,
    /// Wall clock of the most recent observation.
    pub last_seen_ms: u64,
    /// Observation count — what lets a reader judge how much to trust the peak.
    pub turns: u64,
    /// The last [`NEED_SAMPLES`] turns' reasoning tokens, a ring (`need_turns` is the
    /// cursor). A file from before this field loads zeros with `need_turns` 0 — no
    /// need measured, never a zero need.
    #[serde(default)]
    pub reasoning_samples: [u32; NEED_SAMPLES],
    /// The last [`NEED_SAMPLES`] turns' answer tokens, the same ring.
    #[serde(default)]
    pub answer_samples: [u32; NEED_SAMPLES],
    /// Turns recorded into the rings (saturating). `< MIN_NEED_TURNS` = no need yet.
    #[serde(default)]
    pub need_turns: u32,
}

/// How a generation ENDED, as the emission record has to know it. The registry sizes
/// the next allowance from what it records here, so the record must say not only that
/// a generation was cut at its cap but WHICH CHANNEL the cap fell in.
///
/// Why (BigMama, Kimi, 2026-09-26): every `FinishReason::Length` used to record at DOUBLE
/// — "clamped by the reserve, so the true demand is above the count". Kimi's turns
/// refuted that reading: 41% of her requests failed, and the failed ones were exactly
/// the large ones (`maxTokens` ≥ 9,848 against a completed p50 of 6,448). A pass cut
/// inside its think doubled the need, the next allowance grew to hold it, the longer
/// generation ran past the turn deadline and failed outright — and a failure records
/// nothing, so only the cuts were ever measured. A one-way ratchet; her live
/// `emission.json` held think-only cuts at 14,272 and 18,154 and a peak of 18,154.
///
/// The first revision halved such a cut. Cormac's review of #4406 showed that is a
/// ratchet the other way once the loop closes — a smaller need cuts her earlier, which
/// halves again, C → C/2 → C/4 down to the floor — and that "committed = a parsed tool
/// call" mislabels a `code/write` cut mid-payload, the very cut that needed more room.
/// So the classification is by CHANNEL, and no factor below one exists anywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmissionStop {
    /// The model stopped on its own (stop token or tool call): both channels record
    /// verbatim — this is her real size for that turn.
    Landed,
    /// Cut at the cap WITH answer tokens: she was saying or calling something and the
    /// cap fell in it. The answer channel records at double (the growth path), the
    /// think verbatim, and the peak is the SUM of the two — never `output × 2`, which
    /// is what walked a 7,000-token think with a three-token answer to 14,006.
    CutMidAnswer,
    /// Cut at the cap with NO answer tokens: the think consumed the whole allowance and
    /// nothing was said. Not a sample of her need at all — a thinking model fills
    /// whatever room it is given, so a censored think says only "at least this much
    /// think fits in this allowance"; pushed into the need ring it would carry the
    /// landed turns' headroom on top of a value that already IS the allowance, and the
    /// loop would drift up ×5/4 a turn to the share. The peak takes it verbatim (so the
    /// reserve holds), `turns` counts it, the need ring does not: the allowance stays
    /// sized by the turns in which she said something, which is the branch's name.
    /// The cut itself is the fault paths' business (`persona.act.think_only`,
    /// `delib.truncated_not_an_answer`), not the measurement's.
    CutMidThought,
    /// The turn LANDED, but its think stopped AT the reasoning budget the request set
    /// (three quarters of the allowance, `deliberation_reasoning_budget`). The reasoning
    /// sample is censored: a thinking model fills whatever room it is given, so the
    /// count says only "at least the budget", not her need. Pushed verbatim it would
    /// teach the need ring the budget, and with the 5/4 headroom the next allowance is
    /// 5/4 × (3/4 A + answer) = 15/16 A + 5/4 answer — a geometric contraction toward
    /// max(time floor, 20 × answer), the forbidden clamp arrived at by feedback (Cormac
    /// on #4409). Recording it AT the allowance instead compounds the other way: a
    /// thinking model fills every budget, so "at least what she was given" ×5/4 each
    /// turn drives every deliberating mind to the full reserve (simulated: 8k → 184k in
    /// 20 turns), and on a slow lane that is the latency law broken. So a budget-hit is
    /// NOT a need sample, exactly as a think-only cut is not: the need ring keeps her
    /// landed shape, the peak takes the turn verbatim, and growth comes only from turns
    /// she ends on her own or from a cut inside the answer.
    ThinkBudgetHit { allowance: u32 },
}

impl EmissionStop {
    /// Classify a finished generation by where the cap fell. `hit_cap` is a
    /// `FinishReason::Length` stop; `answered` is whether the response CARRIES an answer
    /// (text or a call), never a token count, which a stream may not report: a cut with an
    /// answer present and usage missing is a cut in the answer (Codex on #5616). Only a
    /// cap hit is a cut. This is the one classification: the allowance policy, its probes
    /// and the hour line's counter ([`Self::counted`]) all read it.
    pub fn classify(hit_cap: bool, answered: bool) -> Self {
        match (hit_cap, answered) {
            (false, _) => Self::Landed,
            (true, false) => Self::CutMidThought,
            (true, true) => Self::CutMidAnswer,
        }
    }

    /// [`Self::classify`] with the request's reasoning budget in hand: a landed turn
    /// whose think reached the budget is a `ThinkBudgetHit`, the rest classify as before.
    /// The reasoning count is a measurement: missing usage reads 0 and so never claims a hit.
    pub fn classify_with_budget(
        hit_cap: bool,
        answered: bool,
        reasoning_tokens: u32,
        reasoning_budget: Option<u32>,
        allowance: u32,
    ) -> Self {
        match reasoning_budget {
            Some(budget) if !hit_cap && budget > 0 && reasoning_tokens >= budget => {
                Self::ThinkBudgetHit { allowance }
            }
            _ => Self::classify(hit_cap, answered),
        }
    }

    /// The hour line's count of this stop (`health_ledger::Stop`). A landed turn splits by
    /// what it carries: an answer is landed, reasoning alone ended in thought, nothing is empty.
    /// `answered` is the same presence [`Self::classify`] was given.
    pub fn counted(self, answered: bool, reasoned: bool) -> continuum_vitals::health_ledger::Stop {
        use continuum_vitals::health_ledger::Stop;
        match self {
            Self::CutMidAnswer => Stop::CutAnswer,
            Self::CutMidThought => Stop::CutThink,
            Self::ThinkBudgetHit { .. } => Stop::ThinkBudget,
            Self::Landed if answered => Stop::Landed,
            Self::Landed if reasoned => Stop::ThinkEnded,
            Self::Landed => Stop::Empty,
        }
    }
}

/// Per-persona observed turn demand for ONE core.
///
/// Cheap to clone (`Arc` inside) so the deliberation faculty, the serving daemon,
/// and a status command can all hold the same registry without threading a lock
/// through their signatures.
#[derive(Debug, Clone, Default)]
pub struct WorkingSetRegistry {
    observed: Arc<DashMap<Uuid, PersonaDemand>>,
    emitted: Arc<DashMap<Uuid, PersonaEmission>>,
}

impl WorkingSetRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one turn's UNCLAMPED demand for `persona`.
    ///
    /// Called from the seam that assembles the prompt and therefore knows every
    /// component's true size — including the parts it then had to drop. A zero
    /// demand is not recorded: it means the assembly produced nothing, which is a
    /// defect to be seen elsewhere, not a data point that would drag a peak down.
    pub fn record(&self, persona: Uuid, demand_tokens: u32, now_ms: u64) {
        if demand_tokens == 0 {
            return;
        }
        let updated = self.record_in_memory(persona, demand_tokens, now_ms);
        // Persist EVERY observation. A restart must not re-strangle her: the
        // registry is in-memory, so before this the reboot erased the measurement
        // and the planner fell back to the cold-start constant until enough turns
        // re-measured — observed live 2026-08-06, where a reboot dropped the served
        // window from a measured 24,126 back to 16,384. Joel's standard for a
        // restart is a PAUSE, not a death; a mind that has to re-earn its own
        // window every boot is not paused. One tiny JSON per turn, atomic
        // tmp+rename, best-effort — losing one interval is acceptable, blocking a
        // turn is not (the same contract as `save_volatile`).
        Self::save(persona, &updated);
    }

    /// The in-memory half, split out so persistence is a separate concern and the
    /// hot update stays testable without touching disk.
    /// The in-memory half of [`Self::record`], without the disk write. Crate-visible
    /// so a test can stand up a MEASURED demand (the thing that makes a plan exceed
    /// the cold-start prior at all) without touching the operator's home directory.
    pub fn record_in_memory(
        &self,
        persona: Uuid,
        demand_tokens: u32,
        now_ms: u64,
    ) -> PersonaDemand {
        *self
            .observed
            .entry(persona)
            .and_modify(|d| {
                d.peak_tokens = d.peak_tokens.max(demand_tokens);
                d.last_tokens = demand_tokens;
                // Only a previous TURN starts a return interval: a sent/need record can
                // create the entry seconds before her first demand (turns 0), and that
                // stamp is not a turn she came back from.
                if d.turns > 0 {
                    d.prev_seen_ms = d.last_seen_ms;
                }
                d.last_seen_ms = now_ms;
                d.turns += 1;
                push_recent(d, demand_tokens);
            })
            .or_insert_with(|| {
                let mut d = PersonaDemand {
                    peak_tokens: demand_tokens,
                    last_tokens: demand_tokens,
                    last_seen_ms: now_ms,
                    turns: 1,
                    ..PersonaDemand::default()
                };
                push_recent(&mut d, demand_tokens);
                d
            })
    }

    /// The prompt size a turn contributes to the seat's typical-prompt pool: what it
    /// sent — UNLESS the served window trimmed it, in which case what it NEEDED.
    ///
    /// A post-fit "sent" is bounded by the window by construction. Fed back as the
    /// window's floor it can only ever confirm the window: on the 5090 (2026-09-20,
    /// build e1c9e41a2) a boot at 2 × 2048 fitted every 56k-token turn down to ~828,
    /// the median read 828, the plan floored from it at 2048, the store remembered
    /// 2048 as steady, and 2,932 prompts (p50 56k, p90 136k) were refused while the
    /// seat reported its typical prompt as 828 — the window measuring itself. A turn
    /// the window starved is not a measurement of the turn; its untrimmed demand is.
    pub fn sent_sample_for_plan(demand_tokens: u32, sent_tokens: u32, served_window: u32) -> u32 {
        if served_window > 0 && demand_tokens > served_window {
            demand_tokens
        } else {
            sent_tokens
        }
    }

    /// Record the starvation-aware growth sample. Persisted with demand, never
    /// used as evidence that the engine successfully served that many tokens.
    pub fn record_sent(&self, persona: Uuid, sent_tokens: u32, now_ms: u64) {
        if sent_tokens == 0 {
            return;
        }
        let updated = self.record_sent_in_memory(persona, sent_tokens, now_ms);
        Self::save(persona, &updated);
    }

    /// The in-memory half of [`Self::record_sent`], without the disk write — the
    /// sibling of [`Self::record_in_memory`], so a test stands up a sent sample
    /// without writing into the operator's `~/.continuum/personas`.
    pub fn record_sent_in_memory(&self, persona: Uuid, sent_tokens: u32, now_ms: u64) -> PersonaDemand {
        *self
            .observed
            .entry(persona)
            .and_modify(|d| {
                d.sent_peak = d.sent_peak.max(sent_tokens);
                push_sent(d, sent_tokens);
            })
            .or_insert_with(|| {
                let mut d = PersonaDemand { last_seen_ms: now_ms, sent_peak: sent_tokens, ..PersonaDemand::default() };
                push_sent(&mut d, sent_tokens);
                d
            })
    }

    /// Record the post-fit size of a turn the fit HELD — her need (see
    /// [`need_sample`] for what counts). Persisted with the demand.
    pub fn record_need(&self, persona: Uuid, need_tokens: u32, now_ms: u64) {
        if need_tokens == 0 {
            return;
        }
        let updated = self.record_need_in_memory(persona, need_tokens, now_ms);
        Self::save(persona, &updated);
    }

    pub(crate) fn record_need_in_memory(&self, persona: Uuid, need_tokens: u32, now_ms: u64) -> PersonaDemand {
        *self
            .observed
            .entry(persona)
            .and_modify(|d| push_need(d, need_tokens))
            .or_insert_with(|| {
                let mut d = PersonaDemand { last_seen_ms: now_ms, ..PersonaDemand::default() };
                push_need(&mut d, need_tokens);
                d
            })
    }

    /// The largest untrimmed demand among `personas` (the residents), not the whole
    /// registry — 490 persisted entries include every test fixture and departed mind.
    pub fn ceiling_of(&self, personas: &[Uuid]) -> Option<u32> {
        personas
            .iter()
            .filter_map(|p| self.observed.get(p).map(|e| e.value().peak_tokens))
            .max()
            .filter(|&t| t > 0)
    }

    /// Typical growth sample among `personas` — the median of their RECENT starvation-aware
    /// peaks ([`growth_sample`], never a lifetime max). The planner's per-lane floor follows this, not the ceiling: one mind
    /// with a 96k prompt must not size every lane to 120k and starve the roster
    /// (2026-09-14: 16 residents on 3 lanes). The outlier is reconciled down to the
    /// served window instead.
    pub fn sent_median_of(&self, personas: &[Uuid]) -> Option<u32> {
        self.sent_median_with(personas, &[])
    }
    /// Growth demand over the residents' peaks PLUS `extra` requested samples —
    /// including refused prompts from minds hosted on other nodes
    /// ([`crate::inference::resource_admission::leased_in_sent_samples`]). ONE median for
    /// the seat's whole demand pool: a seat that plans lanes for leased-in minds must size
    /// those lanes to the prompts they actually send, not to one local resident's. Zeros
    /// are not samples on either side.
    pub fn sent_median_with(&self, personas: &[Uuid], extra: &[u32]) -> Option<u32> {
        let mut peaks: Vec<u32> = personas
            .iter()
            .filter_map(|p| self.observed.get(p).and_then(|e| growth_sample(e.value())))
            .chain(extra.iter().copied())
            .filter(|&t| t > 0)
            .collect();
        if peaks.is_empty() {
            return None;
        }
        peaks.sort_unstable();
        Some(peaks[peaks.len() / 2])
    }
    /// Largest recent growth sample among `personas`, including unmet demand ([`growth_sample`]).
    pub fn sent_ceiling_of(&self, personas: &[Uuid]) -> Option<u32> {
        personas
            .iter()
            .filter_map(|p| self.observed.get(p).and_then(|e| growth_sample(e.value())))
            .max()
    }

    /// Record one completed generation's measured output for `persona`.
    ///
    /// `output_tokens` is the SERVER's count (`usage.output_tokens`) — reasoning
    /// tokens included, never an estimate; `reasoning_tokens` is the share of it the
    /// reasoning channel took (the faculty apportions the server's count by channel
    /// bytes), so the answer is the remainder. `stop` says where it ended
    /// ([`EmissionStop`]): a landed generation records verbatim; a cut inside the
    /// answer records the answer at double (a floor on true demand, and the growth
    /// path — see [`PersonaEmission::peak_tokens`]) and the peak as the sum of the
    /// channels; a cut inside the think records the peak verbatim and is not a need
    /// sample. Zero-token completions
    /// are the empty-completion fault's territory, not a data point to drag the peak with.
    /// Returns this exact measurement even when persistence fails, so concern-owned
    /// demand can retain its evidence without racing a later registry observation.
    pub(crate) fn record_emission(
        &self,
        persona: Uuid,
        output_tokens: u32,
        reasoning_tokens: u32,
        stop: EmissionStop,
        now_ms: u64,
    ) -> Option<PersonaEmission> {
        if output_tokens == 0 {
            return None;
        }
        let updated =
            self.record_emission_in_memory(persona, output_tokens, reasoning_tokens, stop, now_ms);
        // Same persistence contract as demand: every observation, atomic, best-effort
        // — a restart is a pause, and a mind must not re-earn its reply size per boot.
        let path = match emission_path(persona) {
            Ok(path) => path,
            Err(error) => {
                tracing::warn!(persona_id = %persona, %error, "emission root unavailable — measurement remains in memory");
                return Some(updated);
            }
        };
        let write = || -> std::io::Result<()> {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let tmp = path.with_extension("json.tmp");
            std::fs::write(&tmp, serde_json::to_vec(&updated)?)?; // BOUNDARY: disk — the per-persona emission.json durable format
            std::fs::rename(&tmp, &path)
        };
        if let Err(e) = write() {
            tracing::warn!(
                persona_id = %persona, error = %e, path = %path.display(),
                "emission not persisted — this mind re-measures its reply size after the next restart"
            );
        }
        Some(updated)
    }

    /// How much of a past peak survives each new observation (7/8).
    ///
    /// The peak must be able to FALL, or a mid-answer cut's doubling is a one-way ratchet to the
    /// ceiling (see [`Self::record_emission_in_memory`]). 7/8 per observation is ~16 turns
    /// to forget a measurement that was never real — fast enough that a poisoned record
    /// heals within a work session, slow enough that a citizen who genuinely writes long
    /// replies keeps her room across the quiet turns between them.
    ///
    /// Expressed as a ratio on integers rather than a float: this value is persisted and
    /// compared, and a rounding difference between platforms would make two nodes disagree
    /// about the same citizen's reply size.
    const PEAK_DECAY_NUM: u32 = 7;
    const PEAK_DECAY_DEN: u32 = 8;

    /// The in-memory half of [`Self::record_emission`], without the disk write —
    /// same split (and same reason) as [`Self::record_in_memory`].
    pub fn record_emission_in_memory(
        &self,
        persona: Uuid,
        output_tokens: u32,
        reasoning_tokens: u32,
        stop: EmissionStop,
        now_ms: u64,
    ) -> PersonaEmission {
        let reasoning = reasoning_tokens.min(output_tokens);
        let answer = output_tokens - reasoning;
        // The cut channel takes the growth, and the peak is the SUM of the channels as
        // measured — never `output × 2`: a 7,000-token think cut three tokens into its
        // answer needs 7,006, not 14,006 (see [`EmissionStop`]).
        let (reasoning, answer) = match stop {
            EmissionStop::Landed | EmissionStop::CutMidThought => (reasoning, answer),
            EmissionStop::CutMidAnswer => (reasoning, answer.saturating_mul(2)),
            // A censored think is recorded verbatim on the peak side; the need ring skips it.
            EmissionStop::ThinkBudgetHit { .. } => (reasoning, answer),
        };
        let observed = reasoning.saturating_add(answer);
        let push_need = |e: &mut PersonaEmission| {
            // A think-only cut is not a sample of her need ([`EmissionStop::CutMidThought`]).
            if matches!(stop, EmissionStop::CutMidThought | EmissionStop::ThinkBudgetHit { .. }) {
                return;
            }
            let slot = (e.need_turns as usize) % NEED_SAMPLES;
            e.reasoning_samples[slot] = reasoning;
            e.answer_samples[slot] = answer;
            e.need_turns = e.need_turns.saturating_add(1);
        };
        *self
            .emitted
            .entry(persona)
            .and_modify(|e| {
                push_need(&mut *e);
                // A RECENT high-water mark, not an eternal one. `.max()` alone is a
                // one-way ratchet: a mid-answer cut records its answer at DOUBLE, so a single truncated
                // turn pins the peak at twice the cap forever, the reserve derived from
                // it saturates at the ceiling, and — because this file is persisted so a
                // mind "must not re-earn its reply size per boot" — the poisoned value
                // outlives every restart. Measured 2026-09-18 (Cormac): emission.json
                // holding peak_tokens 16384 for a citizen whose real replies are a few
                // hundred tokens, reserving half her window against a reply she would
                // never write until she had 297 tokens of context left and answered
                // "As an AI assistant" when asked who she was.
                //
                // Decaying by PEAK_DECAY_NUM/PEAK_DECAY_DEN each observation keeps both
                // properties that mattered: a genuine large reply still takes the peak
                // instantly (the growth path `CutMidAnswer` exists for), and a peak that was
                // never real fades on its own — so poisoned records SELF-HEAL over a few
                // dozen turns with no migration and no operator step.
                e.peak_tokens = observed.max(
                    e.peak_tokens
                        .saturating_mul(Self::PEAK_DECAY_NUM)
                        / Self::PEAK_DECAY_DEN,
                );
                e.last_tokens = observed;
                e.last_seen_ms = now_ms;
                e.turns += 1;
            })
            .or_insert_with(|| {
                let mut e = PersonaEmission {
                    peak_tokens: observed,
                    last_tokens: observed,
                    last_seen_ms: now_ms,
                    turns: 1,
                    reasoning_samples: [0; NEED_SAMPLES],
                    answer_samples: [0; NEED_SAMPLES],
                    need_turns: 0,
                };
                push_need(&mut e);
                e
            })
    }

    /// This persona's observed reply size, for the reserve derivation and the glass box.
    pub(crate) fn emission_of(&self, persona: Uuid) -> Option<PersonaEmission> {
        self.emitted.get(&persona).map(|e| *e.value())
    }

    /// What ONE turn of this mind needs to generate, per channel — `None` until
    /// [`MIN_NEED_TURNS`] turns have been measured (a legacy `emission.json` without the
    /// rings reads `None` too). Unknown is not zero: the caller falls back to the reserve.
    pub fn need_of(&self, persona: Uuid) -> Option<OutputNeed> {
        let e = self.emission_of(persona)?;
        if e.need_turns < MIN_NEED_TURNS {
            return None;
        }
        let filled = (e.need_turns as usize).min(NEED_SAMPLES);
        Some(OutputNeed {
            reasoning: p90_with_headroom(&e.reasoning_samples[..filled]),
            answer: p90_with_headroom(&e.answer_samples[..filled]),
            turns: e.need_turns.min(NEED_SAMPLES as u32),
        })
    }

    /// Atomic tmp+rename so a crash mid-write never leaves a torn file that would
    /// fail to parse and silently wake her at the cold-start window.
    fn save(persona: Uuid, demand: &PersonaDemand) {
        let path = match demand_path(persona) {
            Ok(path) => path,
            Err(error) => {
                tracing::warn!(persona_id = %persona, %error, "working-set root unavailable — measurement remains in memory");
                return;
            }
        };
        let write = || -> std::io::Result<()> {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let tmp = path.with_extension("json.tmp");
            std::fs::write(&tmp, serde_json::to_vec(demand)?)?;
            std::fs::rename(&tmp, &path)
        };
        if let Err(e) = write() {
            tracing::warn!(
                persona_id = %persona, error = %e, path = %path.display(),
                "working-set demand not persisted — this mind re-measures its window after the next restart"
            );
        }
    }

    /// Re-adopt a mind's measured demand at spawn, so she wakes at the window she
    /// earned rather than the cold-start floor. Unreadable/absent = no observation
    /// (honest), never an invented number.
    pub fn rehydrate(&self, persona: Uuid) {
        let path = match demand_path(persona) {
            Ok(path) => path,
            Err(error) => {
                tracing::warn!(persona_id = %persona, %error, "working-set root unavailable — prior demand could not be loaded");
                return;
            }
        };
        let Ok(bytes) = std::fs::read(&path) else {
            return;
        };
        match serde_json::from_slice::<PersonaDemand>(&bytes) {
            Ok(d) if d.peak_tokens > 0 => {
                self.observed.insert(persona, d);
                tracing::info!(
                    probe_class = "working_set.rehydrated",
                    persona_id = %persona,
                    peak_tokens = d.peak_tokens,
                    turns = d.turns,
                    "re-adopted this mind's measured window demand across the restart"
                );
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(
                persona_id = %persona, error = %e, path = %path.display(),
                "working-set file unreadable — this mind re-measures its window from scratch"
            ),
        }
        // The emission twin rides the same rehydration pass: absent/unreadable stays
        // silent (the reserve falls back to the cold-start share — honest, never invented).
        if let Ok(bytes) = emission_path(persona).and_then(std::fs::read) {
            match serde_json::from_slice::<PersonaEmission>(&bytes) {
                Ok(e) if e.peak_tokens > 0 => {
                    self.emitted.insert(persona, e);
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(
                    persona_id = %persona, error = %e,
                    "emission file unreadable — this mind re-measures its reply size from scratch"
                ),
            }
        }
    }

    /// Re-adopt EVERY mind's persisted demand at boot, before the planner can tick.
    ///
    /// # Why the per-persona `rehydrate` was not enough (measured 2026-08-20)
    ///
    /// `ceiling()` is a HOST question — "how much serving does the work on this box need"
    /// — but the only thing that populated it was `rehydrate`, called per-persona at spawn.
    /// So between boot and the first spawn (measured ~10 min, #412) the host had no demand
    /// at all, and with nothing resident it had none indefinitely: the plan fell to
    /// `BOOTSTRAP_WORKING_SET` and served 16,384 while 224 `working-set.json` files sat on
    /// disk — one of them recording a peak of 31,834 tokens over 18 turns. The 27B was
    /// serving a quarter of the window this host had already proven it needs.
    ///
    /// Loading all of them cannot over-commit the GPU, and that is worth stating because
    /// it is where the caution belongs and does NOT: demand is a REQUEST, not an
    /// allocation. `plan_serving_stable` clamps it against what the host can fit — that is
    /// precisely what a `bound_by=below-demand` plan is — under the power mode's fraction (the
    /// plan's only headroom since #4230; Performance keeps the thinnest, 4%) and the
    /// governor's own `budget_for_replacing`. Raising demand can only raise the ASK; the
    /// governor still decides. The failure mode of asking for too little is the one we
    /// measured; the failure mode of asking for too much is a plan that says `below-demand`.
    ///
    /// Unreadable or absent files stay silent — the same honesty `rehydrate` keeps. A
    /// ghost persona dir with no `working-set.json` contributes nothing rather than a zero.
    pub fn rehydrate_all(&self) -> usize {
        let root = match personas_root() {
            Ok(root) => root,
            Err(error) => {
                tracing::warn!(%error, "working-set root unavailable — persisted demand could not be enumerated");
                return 0;
            }
        };
        let Ok(entries) = std::fs::read_dir(root) else {
            return 0; // no personas root yet — a fresh install, not an error
        };
        let mut adopted = 0;
        for entry in entries.flatten() {
            let Some(persona) = entry
                .file_name()
                .to_str()
                .and_then(|s| Uuid::parse_str(s).ok())
            else {
                continue; // not a persona dir; never guess an id from a non-uuid name
            };
            let before = self.observed.contains_key(&persona);
            self.rehydrate(persona);
            if !before && self.observed.contains_key(&persona) {
                adopted += 1;
            }
        }
        tracing::info!(
            probe_class = "working_set.rehydrated_all",
            adopted,
            ceiling = ?self.ceiling(),
            "re-adopted this host's persisted window demand before the first plan"
        );
        adopted
    }

    /// The window this host's minds have actually demanded: the largest per-persona
    /// peak observed.
    ///
    /// `None` means **no turn has been measured yet** — an honest absence of data, and
    /// the caller must treat it as such rather than substituting a number here. (See
    /// `serving_plan`'s cold-start arm, which is the one place that decision belongs.)
    pub fn ceiling(&self) -> Option<u32> {
        self.observed
            .iter()
            .map(|e| e.value().peak_tokens)
            .max()
            .filter(|&t| t > 0)
    }

    /// This persona's observed demand, for the glass box and for a status command.
    pub fn demand_of(&self, persona: Uuid) -> Option<PersonaDemand> {
        self.observed.get(&persona).map(|e| *e.value())
    }

    /// Every observation, for reporting. Order is unspecified (a concurrent map).
    pub fn all(&self) -> Vec<(Uuid, PersonaDemand)> {
        self.observed
            .iter()
            .map(|e| (*e.key(), *e.value()))
            .collect()
    }

    /// How many minds have been measured.
    pub fn observed_personas(&self) -> usize {
        self.observed.len()
    }
}

/// Each mind's requirement, from the untrimmed demand her turns assemble with the
/// standard headroom, with her parked training examples' reservation beside it (never
/// folded into her target or floor) — the ONE derivation, shared by the serving daemon (sizing this
/// seat's lanes) and the grid allocator daemon (sizing the grid's roles). A mind with
/// no turn yet is `Unknown`, never a number.
pub fn requirements_for(
    personas: &[Uuid],
    working_set: &WorkingSetRegistry,
) -> Vec<crate::inference::planning::window_allocator::LaneRequirement> {
    personas
        .iter()
        .map(|p| {
            let demand = working_set.demand_of(*p);
            let wish = demand
                .as_ref()
                .map(requirement_tokens)
                .unwrap_or(0); // JUSTIFIED unwrap_or: a mind with no turn yet is Unknown to the allocator (0 = no measurement, never a number)
            let need = demand
                .as_ref()
                .and_then(need_tokens)
                .unwrap_or(0); // JUSTIFIED unwrap_or: no held turn measured = no floor beyond the target (0 = no measurement)
            // Her parked training examples' reservation, carried beside her conversational target
            // and need (card ea157b1a): the allocator grows toward it only from spare budget.
            let training = crate::genome::fine_tuning::training_demand::TrainingDemand::for_persona(*p).reservation();
            crate::inference::planning::window_allocator::LaneRequirement::from_demand(*p, wish, crate::inference::planning::serving_plan::SENT_HEADROOM)
                .with_need(need)
                .with_training(training)
        })
        .collect()
}

#[cfg(test)]
mod tests;
