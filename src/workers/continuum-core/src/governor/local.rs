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
//!   + `Mutex<GovernorSnapshot>` (snapshot history is mutex-protected;
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

use crate::governor::policy_selector::{select_policy, PolicySelectionError};
use crate::governor::types::{GovernorPolicy, GovernorSnapshot, HardwareClass, PressureSignal};
use crate::governor::PolicyFile;
use crate::governor::SubstrateGovernor;
use arc_swap::ArcSwap;
use std::sync::{Arc, Mutex};

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
    /// `store(Arc::new(new_policy))`.
    policy: Arc<ArcSwap<GovernorPolicy>>,

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
}

impl LocalSubstrateGovernor {
    /// Construct with an initial policy. The governor starts ready to
    /// serve `current_policy()` immediately. `set_candidates` +
    /// `on_hardware_detected` can rewrite later.
    pub fn new(initial_policy: GovernorPolicy) -> Self {
        Self {
            policy: Arc::new(ArcSwap::from(Arc::new(initial_policy))),
            candidates: Mutex::new(Vec::new()),
            snapshot_state: Mutex::new(SnapshotState {
                cascade_transition_count: 0,
                recent_signals: Vec::with_capacity(RECENT_SIGNALS_CAPACITY),
            }),
        }
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
        let new_policy = crate::governor::into_governor_policy(selected.clone(), hw, now_unix_ms());
        drop(candidates); // release before publish to keep mutex hold time tiny
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
        let mut state = self
            .snapshot_state
            .lock()
            .expect("LocalSubstrateGovernor snapshot mutex poisoned");
        if state.recent_signals.len() >= RECENT_SIGNALS_CAPACITY {
            // Drop oldest (front). With a Vec this is O(N) but N=32
            // so cost is trivial; using VecDeque would shave a few
            // ns but adds an enum-discriminant cost to every read.
            state.recent_signals.remove(0);
        }
        state.recent_signals.push(signal);
        // PR-3c will conditionally bump cascade_transition_count here
        // when a signal crosses a threshold. PR-3b just records.
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

/// Unix-ms timestamp. Used as the `committed_at_ms` on every
/// published policy. Pure infra helper.
fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .expect("system clock before UNIX_EPOCH")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governor::policy_file::{
        CadenceMultipliersFile, ConcurrencyCapsFile, ConsolidationFileSection,
        FederationCadenceFile, PolicyFile, RecallScoreWeightsFile, SpeculationFileSection,
        TierSizesFile,
    };
    use crate::governor::types::{
        CadenceMultipliers, ConcurrencyCaps, ConsolidationSchedule, FederationCadence,
        HardwareClass, PowerSource, RecallScoreWeights, SpeculationLevel, TargetSilicon,
        ThermalClass, ThermalSeverity, TierSizes,
    };

    fn hw(
        silicon: TargetSilicon,
        thermal: ThermalClass,
        vram_mb: u64,
        ram_mb: u64,
    ) -> HardwareClass {
        HardwareClass {
            silicon,
            silicon_model: "test".into(),
            vram_mb,
            system_ram_mb: ram_mb,
            power_source: PowerSource::Plugged,
            thermal_class: thermal,
            battery_pct: None,
            thermal_headroom_pct: None,
        }
    }

    fn pol(applies_to: &str, l1_lora_layers: u32) -> PolicyFile {
        PolicyFile {
            policy_version: 1,
            applies_to: applies_to.into(),
            tier_sizes: TierSizesFile {
                l1_lora_layers,
                l1_kv_tokens: 2048,
                l2_lora_layers: 4,
                l3_lora_layers: 12,
                l3_engrams: 1024,
            },
            cadence_multipliers: CadenceMultipliersFile {
                realtime: 1.0,
                delayed: 1.0,
                background: 1.0,
            },
            concurrency_caps: ConcurrencyCapsFile {
                personas_concurrent: 1,
                inference_lanes: 1,
                foundry_lanes: 0,
                sentinel_lanes: 1,
            },
            speculation: SpeculationFileSection {
                level: SpeculationLevel::Conservative,
            },
            consolidation: ConsolidationFileSection {
                schedule: ConsolidationSchedule::Manual,
            },
            federation: FederationCadenceFile {
                pull_cadence_seconds: 600,
            },
            recall_weights: RecallScoreWeightsFile {
                semantic: 0.4,
                outcome_history: 0.3,
                recency: 0.1,
                tier_proximity: 0.1,
                provenance_trust: 0.1,
            },
        }
    }

    fn initial_policy() -> GovernorPolicy {
        GovernorPolicy {
            policy_version: 0,
            hardware_class: hw(TargetSilicon::None, ThermalClass::Workstation, 0, 0),
            tier_sizes: TierSizes {
                l1_lora_layers: 1,
                l1_kv_tokens: 256,
                l2_lora_layers: 1,
                l3_lora_layers: 1,
                l3_engrams: 1,
            },
            cadence_multipliers: CadenceMultipliers {
                realtime: 1.0,
                delayed: 1.0,
                background: 1.0,
            },
            concurrency_caps: ConcurrencyCaps {
                personas_concurrent: 1,
                inference_lanes: 1,
                foundry_lanes: 0,
                sentinel_lanes: 1,
            },
            speculation_aggressiveness: SpeculationLevel::Off,
            consolidation_schedule: ConsolidationSchedule::Manual,
            federation_pull_cadence: FederationCadence {
                pull_cadence_seconds: 0,
            },
            recall_score_weights: RecallScoreWeights {
                semantic: 0.4,
                outcome_history: 0.3,
                recency: 0.1,
                tier_proximity: 0.1,
                provenance_trust: 0.1,
            },
            cascade_step: 0,
            committed_at_ms: 0,
        }
    }

    // ===== construction =====

    /// What this catches: new() with an initial policy lets
    /// current_policy() return that policy immediately. Smoke test —
    /// governor is ready to serve reads from boot.
    #[test]
    fn new_serves_initial_policy_immediately() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        let p = g.current_policy();
        assert_eq!(p.policy_version, 0);
        assert_eq!(p.hardware_class.silicon, TargetSilicon::None);
    }

    /// What this catches: candidate_count starts at 0 + grows when
    /// set_candidates is called. Defensive — file-watcher (PR-3d) needs
    /// this introspection to verify it loaded files.
    #[test]
    fn candidate_count_reflects_set_candidates() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        assert_eq!(g.candidate_count(), 0);
        g.set_candidates(vec![pol("apple-m", 2), pol("nvidia", 4)]);
        assert_eq!(g.candidate_count(), 2);
        g.set_candidates(vec![]);
        assert_eq!(g.candidate_count(), 0);
    }

    // ===== on_hardware_detected =====

    /// What this catches: on_hardware_detected with a matching
    /// candidate publishes a new policy via arc_swap. The new policy
    /// reflects the matched candidate's tier_sizes (l1_lora_layers=2
    /// for M-Air pol).
    #[test]
    fn on_hardware_detected_publishes_matching_policy() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        g.set_candidates(vec![
            pol(
                "apple-m,thinandlight,uma,vram_mb=0..0,ram_mb=14000..18000",
                2,
            ),
            pol("nvidia,workstation,vram_mb=30000..36000", 8),
        ]);
        let m2_air = hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 16384);
        g.on_hardware_detected(m2_air.clone())
            .expect("matching M-Air policy should publish");
        let p = g.current_policy();
        assert_eq!(p.tier_sizes.l1_lora_layers, 2, "matched M-Air l1_lora=2");
        assert_eq!(p.hardware_class.silicon, TargetSilicon::AppleM);
    }

    /// What this catches: try_hardware_detected returns the typed
    /// error when no candidate matches. Caller path that wants the
    /// failure-mode info.
    #[test]
    fn try_hardware_detected_returns_no_matching_policy_err() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        g.set_candidates(vec![pol("nvidia,workstation,vram_mb=30000..36000", 8)]);
        let m2_air = hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 16384);
        let result = g.try_hardware_detected(m2_air);
        assert!(matches!(
            result,
            Err(PolicySelectionError::NoMatchingPolicy { .. })
        ));
    }

    /// What this catches: on_hardware_detected with NO matching
    /// candidate returns a typed error and leaves the previous policy
    /// IN PLACE. Defensive — a misconfigured policy dir shouldn't wipe
    /// out the governor's running state.
    #[test]
    fn on_hardware_detected_no_match_keeps_previous_policy() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        g.set_candidates(vec![pol("nvidia,workstation,vram_mb=30000..36000", 8)]);
        let m2_air = hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 16384);
        let result = g.on_hardware_detected(m2_air);
        assert!(matches!(
            result,
            Err(PolicySelectionError::NoMatchingPolicy { .. })
        ));
        // Policy should still be the initial one (version 0)
        assert_eq!(g.current_policy().policy_version, 0);
    }

    /// What this catches: on_hardware_detected with empty candidates
    /// returns a typed error and leaves the policy intact. First-boot
    /// before file watcher loads anything = explicit failure + governor
    /// still serves the last committed policy.
    #[test]
    fn on_hardware_detected_empty_candidates_returns_error() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        let m2_air = hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 16384);
        let result = g.on_hardware_detected(m2_air);
        assert!(matches!(
            result,
            Err(PolicySelectionError::NoMatchingPolicy { .. })
        ));
        assert_eq!(g.current_policy().policy_version, 0);
    }

    /// What this catches: successive on_hardware_detected calls
    /// successfully republish. Multiple hardware-change events should
    /// each result in a published policy if a match is found.
    #[test]
    fn successive_hardware_detected_publishes_multiple_times() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        g.set_candidates(vec![
            pol(
                "apple-m,thinandlight,uma,vram_mb=0..0,ram_mb=14000..18000",
                2,
            ),
            pol("nvidia,workstation,vram_mb=30000..36000", 8),
        ]);

        let m2_air = hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 16384);
        g.on_hardware_detected(m2_air)
            .expect("M-Air policy should publish");
        assert_eq!(g.current_policy().tier_sizes.l1_lora_layers, 2);

        let blackwell = hw(
            TargetSilicon::NvidiaCuda,
            ThermalClass::Workstation,
            32 * 1024,
            64 * 1024,
        );
        g.on_hardware_detected(blackwell)
            .expect("Blackwell policy should publish");
        assert_eq!(g.current_policy().tier_sizes.l1_lora_layers, 8);
    }

    // ===== on_pressure_signal =====

    /// What this catches: on_pressure_signal records the signal in
    /// snapshot.recent_signals. PR-3b doesn't react to thresholds yet
    /// (PR-3c does), but it must record.
    #[test]
    fn on_pressure_signal_records_signal() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        g.on_pressure_signal(PressureSignal::Thermal {
            severity: ThermalSeverity::Hot,
        });
        let snap = g.snapshot();
        assert_eq!(snap.recent_signals.len(), 1);
        assert!(matches!(
            snap.recent_signals[0],
            PressureSignal::Thermal {
                severity: ThermalSeverity::Hot
            }
        ));
    }

    /// What this catches: recent_signals ring eviction at capacity.
    /// Pushing CAPACITY+1 signals retains the most recent CAPACITY.
    #[test]
    fn recent_signals_capped_at_capacity() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        for i in 0..(RECENT_SIGNALS_CAPACITY + 5) {
            g.on_pressure_signal(PressureSignal::InferenceQueueDepth { depth: i as u32 });
        }
        let snap = g.snapshot();
        assert_eq!(snap.recent_signals.len(), RECENT_SIGNALS_CAPACITY);
        // The OLDEST 5 (depth 0..4) should have been evicted; depth 5..36
        // should remain.
        match snap.recent_signals[0] {
            PressureSignal::InferenceQueueDepth { depth } => {
                assert_eq!(depth, 5, "front should be depth=5 after 5 evictions");
            }
            other => panic!("expected InferenceQueueDepth, got {other:?}"),
        }
    }

    // ===== snapshot =====

    /// What this catches: snapshot returns the current policy + the
    /// transition count + recent_signals. Telemetry consumer reads
    /// this for VDD reports.
    #[test]
    fn snapshot_includes_policy_and_signals() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        g.set_candidates(vec![pol(
            "apple-m,thinandlight,uma,vram_mb=0..0,ram_mb=14000..18000",
            2,
        )]);
        g.on_hardware_detected(hw(
            TargetSilicon::AppleM,
            ThermalClass::ThinAndLight,
            0,
            16384,
        ))
        .expect("M-Air policy should publish");
        g.on_pressure_signal(PressureSignal::Thermal {
            severity: ThermalSeverity::Warm,
        });

        let snap = g.snapshot();
        assert_eq!(snap.current_policy.tier_sizes.l1_lora_layers, 2);
        assert_eq!(
            snap.cascade_transition_count, 1,
            "1 publish from on_hardware_detected"
        );
        assert_eq!(snap.recent_signals.len(), 1);
    }

    /// What this catches: cascade_transition_count starts at 0 +
    /// increments per publish. Verifies the bump in publish().
    #[test]
    fn cascade_transition_count_increments_per_publish() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        g.set_candidates(vec![
            pol(
                "apple-m,thinandlight,uma,vram_mb=0..0,ram_mb=14000..18000",
                2,
            ),
            pol("nvidia,workstation,vram_mb=30000..36000", 8),
        ]);
        assert_eq!(g.snapshot().cascade_transition_count, 0);

        g.on_hardware_detected(hw(
            TargetSilicon::AppleM,
            ThermalClass::ThinAndLight,
            0,
            16384,
        ))
        .expect("M-Air policy should publish");
        assert_eq!(g.snapshot().cascade_transition_count, 1);

        g.on_hardware_detected(hw(
            TargetSilicon::NvidiaCuda,
            ThermalClass::Workstation,
            32 * 1024,
            64 * 1024,
        ))
        .expect("Blackwell policy should publish");
        assert_eq!(g.snapshot().cascade_transition_count, 2);
    }

    /// What this catches: cascade_transition_count does NOT increment
    /// when on_hardware_detected fails to find a match (policy unchanged
    /// = no publish = no transition). Important — operators should see
    /// 0 if their files don't match anything, not a phantom count.
    #[test]
    fn cascade_transition_count_unchanged_on_no_match() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        g.set_candidates(vec![pol("nvidia,workstation,vram_mb=30000..36000", 8)]);
        let result = g.on_hardware_detected(hw(
            TargetSilicon::AppleM,
            ThermalClass::ThinAndLight,
            0,
            16384,
        ));
        assert!(matches!(
            result,
            Err(PolicySelectionError::NoMatchingPolicy { .. })
        ));
        assert_eq!(g.snapshot().cascade_transition_count, 0);
    }

    /// What this catches: on_pressure_signal does NOT increment
    /// cascade_transition_count in PR-3b (signal-recording only; PR-3c
    /// adds the threshold-crossing → transition logic). Pinned so PR-3c
    /// has to land + update this test together.
    #[test]
    fn pressure_signal_does_not_transition_in_pr3b() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        g.on_pressure_signal(PressureSignal::Thermal {
            severity: ThermalSeverity::Critical,
        });
        assert_eq!(
            g.snapshot().cascade_transition_count,
            0,
            "PR-3b: signal recording only; PR-3c adds threshold-driven transitions"
        );
    }

    // ===== concurrency =====

    /// What this catches: many concurrent reads return the current
    /// policy without blocking. Sanity check on the arc_swap wait-free
    /// claim — if this hangs or deadlocks, the design is wrong.
    #[test]
    fn many_concurrent_reads_dont_block() {
        let g = Arc::new(LocalSubstrateGovernor::new(initial_policy()));
        let mut handles = Vec::new();
        for _ in 0..16 {
            let g_clone = Arc::clone(&g);
            handles.push(std::thread::spawn(move || {
                for _ in 0..1000 {
                    let _ = g_clone.current_policy();
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
    }

    /// What this catches: a concurrent reader observes a CONSISTENT
    /// policy snapshot even while a writer is rewriting. arc_swap's
    /// load_full() returns an Arc — the reader holds a stable snapshot
    /// even if a new policy lands a nanosecond later. Test pins this
    /// guarantee.
    #[test]
    fn concurrent_read_during_write_sees_consistent_snapshot() {
        let g = Arc::new(LocalSubstrateGovernor::new(initial_policy()));
        g.set_candidates(vec![
            pol(
                "apple-m,thinandlight,uma,vram_mb=0..0,ram_mb=14000..18000",
                2,
            ),
            pol("nvidia,workstation,vram_mb=30000..36000", 8),
        ]);

        let g_writer = Arc::clone(&g);
        let writer = std::thread::spawn(move || {
            for i in 0..100 {
                let h = if i % 2 == 0 {
                    hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 16384)
                } else {
                    hw(
                        TargetSilicon::NvidiaCuda,
                        ThermalClass::Workstation,
                        32 * 1024,
                        64 * 1024,
                    )
                };
                g_writer
                    .on_hardware_detected(h)
                    .expect("test candidates should match alternating hardware");
            }
        });

        let g_reader = Arc::clone(&g);
        let reader = std::thread::spawn(move || {
            for _ in 0..500 {
                let p = g_reader.current_policy();
                // Either the initial policy OR an air policy OR a blackwell
                // policy; never garbage. The Arc holds a complete snapshot.
                let l1 = p.tier_sizes.l1_lora_layers;
                assert!(
                    l1 == 1 || l1 == 2 || l1 == 8,
                    "unexpected l1_lora_layers={l1} — torn read of policy?"
                );
            }
        });

        writer.join().unwrap();
        reader.join().unwrap();
    }

    /// What this catches: current_policy() returns the SAME Arc on
    /// back-to-back calls when no write happened. arc_swap.load_full
    /// returns a clone of the same Arc, so two reads share the same
    /// allocation pointer.
    #[test]
    fn current_policy_returns_same_arc_when_no_writes() {
        let g = LocalSubstrateGovernor::new(initial_policy());
        let a = g.current_policy();
        let b = g.current_policy();
        assert!(
            Arc::ptr_eq(&a, &b),
            "expected same Arc pointer on back-to-back reads"
        );
    }
}
