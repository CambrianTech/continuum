//! A queued dream holds nothing a resident needs (card 920c43bc).
#![cfg(test)]
use super::*;

// What this catches (Fable, M5 0b2854327 01:24-01:50Z): a dream parked on the single scratch
// slot (`acquire_slot(scratch)`) while holding the adapter permit and a serving lane its
// distiller had taken before generate, so residents waited at the lane gate (Aiko 1,141 s,
// Benchy 132 s) with two slots idle. Queued, a dream now holds no adapter permit and no lane;
// admitted, it holds exactly one adapter permit, the scratch slot, and still no lane.
#[tokio::test]
async fn a_dream_queued_on_the_scratch_slot_holds_no_adapter_permit_and_no_lane() {
    let pool = Arc::new(KvSlotPool::new("test://queued-dream", 3)); // index 2 is scratch
    let scratch = pool.scratch_slot().expect("three slots reserve a scratch slot"); // test: pool contract
    let held = pool.acquire_slot(scratch).await.expect("test: another call holds scratch");
    let sem = Arc::new(Semaphore::new(3));
    let client = reqwest::Client::new();
    let queued = admit_turn(&sem, SlotClass::Dream, None, Some(pool.clone()), &client, "test://queued-dream", 0, std::time::Duration::ZERO);
    tokio::pin!(queued);
    assert!(futures::poll!(&mut queued).is_pending(), "the dream waits for the scratch slot");
    assert_eq!(sem.available_permits(), 3, "a queued dream holds no adapter permit");
    drop(held);
    let admitted = queued.await.expect("test: admitted once scratch is released");
    assert_eq!(admitted.scratch_slot(), Some(scratch));
    assert!(admitted._lane.is_none(), "on a scratch slot a dream takes no resident lane");
    assert_eq!(sem.available_permits(), 2, "admitted, it holds one adapter permit");
}

// What this catches: with no scratch slot a dream borrows a citizen slot, so it must still
// yield to residents through the lane gate (the 2026-07-26 failure: dreams on any lane, a
// directed reply queued behind them), yet a dream QUEUED on a busy citizen slot holds no lane
// and no adapter permit (Fable on #5786: the lane was held across the slot wait). Other
// background traffic (a summary, possibly nested in a lane-holding turn) takes no lane.
// One test, so the process-wide lane gate's count is read with nothing else of ours on it.
#[tokio::test]
async fn without_a_scratch_slot_a_dream_queues_holding_nothing_runs_holding_one_ambient_lane_and_waits_for_the_adapter_holding_no_slot_and_no_lane() {
    use crate::inference::resource_admission::serving_lane_permits_available as lanes_free;
    let pool = Arc::new(KvSlotPool::new("test://dream-borrow", 2));
    assert!(pool.scratch_slot().is_none(), "two lanes reserve no scratch slot");
    let sem = Arc::new(Semaphore::new(2));
    let client = reqwest::Client::new();
    let before = lanes_free();

    let busy = pool.acquire_slot(pool.transient_slot()).await.expect("test: a decode holds the citizen slot");
    let queued = admit_turn(&sem, SlotClass::Dream, None, Some(pool.clone()), &client, "test://dream-borrow", 0, std::time::Duration::ZERO);
    tokio::pin!(queued);
    assert!(futures::poll!(&mut queued).is_pending(), "the dream waits for the busy citizen slot");
    assert_eq!(sem.available_permits(), 2, "a queued dream holds no adapter permit");
    assert_eq!(lanes_free(), before, "a queued dream holds no serving lane");
    drop(busy);
    let dream = queued.await.expect("test: admitted once the slot is released");
    assert!(dream._lane.is_some(), "a borrowing dream runs holding an ambient lane");
    assert_eq!(lanes_free(), before - 1, "exactly one");
    drop(dream);
    assert_eq!(lanes_free(), before, "and gives it back");

    // The inversion Codex named on #5786: a resident holds the adapter permit while it waits for
    // a slot. A dream waiting for the adapter must then hold no slot, or each waits on the other.
    let one = Arc::new(Semaphore::new(1));
    let resident = DecodeBound::from(&one).try_acquire().expect("test: a resident holds the only adapter permit");
    let waiting = admit_turn(&one, SlotClass::Dream, None, Some(pool.clone()), &client, "test://dream-borrow", 0, std::time::Duration::ZERO);
    tokio::pin!(waiting);
    assert!(futures::poll!(&mut waiting).is_pending(), "the dream waits for the adapter permit");
    assert!(pool.try_acquire_slot(pool.transient_slot()).is_some(), "while it waits for the adapter it holds no slot");
    assert_eq!(lanes_free(), before, "and no serving lane (Codex on #5786)");
    drop(resident);
    let admitted = waiting.await.expect("test: admitted once the resident's permit returns");
    assert!(admitted._lane.is_some());
    drop(admitted);

    // Codex on #5786: the lane follows the ADMITTED generation's pool, never the discovery the
    // call came with. A stale three-slot discovery (scratch) over a two-slot endpoint borrows.
    let root = format!("test://dream-generation-{}", Uuid::new_v4());
    let _admitted_pool = crate::inference::slots::directory().ensure_pool(&root, 2);
    let stale_discovery = Arc::new(KvSlotPool::new(&root, 3));
    let after_transition = admit_turn(&sem, SlotClass::Dream, None, Some(stale_discovery), &client, &root, 0, std::time::Duration::ZERO)
        .await
        .expect("test: the dream borrows a citizen slot of the admitted generation");
    assert!(after_transition._lane.is_some(), "a dream on a no-scratch generation holds its lane");
    drop(after_transition);
    assert_eq!(lanes_free(), before, "and gives it back");

    let summary = admit_turn(&sem, SlotClass::Background, None, Some(pool.clone()), &client, "test://dream-borrow", 0, std::time::Duration::ZERO)
        .await
        .expect("test: background traffic borrows a free citizen slot");
    assert!(summary._lane.is_none(), "other background traffic takes no lane");
    drop(summary);

    // A KEYED dream (her learning step, card 23069702; Codex's two hazards). Here, not in a test of
    // its own, so the process-wide lane count above is read with nothing else of ours on it.
    // (1) Her act holds her gate: the dream is refused at once, having held nothing.
    let her = PageKey::mind(Uuid::new_v4()).expect("test: a mind key");
    let act = crate::inference::slots::MindTurn::begin(her).await.expect("test: her act takes her gate");
    // Bounded: a keyed dream that queues for her gate (the regression) would wait here forever.
    let refused = tokio::time::timeout(std::time::Duration::from_secs(5), admit_turn(&sem, SlotClass::Dream, Some(her), Some(pool.clone()), &client, "test://dream-borrow", 0, std::time::Duration::ZERO))
        .await
        .expect("a keyed dream is refused at once while her act holds her gate, never queued behind it");
    assert_eq!(refused.err().as_deref(), Some(HER_ACT_FIRST), "her act comes first");
    assert_eq!(sem.available_permits(), 2, "the refused dream holds no adapter permit");
    assert_eq!(lanes_free(), before, "and no serving lane");
    drop(act);
    // (1b) FIFO (Codex): an act already QUEUED for her gate is not overtaken by the dream's try.
    // When the holder releases, the gate passes to the queued act, never to a try that arrives.
    let holder = crate::inference::slots::MindTurn::begin(her).await.expect("test: one act holds her gate");
    let queued_act = crate::inference::slots::MindTurn::begin(her);
    tokio::pin!(queued_act);
    assert!(futures::poll!(&mut queued_act).is_pending(), "her next act queues for the gate");
    drop(holder);
    let refused = tokio::time::timeout(std::time::Duration::from_secs(5), admit_turn(&sem, SlotClass::Dream, Some(her), Some(pool.clone()), &client, "test://dream-borrow", 0, std::time::Duration::ZERO))
        .await
        .expect("a keyed dream behind a queued act is refused at once, never queued behind it");
    assert_eq!(refused.err().as_deref(), Some(HER_ACT_FIRST), "the queued act, not the dream, takes the released gate");
    let next_act = queued_act.await.expect("test: the queued act gets her gate");
    drop(next_act);
    assert_eq!(lanes_free(), before, "the refused dream gave back its ambient lane");
    // (2) Waiting for her slot, it holds no gate, so her act is never queued behind it.
    let busy_a = pool.acquire_slot(0).await.expect("test: a decode holds slot 0");
    let busy_b = pool.acquire_slot(1).await.expect("test: a decode holds slot 1");
    let queued = admit_turn(&sem, SlotClass::Dream, Some(her), Some(pool.clone()), &client, "test://dream-borrow", 0, std::time::Duration::ZERO);
    tokio::pin!(queued);
    assert!(futures::poll!(&mut queued).is_pending(), "the keyed dream waits for her slot");
    assert!(crate::inference::slots::MindTurn::try_begin(her).is_some(), "a waiting keyed dream holds no gate: her act would take it now");
    assert_eq!(lanes_free(), before, "nor a serving lane while it waits for a busy slot (Codex on #5898)");
    drop((busy_a, busy_b));
    let learning = queued.await.expect("test: admitted once a slot frees");
    assert!(learning._lane.is_some(), "a keyed dream runs holding an ambient lane");
    assert!(crate::inference::slots::MindTurn::try_begin(her).is_none(), "admitted, it holds her gate for the step");
    drop(learning);
    assert_eq!(lanes_free(), before, "and gives the lane back");
    // (2b) Every slot PINNED by other minds' turns (the lease finds nothing): it waits holding no
    // lane and no gate, then runs once a turn releases (Codex on #5898: the lane was held here).
    let a = PageKey::mind(Uuid::new_v4()).expect("test: a mind key");
    let b = PageKey::mind(Uuid::new_v4()).expect("test: a mind key");
    let turn_a = admit_turn(&sem, SlotClass::Turn, Some(a), Some(pool.clone()), &client, "test://dream-borrow", 0, std::time::Duration::ZERO).await.expect("test: a's turn pins a slot");
    let turn_b = admit_turn(&sem, SlotClass::Turn, Some(b), Some(pool.clone()), &client, "test://dream-borrow", 0, std::time::Duration::ZERO).await.expect("test: b's turn pins the other");
    // Its own adapter semaphore: the two turns hold the shared one's permits, and this scenario is
    // about pins, not the adapter.
    let own_permits = Arc::new(Semaphore::new(2));
    let behind_pins = admit_turn(&own_permits, SlotClass::Dream, Some(her), Some(pool.clone()), &client, "test://dream-borrow", 0, std::time::Duration::ZERO);
    tokio::pin!(behind_pins);
    assert!(futures::poll!(&mut behind_pins).is_pending(), "every slot is pinned: the keyed dream waits");
    assert_eq!(lanes_free(), before, "holding no serving lane");
    assert!(crate::inference::slots::MindTurn::try_begin(her).is_some(), "and no gate");
    drop((turn_a, turn_b));
    let after_pins = tokio::time::timeout(std::time::Duration::from_secs(5), behind_pins).await.expect("test: a released pin wakes it").expect("test: admitted");
    assert!(after_pins._lane.is_some(), "admitted, it holds its ambient lane");
    drop(after_pins);
    assert_eq!(lanes_free(), before, "and gives it back");
    // (3) A scratch slot does not exempt it: her page is a citizen page, so it still yields to
    // residents through the ambient lane (a keyless dream on scratch takes none, test above).
    let roomy = Arc::new(KvSlotPool::new("test://keyed-dream-scratch", 3));
    assert!(roomy.scratch_slot().is_some(), "three lanes reserve a scratch slot");
    let on_scratch_node = admit_turn(&Arc::new(Semaphore::new(3)), SlotClass::Dream, Some(her), Some(roomy), &client, "test://keyed-dream-scratch", 0, std::time::Duration::ZERO)
        .await
        .expect("test: admitted on her page");
    assert!(on_scratch_node._lane.is_some(), "a keyed dream holds the ambient lane even where a scratch slot exists");
    drop(on_scratch_node);
    assert_eq!(lanes_free(), before, "and gives it back");
    // (4) TWO keyed dreams on her ONE busy slot (card 6742aad1: her learning step and a dream
    // consolidation, the M5 2026-11-13). Queued in the slot's semaphore, each was handed the permit
    // only to drop it while the other's retry found it taken: 35,000 lane grants a second, and
    // neither ran. Now each waits for a release holding nothing, one runs, then the other.
    let busy = (pool.acquire_slot(0).await.expect("test: a decode holds slot 0"), pool.acquire_slot(1).await.expect("test: a decode holds slot 1"));
    let grants = crate::inference::resource_admission::serving_lane_grants();
    let learning = admit_turn(&sem, SlotClass::Dream, Some(her), Some(pool.clone()), &client, "test://dream-borrow", 0, std::time::Duration::ZERO);
    let consolidating = admit_turn(&sem, SlotClass::Dream, Some(her), Some(pool.clone()), &client, "test://dream-borrow", 0, std::time::Duration::ZERO);
    tokio::pin!(learning, consolidating);
    assert!(futures::poll!(&mut learning).is_pending(), "her slot is busy: the learning step waits");
    assert!(futures::poll!(&mut consolidating).is_pending(), "and so does the consolidation");
    assert_eq!(lanes_free(), before, "both wait holding no lane");
    drop(busy);
    // Bounded: under the handoff ping-pong the first never got the slot and this timed out.
    let ran = tokio::time::timeout(std::time::Duration::from_secs(5), &mut learning).await.expect("one runs once her slot is released, never a spin").expect("test: admitted");
    assert!(futures::poll!(&mut consolidating).is_pending(), "the other waits for her slot again, once");
    assert_eq!(lanes_free(), before - 1, "holding no lane while it waits; the running one holds one");
    drop(ran);
    let then = tokio::time::timeout(std::time::Duration::from_secs(5), &mut consolidating).await.expect("the other runs when the first releases her slot").expect("test: admitted");
    drop(then);
    // Five passes take one lane each (two per dream to the first release, one to the second's
    // admission); the spin took thousands. Counted on this test's thread only.
    let taken = crate::inference::resource_admission::serving_lane_grants() - grants;
    assert!(taken <= 5, "each wait is one pass, never a spin: {taken} lane grants");
    assert_eq!(lanes_free(), before, "and every lane comes back");
}
