//! `LocalSubstrateGovernor` — reference impl of the `SubstrateGovernor`
//! trait. Lane H PR-3b per GENOME-FOUNDRY-SENTINEL #1327 Part 11.
//!
//! PR-3a (#1352) shipped policy SELECTION (`HardwareClass + Vec<PolicyFile>
//! → PolicyFile`). This PR-3b ships the implementation that PUBLISHES
//! the selected policy + holds the cascade-snapshot state. Other
//! modules (tier stores, recall, composer, speculator) read via
//! `current_policy()` — wait-free `Arc<GovernorPolicy>` clone.
//!
//! ## Scope of PR-3b
//!
//! - `LocalSubstrateGovernor` struct holding `Arc<ArcSwap<GovernorPolicy>>`
//!   plus `Mutex<GovernorSnapshot>` (snapshot history is mutex-protected;
//!   policy reads are arc_swap'd lock-free)
//! - Impl `SubstrateGovernor` trait: `current_policy + on_hardware_detected
//!   + on_pressure_signal + snapshot`
//! - `new(initial_policy)` constructor
//! - `on_hardware_detected(hw)` selects + publishes a new policy by
//!   re-running the policy_selector logic over the cached candidate
//!   list (caller supplies the candidates via `set_candidates`). If
//!   selection fails, the typed error returns to the caller and the
//!   current policy remains intact.
//! - `on_pressure_signal(signal)` for PR-3b: RECORDS the signal in
//!   recent_signals (bounded ring) + increments cascade_transition_count
//!   when a signal-bearing state change occurs. The full threshold +
//!   hysteresis cascade lands in PR-3c.
//! - `snapshot()` returns a `GovernorSnapshot` clone with current
//!   policy + transition count + recent signals
//!
//! ## Concurrency model
//!
//! Reads (`current_policy`) are wait-free `arc_swap` loads + `Arc`
//! clones. A composer reading the policy 1000× per turn pays no
//! contention cost.
//!
//! Writes (`on_hardware_detected`, `on_pressure_signal`) hold a small
//! mutex on the snapshot history + atomically publish via `arc_swap`.
//! Mutex hold time should be under a microsecond.
//!
//! ## What this PR DOES NOT do
//!
//! - Cascade state machine + thresholds (PR-3c)
//! - File watcher / hot reload (PR-3d)
//! - PressureBroker subscription wiring (PR-4)
//! - Policy directory discovery (PR-3d); callers must provide explicit
//!   candidates via `set_candidates`

use crate::governor::cascade::{
    apply_action, apply_cascade_step_to_policy, evaluate_next_step, CascadeAction,
    CascadeThresholds,
};
use crate::governor::policy_selector::{select_policy, PolicySelectionError};
use crate::governor::types::{GovernorPolicy, GovernorSnapshot, HardwareClass, PressureSignal};
use crate::governor::PolicyFile;
use crate::governor::SubstrateGovernor;
use arc_swap::ArcSwap;
use std::sync::{Arc, Mutex};

/// Minimum time the cascade must stay in a step before advancing
/// further. Per spec §"Adjustment Cascade": step 1 must be active
/// for more than 30 seconds before advancing to step 2; same shape
/// for step 2 to 3 (30s), step 3 to 4 (60s). PR-3c2 uses a single
/// conservative value for all transitions; PR-3c3 can per-step-tune
/// if the spec's 30s/30s/60s ladder matters.
///
/// EmergencyAdvanceToMax bypasses this gate entirely — thermal
/// Critical + battery < emergency_pct skip straight to max regardless
/// of time-in-step.
///
/// Retreat is not gated by time-in-step — the cascade may retreat as
/// soon as conditions clear (the all-clear exit threshold IS the
/// hysteresis; doubling-up with a time gate would over-throttle).
pub const MIN_TIME_IN_STEP_MS: u64 = 30_000;

/// Maximum number of recent pressure signals retained in the snapshot.
/// The ring evicts oldest-first. Diagnostic — operators look at the
/// last N events to understand "why did the governor cascade just now."
const RECENT_SIGNALS_CAPACITY: usize = 32;

/// Reference `SubstrateGovernor` implementation. Holds the live policy
/// behind `arc_swap` for wait-free reads + a mutex-protected snapshot
/// history for telemetry.
pub struct LocalSubstrateGovernor {
    /// Wait-free policy publish. `current_policy()` is an
    /// `ArcSwap::load_full()` (returns `Arc<GovernorPolicy>`); writers
    /// `store(Arc::new(new_policy))`. This is the ACTIVE (possibly-
    /// throttled) policy; see `base_policy` for the un-throttled
    /// canonical version.
    policy: Arc<ArcSwap<GovernorPolicy>>,

    /// BASE policy — the canonical un-throttled policy as loaded from
    /// the policy file (cascade_step always 0). Cascade transitions
    /// always derive the new ACTIVE policy by calling
    /// `apply_cascade_step_to_policy(base, new_step)` rather than
    /// transforming the already-throttled current policy. This is
    /// what `apply_cascade_step_to_policy`'s `not_reversible` test
    /// (PR-3c3) was preparing for — keep the base separate so retreat
    /// can re-derive cleanly.
    ///
    /// Mutex-protected because `on_hardware_detected` rewrites it
    /// when a new HardwareClass is detected; cascade transitions
    /// only READ it under the same mutex.
    base_policy: Mutex<GovernorPolicy>,

    /// Pool of candidate policy files. `on_hardware_detected` walks
    /// this with `select_policy` (PR-3a) to pick the best match.
    /// Empty until `set_candidates` is called — until then,
    /// `on_hardware_detected` returns `NoMatchingPolicy` and leaves the
    /// current policy unchanged.
    candidates: Mutex<Vec<PolicyFile>>,

    /// Snapshot history — recent pressure signals + cascade transition
    /// counter. Mutex-protected (only telemetry callers contend).
    snapshot_state: Mutex<SnapshotState>,
}

struct SnapshotState {
    cascade_transition_count: u64,
    recent_signals: Vec<PressureSignal>,
    /// Restore-speculation-one-step-later marker (PR-3c4). When the
    /// cascade RETREATS from step N → N-1, set this true. On the
    /// NEXT retreat (or the next inactivity check), apply the lower
    /// step's transformations BUT keep speculation at the previous
    /// (one-higher-step) value for one more cycle. Clears when the
    /// cycle completes.
    ///
    /// The spec's "restore speculation one step later" rule is the
    /// load-bearing anti-oscillation guarantee — speculation thrash
    /// is the most user-visible cascade flapping, and keeping it
    /// dampened by one step prevents back-and-forth.
    pending_speculation_retreat: bool,
    /// Current cascade step. Mirrors `policy.cascade_step` but tracked
    /// here separately so the time-in-step gate doesn't have to
    /// arc_swap-load the full policy on every signal.
    current_step: u8,
    /// Unix-ms timestamp the cascade last transitioned (advance or
    /// retreat). Used by the time-in-step gate to enforce the spec's
    /// "step N must be active > 30s before advancing to step N+1"
    /// rule. PR-3c2 uses a single value (`MIN_TIME_IN_STEP_MS`); PR-3c3
    /// may per-step-tune if the spec's ladder matters.
    last_step_change_ms: u64,
    /// Cascade thresholds — used by `evaluate_next_step`. Carried in
    /// the state so PR-3c3 can hot-reload them when the policy file
    /// changes (PR-3d's file watcher).
    thresholds: CascadeThresholds,
}

impl LocalSubstrateGovernor {
    /// Construct with an initial policy. The governor starts ready to
    /// serve `current_policy()` immediately. `set_candidates` +
    /// `on_hardware_detected` can rewrite later.
    pub fn new(initial_policy: GovernorPolicy) -> Self {
        let initial_step = initial_policy.cascade_step;
        // The initial policy IS the base — caller passes the
        // canonical un-throttled version. Cascade transitions
        // re-derive ACTIVE from BASE; if cascade_step != 0 at
        // construction time, we still treat the supplied policy
        // as base (cascade_step normalization is the caller's job).
        let mut base = initial_policy.clone();
        base.cascade_step = 0;
        Self {
            policy: Arc::new(ArcSwap::from(Arc::new(initial_policy))),
            base_policy: Mutex::new(base),
            candidates: Mutex::new(Vec::new()),
            snapshot_state: Mutex::new(SnapshotState {
                cascade_transition_count: 0,
                recent_signals: Vec::with_capacity(RECENT_SIGNALS_CAPACITY),
                current_step: initial_step,
                last_step_change_ms: crate::time::EpochMs::now().millis(),
                thresholds: CascadeThresholds::default(),
                pending_speculation_retreat: false,
            }),
        }
    }

    /// Override the cascade thresholds (PR-3d wires the policy-file
    /// hot-reload path; for PR-3c2 callers can set manually for tests).
    pub fn set_thresholds(&self, thresholds: CascadeThresholds) {
        let mut state = self
            .snapshot_state
            .lock()
            .expect("LocalSubstrateGovernor snapshot mutex poisoned");
        state.thresholds = thresholds;
    }

    /// Current cascade step. Diagnostic — tests + telemetry consumers
    /// can introspect without going through snapshot().
    pub fn current_cascade_step(&self) -> u8 {
        self.snapshot_state
            .lock()
            .expect("LocalSubstrateGovernor snapshot mutex poisoned")
            .current_step
    }

    /// Set the pool of candidate policy files used by
    /// `on_hardware_detected`. Replaces any prior candidates atomically.
    /// PR-3d (file watcher) calls this on file-system change events.
    pub fn set_candidates(&self, candidates: Vec<PolicyFile>) {
        let mut guard = self
            .candidates
            .lock()
            .expect("LocalSubstrateGovernor candidates mutex poisoned");
        *guard = candidates;
    }

    /// Snapshot-only: how many candidates are currently registered.
    /// Diagnostic for "did the file watcher actually load anything?"
    pub fn candidate_count(&self) -> usize {
        self.candidates
            .lock()
            .expect("LocalSubstrateGovernor candidates mutex poisoned")
            .len()
    }

    /// Internal: publish a new policy via arc_swap + bump the cascade
    /// transition counter (every publish is a transition).
    fn publish(&self, new_policy: GovernorPolicy) {
        self.policy.store(Arc::new(new_policy));
        let mut state = self
            .snapshot_state
            .lock()
            .expect("LocalSubstrateGovernor snapshot mutex poisoned");
        state.cascade_transition_count = state.cascade_transition_count.saturating_add(1);
    }

    /// Select a new policy for the given hardware. Selection failures
    /// are typed and leave the current policy untouched. Successful
    /// selection publishes the new policy + returns `Ok(())`.
    pub fn try_hardware_detected(&self, hw: HardwareClass) -> Result<(), PolicySelectionError> {
        let candidates = self
            .candidates
            .lock()
            .expect("LocalSubstrateGovernor candidates mutex poisoned");
        let selected = select_policy(&candidates, &hw)?;
        let new_policy = crate::governor::into_governor_policy(selected.clone(), hw, crate::time::EpochMs::now().millis());
        drop(candidates);

        // PR-3c4: refresh BASE policy too. New hardware = new canonical
        // base; cascade transitions re-derive from this. Reset the
        // cascade to step 0 (new hardware = fresh start; if pressure
        // returns, the cascade re-evaluates from a known-good state).
        {
            let mut base = self
                .base_policy
                .lock()
                .expect("LocalSubstrateGovernor base_policy mutex poisoned");
            *base = new_policy.clone();
            base.cascade_step = 0;
        }
        {
            let mut state = self
                .snapshot_state
                .lock()
                .expect("LocalSubstrateGovernor snapshot mutex poisoned");
            state.current_step = 0;
            state.last_step_change_ms = crate::time::EpochMs::now().millis();
            state.pending_speculation_retreat = false;
        }

        self.publish(new_policy);
        Ok(())
    }
}

impl SubstrateGovernor for LocalSubstrateGovernor {
    fn current_policy(&self) -> Arc<GovernorPolicy> {
        self.policy.load_full()
    }

    fn on_hardware_detected(&self, hw: HardwareClass) -> Result<(), PolicySelectionError> {
        self.try_hardware_detected(hw)
    }

    fn on_pressure_signal(&self, signal: PressureSignal) {
        // PR-3c2 wiring + PR-3c4 base-vs-active split:
        // - record signal in ring
        // - evaluate cascade action (Hold/Advance/Retreat/EmergencyAdvanceToMax)
        // - time-in-step gate blocks Advance from step > 0 within
        //   MIN_TIME_IN_STEP_MS (brief spikes don't escalate)
        // - EmergencyAdvanceToMax bypasses gate (protect hardware/user)
        // - Retreat never gated (hysteresis IS the anti-oscillation)
        // - On step change: derive new ACTIVE from BASE via
        //   apply_cascade_step_to_policy (not from current — keeps
        //   transformations symmetric + reversible)
        // - Restore-speculation-one-step-later: on retreat, keep
        //   speculation at the higher-step value for one more cycle
        let now = crate::time::EpochMs::now().millis();
        let mut new_policy_to_publish: Option<GovernorPolicy> = None;

        {
            let mut state = self
                .snapshot_state
                .lock()
                .expect("LocalSubstrateGovernor snapshot mutex poisoned");

            // Record the signal in the ring.
            if state.recent_signals.len() >= RECENT_SIGNALS_CAPACITY {
                state.recent_signals.remove(0);
            }
            state.recent_signals.push(signal);

            let action = evaluate_next_step(state.current_step, &signal, &state.thresholds);

            let gated_action = match action {
                CascadeAction::Advance => {
                    let time_in_step = now.saturating_sub(state.last_step_change_ms);
                    if state.current_step > 0 && time_in_step < MIN_TIME_IN_STEP_MS {
                        CascadeAction::Hold
                    } else {
                        action
                    }
                }
                _ => action,
            };

            let prev_step = state.current_step;
            let new_step = apply_action(prev_step, gated_action);
            if new_step != prev_step {
                state.current_step = new_step;
                state.last_step_change_ms = now;

                // Whether THIS transition is a retreat (used for
                // restore-speculation-one-step-later logic).
                let is_retreat = new_step < prev_step;

                // Re-derive active policy from BASE — NOT from current.
                // Per PR-3c3's not-reversible test: transformations
                // applied to an already-transformed policy don't undo
                // cleanly. Always derive from the canonical base.
                let base_clone: GovernorPolicy = self
                    .base_policy
                    .lock()
                    .expect("LocalSubstrateGovernor base_policy mutex poisoned")
                    .clone();

                let mut next_policy = apply_cascade_step_to_policy(&base_clone, new_step);

                // Restore-speculation-one-step-later: on retreat, keep
                // speculation at the PREVIOUS-step (higher) value for
                // one more cycle. This dampens speculation thrash —
                // the most user-visible cascade flapping per spec.
                //
                // On advance, clear any pending retreat marker — new
                // pressure means we're going up, not still completing
                // a previous restoration.
                if is_retreat {
                    // Compute what the previous step's speculation
                    // would have been + use that instead of new_step's.
                    let prev_step_policy = apply_cascade_step_to_policy(&base_clone, prev_step);
                    next_policy.speculation_aggressiveness =
                        prev_step_policy.speculation_aggressiveness;
                    state.pending_speculation_retreat = true;
                } else if state.pending_speculation_retreat
                    && gated_action == CascadeAction::Advance
                {
                    // Advancing again clears the pending-retreat marker
                    // since speculation will be re-throttled by the
                    // new (higher) step's transformations.
                    state.pending_speculation_retreat = false;
                }

                next_policy.policy_version =
                    self.policy.load_full().policy_version.saturating_add(1);
                next_policy.committed_at_ms = now;
                new_policy_to_publish = Some(next_policy);
            } else if state.pending_speculation_retreat && gated_action == CascadeAction::Hold {
                // Hold with pending retreat marker → restore speculation
                // to the lower-step value (the "one cycle later" delivery).
                // This is the second-half of the restore-one-step-later
                // semantics: first retreat keeps speculation high; next
                // Hold-or-Retreat clears it.
                let base_clone: GovernorPolicy = self
                    .base_policy
                    .lock()
                    .expect("LocalSubstrateGovernor base_policy mutex poisoned")
                    .clone();
                let mut next_policy = apply_cascade_step_to_policy(&base_clone, state.current_step);
                next_policy.policy_version =
                    self.policy.load_full().policy_version.saturating_add(1);
                next_policy.committed_at_ms = now;
                state.pending_speculation_retreat = false;
                // Don't bump cascade_transition_count for this — the
                // step didn't change, only speculation restored.
                self.policy.store(Arc::new(next_policy));
                return;
            }
        }
        if let Some(policy) = new_policy_to_publish {
            self.publish(policy);
        }
    }

    fn snapshot(&self) -> GovernorSnapshot {
        let policy = self.current_policy();
        let state = self
            .snapshot_state
            .lock()
            .expect("LocalSubstrateGovernor snapshot mutex poisoned");
        GovernorSnapshot {
            current_policy: (*policy).clone(),
            cascade_transition_count: state.cascade_transition_count,
            recent_signals: state.recent_signals.clone(),
        }
    }
}

#[cfg(test)]
mod tests;
