#![cfg(test)]
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

// what this catches: the M5 2026-11-14 11:21:51Z warm-restart race. The first capability probe
// published the adopted engine's pool and only then read its ledger (~20 ms); a second discovery
// found the pool known, skipped the adoption (`pool_already_installed`), and Aris's first turn
// admitted against a pool with no ledger: `cold_turn cause="no_page_dir"` while her page sat on
// disk, 20 ms before `ledger_adopted loaded=4` named her. A pool is published only settled, so an
// admission racing the adoption finds no pool yet (and discovers), or finds her page restorable.
#[tokio::test]
async fn an_admission_racing_the_ledger_adoption_restores_her_page() -> Result<(), Box<dyn std::error::Error>> {
    use crate::inference::slots::discover::{NotAdopted, SettledPageStore};
    let dir = Arc::new(SlotDirectory { endpoints: dashmap::DashMap::new() });
    let root = "test://adopted-mid-restart";
    let pages = tempfile::tempdir()?;
    let engine = (4242, 1_700_000_000);
    let aris = PageKey::mind(Uuid::new_v4()).ok_or("test: a mind key")?;
    // The last core confirmed her page under this engine process, then the deploy restarted the core.
    let last_core = KvSlotPool::new(root, 2);
    last_core.attach_ledger(SavedLedger::in_dir(pages.path(), "fp".into()), false);
    last_core.stamp_incarnation(engine);
    std::fs::write(pages.path().join(page_filename(&aris)), b"kv")?;
    last_core.note_saved(aris);
    drop(last_core);

    // The first probe's adoption is held mid-read until the racing admission has looked.
    let (reading, mid_read) = tokio::sync::oneshot::channel();
    let (looked, resume) = std::sync::mpsc::channel::<()>();
    let first_probe = tokio::task::spawn_blocking({
        let (dir, pages) = (Arc::clone(&dir), pages.path().to_path_buf());
        move || {
            dir.install_settled_pool(root, 2, |pool| {
                let _ = reading.send(());
                let _ = resume.recv(); // test: held until the admission has looked
                let (adoption, snapshot) =
                    crate::inference::slots::engine_generation::settle_engine_ledger(pool, &pages, engine.0, engine.1);
                SettledPageStore { adoption: adoption.map_err(NotAdopted::Refused), snapshot }
            })
        }
    });
    mid_read.await?;
    assert!(
        matches!(dir.get(root), Probed::NotAsked),
        "mid-adoption no pool is published: an admission discovers instead of admitting against no ledger"
    );
    looked.send(())?;
    let installed = first_probe.await?;
    let Probed::Known(pool) = dir.get(root) else { return Err("the settled pool was not published".into()) };
    assert!(Arc::ptr_eq(&pool, &installed));
    let slot = pool.lease(aris).await.ok_or("test: a free slot")?;
    assert_eq!(pool.page_on_disk(&aris), Some(true), "her page is found beside the adopted ledger, never `no_page_dir`");
    assert!(pool.plan_paging(slot, aris).restore, "her first turn after the restart restores, never a cold prefill");

    // A second settled install (a racing probe) loses: the published pool and its settlement stand.
    let second = dir.install_settled_pool(root, 2, |_| SettledPageStore { adoption: Err(NotAdopted::NoLiveLaneRecord), snapshot: None });
    assert!(Arc::ptr_eq(&second, &pool), "one pool per root");
    Ok(())
}
