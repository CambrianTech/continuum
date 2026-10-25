//! A kept lane records where it lives from its engine's own account (card 22aa3379).
//!
//! A deploy keeps healthy engines (`stop_with_authority(keep_lanes = true)`), so a lane launched
//! before lane records carried a placement keeps `placement: None` until it happens to relaunch,
//! and training withholds a lane whose pool it cannot name (#5264). The engine reports where its
//! weights live on `/props` (`model_weight_buffers`, [`WeightResidency::observed_placement`]). When
//! the serving reconcile adopts a lane that is already serving, that report is written onto the
//! lane's record, an observed runtime fact and never configuration, so no relaunch is needed.
//!
//! The record names the pool training leases beside the lane, so it must be the pool serving
//! charged the lane's bytes to. Serving registers its one consumer in `serving_pool_kind()`,
//! which is [`main_lane_placement`]`().pool_kind()` (modules/serving_daemon/consumer.rs), for a
//! kept lane as for a launched one. An observation that disagrees with that charge is not
//! written: training stays withheld and the disagreement is said loudly, never papered over.
//!
//! [`WeightResidency::observed_placement`]: crate::inference::weight_residency::WeightResidency::observed_placement
//! [`main_lane_placement`]: super::main_lane_placement

use crate::inference::engine_residency::EngineIncarnation;
use crate::inference::lane_registry::LaneRecord;
use crate::inference::llama_server::LanePlacement;

/// Why a kept lane's placement was not recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unstamped {
    /// The record already names a placement (a launch record is never overwritten).
    AlreadyRecorded,
    /// The process at the record's pid was not the recorded incarnation before or after the
    /// `/props` read (exited, replaced, or its start time unreadable): the answer may be
    /// another engine's.
    ProcessChanged,
    /// The engine's account decided nothing (no answer, no field, an unknown backend).
    Undecided,
    /// The engine's weights are not in the pool serving charged them to.
    DisagreesWithCharge { observed: LanePlacement, charged: LanePlacement },
}

/// The placement to record on `record`, from the live incarnation sampled before and after the
/// `/props` read, what the engine reported, and the placement serving charged the lane in.
/// Pure: every precondition in the module doc, nothing read here.
pub(crate) fn placement_to_stamp(
    record: &LaneRecord,
    live_before: Option<EngineIncarnation>,
    live_after: Option<EngineIncarnation>,
    observed: Option<LanePlacement>,
    charged: LanePlacement,
) -> Result<LanePlacement, Unstamped> {
    if record.placement.is_some() {
        return Err(Unstamped::AlreadyRecorded);
    }
    let recorded = Some(record.incarnation());
    if live_before != recorded || live_after != recorded {
        return Err(Unstamped::ProcessChanged);
    }
    let observed = observed.ok_or(Unstamped::Undecided)?;
    if observed != charged {
        return Err(Unstamped::DisagreesWithCharge { observed, charged });
    }
    Ok(observed)
}

impl super::LlamaServerProcess {
    /// See the module doc. Cheap once stamped: the record is read first, and an engine whose
    /// record already names its placement is not asked anything.
    pub(super) async fn stamp_observed_placement(&self) {
        let (_host, port) = super::split_host_port(&self.root);
        let Some(record) = crate::inference::lane_registry::records().into_iter().find(|rec| rec.port == port) else {
            return;
        };
        if record.placement.is_some() {
            return;
        }
        let live_before = EngineIncarnation::of(record.pid, port);
        let observed = match self.weight_residency().await {
            Ok(Some(residency)) => residency.observed_placement(),
            Ok(None) | Err(_) => None,
        };
        let live_after = EngineIncarnation::of(record.pid, port);
        let charged = super::main_lane_placement();
        let placement = match placement_to_stamp(&record, live_before, live_after, observed, charged) {
            Ok(placement) => placement,
            Err(why) => {
                continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.placement.unstamped",
                    port = port as u64,
                    pid = record.pid as u64,
                    why = ?why,
                    "a kept lane's placement was not recorded; training beside it stays withheld"
                );
                return;
            }
        };
        // The write itself is guarded: only onto the exact record read above, never a
        // replacement, and a removed record is never recreated.
        match crate::inference::lane_registry::restamp_placement(&record, placement) {
            Ok(true) => continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.placement.observed",
                port = port as u64,
                pid = record.pid as u64,
                placement = ?placement,
                "a kept lane's placement, recorded from its engine's own /props, in the pool serving charged it to"
            ),
            Ok(false) => continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.placement.unstamped",
                port = port as u64,
                pid = record.pid as u64,
                why = "record_changed",
                "a kept lane's record changed during the read; nothing written"
            ),
            Err(e) => continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.placement.unrecorded",
                port = port as u64,
                error = %e,
                "a kept lane's observed placement could not be written to its record"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(placement: Option<LanePlacement>) -> LaneRecord {
        LaneRecord {
            pid: 52415,
            port: 58057,
            role: crate::inference::lane_registry::LaneRole::Live,
            model: "continuum-ai/qwen2.5-coder-1.5b-instruct-GGUF".into(),
            context_window: 32768,
            lanes: 2,
            page_dir: None,
            engine_bin: None,
            weights: None,
            started_s: 1_792_640_079,
            placement,
        }
    }

    // what this catches (Codex on #5270): a placement recorded for the wrong engine or the
    // wrong pool. Stamped only when the recorded incarnation is live both before AND after
    // the read (an unchanged record proves nothing about the process: Codex's dead/replaced
    // case), the engine decided, and its pool is the one serving charged; a launch record
    // is never overwritten.
    #[test]
    fn a_kept_lane_is_stamped_only_by_its_own_engine_in_the_charged_pool() {
        use LanePlacement::{Cpu, Gpu};
        let kept = rec(None);
        let same = Some(kept.incarnation());
        assert_eq!(placement_to_stamp(&kept, same, same, Some(Cpu), Cpu), Ok(Cpu));

        let replacement = Some(EngineIncarnation { started_s: 1_792_650_000, ..kept.incarnation() });
        assert_eq!(placement_to_stamp(&kept, same, replacement, Some(Cpu), Cpu), Err(Unstamped::ProcessChanged), "replaced during the read");
        assert_eq!(placement_to_stamp(&kept, same, None, Some(Cpu), Cpu), Err(Unstamped::ProcessChanged), "exited during the read");
        assert_eq!(placement_to_stamp(&kept, replacement, replacement, Some(Cpu), Cpu), Err(Unstamped::ProcessChanged), "not the recorded process");
        assert_eq!(placement_to_stamp(&kept, same, same, None, Cpu), Err(Unstamped::Undecided));
        assert_eq!(
            placement_to_stamp(&kept, same, same, Some(Cpu), Gpu),
            Err(Unstamped::DisagreesWithCharge { observed: Cpu, charged: Gpu }),
            "a CPU lane on a host serving charges to VRAM keeps training withheld"
        );
        let launched = rec(Some(Gpu));
        assert_eq!(placement_to_stamp(&launched, same, same, Some(Gpu), Gpu), Err(Unstamped::AlreadyRecorded));
    }
}
