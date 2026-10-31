use super::*;

/// WARM-GATE: never measure a COLD model. Probes the cycle's lane with a throwaway
/// deliberation (no acts) until it answers, at most 15 tries 4 s apart; `false` if it never
/// does. Resets the volatile scratch the probe touched, so the first task starts clean.
/// Shared by `cognition/eval` and the gene examiner's sitting.
pub(super) async fn warm_gate(cycle: &crate::cognition::workspace::WorkspaceCycle) -> bool {
    let warm_room = crate::identity::ActivityRoom::mint();
    let probe_delivery = crate::cognition::rag_budget::RagDelivery {
        source_id: "airc".to_string(),
        items: vec![crate::cognition::rag_budget::RagItem {
            key: String::new(),
            summary: None,
            pointer: None,
            content: "ready check".to_string(),
            tokens: 0,
            metadata: serde_json::json!({ "peer_id": "peer", "occurred_at_ms": EVAL_EPOCH_MS }),
        }],
        tokens_used: 0,
        truncated: false,
    };
    let mut warm = false;
    for _ in 0..15u32 {
        let burst = crate::cognition::workspace::Burst::from_turns(
            warm_room,
            crate::cognition::conversation::workspace_turns::build_workspace_turns(
                std::slice::from_ref(&probe_delivery),
                "",
                "",
                None,
            ),
        );
        let probe = crate::cognition::act_observe::drive_to_settle(
            cycle,
            burst,
            0, // no acts — a bare deliberation is enough to prove the lane answers
            crate::cognition::workspace::TurnFraming::directed(),
        )
        .await;
        if probe.inference_error.is_none() {
            warm = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(4)).await;
    }
    // Reset the volatile scratch the probe touched so task #1 starts from the same
    // clean frame every other task does.
    cycle.reset_working_memory();
    warm
}
