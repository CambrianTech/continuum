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
async fn each_engine_state_answers_its_own_probed_variant() -> Result<(), Box<dyn std::error::Error>> {
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
            (_, before) => return Err(format!("an unready launch over a {before} pool changed its answer").into()),
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
            dir.install_settled_pool(root, 2, None, |pool| {
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
    let second = dir.install_settled_pool(root, 2, None, |_| SettledPageStore { adoption: Err(NotAdopted::NoLiveLaneRecord), snapshot: None });
    assert!(Arc::ptr_eq(&second, &pool), "one pool per root");
    Ok(())
}

// what this catches: the M5 2026-11-17 04:20Z outage (regression for core 37a354cf2, #6087). An owned spawn
// publishes its pool at readiness from its launch contract, which carries no minds declaration; discovery then
// compared that pool's "no minds" to the engine's declared minds and refused EVERY generation (minds_mismatch,
// ~10 re-probes a second) for the engine's whole life. The pool's minds are a set-once record of the engine's
// own declaration: a pool with no record takes the first; only a recorded, different declaration mismatches.
#[test]
fn a_pool_records_the_engines_declared_minds_once() {
    use crate::inference::weight_residency::ResidentMinds;
    let minds = Some(ResidentMinds { lanes: 5, parked_ids: 5, incarnation: 4_077_290_934_432_821_903 });
    let other = Some(ResidentMinds { lanes: 5, parked_ids: 5, incarnation: 7 });

    let owned = KvSlotPool::new("owned", 5); // as an owned spawn publishes it: no declaration yet
    assert_eq!(owned.resident_minds(), None, "undeclared reads as no minds, never as a recorded none");
    assert!(owned.declare_resident_minds(minds), "the first discovery records the engine's declaration");
    assert_eq!(owned.resident_minds(), minds);
    assert!(owned.declare_resident_minds(minds), "the same engine's declaration agrees");
    assert!(!owned.declare_resident_minds(other), "another engine's declaration is the mismatch");

    let discovered = KvSlotPool::new("discovered", 5).with_resident_minds(None); // installed by a /props with no minds
    assert!(!discovered.declare_resident_minds(minds), "a recorded 'no minds' is a declaration, not a blank");
}

/// A fake engine declaring resident minds on `/props`, and this core's owned spawn of it: a ready generation
/// whose pool was published from the launch contract (no minds declaration yet).
async fn owned_spawn_declaring_minds() -> (String, Arc<KvSlotPool>, tempfile::TempDir, tokio::task::JoinHandle<()>) {
    // the fork's /props shape for an engine with resident minds, typed (as the engine writes it)
    #[derive(serde::Serialize, Clone)]
    struct Minds { enabled: bool, lanes: u64, parked_ids: u64, parked_free: u64, incarnation: u64 }
    #[derive(serde::Serialize, Clone)]
    struct Geometry { allocated_tokens: u64, max_tokens_per_sequence: u64, sequence_capacity: u64, resident_minds: Minds }
    #[derive(serde::Serialize, Clone)]
    struct Props { total_slots: u32, context_geometry: Geometry }
    let props = Props {
        total_slots: 2,
        context_geometry: Geometry {
            allocated_tokens: 8192,
            max_tokens_per_sequence: 4096,
            sequence_capacity: 4,
            resident_minds: Minds { enabled: true, lanes: 2, parked_ids: 2, parked_free: 2, incarnation: 42 },
        },
    };
    let app = axum::Router::new()
        .route("/props", axum::routing::get(move || { let props = props.clone(); async move { axum::Json(props) } }));
    let namespace = format!("/{}", uuid::Uuid::new_v4()); // a recycled port is not this fixture engine
    let app = axum::Router::new().nest(&namespace, app);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test: bind");
    let root = format!("http://{}{namespace}", listener.local_addr().expect("test: addr"));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.expect("test: serve") });

    let dir = tempfile::tempdir().expect("test: page dir");
    let endpoint = directory().endpoint(&root);
    let contract = KvPageContract {
        model_id: "fixture".into(),
        model: "fixture.gguf".into(),
        adapters: vec![],
        page_dir: Some(dir.path().to_path_buf()),
        context: 8192,
        slots: 2,
        cache_type: Some("q8_0".into()),
        flash_attn: Some(true),
        revisions: Some(vec![("fixture.gguf".into(), 73, std::time::SystemTime::UNIX_EPOCH)]),
    };
    let transition = endpoint.transition().await;
    let generation = transition.start_generation().expect("test: generation");
    transition.ready(&root, &generation, contract).expect("test: ready");
    drop(transition);
    let owned = endpoint.admit().await.expect("test: ready pool").pool.expect("test: managed pool");
    assert_eq!(owned.resident_minds(), None, "the spawn knows its contract, not the engine's declaration");
    (root, owned, dir, server)
}

const DECLARED: crate::inference::weight_residency::ResidentMinds =
    crate::inference::weight_residency::ResidentMinds { lanes: 2, parked_ids: 2, incarnation: 42 };

// what this catches: the M5 2026-11-17 outage end to end (owned spawn -> adapter discovery -> admission). The
// owned pool must be accepted, record the engine's declaration and be admitted, and the fast path afterwards
// returns the same DECLARED pool. Before the fix every discovery here returned MindsMismatch.
#[tokio::test]
async fn an_owned_spawn_learns_its_engines_minds_at_discovery() {
    let (root, owned, _dir, server) = owned_spawn_declaring_minds().await;
    let client = reqwest::Client::new();
    let pool = directory().discover_pool(&client, &root).await.expect("discovery accepts the owned pool");
    assert!(Arc::ptr_eq(&pool, &owned), "the owned pool is the one discovered, not a replacement");
    assert_eq!(pool.resident_minds(), Some(DECLARED), "discovery recorded the engine's own declaration");
    assert!(directory().page_admission_known(&root), "page admission granted after the declaration");
    let again = directory().discover_pool(&client, &root).await.expect("the fast path");
    assert!(Arc::ptr_eq(&again, &owned) && again.resident_minds() == Some(DECLARED), "the fast path returns the declared pool");
    server.abort();
}

// what this catches: the lifecycle path granting page admission FIRST (a checkpoint before any adapter request)
// on an owned pool without the engine's minds declaration; the adapter's discovery then takes the fast path and
// would read the undeclared pool as core-paged minds (Codex on 5b2f48ae7). The lifecycle grant must declare.
#[tokio::test]
async fn a_lifecycle_admission_first_still_declares_the_engines_minds() {
    let (root, owned, _dir, server) = owned_spawn_declaring_minds().await;
    let client = reqwest::Client::new();
    directory().discover_page_admission(&client, &root).await.expect("the lifecycle path grants page admission");
    assert_eq!(owned.resident_minds(), Some(DECLARED), "the lifecycle grant recorded the engine's declaration");
    let pool = directory().discover_pool(&client, &root).await.expect("the adapter's fast path");
    assert!(Arc::ptr_eq(&pool, &owned), "the fast path returned the admitted owned pool");
    assert_eq!(pool.resident_minds(), Some(DECLARED), "never an undeclared pool read as core-paged minds");
    server.abort();
}
