#![cfg(test)]

    use super::*;
    use crate::commands::training_trigger::test_support::{build_runtime, ex, submit_params};
    use crate::runtime::ServiceModule;


    pub(super) fn pause_at(state: &TrainingTriggerState, point: OperationPoint) -> Arc<OperationPause> {
        let pause = Arc::new(OperationPause {
            point,
            entered: tokio::sync::Notify::new(),
            resume: tokio::sync::Notify::new(),
        });
        *state.operation_pause.lock().unwrap() = Some(pause.clone());
        pause
    }



    // What this catches (278afa6c): an init timeout must not permanently disable
    // learning. Cancel the real owner's setup wait; its existing tick retries
    // against the retained actual SQLite adapter, without a global HOME/config edit.
    #[tokio::test]
    pub(super) async fn owner_tick_retries_interrupted_schema_setup() {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let trigger = Arc::new(super::super::TrainingTriggerModule::new(crate::genome::learner::seat::Authority::isolated(1 << 30)));
        let pause = pause_at(&trigger.state, OperationPoint::BeforeSchemaSetup);
        let state = trigger.state.clone();
        let setup = tokio::spawn(async move { state.initialize_storage(adapter).await });
        tokio::time::timeout(std::time::Duration::from_secs(5), pause.entered.notified())
            .await
            .unwrap();
        setup.abort();
        assert!(setup.await.unwrap_err().is_cancelled());
        assert!(trigger.state.require_ready().is_err());
        trigger.tick().await.unwrap();
        trigger.state.require_ready().unwrap();
        assert!(trigger
            .state
            .durable
            .require()
            .unwrap()
            .submissions
            .find_all()
            .await
            .unwrap()
            .is_empty());
        trigger.shutdown().await.unwrap();
        // A late initialization publication cannot override authoritative stop.
        trigger.state.ready.store(true, Ordering::Release);
        assert!(trigger.state.require_ready().is_err());
        assert!(trigger.tick().await.is_err());
        let admission: Result<(), String> = trigger
            .state
            .run_owned(
                BucketKey {
                    persona_id: Uuid::new_v4(),
                    trait_kind: "code".into(),
                    base_model: "synthetic".into(),
                },
                |_, _| async { panic!("shutdown admitted a new operation") },
            )
            .await;
        assert!(admission.is_err());
    }


    // What this catches (278afa6c): real SQLite INSERT commits, then the caller
    // cancels before cache publication. The admitted owner must keep its lease,
    // preserve A's policy/order, and prevent B from installing conflicting policy.
    #[tokio::test]
    pub(super) async fn cancelled_acceptance_keeps_policy_and_order_until_commit_is_published() {
        use std::time::Duration;
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let (trigger, executor) = build_runtime(adapter.clone()).await;
        let persona = Uuid::new_v4();
        let id = Uuid::new_v4();
        let mut request = submit_params(persona, "code", vec![ex("first", "a")]);
        request["submissionId"] = serde_json::json!(id);
        let pause = pause_at(&trigger.state, OperationPoint::AcceptanceCommitted);
        let caller_executor = executor.clone();
        let caller_request = request.clone();
        let caller = tokio::spawn(async move {
            caller_executor
                .execute_json("genome/training-trigger/submit", caller_request)
                .await
        });
        tokio::time::timeout(Duration::from_secs(5), pause.entered.notified())
            .await
            .unwrap();
        assert!(trigger
            .state
            .durable
            .require()
            .unwrap()
            .submissions
            .find_by_id(id)
            .await
            .unwrap()
            .is_some());
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        let mut changed = submit_params(persona, "code", vec![ex("second", "b")]);
        changed["preferredProvider"] = serde_json::json!("different-policy");
        let waiting = executor.execute_json("genome/training-trigger/submit", changed);
        tokio::pin!(waiting);
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut waiting)
                .await
                .is_err(),
            "cancelled caller must not release the owner's lease"
        );
        pause.resume.notify_one();
        let refused = waiting.await.unwrap();
        assert_eq!(refused["errorKind"], "InconsistentBucket", "{refused}");
        assert!(refused.get("acceptance").is_none());
        let replay = executor
            .execute_json("genome/training-trigger/submit", request)
            .await
            .unwrap();
        assert_eq!(replay["acceptance"]["replayed"], true);
        let next = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(persona, "code", vec![ex("second", "b")]),
            )
            .await
            .unwrap();
        assert_eq!(next["currentCount"], 2);
        trigger.shutdown().await.unwrap();
        let (restarted, _) = build_runtime(adapter).await;
        restarted.tick().await.unwrap();
        let arrived = restarted.state.durable.require().unwrap().arrivals(0, 8).await.unwrap();
        assert_eq!(
            arrived.iter().flat_map(|row| row.examples.iter().map(|example| example.prompt.as_str())).collect::<Vec<_>>(),
            ["first", "second"],
            "the journal holds both, in acceptance order"
        );
    }


