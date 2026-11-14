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

use continuum_kernel::system_resources::disk_reporters::ClassWriter;
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

/// The disk class one mind's measured demand lives in across restarts: `personas`, beside the rest
/// of her durable state, because it IS her property and travels with her. Through the class, every
/// file is under a measured root with an eviction decision (`disk_reporters::standard_tracked_dirs`,
/// ONE-BUDGET row 5): no hand-built path.
const PERSONAS_CLASS: &str = "personas";
/// Her measured window demand, within the class.
const DEMAND_FILE: &str = "working-set.json";
/// Her measured reply size, within the class.
const EMISSION_FILE: &str = "emission.json";

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
    /// The last [`RECENT_TURNS`] windows her held turns' allocations CHOSE (the priced optimum,
    /// `continuum_context::allocation::need`) — her NEED, as opposed to `recent` (her WISH, the
    /// untrimmed assembly). A turn counts here only when `fit_messages` kept the source's
    /// truthful minimum and something survived the fit, and its act time was priced. The
    /// allocator GATES on the median of this ring and TARGETS the wish (card 70706a9e: Kimi's
    /// wish 99k × 1.25 read as "over the grid" at a 75,776 seat her turns were fitting into
    /// every time). Legacy files read empty → no need known → the wish stands in, as before.
    /// Each slot is typed exact or censored ([`NeedSample`]): an optimum past her grant is a
    /// lower bound, never a size.
    #[serde(default)]
    pub need_recent: [NeedSample; RECENT_TURNS],
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
    /// Her newest turn's IRREDUCIBLE window: framing, tools, her view's required floor and the
    /// turn's required payload, below which the turn cannot be sent at all. Recorded on every
    /// turn, held or refused, the newest replacing the last. The need ring learns only from held
    /// turns, so without this a turn refused for want of 123 tokens never raised her need and her
    /// grant never grew to fit it (Aiko on the M5, 2026-11-04: 26 refusals in 7 minutes at 17,780).
    /// `None` until a turn measured it.
    #[serde(default)]
    pub floor_tokens: Option<u32>,
    /// Her newest turn's demand when it exceeded the window she was served: what her view holds at
    /// full detail did not fit her grant, so her view collapsed. `None` when her newest turn fit.
    /// The upward probe: her requirement is at least it ([`requirement_tokens`]), so the next grant
    /// holds what overflowed instead of waiting for the median of a ring her collapses filled (Aris
    /// on the M5, 2026-11-13, 2b17c41df). Legacy files read `None`.
    #[serde(default)]
    pub overflow_tokens: Option<u32>,
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

mod held_need;
use held_need::push_need;
pub use held_need::{need_sample, need_tokens, resident_need, NeedSample};

/// What a lane must hold for her: the typical turn, else the last one — one honest
/// sample, never the peak — and at least her newest turn when it overflowed the window she was
/// served ([`PersonaDemand::overflow_tokens`]): a ring her collapses filled lags her by up to eight
/// turns, every one of them another collapse.
pub fn requirement_tokens(d: &PersonaDemand) -> u32 {
    let typical = typical_tokens(d).unwrap_or(d.last_tokens); // unwrap_or: no ring yet (a legacy record) = her last measured turn, a measurement, never the peak or a constant
    d.overflow_tokens.map_or(typical, |overflow| typical.max(overflow)) // map_or: her newest turn fit, so the typical turn stands
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
mod need;
pub use need::{ActRoom, ChannelSample, OutputNeed, NEED_SAMPLES};
mod emission;
pub use emission::{answer_payload_bytes, reasoning_answer_split, EmissionStop, ResponseEmission};
use need::{answer_need, p90, reasoning_at_least};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonaEmission {
    /// High-water mark, in tokens, of a completed generation: the sum of its channels as
    /// measured. A turn cut at its cap records what it wrote; that its true demand is higher is
    /// the answer ring's bound ([`Self::answer_samples`]), never a factor here.
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
    /// The last [`NEED_SAMPLES`] turns' answer tokens, the same ring: exact when she ended it,
    /// a bound when the cap cut it ([`EmissionStop::CutMidAnswer`]; an untagged slot reads exact).
    #[serde(default)]
    pub answer_samples: [ChannelSample; NEED_SAMPLES],
    /// Turns recorded into the rings (saturating). `0` = no need measured yet.
    #[serde(default)]
    pub need_turns: u32,
    /// The last [`NEED_SAMPLES`] thinks cut before they ended (`CutMidThought`,
    /// `ThinkBudgetHit`): each a lower bound on her reasoning need, never a measurement, so
    /// they stay out of the landed rings ([`OutputNeed::reasoning_at_least`]). A ring with
    /// `cut_count` as its cursor; a file written before cuts were kept has none, which is
    /// what it recorded.
    #[serde(default)]
    pub cut_samples: [u32; NEED_SAMPLES],
    /// The observation (`turns`) each cut was recorded at: a cut counts only while it is
    /// among her last [`NEED_SAMPLES`] turns, so an old cut ages out and the bound can fall.
    #[serde(default)]
    pub cut_at_turn: [u64; NEED_SAMPLES],
    /// Cuts recorded into the cut ring (saturating).
    #[serde(default)]
    pub cut_count: u32,
}

/// Per-persona observed turn demand for ONE core.
///
/// Cheap to clone (`Arc` inside) so the deliberation faculty, the serving daemon,
/// and a status command can all hold the same registry without threading a lock
/// through their signatures.
#[derive(Clone, Default)]
pub struct WorkingSetRegistry {
    observed: Arc<DashMap<Uuid, PersonaDemand>>,
    emitted: Arc<DashMap<Uuid, PersonaEmission>>,
    /// The class it persists through when one is given ([`Self::with_store`], a test's own root);
    /// otherwise `ClassWriter::of(PERSONAS_CLASS)`, resolved at each use, so a global built before
    /// boot resolves at its first write and a write before boot is a loud refusal, never a guess.
    store: Option<ClassWriter>,
}

impl std::fmt::Debug for WorkingSetRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkingSetRegistry")
            .field("observed", &self.observed.len())
            .field("emitted", &self.emitted.len())
            .field("store", &self.store.as_ref().map(ClassWriter::class))
            .finish()
    }
}

impl WorkingSetRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// A registry that persists through `class` (a test's own root; production resolves the class
    /// boot installed, at each use).
    pub fn with_store(class: ClassWriter) -> Self {
        Self { store: Some(class), ..Self::default() }
    }

    /// The class this registry persists through: the one it was given, else the registered one.
    fn class(&self) -> std::io::Result<ClassWriter> {
        match &self.store {
            Some(class) => Ok(class.clone()),
            None => ClassWriter::of(PERSONAS_CLASS),
        }
    }

    /// Write one mind's file whole through the class (staged, synced, then placed: a crash never
    /// leaves a torn file that would fail to parse and wake her at the cold-start window).
    fn persist(&self, persona: Uuid, name: &str, bytes: std::io::Result<Vec<u8>>, what: &str) {
        let written = self.class().and_then(|class| bytes.and_then(|bytes| class.owned_by(persona).replace(name.as_ref(), &bytes)));
        if let Err(error) = written {
            tracing::warn!(persona_id = %persona, %error, "{what} not persisted — this mind re-measures it after the next restart");
        }
    }

    /// One mind's file read back through the class; `None` when absent, unreadable, or the class
    /// is not under management.
    fn read(&self, persona: Uuid, name: &str) -> Option<Vec<u8>> {
        let path = self.class().ok()?.owned_by(persona).path(name.as_ref()).ok()?;
        std::fs::read(path).ok()
    }

    /// Record one turn's UNCLAMPED demand for `persona`.
    ///
    /// Called from the seam that assembles the prompt and therefore knows every
    /// component's true size — including the parts it then had to drop. A zero
    /// demand is not recorded: it means the assembly produced nothing, which is a
    /// defect to be seen elsewhere, not a data point that would drag a peak down.
    /// `served_window` is the window the turn was assembled in: a demand past it overflowed
    /// ([`PersonaDemand::overflow_tokens`]).
    pub fn record(&self, persona: Uuid, demand_tokens: u32, served_window: u32, now_ms: u64) {
        if demand_tokens == 0 {
            return;
        }
        self.record_in_memory(persona, demand_tokens, now_ms);
        let updated = self.record_overflow_in_memory(persona, demand_tokens, served_window);
        // Persist EVERY observation. A restart must not re-strangle her: the
        // registry is in-memory, so before this the reboot erased the measurement
        // and the planner fell back to the cold-start constant until enough turns
        // re-measured — observed live 2026-08-06, where a reboot dropped the served
        // window from a measured 24,126 back to 16,384. Joel's standard for a
        // restart is a PAUSE, not a death; a mind that has to re-earn its own
        // window every boot is not paused. One tiny JSON per turn, atomic
        // tmp+rename, best-effort — losing one interval is acceptable, blocking a
        // turn is not (the same contract as `save_volatile`).
        self.save(persona, &updated);
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

    /// The in-memory half of the overflow [`Self::record`] notes: her newest demand when it
    /// exceeded `served_window`, else none.
    pub fn record_overflow_in_memory(&self, persona: Uuid, demand_tokens: u32, served_window: u32) -> PersonaDemand {
        let mut entry = self.observed.entry(persona).or_default();
        entry.overflow_tokens = (demand_tokens > served_window).then_some(demand_tokens);
        *entry
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
        self.save(persona, &updated);
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

    /// Record one turn's need — exact for a turn the fit HELD (see [`need_sample`] for what
    /// counts), a lower bound for a turn whose view overflowed its grant ([`NeedSample`]).
    /// Persisted with the demand.
    pub fn record_need(&self, persona: Uuid, sample: NeedSample, now_ms: u64) {
        if sample.value() == 0 {
            return;
        }
        let updated = self.record_need_in_memory(persona, sample, now_ms);
        self.save(persona, &updated);
    }

    pub(crate) fn record_need_in_memory(&self, persona: Uuid, sample: NeedSample, now_ms: u64) -> PersonaDemand {
        *self
            .observed
            .entry(persona)
            .and_modify(|d| push_need(d, sample))
            .or_insert_with(|| {
                let mut d = PersonaDemand { last_seen_ms: now_ms, ..PersonaDemand::default() };
                push_need(&mut d, sample);
                d
            })
    }

    /// Record her newest turn's irreducible window (see [`PersonaDemand::floor_tokens`]).
    /// Persisted with the demand when it changed: the floor repeats turn after turn.
    pub fn record_floor(&self, persona: Uuid, floor_tokens: u32, now_ms: u64) {
        if floor_tokens == 0 {
            return;
        }
        let before = self.observed.get(&persona).and_then(|d| d.floor_tokens);
        let updated = self.record_floor_in_memory(persona, floor_tokens, now_ms);
        if before != Some(floor_tokens) {
            self.save(persona, &updated);
        }
    }

    pub(crate) fn record_floor_in_memory(&self, persona: Uuid, floor_tokens: u32, now_ms: u64) -> PersonaDemand {
        *self
            .observed
            .entry(persona)
            .and_modify(|d| d.floor_tokens = Some(floor_tokens))
            .or_insert_with(|| PersonaDemand { last_seen_ms: now_ms, floor_tokens: Some(floor_tokens), ..PersonaDemand::default() })
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
    /// answer records the think verbatim and the answer as its lower bound (censored,
    /// [`PersonaEmission::answer_samples`]); a cut inside the think records the peak
    /// verbatim and is not a need sample. The peak is always the sum of the channels as
    /// measured. Zero-token completions
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
        self.persist(persona, EMISSION_FILE, serde_json::to_vec(&updated).map_err(std::io::Error::from), "emission"); // BOUNDARY: disk — the per-persona emission.json durable format
        Some(updated)
    }

    /// How much of a past peak survives each new observation (7/8).
    ///
    /// The peak must be able to FALL, or one loud turn is a one-way ratchet to the
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
        // The peak is the SUM of the channels as measured. A cut inside the answer is censored
        // evidence of that channel (her need is above what she wrote): a bound in the answer
        // ring, read by the one censored estimator (`need::answer_need`), never a factor.
        let answer_sample = match stop {
            EmissionStop::CutMidAnswer => ChannelSample::AtLeast { at_least: answer },
            EmissionStop::Landed | EmissionStop::CutMidThought | EmissionStop::ThinkBudgetHit { .. } => ChannelSample::Exact(answer),
        };
        let observed = output_tokens;
        let push_need = |e: &mut PersonaEmission| {
            // A think-only cut is not a sample of her need ([`EmissionStop::CutMidThought`]):
            // it is a lower bound on it, kept in its own ring at the turn it happened.
            if matches!(stop, EmissionStop::CutMidThought | EmissionStop::ThinkBudgetHit { .. }) {
                let slot = (e.cut_count as usize) % NEED_SAMPLES;
                e.cut_samples[slot] = reasoning;
                e.cut_at_turn[slot] = e.turns;
                e.cut_count = e.cut_count.saturating_add(1);
                return;
            }
            let slot = (e.need_turns as usize) % NEED_SAMPLES;
            e.reasoning_samples[slot] = reasoning;
            e.answer_samples[slot] = answer_sample;
            e.need_turns = e.need_turns.saturating_add(1);
        };
        *self
            .emitted
            .entry(persona)
            .and_modify(|e| {
                push_need(&mut *e);
                // A RECENT high-water mark, not an eternal one. `.max()` alone is a
                // one-way ratchet: a mid-answer cut once recorded its answer at DOUBLE, so a single truncated
                // turn pinned the peak at twice the cap forever, the reserve derived from
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
                // instantly, and a peak that was
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
                    answer_samples: [ChannelSample::default(); NEED_SAMPLES],
                    need_turns: 0,
                    cut_samples: [0; NEED_SAMPLES],
                    cut_at_turn: [0; NEED_SAMPLES],
                    cut_count: 0,
                };
                push_need(&mut e);
                e
            })
    }

    /// This persona's observed reply size, for the reserve derivation and the glass box.
    pub(crate) fn emission_of(&self, persona: Uuid) -> Option<PersonaEmission> {
        self.emitted.get(&persona).map(|e| *e.value())
    }

    /// What ONE turn of this mind needs to generate, per channel — `None` until a turn has
    /// landed (a legacy `emission.json` without the rings reads `None` too). Unknown is not
    /// zero: the caller falls back to the cold prior. Any measured turn is used as measured;
    /// how far to trust few turns is the claim's (`OutputNeed::turns`), never a bar here.
    pub fn need_of(&self, persona: Uuid) -> Option<OutputNeed> {
        let e = self.emission_of(persona)?;
        if e.need_turns == 0 {
            return None;
        }
        let filled = (e.need_turns as usize).min(NEED_SAMPLES);
        let landed = &e.reasoning_samples[..filled];
        let reasoning = p90(landed);
        // Cuts among her last NEED_SAMPLES observations (`turns` counts the observation a
        // cut was recorded at before it increments).
        let recent: Vec<u32> = (0..(e.cut_count as usize).min(NEED_SAMPLES))
            .filter(|&slot| e.turns.saturating_sub(e.cut_at_turn[slot]) <= NEED_SAMPLES as u64)
            .map(|slot| e.cut_samples[slot])
            .collect();
        let (answer, answer_at_least) = answer_need(&e.answer_samples[..filled]);
        Some(OutputNeed {
            reasoning,
            answer,
            turns: e.need_turns.min(NEED_SAMPLES as u32),
            reasoning_at_least: reasoning_at_least(landed, &recent, reasoning),
            answer_at_least,
        })
    }

    /// Her need, or, before her own first landed turn, the need the minds on this node measured
    /// (each channel's median across them): a measurement, never a prior. `None` only on a node
    /// where no mind has landed a turn.
    pub fn need_or_pooled(&self, persona: Uuid) -> Option<OutputNeed> {
        self.need_of(persona).or_else(|| self.pooled_need())
    }

    fn pooled_need(&self) -> Option<OutputNeed> {
        // Keys first, so no map guard is held while each need is read.
        let minds: Vec<Uuid> = self.emitted.iter().map(|entry| *entry.key()).collect();
        let needs: Vec<OutputNeed> = minds.into_iter().filter_map(|mind| self.need_of(mind)).collect();
        let median = |mut values: Vec<u32>| -> Option<u32> {
            values.sort_unstable();
            values.get(values.len() / 2).copied()
        };
        Some(OutputNeed {
            reasoning: median(needs.iter().map(|n| n.reasoning).collect())?,
            answer: median(needs.iter().map(|n| n.answer).collect())?,
            turns: 0, // none of them hers
            reasoning_at_least: None,
            answer_at_least: None,
        })
    }

    fn save(&self, persona: Uuid, demand: &PersonaDemand) {
        self.persist(persona, DEMAND_FILE, serde_json::to_vec(demand).map_err(std::io::Error::from), "working-set demand"); // BOUNDARY: disk — the per-persona working-set.json durable format
    }

    /// Re-adopt a mind's measured demand at spawn, so she wakes at the window she
    /// earned rather than the cold-start floor. Unreadable/absent = no observation
    /// (honest), never an invented number.
    pub fn rehydrate(&self, persona: Uuid) {
        let Some(bytes) = self.read(persona, DEMAND_FILE) else {
            return;
        };
        match serde_json::from_slice::<PersonaDemand>(&bytes) {
            // A floor alone is a measurement too: a mind refused on every turn has no peak yet.
            Ok(d) if d.peak_tokens > 0 || d.floor_tokens.is_some() => {
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
                persona_id = %persona, error = %e,
                "working-set file unreadable — this mind re-measures its window from scratch"
            ),
        }
        // The emission twin rides the same rehydration pass: absent/unreadable stays
        // silent (the reserve falls back to the cold-start share — honest, never invented).
        if let Some(bytes) = self.read(persona, EMISSION_FILE) {
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
        let owners = match self.class().and_then(|class| class.owners()) {
            Ok(owners) => owners,
            Err(error) => {
                tracing::warn!(%error, class = PERSONAS_CLASS, "persisted demand could not be enumerated");
                return 0;
            }
        };
        let mut adopted = 0;
        for persona in owners {
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

    /// The seat one mind's turn requires: her typical growth sample ([`Self::sent_median_of`])
    /// grown by HER measured growth ([`growth_of`]), never a fleet-wide factor. `None` until she
    /// has a sample. The one derivation the grid's placement rules read.
    pub fn prompt_floor_for(&self, persona: Uuid) -> Option<u32> {
        let growth = self.observed.get(&persona).map_or(
            crate::inference::planning::window_allocator::Growth::Unmeasured,
            |e| growth_of(e.value()),
        );
        self.sent_median_of(&[persona])
            .and_then(crate::inference::planning::window_allocator::Tokens::new)
            .map(|median| crate::inference::planning::serving_plan::prompt_floor_of(median, growth).get())
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

    /// The `count` minds this host served most recently, newest first: the residents a boot plans
    /// for before its persona registry exists. Only a mind a held turn measured counts (her need is
    /// what a turn held here, and her wish what her view holds at full detail); a departed mind or a
    /// test fixture that never held a turn is not a resident. On the M5, 2026-11-13 19:36Z, a boot
    /// with no residents in its demand launched the engine at a 2,048-token pool beside four minds
    /// whose requirements were 68-110k each.
    pub fn remembered_residents(&self, count: usize) -> Vec<Uuid> {
        let mut held: Vec<(u64, Uuid)> = self
            .observed
            .iter()
            .filter(|e| resident_need(e.value()).is_some() && requirement_tokens(e.value()) > 0)
            .map(|e| (e.value().last_seen_ms, *e.key()))
            .collect();
        held.sort_unstable_by(|a, b| b.cmp(a));
        held.into_iter().take(count).map(|(_, mind)| mind).collect()
    }
}

/// How far her act grows past her typical act: the p95 of her recent untrimmed demands over their
/// median, measured once her ring holds a full working session ([`RECENT_TURNS`]), unmeasured
/// before (`window_allocator::Growth`).
pub fn growth_of(d: &PersonaDemand) -> crate::inference::planning::window_allocator::Growth {
    // A zero slot is a turn not recorded yet; only recorded demands are samples.
    let recorded = d.recent.iter().copied().filter_map(crate::inference::planning::window_allocator::Tokens::new);
    crate::inference::planning::window_allocator::Growth::of(recorded, RECENT_TURNS)
}

/// Each mind's requirement, from the untrimmed demand her turns assemble grown by her own
/// measured growth ([`growth_of`]), with her parked training examples' reservation beside it (never
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
            use crate::inference::planning::window_allocator::{LaneRequirement, Tokens};
            // Measured or not, as a type: no turn yet is unmeasured, never a zero standing in for one.
            let wish = demand.as_ref().and_then(|d| Tokens::new(requirement_tokens(d)));
            // Her need as measured, a bound kept a bound: an overflow is reducible by her view's
            // collapse, so admission clamps it to what one sequence holds (`resident_need`).
            let need = demand.as_ref().and_then(resident_need);
            // Her parked training examples' reservation, carried beside her conversational target
            // and need (card ea157b1a): the allocator grows toward it only from spare budget.
            let training = crate::genome::fine_tuning::training_demand::TrainingDemand::for_persona(*p).reservation();
            let measured = match (wish, demand.as_ref()) {
                (Some(wish), Some(d)) => LaneRequirement::wished(*p, wish, growth_of(d)),
                _ => LaneRequirement::unmeasured(*p),
            };
            match need {
                Some(need) => measured.held(need),
                None => measured,
            }
            .with_training(training)
        })
        .collect()
}

#[cfg(test)]
mod tests;
