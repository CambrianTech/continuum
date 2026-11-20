use super::*;
use crate::resources::consumer::ConsumerFootprint;

/// The M5, 2026-08-19, as `resources/board` reported it while the planner was
/// choosing what to serve. Real numbers — this test IS the incident.
const CAPACITY: u64 = 55_125_917_696; // 51.3 GiB vram ceiling
const SERVING_RESIDENT: u64 = 30_253_811_702; // 28.2 GiB of Qwen3.8-27B weights+KV
const A_27B_NEEDS: u64 = 30_000_000_000; // roughly what it was already holding

fn ledger_serving_a_27b() -> ResourceLeaseLedger {
    let mut l = ResourceLeaseLedger::new();
    l.set_capacity(ResourceKind::Vram, CAPACITY);
    // The board's physical_used included serving's own weights, as it must —
    // they ARE resident.
    l.set_physical_used(ResourceKind::Vram, 38_171_508_736);
    l.set_measured(
        "serving",
        vec![ConsumerFootprint {
            kind: ResourceKind::Vram,
            bytes: SERVING_RESIDENT,
            detail: "Qwen3.8-27B weights+KV resident".into(),
        }],
    );
    l
}

// what this catches: THE RATCHET. A machine already serving model X must always
// be able to re-plan model X. Before `budget_for_replacing`, serving was handed
// `available_for` — which subtracts serving's own 28 GB from serving's own budget
// — so a 27B did not "fit" on a box that was RUNNING IT. The planner stepped down
// to a 7B, then a 0.5B, and could never climb back, because each smaller incumbent
// still ate its own successor's headroom. That is #438 and #214, one defect.
#[test]
fn a_machine_serving_a_27b_can_still_plan_a_27b() {
    let l = ledger_serving_a_27b();

    let circular = l.available_for("serving", ResourceKind::Vram);
    assert!(
        circular < A_27B_NEEDS,
        "precondition: the OLD number must be too small for the model already \
         running — that is the bug being pinned ({circular} >= {A_27B_NEEDS})"
    );

    let honest = l.budget_for_replacing("serving", ResourceKind::Vram);
    assert!(
        honest >= A_27B_NEEDS,
        "a machine running a 27B must be able to choose a 27B: budget {honest} < {A_27B_NEEDS}"
    );
    assert_eq!(honest, circular + SERVING_RESIDENT);
}

// what this catches (the 5090, 2026-09-17): a declared footprint LARGER than the
// device. Serving declared 67.5 GB (`peak_resident_bytes` at a 144k window, f16
// KV) on a 33.6 GB card; available fell to 2.3 GB; the add-back produced a 69.8 GB
// budget the planner read as `usable_gb=54` and sized a window against. The
// replace-myself budget is capped at capacity — the device is the ceiling of
// every number this ledger speaks.
#[test]
fn the_replace_myself_budget_never_exceeds_the_device() {
    const CARD: u64 = 33_654_046_720; // the 5090 as the board reports it
    const DECLARED: u64 = 67_555_372_534; // serving's own over-estimate
    let mut l = ResourceLeaseLedger::new();
    l.set_capacity(ResourceKind::Vram, CARD);
    l.set_physical_used(ResourceKind::Vram, 31_372_345_344);
    l.set_measured(
        "serving",
        vec![ConsumerFootprint {
            kind: ResourceKind::Vram,
            bytes: DECLARED,
            detail: "27B at 144640 ctx, f16 KV estimate".into(),
        }],
    );
    let budget = l.budget_for_replacing("serving", ResourceKind::Vram);
    assert_eq!(budget, CARD, "capped at the device, not the declaration");
    assert!(
        l.available_for("serving", ResourceKind::Vram) + DECLARED > CARD,
        "precondition: the uncapped sum really does exceed the card"
    );
}

// what this catches: the add-back leaking into OTHER consumers' budgets. Serving
// releasing its weights is part of serving's swap; it is not headroom for vision or
// staging, and granting it to them would over-commit the pool.
#[test]
fn the_add_back_is_only_ever_the_callers_own_residency() {
    let l = ledger_serving_a_27b();
    assert_eq!(
        l.budget_for_replacing("vision", ResourceKind::Vram),
        l.available_for("vision", ResourceKind::Vram),
        "a peer must see NO add-back from serving's residency"
    );
}

// The 5090 had 302 MiB free, less than the embedding context's outstanding floor.
// Releasing serving's incumbent must pay that deficit before sizing its successor.
#[test]
fn a_replacement_honors_a_floor_that_exceeds_current_free_memory() {
    let mut l = ResourceLeaseLedger::new();
    let kind = ResourceKind::Vram;
    l.set_capacity(kind, 34_190_917_632);
    l.set_physical_used(kind, 33_874_247_680);
    let resident = |bytes| vec![ConsumerFootprint { kind, bytes, detail: "resident".into() }];
    l.set_measured("serving", resident(28_963_869_824));
    l.set_measured("embed", resident(633_205_120));
    l.reserve("embed", kind, 633_205_120 + 352_370_816);

    assert_eq!(l.available_for("learning", kind), 0);
    assert_eq!(l.budget_for_replacing("serving", kind), 28_928_168_960);
}

#[test]
fn replacing_keeps_pending_peer_grants_on_each_pool() {
    let gpu = ResourceKind::Vram;
    for backing in [None, Some(ResourceKind::Ram)] {
        let root = backing.unwrap_or(gpu);
        let mut ledger = ResourceLeaseLedger::new();
        ledger.set_capacity(root, 100);
        ledger.set_capacity(gpu, 100);
        ledger.set_backing(gpu, backing);
        ledger.set_physical_used(root, 70);
        ledger.acquire(&req("peer", root, 20, ReclaimPolicy::Graceful), "peer-lease".into(), 0).unwrap();
        ledger.set_physical_used(root, 90);
        ledger.set_measured("serving", vec![ConsumerFootprint { kind: gpu, bytes: 40, detail: "model".into() }]);
        assert_eq!(ledger.budget_for_replacing("serving", gpu), 30);
    }
}

#[test]
fn inflated_residency_cannot_reclaim_a_peers_measured_bytes() {
    let gpu = ResourceKind::Vram;
    for backing in [None, Some(ResourceKind::Ram)] {
        let root = backing.unwrap_or(gpu);
        let mut ledger = ResourceLeaseLedger::new();
        ledger.set_capacity(gpu, 100);
        ledger.set_capacity(root, 100);
        ledger.set_backing(gpu, backing);
        ledger.set_physical_used(root, 90);
        ledger.set_measured("serving", vec![ConsumerFootprint { kind: gpu, bytes: 100, detail: "model estimate".into() }]);
        ledger.set_measured("embed", vec![ConsumerFootprint { kind: root, bytes: 10, detail: "embedding".into() }]);
        ledger.reserve("embed", root, 10);
        assert_eq!(ledger.budget_for_replacing("serving", gpu), 90);
    }
}

// what this catches: someone reaching for the replace-myself budget on the ACQUIRE
// path. A lease granted against bytes not yet released is a real over-commit — the
// add-back is only valid for a DECISION about what to become next.
#[test]
fn acquire_still_refuses_against_bytes_not_yet_released() {
    let mut l = ledger_serving_a_27b();
    let honest = l.budget_for_replacing("serving", ResourceKind::Vram);
    let err = l.acquire(
        &req("serving", ResourceKind::Vram, honest, ReclaimPolicy::Graceful),
        "lease-1".into(),
        0,
    );
    assert!(
        err.is_err(),
        "acquire must still check available_for — the replace-myself budget is a \
         planning number, never a grantable one"
    );
}
