#![cfg(test)]
    use super::*;

    fn budget(
        source_id: &'static str,
        priority: u8,
        floor: u32,
        min: u32,
        max: u32,
        required: bool,
    ) -> RagSourceBudget {
        RagSourceBudget {
            source_id: source_id.to_string(),
            priority,
            floor_tokens: floor,
            min_tokens: min,
            max_tokens: max,
            required,
        }
    }

    fn reserved(system: u32, completion: u32) -> ReservedTokens {
        ReservedTokens { system, completion }
    }

    fn alloc_for<'a>(result: &'a BudgetAllocation, id: &str) -> &'a SourceAllocation {
        result
            .allocations
            .iter()
            .find(|a| a.source_id == id)
            .unwrap()
    }

    fn ctx() -> RagContext {
        RagContext::for_persona(
            uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
            1_000_000,
        )
    }

    #[test]
    fn empty_context_window_under_provisions_required() {
        let adapter = FlexboxRagBudgetAdapter::new();
        let result = adapter.allocate(
            &ctx(),
            500,
            reserved(400, 200),
            &[budget("conversation", 10, 100, 200, 1000, true)],
        );
        assert_eq!(result.available_for_sources, 0);
        assert!(result.escalation_needed);
        assert_eq!(
            alloc_for(&result, "conversation").state,
            AllocationState::UnderProvisioned
        );
    }

    #[test]
    fn single_required_source_satisfied() {
        let adapter = FlexboxRagBudgetAdapter::new();
        let result = adapter.allocate(
            &ctx(),
            10_000,
            reserved(500, 2000),
            &[budget("conversation", 10, 200, 500, 5000, true)],
        );
        let conv = alloc_for(&result, "conversation");
        assert!(conv.allocated_tokens >= 500);
        assert_eq!(conv.state, AllocationState::Satisfied);
        assert!(!result.escalation_needed);
    }

    #[test]
    fn priority_distributes_remaining_proportionally() {
        let adapter = FlexboxRagBudgetAdapter::new();
        // max well above expected share so neither caps before the
        // priority ratio gets to express.
        let result = adapter.allocate(
            &ctx(),
            10_000,
            reserved(0, 0),
            &[
                budget("conversation", 10, 100, 500, 50_000, true),
                budget("memories", 5, 100, 500, 50_000, true),
            ],
        );
        let conv = alloc_for(&result, "conversation");
        let mem = alloc_for(&result, "memories");
        // Both got their mins (500). Remaining 9000 distributed by
        // priority 10 vs 5 → conv should get roughly 2× memories.
        assert!(
            conv.allocated_tokens > mem.allocated_tokens,
            "conv {} mem {}",
            conv.allocated_tokens,
            mem.allocated_tokens
        );
    }

    #[test]
    fn optional_source_drops_when_floor_cant_fit() {
        let adapter = FlexboxRagBudgetAdapter::new();
        let result = adapter.allocate(
            &ctx(),
            1_000,
            reserved(500, 200),
            &[
                budget("conversation", 10, 200, 200, 500, true),
                budget("artifacts", 3, 200, 200, 500, false),
            ],
        );
        // Conversation required, gets its 200 floor. Remaining 100 <
        // artifacts floor 200, so artifacts is Dropped.
        let conv = alloc_for(&result, "conversation");
        let art = alloc_for(&result, "artifacts");
        assert!(conv.allocated_tokens >= 200);
        assert_ne!(conv.state, AllocationState::Dropped);
        assert_eq!(art.allocated_tokens, 0);
        assert_eq!(art.state, AllocationState::Dropped);
        assert!(!result.escalation_needed); // optional drop is fine
    }

    #[test]
    fn required_under_provisions_when_floor_cant_fit() {
        let adapter = FlexboxRagBudgetAdapter::new();
        let result = adapter.allocate(
            &ctx(),
            300,
            reserved(100, 100),
            &[
                budget("conversation", 10, 200, 200, 500, true),
                budget("memories", 5, 200, 200, 500, true),
            ],
        );
        // Available = 100; conv floor 200 takes it all; memories floor
        // 200 can't fit → UnderProvisioned + escalate.
        assert!(result.escalation_needed);
        assert_eq!(
            alloc_for(&result, "memories").state,
            AllocationState::UnderProvisioned
        );
    }

    #[test]
    fn floor_is_honored_above_min() {
        // Joel's recent-universal floor doctrine: even if min is
        // squeezed, floor is unconditional. Here floor == min so the
        // test verifies the floor lands BEFORE the min pass.
        let adapter = FlexboxRagBudgetAdapter::new();
        let result = adapter.allocate(
            &ctx(),
            2_000,
            reserved(0, 0),
            &[
                budget("conversation", 10, 500, 500, 1000, true),
                budget("memories", 5, 200, 600, 1500, false),
            ],
        );
        let conv = alloc_for(&result, "conversation");
        let mem = alloc_for(&result, "memories");
        assert!(conv.allocated_tokens >= 500);
        assert!(mem.allocated_tokens >= 200);
    }

    #[test]
    fn max_caps_distribution() {
        let adapter = FlexboxRagBudgetAdapter::new();
        let result = adapter.allocate(
            &ctx(),
            10_000,
            reserved(0, 0),
            &[
                budget("tiny", 10, 0, 0, 100, false),
                budget("big", 5, 0, 0, 9_000, false),
            ],
        );
        let tiny = alloc_for(&result, "tiny");
        let big = alloc_for(&result, "big");
        assert_eq!(tiny.allocated_tokens, 100); // capped
                                                // Big should absorb whatever the priority-10 cap left behind.
        assert!(big.allocated_tokens >= 5000);
        assert!(big.allocated_tokens <= 9_000);
    }

    #[test]
    fn deterministic_priority_tiebreak() {
        // Two sources at same priority must allocate identically across
        // runs. Use source_id alpha order.
        let adapter = FlexboxRagBudgetAdapter::new();
        let result_a = adapter.allocate(
            &ctx(),
            10_000,
            reserved(0, 0),
            &[
                budget("a", 5, 0, 500, 2000, false),
                budget("b", 5, 0, 500, 2000, false),
            ],
        );
        let result_b = adapter.allocate(
            &ctx(),
            10_000,
            reserved(0, 0),
            &[
                budget("b", 5, 0, 500, 2000, false),
                budget("a", 5, 0, 500, 2000, false),
            ],
        );
        let a_in_a = alloc_for(&result_a, "a").allocated_tokens;
        let a_in_b = alloc_for(&result_b, "a").allocated_tokens;
        assert_eq!(a_in_a, a_in_b, "allocation must be input-order-independent");
    }

    // ---- Source trait + stub tests ----

    fn item(text: &str, tokens: u32) -> RagItem {
        RagItem {
            content: text.to_string(),
            tokens,
            metadata: serde_json::json!({}),
        }
    }

    fn persona() -> uuid::Uuid {
        uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }

    #[tokio::test]
    async fn stub_source_delivers_what_fits() {
        let source = StubRagSource::new(
            "stub",
            persona(),
            vec![item("a", 10), item("b", 20), item("c", 100)],
        );
        let delivery = source.deliver(&ctx(), 50, ResolutionPreference::Raw).await;
        // a (10) + b (20) = 30 fits, c (100) doesn't.
        assert_eq!(delivery.items.len(), 2);
        assert_eq!(delivery.tokens_used, 30);
        assert!(delivery.continuation.is_some());
        assert_eq!(delivery.continuation.unwrap().persona_id, persona());
    }

    #[tokio::test]
    async fn stub_source_continuation_resumes() {
        let source = StubRagSource::new(
            "stub",
            persona(),
            vec![item("a", 10), item("b", 10), item("c", 10), item("d", 10)],
        );
        let first = source.deliver(&ctx(), 20, ResolutionPreference::Raw).await;
        assert_eq!(first.items.len(), 2);
        let cursor = first.continuation.unwrap();
        let second = source
            .deliver_continuation(&ctx(), cursor, 100)
            .await
            .unwrap();
        assert_eq!(second.items.len(), 2);
        assert!(second.continuation.is_none());
    }

    #[tokio::test]
    async fn stub_source_returns_none_when_exhausted() {
        let source = StubRagSource::new("stub", persona(), vec![item("a", 10)]);
        let first = source.deliver(&ctx(), 100, ResolutionPreference::Raw).await;
        assert_eq!(first.items.len(), 1);
        assert!(first.continuation.is_none());

        let stale = ContinuationCursor {
            persona_id: persona(),
            source_id: "stub".to_string(),
            opaque: serde_json::json!({ "next": 99 }),
        };
        let exhausted = source.deliver_continuation(&ctx(), stale, 100).await;
        assert!(exhausted.is_none());
    }

    #[tokio::test]
    async fn stub_source_never_partial_includes() {
        // The no-clipping invariant: even with budget mid-item, the
        // source skips the over-budget item rather than partial-include.
        let source = StubRagSource::new("stub", persona(), vec![item("huge", 500)]);
        let delivery = source.deliver(&ctx(), 100, ResolutionPreference::Raw).await;
        assert_eq!(delivery.items.len(), 0);
        assert_eq!(delivery.tokens_used, 0);
        // Continuation set because the item still exists, just didn't
        // fit at this budget.
        assert!(delivery.continuation.is_some());
    }

    #[tokio::test]
    async fn stub_source_refuses_cross_persona_cursor() {
        // Joel's substrate-side identity check: cursors from another
        // citizen MUST be refused. "We know who is who, have to use
        // handles" — handles enforce persona scoping.
        let pax = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000abc").unwrap();
        let maya = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000def").unwrap();
        let pax_ctx = RagContext::for_persona(pax, 1_000_000);
        let maya_ctx = RagContext::for_persona(maya, 1_000_000);

        let pax_source = StubRagSource::new("stub", pax, vec![item("a", 10), item("b", 10)]);
        let pax_first = pax_source
            .deliver(&pax_ctx, 15, ResolutionPreference::Raw)
            .await;
        let pax_cursor = pax_first.continuation.unwrap();
        assert_eq!(pax_cursor.persona_id, pax);

        // Maya's source must refuse Pax's cursor — both because the
        // cursor's persona_id doesn't match Maya's binding AND because
        // the source verifies its own persona_id against ctx.persona_id.
        let maya_source = StubRagSource::new("stub", maya, vec![item("x", 10), item("y", 10)]);
        let cross = maya_source
            .deliver_continuation(&maya_ctx, pax_cursor, 100)
            .await;
        assert!(cross.is_none(), "cross-persona cursor must be refused");
    }

    #[tokio::test]
    async fn stub_source_refuses_wrong_source_id_cursor() {
        let source = StubRagSource::new("conversation", persona(), vec![item("a", 10)]);
        let alien_cursor = ContinuationCursor {
            persona_id: persona(),
            source_id: "memories".to_string(),
            opaque: serde_json::json!({ "next": 0 }),
        };
        let cross = source.deliver_continuation(&ctx(), alien_cursor, 100).await;
        assert!(cross.is_none(), "wrong-source cursor must be refused");
    }

    #[tokio::test]
    async fn stub_source_refuses_wrong_persona_ctx() {
        // The defense-in-depth check: source bound to persona A,
        // called with ctx for persona B — must return empty rather
        // than serve B's caller with A's content.
        let pax = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000abc").unwrap();
        let maya = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000def").unwrap();
        let pax_source = StubRagSource::new("stub", pax, vec![item("a", 10)]);
        let maya_ctx = RagContext::for_persona(maya, 1_000_000);
        let delivery = pax_source
            .deliver(&maya_ctx, 100, ResolutionPreference::Raw)
            .await;
        assert_eq!(delivery.items.len(), 0);
        assert_eq!(delivery.resolution_used, ResolutionPreference::Placeholder);
    }

    // what this catches (#338, measured live 2026-08-06): a source being dropped
    // WHOLE because its declared floor was its COMFORTABLE size rather than the
    // cost of its first complete unit.
    //
    // Every grounding source declared floor=min=500 (a hardcoded number in
    // unified.rs, unrelated to any of them) while their real first units measured
    // 6..40 tokens — about 104 for one unit from all six. On a node whose
    // grounding budget measured 0..214, that made the allocator drop EVERY source
    // on 100% of turns (137/137 and 132/132 for two citizens), so a citizen who
    // could have been told "you hold 0 cards; 59 claimable" in 26 tokens was told
    // nothing and reported she had no work. Grounding was all-or-nothing when it
    // should have degraded.
    //
    // The invariant: at a budget that fits the FLOORS but not the comfortable
    // sizes, every source still speaks. Floor is survival; min is appetite.
    #[test]
    fn sources_survive_at_their_first_unit_when_budget_wont_fit_their_appetite() {
        let adapter = FlexboxRagBudgetAdapter::new();
        // Real measured first units; 500 is the comfortable target each would
        // like. Total floors = 104, total appetites = 2000.
        let sources = [
            budget("room", 10, 6, 500, 2000, false),
            budget("room-kanban", 10, 26, 500, 2000, false),
            budget("workspace-map", 10, 32, 500, 2000, false),
            budget("recall", 10, 40, 500, 2000, false),
        ];
        // 214 tokens — the live starved budget. Fits all four floors (104),
        // fits not even ONE comfortable size.
        let alloc = adapter.allocate(
            &RagContext::for_persona(uuid::Uuid::nil(), 0),
            214,
            ReservedTokens {
                system: 0,
                completion: 0,
            },
            &sources,
        );
        for (i, s) in sources.iter().enumerate() {
            assert!(
                alloc.allocations[i].allocated_tokens >= s.floor_tokens,
                "`{}` must survive at its first unit ({} tokens) on a starved budget — \
                 got {}. A source dropped here is a citizen told nothing at all.",
                s.source_id,
                s.floor_tokens,
                alloc.allocations[i].allocated_tokens,
            );
            assert!(
                !matches!(alloc.allocations[i].state, AllocationState::Dropped),
                "`{}` was DROPPED at a budget that fits its floor — this is the #338 defect",
                s.source_id,
            );
        }
    }

    /// What this catches: the window-scaled reservation stays ONE derivation
    /// with the documented shape — a second hand-copied variant (the exact
    /// duplication this replaced across unified.rs / rag_inspect.rs) shows up
    /// as a drift here. Exhausted windows must not fabricate source capacity
    /// (card 2bbee27a); construction and allocation share the same headroom.
    #[test]
    fn reserved_tokens_scaled_shape_is_the_one_derivation() {
        // Compat-tier 2048: percentages, floors active.
        let small = ReservedTokens::scaled_for_window(2048);
        assert_eq!(small.system, 204); // 10%, inside [128, 512]
        assert_eq!(small.completion, 512); // 25%, inside [256, 4_000]
        assert_eq!(small.headroom_within(2048), 2048 - 204 - 512);

        // M-series 32k: both ceilings engage.
        let big = ReservedTokens::scaled_for_window(32_768);
        assert_eq!(big.system, 512);
        assert_eq!(big.completion, 4_000);
        assert_eq!(big.headroom_within(32_768), 32_768 - 4_512);

        // Reservations exhaust the window: report no capacity rather than 512
        // tokens that the lane cannot hold.
        let tiny = ReservedTokens::scaled_for_window(256);
        assert_eq!((tiny.system, tiny.completion), (128, 256));
        assert_eq!(tiny.headroom_within(256), 0);
        for window in [0, 256, 384, 400, 512, 2_048, 51_712] {
            let reserved = ReservedTokens::scaled_for_window(window);
            let allocation = FlexboxRagBudgetAdapter::new().allocate(
                &RagContext::for_persona(uuid::Uuid::nil(), 0),
                window,
                reserved,
                &[],
            );
            assert_eq!(reserved.headroom_within(window), allocation.available_for_sources);
            assert!(allocation.available_for_sources <= window);
        }
    }
