#![cfg(test)]

use super::*;

// what this catches: an M5 Pro (or any capable silicon) must NOT classify
// as Compat (which would force n_gpu_layers=0 = CPU); unknown hardware
// stays on the safe LCD/CPU fallback.
#[test]
fn detect_tier_classifies_silicon() {
    use crate::persona::hw_tier_descriptor::HwTierCategory;
    assert_eq!(detect_tier("Apple M5 Pro").1, HwTierCategory::MSeriesPro);
    assert_eq!(detect_tier("Apple M2").1, HwTierCategory::MSeries);
    assert_eq!(
        detect_tier("NVIDIA GeForce RTX 5090").1,
        HwTierCategory::Cuda
    );
    assert_eq!(detect_tier("llvmpipe").1, HwTierCategory::Compat);
}

/// A minimal [`Model`] for reconcile-wiring tests — only `id` is load-bearing
/// (the FakeServer ignores the rest); the planned served window rides on the
/// `ServingTarget`, not here.
// what this catches (card af635057, the 5090 after a reboot): an embedding or
// reranking row — Ready, with a real GGUF — chosen as the base model. A mind needs
// text generation; a row without it is never a candidate, pinned or not.
#[test]
fn a_model_that_cannot_speak_is_never_a_base_candidate() {
    use crate::model_registry::types::Capability;
    let mut speaks = fake_model("chat/model");
    speaks.capabilities = [Capability::Chat, Capability::TextGeneration]
        .into_iter()
        .collect();
    assert!(can_serve_minds(&speaks));
    let mut embeds = fake_model("embed/model");
    embeds.capabilities = [Capability::Embedding].into_iter().collect();
    assert!(
        !can_serve_minds(&embeds),
        "an embedding-only row cannot host a mind"
    );
    let mut mute = fake_model("mute/model");
    mute.capabilities.clear();
    assert!(
        !can_serve_minds(&mute),
        "no capabilities = no candidate, never a guess"
    );
}

// what this catches: THE slice-6 no-reboot servability contract. A model that
// is NotDownloaded is NOT offered as a serving candidate; the instant a pull
// flips it Ready with a real on-disk path (`attach_local_artifact`), the SAME
// live snapshot yields it as a candidate. Proves serving plans off the LIVE
// catalog, not the immutable seed — a runtime acquisition becomes servable on
// the next tick with no restart. Regresses the convergence gap where serving
// re-derived candidates from `model_registry::global()` and never saw a pull.
#[test]
fn pulled_model_becomes_a_candidate_without_reboot() {
    use crate::model_registry::live::ModelStatus;
    use std::io::Write;

    let catalog = test_catalog();
    let id = "slice6-convergence-probe";
    // A fresh local model with no artifact on disk → NotDownloaded.
    catalog.register(
        fake_model(id),
        ModelStatus {
            availability: Availability::NotDownloaded,
            verified: None,
        },
    );

    // Before the pull: not on disk ⇒ never offered for serving.
    let before = candidates_from_snapshot(&catalog.snapshot());
    assert!(
        !before.iter().any(|f| f.model_id == id),
        "a NotDownloaded model must never be a serving candidate"
    );

    // The pull lands the artifact: a real, non-empty GGUF on disk.
    let mut gguf = tempfile::Builder::new()
        .suffix(".gguf")
        .tempfile()
        .expect("temp gguf");
    gguf.write_all(&[0u8; 4096]).expect("write weights");
    gguf.flush().expect("flush");
    assert!(
        catalog.attach_local_artifact(id, gguf.path().to_path_buf(), None),
        "model is present, artifact attaches + flips Ready"
    );

    // After the pull: the SAME live snapshot now yields it as a candidate,
    // with the footprint reflecting the real on-disk bytes — no reboot.
    let after = candidates_from_snapshot(&catalog.snapshot());
    let found = after
        .iter()
        .find(|f| f.model_id == id)
        .expect("a freshly-pulled Ready model becomes a serving candidate on the next snapshot");
    assert_eq!(
        found.weights_bytes, 4096,
        "footprint counts the real bytes that will be loaded"
    );
}

// what this catches: the VRAM-axis deallocation. `serving/unload` inserts a
// model id into the daemon's suppress-set; `live_candidates` must then EXCLUDE
// that model even though it is Ready on disk, so the planner can no longer pick
// it and the next reconcile frees its lane. `serving/load` (un-suppress)
// restores it as a candidate. Without this filter an unloaded model would be
// re-planned on the very next tick and its VRAM would never free — the
// imperative-suppress contract the operator chose.
#[tokio::test]
async fn live_candidates_honors_the_suppress_set() {
    use crate::model_registry::live::ModelStatus;
    use std::io::Write;

    let serves = Arc::new(AtomicUsize::new(0));
    let daemon = daemon_with(Arc::new(FakeServer::healthy(serves, true)));
    let id = "suppress-probe";

    // Land a Ready model so it IS a candidate to begin with.
    let mut gguf = tempfile::Builder::new()
        .suffix(".gguf")
        .tempfile()
        .expect("temp gguf");
    gguf.write_all(&[0u8; 4096]).expect("write weights");
    gguf.flush().expect("flush");
    daemon.catalog.register(
        fake_model(id),
        ModelStatus {
            availability: Availability::NotDownloaded,
            verified: None,
        },
    );
    assert!(
        daemon
            .catalog
            .attach_local_artifact(id, gguf.path().to_path_buf(), None),
        "ready model lands on disk"
    );
    assert!(
        daemon.live_candidates().iter().any(|f| f.model_id == id),
        "a Ready model is a candidate before unload"
    );

    // serving/unload: pin it OFF → excluded from candidates → lane frees.
    daemon.intent.set_suppressed(id, true, true);
    assert!(
        !daemon.live_candidates().iter().any(|f| f.model_id == id),
        "a suppressed model is excluded → planner drops it → VRAM frees"
    );

    // serving/load: permit it again → returns as a candidate (planner decides).
    daemon.intent.set_suppressed(id, false, true);
    assert!(
        daemon.live_candidates().iter().any(|f| f.model_id == id),
        "an un-suppressed model returns as a candidate"
    );
}

// what this catches: #142 — the autonomic planner conscripting a benchmark
// OPPONENT as the citizens' model. A Ready GGUF whose catalog row opts out
// (`persona_serving_eligible: false`) must be invisible to the autonomic
// plan (the tick picked Hermes-4.3 twice, 2026-07-12, purely because it was
// the largest Ready artifact), while an explicit `serving/pin` — operator
// consent — still serves it (how the benchmark matrix brings one up).
#[tokio::test]
async fn ineligible_rows_never_join_the_autonomic_plan_but_pin_bypasses() {
    use crate::model_registry::live::ModelStatus;
    use std::io::Write;

    let serves = Arc::new(AtomicUsize::new(0));
    let daemon = daemon_with(Arc::new(FakeServer::healthy(serves, true)));
    let id = "opponent-probe";

    let mut opponent = fake_model(id);
    opponent.persona_serving_eligible = false;
    let mut gguf = tempfile::Builder::new()
        .suffix(".gguf")
        .tempfile()
        .expect("temp gguf");
    gguf.write_all(&[0u8; 4096]).expect("write weights");
    gguf.flush().expect("flush");
    daemon.catalog.register(
        opponent,
        ModelStatus {
            availability: Availability::NotDownloaded,
            verified: None,
        },
    );
    assert!(
        daemon
            .catalog
            .attach_local_artifact(id, gguf.path().to_path_buf(), None),
        "opponent GGUF lands on disk and flips Ready"
    );

    // Ready on disk, but ineligible → the autonomic path must never see it.
    assert!(
        !daemon.live_candidates().iter().any(|f| f.model_id == id),
        "an ineligible Ready model is excluded from the autonomic plan"
    );

    // Explicit pin = operator consent → eligibility is bypassed.
    daemon.intent.set_pin(Some(id.to_string()), true);
    assert!(
        daemon.live_candidates().iter().any(|f| f.model_id == id),
        "a pinned ineligible model serves — pin is consent"
    );

    // Unpin → back off the autonomic plan.
    daemon.intent.set_pin(None, true);
    assert!(
        !daemon.live_candidates().iter().any(|f| f.model_id == id),
        "unpin returns the opponent to benchmark-only invisibility"
    );
}

// what this catches: a node saying "no servable model on disk" while its weights sit
// there (the 5090, 2026-09-08: zero citizens for an hour, candidates: 0, and the plan
// named neither the models it refused nor why). Every refusal has a reason and the
// reason reaches the reader.
#[test]
fn every_refused_candidate_is_named_with_its_reason() {
    use std::collections::HashSet;
    let snap = test_catalog().snapshot();
    // ELIGIBLE too: the three opt-out rows (an embedding model, the benchmark
    // opponent, Devstral) are refused for a different reason and would make the last
    // assertion here read as a bug in the filter rather than as the catalog's intent.
    let any = snap
        .models
        .values()
        .find(|l| {
            l.status.availability == Availability::Ready
                && can_serve_minds(&l.model)
                && l.model.persona_serving_eligible
                && footprint_for(&l.model).is_some()
        })
        .map(|l| l.model.id.clone());
    let Some(model_id) = any else {
        return; // a catalog with nothing ready has nothing to refuse
    };
    // Suppressed by an unload.
    let mut sup = HashSet::new();
    sup.insert(model_id.clone());
    let (kept, refused) = servable_candidates_with_refusals(&snap, &sup, &None);
    assert!(!kept.iter().any(|c| c.model_id == model_id));
    assert!(
        refused
            .iter()
            .any(|r| r.model_id == model_id && r.reason == REFUSED_SUPPRESSED),
        "a suppressed model names the unload: {refused:?}"
    );
    // Another model pinned.
    let (kept, refused) = servable_candidates_with_refusals(
        &snap,
        &HashSet::new(),
        &Some("not-a-real-model".to_string()),
    );
    assert!(
        kept.is_empty(),
        "nothing matches a pin for a model that is not here"
    );
    assert!(
        refused.iter().all(|r| r.reason == REFUSED_NOT_PINNED) && !refused.is_empty(),
        "every refusal names the pin: {refused:?}"
    );
    // Nothing suppressed, nothing pinned: eligible rows survive.
    let (kept, _) = servable_candidates_with_refusals(&snap, &HashSet::new(), &None);
    assert!(kept.iter().any(|c| c.model_id == model_id));
}
