//! Pool discovery: which `/props` verdicts may latch the directory (moved with the discovery from
//! the OpenAI adapter).

use super::*;
use crate::inference::slots::discover::props_status_proves_endpoint_absent;

// what this catches: the M5 2026-11-04 14:56Z pool-growth refusal. The pool accessor answered an
// `Option`, so "never probed" (`Engine::Unprobed`) and "probed, no paging" (`Engine::Unsupported`)
// both read `None`, and the adopted checkpoint refused "requires a known pool" instead of
// discovering it. Each engine state answers its own `Probed` variant, owned launches included.
#[tokio::test]
async fn each_engine_state_answers_its_own_probed_variant() -> Result<(), String> {
    let dir = SlotDirectory { endpoints: dashmap::DashMap::new() };
    assert!(matches!(dir.get("unprobed"), Probed::NotAsked), "never probed asks, never reads as absent");
    dir.latch_unsupported("latched");
    assert!(matches!(dir.get("latched"), Probed::Absent(PropsAbsent)), "latched unsupported refuses with its reason");
    let adopted = dir.ensure_pool("adopted", 2);
    assert!(matches!(dir.get("adopted"), Probed::Known(pool) if Arc::ptr_eq(&pool, &adopted)), "an adopted pool is known");

    // An owned launch keeps what was probed before it until it verifies ready.
    for (root, before) in [("unprobed", "not asked"), ("latched", "absent")] {
        let endpoint = dir.endpoint(root);
        let transition = endpoint.transition().await;
        let generation = transition.start_generation()?;
        match (dir.get(root), before) {
            (Probed::NotAsked, "not asked") | (Probed::Absent(PropsAbsent), "absent") => {}
            (_, before) => return Err(format!("an unready launch over a {before} pool changed its answer")),
        }
        let contract = KvPageContract {
            model_id: "fixture".into(),
            model: "fixture.gguf".into(),
            adapters: vec![],
            page_dir: None,
            context: 4096,
            slots: 2,
            cache_type: None,
            flash_attn: None,
            revisions: None,
        };
        transition.ready(root, &generation, contract)?;
        assert!(matches!(dir.get(root), Probed::Known(pool) if pool.n_slots() == 2), "a verified install is known");
    }
    Ok(())
}

// what this catches: the boot-race latch that killed slot affinity on effectively
// EVERY boot (2026-08-21). Personas start deliberating while llama-server still
// answers 503_loading (measured ×93 in the same ledger); the old code latched
// Unsupported on ANY non-success status, so one probe racing the load window
// disabled pinning for the process's life and prefix-similarity slot theft took
// over (`cached: 0` mid-conversation). Only "no such endpoint" may be permanent.
#[test]
fn a_loading_lane_must_not_latch_slot_affinity_off() {
    use reqwest::StatusCode;
    for transient in [
        StatusCode::SERVICE_UNAVAILABLE, // llama-server mid-load — THE incident
        StatusCode::INTERNAL_SERVER_ERROR,
        StatusCode::BAD_GATEWAY,
        StatusCode::TOO_MANY_REQUESTS,
    ] {
        assert!(
            !props_status_proves_endpoint_absent(transient),
            "{transient} is a statement about NOW, not about what the server IS — \
             latching Unsupported on it re-opens the boot race"
        );
    }
    for absent in [StatusCode::NOT_FOUND, StatusCode::NOT_IMPLEMENTED] {
        assert!(
            props_status_proves_endpoint_absent(absent),
            "{absent} genuinely proves the surface is missing — without the latch \
             every cloud provider would be re-probed per persona request forever"
        );
    }
}
