//! The allocator's own contract (moved with it from `continuum-grounding::rag_budget`).

use super::*;

fn budget(source_id: &'static str, priority: u8, floor: u32, min: u32, max: u32, required: bool) -> FlexClaim {
    FlexClaim { source_id: source_id.to_string(), priority, floor_tokens: floor, min_tokens: min, max_tokens: max, required }
}

/// The window less a fixed reservation: what the one allocation is handed.
fn reserved(system: u32, completion: u32) -> u32 {
    system.saturating_add(completion)
}

fn alloc_for<'a>(result: &'a FlexAllocation, id: &str) -> &'a SourceAllocation {
    result.of(id).expect("test: the claim was made") // JUSTIFIED: test fixture
}

// what this catches: a required claim on a window too small for its floor reading as served.
#[test]
fn empty_context_window_under_provisions_required() {
    let result = flex_allocate(500u32.saturating_sub(reserved(400, 200)), &[budget("conversation", 10, 100, 200, 1000, true)]);
    assert!(result.escalation_needed);
    assert_eq!(alloc_for(&result, "conversation").state, AllocationState::UnderProvisioned);
}

// what this catches: a lone required claim not reaching its min when the window holds it.
#[test]
fn single_required_source_satisfied() {
    let result = flex_allocate(10_000u32.saturating_sub(reserved(500, 2000)), &[budget("conversation", 10, 200, 500, 5000, true)]);
    let conv = alloc_for(&result, "conversation");
    assert!(conv.allocated_tokens >= 500);
    assert_eq!(conv.state, AllocationState::Satisfied);
    assert!(!result.escalation_needed);
}

// what this catches: growth that ignores priority weight.
#[test]
fn priority_distributes_remaining_proportionally() {
    let result = flex_allocate(10_000, &[
        budget("conversation", 10, 100, 500, 50_000, true),
        budget("memories", 5, 100, 500, 50_000, true),
    ]);
    let conv = alloc_for(&result, "conversation");
    let mem = alloc_for(&result, "memories");
    assert!(conv.allocated_tokens > mem.allocated_tokens, "conv {} mem {}", conv.allocated_tokens, mem.allocated_tokens);
}

// what this catches: an optional claim kept as a fragment below its floor, or its drop escalating.
#[test]
fn optional_source_drops_when_floor_cant_fit() {
    let result = flex_allocate(1_000u32.saturating_sub(reserved(500, 200)), &[
        budget("conversation", 10, 200, 200, 500, true),
        budget("artifacts", 3, 200, 200, 500, false),
    ]);
    let conv = alloc_for(&result, "conversation");
    let art = alloc_for(&result, "artifacts");
    assert!(conv.allocated_tokens >= 200);
    assert_ne!(conv.state, AllocationState::Dropped);
    assert_eq!(art.allocated_tokens, 0);
    assert_eq!(art.state, AllocationState::Dropped);
    assert!(!result.escalation_needed, "an optional drop is fine");
}

// what this catches: a required claim that cannot get its floor passing silently.
#[test]
fn required_under_provisions_when_floor_cant_fit() {
    let result = flex_allocate(300u32.saturating_sub(reserved(100, 100)), &[
        budget("conversation", 10, 200, 200, 500, true),
        budget("memories", 5, 200, 200, 500, true),
    ]);
    assert!(result.escalation_needed);
    assert_eq!(alloc_for(&result, "memories").state, AllocationState::UnderProvisioned);
}

// what this catches: the floor landing after the min pass instead of before it.
#[test]
fn floor_is_honored_above_min() {
    let result = flex_allocate(2_000, &[
        budget("conversation", 10, 500, 500, 1000, true),
        budget("memories", 5, 200, 600, 1500, false),
    ]);
    assert!(alloc_for(&result, "conversation").allocated_tokens >= 500);
    assert!(alloc_for(&result, "memories").allocated_tokens >= 200);
}

// what this catches: a claim granted past its max.
#[test]
fn max_caps_distribution() {
    let result = flex_allocate(10_000, &[budget("tiny", 10, 0, 0, 100, false), budget("big", 5, 0, 0, 9_000, false)]);
    assert_eq!(alloc_for(&result, "tiny").allocated_tokens, 100);
    let big = alloc_for(&result, "big").allocated_tokens;
    assert!((5_000..=9_000).contains(&big), "{big}");
}

// what this catches: grants that depend on the claims' input order.
#[test]
fn deterministic_priority_tiebreak() {
    let a = flex_allocate(10_000, &[budget("a", 5, 0, 500, 2000, false), budget("b", 5, 0, 500, 2000, false)]);
    let b = flex_allocate(10_000, &[budget("b", 5, 0, 500, 2000, false), budget("a", 5, 0, 500, 2000, false)]);
    assert_eq!(alloc_for(&a, "a").allocated_tokens, alloc_for(&b, "a").allocated_tokens);
}

// what this catches (#338, measured live 2026-08-06): a source dropped WHOLE because its floor
// was its comfortable size rather than the cost of its first complete unit. At a budget that
// fits the FLOORS but not the comfortable sizes, every source still speaks.
#[test]
fn sources_survive_at_their_first_unit_when_budget_wont_fit_their_appetite() {
    let sources = [
        budget("room", 10, 6, 500, 2000, false),
        budget("room-kanban", 10, 26, 500, 2000, false),
        budget("workspace-map", 10, 32, 500, 2000, false),
        budget("recall", 10, 40, 500, 2000, false),
    ];
    let alloc = flex_allocate(214, &sources);
    for (i, s) in sources.iter().enumerate() {
        assert!(alloc.allocations[i].allocated_tokens >= s.floor_tokens, "`{}` must survive at its first unit", s.source_id);
        assert!(!matches!(alloc.allocations[i].state, AllocationState::Dropped), "`{}` was dropped (#338)", s.source_id);
    }
}

// what this catches: a lane need learned from the GRANT, which the flexbox grows to any window,
// so the need tracked the window and refused every extra lane (the M5, 10-18).
#[test]
fn a_turns_need_is_its_floors_however_much_the_window_granted() {
    let claim = |id: &str, floor: u32, max: u32| FlexClaim {
        source_id: id.into(),
        priority: 5,
        floor_tokens: floor,
        min_tokens: floor,
        max_tokens: max,
        required: false,
    };
    let claims = [claim("required", 9_000, 9_000), claim("history", 4_000, 60_000), claim("reply", 2_000, 16_000)];
    assert_eq!(turn_need(&claims), 15_000);
    for window in [20_000, 102_912] {
        let granted: u32 = ["required", "history", "reply"].iter().map(|id| flex_allocate(window, &claims).granted(id)).sum();
        assert!(granted >= turn_need(&claims), "a window that holds the floors grants at least them");
    }
    assert_eq!(turn_need(&[claim("a", u32::MAX, u32::MAX), claim("b", 1, 1)]), u32::MAX, "saturates, never wraps");
}
