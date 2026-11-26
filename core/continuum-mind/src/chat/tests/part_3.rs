#![cfg(test)]
use super::*;

    // ── #265: ring hydration from the durable transcript ─────────────
    #[tokio::test]
    pub(super) async fn hydrate_speech_rings_seeds_room_and_own_rings_from_durable_store() {
        // what this catches: reboot wipes the in-process speech rings, so the
        // repetition/[settled] facts are blind exactly during the post-boot
        // wake-greeting round (observed live 2026-07-30 — every persona
        // re-greeted 2+ times, facts silent). Hydration must replay the
        // durable transcript into BOTH the per-room and per-sender rings.
        // Fresh UUIDs per run: the rings are process-global, so unique keys
        // keep this isolated under full-suite parallelism (the #7 lesson).
        let room = Uuid::new_v4();
        let persona = Uuid::new_v4();
        let room_s = room.to_string();
        let persona_s = persona.to_string();

        let chat = chat_with_stubs(vec![Arc::new(StubDataModule::query_only(move |_p| {
            // Store returns DESC (latest-first); poll normalizes chronological.
            json!({
                "success": true,
                "data": [
                    { "id": "m2", "data": { "id": "m2", "roomId": room_s, "senderId": persona_s,
                        "timestamp": "2026-07-30T12:00:01Z",
                        "content": { "text": "Hello everyone! I'm Benchy, back on the grid." } } },
                    { "id": "m1", "data": { "id": "m1", "roomId": room_s, "senderId": persona_s,
                        "timestamp": "2026-07-30T12:00:00Z",
                        "content": { "text": "the wordstats tests are green, posting output" } } }
                ]
            })
        }))]);

        chat.hydrate_speech_rings()
            .await
            .expect("hydration over a healthy store must succeed");

        let room_ring = crate::cognition::deliberation_budget::recent_room_speech(room);
        assert_eq!(
            room_ring.len(),
            2,
            "both durable lines must land in the room ring"
        );
        assert!(
            room_ring[1].contains("back on the grid"),
            "chronological order: the newest line is last (ring tail)"
        );

        let own_ring = crate::cognition::deliberation_budget::recent_own_speech(
            crate::identity::PeerId::from_uuid(persona),
            room,
        );
        assert_eq!(
            own_ring.len(),
            2,
            "the sender's own-speech ring must carry her durable lines — this is \
             what lets own_repetition/inbound_restates fire on a post-boot re-greeting"
        );
        // …and they must be filed under the room they were SAID in: hydration that
        // dumped every line into one undifferentiated ring would re-create the
        // cross-room repetition fact (a citizen entering a fresh room told she is
        // "circling" about utterances from elsewhere).
        assert!(
            crate::cognition::deliberation_budget::recent_own_speech(
                crate::identity::PeerId::from_uuid(persona),
                uuid::Uuid::new_v4(),
            )
            .is_empty(),
            "hydration must key by room — a room she never spoke in reads as silent"
        );
    }
