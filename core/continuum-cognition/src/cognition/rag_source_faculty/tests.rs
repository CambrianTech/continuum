#![cfg(test)]
    use super::*;
    use crate::cognition::rag_budget::{RagDelivery, RagItem};
    use std::sync::Mutex;

    fn persona() -> Uuid {
        Uuid::parse_str("00000000-0000-0000-0000-000000000aaa").unwrap()
    }

    /// A stub RagSource that returns canned items and records the persona_id it
    /// was delivered for (to prove the bridge passes scope through).
    struct StubSource {
        id: &'static str,
        items: Vec<RagItem>,
        seen_persona: Mutex<Option<Uuid>>,
        seen_room: Mutex<Option<Uuid>>,
    }

    impl StubSource {
        fn new(id: &'static str, contents: &[&str]) -> Self {
            let items = contents
                .iter()
                .map(|c| RagItem {
                    key: String::new(),
                    summary: None,
                    pointer: None,
                    content: c.to_string(),
                    tokens: continuum_base::token_estimate::estimate_prompt_tokens(c),
                    metadata: serde_json::json!({}),
                })
                .collect();
            Self {
                id,
                items,
                seen_persona: Mutex::new(None),
                seen_room: Mutex::new(None),
            }
        }
    }

    #[async_trait]
    impl RagSource for StubSource {
        fn source_id(&self) -> &'static str {
            self.id
        }

        fn expand_command(&self) -> Option<&'static str> {
            // Test/stub source — nothing further to fetch.
            None
        }

        /// Test/stub source.
        async fn deliver(&self, ctx: &RagContext) -> RagDelivery {
            *self.seen_persona.lock().unwrap() = Some(ctx.persona_id);
            *self.seen_room.lock().unwrap() = ctx.airc_room.map(|room| room.as_uuid());
            let tokens_used = self.items.iter().map(|i| i.tokens).sum();
            RagDelivery {
                source_id: self.id.to_string(),
                items: self.items.clone(),
                tokens_used,
                truncated: false,
            }
        }
    }

    // what this catches: an empty delivery becoming an empty bid, or content not bidding: an
    // optional source with nothing abstains, and one with content bids. A REQUIRED source cannot
    // be empty at all (`CriticalSource` returns `Grounded`), so there is no absence to account.
    #[tokio::test]
    async fn contribute_abstains_on_empty_and_bids_on_content() {
        let new = |items: &[&'static str]| {
            RagSourceFaculty::new(persona(), Arc::new(StubSource::new("room-doctrine", items)), SaliencePolicy::StandingFraming)
                .with_clock(Arc::new(|| 1_000))
        };
        assert!(new(&[]).contribute(&Workspace::new("anything")).await.is_none(), "an empty delivery is no bid");
        assert!(
            new(&["This room is for coordination."]).contribute(&Workspace::new("anything")).await.is_some(),
            "a source with content bids"
        );
    }

    // what this catches: ANY RagSource is lifted into a perception-tier context
    // Contribution — the delivery's items become the bid content, the faculty id
    // is the source id, and it carries no Decision (grounding is context, never a
    // verdict). This is the regression fix: roster/doctrine reach the decider.
    #[tokio::test]
    async fn lifts_a_rag_source_into_a_context_bid() {
        let source = Arc::new(StubSource::new(
            "room-roster",
            &["Aria [persona]", "win-claude [claude] — Busy"],
        ));
        let faculty = RagSourceFaculty::new(persona(), source, SaliencePolicy::StandingFraming)
            .with_clock(Arc::new(|| 1_000));

        assert!(
            !faculty.reacts_to_broadcast(),
            "grounding is perception tier — it bids in phase 1, before deliberation"
        );
        assert_eq!(faculty.id(), FacultyId::Custom("room-roster".to_string()));

        let c = faculty
            .contribute(&Workspace::new("who's around?"))
            .await
            .expect("a non-empty source must bid");
        assert_eq!(c.faculty, FacultyId::Custom("room-roster".to_string()));
        assert!(
            c.decision.is_none(),
            "grounding is context, never a verdict"
        );
        assert!(c.content.contains("Aria [persona]"));
        assert!(c.content.contains("win-claude [claude] — Busy"));
        assert!(c.salience > 0.0);
        assert!(
            c.standing_grounding,
            "the assembler contract reaches the prompt budget"
        );
    }

    // what this catches: the KV routing contract. EVERY grounding bid rides her view
    // (`trailing`), never the system message: volatile content because its churn
    // re-prefilled every token after it (2026-08-23, live KV reuse 18-33%), and
    // standing content because it is the ROOM's while the system message is hers in
    // every room — a doctrine there rewrote its end on each room switch (2026-10-23).
    // `stable` survives as a fact about the bytes, and importance is never erased.
    #[tokio::test]
    async fn every_grounding_bid_rides_her_view_and_keeps_its_byte_stability() {
        let stable = RagSourceFaculty::new(
            persona(),
            Arc::new(StubSource::new("room-roster", &["Aria [persona]"])),
            SaliencePolicy::StandingFraming,
        )
        .with_clock(Arc::new(|| 1_000));
        let c = stable
            .contribute(&Workspace::new("hi"))
            .await
            .expect("non-empty source bids");
        assert!(c.stable, "standing framing still says its bytes are stable");
        assert!(c.standing_grounding);
        assert!(c.trailing, "standing grounding is a block of her view, never the system message");

        let volatile = RagSourceFaculty::new(
            persona(),
            Arc::new(StubSource::new("workspace-map", &["src/ lib/ tests/"])),
            SaliencePolicy::StandingFraming,
        )
        .with_volatile_content(true)
        .with_clock(Arc::new(|| 1_000));
        let c = volatile
            .contribute(&Workspace::new("hi"))
            .await
            .expect("non-empty source bids");
        assert!(!c.stable, "volatile content leaves the stable tier");
        assert!(
            c.standing_grounding,
            "moving KV placement must not erase importance"
        );
        assert!(
            c.trailing,
            "volatile grounding rides a trailing turn — churn costs its own tokens, \
             never the conversation's"
        );
        let retrieved = RagSourceFaculty::new(
            persona(),
            Arc::new(StubSource::new("recall", &["a retrieved memory"])),
            SaliencePolicy::Retrieved,
        );
        let c = retrieved
            .contribute(&Workspace::new("hi"))
            .await
            .expect("non-empty retrieved source bids");
        assert!(
            !c.standing_grounding,
            "retrieval remains optional enrichment"
        );
    }

    // what this catches (card c119ace7, Cormac's condition on #4280): a BOARD-churn
    // source registered as plain framing — no `volatile_content` — must NOT reach the
    // cacheable system prefix. `room-wall` was exactly that: declared standing, riding
    // the system message ahead of the conversation, with bytes that change whenever any
    // teammate writes a card ledger. The M5's 2026-09-20 log convicted it from both
    // sides — the turns where the wall changed reused 2,046 tokens (6%: the tools +
    // identity head), and the turns where it did not reused 65-78% with the SAME
    // conversation behind it. Placement now follows the CHURN TABLE
    // (`deliberation_prompt::churn_of`), so no future registration can put a mutating
    // block back in front of the conversation by forgetting one builder call.
    #[tokio::test]
    async fn a_board_churn_source_never_reaches_the_cacheable_prefix_however_it_is_registered() {
        let wall = RagSourceFaculty::new(
            persona(),
            Arc::new(StubSource::new("room-wall", &["card 1234: a ledger line"])),
            // Registered EXACTLY as the live supervisor registers it: framing, no
            // `with_volatile_content`. This is the registration that shipped the defect.
            SaliencePolicy::StandingFraming,
        )
        .with_clock(Arc::new(|| 1_000));
        let c = wall
            .contribute(&Workspace::new("hi"))
            .await
            .expect("non-empty source bids");
        assert!(
            !c.stable,
            "the wall is BOARD churn — it may never sit in the cacheable system prefix"
        );
        assert!(
            c.trailing,
            "board grounding renders AFTER the conversation, beside map/active-work/kanban"
        );
        assert!(
            c.standing_grounding,
            "moving KV placement must not erase importance — the wall is still standing ground"
        );
    }

    // what this catches: an empty delivery → abstain (None), not an empty bid.
    // A degraded/absent source (good-citizen empty delivery) simply does not
    // clutter the workspace.
    #[tokio::test]
    async fn empty_delivery_abstains() {
        let source = Arc::new(StubSource::new("room-doctrine", &[]));
        let faculty = RagSourceFaculty::new(persona(), source, SaliencePolicy::StandingFraming);
        assert!(faculty
            .contribute(&Workspace::new("anything?"))
            .await
            .is_none());
    }

    // what this catches: the bridge passes the faculty's persona scope through to
    // the source (so the source's persona-scoping / defense-in-depth check sees
    // the right citizen).
    #[tokio::test]
    async fn passes_persona_scope_to_the_source() {
        let source = Arc::new(StubSource::new("room-doctrine", &["coordination room"]));
        let probe = source.clone();
        let faculty = RagSourceFaculty::new(persona(), source, SaliencePolicy::Retrieved);
        let mut ws = Workspace::new("burst");
        ws.activity_room_id = Uuid::new_v4();
        let _ = faculty.contribute(&ws).await;
        assert_eq!(*probe.seen_room.lock().unwrap(), Some(ws.room_id), "conversation grounding retains wake scope");
        assert_eq!(
            *probe.seen_persona.lock().unwrap(),
            Some(persona()),
            "the source must be delivered for the faculty's persona"
        );
    }

    // what this catches: the whole point — bridged grounding reaches the
    // deliberation faculty through the staged cycle. A doctrine bid in phase 1 is
    // in the assembled broadcast the decider reads in phase 2.
    #[tokio::test]
    async fn bridged_grounding_reaches_the_broadcast() {
        use crate::cognition::workspace::{Contribution, Decision, SalienceArbiter, WorkspaceCycle};

        // A deliberation faculty that proves it saw the bridged doctrine.
        struct SeesDoctrine;
        #[async_trait]
        impl crate::cognition::workspace::Faculty for SeesDoctrine {
            fn id(&self) -> FacultyId {
                FacultyId::Deliberation
            }
            fn reacts_to_broadcast(&self) -> bool {
                true
            }
            async fn contribute(&self, ws: &Workspace) -> Option<Contribution> {
                let doctrine = ws
                    .broadcast
                    .iter()
                    .find(|c| c.faculty == FacultyId::Custom("room-doctrine".to_string()));
                match doctrine {
                    Some(d) => Some(Contribution::verdict(
                        Decision::Speak {
                            text: format!("noted the room is: {}", d.content),
                        },
                        0.9,
                        "conditioned on the bridged doctrine grounding",
                    )),
                    None => Some(Contribution::verdict(
                        Decision::pass(),
                        0.4,
                        "no doctrine in the broadcast",
                    )),
                }
            }
        }

        let doctrine_source = Arc::new(StubSource::new(
            "room-doctrine",
            &["a coordination room — respond sparingly"],
        ));
        let faculties: Vec<Arc<dyn crate::cognition::workspace::Faculty>> = vec![
            Arc::new(
                RagSourceFaculty::new(persona(), doctrine_source, SaliencePolicy::StandingFraming)
                    .with_clock(Arc::new(|| 1)),
            ),
            Arc::new(SeesDoctrine),
        ];
        let ws = WorkspaceCycle::new(faculties, Arc::new(SalienceArbiter))
            .run("is anyone going to merge this?")
            .await;

        match ws.decision() {
            Some(Decision::Speak { text }) => assert!(
                text.contains("respond sparingly"),
                "the decider must condition on the bridged doctrine grounding, got: {text}"
            ),
            other => panic!("expected a doctrine-grounded Speak, got {other:?}"),
        }
    }

    // what this catches: the salience POLICY — standing framing (roster/doctrine)
    // bids HIGHER than retrieved grounding, so the floor exists. This is the
    // ordering is a preference, not required placement.
    #[tokio::test]
    async fn standing_framing_outbids_retrieved() {
        let framing_source = Arc::new(StubSource::new("room-doctrine", &["respond sparingly"]));
        let retrieved_source = Arc::new(StubSource::new("conversation", &["someone said hi"]));
        let framing = RagSourceFaculty::new(
            persona(),
            framing_source.clone(),
            SaliencePolicy::StandingFraming,
        )
        .with_clock(Arc::new(|| 1));
        let retrieved = RagSourceFaculty::new(
            persona(),
            retrieved_source.clone(),
            SaliencePolicy::Retrieved,
        )
        .with_clock(Arc::new(|| 1));

        let f = framing.contribute(&Workspace::new("q")).await.unwrap();
        let r = retrieved.contribute(&Workspace::new("q")).await.unwrap();
        assert!(
            f.salience > r.salience,
            "standing framing must out-bid retrieved (floor): framing={} retrieved={}",
            f.salience,
            r.salience
        );

        // The Fixed escape-hatch is passed through verbatim (after clamp).
        let fixed = RagSourceFaculty::new(
            persona(),
            Arc::new(StubSource::new("x", &["y"])),
            SaliencePolicy::Fixed(0.42),
        )
        .with_clock(Arc::new(|| 1));
        let c = fixed.contribute(&Workspace::new("q")).await.unwrap();
        assert!((c.salience - 0.42).abs() < 1e-6);
    }

    // what this catches: THE CANARY — standing framing keeps a non-zero presence
    // in the assembled context_broadcast even when a RETRIEVED bid out-saliences
    // it. At the default workspace capacity the floor is what guarantees the
    // persona never "forgets the room's rules mid-turn" because a relevant memory
    // crowded the doctrine out. (Hard-capacity-pressure exemption — making
    // standing framing truncation-proof regardless of capacity — is the
    // convergence follow-up; this asserts the realistic-capacity guarantee the
    // bridge ships with.)
    #[tokio::test]
    async fn standing_framing_present_even_when_outsalienced() {
        use crate::cognition::workspace::{Contribution, FacultyId as FId, SalienceArbiter, Workspace as Ws, WorkspaceCaptureSink, WorkspaceCycle, WorkspaceTrace};

        // A retrieved-tier faculty that bids ABOVE the standing-framing floor
        // (a very relevant memory) — the adversarial case for the canary.
        struct HotRetrieval;
        #[async_trait]
        impl crate::cognition::workspace::Faculty for HotRetrieval {
            fn id(&self) -> FId {
                FId::Recall
            }
            async fn contribute(&self, _ws: &Ws) -> Option<Contribution> {
                Some(Contribution::context(
                    FId::Recall,
                    "a highly relevant recalled memory",
                    0.99,
                    "hot cosine hit",
                ))
            }
        }

        #[derive(Default)]
        struct Sink(std::sync::Mutex<Vec<WorkspaceTrace>>);
        impl WorkspaceCaptureSink for Sink {
            fn record(&self, t: &WorkspaceTrace) {
                self.0.lock().unwrap().push(t.clone());
            }
        }

        let doctrine = RagSourceFaculty::new(
            persona(),
            Arc::new(StubSource::new(
                "room-doctrine",
                &["coordination room — respond sparingly"],
            )),
            SaliencePolicy::StandingFraming,
        )
        .with_clock(Arc::new(|| 1));

        let sink = Arc::new(Sink::default());
        let faculties: Vec<Arc<dyn crate::cognition::workspace::Faculty>> =
            vec![Arc::new(doctrine), Arc::new(HotRetrieval)];
        let _ = WorkspaceCycle::new(
            faculties,
            Arc::new(SalienceArbiter),
        )
        .with_capture(sink.clone())
        .run("anything urgent?")
        .await;

        let traces = sink.0.lock().unwrap();
        let ctx = &traces[0].context_broadcast;
        assert!(
            ctx.iter()
                .any(|c| c.faculty == FId::Custom("room-doctrine".to_string())),
            "standing-framing doctrine must remain in context_broadcast even when a \
             retrieved bid out-saliences it — else the persona forgets the room's rules"
        );
    }

