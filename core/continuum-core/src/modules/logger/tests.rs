#![cfg(test)]
    //! The module now owns only config + the shared command state + the dep-holding
    //! family. The write/ping behavior contracts are pinned in
    //! `commands/log/{write,write_batch,ping}.rs`; these tests guard the module's
    //! wiring.
    use super::*;

    // what this catches: config exposes the canonical `log/` prefix + module name.
    // If either drifts, the registry routes the command elsewhere.
    #[test]
    fn config_reports_name_and_prefix() {
        let m = LoggerModule::new();
        let cfg = m.config();
        assert_eq!(cfg.name, "logger");
        assert_eq!(cfg.command_prefixes, &["log/"]);
    }

    // what this catches: the legacy string-match surface is retired — any
    // handle_command call fails loud naming the command (never silently swallows
    // or routes a stale name), per Joel's never-swallow rule.
    #[tokio::test]
    async fn legacy_handle_command_fails_loud() {
        let m = LoggerModule::new();
        let err = m
            .handle_command("log/ping", Value::Null)
            .await
            .expect_err("migrated surface must fail loud");
        assert!(err.contains("migrated to the typed registry"));
        assert!(err.contains("log/ping"));
    }

    // what this catches: the module contributes exactly the three dep-holding
    // log verbs (each carrying the shared state) to the typed registry.
    #[test]
    fn contributes_the_three_log_commands() {
        let m = LoggerModule::new();
        let names: Vec<&str> = m.commands().iter().map(|c| c.name()).collect();
        assert_eq!(names, vec!["log/write", "log/write-batch", "log/ping"]);
    }

    #[test]
    fn test_rate_limiter() {
        let mut rl = RateLimiter::new(3);
        assert!(matches!(rl.check("test"), RateDecision::Allow));
        assert!(matches!(rl.check("test"), RateDecision::Allow));
        assert!(matches!(rl.check("test"), RateDecision::Allow));
        assert!(matches!(rl.check("test"), RateDecision::Drop));
    }

    /// The stop must not be called clean when log content did not reach disk.
    mod write_failures_reach_the_receipt {
        use super::*;

        // what this catches: a write failure being swallowed while the stop still reports
        // clean. The writer thread has nowhere to return an error to, so failures went to
        // stderr — which on a stopping node is nobody — and `shutdown()` returned `Ok(())`
        // regardless. With only a fully completed stop supporting durability, that made
        // the CLI's exit code claim the citizens' logs were safe when they were not.
        //
        // Drives the REAL `write_log_message` against a real unwritable destination, a
        // real successful one after it, and the REAL decision `shutdown` delegates to.
        #[tokio::test]
        async fn a_failed_write_then_a_successful_one_still_ends_in_a_non_clean_stop() {
            let tmp = tempfile::tempdir().unwrap();
            let failures: WriteFailures = Arc::new(AtomicU64::new(0));
            let cache: FileCache = Default::default();
            let headers: HeaderTracker = Default::default();
            let payload = WriteLogPayload {
                category: "test".into(),
                level: LogLevel::Warn,
                component: "regression".into(),
                message: "this write cannot land".into(),
                args: None,
            };

            // A FILE where the log directory must be: the write genuinely cannot land.
            let blocked = tmp.path().join("blocked");
            std::fs::write(&blocked, b"in the way").unwrap();
            // THE SHARED OPERATION — the same `write_and_latch` the writer thread calls.
            // The earlier version called `write_log_message` and then did the
            // `fetch_add` ITSELF, which proved a counter can be incremented rather than
            // that production increments it: deleting the production latch would have
            // left it green.
            let landed = write_and_latch(
                &payload,
                &blocked.to_string_lossy(),
                tmp.path(),
                &cache,
                &headers,
                &failures,
            );
            assert!(
                !landed,
                "a log directory that is a FILE must fail the write, not silently succeed"
            );
            assert_eq!(
                failures.load(Ordering::Relaxed),
                1,
                "the shared write+latch must record the failure — no manual increment here"
            );

            // A LATER write SUCCEEDS. This is the case that used to read as clean: the
            // last write worked and the buffers flushed, and nothing carried the earlier
            // loss forward.
            let ok_dir = tmp.path().join("writable");
            std::fs::create_dir_all(&ok_dir).unwrap();
            let good = write_and_latch(
                &payload,
                &ok_dir.to_string_lossy(),
                tmp.path(),
                &cache,
                &headers,
                &failures,
            );
            assert!(good, "a writable destination must still work");
            assert_eq!(
                failures.load(Ordering::Relaxed),
                1,
                "a SUCCESSFUL write must not add a failure — the count is the earlier loss"
            );

            // THE POINT: the REAL `shutdown()` on a REAL module holding that count is not
            // clean. Reading `stop_outcome` directly, as the earlier version did, would
            // have stayed green if `shutdown` stopped delegating to it.
            let (log_tx, _rx) = mpsc::sync_channel::<WriteLogPayload>(8);
            let state = Arc::new(LoggerCommandState {
                log_tx,
                file_cache: cache.clone(),
                started_at: Instant::now(),
                requests_processed: AtomicU64::new(0),
                pending_writes: Arc::new(AtomicU64::new(0)),
                write_failures: failures.clone(),
            });
            let module = LoggerModule::with_state(state);
            let err = module
                .shutdown()
                .await
                .expect_err("a module that lost log content must not stop clean");
            assert!(
                err.contains('1') && err.contains("did not reach disk"),
                "the failure must be counted and named, got: {err}"
            );
        }

        // what this catches: the opposite error — a module with no failures refusing to
        // stop cleanly, which would make every ordinary shutdown report data loss and
        // train the operator to ignore the signal entirely.
        // what this catches: a module with no failures refusing to stop cleanly, which
        // would make every ordinary shutdown report data loss and train the operator to
        // ignore the signal. Also through the REAL `shutdown`, so it fails if the
        // delegation inverts.
        #[tokio::test]
        async fn a_module_that_wrote_everything_stops_clean() {
            let (log_tx, _rx) = mpsc::sync_channel::<WriteLogPayload>(8);
            let state = Arc::new(LoggerCommandState {
                log_tx,
                file_cache: Default::default(),
                started_at: Instant::now(),
                requests_processed: AtomicU64::new(0),
                pending_writes: Arc::new(AtomicU64::new(0)),
                write_failures: Arc::new(AtomicU64::new(0)),
            });
            assert!(
                LoggerModule::with_state(state).shutdown().await.is_ok(),
                "no failures means a clean stop"
            );
        }

        // what this catches: the counter being process-global again. Two modules must not
        // see each other's failures — which is exactly why this was moved off a `static`,
        // and why a test could not drive a real failure before.
        #[test]
        fn one_modules_failures_do_not_condemn_anothers_stop() {
            let mine: WriteFailures = Arc::new(AtomicU64::new(3));
            let theirs: WriteFailures = Arc::new(AtomicU64::new(0));
            assert!(stop_outcome(mine.load(Ordering::Relaxed)).is_err());
            assert!(stop_outcome(theirs.load(Ordering::Relaxed)).is_ok());
        }
    }
