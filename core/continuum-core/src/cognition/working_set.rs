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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonaDemand {
    /// High-water mark, in tokens, of a full unclamped turn for this persona.
    pub peak_tokens: u32,
    /// The most recent observation's value — the peak's honest companion, so the
    /// glass box can show "peaked at 47k, currently running 9k" rather than only
    /// the extreme.
    pub last_tokens: u32,
    /// Wall clock of the most recent observation.
    pub last_seen_ms: u64,
    /// How many turns have been observed. One observation is a measurement; the
    /// count is what lets a reader judge how much to trust the peak.
    pub turns: u64,
    /// The largest prompt this mind actually SENT (post-fit: framing + fitted
    /// messages + reserve). `peak_tokens` says how much she COULD use (225k–505k
    /// measured 2026-09-13 — the whole assembled context); this says how much a turn
    /// needs to FIT. The served window follows this with headroom, so a 25k working
    /// set no longer provisions a 137k slot that starves the RAM tier and the lane
    /// count. Legacy files without it read 0.
    #[serde(default)]
    pub sent_peak: u32,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PersonaEmission {
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
    pub(crate) fn record_in_memory(
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
                d.last_seen_ms = now_ms;
                d.turns += 1;
            })
            .or_insert(PersonaDemand {
                peak_tokens: demand_tokens,
                last_tokens: demand_tokens,
                last_seen_ms: now_ms,
                turns: 1,
                sent_peak: 0,
            })
    }

    /// Record what a turn actually SENT (post-fit). Persisted with the demand.
    pub fn record_sent(&self, persona: Uuid, sent_tokens: u32, now_ms: u64) {
        if sent_tokens == 0 {
            return;
        }
        let updated = *self
            .observed
            .entry(persona)
            .and_modify(|d| d.sent_peak = d.sent_peak.max(sent_tokens))
            .or_insert(PersonaDemand {
                peak_tokens: 0,
                last_tokens: 0,
                last_seen_ms: now_ms,
                turns: 0,
                sent_peak: sent_tokens,
            });
        Self::save(persona, &updated);
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

    /// The TYPICAL sent prompt among `personas` — the median of their measured sent
    /// peaks. The planner's per-lane floor follows this, not the ceiling: one mind
    /// with a 96k prompt must not size every lane to 120k and starve the roster
    /// (2026-09-14: 16 residents on 3 lanes). The outlier is reconciled down to the
    /// served window instead.
    pub fn sent_median_of(&self, personas: &[Uuid]) -> Option<u32> {
        self.sent_median_with(personas, &[])
    }
    /// The typical prompt over the residents' sent peaks PLUS `extra` samples — the prompt
    /// sizes of the generates this seat served for minds hosted on other nodes
    /// ([`crate::cognition::resource_admission::leased_in_sent_samples`]). ONE median for
    /// the seat's whole demand pool: a seat that plans lanes for leased-in minds must size
    /// those lanes to the prompts they actually send, not to one local resident's. Zeros
    /// are not samples on either side.
    pub fn sent_median_with(&self, personas: &[Uuid], extra: &[u32]) -> Option<u32> {
        let mut peaks: Vec<u32> = personas
            .iter()
            .filter_map(|p| self.observed.get(p).map(|e| e.value().sent_peak))
            .chain(extra.iter().copied())
            .filter(|&t| t > 0)
            .collect();
        if peaks.is_empty() {
            return None;
        }
        peaks.sort_unstable();
        Some(peaks[peaks.len() / 2])
    }
    /// The largest SENT prompt among `personas`, when any has been measured.
    pub fn sent_ceiling_of(&self, personas: &[Uuid]) -> Option<u32> {
        personas
            .iter()
            .filter_map(|p| self.observed.get(p).map(|e| e.value().sent_peak))
            .max()
            .filter(|&t| t > 0)
    }

    /// Record one completed generation's measured output for `persona`.
    ///
    /// `output_tokens` is the SERVER's count (`usage.output_tokens`) — reasoning
    /// tokens included, never an estimate. `hit_cap` marks a `FinishReason::Length`
    /// stop: that emission was clamped by the very reserve this measurement sizes,
    /// so it records at double (a floor on true demand, and the growth path — see
    /// [`PersonaEmission::peak_tokens`]). Zero-token completions are the empty-
    /// completion fault's territory, not a data point to drag the peak with.
    pub(crate) fn record_emission(
        &self,
        persona: Uuid,
        output_tokens: u32,
        hit_cap: bool,
        now_ms: u64,
    ) {
        if output_tokens == 0 {
            return;
        }
        let updated = self.record_emission_in_memory(persona, output_tokens, hit_cap, now_ms);
        // Same persistence contract as demand: every observation, atomic, best-effort
        // — a restart is a pause, and a mind must not re-earn its reply size per boot.
        let path = match emission_path(persona) {
            Ok(path) => path,
            Err(error) => {
                tracing::warn!(persona_id = %persona, %error, "emission root unavailable — measurement remains in memory");
                return;
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
    }

    /// How much of a past peak survives each new observation (7/8).
    ///
    /// The peak must be able to FALL, or `hit_cap`'s doubling is a one-way ratchet to the
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
    pub(crate) fn record_emission_in_memory(
        &self,
        persona: Uuid,
        output_tokens: u32,
        hit_cap: bool,
        now_ms: u64,
    ) -> PersonaEmission {
        let observed = if hit_cap {
            output_tokens.saturating_mul(2)
        } else {
            output_tokens
        };
        *self
            .emitted
            .entry(persona)
            .and_modify(|e| {
                // A RECENT high-water mark, not an eternal one. `.max()` alone is a
                // one-way ratchet: `hit_cap` records at DOUBLE, so a single truncated
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
                // instantly (the growth path `hit_cap` exists for), and a peak that was
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
            .or_insert(PersonaEmission {
                peak_tokens: observed,
                last_tokens: observed,
                last_seen_ms: now_ms,
                turns: 1,
            })
    }

    /// This persona's observed reply size, for the reserve derivation and the glass box.
    pub(crate) fn emission_of(&self, persona: Uuid) -> Option<PersonaEmission> {
        self.emitted.get(&persona).map(|e| *e.value())
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
    /// precisely what a `bound_by=host-fit` plan is — under the power mode's fraction (the
    /// plan's only headroom since #4230; Performance keeps the thinnest, 4%) and the
    /// governor's own `budget_for_replacing`. Raising demand can only raise the ASK; the
    /// governor still decides. The failure mode of asking for too little is the one we
    /// measured; the failure mode of asking for too much is a plan that says `host-fit`.
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

#[cfg(test)]
mod tests {

    // what this catches (BigMama's correction on #4197, 2026-09-18): a seat whose floor is
    // its residents' median alone. The 5090 had two residents; the median WAS one SWE
    // prompt (~60k+), so 2 × 49k stayed under the floor and the seat kept one lane while
    // twelve leased-in coders sending ~30k queued on it. Their prompts must pull the median
    // to what the seat actually serves; and a seat with NO local residents but leased-in
    // minds must still have a floor.
    #[test]
    fn the_typical_prompt_includes_the_prompts_a_seat_serves_for_other_nodes() {
        let reg = WorkingSetRegistry::new();
        let local = Uuid::new_v4();
        reg.record_sent(local, 64_000, 1);
        assert_eq!(reg.sent_median_of(&[local]), Some(64_000), "one resident: her prompt is the median");
        // twelve leased-in coders at ~30k pull the typical prompt to theirs
        let leased: Vec<u32> = (0..12).map(|i| 30_000 + i * 100).collect();
        let m = reg.sent_median_with(&[local], &leased).expect("a pool");
        assert!(m < 40_000, "the median follows the grid's prompts, not the one resident's: {m}");
        assert!(m >= 30_000);
        // no local residents at all, only leased-in: still a floor
        assert_eq!(reg.sent_median_with(&[], &[30_000, 32_000, 34_000]), Some(32_000));
        // zeros are not samples on either side
        assert_eq!(reg.sent_median_with(&[], &[0, 0]), None);
        assert_eq!(reg.sent_median_of(&[Uuid::new_v4()]), None);
    }

    use super::*;

    fn p(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    // what this catches: the thermostat-inside-the-thermometer failure. Demand ABOVE
    // the window currently served is the only signal that can ever grow the window, so
    // the registry must accept and keep it rather than clamping to anything it knows
    // about the current serving state. If this ever starts capping, an 8k-served
    // citizen can never report that her turn wanted 47k, and the window is frozen at
    // whatever it was first set to — which is the exact bug this module replaces.
    #[test]
    fn demand_far_above_the_current_window_is_recorded_not_clamped() {
        let reg = WorkingSetRegistry::new();
        reg.record(p(1), 47_000, 1_000);
        assert_eq!(reg.ceiling(), Some(47_000));
        assert_eq!(reg.demand_of(p(1)).map(|d| d.peak_tokens), Some(47_000));
    }

    // what this catches: averaging a coding turn with idle chatter into a window that
    // serves neither. The high-water mark is the point — a later small turn must not
    // walk the ceiling back down, or one quiet minute re-strangles the next code turn.
    #[test]
    fn a_later_smaller_turn_never_lowers_the_peak() {
        let reg = WorkingSetRegistry::new();
        reg.record(p(1), 40_000, 1_000);
        reg.record(p(1), 900, 2_000);
        let d = reg.demand_of(p(1)).expect("observed");
        assert_eq!(d.peak_tokens, 40_000, "peak is the high-water mark");
        assert_eq!(d.last_tokens, 900, "…and the latest is kept alongside it");
        assert_eq!(d.turns, 2);
        assert_eq!(reg.ceiling(), Some(40_000));
    }

    // what this catches: provisioning the host for one citizen and starving the other.
    // Lanes share one served window, so the ceiling must be the MAX across minds — the
    // busiest resident is the one whose demand decides whether the lane is big enough.
    #[test]
    fn the_ceiling_is_the_busiest_minds_demand_not_an_average() {
        let reg = WorkingSetRegistry::new();
        reg.record(p(1), 6_000, 1_000);
        reg.record(p(2), 31_000, 1_000);
        reg.record(p(3), 4_000, 1_000);
        assert_eq!(reg.ceiling(), Some(31_000));
    }

    // what this catches: the growth path of the measured reply reserve. A turn that
    // stopped at its output cap is a CLAMPED observation — recording it verbatim
    // would freeze the reserve at whatever strangled it (the same measure-the-clamp
    // trap the demand side documents). It must record at double so the next turn's
    // reserve is larger than the cap that cut this one.
    #[test]
    fn a_capped_emission_records_double_and_an_uncapped_one_verbatim() {
        let reg = WorkingSetRegistry::new();
        reg.record_emission_in_memory(p(1), 2_500, false, 1_000);
        assert_eq!(reg.emission_of(p(1)).map(|e| e.peak_tokens), Some(2_500));
        reg.record_emission_in_memory(p(1), 3_000, true, 2_000);
        let e = reg.emission_of(p(1)).expect("observed");
        assert_eq!(
            e.peak_tokens, 6_000,
            "a Length stop is a floor, not a measurement"
        );
        assert_eq!(e.turns, 2);
        // A later small reply lowers the peak, but only by the decay step — it must not
        // collapse to the size of one ack (that is the tiny-talker trap the floor exists
        // for), and it must not stay pinned either (that is the ratchet that poisoned a
        // citizen's emission.json at 16,384 and left her 297 tokens of context).
        reg.record_emission_in_memory(p(1), 40, false, 3_000);
        let decayed = reg
            .emission_of(p(1))
            .map(|e| e.peak_tokens)
            .expect("observed");
        assert!(decayed < 6_000, "a peak that can only rise is a ratchet");
        assert!(decayed > 40, "one small reply must not erase a real measurement");
        assert_eq!(decayed, 6_000 * 7 / 8);
    }

    // what this catches: the poisoned-record half of the 2026-09-18 identity defect. A
    // capped turn records at DOUBLE, and with an all-time `.max()` that value outlived
    // every restart because emission.json is persisted on purpose ("a restart is a pause").
    // One truncation therefore reserved half of every future window forever. The decay must
    // make such a record heal ON ITS OWN — no migration, no operator step, no command —
    // because the citizens carrying poisoned files are the ones least able to ask for help.
    #[test]
    fn a_peak_that_was_never_real_heals_itself_without_a_migration() {
        let reg = WorkingSetRegistry::new();
        // Exactly the poisoned value read off a real citizen's emission.json.
        reg.record_emission_in_memory(p(7), 8_192, true, 1_000);
        assert_eq!(
            reg.emission_of(p(7)).map(|e| e.peak_tokens),
            Some(16_384),
            "fixture must reproduce the poisoning, not assume it"
        );

        // Her actual replies are a few hundred tokens. Within a work session of them the
        // measurement must come back to something a reply-sized reserve can be built on.
        for turn in 2..=40u64 {
            reg.record_emission_in_memory(p(7), 300, false, turn * 1_000);
        }
        let healed = reg
            .emission_of(p(7))
            .map(|e| e.peak_tokens)
            .expect("observed");
        assert!(
            healed <= 2_048,
            "a peak that was never real is still {healed} after 39 honest turns"
        );
        assert!(healed >= 300, "and it must not fall below what she actually writes");
    }

    // what this catches: a restart that demotes her. The registry is in-memory, so
    // before persistence a reboot erased every measurement and the planner fell back
    // to the cold-start constant until enough turns re-measured — observed live
    // 2026-08-06, where a measured 24,126 window fell to 16,384 across one reboot and
    // the citizens were re-strangled until they earned it back. Joel's standard for a
    // restart is a PAUSE, not a death.
    #[test]
    fn a_measured_peak_survives_a_restart_so_a_reboot_is_a_pause_not_a_demotion() {
        let home = tempfile::tempdir().expect("tmp home");
        // HOME is absent at the shared policy seam; native discovery points at
        // this thread's isolated home. Never mutate the parallel suite's env.
        let _native = crate::paths::NativeHomeOverride::install(home.path());

        let persona = p(9);
        let before = WorkingSetRegistry::new();
        before.record(persona, 24_126, 1_000);
        before.record_emission(persona, 321, false, 1_001);
        assert_eq!(before.ceiling(), Some(24_126));

        // A fresh process: new registry, nothing in memory.
        let after = WorkingSetRegistry::new();
        assert_eq!(
            after.ceiling(),
            None,
            "a new registry starts genuinely empty"
        );
        assert_eq!(after.rehydrate_all(), 1);
        assert_eq!(
            after.ceiling(),
            Some(24_126),
            "her measured window must survive the restart, not be re-earned turn by turn"
        );

        assert_eq!(after.emission_of(persona), before.emission_of(persona));
        assert!(home
            .path()
            .join(".continuum/personas")
            .join(persona.to_string())
            .join("working-set.json")
            .is_file());
    }

    // what this catches: an invented number standing in for missing data. Before any
    // turn is measured there IS no measurement, and this must say so — the cold-start
    // decision belongs to the serving planner, in one place, not smuggled in here as a
    // default that every caller then inherits without noticing.
    #[test]
    fn no_observations_reports_absence_never_a_stand_in_number() {
        let reg = WorkingSetRegistry::new();
        assert_eq!(reg.ceiling(), None);
        assert_eq!(reg.observed_personas(), 0);
        // A zero-token turn is a defect signal elsewhere, not an observation here.
        reg.record(p(1), 0, 1_000);
        assert_eq!(
            reg.ceiling(),
            None,
            "a zero demand must not register as data"
        );
    }
}
