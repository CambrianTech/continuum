#![cfg(test)]
use super::*;

    /// A grounding RagSource whose `deliver()` the TEST holds the door on: it
    /// cannot return until the gate is opened. Models the real I/O cost (roster
    /// query / workspace-map scan) the slice moves off the hot path — but as a
    /// *control-flow* hold rather than a `sleep`, so "is this deliver on the
    /// critical path?" is answered by whether the tick completes, never by a
    /// wall-clock threshold a loaded CI runner can forge. Exercised through the
    /// LIVE grounding path (GroundingSource → RagSourceFaculty → DeferredFaculty),
    /// not a hand-built faculty.
    pub(super) struct GatedGrounding {
        /// `false` = held shut. Every `deliver()` parks here until it flips true.
        pub(super) gate: watch::Sender<bool>,
        /// Bumped on ENTRY to `deliver()`, before parking — so the test can wait
        /// for "the tick reached the source" without polling a clock.
        pub(super) entries: AtomicUsize,
        /// Fires on each entry, so that wait is event-driven (system law: no
        /// condition-polling).
        pub(super) entered: Notify,
    }

    impl GatedGrounding {
        /// Starts OPEN — the cold tick must be able to complete.
        pub(super) fn new() -> Self {
            let (gate, _rx) = watch::channel(true);
            Self {
                gate,
                entries: AtomicUsize::new(0),
                entered: Notify::new(),
            }
        }
        /// `send_replace`, not `send`: with no receiver parked at this instant
        /// `send` fails and silently leaves the value UNCHANGED, which quietly
        /// turns the whole gate into a no-op. Same reason `DashboardCaptureSink`
        /// publishes with `send_replace`.
        pub(super) fn open_gate(&self) {
            self.gate.send_replace(true);
        }
        pub(super) fn shut_gate(&self) {
            self.gate.send_replace(false);
        }
        pub(super) fn deliver_entries(&self) -> usize {
            self.entries.load(AtomicOrdering::SeqCst)
        }
        /// Resolves once `deliver()` has been ENTERED beyond `baseline`. Loops on
        /// the counter because a stale `notify_one` permit from an earlier entry
        /// may wake us spuriously.
        pub(super) async fn entered_beyond(&self, baseline: usize) {
            while self.deliver_entries() == baseline {
                self.entered.notified().await;
            }
        }
    }

    #[async_trait::async_trait]
    impl RagSource for GatedGrounding {
        fn source_id(&self) -> &'static str {
            "gated-grounding"
        }

        fn expand_command(&self) -> Option<&'static str> {
            // Test/stub source — nothing further to fetch.
            None
        }

        /// Test/stub source — floorless, so it never encodes a production floor.
        async fn deliver(&self, _ctx: &crate::cognition::rag_budget::RagContext) -> crate::cognition::rag_budget::RagDelivery {
            self.entries.fetch_add(1, AtomicOrdering::SeqCst);
            self.entered.notify_one();
            // Park until the test opens the gate. `wait_for` returns immediately
            // when it is already open, so an open gate costs nothing.
            let _ = self.gate.subscribe().wait_for(|open| *open).await;
            crate::cognition::rag_budget::RagDelivery {
                source_id: "gated-grounding".to_string(),
                items: vec![crate::cognition::rag_budget::RagItem {
                    key: String::new(),
                    summary: None,
                    pointer: None,
                    content: "roster: Ivar [persona], win-claude [claude]".to_string(),
                    tokens: 12,
                    metadata: serde_json::Value::Null,
                }],
                tokens_used: 12,
                truncated: false,
            }
        }
    }

    // what this catches: that deferring grounding takes the slow source OFF the
    // perception barrier — the structural claim, proven by control flow instead of
    // by a stopwatch. Model LOCKED (same HeuristicInferenceAdapter both forks), ONE
    // variable changed (defer_grounding). With the source's deliver held SHUT:
    //   * the DEFERRED fork's warm tick still completes — it can only do that by
    //     serving reprojected last-good, i.e. never awaiting deliver; and
    //   * the SYNCHRONOUS fork's warm tick runs INTO deliver and parks there —
    //     the control that proves the hold is real and the completion above means
    //     something.
    // A future parked on a shut gate cannot complete no matter how loaded the
    // machine is, so neither assertion has a timing tolerance to blow. The COST
    // this buys (that removing it from the barrier removes exactly its deliver
    // from the turn's wait) is the arithmetic half, table-tested clock-free elsewhere
    // (the dashboard module that held it was removed as dead).
    //
    // This replaces a version that asserted `deferred_critical_path < 30ms` on the
    // live cycle. It measured wall clock through `cargo test`'s parallelism, so on
    // a loaded CI runner (a 60s CPU-hog test running alongside it) the deferred
    // fork clocked 99,798µs against the synchronous fork's 62,427µs — the fork
    // that pays NO grounding cost measuring higher than the one that pays 60ms.
    // The number was scheduler stall, not grounding; the invariant was never
    // temporal. Never re-express it as a duration.
    #[tokio::test]
    pub(super) async fn deferring_grounding_takes_the_slow_source_off_the_perception_barrier() {
        // A LIVENESS bound, never a performance threshold: every assertion here is
        // about control flow, so this only fires if a tick genuinely hangs.
        const LIVENESS: Duration = Duration::from_secs(30);
        const WARM_BURST: &str = "teammate: and the rollback plan?";

        // Build a cycle with ONE gated grounding source and run the COLD tick with
        // the gate OPEN, so the deferred lane publishes its last-good finding and
        // both forks reach the steady state a live persona actually runs at.
        async fn warmed(defer_grounding: bool) -> (WorkspaceCycle, Arc<GatedGrounding>, Uuid) {
            let persona = Uuid::new_v4();
            let room = Uuid::new_v4();
            let gate = Arc::new(GatedGrounding::new());
            let source: Arc<dyn RagSource> = Arc::clone(&gate) as Arc<dyn RagSource>;
            let mut cfg = cfg_for(persona);
            cfg.grounding_sources = vec![GroundingSource::framing(source).defer_tolerant()];
            cfg.defer_grounding = defer_grounding;

            let cycle = build_workspace_cycle(cfg);
            timeout(
                LIVENESS,
                cycle.run_in_room("teammate: where are we on the deploy?", room),
            )
            .await
            .expect("cold tick completes with the grounding gate open");
            (cycle, gate, room)
        }

        // DEFERRED fork: with deliver held shut, the warm tick must STILL complete.
        let (cycle, gate, room) = warmed(true).await;
        gate.shut_gate();
        timeout(LIVENESS, cycle.run_in_room(WARM_BURST, room))
            .await
            .expect(
                "deferred fork's warm tick never completed with the grounding deliver held \
                 shut — the deferred source is back ON the perception barrier",
            );
        // Release the background worker (parked on the gate) before teardown.
        gate.open_gate();
        drop(cycle);

        // SYNCHRONOUS fork — the control. Same gate, same hold: this tick must run
        // INTO deliver rather than complete, which is what makes the deferred
        // fork's completion above evidence of anything.
        let (cycle, gate, room) = warmed(false).await;
        gate.shut_gate();
        let before = gate.deliver_entries();
        let mut tick = std::pin::pin!(cycle.run_in_room(WARM_BURST, room));
        tokio::select! {
            _ = tick.as_mut() => panic!(
                "synchronous fork's warm tick completed while the grounding deliver was held \
                 shut — the source is not on the perception barrier at all, so this test can \
                 no longer tell deferral apart from nothing happening"
            ),
            reached = timeout(LIVENESS, gate.entered_beyond(before)) => {
                reached.expect(
                    "synchronous fork's warm tick never reached the grounding deliver — the \
                     source is not being delivered on the barrier"
                );
            }
        }
        // …and it finishes the moment the door opens, proving the hold — not a
        // deadlock elsewhere — is what kept it parked.
        gate.open_gate();
        timeout(LIVENESS, tick)
            .await
            .expect("synchronous fork's warm tick finishes once the gate opens");
    }

