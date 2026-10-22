#![cfg(test)]
    use super::*;
    use airc_core::PeerId;
    use airc_lib::AircError;
    use airc_lib::RoomMember;
    use async_trait::async_trait;
    // The module proper no longer names `SenderKind` (the coarse kind is
    // derived inside the shared `roster_slot_from_member` projection now);
    // the tests still assert on the projected slot's `kind`.
    use continuum_positron::SenderKind;

    /// A roster reader that returns a fixed member list — enough to drive
    /// [`emit_once`]'s read → project → publish path without a daemon.
    struct StubRosterReader {
        members: Vec<airc_lib::RoomMemberCard>,
    }

    #[async_trait]
    impl AircRosterReader for StubRosterReader {
        fn self_peer_id(&self) -> PeerId {
            PeerId::new()
        }

        async fn room_roster(
            &self,
            _within: Duration,
            _window: usize,
            _room: Option<uuid::Uuid>,
        ) -> Result<Vec<RoomMember>, AircError> {
            Ok(self
                .members
                .iter()
                .map(|c| RoomMember {
                    peer_id: c.peer_id,
                    display_name: c.identity.as_ref().map(|i| i.name.clone()),
                    runtime: c.runtime.clone(),
                    availability: c.availability,
                    last_seen_ms: c.last_seen_ms,
                })
                .collect())
        }

        async fn room_roster_cards(
            &self,
            _within: Duration,
            _window: usize,
            _room: Option<uuid::Uuid>,
        ) -> Result<Vec<airc_lib::RoomMemberCard>, AircError> {
            Ok(self.members.clone())
        }
    }

    /// Build a `RoomMember` — `name: Some` mirrors a peer that published
    /// an identity card; `None` mirrors present-but-unnamed. Mirrors the
    /// `room_roster_source` test builder so both sides describe the same
    /// airc shape.
    fn member(peer: PeerId, runtime: &str, name: Option<&str>) -> airc_lib::RoomMemberCard {
        airc_lib::RoomMemberCard {
            peer_id: peer,
            runtime: runtime.to_string(),
            availability: None,
            last_seen_ms: 1_000_000,
            identity: name.map(|s| airc_core::identity::Identity::new(s.to_string())),
        }
    }

    // what this catches: the identity card the emitter produces must
    // carry the member's verifiable origin (provenance.runtime, verbatim)
    // AND a coarse neutral kind — the accountability guarantee Commit 2
    // exists to weave in. A regression that dropped provenance, or that
    // string-matched runtime into kind instead of carrying it whole,
    // would leave every rendered row unattributable.
    #[test]
    fn project_carries_origin_verbatim_and_coarse_kind() {
        let named = PeerId::new();
        let unnamed = PeerId::new();
        let human = PeerId::new();
        let update = project_presence(
            vec![
                member(named, "claude", Some("win-claude")),
                member(unnamed, "codex", None),
                member(human, "interactive", Some("Operator")),
            ],
            Uuid::from_u128(0xa),
            "general".into(),
            &HashMap::new(),
        );
        assert_eq!(update.roster.len(), 3, "every present member is a slot");

        let card = &update.roster[0];
        assert_eq!(card.member_id, named.as_uuid());
        assert_eq!(card.display_name, "win-claude");
        assert_eq!(card.kind, SenderKind::Agent);
        assert_eq!(
            card.provenance.runtime, "claude",
            "the full runtime origin is carried verbatim, not lost to the coarse kind"
        );
        assert!(
            card.integrations.is_empty(),
            "no airc badge map yet — empty is honest, not fabricated"
        );
        assert!(card.active, "a present member is active");

        // Unnamed peer → the SAME provisional short-peer label the
        // consumer uses; still an Agent by its codex runtime.
        let anon = &update.roster[1];
        assert!(
            anon.display_name.starts_with("peer-"),
            "unnamed citizen is labelled, never invisible"
        );
        assert_eq!(anon.kind, SenderKind::Agent);
        assert_eq!(anon.provenance.runtime, "codex");

        // The one interactive runtime → Human (the coarse hint), origin
        // still whole in provenance.
        let carbon = &update.roster[2];
        assert_eq!(carbon.kind, SenderKind::Human);
        assert_eq!(carbon.provenance.runtime, "interactive");
    }

    // what this catches: #258/#262 grey-not-gone (Joel 2026-07-30: "Why won't
    // bigmama's persona ever show up?"). (1) A remembered member absent from
    // the live read joins the published roster as an `active: false` ghost
    // with stale liveness signals (availability/vitals) cleared — never
    // vanished, never lying about liveness. (2) A real display name adopted
    // once never regresses to the provisional peer label on a card-less
    // sighting. (3) A member reappearing live replaces its ghost. Before
    // this, anyone silent >120s ceased to exist in the roster — BigMama's
    // citizens were perpetually unborn on this node.
    #[test]
    fn directory_keeps_offline_members_grey_and_names_never_regress() {
        let kimi = PeerId::new();
        let local = PeerId::new();
        // Sighting 1: Kimi live, named (her card crossed once).
        let named = crate::ipc::positron_source::roster_slot_from_card(&member(
            kimi,
            "persona",
            Some("Kimi"),
        ));
        let mut dir = HashMap::new();
        assert!(fold_into_directory(&mut dir, std::slice::from_ref(&named)));

        // Her node drops off: live read has only the local member.
        let live = vec![crate::ipc::positron_source::roster_slot_from_card(&member(
            local,
            "interactive",
            Some("Operator"),
        ))];
        let published = union_with_directory(live.clone(), &dir);
        let ghost = published
            .iter()
            .find(|s| s.member_id == kimi.as_uuid())
            .expect("remembered member must stay in the roster");
        assert!(!ghost.active, "unreachable member renders grey, not gone");
        assert_eq!(
            ghost.display_name, "Kimi",
            "adopted identity survives the outage"
        );
        assert!(
            ghost.availability.is_none() && ghost.vitals.is_empty(),
            "stale liveness signals cleared — the roster never lies about liveness"
        );
        assert!(published[0].active, "active members sort before ghosts");

        // Later sighting WITHOUT a card (provisional label) must not erase her name.
        let unnamed =
            crate::ipc::positron_source::roster_slot_from_card(&member(kimi, "persona", None));
        fold_into_directory(&mut dir, std::slice::from_ref(&unnamed));
        assert_eq!(
            dir[&kimi.as_uuid()].display_name,
            "Kimi",
            "a card-less sighting never regresses an adopted name"
        );

        // She reappears live: the ghost is replaced by the live slot.
        let back = vec![crate::ipc::positron_source::roster_slot_from_card(&member(
            kimi,
            "persona",
            Some("Kimi"),
        ))];
        let published = union_with_directory(back, &dir);
        assert_eq!(
            published
                .iter()
                .filter(|s| s.member_id == kimi.as_uuid())
                .count(),
            1
        );
        assert!(
            published
                .iter()
                .find(|s| s.member_id == kimi.as_uuid())
                .unwrap()
                .active
        );
    }

    // what this catches: the avatar-image enrichment — a peer with a stored
    // `<uuid>.png` carries its URL on the slot, everyone else stays honestly
    // absent (glyph fallback, never a broken image). And the store scan maps
    // ONLY uuid-named .png files — named art / emote subdirs are not member
    // avatars and must not leak onto arbitrary slots.
    #[test]
    fn avatar_url_enriches_only_members_with_a_stored_image() {
        let pictured = PeerId::new();
        let plain = PeerId::new();
        let mut avatars = HashMap::new();
        avatars.insert(
            pictured.as_uuid(),
            format!("/avatars/{}.png", pictured.as_uuid()),
        );
        let update = project_presence(
            vec![
                member(pictured, "claude", Some("win-claude")),
                member(plain, "codex", None),
            ],
            Uuid::from_u128(0xd),
            "general".into(),
            &avatars,
        );
        assert_eq!(
            update.roster[0].avatar_url.as_deref(),
            Some(format!("/avatars/{}.png", pictured.as_uuid()).as_str()),
            "a stored avatar rides the slot as its URL"
        );
        assert_eq!(
            update.roster[1].avatar_url, None,
            "no stored image → honest absent, never a fabricated face"
        );

        // The store scan: uuid-named .png only.
        let dir = std::env::temp_dir().join(format!("avatar-scan-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("mk tmp store");
        let id = Uuid::new_v4();
        std::fs::write(dir.join(format!("{id}.png")), b"png").expect("write");
        std::fs::write(dir.join("asha.png"), b"png").expect("write");
        std::fs::write(dir.join(format!("{id}-happy.png")), b"png").expect("write");
        std::fs::create_dir_all(dir.join("emote")).expect("subdir");
        let map = scan_avatar_store(&dir);
        assert_eq!(
            map.len(),
            1,
            "only exact <uuid>.png names are member avatars"
        );
        assert_eq!(
            map.get(&id).map(String::as_str),
            Some(format!("/avatars/{id}.png").as_str())
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    // what this catches: the widget roster must include self — unlike the
    // persona-grounding roster (which drops self). A regression that
    // copied the self-exclusion here would erase the local user from
    // their own room roster.
    #[test]
    fn self_is_included_in_the_widget_roster() {
        let me = PeerId::new();
        let update = project_presence(
            vec![member(me, "interactive", Some("Operator"))],
            Uuid::from_u128(0xb),
            "general".into(),
            &HashMap::new(),
        );
        assert_eq!(update.roster.len(), 1);
        assert_eq!(update.roster[0].member_id, me.as_uuid());
    }

    // what this catches: the emitter's serialized output must survive a
    // round-trip through the exact wire structs the consumer
    // deserializes — the "both sides agree by construction" contract. A
    // divergence (e.g. a rename_all mismatch or a non-serde field) would
    // silently drop the roster on the consumer side.
    #[test]
    fn serialized_update_round_trips_the_wire_shape() {
        let update = project_presence(
            vec![member(PeerId::new(), "claude", Some("win-claude"))],
            Uuid::from_u128(0xc),
            "general".into(),
            &HashMap::new(),
        );
        let json = serde_json::to_value(&update).expect("serializes");
        let back: AircPresenceUpdate = serde_json::from_value(json).expect("round-trips");
        assert_eq!(update, back);
    }

    // what this catches: the late-subscriber gap (#118). The idle tick
    // dedups an unchanged roster (publishes nothing on the second call);
    // a resync cue must set `force` and re-publish the SAME roster anyway,
    // so a projector that booted after the emitter's one-and-only publish
    // still receives presence. A regression that let the dedup swallow a
    // forced emit would leave that projector roster-empty forever.
    #[tokio::test]
    async fn resync_forces_republish_of_unchanged_roster() {
        let reader: Arc<dyn AircRosterReader> = Arc::new(StubRosterReader {
            members: vec![member(PeerId::new(), "claude", Some("win-claude"))],
        });
        let bus = MessageBus::new();
        let mut rx = bus.receiver();
        let room = Uuid::from_u128(0x1);
        let mut last = None;
        let mut dir = HashMap::new();

        // First idle emit publishes (roster changed None → Some).
        emit_once(
            &reader,
            room,
            "general",
            &bus,
            &mut last,
            &HashMap::new(),
            &mut dir,
            false,
        )
        .await;
        let first = rx
            .try_recv()
            .expect("first emit publishes presence:updated");
        assert_eq!(first.name, PRESENCE_UPDATED);

        // Second idle emit dedups — nothing published.
        emit_once(
            &reader,
            room,
            "general",
            &bus,
            &mut last,
            &HashMap::new(),
            &mut dir,
            false,
        )
        .await;
        assert!(
            rx.try_recv().is_err(),
            "an unchanged roster dedups on the idle path"
        );

        // A resync cue forces a re-publish of the unchanged roster.
        emit_once(
            &reader,
            room,
            "general",
            &bus,
            &mut last,
            &HashMap::new(),
            &mut dir,
            true,
        )
        .await;
        let forced = rx
            .try_recv()
            .expect("a resync forces a re-publish even when the roster is unchanged");
        assert_eq!(forced.name, PRESENCE_UPDATED);
    }

    // what this catches: the consumer-side half of the cue — a booting
    // projector must actually emit `presence:resync` on the bus so the
    // emitter's select arm fires. A regression that renamed the event or
    // dropped the publish would silently reopen the late-subscriber gap.
    #[tokio::test]
    async fn request_presence_resync_publishes_the_cue() {
        let bus = MessageBus::new();
        let mut rx = bus.receiver();
        request_presence_resync(&bus);
        let cue = rx
            .try_recv()
            .expect("request_presence_resync publishes a cue event");
        assert_eq!(cue.name, PRESENCE_RESYNC);
    }
