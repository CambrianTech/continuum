#![cfg(test)]
    // what this catches (the 5090, 2026-09-21): a page switch that costs seconds on a
    // discrete card is not a wedge — the bound rises with this node's measured p90 and
    // never falls below the floor; an unmeasured node is judged by the floor alone.
    #[test]
    fn the_wedge_bound_is_sized_from_the_nodes_measured_switches_never_a_flat_cap() {
        use super::kv_page_wedge_bound;
        use std::time::Duration;
        assert_eq!(kv_page_wedge_bound(0), Duration::from_secs(10), "unmeasured: the floor");
        assert_eq!(kv_page_wedge_bound(100), Duration::from_secs(10), "the M5's 0.1 s: the floor");
        assert_eq!(kv_page_wedge_bound(6_835), Duration::from_millis(20_505), "the 5090's p90 × 3: a 10 s switch is a switch");
        assert!(kv_page_wedge_bound(6_835) > Duration::from_millis(10_004), "the max this box measured is inside the bound");
    }

    use super::*;
    use uuid::Uuid;

    mod one_act_per_mind;
    mod lifecycle;

    // what this catches (M5, 2026-09-25 and 2026-09-26): a slow switch on a moving engine
    // must wait; a save on a QUIET engine (no slot processing, so no counter moves) must
    // also wait, up to the quiet cap; a switch on an engine whose slots claim to process
    // but whose counters never move is stuck; and a silent engine ends the wait.
    #[tokio::test(start_paused = true)]
    async fn a_slow_page_switch_waits_while_the_engine_progresses_and_ends_when_it_stalls() {
        use crate::inference::llama_server::{EngineProbe, EngineProgress};
        let bound = std::time::Duration::from_secs(10);
        let slow = async {
            tokio::time::sleep(bound * 3 + std::time::Duration::from_secs(1)).await;
            "receipt"
        };
        let mut tick = 0u64;
        let mut seen = Vec::new();
        let got = wait_while_engine_progresses(
            slow,
            bound,
            || { tick += 1; let t = tick; async move { EngineProbe::Progress(EngineProgress { fingerprint: t, any_processing: true }) } },
            |n| seen.push(n),
        )
        .await;
        assert_eq!(got, Some("receipt"), "a moving queue keeps the switch");
        assert_eq!(seen, vec![1, 2, 3], "each bound is a reported checkpoint, not a verdict");

        let quiet_save = async {
            tokio::time::sleep(bound * 2 + std::time::Duration::from_secs(1)).await;
            "saved"
        };
        let got = wait_while_engine_progresses(
            quiet_save,
            bound,
            || async { EngineProbe::Progress(EngineProgress { fingerprint: 7, any_processing: false }) },
            |_| {},
        )
        .await;
        assert_eq!(got, Some("saved"), "a save on a quiet engine is the queue head, not a stall");

        let endless_quiet = wait_while_engine_progresses(
            std::future::pending::<&str>(),
            bound,
            || async { EngineProbe::Progress(EngineProgress { fingerprint: 7, any_processing: false }) },
            |_| {},
        )
        .await;
        assert_eq!(endless_quiet, None, "quiet is bounded: the cap ends a wait that never completes");

        let stuck = wait_while_engine_progresses(
            std::future::pending::<&str>(),
            bound,
            || async { EngineProbe::Progress(EngineProgress { fingerprint: 42, any_processing: true }) },
            |_| {},
        )
        .await;
        assert_eq!(stuck, None, "slots processing with nothing moving is stuck");

        let gone = wait_while_engine_progresses(std::future::pending::<&str>(), bound, || async { EngineProbe::Unreachable }, |_| {}).await;
        assert_eq!(gone, None, "an engine that is not there ends the wait");
        // card 8c06f778: a probe that times out is the engine WORKING, not gone — the
        // restore it cannot answer around completes, and the switch keeps it.
        let busy_restore = async {
            tokio::time::sleep(bound * 2 + std::time::Duration::from_secs(1)).await;
            "restored"
        };
        let got = wait_while_engine_progresses(busy_restore, bound, || async { EngineProbe::Busy }, |_| {}).await;
        assert_eq!(got, Some("restored"), "a probe timeout mid-switch is busy, never dead");
        let endless_busy = wait_while_engine_progresses(std::future::pending::<&str>(), bound, || async { EngineProbe::Busy }, |_| {}).await;
        assert_eq!(endless_busy, None, "busy is bounded like quiet: the cap ends a wait that never completes");
    }

    // what this catches: the fingerprint moves when any slot prefills, decodes or takes
    // a new task, and a body that is not the /slots array reads as no answer.
    #[test]
    fn the_slots_fingerprint_moves_with_any_slot_work() {
        let fp = |v: &serde_json::Value| crate::inference::llama_server::slots_progress(v).map(|p| p.fingerprint);
        let a = json!([{"id_task": 7, "n_prompt_tokens_processed": 100, "next_token": [{"n_decoded": 5}]},
                       {"id_task": 3, "n_prompt_tokens_processed": 0, "next_token": [{"n_decoded": 0}]}]);
        let decoded = json!([{"id_task": 7, "n_prompt_tokens_processed": 100, "next_token": [{"n_decoded": 6}]},
                             {"id_task": 3, "n_prompt_tokens_processed": 0, "next_token": [{"n_decoded": 0}]}]);
        let new_task = json!([{"id_task": 7, "n_prompt_tokens_processed": 100, "next_token": [{"n_decoded": 5}]},
                              {"id_task": 4, "n_prompt_tokens_processed": 0, "next_token": [{"n_decoded": 0}]}]);
        assert_ne!(fp(&a), fp(&decoded), "a decoded token is progress");
        assert_ne!(fp(&a), fp(&new_task), "a new task is progress");
        assert_eq!(fp(&a), fp(&a.clone()), "no work, no change");
        assert_eq!(fp(&json!({"error": "no slots"})), None, "not the slots array");
    }

    #[tokio::test]
    async fn anonymous_traffic_waits_for_the_scratch_slot_its_holder_owns() {
        // what this catches (M5, 2026-09-26 03:01Z): non-turn traffic wrote the scratch
        // slot without its permit, so the cache probe holding that slot was overwritten
        // between requests and the self-check held the fleet on a false `no_reuse`.
        let pool = Arc::new(KvSlotPool::new("test://scratch-owner", 3)); // index 2 is scratch
        let scratch = pool.scratch_slot().expect("three slots reserve a scratch slot"); // test: pool contract
        let held = pool.acquire_slot(scratch).await.expect("test: the probe holds scratch");
        let sem = Arc::new(Semaphore::new(4));
        let client = reqwest::Client::new();
        let waiting = admit_turn(&sem, None, Some(pool.clone()), &client, "test://scratch-owner", 0, std::time::Duration::ZERO);
        tokio::pin!(waiting);
        assert!(
            futures::poll!(&mut waiting).is_pending(),
            "anonymous traffic entered the scratch slot while its holder owned it"
        );
        drop(held);
        let admitted = waiting.await.expect("test: admitted once the holder releases");
        assert_eq!(admitted.scratch_slot(), Some(scratch));
    }

    // regression for card 4deaed09 (BigMama, 2026-09-26 17:47–17:59Z): a 2-lane engine
    // has no scratch slot, and non-turn traffic went UNPINNED — the server placed it by
    // similarity onto a resident's slot and overwrote her page; Sahar reused 0 tokens
    // every turn while Kimi reused 44,950 on the other slot.
    // what this catches: on a pool with no scratch slot, transient traffic borrows a
    // citizen slot THROUGH the pool — the cheapest resident is saved and detached first
    // and the admission carries that slot pinned — the single-slot rule generalised. And a
    // quote that names another checkpoint subset than its write (card 3512f496).
    #[tokio::test]
    async fn on_a_server_with_no_scratch_slot_transient_traffic_borrows_the_cheapest_citizen_slot_and_saves_its_resident() {
        use axum::{
            extract::{Path, Query},
            http::StatusCode,
            routing::post,
            Json, Router,
        };
        let saves = Arc::new(std::sync::Mutex::new(Vec::<u32>::new()));
        let subsets = Arc::new(std::sync::Mutex::new(Vec::<serde_json::Value>::new()));
        let app = Router::new().route(
            "/slots/{slot}",
            post({
                let (saves, subsets) = (saves.clone(), subsets.clone());
                move |Path(slot): Path<u32>, Query(query): Query<std::collections::HashMap<String, String>>, Json(body): Json<serde_json::Value>| {
                    let (saves, subsets) = (saves.clone(), subsets.clone());
                    async move {
                        let action = query.get("action").cloned().unwrap_or_default(); // unwrap_or_default: a fixture request with no action is a restore
                        if action.starts_with("save") {
                            subsets.lock().expect("test: fixture mutex").push(json!([body["keep_view_checkpoints"], body["keep_newest_checkpoints"]]));
                        }
                        if action == "save-size" {
                            return (StatusCode::OK, Json(json!({"id_slot": slot, "filename": body["filename"], "projected_bytes": 100})));
                        }
                        if action == "save" {
                            saves.lock().expect("test: fixture mutex").push(slot);
                        }
                        let count = if action == "save" { "n_saved" } else { "n_restored" };
                        (StatusCode::OK, Json(json!({"id_slot": slot, "filename": body["filename"], (count): 10})))
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test: bind");
        let root = format!("http://{}", listener.local_addr().expect("test: addr"));
        let server = tokio::spawn(async move { axum::serve(listener, app).await });
        let pool = crate::inference::slots::directory().ensure_pool(&root, 2);
        assert!(crate::inference::slots::directory()
            .negotiate_page_admission(&root, &pool, None, None).await,
            "this fixture advertises the legacy paging protocol");
        assert!(pool.scratch_slot().is_none(), "two lanes reserve no scratch slot");
        let sem = Arc::new(Semaphore::new(4));
        let client = reqwest::Client::new();
        let a = PageKey::mind(Uuid::from_u128(1)).expect("test: non-nil ids");
        let b = PageKey::mind(Uuid::from_u128(3)).expect("test: non-nil ids");
        // Exact pre-write admission uses the same HTTP boundary and policy, with
        // supplied volume evidence instead of mutating the process-wide monitor.
        assert_eq!(kv_page_action_with_capacity(&client, &root, 0, &a, "save", true, |bytes| {
            crate::paging::pool::ensure_projected_disk_capacity("fixture", 900, 1000, bytes)
                .map_err(|error| error.to_string())
        }).await, PageOutcome::Rejected);
        assert!(saves.lock().unwrap().is_empty(), "capacity refusal must not issue a save");
        assert_eq!(kv_page_action_with_capacity(&client, &root, 0, &a, "save", true, |bytes| {
            assert_eq!(bytes, 100);
            crate::paging::pool::ensure_projected_disk_capacity("fixture", 800, 1000, bytes)
                .map_err(|error| error.to_string())
        }).await, PageOutcome::Completed);
        assert_eq!(saves.lock().unwrap().as_slice(), &[0]);
        saves.lock().unwrap().clear();
        let subsets = std::mem::take(&mut *subsets.lock().unwrap());
        assert_eq!(subsets.len(), 3, "two quotes and one write");
        assert!(subsets.iter().all(|subset| *subset == json!([true, 0])), "{subsets:?}");
        // Two residents warm their slots: a large working set and a small one. Each
        // turn COMPLETES (a dropped, uncommitted turn forgets its attribution — the
        // cancellation rule — and would leave both slots reading as free).
        let mut adm_a = admit_turn(&sem, Some(a), Some(pool.clone()), &client, &root, 20_000, std::time::Duration::ZERO)
            .await
            .expect("test: a admitted");
        let slot_a = adm_a.slot().expect("test: a leased a slot");
        adm_a.generation_completed();
        drop(adm_a);
        let mut adm_b = admit_turn(&sem, Some(b), Some(pool.clone()), &client, &root, 500, std::time::Duration::ZERO)
            .await
            .expect("test: b admitted");
        let slot_b = adm_b.slot().expect("test: b leased a slot");
        adm_b.generation_completed();
        drop(adm_b);
        assert_ne!(slot_a, slot_b);
        assert_eq!(pool.transient_slot(), slot_b, "the cheapest page to lose is the small one");
        // Anonymous traffic: borrowed THROUGH the pool — b's page saved, the slot carried.
        let transient = admit_turn(&sem, None, Some(pool.clone()), &client, &root, 0, std::time::Duration::ZERO)
            .await
            .expect("test: transient admitted");
        assert_eq!(transient.slot(), Some(slot_b), "pinned to the borrowed slot, never left to the server");
        assert_eq!(
            saves.lock().expect("test: fixture mutex").as_slice(),
            &[slot_b],
            "the resident was saved first, and only that one"
        );
        drop(transient);
        server.abort();
    }

    // what this catches (card 8fa1c147): a refused page save that cannot say why. Each way a
    // save-size reply fails to quote reads as its own miss, so the refusal probe names it.
    #[test]
    fn a_page_quote_names_how_it_failed() {
        let ok = json!({"id_slot": 1, "filename": "mind-a.bin", "projected_bytes": 4096});
        assert_eq!(read_quote(200, Some(&ok), 1, "mind-a.bin"), Ok(4096));
        assert_eq!(read_quote(501, Some(&ok), 1, "mind-a.bin"), Err(QuoteMiss::Status(501)));
        assert_eq!(read_quote(200, Some(&ok), 0, "mind-a.bin"), Err(QuoteMiss::Mismatch), "another slot");
        assert_eq!(read_quote(200, Some(&ok), 1, "mind-b.bin"), Err(QuoteMiss::Mismatch), "another file");
        let zero = json!({"id_slot": 1, "filename": "mind-a.bin", "projected_bytes": 0});
        assert_eq!(read_quote(200, Some(&zero), 1, "mind-a.bin"), Err(QuoteMiss::NoSize));
        assert_eq!(read_quote(200, None, 1, "mind-a.bin"), Err(QuoteMiss::NoSize), "an unreadable body");
        assert!(QuoteMiss::EngineStopped.to_string().contains("stopped progressing"));
    }

    // what this catches (Codex on #5441): an engine that answers 501 and then stalls its error
    // body. The status must return before the body is read, as a Status miss, not hang the quote
    // until a watchdog calls the engine stopped.
    #[tokio::test]
    async fn a_refused_quote_returns_its_status_without_waiting_for_a_stalled_body() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test: bind");
        let root = format!("http://{}", listener.local_addr().expect("test: addr"));
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("test: accept");
            let mut request = [0u8; 4096];
            let _ = socket.read(&mut request).await;
            // Headers promise a body that never arrives.
            socket.write_all(b"HTTP/1.1 501 Not Implemented\r\ncontent-type: application/json\r\ncontent-length: 64\r\n\r\n{").await.expect("test: write");
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        });
        let quoted = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            request_quote(&reqwest::Client::new(), &root, 0, "mind-a.bin"),
        )
        .await
        .expect("the status returns without reading the stalled body"); // expect: test-only bound
        assert_eq!(quoted, Err(QuoteMiss::Status(501)));
        server.abort();
    }

    // what this catches: the restore-into-busy-slot bug at its source — while a turn
    // holds its admission (permit + pin), a second activity contending for the SAME
    // single slot must NOT be able to evict the pinned slot out from under it. Only
    // once the first admission drops does the slot become leasable again. This is the
    // invariant that makes the 90s restore wait unnecessary. No HTTP: a fresh key has
    // no page, so save/restore are skipped.
    #[tokio::test]
    async fn a_pinned_slot_survives_a_contending_lease_until_the_turn_drops() {
        let pool = Arc::new(KvSlotPool::new("test://admit", 1)); // ONE citizen slot
        let sem = Arc::new(Semaphore::new(1));
        let client = reqwest::Client::new();
        let a = PageKey::mind(Uuid::from_u128(1)).unwrap();  // test: non-nil ids
        let b = PageKey::mind(Uuid::from_u128(3)).unwrap();  // test: non-nil ids

        // A is admitted: holds the permit and pins the one slot.
        let adm_a = admit_turn(
            &sem,
            Some(a),
            Some(pool.clone()),
            &client,
            "test://admit",
            100,
            std::time::Duration::ZERO,
        )
        .await
        .expect("test: endpoint admitted");
        let slot_a = adm_a.slot().expect("A leased the slot"); // test: a 1-slot pool leases to the first admission

        // B tries to lease while A is pinned — eviction must skip the pinned slot, so
        // B cannot take slot_a (the pool has no other slot to give).
        assert!(
            pool.lease(b).await.is_none(),
            "a pinned slot was handed to a contending activity — the exact restore-into-busy-slot bug"
        );

        // A's turn ends — the pin (and permit) release.
        drop(adm_a);

        // Now B can lease, and it gets the freed slot.
        let after_cancel = pool.lease_paged(b).await.expect("slot released after cancellation");
        assert_eq!(
            after_cancel.slot,
            slot_a,
            "after the turn dropped, the slot must become leasable again"
        );
        assert_eq!(
            after_cancel.save_first, None,
            "an admission dropped without successful generation must not be saved as activity A"
        );

        // The endpoint lease is shared across adapter semaphores. A different
        // adapter cannot enter while retirement drains this admitted turn.
        let adm = admit_turn(&sem, Some(b), Some(pool), &client, "test://admit", 100, std::time::Duration::ZERO)
            .await
            .expect("test: second turn");
        let endpoint = crate::inference::slots::directory().endpoint("test://admit");
        let transition = endpoint.transition();
        tokio::pin!(transition);
        assert!(futures::poll!(&mut transition).is_pending());
        let other_adapter = Arc::new(Semaphore::new(1));
        assert!(
            admit_turn(&other_adapter, None, None, &client, "test://admit", 0, std::time::Duration::ZERO)
                .await
                .is_err(),
            "another adapter cannot bypass endpoint suspension"
        );
        drop(adm);
        drop(transition.await);
        assert!(
            admit_turn(&other_adapter, None, None, &client, "test://admit", 0, std::time::Duration::ZERO)
                .await
                .is_err(),
            "failed transition remains closed"
        );
    }

    // what this catches (card ff9ecfd8): a turn that finds every slot pinned used to be
    // admitted with NO slot, so the server placed it on whichever slot freed and overwrote a
    // resident's KV with no save (M5 8/13, IntelMac 19/36 contended leases). It must wait,
    // stay pending while the slot is held, and land PINNED on the freed slot once the holder
    // drops; warm-ahead still gives up at once.
    #[tokio::test]
    async fn a_turn_that_finds_every_slot_pinned_waits_and_lands_pinned() {
        let root = "test://all-pinned";
        let pool = Arc::new(KvSlotPool::new(root, 1)); // ONE citizen slot
        let sem = Arc::new(Semaphore::new(2)); // lanes free: only the slot is contended
        let client = reqwest::Client::new();
        let a = PageKey::mind(Uuid::from_u128(11)).unwrap(); // test: non-nil ids
        let b = PageKey::mind(Uuid::from_u128(13)).unwrap(); // test: non-nil ids
        let adm_a = admit_turn(&sem, Some(a), Some(pool.clone()), &client, root, 100, std::time::Duration::ZERO)
            .await
            .expect("test: A admitted");
        let slot_a = adm_a.slot().expect("A pinned the one slot"); // test: a 1-slot pool leases to the first admission

        let warm = warm_ahead(&sem, b, pool.clone(), &client, root);
        assert!(warm.await.is_ok(), "warm-ahead is best-effort: it gives up at once, never waits");

        let turn_b = admit_turn(&sem, Some(b), Some(pool.clone()), &client, root, 100, std::time::Duration::ZERO);
        tokio::pin!(turn_b);
        assert!(futures::poll!(&mut turn_b).is_pending(), "B was admitted unpinned while every slot was held");
        assert_eq!(sem.available_permits(), 1, "a turn waiting for a slot must not hold a lane permit (Fable on #4515)");
        drop(adm_a);
        let adm_b = tokio::time::timeout(std::time::Duration::from_secs(5), turn_b)
            .await
            .expect("test: B woke on the release")
            .expect("test: B admitted");
        assert_eq!(adm_b.slot(), Some(slot_a), "B must land PINNED on the freed slot");
    }

    // what this catches (10417f6d): a queued lifecycle writer must not form a cycle
    // with an existing slot waiter's semaphore reacquisition and a later admission.
    #[tokio::test(start_paused = true)]
    async fn a_later_admission_cannot_block_the_reader_a_transition_is_draining()
        -> Result<(), Box<dyn std::error::Error>>
    {
        let root = format!("test://admission-drain-{}", Uuid::new_v4());
        let pool = Arc::new(KvSlotPool::new(&root, 1));
        let first_adapter = Arc::new(Semaphore::new(1));
        let waiting_adapter = Arc::new(Semaphore::new(1));
        let client = reqwest::Client::new();
        let first_key = PageKey::mind(Uuid::new_v4()).ok_or("first key")?;
        let waiting_key = PageKey::mind(Uuid::new_v4()).ok_or("waiting key")?;
        let first = admit_turn(
            &first_adapter, Some(first_key), Some(pool.clone()), &client, &root,
            100, std::time::Duration::ZERO,
        ).await?;
        let first_slot = first.slot().ok_or("first admission must pin the slot")?;
        let mut waiting = Box::pin(admit_turn(
            &waiting_adapter, Some(waiting_key), Some(pool.clone()), &client, &root,
            100, std::time::Duration::ZERO,
        ));
        assert!(futures::poll!(&mut waiting).is_pending(), "the only slot is pinned");
        assert_eq!(waiting_adapter.available_permits(), 1, "slot waiter released its permit");

        let endpoint = crate::inference::slots::directory().endpoint(&root);
        // A superseded intent still drains before checking authority, but must leave
        // the healthy generation ready. No engine or HTTP fixture is needed here.
        let superseded = || false;
        let mut transition = Box::pin(endpoint.transition_if(&superseded));
        assert!(futures::poll!(&mut transition).is_pending(), "existing readers must drain");
        let mut later = Box::pin(admit_turn(
            &waiting_adapter, None, None, &client, &root,
            0, std::time::Duration::ZERO,
        ));
        assert!(futures::poll!(&mut later).is_pending(), "a later reader cannot overtake the writer");
        drop(first);

        // On the old ordering, later owns the sole semaphore permit behind the writer,
        // waiting retains a reader while reacquiring that permit, and the writer waits
        // for waiting. The writer must drain without forcing either queued turn to
        // run first; no outer watchdog fires or cancels an admission to free it.
        assert!(tokio::time::timeout(std::time::Duration::from_secs(1), &mut transition)
            .await.map_err(|_| "slot waiter blocked lifecycle drain")?.is_err());
        assert!(endpoint.is_ready());
        let later_admitted = tokio::time::timeout(std::time::Duration::from_secs(1), &mut later)
            .await.map_err(|_| "later admission did not resume after lifecycle drain")??;
        drop(later_admitted);
        drop(later);
        let admitted = tokio::time::timeout(std::time::Duration::from_secs(1), &mut waiting)
            .await.map_err(|_| "released slot could not drain: endpoint/semaphore lock cycle")??;
        assert_eq!(admitted.slot(), Some(first_slot));
        drop(admitted);
        drop(waiting);
        assert_eq!(waiting_adapter.available_permits(), 1);
        assert_eq!(first_adapter.available_permits(), 1);
        Ok(())
    }

    // what this catches (10417f6d): fixing the lock order must not turn queued
    // adapter admissions into endpoint readers that delay a lifecycle transition.
    #[tokio::test(start_paused = true)]
    async fn a_transition_drains_running_turns_not_the_adapter_queue()
        -> Result<(), Box<dyn std::error::Error>>
    {
        let root = format!("test://admission-queue-drain-{}", Uuid::new_v4());
        let sem = Arc::new(Semaphore::new(1));
        let client = reqwest::Client::new();
        let running = admit_turn(&sem, None, None, &client, &root, 0, std::time::Duration::ZERO).await?;
        let mut first = Box::pin(admit_turn(&sem, None, None, &client, &root, 0, std::time::Duration::ZERO));
        let mut second = Box::pin(admit_turn(&sem, None, None, &client, &root, 0, std::time::Duration::ZERO));
        assert!(futures::poll!(&mut first).is_pending());
        assert!(futures::poll!(&mut second).is_pending());
        let endpoint = crate::inference::slots::directory().endpoint(&root);
        let superseded = || false;
        let mut transition = Box::pin(endpoint.transition_if(&superseded));
        assert!(futures::poll!(&mut transition).is_pending());
        drop(running);
        // Poll the semaphore winner before the writer: it must wait behind the
        // writer without already carrying a reader from its earlier queue wait.
        assert!(futures::poll!(&mut first).is_pending());
        assert!(tokio::time::timeout(std::time::Duration::from_secs(1), &mut transition)
            .await.map_err(|_| "adapter queue retained endpoint readers")?.is_err());
        let admitted = first.as_mut().await?;
        drop(admitted);
        drop(first);
        let admitted = second.as_mut().await?;
        drop(admitted);
        drop(second);
        assert_eq!(sem.available_permits(), 1);
        Ok(())
    }

    // what this catches (10417f6d): an all-pinned waiter must discard discovery
    // from the old generation when a lifecycle transition replaces its slot pool.
    #[tokio::test(start_paused = true)]
    async fn a_slot_waiter_resumes_on_the_replacement_generations_pool()
        -> Result<(), Box<dyn std::error::Error>>
    {
        let root = format!("test://admission-generation-{}", Uuid::new_v4());
        let endpoint = crate::inference::slots::directory().endpoint(&root);
        let mut contract = crate::inference::slots::KvPageContract {
            model_id: "fixture".into(), model: "fixture.gguf".into(), adapters: vec![],
            page_dir: Some("fixture-pages".into()), context: 8192, slots: 1,
            cache_type: Some("q8_0".into()), flash_attn: Some(true),
            revisions: Some(vec![("fixture.gguf".into(), 73, std::time::SystemTime::UNIX_EPOCH)]),
        };
        let initial = endpoint.transition().await;
        let old = initial.start_generation()?;
        initial.ready(&root, &old, contract.clone())?;
        drop(initial);
        let old_pool = endpoint.admit().await?.pool.ok_or("initial pool")?;
        let sem = Arc::new(Semaphore::new(2));
        let client = reqwest::Client::new();
        let first_key = PageKey::mind(Uuid::new_v4()).ok_or("first key")?;
        let waiting_key = PageKey::mind(Uuid::new_v4()).ok_or("waiting key")?;
        let first = admit_turn(&sem, Some(first_key), Some(old_pool.clone()), &client, &root,
            100, std::time::Duration::ZERO).await?;
        let mut waiting = Box::pin(admit_turn(&sem, Some(waiting_key), Some(old_pool.clone()),
            &client, &root, 100, AdmissionPatience::for_turn()));
        assert!(futures::poll!(&mut waiting).is_pending());
        // Healthy slot contention outlasts the 240s readiness allowance without
        // spending it. A later engine replacement still gets its readiness hold.
        tokio::time::advance(std::time::Duration::from_secs(300)).await;
        let mut replacing = Box::pin(endpoint.transition());
        assert!(futures::poll!(&mut replacing).is_pending());
        drop(first);
        let replacement = tokio::time::timeout(std::time::Duration::from_secs(1), &mut replacing)
            .await.map_err(|_| "slot waiter blocked generation replacement")?;
        old.observed_exit();
        let new = replacement.start_generation()?;
        assert!(futures::poll!(&mut waiting).is_pending());
        tokio::time::advance(std::time::Duration::from_secs(20)).await;
        contract.context = 16384;
        replacement.ready(&root, &new, contract)?;
        drop(replacement);
        let new_pool = endpoint.admit().await?.pool.ok_or("replacement pool")?;
        assert!(!Arc::ptr_eq(&old_pool, &new_pool));
        let admitted = tokio::time::timeout(std::time::Duration::from_secs(1), &mut waiting)
            .await.map_err(|_| "slot waiter missed release across replacement")??;
        let (admitted_pool, key, _) = admitted.uncommitted.as_ref().ok_or("provisional slot")?;
        assert!(Arc::ptr_eq(admitted_pool, &new_pool), "stale discovery pool reused");
        assert_eq!(*key, waiting_key);
        drop(admitted);
        drop(waiting);
        assert_eq!(sem.available_permits(), 2);
        Ok(())
    }

    // what this catches (10417f6d): restarting admission after a slot wake must
    // retain readiness patience after slot contention, and cancellation releases it.
    #[tokio::test(start_paused = true)]
    async fn slot_waits_preserve_readiness_patience_and_cancellation_releases_admission()
        -> Result<(), Box<dyn std::error::Error>>
    {
        let root = format!("test://admission-patience-{}", Uuid::new_v4());
        let pool = Arc::new(KvSlotPool::new(&root, 1));
        let sem = Arc::new(Semaphore::new(2));
        let client = reqwest::Client::new();
        let first_key = PageKey::mind(Uuid::new_v4()).ok_or("first key")?;
        let waiting_key = PageKey::mind(Uuid::new_v4()).ok_or("waiting key")?;
        let first = admit_turn(&sem, Some(first_key), Some(pool.clone()), &client, &root,
            100, std::time::Duration::ZERO).await?;
        let started = tokio::time::Instant::now();
        let mut waiting = Box::pin(admit_turn(&sem, Some(waiting_key), Some(pool.clone()),
            &client, &root, 100, std::time::Duration::from_secs(10)));
        assert!(futures::poll!(&mut waiting).is_pending());
        let mut cancelled = Box::pin(admit_turn(&sem, Some(waiting_key), Some(pool.clone()),
            &client, &root, 100, std::time::Duration::from_secs(10)));
        assert!(futures::poll!(&mut cancelled).is_pending());
        drop(cancelled);
        assert_eq!(sem.available_permits(), 1);
        tokio::time::advance(std::time::Duration::from_secs(8)).await;
        let endpoint = crate::inference::slots::directory().endpoint(&root);
        let mut transition = Box::pin(endpoint.transition());
        assert!(futures::poll!(&mut transition).is_pending());
        drop(first);
        drop(tokio::time::timeout(std::time::Duration::from_secs(1), &mut transition)
            .await.map_err(|_| "cancelled or waiting turn retained endpoint reader")?);
        assert!(waiting.as_mut().await.is_err(), "closed endpoint must exhaust patience");
        assert_eq!(started.elapsed(), std::time::Duration::from_secs(18));
        drop(waiting);
        assert_eq!(sem.available_permits(), 2);
        Ok(())
    }

    // what this catches (10417f6d): excluding slot waits from readiness patience
    // must not exclude them from the request's absolute deadline or leak a permit.
    #[tokio::test(start_paused = true)]
    async fn the_turn_deadline_bounds_adapter_and_slot_waits()
        -> Result<(), Box<dyn std::error::Error>>
    {
        for permits in [1, 2] {
            let root = format!("test://admission-deadline-{}", Uuid::new_v4());
            let sem = Arc::new(Semaphore::new(permits));
            let pool = Arc::new(KvSlotPool::new(&root, 1));
            let client = reqwest::Client::new();
            let first_key = PageKey::mind(Uuid::new_v4()).ok_or("first key")?;
            let next_key = PageKey::mind(Uuid::new_v4()).ok_or("next key")?;
            let running = admit_turn(&sem, Some(first_key), Some(pool.clone()), &client, &root,
                100, std::time::Duration::ZERO).await?;
            let started = tokio::time::Instant::now();
            let waiting = admit_turn(&sem, Some(next_key), Some(pool), &client, &root, 100,
                AdmissionPatience::until(started + std::time::Duration::from_secs(8)));
            assert!(waiting.await.is_err());
            assert_eq!(started.elapsed(), std::time::Duration::from_secs(8));
            assert_eq!(sem.available_permits(), permits - 1);
            drop(running);
            assert_eq!(sem.available_permits(), permits);
        }
        Ok(())
    }

    // what this catches (Codex on #4515): a turn that pinned its slot and is then CANCELLED
    // while awaiting the slot's operation permit (pin held, no permit) frees an evictable lease,
    // and no SlotPermit drop will ever announce it; a turn parked on the all-pinned pool must
    // still wake, lease and pin. The permit stays held by a stand-in decode throughout, so the
    // only possible wake is the cancelled admission's own Drop.
    #[tokio::test]
    async fn a_cancelled_pin_holder_wakes_a_turn_parked_on_an_all_pinned_pool() {
        let root = "test://cancelled-pin";
        let pool = Arc::new(KvSlotPool::new(root, 1)); // ONE citizen slot, index 0
        let sem = Arc::new(Semaphore::new(3));
        let client = reqwest::Client::new();
        let a = PageKey::mind(Uuid::from_u128(21)).unwrap(); // test: non-nil ids
        let b = PageKey::mind(Uuid::from_u128(23)).unwrap(); // test: non-nil ids
        // A decode is in flight on slot 0, so A leases and pins, then waits for the permit.
        let busy = pool.acquire_slot(0).await.expect("test: the stand-in decode holds slot 0");
        let mut turn_a = Box::pin(admit_turn(&sem, Some(a), Some(pool.clone()), &client, root, 100, std::time::Duration::ZERO));
        assert!(futures::poll!(&mut turn_a).is_pending(), "A waits for the slot permit");
        assert!(pool.pin(&a).is_some(), "A holds its lease, pinned");
        let turn_b = admit_turn(&sem, Some(b), Some(pool.clone()), &client, root, 100, std::time::Duration::ZERO);
        tokio::pin!(turn_b);
        assert!(futures::poll!(&mut turn_b).is_pending(), "every slot pinned: B parks");
        assert!(pool.pin(&b).is_none(), "B has no lease while it is parked");
        // Cancel A while it holds a pin and no permit.
        drop(turn_a);
        for _ in 0..16 {
            let _ = futures::poll!(&mut turn_b);
            tokio::task::yield_now().await;
        }
        assert!(pool.pin(&b).is_some(), "B was never woken: a cancelled pin holder announced nothing");
        drop(busy);
        let adm_b = tokio::time::timeout(std::time::Duration::from_secs(5), turn_b)
            .await
            .expect("test: B completes once the permit frees")
            .expect("test: B admitted");
        assert_eq!(adm_b.slot(), Some(0), "B lands pinned on the slot A gave up");
    }

    // what this catches: warm-ahead at 4fe7e312 marked a lease warm even when
    // no restore occurred, failed, or was cancelled; later turns skipped restore.
    #[tokio::test]
    async fn warm_ahead_confirms_only_resident_or_successfully_restored_pages() {
        use axum::{
            extract::{Path, Query},
            http::StatusCode,
            routing::post,
            Json, Router,
        };
        use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

        let mode = Arc::new(AtomicU8::new(0));
        let calls = Arc::new(AtomicUsize::new(0));
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let finished = Arc::new(tokio::sync::Notify::new());
        let app = Router::new().route("/props", axum::routing::get(|| async {
            Json(json!({"total_slots": 3}))
        })).route(
            "/slots/{slot}",
            post({
                let mode = mode.clone();
                let calls = calls.clone();
                let entered = entered.clone();
                let release = release.clone();
                let finished = finished.clone();
                move |Path(slot): Path<u32>, Query(query): Query<std::collections::HashMap<String, String>>, Json(body): Json<serde_json::Value>| {
                    let mode = mode.clone();
                    let calls = calls.clone();
                    let entered = entered.clone();
                    let release = release.clone();
                    let finished = finished.clone();
                    async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        if mode.load(Ordering::SeqCst) == 1 || (mode.load(Ordering::SeqCst) == 7 && slot == 1) {
                            return (StatusCode::BAD_REQUEST, Json(json!({"error": {"code": 400, "type": "invalid_request_error", "message": "Unable to restore slot"}})));
                        }
                        if mode.load(Ordering::SeqCst) == 2 || (mode.load(Ordering::SeqCst) == 5 && slot == 1) {
                            entered.notify_one();
                            release.notified().await;
                            finished.notify_one();
                        }
                        if mode.load(Ordering::SeqCst) == 3 || (mode.load(Ordering::SeqCst) == 6 && slot == 1) {
                            return (StatusCode::OK, Json(json!({"unverified": true})));
                        }
                        let receipt_slot = if mode.load(Ordering::SeqCst) == 4 && slot == 1 { slot + 1 } else { slot };
                        let count = if query.get("action").map(String::as_str) == Some("save") { "n_saved" } else { "n_restored" };
                        (StatusCode::OK, Json(json!({"id_slot": receipt_slot, "filename": body["filename"], (count): 10})))
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("isolated paging HTTP fixture");
        let root = format!(
            "http://{}",
            listener.local_addr().expect("bound fixture address")
        );
        let server = tokio::spawn(async move { axum::serve(listener, app).await });
        let endpoint = crate::inference::slots::directory().endpoint(&root);
        let contract = crate::inference::slots::KvPageContract {
            model_id: "fixture".into(),
            model: "fixture.gguf".into(),
            adapters: vec![],
            page_dir: Some("fixture-pages".into()),
            context: 8192,
            slots: 3,
            cache_type: Some("q8_0".into()),
            flash_attn: Some(true),
            revisions: Some(vec![(
                "fixture.gguf".into(),
                73,
                std::time::SystemTime::UNIX_EPOCH,
            )]),
        };
        let initial = endpoint.transition().await;
        let old = initial.start_generation().expect("initial generation");
        initial
            .ready(&root, &old, contract.clone())
            .expect("initial readiness");
        drop(initial);
        let pool = endpoint
            .admit()
            .await
            .expect("ready pool")
            .pool
            .expect("managed pool");
        let sem = Arc::new(Semaphore::new(2));
        let client = reqwest::Client::new();
        let key = PageKey::mind(Uuid::new_v4()).expect("fixture activity");

        // A discovered multi-slot pool is not sufficient: without reserved
        // scratch, anonymous traffic can still select a speculative restore slot.
        let small_root = format!("test://warm-two-slot-{}", Uuid::new_v4());
        let small = crate::inference::slots::directory().ensure_pool(&small_root, 2);
        small.note_saved(key);
        let stale_discovery = Arc::new(KvSlotPool::new(&small_root, 3));
        warm_ahead(&sem, key, stale_discovery, &client, &small_root)
            .await
            .expect("authoritative two-slot skip");
        let cold = small.lease_paged(key).await.expect("small pool lease");
        assert!(!cold.already_resident);
        assert!(
            cold.restore,
            "skipped warm-ahead must leave the page for the actual turn"
        );

        // A cold lease has no physical KV to attribute, and must issue no HTTP.
        warm_ahead(&sem, key, pool.clone(), &client, &root)
            .await
            .expect("cold warm-ahead");
        let slot = pool.lease(key).await.expect("assigned physical slot");
        assert!(pool.take_resident(slot).0.is_none());
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        pool.note_saved(key);
        mode.store(1, Ordering::SeqCst);
        warm_ahead(&sem, key, pool.clone(), &client, &root)
            .await
            .expect("failed restore is best effort");
        assert!(pool.take_resident(slot).0.is_none());
        let after_failure = pool.lease_paged(key).await.expect("cold after failure");
        assert!(!after_failure.restore, "failed page is no longer offered");
        assert!(!after_failure.already_resident);
        pool.forget_resident(slot, key);

        pool.note_saved(key);
        mode.store(2, Ordering::SeqCst);
        let mut queued = {
            let warming = warm_ahead(&sem, key, pool.clone(), &client, &root);
            tokio::pin!(warming);
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                tokio::select! {
                    result = &mut warming => panic!("restore completed before cancellation: {result:?}"),
                    _ = entered.notified() => {}
                }
            }).await.expect("restore request reached fixture");
            let mut queued = Box::pin(admit_turn(
                &sem,
                Some(key),
                Some(pool.clone()),
                &client,
                &root,
                100,
                std::time::Duration::ZERO,
            ));
            assert!(futures::poll!(&mut queued).is_pending());
            queued
        };
        // Dropping the warm-ahead waiter is not cancellation of engine IO.
        assert!(futures::poll!(&mut queued).is_pending(), "same-slot waiter stays behind the restore");
        assert_eq!(sem.available_permits(), 0, "both admissions still own their lanes");
        assert!(endpoint.is_ready(), "caller cancellation does not condemn a progressing engine");
        assert_eq!(calls.load(Ordering::SeqCst), 2, "no duplicate restore was dispatched");
        release.notify_one();
        tokio::time::timeout(std::time::Duration::from_secs(5), finished.notified())
            .await.expect("fixture request drained");
        let mut admitted = tokio::time::timeout(std::time::Duration::from_secs(5), queued)
            .await.expect("slot released after receipt").expect("same engine remains usable");
        assert_eq!(admitted.page_in(), PageIn::Resident, "abandoned verified restore retained warmth");
        admitted.generation_completed();
        drop(admitted);
        assert_eq!(sem.available_permits(), 2);
        let new = old;
        mode.store(0, Ordering::SeqCst);
        warm_ahead(&sem, key, pool.clone(), &client, &root)
            .await
            .expect("successful retry");
        let restored_calls = calls.load(Ordering::SeqCst);
        assert_eq!(restored_calls, 2);
        warm_ahead(&sem, key, pool.clone(), &client, &root)
            .await
            .expect("already resident");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            restored_calls,
            "confirmed residency skips redundant restore"
        );
        assert_eq!(pool.take_resident(slot).0, Some(key));

        // A warm-ahead while her act runs, even from another adapter, is a no-op: her KV is
        // already in her slot, and it never queues behind her act (`slots::mind_gate`). It
        // cannot see or erase her act's provisional holder.
        let mut turn = admit_turn(&sem, Some(key), Some(pool.clone()), &client, &root, 100, std::time::Duration::ZERO)
            .await
            .expect("turn restore");
        let other_adapter = Arc::new(Semaphore::new(2));
        let before = calls.load(Ordering::SeqCst);
        tokio::time::timeout(std::time::Duration::from_secs(5), warm_ahead(&other_adapter, key, pool.clone(), &client, &root))
            .await
            .expect("a mid-act warm-ahead returns at once")
            .expect("skipped, not failed");
        assert_eq!(calls.load(Ordering::SeqCst), before, "no paging while she acts");
        turn.generation_completed();
        drop(turn);
        assert_eq!(pool.take_resident(slot).0, Some(key));
        mode.store(3, Ordering::SeqCst);
        assert!(
            warm_ahead(&sem, key, pool.clone(), &client, &root)
                .await
                .is_err(),
            "a status-only or malformed acknowledgement is uncertain"
        );
        assert!(!endpoint.is_ready());
        assert!(pool.take_resident(slot).0.is_none());
        // What this catches: a drained checkpoint must confirm every resident,
        // never carry an old saved bit across a failed write or let cancellation
        // reopen a backend request whose completion is unknown.
        let other = PageKey::mind(Uuid::new_v4()).expect("second resident");
        let unknown_root = format!("test://checkpoint-unknown-{}", Uuid::new_v4());
        let unknown = crate::inference::slots::directory().endpoint(&unknown_root);
        let mut unowned = unknown.transition().await;
        assert!(unowned
            .checkpoint_residents(&client, &unknown_root)
            .await
            .is_err());
        drop(unowned);
        let mut prior = new;
        for behavior in [0, 7, 6, 4, 5] {
            let mut checkpointing = endpoint.transition().await;
            prior.observed_exit();
            let generation = checkpointing
                .start_generation()
                .expect("fixture child replacement");
            let before = calls.load(Ordering::SeqCst);
            assert!(
                checkpointing
                    .checkpoint_residents(&client, &root)
                    .await
                    .is_err(),
                "a new generation cannot checkpoint its predecessor's ledger"
            );
            checkpointing
                .ready(&root, &generation, contract.clone())
                .expect("verified replacement");
            let residents = crate::inference::slots::directory()
                .get(&root)
                .flatten()
                .expect("verified pool");
            residents.plan_paging(0, key);
            residents.plan_paging(1, other);
            residents.note_saved(key);
            residents.note_saved(other);
            assert!(checkpointing
                .checkpoint_residents(&client, &unknown_root)
                .await
                .is_err());
            assert_eq!(
                calls.load(Ordering::SeqCst),
                before,
                "identity refusals issue no HTTP"
            );
            mode.store(behavior, Ordering::SeqCst);
            assert!(!crate::inference::slots::directory().page_admission_known(&root),
                "a new ready generation has not received an adapter probe");
            assert!(crate::inference::slots::directory().page_admission_path(&root).is_err(),
                "unknown capability cannot authorize a save");
            if behavior == 5 {
                {
                    let saving = checkpointing.checkpoint_residents(&client, &root);
                    tokio::pin!(saving);
                    tokio::time::timeout(std::time::Duration::from_secs(5), async {
                        tokio::select! {
                            _ = &mut saving => panic!("checkpoint completed before fixture release"),
                            _ = entered.notified() => {}
                        }
                    }).await.expect("second save reached fixture");
                    assert!(
                        endpoint.admit().await.is_err(),
                        "writer keeps late turns closed"
                    );
                }
                assert!(
                    endpoint.paging_recovery_required(),
                    "cancelled save quarantines"
                );
                assert!(checkpointing
                    .ready(&root, &generation, contract.clone())
                    .is_err());
                let refused_at = calls.load(Ordering::SeqCst);
                assert!(checkpointing
                    .checkpoint_residents(&client, &root)
                    .await
                    .is_err());
                assert_eq!(calls.load(Ordering::SeqCst), refused_at);
                release.notify_one();
                tokio::time::timeout(std::time::Duration::from_secs(5), finished.notified())
                    .await
                    .expect("cancelled fixture drained");
            } else if behavior == 0 {
                let receipt = checkpointing
                    .checkpoint_residents(&client, &root)
                    .await
                    .expect("both residents saved");
                assert!(crate::inference::slots::directory().page_admission_known(&root),
                    "lifecycle checkpoint negotiates before any adapter request");
                assert!(
                    receipt.transition_for(&prior).is_err(),
                    "receipt refuses predecessor"
                );
                assert!(receipt.transition_for(&generation).is_ok());
                assert!(
                    endpoint.admit().await.is_err(),
                    "receipt retains closed writer"
                );
                generation.observed_exit();
                assert!(
                    receipt.transition_for(&generation).is_err(),
                    "exit invalidates receipt"
                );
                drop(receipt);
            } else {
                assert!(checkpointing
                    .checkpoint_before_replacement(&client, &root, &|| true)
                    .await
                    .is_err());
                assert_eq!(endpoint.is_ready(), behavior == 7,
                    "terminal save rejection resumes the original engine; uncertain saves stay closed");
                assert_eq!(
                    endpoint.paging_recovery_required(),
                    behavior != 7,
                    "only terminal rejection proves completion"
                );
                assert!(
                    !generation.has_exited(),
                    "save refusal never retires the child"
                );
            }
            assert_eq!(
                calls.load(Ordering::SeqCst),
                before + 2,
                "only attributed slots are saved"
            );
            residents.take_resident(0);
            assert!(
                residents.plan_paging(0, key).restore,
                "confirmed first save survives partial failure"
            );
            residents.take_resident(1);
            assert_eq!(
                residents.plan_paging(1, other).restore,
                behavior == 0,
                "failed or unconfirmed overwrite invalidates previous saved eligibility"
            );
            drop(checkpointing);
            assert_eq!(
                endpoint.is_ready(), behavior == 7,
                "dropping writer preserves the explicit resume decision"
            );
            prior = generation;
        }
        server.abort();
    }

mod paging_lifetime;
