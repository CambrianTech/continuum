#![cfg(test)]
    use super::*;
    use serde_json::json;

    /// A `persona:act` payload — one executed tool receipt (#243).
    fn act_payload(
        room: Uuid,
        act: Uuid,
        actor: Uuid,
        tool: &str,
        summary: &str,
    ) -> serde_json::Value {
        json!({
            "actId": act,
            "roomId": room,
            "actorId": actor,
            "actorName": "Asha",
            "tool": tool,
            "summary": summary,
            "ok": true,
            "timestamp": 1_700_000_000_000u64,
        })
    }

    #[test]
    fn act_receipts_fold_into_the_snapshot_dedup_and_clear_on_room_switch() {
        // what this catches: the #243 act stream's three contracts — a
        // `persona:act` event lands in `ChatViewState.acts` with its facts
        // intact, a redelivered act_id folds once, and a room switch clears
        // the receipts with the rest of the per-room state.
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room = Uuid::from_u128(0x1);
        let act = Uuid::from_u128(0x2);
        let actor = Uuid::from_u128(0x3);
        let payload = act_payload(room, act, actor, "code/read", "sympy/core/mul.py");
        for _ in 0..2 {
            if let Some(ProjectionInput::Act(a)) = classify(PERSONA_ACT, &payload) {
                p.apply_act(a);
            } else {
                panic!("persona:act payload must classify as an Act input");
            }
        }
        let view = current_chat(&substrate);
        assert_eq!(view.acts.len(), 1, "redelivered act_id must fold once");
        let r = &view.acts[0];
        assert_eq!(r.tool, "code/read");
        assert_eq!(r.summary, "sympy/core/mul.py");
        assert_eq!(r.actor_name, "Asha");
        assert!(r.ok);
        assert_eq!(r.thought, None, "a receipt that carries no reasoning shows none");
        // Room switch clears the receipt ring with the rest of the state.
        // PREMISE CHANGE (anti-wander): the switch is explicit focus now.
        p.apply_focus(Uuid::from_u128(0x9));
        // Her reasoning for the batch rides its first receipt to the human's view, whole:
        // the renderer decides how much of it to show (card ba1eeda1).
        let reasoning = "the label drops the index; ".repeat(40) + "so check checks.py:1040 first";
        let mut thinking = act_payload(Uuid::from_u128(0x9), Uuid::from_u128(0xa), actor, "code/shell", "pytest -x");
        thinking["thought"] = json!(reasoning);
        if let Some(ProjectionInput::Act(a)) = classify(PERSONA_ACT, &thinking) {
            p.apply_act(a);
        }
        let view = current_chat(&substrate);
        assert_eq!(view.acts.len(), 1);
        assert_eq!(view.acts[0].tool, "code/shell");
        assert_eq!(view.acts[0].thought.as_deref(), Some(reasoning.as_str()), "the view carries her reasoning whole, never clipped");
    }

    // what this catches: a `chat:heard` receipt decorates the row it names — the
    // citizen joins `heard_by` exactly once, a repeat is a no-op, and a receipt for a
    // row the window does not hold is dropped (never a row of its own). The delivery
    // receipt the human reads as "heard by N" (2026-09-04).
    #[test]
    fn a_heard_receipt_joins_the_row_once_and_never_mints_a_row() {
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room = Uuid::from_u128(0x1);
        let msg = Uuid::from_u128(0xa);
        if let Some(ProjectionInput::Message(m)) = classify(CHAT_POSTED, &posted(room, msg, "hi all")) {
            p.apply_message(m);
        }
        let heard = |persona: u128, message: Uuid| {
            json!({ "message_id": message, "room_id": room, "persona_id": Uuid::from_u128(persona) })
        };
        for payload in [heard(0x7, msg), heard(0x7, msg), heard(0x8, msg), heard(0x9, Uuid::from_u128(0xdead))] {
            if let Some(ProjectionInput::Heard(h)) = classify(CHAT_HEARD, &payload) {
                p.apply_heard(h);
            } else {
                panic!("chat:heard must classify: {payload}");
            }
        }
        let chat = current_chat(&substrate);
        assert_eq!(chat.messages.len(), 1, "a receipt never mints a row");
        assert_eq!(chat.messages[0].heard_by.len(), 2, "two citizens heard it, one of them twice");
    }

    #[test]
    fn nil_room_act_never_steals_the_projection() {
        // what this catches: regression for the 2026-08-12 live find — a
        // headless solve's act carries Uuid::nil() as its room, and folding
        // it switch_room(nil)'d the projection off the real room, wiping the
        // live view onto a phantom. A nil-room act must be a no-op.
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room = Uuid::from_u128(0x1);
        if let Some(ProjectionInput::Act(a)) = classify(
            PERSONA_ACT,
            &act_payload(
                room,
                Uuid::from_u128(0x2),
                Uuid::from_u128(0x3),
                "code/read",
                "a.py",
            ),
        ) {
            p.apply_act(a);
        }
        assert_eq!(current_chat(&substrate).acts.len(), 1);
        if let Some(ProjectionInput::Act(a)) = classify(
            PERSONA_ACT,
            &act_payload(
                Uuid::nil(),
                Uuid::from_u128(0x4),
                Uuid::from_u128(0x3),
                "code/shell",
                "pytest",
            ),
        ) {
            p.apply_act(a);
        }
        let view = current_chat(&substrate);
        assert_eq!(view.acts.len(), 1, "nil-room act must not fold or clear");
        assert_eq!(
            view.acts[0].tool, "code/read",
            "real room's receipts survive"
        );
    }

    /// A thin `chat:posted` payload — core message facts only, sender
    /// hardcoded to `0xc`. Mirrors the emitter contract (identity is NOT
    /// on the message; it resolves from the roster).
    fn posted(room: Uuid, msg: Uuid, text: &str) -> serde_json::Value {
        posted_from(room, msg, Uuid::from_u128(0xc), text)
    }

    /// A thin `chat:posted` payload with an explicit `sender_id` — used to
    /// prove roster-based identity resolution.
    fn posted_from(room: Uuid, msg: Uuid, sender: Uuid, text: &str) -> serde_json::Value {
        json!({
            "messageId": msg,
            "roomId": room,
            "senderId": sender,
            "content": text,
            "timestamp": 1_700_000_000_000u64,
        })
    }

    /// A `presence:updated` payload for `room` with a single member carrying
    /// `kind` + `integrations` — serialized from the real typed slot via the
    /// shared [`test_presence_payload`], never a hand-authored JSON literal.
    fn presence_one(
        room: Uuid,
        member: Uuid,
        name: &str,
        kind: &str,
        integrations: serde_json::Value,
    ) -> serde_json::Value {
        let kind: SenderKind = serde_json::from_value(json!({ "kind": kind })).unwrap();
        let integrations: BTreeMap<String, String> = serde_json::from_value(integrations).unwrap();
        test_presence_payload(
            room,
            vec![RosterSlotView {
                integrations,
                ..test_roster_slot(member, name, kind)
            }],
        )
    }

    fn current_chat(substrate: &Substrate) -> ChatViewState {
        let env = substrate
            .cache()
            .get(ChatViewState::KIND)
            .expect("a chat envelope must be stored");
        serde_json::from_value(env.payload.clone()).expect("payload is a ChatViewState")
    }

    // what this catches (#408): the per-room sink actually receiving data. The node
    // substrate holds ONE room's view — whichever wrote last — so a citizen standing
    // in an EARLIER room reads the wrong room or nothing. With the dual sink, each
    // room's own store keeps ITS view, which is what `ViewStateRagSource` reads.
    // If this regresses, per-room substrates exist but stay empty and every citizen
    // is blind again with nothing failing.
    #[tokio::test]
    async fn each_rooms_view_lands_in_that_rooms_own_store() {
        use continuum_positron::scoping::PerRoomSubstrates;
        let node = Substrate::new();
        let rooms = std::sync::Arc::new(PerRoomSubstrates::new());
        let mut p = ChatProjection::with_rooms(
            node.clone(),
            crate::experience::room_purpose::default_source(),
            Some(rooms.clone()),
        );

        let room_a = Uuid::from_u128(0xa);
        let room_b = Uuid::from_u128(0xb);
        // Two rooms speak, B last — so the node substrate ends on B.
        for (room, msg, text) in [
            (room_a, Uuid::from_u128(0x1), "hello from A"),
            (room_b, Uuid::from_u128(0x2), "hello from B"),
        ] {
            match classify(CHAT_POSTED, &posted(room, msg, text)).unwrap() {
                ProjectionInput::Message(m) => p.apply_message(m),
                _ => panic!("chat:posted must classify as a Message"),
            }
        }

        // PREMISE CHANGE (anti-wander): the node mirror holds the FIRST
        // adopted room — later rooms' events fold into their own stores and
        // never move the center pane; only nav/select does.
        assert_eq!(current_chat(&node).room_id, room_a);

        // But each room's OWN store kept its own view: room A is not lost.
        let read = |room: Uuid| -> ChatViewState {
            let env = rooms
                .for_room(room)
                .cache()
                .get(ChatViewState::KIND)
                .expect("each room keeps its own chat view");
            serde_json::from_value(env.payload.clone()).expect("a ChatViewState")
        };
        assert_eq!(read(room_a).room_id, room_a);
        assert_eq!(read(room_b).room_id, room_b);
        assert!(read(room_a).messages.iter().any(|m| m.content == "hello from A"));
        assert!(read(room_b).messages.iter().any(|m| m.content == "hello from B"));
    }

    #[test]
    fn message_event_projects_into_the_substrate() {
        // what this catches: regression where a chat:posted event does
        // not reach the substrate as a ChatViewState — the whole point
        // of the airc source wiring. Drives the pure fold (no bus) and
        // asserts the cache holds the projected message. With no presence
        // yet, the sender renders provisionally (short peer-id label,
        // neutral Human) and the room_name is unresolved.
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room = Uuid::from_u128(0xa);
        let m = Uuid::from_u128(0xb);
        match classify(CHAT_POSTED, &posted(room, m, "hi")).unwrap() {
            ProjectionInput::Message(msg) => p.apply_message(msg),
            _ => panic!("chat:posted must classify as a Message"),
        }
        let view = current_chat(&substrate);
        assert_eq!(view.room_id, room);
        assert_eq!(view.room_name, "", "room_name is unresolved until presence");
        assert_eq!(view.messages.len(), 1);
        assert_eq!(view.messages[0].id, m);
        assert_eq!(view.messages[0].content, "hi");
        assert_eq!(view.messages[0].sender_kind, SenderKind::Human);
        assert!(
            view.messages[0].sender_name.starts_with("peer-"),
            "provisional sender label, got {}",
            view.messages[0].sender_name
        );
        assert!(view.messages[0].integrations.is_empty());
    }

    #[test]
    fn persona_vitals_fold_into_the_member_slot_and_survive_presence() {
        // what this catches: design B's fold — a persona:vitals event must merge
        // its readouts into that member's roster slot BY ID, and (because vitals
        // live in their own map, not the presence roster) a later presence
        // snapshot that REPLACES the roster must not drop them. A member with no
        // vitals event keeps an empty map — no fabricated bars.
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room = Uuid::from_u128(0xa);
        let asha = Uuid::from_u128(0xb);

        // Roster arrives first — no vitals yet.
        match classify(
            PRESENCE_UPDATED,
            &presence_one(room, asha, "Asha", "agent", json!({})),
        )
        .unwrap()
        {
            ProjectionInput::Presence(u) => p.apply_presence(u),
            _ => panic!("presence:updated must classify as Presence"),
        }
        assert!(
            current_chat(&substrate).roster[0].vitals.is_empty(),
            "no vitals before any radiate"
        );

        // Asha radiates her vitals.
        let radiated = serde_json::to_value(PersonaVitalsUpdate {
            member_id: asha,
            vitals: BTreeMap::from([
                ("energy".to_string(), 80u8),
                ("attention".to_string(), 90u8),
            ]),
            loadout: None,
            genes: Vec::new(),
        })
        .unwrap();
        match classify(PERSONA_VITALS, &radiated).unwrap() {
            ProjectionInput::Vitals(v) => p.apply_vitals(v),
            _ => panic!("persona:vitals must classify as Vitals"),
        }
        let slot = current_chat(&substrate).roster.remove(0);
        assert_eq!(slot.vitals.get("energy"), Some(&80));
        assert_eq!(slot.vitals.get("attention"), Some(&90));

        // A fresh presence snapshot replaces the roster wholesale — vitals stay.
        match classify(
            PRESENCE_UPDATED,
            &presence_one(room, asha, "Asha", "agent", json!({})),
        )
        .unwrap()
        {
            ProjectionInput::Presence(u) => p.apply_presence(u),
            _ => panic!("presence:updated must classify as Presence"),
        }
        assert_eq!(
            current_chat(&substrate).roster[0].vitals.get("energy"),
            Some(&80),
            "vitals survive a roster refresh (they are keyed by id, not carried on presence)"
        );
    }

    #[test]
    fn persona_loadout_folds_into_the_slot_and_survives_a_vitals_only_tick() {
        // what this catches: a radiated LOADOUT (model · size · ctx) must fold
        // into the member's slot by id — the strip the tile draws — AND persist
        // as a durable capability fact: a later vitals-only tick (`loadout:
        // None`, the common 2s pulse) must NOT wipe the known model. A
        // regression that overwrote loadout with every tick would blank the
        // strip the instant vitals moved without a re-home.
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room = Uuid::from_u128(0xa);
        let asha = Uuid::from_u128(0xd);
        if let ProjectionInput::Presence(u) = classify(
            PRESENCE_UPDATED,
            &presence_one(room, asha, "Asha", "agent", json!({})),
        )
        .unwrap()
        {
            p.apply_presence(u);
        }

        // Radiate the model backing this persona.
        let radiated = serde_json::to_value(PersonaVitalsUpdate {
            member_id: asha,
            vitals: BTreeMap::new(),
            loadout: Some(Loadout {
                model: Some("devstral-24b".into()),
                params: Some(24_000_000_000),
                context_window: Some(32_768),
            }),
            genes: Vec::new(),
        })
        .unwrap();
        if let ProjectionInput::Vitals(v) = classify(PERSONA_VITALS, &radiated).unwrap() {
            p.apply_vitals(v);
        }
        let slot = current_chat(&substrate).roster.remove(0);
        let lo = slot.loadout.as_ref().expect("loadout folded into the slot");
        assert_eq!(lo.model.as_deref(), Some("devstral-24b"));
        assert_eq!(lo.params, Some(24_000_000_000));
        assert_eq!(lo.context_window, Some(32_768));

        // A later vitals-only tick (no loadout this sample) must keep the model.
        let vitals_only = serde_json::to_value(PersonaVitalsUpdate {
            member_id: asha,
            vitals: BTreeMap::from([("activity".to_string(), 50u8)]),
            loadout: None,
            genes: Vec::new(),
        })
        .unwrap();
        if let ProjectionInput::Vitals(v) = classify(PERSONA_VITALS, &vitals_only).unwrap() {
            p.apply_vitals(v);
        }
        let slot = current_chat(&substrate).roster.remove(0);
        assert_eq!(
            slot.loadout.as_ref().and_then(|l| l.context_window),
            Some(32_768),
            "a vitals-only tick must not clear the durable loadout"
        );
        assert_eq!(slot.vitals.get("activity"), Some(&50));
    }

    #[test]
    fn message_before_presence_is_provisional_then_upgrades() {
        // what this catches: the provisional-until-card contract — a
        // message posted before its sender's identity card arrives
        // renders provisionally, then UPGRADES in place (name + kind +
        // badges) the instant presence folds the card. A regression that
        // dropped the re-resolution would leave the sender stuck as
        // "peer-xxxx"/Human forever. [[fallbacks-are-illegal-fail-loud]].
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room = Uuid::from_u128(0xa);
        let sender = Uuid::from_u128(0xd);
        if let ProjectionInput::Message(m) = classify(
            CHAT_POSTED,
            &posted_from(room, Uuid::from_u128(0xb), sender, "hi"),
        )
        .unwrap()
        {
            p.apply_message(m);
        }
        // Provisional before the card.
        assert_eq!(
            current_chat(&substrate).messages[0].sender_kind,
            SenderKind::Human
        );
        // Card arrives via presence: Agent named Helper carrying a badge.
        let presence = presence_one(
            room,
            sender,
            "Helper",
            "agent",
            json!({ "continuum.persona_id": "helper-1" }),
        );
        if let ProjectionInput::Presence(u) = classify(PRESENCE_UPDATED, &presence).unwrap() {
            p.apply_presence(u);
        }
        let msg = &current_chat(&substrate).messages[0];
        assert_eq!(msg.sender_name, "Helper");
        assert_eq!(msg.sender_kind, SenderKind::Agent);
        assert_eq!(
            msg.integrations
                .get("continuum.persona_id")
                .map(String::as_str),
            Some("helper-1"),
            "opaque badge resolved from the card"
        );
    }

    #[test]
    fn message_after_presence_resolves_identity_from_the_roster() {
        // what this catches: regression where sender identity is not
        // resolved from the roster (the whole point of the thin emitter).
        // Presence first establishes the card, then a message from that
        // sender must render richly with no provisional phase.
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room = Uuid::from_u128(0xa);
        let sender = Uuid::from_u128(0xd);
        let presence = presence_one(room, sender, "Helper", "agent", json!({}));
        if let ProjectionInput::Presence(u) = classify(PRESENCE_UPDATED, &presence).unwrap() {
            p.apply_presence(u);
        }
        if let ProjectionInput::Message(m) = classify(
            CHAT_POSTED,
            &posted_from(room, Uuid::from_u128(0xb), sender, "hi"),
        )
        .unwrap()
        {
            p.apply_message(m);
        }
        let msg = &current_chat(&substrate).messages[0];
        assert_eq!(msg.sender_name, "Helper");
        assert_eq!(msg.sender_kind, SenderKind::Agent);
    }

    #[test]
    fn presence_event_projects_the_roster() {
        // what this catches: regression where presence:updated does not
        // fold into the roster — the second airc stream (outlier B,
        // maximally different from the message stream) proving the
        // accumulator holds two independent airc shapes on one kind, and
        // carries each member's neutral kind + opaque badges.
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room = Uuid::from_u128(0xa);
        let mut helper_badges = BTreeMap::new();
        helper_badges.insert("continuum.persona_id".to_string(), "helper-1".to_string());
        let payload = test_presence_payload(
            room,
            vec![
                RosterSlotView {
                    integrations: helper_badges,
                    ..test_roster_slot(Uuid::from_u128(0xd), "Helper", SenderKind::Agent)
                },
                RosterSlotView {
                    active: false,
                    ..test_roster_slot(Uuid::from_u128(0xe), "Operator", SenderKind::Human)
                },
            ],
        );
        match classify(PRESENCE_UPDATED, &payload).unwrap() {
            ProjectionInput::Presence(u) => p.apply_presence(u),
            _ => panic!("presence:updated must classify as Presence"),
        }
        let view = current_chat(&substrate);
        assert_eq!(view.roster.len(), 2);
        assert_eq!(view.roster[0].display_name, "Helper");
        assert_eq!(view.roster[0].kind, SenderKind::Agent);
        assert_eq!(
            view.roster[0]
                .integrations
                .get("continuum.persona_id")
                .map(String::as_str),
            Some("helper-1")
        );
        assert!(view.roster[0].active);
        assert_eq!(view.roster[1].kind, SenderKind::Human);
        assert!(!view.roster[1].active);
    }

    #[test]
    fn provenance_flows_from_presence_through_roster_to_message() {
        // what this catches: the accountability slot (task #38's
        // substrate seam) must ride the identity card end to end — a
        // presence slot's `provenance.runtime` lands on the roster entry
        // AND upgrades a provisional message's provenance in place. A
        // regression that dropped provenance from `apply_presence` /
        // `reresolve_messages` would leave every rendered row
        // unattributable — the exact leak the zero-trust flow doctrine
        // exists to close. A message with no card yet is honestly
        // unresolved (empty runtime), never a fabricated origin.
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room = Uuid::from_u128(0xa);
        let sender = Uuid::from_u128(0xd);
        // Message before the card → provenance honestly unresolved.
        if let ProjectionInput::Message(m) = classify(
            CHAT_POSTED,
            &posted_from(room, Uuid::from_u128(0xb), sender, "hi"),
        )
        .unwrap()
        {
            p.apply_message(m);
        }
        assert_eq!(
            current_chat(&substrate).messages[0].provenance.runtime,
            "",
            "unresolved provenance is empty, not fabricated"
        );
        // Presence folds the card carrying a runtime origin.
        let presence = test_presence_payload(
            room,
            vec![RosterSlotView {
                provenance: Provenance {
                    runtime: "claude".to_string(),
                },
                ..test_roster_slot(sender, "Helper", SenderKind::Agent)
            }],
        );
        if let ProjectionInput::Presence(u) = classify(PRESENCE_UPDATED, &presence).unwrap() {
            p.apply_presence(u);
        }
        let view = current_chat(&substrate);
        assert_eq!(
            view.roster[0].provenance.runtime, "claude",
            "roster carries the member's origin"
        );
        assert_eq!(
            view.messages[0].provenance.runtime, "claude",
            "the provisional message's provenance upgraded from the card"
        );
    }

    #[test]
    fn message_and_presence_compose_on_one_chat_view() {
        // what this catches: regression where the two streams clobber
        // each other instead of composing — a presence update must
        // preserve the message ring for the same room, and vice-versa.
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room = Uuid::from_u128(0xa);
        if let ProjectionInput::Message(m) =
            classify(CHAT_POSTED, &posted(room, Uuid::from_u128(0xb), "hi")).unwrap()
        {
            p.apply_message(m);
        }
        let presence = presence_one(room, Uuid::from_u128(0xd), "Helper", "agent", json!({}));
        if let ProjectionInput::Presence(u) = classify(PRESENCE_UPDATED, &presence).unwrap() {
            p.apply_presence(u);
        }
        let view = current_chat(&substrate);
        assert_eq!(view.messages.len(), 1, "roster update kept the message");
        assert_eq!(view.roster.len(), 1, "message left the roster intact");
    }

    #[test]
    fn redelivered_message_is_idempotent() {
        // what this catches: regression where a best-effort bus
        // redelivery double-appends the same message. Dedup by
        // message_id keeps the snapshot stable under redelivery.
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room = Uuid::from_u128(0xa);
        let m = Uuid::from_u128(0xb);
        for _ in 0..3 {
            if let ProjectionInput::Message(msg) =
                classify(CHAT_POSTED, &posted(room, m, "hi")).unwrap()
            {
                p.apply_message(msg);
            }
        }
        assert_eq!(current_chat(&substrate).messages.len(), 1);
    }

    #[test]
    fn multi_hop_replay_with_fresh_id_does_not_double_render() {
        // what this catches: #16 — a multi-hop replay re-emits the SAME logical
        // message with a FRESH message_id, which dedup-by-id alone cannot collapse.
        // It showed as doubled turns in every client (web/terminal/RAG). Content
        // identity (sender + content) collapses it, matching the cognition
        // content_hash admission dedup; a genuinely new message still appends.
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room = Uuid::from_u128(0xa);
        let sender = Uuid::from_u128(0xc);
        for id in [0x10u128, 0x11, 0x12] {
            if let ProjectionInput::Message(msg) = classify(
                CHAT_POSTED,
                &posted_from(room, Uuid::from_u128(id), sender, "hello team"),
            )
            .unwrap()
            {
                p.apply_message(msg);
            }
        }
        assert_eq!(
            current_chat(&substrate).messages.len(),
            1,
            "replay must collapse"
        );
        if let ProjectionInput::Message(msg) = classify(
            CHAT_POSTED,
            &posted_from(room, Uuid::from_u128(0x13), sender, "a new thought"),
        )
        .unwrap()
        {
            p.apply_message(msg);
        }
        assert_eq!(
            current_chat(&substrate).messages.len(),
            2,
            "new content still appends"
        );
    }

    #[test]
    fn switching_rooms_resets_the_accumulator() {
        // what this catches: regression where the single-cache-per-kind
        // model leaks room A's messages/roster into room B. Focusing a
        // new room must clear the prior room's ring + roster.
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room_a = Uuid::from_u128(0xa);
        let room_b = Uuid::from_u128(0xf);
        if let ProjectionInput::Message(m) =
            classify(CHAT_POSTED, &posted(room_a, Uuid::from_u128(0xb), "a")).unwrap()
        {
            p.apply_message(m);
        }
        // PREMISE CHANGE (2026-08-31 anti-wander): events no longer switch the
        // view — only explicit focus does. The clearing behavior under test is
        // unchanged; the switch is now the nav/select path.
        p.apply_focus(room_b);
        if let ProjectionInput::Message(m) =
            classify(CHAT_POSTED, &posted(room_b, Uuid::from_u128(0xc), "b")).unwrap()
        {
            p.apply_message(m);
        }
        let view = current_chat(&substrate);
        assert_eq!(view.room_id, room_b);
        assert_eq!(view.messages.len(), 1, "room A's message was cleared");
        assert_eq!(view.messages[0].content, "b");
    }

    #[test]
    fn switching_rooms_clears_vitals() {
        // what this catches: switch_room must clear the per-member vitals map,
        // or a member with the SAME id in a new room would inherit the prior
        // room's meters — a cross-room leak. The vitals twin of
        // switching_rooms_resets_the_accumulator.
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room_a = Uuid::from_u128(0xa);
        let room_b = Uuid::from_u128(0xf);
        let m = Uuid::from_u128(0xb);

        // Room A: member m present + radiating vitals.
        if let ProjectionInput::Presence(u) = classify(
            PRESENCE_UPDATED,
            &presence_one(room_a, m, "Asha", "agent", json!({})),
        )
        .unwrap()
        {
            p.apply_presence(u);
        }
        let radiated = serde_json::to_value(PersonaVitalsUpdate {
            member_id: m,
            vitals: BTreeMap::from([("energy".to_string(), 70u8)]),
            loadout: None,
            genes: Vec::new(),
        })
        .unwrap();
        if let ProjectionInput::Vitals(v) = classify(PERSONA_VITALS, &radiated).unwrap() {
            p.apply_vitals(v);
        }
        assert_eq!(
            current_chat(&substrate).roster[0].vitals.get("energy"),
            Some(&70)
        );

        // PREMISE CHANGE (2026-08-31, per-room accumulators): vitals are
        // MEMBER-truth, not room state — the same citizen standing in room B
        // carries the same live meters she radiated in room A. The old clear
        // was an artifact of the single shared accumulator.
        p.apply_focus(room_b);
        if let ProjectionInput::Presence(u) = classify(
            PRESENCE_UPDATED,
            &presence_one(room_b, m, "Asha", "agent", json!({})),
        )
        .unwrap()
        {
            p.apply_presence(u);
        }
        let view = current_chat(&substrate);
        assert_eq!(view.room_id, room_b);
        assert_eq!(
            view.roster[0].vitals.get("energy"),
            Some(&70),
            "a member's radiated vitals follow HER into every room she stands in"
        );
    }

    #[test]
    fn explicit_focus_switches_to_a_quiet_room_with_an_honest_empty_view() {
        // what this catches: the nav/select → chat seam. A `chat:focused` event
        // (built from the REAL AircChatFocused struct — the emitter's wire shape)
        // must refocus the accumulator onto the selected room, and a QUIET room
        // (no events observed since boot) must render the honest empty view —
        // room id set, no messages/roster/name — never room A's leftovers and
        // never a fabricated transcript. (`chat/history` backfill for quiet
        // rooms is the documented follow-up.)
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room_a = Uuid::from_u128(0xa);
        let quiet = Uuid::from_u128(0x40);
        if let ProjectionInput::Message(m) =
            classify(CHAT_POSTED, &posted(room_a, Uuid::from_u128(0xb), "hi")).unwrap()
        {
            p.apply_message(m);
        }
        let focus = serde_json::to_value(AircChatFocused { room_id: quiet }).unwrap();
        match classify(CHAT_FOCUSED, &focus).unwrap() {
            ProjectionInput::Focus(f) => p.apply_focus(f.room_id),
            _ => panic!("chat:focused must classify as Focus"),
        }
        let view = current_chat(&substrate);
        assert_eq!(view.room_id, quiet);
        assert!(
            view.messages.is_empty(),
            "quiet room renders empty, honestly"
        );
        assert!(view.roster.is_empty());
        assert_eq!(view.room_name, "", "no fabricated name before presence");
    }

    #[test]
    fn explicit_focus_pins_the_view_against_other_rooms_events() {
        // what this catches: the yank-back regression the presence-resync doc
        // warns about — once a citizen explicitly selects room B, a later
        // `chat:posted` or `presence:updated` in room A must NOT steal the
        // focused view (pre-select, the accumulator follows events; post-select
        // it is pinned). Events for the SELECTED room still fold in.
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room_a = Uuid::from_u128(0xa);
        let room_b = Uuid::from_u128(0xf);
        p.apply_focus(room_b);
        // Room A activity: must not refocus, must not fold.
        if let ProjectionInput::Message(m) =
            classify(CHAT_POSTED, &posted(room_a, Uuid::from_u128(0x1), "noise")).unwrap()
        {
            p.apply_message(m);
        }
        if let ProjectionInput::Presence(u) = classify(
            PRESENCE_UPDATED,
            &presence_one(room_a, Uuid::from_u128(0xd), "Helper", "agent", json!({})),
        )
        .unwrap()
        {
            p.apply_presence(u);
        }
        let view = current_chat(&substrate);
        assert_eq!(
            view.room_id, room_b,
            "pinned focus survives other rooms' events"
        );
        assert!(view.messages.is_empty());
        assert!(view.roster.is_empty());
        // The selected room's own events fold in normally.
        if let ProjectionInput::Message(m) =
            classify(CHAT_POSTED, &posted(room_b, Uuid::from_u128(0x2), "here")).unwrap()
        {
            p.apply_message(m);
        }
        if let ProjectionInput::Presence(u) = classify(
            PRESENCE_UPDATED,
            &presence_one(room_b, Uuid::from_u128(0xe), "Asha", "agent", json!({})),
        )
        .unwrap()
        {
            p.apply_presence(u);
        }
        let view = current_chat(&substrate);
        assert_eq!(view.messages.len(), 1);
        assert_eq!(view.messages[0].content, "here");
        assert_eq!(view.roster.len(), 1);
        // A second select moves the pin — focus is switchable, not one-way.
        p.apply_focus(room_a);
        assert_eq!(current_chat(&substrate).room_id, room_a);
    }

    #[test]
    fn foreign_and_malformed_events_are_skipped() {
        // what this catches: regression where a non-chat event or a
        // chat:posted missing required identity fields gets partially
        // rendered (fabricated identity) instead of skipped. Per
        // [[fallbacks-are-illegal-fail-loud]]: an event that can't
        // deserialize into the contract is not-a-chat-event → None.
        assert!(classify("media:frame", &json!({ "bytes": 4 })).is_none());
        // chat:posted missing the core message facts (ids / timestamp) →
        // not a renderable message.
        assert!(classify(CHAT_POSTED, &json!({ "content": "hi" })).is_none());
    }

    #[test]
    fn revision_advances_monotonically_across_stores() {
        // what this catches: regression where successive projections
        // stamp a stale or non-monotonic revision — the session-protocol
        // last_seen replay routes by revision, so a flat revision would
        // stop the renderer from seeing updates.
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room = Uuid::from_u128(0xa);
        if let ProjectionInput::Message(m) =
            classify(CHAT_POSTED, &posted(room, Uuid::from_u128(0x1), "one")).unwrap()
        {
            p.apply_message(m);
        }
        let r1 = substrate.cache().get(ChatViewState::KIND).unwrap().revision;
        if let ProjectionInput::Message(m) =
            classify(CHAT_POSTED, &posted(room, Uuid::from_u128(0x2), "two")).unwrap()
        {
            p.apply_message(m);
        }
        let r2 = substrate.cache().get(ChatViewState::KIND).unwrap().revision;
        assert!(r2 > r1, "revision must advance: {r1:?} -> {r2:?}");
    }
