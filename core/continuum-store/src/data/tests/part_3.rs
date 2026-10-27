#![cfg(test)]

    // ════════════════════════════════════════════════════════════════
    // Concurrency stress tests for the query-cursor surface — gated
    // behind the `stress-tests` cargo feature. Default `cargo test`
    // skips compilation; run them with
    //     cargo test -p continuum-store --features stress-tests
    // (no CI job runs any crate's stress-tests feature yet; Fable carded it on #5472).
    // See continuum-core/Cargo.toml § "stress-tests" for the doctrine.
    // ════════════════════════════════════════════════════════════════
    #[cfg(feature = "stress-tests")]
    pub(super) mod stress {
        use super::super::*;
        //
        // Per Joel 2026-05-30: "Each persona exists in its own threads."
        //
        // The DataModule is registered ONCE; every persona's thread calls
        // its `&self` handlers concurrently. The paginated-query state
        // map is a `DashMap` precisely so concurrent cursor activity
        // doesn't serialize at a module-level mutex. The tests below
        // pin the invariants the substrate is designed to uphold under
        // that load — they are not exercising rare paths, they are the
        // production scenario.
        //
        // Every test uses `flavor = "multi_thread", worker_threads = 4`
        // so tasks actually preempt each other on distinct OS threads.
        // Single-threaded tokio would silently serialize and pass even
        // if the substrate had a data race.

        /// Build a fresh `Arc<DataModule>` + tempdir + schema + N seeded
        /// rows for a concurrency test. Returns the Arc so callers can
        /// `.clone()` it into spawned tasks without lifetime gymnastics.
        /// The tempdir's lifetime extends past the test body when bound
        /// to a `let _tmp = ...` binding so the SQLite file stays alive
        /// for the duration of every spawned task.
        async fn setup_concurrent(
            suffix: &str,
            rows: usize,
        ) -> (Arc<DataModule>, tempfile::TempDir, String) {
            let module = Arc::new(DataModule::new(no_substrate_entities));
            let (tmp, db_path) = test_db_path(suffix);
            let schema = CollectionSchema {
                collection: "test_handle_cursor".to_string(),
                fields: vec![crate::orm::types::SchemaField {
                    name: "name".to_string(),
                    field_type: crate::orm::types::FieldType::String,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                    foreign_key: None,
                }],
                indexes: vec![],
            };
            let adapter = module.state.get_adapter(&db_path).await.unwrap();
            let _ = adapter.ensure_schema(schema).await;
            for i in 0..rows {
                let _ = create_via_state(
                    &module,
                    json!({
                        "dbPath": &db_path,
                        "collection": "test_handle_cursor",
                        "data": { "name": format!("Item {i}") }
                    }),
                )
                .await;
            }
            (module, tmp, db_path)
        }

        /// N personas open their own cursor at the same time. Every cursor
        /// must mint a DISTINCT HandleRef.id (UUID collision check), every
        /// cursor must be independently reachable via query-next, and
        /// closing one must NOT close any other.
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn cursors_are_isolated_under_concurrent_open_and_next() {
            const PARALLEL: usize = 20;
            // 10 rows seeded → pageSize 3 means each cursor's first page
            // is a full 3-item page (3 + 3 + 3 + 1 = 4 pages total).
            let (module, _tmp, db_path) = setup_concurrent("conc_isolated", 10).await;

            // Phase 1: every persona opens its own cursor in parallel.
            let mut open_tasks = Vec::with_capacity(PARALLEL);
            for _ in 0..PARALLEL {
                let module = module.clone();
                let db_path = db_path.clone();
                open_tasks.push(tokio::spawn(async move {
                    let result = module
                        .handle_command(
                            "data/query-open",
                            json!({
                                "dbPath": db_path,
                                "collection": "test_handle_cursor",
                                "pageSize": 3,
                            }),
                        )
                        .await
                        .expect("query-open must succeed");
                    let CommandResult::Json(v) = result else {
                        panic!("expected Json")
                    };
                    v["handle"].clone()
                }));
            }
            let handles: Vec<Value> = futures::future::join_all(open_tasks)
                .await
                .into_iter()
                .map(|h| h.expect("task must not panic"))
                .collect();

            // Every minted cursor must have a distinct id.
            let mut ids: Vec<String> = handles
                .iter()
                .map(|h| h["id"].as_str().unwrap().to_string())
                .collect();
            ids.sort();
            let before = ids.len();
            ids.dedup();
            assert_eq!(
                ids.len(),
                before,
                "concurrent query-open MUST produce distinct cursor UUIDs ({} dups)",
                before - ids.len()
            );
            assert_eq!(ids.len(), PARALLEL);

            // Phase 2: every persona advances its OWN cursor in parallel.
            // Each cursor's first query-next must return a full page (3
            // items); page numbering must be per-cursor (always 1 for the
            // first call), not cross-contaminated.
            let mut next_tasks = Vec::with_capacity(PARALLEL);
            for handle in &handles {
                let module = module.clone();
                let handle = handle.clone();
                next_tasks.push(tokio::spawn(async move {
                    let result = module
                        .handle_command("data/query-next", json!({ "handle": handle }))
                        .await
                        .expect("query-next must succeed");
                    let CommandResult::Json(v) = result else {
                        panic!("expected Json")
                    };
                    (
                        v["data"]["items"].as_array().unwrap().len(),
                        v["data"]["pageNumber"].as_u64().unwrap(),
                    )
                }));
            }
            let next_results: Vec<(usize, u64)> = futures::future::join_all(next_tasks)
                .await
                .into_iter()
                .map(|r| r.expect("task must not panic"))
                .collect();

            for (i, (items, page)) in next_results.iter().enumerate() {
                assert_eq!(
                *items, 3,
                "cursor {i}: first page must return pageSize items independently of sibling cursors"
            );
                assert_eq!(
                    *page, 1,
                    "cursor {i}: first call's pageNumber must be 1 — per-cursor state, not shared"
                );
            }

            // Phase 3: close half the cursors in parallel. The OTHER half
            // must still be usable — close MUST be per-cursor.
            let (to_close, to_keep): (Vec<_>, Vec<_>) =
                handles.iter().enumerate().partition(|(i, _)| i % 2 == 0);

            let mut close_tasks = Vec::with_capacity(to_close.len());
            for (_, handle) in &to_close {
                let module = module.clone();
                let handle = (*handle).clone();
                close_tasks.push(tokio::spawn(async move {
                    module
                        .handle_command("data/query-close", json!({ "handle": handle }))
                        .await
                }));
            }
            for r in futures::future::join_all(close_tasks).await {
                r.unwrap().expect("close must succeed");
            }

            // Closed cursors fail loud on next.
            for (_, handle) in &to_close {
                let err = module
                    .handle_command("data/query-next", json!({ "handle": (*handle).clone() }))
                    .await
                    .expect_err("closed cursor's next must Err");
                assert!(
                    err.contains("handle not found"),
                    "closed cursor must surface handle-not-found, got: {err}"
                );
            }

            // Kept cursors still serve their next page (page 2).
            for (i, handle) in &to_keep {
                let result = module
                    .handle_command("data/query-next", json!({ "handle": (*handle).clone() }))
                    .await
                    .unwrap_or_else(|e| panic!("kept cursor {i} must still work: {e}"));
                let CommandResult::Json(v) = result else {
                    panic!("expected Json")
                };
                assert_eq!(
                v["data"]["pageNumber"], 2,
                "kept cursor {i}: page 2 follows page 1 — closing sibling cursors did NOT touch this one's state"
            );
            }
        }

        /// Same cursor reached by N concurrent `query-next` calls (whether
        /// from one persona retrying or two callers sharing a handle): the
        /// substrate MUST serialize them via the per-cursor mutex so the
        /// cursor advances atomically. Each non-tail page must be served
        /// AT MOST ONCE.
        ///
        /// Originally caught a real substrate kink: without the per-cursor
        /// mutex, all N concurrent callers read the same `current_page`
        /// snapshot and all returned pageNumber=1. The fix wrapped each
        /// cursor's state in a `tokio::sync::Mutex` so the read-then-
        /// async-then-write window is atomic per cursor.
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn same_cursor_concurrent_next_does_not_corrupt_state() {
            const PARALLEL: usize = 8;
            // 30 items at pageSize 5 = 6 pages. With the per-cursor mutex,
            // each non-tail page (1..=5) is served exactly once and page 6
            // is the terminal page (hasMore=false); any extra concurrent
            // calls after that observe the empty-tail response.
            let (module, _tmp, db_path) = setup_concurrent("conc_same_cursor", 30).await;

            let open = module
                .handle_command(
                    "data/query-open",
                    json!({
                        "dbPath": db_path,
                        "collection": "test_handle_cursor",
                        "pageSize": 5,
                    }),
                )
                .await
                .expect("open must succeed");
            let CommandResult::Json(open) = open else {
                panic!("expected Json")
            };
            let handle = open["handle"].clone();

            // Fire PARALLEL concurrent next calls against the SAME handle.
            let mut tasks = Vec::with_capacity(PARALLEL);
            for _ in 0..PARALLEL {
                let module = module.clone();
                let handle = handle.clone();
                tasks.push(tokio::spawn(async move {
                    module
                        .handle_command("data/query-next", json!({ "handle": handle }))
                        .await
                }));
            }
            let outcomes: Vec<Result<CommandResult, String>> = futures::future::join_all(tasks)
                .await
                .into_iter()
                .map(|r| r.expect("task must not panic"))
                .collect();

            // No call should error from concurrency (DashMap's per-shard
            // locking handles the contention). After the cursor exhausts,
            // the substrate returns success with `hasMore=false` and an
            // empty items list — not an error.
            for (i, outcome) in outcomes.iter().enumerate() {
                assert!(
                    outcome.is_ok(),
                    "concurrent next call {i} must not Err: {:?}",
                    outcome
                );
            }

            // The 6 valid pages + however many empty-tail responses fired
            // before the cursor exhausted. Page numbers must be monotone
            // when sorted; no duplicates of a non-tail page (each non-tail
            // page can only be served ONCE because the cursor advances).
            let mut page_numbers: Vec<u64> = outcomes
                .iter()
                .filter_map(|o| o.as_ref().ok())
                .filter_map(|r| match r {
                    CommandResult::Json(v) => v["data"]["pageNumber"].as_u64(),
                    _ => None,
                })
                .collect();
            page_numbers.sort();

            // Every served page number must be in [1, 6] (we have 30 items
            // at pageSize 5 → 6 real pages, all subsequent calls see page
            // 6 again because the cursor stays at exhausted).
            for &pn in &page_numbers {
                assert!(
                    (1..=6).contains(&pn),
                    "concurrent next produced an out-of-range pageNumber: {pn} (expected 1..=6)"
                );
            }

            // CRITICAL: each non-tail page (1..=5) must appear AT MOST
            // once — DashMap's `get_mut` serializes mutators, so the
            // cursor only advances through each page once. (Page 6 may
            // appear multiple times because once exhausted the cursor
            // stops advancing but keeps returning the empty-tail response
            // — that's the contract.)
            let mut non_tail_counts = std::collections::HashMap::new();
            for &pn in page_numbers.iter().filter(|&&pn| pn < 6) {
                *non_tail_counts.entry(pn).or_insert(0) += 1;
            }
            for (page, count) in non_tail_counts {
                assert_eq!(
                count, 1,
                "page {page} served {count} times — the cursor advanced through it MORE than once, indicating a lost serialization"
            );
            }
        }
    }
