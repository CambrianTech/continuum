#![cfg(test)]
    use super::*;

    /// A canned reader — returns a fixed snapshot, the nav analogue of the
    /// kanban `StubReader`.
    // what this catches: nesting is GENERIC — an activity whose binding names a parent
    // nests under it in the nav even when it is not a bench solve room. Before
    // 2026-09-03 only solve rooms nested (under their run room, via the bench tracker);
    // a dispatched run room was a flat top-level tab, or absent (Joel: "active benchmark
    // rooms don't even show up… not under the base academy room").
    #[test]
    fn a_bound_activity_nests_under_its_binding_parent() {
        let academy = Uuid::new_v4();
        let run = Uuid::new_v4();
        let mut a = room(&run.to_string(), "bench-swe-bench-verified-1", 0, 0);
        a.parent = Some(academy.to_string());
        let snap = NavSnapshot { current: None, activities: vec![a], bookmarks: vec![] };
        let view = project_nav(Uuid::new_v4(), snap);
        assert_eq!(view.open_tabs[0].parent_ref, academy.to_string());
    }

    // what this catches: the strip's membership — a selected ROOM reads opened
    // (and is remembered until closed) yet never surfaces as an EXTRA tab, which
    // would duplicate the room-set fold's row (49 identical tabs, live 2026-09-03).
    #[test]
    fn a_room_select_opens_the_tab_without_duplicating_the_room_set_row() {
        let focus = NavFocus::default();
        let user = Uuid::new_v4();
        let room = Uuid::new_v4().to_string();
        let persona = Uuid::new_v4().to_string();
        assert!(!focus.is_open(user, &room));
        focus.focus(user, room.clone(), NavTargetKind::Room);
        focus.focus(user, persona.clone(), NavTargetKind::Persona);
        assert!(focus.is_open(user, &room));
        assert!(focus.is_open(user, &persona));
        let extra = focus.open_activities(user);
        assert_eq!(extra, vec![(persona.clone(), NavTargetKind::Persona)]);
        focus.close(user, &room);
        assert!(!focus.is_open(user, &room));
        let snap = NavSnapshot {
            current: Some(room.clone()),
            activities: vec![room_opened(&room, true), room_opened(&persona, false)],
            bookmarks: vec![],
        };
        let view = project_nav(user, snap);
        assert!(view.open_tabs[0].opened);
        assert!(!view.open_tabs[1].opened);
    }

    fn room_opened(id: &str, opened: bool) -> NavActivity {
        let mut a = room(id, "r", 0, 0);
        a.opened = opened;
        a
    }

    struct StubNav(NavSnapshot);
    impl NavReader for StubNav {
        fn nav_snapshot(&self, _user: Uuid) -> NavSnapshot {
            self.0.clone()
        }
    }

    fn room(id: &str, title: &str, unread: u32, last_read: i64) -> NavActivity {
        NavActivity {
            id: id.into(),
            title: title.into(),
            kind: NavTargetKind::Room,
            unread,
            purpose: "chat".into(),
            last_read: Some(last_read),
            parent: None,
            opened: false,
        }
    }

    // what this catches: the projection carries current_tab, derives the
    // per-room last_read map from the activities (one source, not stored twice),
    // and preserves each tab's unread count + order.
    #[test]
    fn projects_snapshot_into_navviewstate_with_read_map() {
        let snap = NavSnapshot {
            current: Some("room-a".into()),
            activities: vec![
                room("room-a", "General", 0, 1_700_000_000_000),
                room("room-b", "Code", 4, 1_699_000_000_000),
            ],
            bookmarks: vec![NavBookmark {
                target: "room-a".into(),
                label: "General".into(),
                kind: NavTargetKind::Room,
            }],
        };
        let view = project_nav(Uuid::from_u128(7), snap);
        assert_eq!(view.user_id, Uuid::from_u128(7));
        assert_eq!(view.current_tab.as_deref(), Some("room-a"));
        assert_eq!(view.open_tabs.len(), 2);
        assert_eq!(view.open_tabs[1].id, "room-b");
        assert_eq!(view.open_tabs[1].unread, 4);
        assert_eq!(view.last_read.get("room-a"), Some(&1_700_000_000_000));
        assert_eq!(view.last_read.get("room-b"), Some(&1_699_000_000_000));
        assert_eq!(view.bookmarks.len(), 1);
    }

    // what this catches: the LIVE reader surfaces the SAME split point the
    // citizen's own window was built at — the digest's `bookmark`. One value, two
    // consumers (her perception and the nav view), so they cannot disagree. The
    // durable cursor behind it is airc's `runtime_cursor`; the staged digest is
    // what this sync projection can see of it.
    #[test]
    fn live_reader_reflects_the_real_shared_bookmark() {
        use crate::cognition::channel_substrate::global_channel_digest_buffer;
        use crate::runtime::ready_buffer::ReadyBuffer;
        // Unique ids so the process-global digest buffer can't collide with
        // another test staging a different (user, room).
        let asha = Uuid::from_u128(0xa54a_u128);
        let room = Uuid::from_u128(0x9e21_u128);
        global_channel_digest_buffer().publish(
            (asha, room),
            std::sync::Arc::new(crate::cognition::channel_digest::ChannelDigest {
                room_id: room,
                persona_id: asha,
                bookmark: 42,
                elements: Vec::new(),
                unread_start: 0,
            }),
        );
        let reader = ChannelBookmarksNavReader::fixed(vec![(room, "General".into())]);
        let snap = reader.nav_snapshot(asha);
        let view = project_nav(asha, snap);
        assert_eq!(view.open_tabs.len(), 1);
        assert_eq!(view.open_tabs[0].title, "General");
        assert_eq!(view.last_read.get(&room.to_string()), Some(&42));
        assert_eq!(view.current_tab, Some(room.to_string()));
    }

    // what this catches: an empty workspace projects to an honest empty nav
    // view (no fabricated default tab), and the reader seam drives it.
    #[test]
    fn empty_snapshot_projects_honest_empty_nav() {
        let view = project_nav(
            Uuid::from_u128(9),
            StubNav(NavSnapshot::default()).nav_snapshot(Uuid::from_u128(9)),
        );
        assert!(view.current_tab.is_none());
        assert!(view.open_tabs.is_empty());
        assert!(view.last_read.is_empty());
        assert!(view.bookmarks.is_empty());
    }

    // what this catches: the room-set fold's change discipline — a chat event
    // registers an unseen room (empty title), presence names it, repeats of
    // either are NOT modifications (send_if_modified must not wake subscribers
    // on a no-op), and a rename upgrades the title.
    #[test]
    fn room_set_fold_registers_names_and_skips_noops() {
        let room = Uuid::from_u128(0xf00d);
        let mut set = RoomSet::new();
        assert!(
            fold_observed_room(&mut set, room, None),
            "first sighting registers"
        );
        assert_eq!(set.get(&room).map(String::as_str), Some(""));
        assert!(
            !fold_observed_room(&mut set, room, None),
            "repeat chat = no-op"
        );
        assert!(
            fold_observed_room(&mut set, room, Some("general".into())),
            "presence names the room"
        );
        assert_eq!(set.get(&room).map(String::as_str), Some("general"));
        assert!(
            !fold_observed_room(&mut set, room, Some("general".into())),
            "same name = no-op"
        );
        assert!(
            fold_observed_room(&mut set, room, Some("general-2".into())),
            "rename upgrades"
        );
        assert!(
            !fold_observed_room(&mut set, room, Some(String::new())),
            "empty name never erases a known room"
        );
        assert_eq!(set.get(&room).map(String::as_str), Some("general-2"));
    }

    // what this catches: the explicit-focus half of the reader — after a
    // `nav/select` write lands in the shared NavFocus store, `current` must
    // surface THAT room (not the first-room stand-in), and a citizen who never
    // selected keeps the first-room fallback. The read half of the seam the
    // nav/select command's own test drives the write half of.
    #[test]
    fn explicit_focus_beats_the_first_room_as_current() {
        // Unique user ids: the focus store is process-global, so a shared id
        // would collide with a parallel test's write.
        let room_a = Uuid::from_u128(0x50a);
        let room_b = Uuid::from_u128(0x50b);
        let rooms = vec![
            (room_a, "General".to_string()),
            (room_b, "Code".to_string()),
        ];
        let reader = ChannelBookmarksNavReader::fixed(rooms.clone());

        let fresh = Uuid::from_u128(0x50f1);
        assert_eq!(
            reader.nav_snapshot(fresh).current,
            Some(room_a.to_string()),
            "no select yet → first room stands in"
        );

        let selector = Uuid::from_u128(0x50f2);
        global_nav_focus().focus(selector, room_b.to_string(), NavTargetKind::Room);
        assert_eq!(
            reader.nav_snapshot(selector).current,
            Some(room_b.to_string()),
            "the explicit focus is surfaced as current"
        );
    }

    // what this catches: a persona-kind focus surfaces as its OWN open tab —
    // kind Persona, purpose "persona", title resolved from the live member-name
    // fold (the presence display_name, never a fabricated label) — and becomes
    // current WITHOUT displacing any room tab. The honest first slice of the
    // persona home: the tab is nav truth, the chat projection stays untouched.
    #[test]
    fn persona_focus_surfaces_a_persona_tab_with_resolved_name() {
        let room = Uuid::from_u128(0x9e50_a);
        let asha = Uuid::from_u128(0x9e50_b);
        let reader = ChannelBookmarksNavReader::fixed_with_members(
            vec![(room, "General".into())],
            vec![(asha, "Asha".into())],
        );
        let user = Uuid::from_u128(0x9e50_c);
        global_nav_focus().focus(user, asha.to_string(), NavTargetKind::Persona);
        let snap = reader.nav_snapshot(user);
        assert_eq!(snap.current, Some(asha.to_string()));
        let persona_tab = snap
            .activities
            .iter()
            .find(|a| a.kind == NavTargetKind::Persona)
            .expect("persona tab surfaced");
        assert_eq!(persona_tab.title, "Asha", "title from the member fold");
        assert_eq!(persona_tab.purpose, "persona");
        assert_eq!(persona_tab.unread, 0);
        // The room tab is still there — a persona tab ADDS, never displaces.
        assert!(snap
            .activities
            .iter()
            .any(|a| a.kind == NavTargetKind::Room));
    }

    // what this catches: a persona focus whose name the fold hasn't observed
    // yet gets the honest short-id label — same discipline as an unnamed room,
    // never an invisible or fabricated title.
    #[test]
    fn unnamed_persona_focus_gets_short_id_label() {
        let room = Uuid::from_u128(0x9e51_a);
        let stranger = Uuid::from_u128(0x9e51_b);
        let reader = ChannelBookmarksNavReader::fixed(vec![(room, "General".into())]);
        let user = Uuid::from_u128(0x9e51_c);
        global_nav_focus().focus(user, stranger.to_string(), NavTargetKind::Persona);
        let snap = reader.nav_snapshot(user);
        let tab = snap
            .activities
            .iter()
            .find(|a| a.kind == NavTargetKind::Persona)
            .expect("persona tab surfaced");
        assert_eq!(
            tab.title,
            airc_core::shown_id::shown_form(stranger)
        );
    }

    // what this catches: a room the fold has seen but presence hasn't named
    // renders an honest short-id tab label, never an invisible empty title.
    #[test]
    fn unnamed_room_gets_short_id_tab_label() {
        let room = Uuid::from_u128(0xbeef);
        let reader = ChannelBookmarksNavReader::fixed(vec![(room, String::new())]);
        let snap = reader.nav_snapshot(Uuid::from_u128(11));
        assert_eq!(snap.activities.len(), 1);
        assert_eq!(snap.activities[0].title, airc_core::shown_id::shown_form(room));
    }

    // what this catches: the registry spawns a citizen's projector exactly once
    // — the second ensure() for the same citizen must not double-spawn (nav is
    // per-citizen process-lifetime state, not per-connection state).
    #[tokio::test]
    async fn registry_ensures_a_projector_once_per_citizen() {
        let per_user = Arc::new(PerUserSubstrates::new());
        let registry = NavProjectorRegistry::new(
            Arc::new(crate::runtime::MessageBus::new()),
            Arc::clone(&per_user),
            Arc::new(ChannelBookmarksNavReader::fixed(vec![(
                Uuid::from_u128(0xcafe),
                "general".into(),
            )])),
        );
        let me = Uuid::from_u128(0xa11ce);
        registry.ensure(me);
        registry.ensure(me);
        assert_eq!(
            registry.spawned.lock().expect("registry lock").len(),
            1,
            "one citizen, one projector"
        );
        // The spawned projector's immediate reload lands the citizen's nav view
        // in THEIR substrate — poll briefly (spawn is async wrt this test).
        let substrate = per_user.for_citizen(me);
        let mut seen = false;
        for _ in 0..50 {
            if substrate.cache().get(NavViewState::KIND).is_some() {
                seen = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(
            seen,
            "the citizen's nav view materialized in their per-user substrate"
        );
    }
    // what this catches: a fresh session must open on the academy base, never on whichever
    // room happens to sort first by id (the paused mini round the desktop landed on twice).
    #[test]
    fn a_session_with_no_focus_lands_on_the_academy_when_the_node_has_one() {
        let mut rooms = RoomSet::new();
        let mini = Uuid::new_v4();
        let academy = Uuid::new_v4();
        rooms.insert(mini, "bench-swe-bench-verified-mini-1788398099".to_string());
        rooms.insert(academy, crate::persona::airc_runtime::CITIZEN_COMMONS_ROOM.to_string());
        assert_eq!(landing_room(&rooms), Some(academy.to_string()));
        rooms.remove(&academy);
        assert_eq!(landing_room(&rooms), Some(mini.to_string()), "no academy → the first room, never None");
        assert_eq!(landing_room(&RoomSet::new()), None);
    }
