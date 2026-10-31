#![cfg(test)]
use super::*;

/// A loop needs its obligation store (cards 920619c5, 68811445), and a turn runs only on a
/// CONFIRMED obligation, so these tests write through the real store's data module.
fn test_obligations(hosted: &HostedPersona) -> crate::persona::obligation::Obligations {
    crate::persona::obligation::Obligations::new(
        &crate::persona::obligation::test_store_executor(),
        hosted.identity.peer_id.as_uuid(),
        &hosted.identity.agent_name,
    )
}

    // Bespoke `StubConversation` / `CannedAdapter` / `EmptyReader` /
    // `fake_hosted` / `fake_hosted_with_delay` deleted per
    // [[test-fixtures-are-system-primitives]]. The substrate's
    // ubiquitous primitives now power every test below:
    //   * `ScriptedConversation` for the `PersonaConversation` impl
    //     (with `.with_events`, `.with_high_water`,
    //     `.with_prime_failure`, `.require_prime_before_next_message`)
    //   * `HeuristicInferenceAdapter` for the adapter (with
    //     `.with_delay_ms` for the honest-latency test)
    //   * `StubAircCitizen` for the AircTranscriptReader — citizens
    //     are also transcript readers via the trait's supertrait
    //     bound, so it doubles as the empty-transcript reader
    //   * `hosted_with_adapter` is the local-helper-of-helpers
    //     wrapping HostedPersona — small, no impls
    pub(super) fn hosted_with_adapter(
        persona_peer_id: Uuid,
        adapter: Arc<dyn AIProviderAdapter>,
    ) -> HostedPersona {
        use crate::persona::hw_tier_descriptor::HwTierCategory;
        use crate::persona::inference_profile::{PersonaInferenceProfile, SamplingProfile};
        // The PersonaInferenceProfile still keys on a bare Uuid; the
        // PersonaInstanceInfo now carries the single canonical
        // `peer_id: PeerId` (Step 4b collapsed the persona_id twin).
        let persona_id = persona_peer_id;
        // Build a profile shaped like the LCD Compat tier — the
        // substrate's lowest common denominator. Test exercises the
        // same `&ctx` derivation path production uses; no hardcoded
        // budget constants outside `profile_builder`.
        let profile = PersonaInferenceProfile {
            persona_id,
            persona_name: "Paige".to_string(),
            model_id: "fake-model".to_string(),
            gguf_local_path: None,
            tier_category: HwTierCategory::Compat,
            tier_id: "test_fixture".to_string(),
            context_length: 2048,
            n_ubatch: 512,
            n_batch: 2048,
            n_seq_max: 1,
            n_gpu_layers: 0,
            sampling: SamplingProfile::chat_defaults(),
            chat_template: None,
            stop_sequences: vec![],
        };
        HostedPersona {
            role: RoleId::Helper,
            identity: PersonaInstanceInfo {
                agent_name: "Paige".to_string(),
                peer_id: crate::identity::PeerId::from_uuid(persona_peer_id),
                home: PathBuf::from("/tmp/fake-service-loop"),
                default_room: Uuid::nil(),
                source: PersonaIdentitySource::FreshlyMinted,
            },
            profile,
            adapter,
            // System-level `StubAircCitizen` satisfies the trait
            // without standing up a real airc daemon per
            // [[test-fixtures-are-system-primitives]].
            runtime: Arc::new(StubAircCitizen::new(persona_peer_id)),
            // Brain. Tests construct a default PersonaCognition
            // and DO NOT bind airc_source — the stub citizen's
            // page_recent returns empty per the no-fallback doctrine,
            // so unit tests exercising the loop don't need the
            // airc-side composition to land items.
            cognition: Arc::new(tokio::sync::Mutex::new(
                crate::persona::unified::PersonaCognition::new(
                    persona_id,
                    "Paige".to_string(),
                    Arc::new(crate::rag::RagEngine::new()),
                ),
            )),
            // #195 slice 2: build via the same helper production
            // uses, NOT a copy-pasted template. If a future PR
            // adjusts the prompt wording, this fixture stays
            // honest about what the test exercises.
            system_prompt: super::super::super::supervisor::build_persona_system_prompt("Paige"),
        }
    }

    pub(super) fn hosted_with_heuristic(persona_peer_id: Uuid) -> HostedPersona {
        hosted_with_adapter(persona_peer_id, Arc::new(HeuristicInferenceAdapter::new()))
    }

    pub(super) fn hosted_with_delay_ms(persona_peer_id: Uuid, delay_ms: u64) -> HostedPersona {
        hosted_with_adapter(
            persona_peer_id,
            Arc::new(HeuristicInferenceAdapter::new().with_delay_ms(delay_ms)),
        )
    }

    pub(super) fn fixed_now() -> u64 {
        1_700_000_000_000
    }

    /// Happy path: one inbound from another peer → one reply posted.
    /// turns_replied=1, turns_skipped=0, turns_errored=0.
    #[ignore = "slice 1D — global adapter registration (#161). respond() needs adapter in GLOBAL_REGISTRY; fixture not yet wired."]
    #[tokio::test]
    pub(super) async fn replies_to_inbound_from_other_peer() {
        let persona_peer = Uuid::new_v4();
        let other_peer = Uuid::new_v4();
        let hosted = hosted_with_heuristic(persona_peer);

        let mut conversation = ScriptedConversation::new().with_events(vec![
            Ok(Some(IncomingMessage {
    work: None,
                media: Vec::new(),
                event_id: uuid::Uuid::nil(),
                lamport: 1,
                peer_id: other_peer,
                text: "hello?".to_string(),
                room_id: Uuid::nil(),
            })),
            Ok(None),
        ]);

        // Caller-primes contract per [[no-fallbacks-ever]] — explicit.
        conversation.prime().await.expect("prime ok");

        let opts = ServeOptions {
            page_recent_limit: 10,
            rag_fetch_limit: 10,
            now_ms: fixed_now,
            ..Default::default()
        };

        let outcome = serve_persona_loop(&hosted, &mut conversation, opts, test_obligations(&hosted))
            .await
            .expect("loop completes");

        assert_eq!(outcome.turns_replied, 1);
        assert_eq!(outcome.turns_skipped, 0);
        assert_eq!(outcome.turns_errored, 0);
        let said = conversation.said();
        assert_eq!(said.len(), 1);
        // HeuristicInferenceAdapter responses are shaped
        // `[heuristic:<hash>] ack: "<echo>"`. Per
        // [[test-fixtures-are-system-primitives]] we verify the
        // shape, not the literal text — the substrate's deterministic
        // adapter wired through cleanly.
        assert!(
            said[0].contains("[heuristic:") && said[0].contains("ack:"),
            "reply must come from HeuristicInferenceAdapter: {}",
            said[0]
        );
        // The loop primes the conversation exactly once at boot —
        // before any high_water_mark or next_message call. Per
        // [[persona-webrtc-all-tiers-latency-obsessed]] this is what
        // moves the airc subscribe round-trip OFF the cognition hot
        // path. If a future refactor regresses to lazy subscribe, the
        // primed count drops to 0 and this test fails loudly.
        assert_eq!(
            conversation.primed_count(),
            1,
            "serve_persona_loop must call prime() exactly once at boot"
        );
        // Latency metric recorded for the one successful reply.
        // count == 1 — exact assertion; the *value* is wall-clock-
        // dependent so we just check it's been captured. Per
        // [[init-once-handle-then-lease-zero-copy-refs]]: the metric
        // is what verifies the prime/warmup/etc. doctrines actually
        // moved cold-start off the hot path; if a future refactor
        // forgets to record, this drops to 0 and fails loudly.
        assert_eq!(
            outcome.turn_latency.count, 1,
            "successful reply must record exactly one latency sample"
        );
        assert!(
            outcome.turn_latency.min_ms.is_some(),
            "min_ms set after one sample"
        );
        assert!(
            outcome.turn_latency.max_ms.is_some(),
            "max_ms set after one sample"
        );
        assert!(
            outcome.turn_latency.mean_ms().is_some(),
            "mean computable after one sample"
        );
    }

    /// Honest latency test: injects a real ~80ms sleep into the
    /// adapter's generate_text, asserts the recorded turn_latency
    /// reflects that delay. Without this, the count-only test below
    /// would be fake-demo-shaped — passing on plumbing, silent on
    /// whether the metric tracks ACTUAL elapsed wall-clock.
    ///
    /// Bounds are generous (lower 50ms, upper 5s) so the test is
    /// jitter-tolerant on noisy CI hosts. A regression that records
    /// the wrong duration (e.g., measuring something other than the
    /// reply path) would land outside this range and fail.
    #[ignore = "slice 1D — global adapter registration (#161). respond() needs adapter in GLOBAL_REGISTRY; fixture not yet wired."]
    #[tokio::test]
    pub(super) async fn latency_metric_reflects_real_wall_clock() {
        let persona_peer = Uuid::new_v4();
        let other_peer = Uuid::new_v4();
        let hosted = hosted_with_delay_ms(persona_peer, 80);

        let mut conversation = ScriptedConversation::new().with_events(vec![
            Ok(Some(IncomingMessage {
    work: None,
                media: Vec::new(),
                event_id: uuid::Uuid::nil(),
                lamport: 1,
                peer_id: other_peer,
                text: "ping?".to_string(),
                room_id: Uuid::nil(),
            })),
            Ok(None),
        ]);

        conversation.prime().await.expect("prime ok");

        let opts = ServeOptions {
            page_recent_limit: 10,
            rag_fetch_limit: 10,
            now_ms: fixed_now,
            ..Default::default()
        };

        let outcome = serve_persona_loop(&hosted, &mut conversation, opts, test_obligations(&hosted))
            .await
            .expect("loop completes");

        assert_eq!(outcome.turn_latency.count, 1);
        let observed_ms = outcome.turn_latency.min_ms.expect("recorded after sample");
        assert!(
            observed_ms >= 50,
            "recorded latency ({observed_ms}ms) must reflect the injected 80ms \
             sleep (allowing CI jitter floor of 50ms)"
        );
        assert!(
            observed_ms < 5000,
            "recorded latency ({observed_ms}ms) must not balloon — \
             upper bound 5s for sanity"
        );
    }

    /// `LatencyAggregate` math: cheap online min/max/sum/count over
    /// arbitrary inputs. Empty aggregate returns None for everything;
    /// after samples, mean = total / count and min/max track extremes.
    #[test]
    pub(super) fn latency_aggregate_records_min_max_sum_count() {
        let mut agg = LatencyAggregate::default();
        assert_eq!(agg.count, 0);
        assert_eq!(agg.total_ms, 0);
        assert!(agg.min_ms.is_none());
        assert!(agg.max_ms.is_none());
        assert!(agg.mean_ms().is_none());

        agg.record(10);
        assert_eq!(agg.count, 1);
        assert_eq!(agg.total_ms, 10);
        assert_eq!(agg.min_ms, Some(10));
        assert_eq!(agg.max_ms, Some(10));
        assert_eq!(agg.mean_ms(), Some(10.0));

        agg.record(50);
        agg.record(30);
        assert_eq!(agg.count, 3);
        assert_eq!(agg.total_ms, 90);
        assert_eq!(agg.min_ms, Some(10));
        assert_eq!(agg.max_ms, Some(50));
        assert_eq!(agg.mean_ms(), Some(30.0));
    }

    /// Saturating add: if the substrate ever runs a session long
    /// enough for total_ms to approach u64::MAX (~580 million years
    /// at 1ms/turn), `record` saturates rather than wraps. Locks the
    /// invariant; the substrate would never hit this in practice but
    /// the safety property matters per [[every-error-is-an-opportunity-to-battle-harden]].
    #[test]
    pub(super) fn latency_aggregate_saturates_on_overflow() {
        let mut agg = LatencyAggregate {
            count: 1,
            total_ms: u64::MAX - 5,
            min_ms: Some(0),
            max_ms: Some(u64::MAX - 5),
        };
        agg.record(100);
        assert_eq!(agg.total_ms, u64::MAX, "saturated, not wrapped");
        assert_eq!(agg.count, 2);
    }

    /// `ServeOutcome::default()` initializes all five phase
    /// aggregates to empty (count=0). Pin this so future fields
    /// added to the struct don't accidentally regress the existing
    /// fields' Default-impl behavior — every aggregate stays
    /// independently zeroed and the decomposition starts clean.
    #[test]
    pub(super) fn serve_outcome_phase_aggregates_default_to_empty() {
        let outcome = ServeOutcome::default();
        assert_eq!(outcome.recall_latency.count, 0);
        assert_eq!(outcome.admit_latency.count, 0);
        assert_eq!(outcome.compose_latency.count, 0);
        assert_eq!(outcome.respond_latency.count, 0);
        assert_eq!(outcome.say_latency.count, 0);
        assert!(outcome.recall_latency.mean_ms().is_none());
        assert!(outcome.respond_latency.mean_ms().is_none());
    }

    /// #152 LCD identity grounding. Pre-fix prompt was a single
    /// line ("You are {name}, an autonomous AI persona on the
    /// grid.") that LCD-tier models (Qwen2.5-0.5B etc.) couldn't
    /// hold under any pressure — they drifted to training
    /// defaults (Claude / GPT / Siemens PLC etc).
    ///
    /// This test pins the structural contract of the post-fix
    /// prompt — the SPECIFIC clauses that address known drift
    /// modes — without pinning prose verbatim (so a future PR
    /// can tighten wording without churning the test). The pin
    /// covers:
    ///
    /// 1. Name + role anchoring in opening line.
    /// 2. Explicit "you are NOT" list of common drift targets.
    /// 3. First-person stability instruction.
    /// 4. Grid / room context vocabulary.
    /// 5. Silence-token reference (couples to the silence
    ///    affordance shipped in the same arc — both prompt
    ///    blocks are load-bearing for #151 + #152).
    ///
    /// A future PR that materially changes any clause must update
    /// the matching assertion; silent drift would re-introduce the
    /// identity-drift bug.
    #[test]
    pub(super) fn system_prompt_carries_lcd_identity_grounding() {
        let prompt = super::super::super::supervisor::build_persona_system_prompt("Paige");
        let s = prompt.as_ref();

        assert!(
            s.contains("Paige"),
            "persona name must appear at least once. Got: {s}"
        );
        assert!(
            s.contains("autonomous AI persona"),
            "role line missing. Got: {s}"
        );
        assert!(
            s.contains("Identity"),
            "identity block header missing. Got: {s}"
        );
        // Drift-target enumeration. Concrete name list is the
        // operationally effective form on LCD models — abstract
        // "don't drift" instructions don't stick.
        for target in &["Claude", "GPT", "Gemini", "Llama", "Qwen", "Siemens PLC"] {
            assert!(
                s.contains(target),
                "drift target {target:?} missing from 'you are NOT' enumeration. Got: {s}"
            );
        }
        // First-person stability. Single most effective LCD anti-
        // drift instruction per Joel 2026-06-03 testing.
        assert!(
            s.contains("first person") || s.contains("first-person"),
            "first-person stability clause missing. Got: {s}"
        );
        // Grid / room context vocabulary.
        assert!(s.contains("grid"), "'grid' vocabulary missing. Got: {s}");
        assert!(
            s.contains("Room") || s.contains("room"),
            "'room' vocabulary missing. Got: {s}"
        );
        // Couples to the silence affordance.
        assert!(
            s.contains("[Conversational Presence]") || s.contains("silence"),
            "silence-affordance reference missing — identity prompt should hint at the affordance assembled downstream. Got: {s}"
        );
    }

    /// #195 slice 2: pin that cloning the cached prompt is a
    /// cheap Arc refcount bump, not a deep copy. Without this,
    /// a future refactor swapping `Arc<str>` for `String` would
    /// silently restore the per-turn-allocation cost the slice
    /// shipped to eliminate.
    #[test]
    pub(super) fn cached_system_prompt_clones_via_arc_refcount() {
        let original = super::super::super::supervisor::build_persona_system_prompt("Paige");
        let cloned = std::sync::Arc::clone(&original);
        assert_eq!(
            std::sync::Arc::strong_count(&original),
            2,
            "Arc::clone must bump the refcount (cheap pointer copy) — \
             not deep-copy the underlying str"
        );
        // Pointer-equality on the underlying str confirms zero-copy.
        assert!(
            std::ptr::eq(
                original.as_ref() as *const str,
                cloned.as_ref() as *const str
            ),
            "cloned Arc must point at the SAME str storage as the original"
        );
    }

    /// #195 slice 3: pin that the migration from
    /// `format!("{:?}", role).to_lowercase()` to `role.as_str()`
    /// in `serve_persona_loop_inner` is byte-for-byte
    /// behavior-preserving for EVERY current `RoleId` variant.
    ///
    /// Non-circular: the two sides are independently-derived —
    /// the left is the new direct production path (a hand-
    /// curated `match role { ... }` in `role_template.rs::as_str`),
    /// the right is the pre-slice-3 Debug-format + Unicode
    /// lowercase chain. They are equal today by careful design
    /// of `as_str()`; if a future PR adds a `RoleId` variant
    /// whose `as_str()` choice DOESN'T match the legacy
    /// Debug+lowercase, this test breaks loudly so the author
    /// makes an explicit decision (update the test to record
    /// the intentional divergence, or align `as_str()` with
    /// the legacy form).
    ///
    /// The exhaustive `match` matcher (no wildcard) makes the
    /// compiler force a new variant to appear here at the same
    /// time it appears in `RoleId`, so the `variants` array
    /// stays complete by construction.
    #[test]
    pub(super) fn role_as_str_preserves_pre_slice3_specialty_format_for_each_role() {
        // RoleId is already in scope via `use crate::experience::role::RoleId`
        // at the top of the test module — no per-test import needed.
        let variants: &[RoleId] = &[
            RoleId::Helper,
            RoleId::Coder,
            RoleId::Sentinel,
            RoleId::Designer,
            RoleId::Custom,
        ];
        for role in variants {
            let direct = role.as_str();
            let legacy = format!("{:?}", role).to_lowercase();
            assert_eq!(
                direct, legacy,
                "RoleId::{:?}.as_str() must produce the SAME string \
                 the pre-#195-slice-3 per-turn \
                 format!(\"{{:?}}\", role).to_lowercase() did — \
                 otherwise switching the service loop's specialty \
                 source from Debug+lowercase to as_str() silently \
                 changes what downstream prompt assembly reads",
                role
            );
        }
        // Compile-time exhaustiveness: a new RoleId variant
        // forces a new arm here, which forces the author to add
        // it to the `variants` array above. The arms each touch
        // the variant explicitly (no `_` wildcard) so the
        // compiler rejects silent drift.
        let _ = |role: RoleId| match role {
            RoleId::Helper => (),
            RoleId::Coder => (),
            RoleId::Sentinel => (),
            RoleId::Designer => (),
            RoleId::Custom => (),
        };
    }

    /// Aggregate-math property: each phase aggregate is structurally
    /// identical to `turn_latency` (same `LatencyAggregate` shape).
    /// A future refactor that swaps one to a different type would
    /// fail this property test. Belt-and-braces for the
    /// "decomposition components compose the same way" contract.
    #[test]
    pub(super) fn phase_aggregates_use_same_type_as_turn_latency() {
        let mut outcome = ServeOutcome::default();
        // Record one sample into each — same call shape as
        // `turn_latency.record`. If any phase aggregate had drifted
        // to a different type/signature, this fails to compile.
        outcome.turn_latency.record(100);
        outcome.recall_latency.record(5);
        outcome.admit_latency.record(2);
        outcome.compose_latency.record(8);
        outcome.respond_latency.record(80);
        outcome.say_latency.record(3);

        // Sum-of-phases should be ≤ turn_latency. Sample test
        // (100 vs 5+2+8+80+3 = 98) demonstrates the residual is
        // the projection + bookkeeping overhead the decomposition
        // doesn't account for. The substrate's `turn_latency`
        // remains the source of truth for end-to-end cost.
        let phase_sum = outcome.recall_latency.total_ms
            + outcome.admit_latency.total_ms
            + outcome.compose_latency.total_ms
            + outcome.respond_latency.total_ms
            + outcome.say_latency.total_ms;
        assert_eq!(phase_sum, 98, "phases sum correctly");
        assert!(
            phase_sum <= outcome.turn_latency.total_ms,
            "phase-sum ({phase_sum}ms) must not exceed turn_latency ({}ms) — \
             the decomposition is internal-to-turn",
            outcome.turn_latency.total_ms
        );
    }

    /// Caller-primes contract: per [[no-fallbacks-ever]] the loop does
    /// NOT prime as a safety net — callers honor the contract. If a
    /// test forgets to call `conversation.prime()` before
    /// `serve_persona_loop`, the conversation's `next_message`
    /// returns a typed `Err("called before prime()")`. The loop
    /// surfaces this as `turns_errored` per the substrate's
    /// honest-error doctrine, then ends when the stub yields `None`.
    ///
    /// Locks the absence of the belt-and-suspenders prime() call in
    /// serve_persona_loop_inner. If a future refactor adds a
    /// safety-net prime back into the loop, this test starts
    /// reporting `turns_errored == 0`, exposing the regression.
    #[tokio::test]
    pub(super) async fn loop_without_caller_prime_surfaces_typed_error_per_turn() {
        let persona_peer = Uuid::new_v4();
        let other_peer = Uuid::new_v4();
        let hosted = hosted_with_heuristic(persona_peer);

        // System primitive: `require_prime_before_next_message` makes
        // `ScriptedConversation` mirror `AircPersonaConversation`'s
        // caller-primes contract — next_message returns Err if prime
        // wasn't called first. Substitutes the bespoke
        // UnprimedConversation per [[test-fixtures-are-system-primitives]].
        let mut conversation = ScriptedConversation::new()
            .with_events(vec![Ok(Some(IncomingMessage {
                work: None,
                media: Vec::new(),
                event_id: uuid::Uuid::nil(),
                lamport: 1,
                peer_id: other_peer,
                text: "would-be-message".to_string(),
                room_id: Uuid::nil(),
            }))])
            .require_prime_before_next_message();

        // INTENTIONALLY do NOT prime — verify the loop doesn't auto-prime.
        let outcome = serve_persona_loop(
            &hosted,
            &mut conversation,
            ServeOptions {
                page_recent_limit: 10,
                rag_fetch_limit: 10,
                now_ms: fixed_now,
                ..Default::default()
            }, test_obligations(&hosted)
        )
        .await
        .expect("loop completes (each next_message err counts as turn_errored)");

        assert_eq!(
            outcome.turns_errored, 1,
            "unprimed conversation's typed next_message err counts as errored turn"
        );
        assert_eq!(outcome.turns_replied, 0);
    }

    /// Self-loop guard: when the inbound peer_id matches the
    /// persona's own peer_id, the loop skips it (no inference call,
    /// no say). turns_skipped=1.
    #[tokio::test]
    pub(super) async fn skips_self_loop_messages() {
        let persona_peer = Uuid::new_v4();
        let hosted = hosted_with_heuristic(persona_peer);

        let mut conversation = ScriptedConversation::new().with_events(vec![
            Ok(Some(IncomingMessage {
                work: None,
                media: Vec::new(),
                event_id: uuid::Uuid::nil(),
                lamport: 1,
                peer_id: persona_peer, // SELF
                text: "my own echo".to_string(),
                room_id: Uuid::nil(),
            })),
            Ok(None),
        ]);
        conversation.prime().await.expect("prime ok");

        let outcome = serve_persona_loop(
            &hosted,
            &mut conversation,
            ServeOptions {
                page_recent_limit: 10,
                rag_fetch_limit: 10,
                now_ms: fixed_now,
                ..Default::default()
            }, test_obligations(&hosted)
        )
        .await
        .expect("loop completes");

        assert_eq!(outcome.turns_replied, 0);
        assert_eq!(outcome.turns_skipped, 1);
        assert_eq!(outcome.turns_errored, 0);
        assert!(conversation.said().is_empty());
    }

    /// Pre-watermark guard: messages with lamport <= high_water are
    #[ignore = "slice 1D — global adapter registration (#161). respond() needs adapter in GLOBAL_REGISTRY; fixture not yet wired."]
    /// skipped. Avoids replying to history on attach.
    #[tokio::test]
    pub(super) async fn skips_messages_below_high_water_mark() {
        let persona_peer = Uuid::new_v4();
        let other_peer = Uuid::new_v4();
        let hosted = hosted_with_heuristic(persona_peer);

        let mut conversation = ScriptedConversation::new()
            .with_high_water(100) // pre-attach history was up to lamport=100
            .with_events(vec![
                Ok(Some(IncomingMessage {
                    work: None,
                    media: Vec::new(),
                    event_id: uuid::Uuid::nil(),
                    lamport: 50, // BEFORE attach
                    peer_id: other_peer,
                    text: "ancient".to_string(),
                    room_id: Uuid::nil(),
                })),
                Ok(Some(IncomingMessage {
                    work: None,
                    media: Vec::new(),
                    event_id: uuid::Uuid::nil(),
                    lamport: 100, // exactly at the mark — also skipped
                    peer_id: other_peer,
                    text: "boundary".to_string(),
                    room_id: Uuid::nil(),
                })),
                Ok(Some(IncomingMessage {
                    work: None,
                    media: Vec::new(),
                    event_id: uuid::Uuid::nil(),
                    lamport: 101, // FRESH
                    peer_id: other_peer,
                    text: "new".to_string(),
                    room_id: Uuid::nil(),
                })),
                Ok(None),
            ]);
        conversation.prime().await.expect("prime ok");

        let outcome = serve_persona_loop(
            &hosted,
            &mut conversation,
            ServeOptions {
                page_recent_limit: 10,
                rag_fetch_limit: 10,
                now_ms: fixed_now,
                ..Default::default()
            }, test_obligations(&hosted)
        )
        .await
        .expect("loop completes");

        assert_eq!(outcome.turns_replied, 1, "only lamport=101 should reply");
        assert_eq!(
            outcome.turns_skipped, 2,
            "lamport=50 and lamport=100 both pre-mark"
        );
        assert_eq!(outcome.turns_errored, 0);
        assert_eq!(conversation.said().len(), 1);
    }

    /// Transient transport error increments turns_errored AND the
    /// loop continues — does NOT propagate as a Result::Err from
    #[ignore = "slice 1D — global adapter registration (#161). respond() needs adapter in GLOBAL_REGISTRY; fixture not yet wired."]
    /// serve_persona_loop. The trailing Ok(None) eventually ends it
    /// cleanly. Pins the "live stream lag — resume continues" shape
    /// per `[[no-fallbacks-ever]]`: typed Err on the boundary, loop
    /// handles, no silent drop.
    #[tokio::test]
    pub(super) async fn transient_next_message_error_does_not_kill_loop() {
        let persona_peer = Uuid::new_v4();
        let other_peer = Uuid::new_v4();
        let hosted = hosted_with_heuristic(persona_peer);

        let mut conversation = ScriptedConversation::new().with_events(vec![
            Err("stream lag".to_string()),
            Ok(Some(IncomingMessage {
                work: None,
                media: Vec::new(),
                event_id: uuid::Uuid::nil(),
                lamport: 1,
                peer_id: other_peer,
                text: "after lag".to_string(),
                room_id: Uuid::nil(),
            })),
            Ok(None),
        ]);
        conversation.prime().await.expect("prime ok");

        let outcome = serve_persona_loop(
            &hosted,
            &mut conversation,
            ServeOptions {
                page_recent_limit: 10,
                rag_fetch_limit: 10,
                now_ms: fixed_now,
                ..Default::default()
            }, test_obligations(&hosted)
        )
        .await
        .expect("loop completes despite transient error");

        assert_eq!(outcome.turns_replied, 1);
        assert_eq!(outcome.turns_errored, 1);
        assert_eq!(outcome.turns_skipped, 0);
        assert_eq!(conversation.said().len(), 1);
    }
