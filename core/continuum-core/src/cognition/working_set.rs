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
fn personas_root() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into()); // JUSTIFIED unwrap_or_else: a HOME-less process is a real environment, not an unknown quantity; "." keeps the path RELATIVE so a demand file lands somewhere inspectable instead of at the filesystem root
    std::path::PathBuf::from(home).join(".continuum/personas")
}

fn demand_path(persona: Uuid) -> std::path::PathBuf {
    personas_root().join(persona.to_string()).join("working-set.json")
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
}

/// Per-persona observed turn demand for ONE core.
///
/// Cheap to clone (`Arc` inside) so the deliberation faculty, the serving daemon,
/// and a status command can all hold the same registry without threading a lock
/// through their signatures.
#[derive(Debug, Clone, Default)]
pub struct WorkingSetRegistry {
    observed: Arc<DashMap<Uuid, PersonaDemand>>,
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
            })
    }

    /// Atomic tmp+rename so a crash mid-write never leaves a torn file that would
    /// fail to parse and silently wake her at the cold-start window.
    fn save(persona: Uuid, demand: &PersonaDemand) {
        let path = demand_path(persona);
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
        let path = demand_path(persona);
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
    /// precisely what a `bound_by=host-fit` plan is — under `CO_CONSUMER_HEADROOM` and the
    /// governor's own `budget_for_replacing`. Raising demand can only raise the ASK; the
    /// governor still decides. The failure mode of asking for too little is the one we
    /// measured; the failure mode of asking for too much is a plan that says `host-fit`.
    ///
    /// Unreadable or absent files stay silent — the same honesty `rehydrate` keeps. A
    /// ghost persona dir with no `working-set.json` contributes nothing rather than a zero.
    pub fn rehydrate_all(&self) -> usize {
        let Ok(entries) = std::fs::read_dir(personas_root()) else {
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

    // what this catches: a restart that demotes her. The registry is in-memory, so
    // before persistence a reboot erased every measurement and the planner fell back
    // to the cold-start constant until enough turns re-measured — observed live
    // 2026-08-06, where a measured 24,126 window fell to 16,384 across one reboot and
    // the citizens were re-strangled until they earned it back. Joel's standard for a
    // restart is a PAUSE, not a death.
    #[test]
    fn a_measured_peak_survives_a_restart_so_a_reboot_is_a_pause_not_a_demotion() {
        let home = std::env::temp_dir().join(format!("ws-restart-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&home).expect("tmp home");
        // SAFETY: single-threaded test scope; HOME is restored below.
        let prior = std::env::var("HOME").ok();
        std::env::set_var("HOME", &home);

        let persona = p(9);
        let before = WorkingSetRegistry::new();
        before.record(persona, 24_126, 1_000);
        assert_eq!(before.ceiling(), Some(24_126));

        // A fresh process: new registry, nothing in memory.
        let after = WorkingSetRegistry::new();
        assert_eq!(
            after.ceiling(),
            None,
            "a new registry starts genuinely empty"
        );
        after.rehydrate(persona);
        assert_eq!(
            after.ceiling(),
            Some(24_126),
            "her measured window must survive the restart, not be re-earned turn by turn"
        );

        match prior {
            Some(h) => std::env::set_var("HOME", h),
            None => std::env::remove_var("HOME"),
        }
        let _ = std::fs::remove_dir_all(&home);
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
