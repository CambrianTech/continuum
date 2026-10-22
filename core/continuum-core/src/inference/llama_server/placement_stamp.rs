//! A kept lane records where it lives from its engine's own account (card 22aa3379).
//!
//! A deploy keeps healthy engines (`stop_with_authority(keep_lanes = true)`), so a lane launched
//! before lane records carried a placement keeps `placement: None` until it happens to relaunch,
//! and training withholds a lane whose pool it cannot name (#5264). The engine reports where its
//! weights live on `/props` (`model_weight_buffers`, [`WeightResidency::observed_placement`]). When
//! the serving reconcile adopts a lane that is already serving, that report is written onto the
//! lane's record, an observed runtime fact and never configuration, so no relaunch is needed.

use crate::inference::engine_residency::EngineIncarnation;
use crate::inference::lane_registry::LaneRecord;
use crate::inference::llama_server::LanePlacement;

/// The record to write, or `None` to write nothing. Every precondition is checked against the
/// lane's record as read BEFORE the `/props` read and AGAIN after it, so a lane replaced in
/// between (another pid, start time or port at that port) is never stamped with its
/// predecessor's placement:
/// - the record had no placement (a launch-stamped one is never overwritten);
/// - the live process at its pid is the same incarnation the record names;
/// - the engine's account decided a placement;
/// - the record read afterwards is the same incarnation, still without a placement.
pub(crate) fn placement_to_record(
    before: Option<&LaneRecord>,
    live: Option<EngineIncarnation>,
    observed: Option<LanePlacement>,
    after: Option<&LaneRecord>,
) -> Option<LaneRecord> {
    let before = before?;
    if before.placement.is_some() || live != Some(before.incarnation()) {
        return None;
    }
    let observed = observed?;
    let after = after?;
    if after.incarnation() != before.incarnation() || after.placement.is_some() {
        return None;
    }
    Some(LaneRecord { placement: Some(observed), ..after.clone() })
}

impl super::LlamaServerProcess {
    /// See the module doc. Cheap once stamped: the record is read first, and an engine whose
    /// record already names its placement is not asked anything.
    pub(super) async fn stamp_observed_placement(&self) {
        let (_host, port) = super::split_host_port(&self.root);
        let at_port = || crate::inference::lane_registry::records().into_iter().find(|rec| rec.port == port);
        let Some(before) = at_port() else {
            return;
        };
        if before.placement.is_some() {
            return;
        }
        let live = EngineIncarnation::of(before.pid, port);
        let observed = match self.weight_residency().await {
            Ok(Some(residency)) => residency.observed_placement(),
            Ok(None) | Err(_) => None,
        };
        let Some(record) = placement_to_record(Some(&before), live, observed, at_port().as_ref()) else {
            continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.placement.unstamped",
                port = port as u64,
                observed = ?observed,
                "a kept lane's placement was not recorded: its engine's account decided nothing, or the lane changed during the read"
            );
            return;
        };
        let placement = record.placement;
        if let Err(e) = crate::inference::lane_registry::record(&record) {
            continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.placement.unrecorded",
                port = port as u64,
                error = %e,
                "a kept lane's observed placement could not be written to its record"
            );
            return;
        }
        // The pool its bytes are in, against the pool serving plans this host's lanes in: a
        // difference means serving's memory accounting and the engine disagree, said loudly.
        let configured = super::main_lane_placement();
        continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.placement.observed",
            port = port as u64,
            placement = ?placement,
            configured = ?configured,
            agrees = placement == Some(configured),
            "a kept lane's placement, recorded from its engine's own /props"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(pid: u32, started_s: u64, placement: Option<LanePlacement>) -> LaneRecord {
        LaneRecord {
            pid,
            port: 58057,
            role: crate::inference::lane_registry::LaneRole::Live,
            model: "continuum-ai/qwen2.5-coder-1.5b-instruct-GGUF".into(),
            context_window: 32768,
            lanes: 2,
            page_dir: None,
            engine_bin: None,
            started_s,
            placement,
        }
    }

    // what this catches (Codex on 22aa3379): a placement written onto the wrong lane or over a
    // launch record. The kept lane is stamped with what its engine reported; a lane replaced
    // during the read (another incarnation after), a process that is not the recorded one, an
    // already-recorded placement, and an engine that decided nothing are all left alone.
    #[test]
    fn a_kept_lane_is_stamped_only_when_the_same_engine_reported_it() {
        let kept = rec(52415, 1_792_640_079, None);
        let live = Some(kept.incarnation());
        let stamped = placement_to_record(Some(&kept), live, Some(LanePlacement::Cpu), Some(&kept));
        assert_eq!(stamped.and_then(|r| r.placement), Some(LanePlacement::Cpu));

        let replaced = rec(60001, 1_792_650_000, None);
        assert!(placement_to_record(Some(&kept), live, Some(LanePlacement::Cpu), Some(&replaced)).is_none(), "replaced during the read");
        let other_process = Some(replaced.incarnation());
        assert!(placement_to_record(Some(&kept), other_process, Some(LanePlacement::Cpu), Some(&kept)).is_none(), "not the recorded process");
        let launched = rec(52415, 1_792_640_079, Some(LanePlacement::Gpu));
        assert!(placement_to_record(Some(&launched), Some(launched.incarnation()), Some(LanePlacement::Cpu), Some(&launched)).is_none(), "a launch record is never overwritten");
        assert!(placement_to_record(Some(&kept), live, None, Some(&kept)).is_none(), "the engine decided nothing");
        assert!(placement_to_record(Some(&kept), live, Some(LanePlacement::Cpu), None).is_none(), "the record is gone");
    }
}
