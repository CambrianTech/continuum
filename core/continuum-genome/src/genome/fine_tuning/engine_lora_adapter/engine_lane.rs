//! Lanes of our own engine stood up for one training run, for a base no live lane here serves.
//!
//! The resident lane trains the base it serves ([`ResidentLane`]). A citizen's examples can be
//! for another base: on IntelMac (2026-10-22) Delia's due bucket was qwen2.5-coder-0.5b while the
//! live lane served 1.5b, and an Intel Mac has neither MLX nor CUDA, so no trainer advertised the
//! base and the dispatch was retryable forever (7a68a4c6). This source stands up a single-purpose
//! lane for that base the way a gene eval does (`inference::single_purpose_lane`: memory headroom,
//! window, GPU-first placement under a governed lease) and trains in it with the same `POST /train`
//! as the resident lane. The lane's choice is read from its own lane record (`lane_choice`), so its
//! pool is the placement it was launched with. The lane lives as long as the run holds the lease.

use super::*;

/// Where training lanes scan for a free port: their own range, distinct from eval lanes (58_200)
/// and the vision sidecar (58_091). `first_free_port` scans upward from here.
const TRAIN_LANE_BASE_PORT: u16 = 58_300;

/// A training lane holds its governed slot for the life of its run, like every training lease
/// (`inference::training_admission::request`): pinned, and released when the lane's process exits.
const TRAIN_LANE_TERMS: crate::inference::single_purpose_lane::LaneLeaseTerms =
    crate::inference::single_purpose_lane::LaneLeaseTerms { consumer: "train-lane", ttl_ms: u64::MAX };

/// Lanes stood up for a run ([`TrainingPlace::EngineLaneHere`]).
pub(super) struct EngineLane;

/// Whether to stand up a lane for `base`: its weights are here, and no live lane serves it. A
/// live lane that serves the base declines it whatever its placement record says, so a base
/// already resident is never loaded a second time beside itself (the resident lane trains it, or
/// it waits for that lane's next launch to record a placement).
pub(super) fn stands_up_for(
    base: &str,
    live: Option<&crate::inference::lane_registry::LaneRecord>,
    weights_here: bool,
) -> bool {
    weights_here && !live.is_some_and(|rec| crate::model_registry::artifacts::names_the_same_base(&rec.model, base))
}

/// Refuse to stand up a lane for `base` once a live lane serves it: the resident trainer takes
/// it, so the dispatch retries (`Transient`) and selection picks the resident lane, rather than
/// loading a second copy beside it.
pub(super) fn refuse_if_resident(
    base: &str,
    live: Option<&crate::inference::lane_registry::LaneRecord>,
) -> Result<(), FineTuningError> {
    if stands_up_for(base, live, true) {
        return Ok(());
    }
    Err(FineTuningError::Transient(format!(
        "a live lane now serves {base}: its resident trainer takes the run, not a second copy"
    )))
}

fn model_for(base: &str) -> Option<crate::model_registry::Model> {
    crate::model_registry::try_global().and_then(|registry| registry.model(base).cloned())
}

#[async_trait]
impl LaneSource for EngineLane {
    fn serves(&self, base: &str) -> bool {
        let weights_here = model_for(base)
            .as_ref()
            .is_some_and(|model| crate::model_registry::artifacts::resolve_gguf_for_model(model).is_some());
        stands_up_for(base, crate::inference::lane_registry::live_lane().as_ref(), weights_here)
    }

    async fn acquire(&self, base: &str) -> Result<LaneLease, FineTuningError> {
        use crate::inference::single_purpose_lane as lane;
        use crate::inference::llama_server::{EphemeralServingLane, ServingTarget};
        use FineTuningError::Transient;
        let model = model_for(base)
            .ok_or_else(|| FineTuningError::InvalidRequest(format!("no model {base} in the registry to train on")))?;
        lane::await_lane_memory_headroom(lane::lane_footprint(&model)).await.map_err(Transient)?;
        // The headroom wait can be long: a lane for this base may have come up since selection.
        refuse_if_resident(base, crate::inference::lane_registry::live_lane().as_ref())?;
        let window = lane::plan_lane_ctx(&model);
        let (evidence, lease) = lane::decide_lane_placement(&model, window, &format!("train-lane {base}"), TRAIN_LANE_TERMS);
        let target = ServingTarget {
            // A single-purpose lane: no citizen population to size a prompt cache from.
            host_prompt_cache_mib: crate::inference::lane_args::CACHE_RAM_MIB,
            model: model.clone(),
            context_window: window,
            lanes: 1,
            adapters: Vec::new(),
            placement: evidence.placement,
            expert_placement: None,
            resident_override: None,
            vision_sidecar: false,
        };
        let spawned = EphemeralServingLane::spawn_with_reservation(&target, TRAIN_LANE_BASE_PORT, lease)
            .await
            .map_err(|e| Transient(format!("could not stand up a training lane for {base}: {e}")))?;
        let port = spawned.port();
        let record = lane_record_at(port)
            .ok_or_else(|| Transient(format!("the training lane for {base} on port {port} registered no lane record")))?;
        let mut choice = lane_choice(record, base)
            .ok_or_else(|| Transient(format!("the training lane on port {port} does not record {base} with a placement")))?;
        // The window the lane serves, read back from it, never the planned one.
        choice.window = spawned
            .served_context_window()
            .await
            .map_err(|e| Transient(format!("the training lane for {base} is up but its served window is unreadable: {e}")))?;
        continuum_base::probe!(
            class = "training.lane.stood_up",
            base = base,
            port = port as u64,
            window = choice.window as u64,
            placement = %format!("{:?}", evidence.placement),
            reason = %evidence.reason,
            "a lane stood up for one training run, for a base no live lane here serves"
        );
        let (owned_base, chosen, url) = (base.to_owned(), choice.engine, choice.url.clone());
        Ok(LaneLease {
            choice,
            // Behind a mutex so the hold is Sync whatever the process handle is; nothing locks it.
            hold: Some(Box::new(std::sync::Mutex::new(spawned))),
            still_bound: Box::new(move || still_bound_to(lane_record_at(port), &owned_base, chosen, &url)),
        })
    }

    fn provider(&self) -> &'static str {
        LANE_PROVIDER_ID
    }

    fn place(&self) -> crate::genome::fine_tuning::adapter::TrainingPlace {
        crate::genome::fine_tuning::adapter::TrainingPlace::EngineLaneHere
    }
}

/// The lane record on `port`: the lane's own account of what it serves and where it keeps it.
fn lane_record_at(port: u16) -> Option<crate::inference::lane_registry::LaneRecord> {
    crate::inference::lane_registry::records().into_iter().find(|rec| rec.port == port)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn live(model: &str) -> crate::inference::lane_registry::LaneRecord {
        crate::inference::lane_registry::LaneRecord {
            pid: 4242,
            port: 58057,
            role: crate::inference::lane_registry::LaneRole::Live,
            model: model.into(),
            context_window: 32768,
            lanes: 2,
            page_dir: None,
            engine_bin: None,
            weights: None,
            started_s: 1_700_000_000,
            placement: None,
        }
    }

    // what this catches (card d67fd801): a lane stood up for a base already resident (a second
    // copy of the same weights beside the live lane), or one stood up for weights that are not
    // here. IntelMac's case, live 1.5b and bucket 0.5b, stands one up; a live lane serving the
    // base declines it even with no placement recorded (#5264 then offers it to no one until it
    // relaunches, rather than loading it twice).
    #[test]
    fn a_lane_is_stood_up_only_for_local_weights_no_live_lane_serves() {
        let small = "continuum-ai/qwen2.5-coder-0.5b-instruct-GGUF";
        let resident = "continuum-ai/qwen2.5-coder-1.5b-instruct-GGUF";
        assert!(stands_up_for(small, Some(&live(resident)), true), "another base is resident");
        assert!(stands_up_for(small, None, true), "nothing is resident");
        assert!(!stands_up_for(resident, Some(&live(resident)), true), "the resident lane serves it");
        assert!(!stands_up_for(small, Some(&live(resident)), false), "no weights here");
    }

    // what this catches (Codex on #5269): a base that became resident between selection and the
    // spawn (the headroom wait can be long) loaded a second time. The recheck refuses it as
    // Transient, so the retried dispatch selects the resident trainer.
    #[test]
    fn a_base_that_became_resident_before_the_spawn_is_refused_as_transient() {
        let small = "continuum-ai/qwen2.5-coder-0.5b-instruct-GGUF";
        assert!(refuse_if_resident(small, None).is_ok());
        assert!(refuse_if_resident(small, Some(&live("continuum-ai/qwen2.5-coder-1.5b-instruct-GGUF"))).is_ok());
        assert!(matches!(refuse_if_resident(small, Some(&live(small))), Err(FineTuningError::Transient(_))));
    }
}
