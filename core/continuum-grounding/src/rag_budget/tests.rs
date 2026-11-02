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

    /// The window less a fixed reservation: what the one allocation is handed.
    fn reserved(system: u32, completion: u32) -> u32 {
        system.saturating_add(completion)
    }

    fn alloc_for<'a>(result: &'a FlexAllocation, id: &str) -> &'a SourceAllocation {
        result
            .allocations
            .iter()
            .find(|a| a.source_id == id)
            .unwrap()
    }

    #[test]
    fn empty_context_window_under_provisions_required() {
        let result = flex_allocate(500u32.saturating_sub(reserved(400, 200)), &[budget("conversation", 10, 100, 200, 1000, true)]);
        assert!(result.escalation_needed);
        assert_eq!(
            alloc_for(&result, "conversation").state,
            AllocationState::UnderProvisioned
        );
    }

    #[test]
    fn single_required_source_satisfied() {
        let result = flex_allocate(10_000u32.saturating_sub(reserved(500, 2000)), &[budget("conversation", 10, 200, 500, 5000, true)]);
        let conv = alloc_for(&result, "conversation");
        assert!(conv.allocated_tokens >= 500);
        assert_eq!(conv.state, AllocationState::Satisfied);
        assert!(!result.escalation_needed);
    }

    #[test]
    fn priority_distributes_remaining_proportionally() {
        // max well above expected share so neither caps before the
        // priority ratio gets to express.
        let result = flex_allocate(10_000u32.saturating_sub(reserved(0, 0)), &[
                budget("conversation", 10, 100, 500, 50_000, true),
                budget("memories", 5, 100, 500, 50_000, true),
            ]);
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
        let result = flex_allocate(1_000u32.saturating_sub(reserved(500, 200)), &[
                budget("conversation", 10, 200, 200, 500, true),
                budget("artifacts", 3, 200, 200, 500, false),
            ]);
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
        let result = flex_allocate(300u32.saturating_sub(reserved(100, 100)), &[
                budget("conversation", 10, 200, 200, 500, true),
                budget("memories", 5, 200, 200, 500, true),
            ]);
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
        let result = flex_allocate(2_000u32.saturating_sub(reserved(0, 0)), &[
                budget("conversation", 10, 500, 500, 1000, true),
                budget("memories", 5, 200, 600, 1500, false),
            ]);
        let conv = alloc_for(&result, "conversation");
        let mem = alloc_for(&result, "memories");
        assert!(conv.allocated_tokens >= 500);
        assert!(mem.allocated_tokens >= 200);
    }

    #[test]
    fn max_caps_distribution() {
        let result = flex_allocate(10_000u32.saturating_sub(reserved(0, 0)), &[
                budget("tiny", 10, 0, 0, 100, false),
                budget("big", 5, 0, 0, 9_000, false),
            ]);
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
        let result_a = flex_allocate(10_000u32.saturating_sub(reserved(0, 0)), &[
                budget("a", 5, 0, 500, 2000, false),
                budget("b", 5, 0, 500, 2000, false),
            ]);
        let result_b = flex_allocate(10_000u32.saturating_sub(reserved(0, 0)), &[
                budget("b", 5, 0, 500, 2000, false),
                budget("a", 5, 0, 500, 2000, false),
            ]);
        let a_in_a = alloc_for(&result_a, "a").allocated_tokens;
        let a_in_b = alloc_for(&result_b, "a").allocated_tokens;
        assert_eq!(a_in_a, a_in_b, "allocation must be input-order-independent");
    }

    // ---- Source trait + stub tests ----

    fn persona() -> uuid::Uuid {
        uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }

    // what this catches: a source sizing itself (FLOW plan §2d). The stub delivers every item
    // it holds whatever is large, and refuses another persona's ctx with nothing.
    #[tokio::test]
    async fn a_source_delivers_every_item_and_only_to_its_persona() {
        let items = vec![RagItem::whole("a", "a", serde_json::json!({})), RagItem::whole("huge", "x".repeat(40_000), serde_json::json!({}))];
        let source = StubRagSource::new("stub", persona(), items);
        let delivery = source.deliver(&RagContext::for_persona(persona(), 1_000_000)).await;
        assert_eq!(delivery.items.len(), 2);
        assert!(!delivery.truncated);
        let maya = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000def").unwrap();
        assert!(source.deliver(&RagContext::for_persona(maya, 1_000_000)).await.items.is_empty());
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
        let alloc = flex_allocate(214, &sources);
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

    // what this catches: a lane need learned from the GRANT, which the flexbox grows to any
    // window, so the need tracked the window and refused every extra lane (the M5, 10-18).
    #[test]
    fn a_turns_need_is_its_floors_however_much_the_window_granted() {
        let claim = |id: &str, floor: u32, max: u32| RagSourceBudget {
            source_id: id.into(), priority: 5, floor_tokens: floor, min_tokens: floor, max_tokens: max, required: false,
        };
        let claims = [claim("required", 9_000, 9_000), claim("history", 4_000, 60_000), claim("reply", 2_000, 16_000)];
        assert_eq!(turn_need(&claims), 15_000);
        for window in [20_000, 102_912] {
            let granted: u32 = ["required", "history", "reply"].iter().map(|id| flex_allocate(window, &claims).granted(id)).sum();
            assert!(granted >= turn_need(&claims), "a window that holds the floors grants at least them");
        }
        assert_eq!(turn_need(&[claim("a", u32::MAX, u32::MAX), claim("b", 1, 1)]), u32::MAX, "saturates, never wraps");
    }
