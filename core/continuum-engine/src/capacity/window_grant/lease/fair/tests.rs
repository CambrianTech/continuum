#![cfg(test)]

use super::*;
use crate::inference::planning::window_allocator::test_tokens as t;

fn engine() -> Option<EngineIncarnation> {
    Some(EngineIncarnation { pid: 7, started_s: 123, port: 8085 })
}

fn mind(n: u128) -> uuid::Uuid {
    uuid::Uuid::from_u128(n)
}

/// A ready mind needing `need` tokens, of at most `ceiling` per sequence.
fn demand(mind: uuid::Uuid, need: u32, ceiling: u32) -> Demand<uuid::Uuid> {
    Demand { mind, need: t(need), target: t(need), wish: t(need), in_flight: false, settled: true, ceiling }
}

/// One dispatch owner and the publication it last made, on one engine.
struct Bench {
    owner: &'static Reservations,
    dispatch: DispatchState,
    current: Option<PoolGrants>,
    on: Option<EngineIncarnation>,
}

impl Bench {
    fn new() -> Self {
        Self { owner: Box::leak(Box::new(Reservations::new())), dispatch: DispatchState::default(), current: None, on: engine() }
    }

    /// One evaluation over `demands` on a pool of `pool` tokens, `pool` per sequence too.
    fn publish(&mut self, demands: &[Demand<uuid::Uuid>], pool: u32) -> Vec<GrantHandoff> {
        self.publish_in(demands, pool, pool)
    }

    /// One evaluation over `demands` on a pool of `pool` tokens, at most `ceiling` per sequence.
    fn publish_in(&mut self, demands: &[Demand<uuid::Uuid>], pool: u32, ceiling: u32) -> Vec<GrantHandoff> {
        let before = self.current.clone();
        let owner = self.owner;
        let mut published = None;
        let tickets = owner
            .dispatch_publish(&mut self.dispatch, Publication { demands, pool, ceiling, engine: self.on }, || before, |next| {
                assert!(owner.held.try_lock().is_err(), "publication and dispatch reservation share one transaction");
                published = Some(next);
            })
            .1;
        self.current = published;
        tickets
    }

    /// Every mind needing 60 of a 100 pool.
    fn tick(&mut self, minds: &[uuid::Uuid]) -> Vec<GrantHandoff> {
        let demands: Vec<_> = minds.iter().map(|m| demand(*m, 60, 100)).collect();
        self.publish(&demands, 100)
    }

    fn grants(&self) -> BTreeMap<uuid::Uuid, u32> {
        self.current.as_ref().expect("test: published").by_mind.clone() // test: a publication ran
    }

    fn lease(&self, mind: uuid::Uuid) -> GrantLease {
        self.current.as_ref().and_then(|c| c.lease_for(mind)).expect("test: she holds a grant") // test: granted above
    }

    /// One completed act of `mind` on her published grant.
    fn serve(&self, mind: uuid::Uuid) {
        let lease = self.lease(mind);
        let request = self.owner.reserve_against(mind, lease, 10, || Some(lease)).expect("test: reserved"); // test: her own grant
        settle(request, Some(&TerminalReceipt { finish: "stop".into() }));
    }
}

// what this catches: an unsent capacity refusal must not require completed service before a mind
// grows into free cells; free cells need no physical reclamation.
#[test]
fn a_mind_grows_into_free_cells_without_a_terminal_receipt() {
    let mut bench = Bench::new();
    let (a, b) = (mind(1), mind(2));
    for floor in [40, 60] {
        assert!(bench.publish(&[demand(a, floor, 100), demand(b, 40, 100)], 100).is_empty());
        assert_eq!(bench.grants(), BTreeMap::from([(a, floor), (b, 40)]));
    }
}

// what this catches (4.9 P3; BigMama, 98 asked and 0 served): one seat turned per evaluation, so a
// large waiter needing two holders waited two evaluations plus two receipts each time. Every holder
// she needs that is not acting is reclaimed in ONE evaluation, and she is granted once both are.
#[test]
fn every_needed_holder_not_acting_is_reclaimed_in_one_evaluation() {
    let mut bench = Bench::new();
    let (a, b, c) = (mind(1), mind(2), mind(3));
    let demands = [demand(a, 20, 100), demand(b, 60, 100), demand(c, 90, 100)];
    bench.publish(&demands, 100);
    bench.serve(a);
    bench.serve(b);
    let tickets = bench.publish(&demands, 100);
    let reclaimed: BTreeSet<_> = tickets.iter().map(GrantHandoff::persona).collect();
    assert_eq!(reclaimed, BTreeSet::from([a, b]), "both seats leave in the same evaluation");
    assert_eq!(bench.dispatch.waiting(c), Some(DispatchWait::Reclaiming));
    for ticket in tickets {
        bench.owner.lock().reclaimed.insert(ticket.persona(), ticket.lease());
    }
    assert!(bench.publish(&demands, 100).is_empty());
    assert_eq!(bench.grants(), BTreeMap::from([(c, 90)]));
}

// what this catches (4.9 I3): a holder mid-act losing her seat. Her act holds it; the waiter waits
// behind it, named as such, and the holder is armed to yield when the act completes.
#[test]
fn an_acting_holder_is_never_reclaimed_and_the_waiter_waits_behind_her_act() {
    let mut bench = Bench::new();
    let (a, b) = (mind(1), mind(2));
    bench.tick(&[a]);
    let lease = bench.lease(a);
    let _act = bench.owner.hold_act(a, || Some(lease)).expect("test: her act holds her grant"); // test: granted above
    assert!(bench.tick(&[a, b]).is_empty(), "no ticket for a mind mid-act");
    assert_eq!(bench.grants(), BTreeMap::from([(a, 60)]));
    assert_eq!(bench.dispatch.waiting(b), Some(DispatchWait::BehindActs { acting: 1 }));
    assert!(bench.owner.lock().yield_after.contains_key(&a), "armed: her seat is reclaimed when her act ends");
}

// what this catches: one holder armed (the one that served most), so a second busy holder chained
// acts past a waiter. Every holder whose seat the waiter may take is armed.
#[test]
fn every_blocking_holder_is_armed_not_one() {
    let mut bench = Bench::new();
    let (a, b, c) = (mind(1), mind(2), mind(3));
    let demands = |minds: &[uuid::Uuid]| minds.iter().map(|m| demand(*m, 50, 100)).collect::<Vec<_>>();
    bench.publish(&demands(&[a, b]), 100);
    let (lease_a, lease_b) = (bench.lease(a), bench.lease(b));
    let _a = bench.owner.reserve_against(a, lease_a, 40, || Some(lease_a)).expect("test: a mid-act"); // test: her grant
    let _b = bench.owner.reserve_against(b, lease_b, 40, || Some(lease_b)).expect("test: b mid-act"); // test: her grant
    bench.publish(&demands(&[a, b, c]), 100);
    let armed: BTreeSet<_> = bench.owner.lock().yield_after.keys().copied().collect();
    assert_eq!(armed, BTreeSet::from([a, b]));
}

// what this catches: a fresh seat thrashing. A seat its holder has not yet acted on is reclaimed
// only for a mind that has waited at least as long; once she has acted on it, it is anyone's.
#[test]
fn a_fresh_seat_waits_for_its_act_unless_an_older_waiter_needs_it() {
    let mut bench = Bench::new();
    let (a, b) = (mind(1), mind(2));
    bench.tick(&[a]);
    assert!(bench.tick(&[a, b]).is_empty(), "a younger waiter does not take a seat its holder has not used");
    assert_eq!(bench.grants(), BTreeMap::from([(a, 60)]));
    bench.serve(a);
    let tickets = bench.tick(&[a, b]);
    assert_eq!(tickets.iter().map(GrantHandoff::persona).collect::<Vec<_>>(), vec![a], "used and not acting: reclaimed");

    // waiting since the same evaluation, even an unused seat leaves through the engine's save and
    // erase: no local receipt is no proof her KV is absent (an adopted engine, Codex on #6060)
    let mut tied = Bench::new();
    let (c, d) = (mind(3), mind(4));
    tied.tick(&[c, d]);
    let ticket = tied.tick(&[c, d]).pop().expect("test: the unused seat's handoff"); // test: d waited as long as c
    assert_eq!(ticket.persona(), c);
    tied.owner.lock().reclaimed.insert(c, ticket.lease());
    tied.tick(&[c, d]);
    assert_eq!(tied.grants().get(&d), Some(&60), "the equally old waiter takes the seat once it is reclaimed");
}

// what this catches (Codex on #6060): reclaim accounting kept for one evaluation only. Cells being
// reclaimed for a mind count toward her need at every later evaluation until they arrive, so a
// partial arrival never reclaims extra donors; when she leaves, they are no one's and go to the pool.
#[test]
fn cells_being_reclaimed_count_for_their_mind_until_they_arrive_or_she_leaves() {
    let mut bench = Bench::new();
    let (a, b, c, d) = (mind(1), mind(2), mind(3), mind(4));
    let need = |m: uuid::Uuid, n: u32| demand(m, n, 100);
    bench.publish(&[need(a, 40), need(b, 40)], 100);
    bench.serve(a);
    bench.serve(b);
    let tickets = bench.publish(&[need(a, 40), need(b, 40), need(c, 80)], 100);
    let mut donors: Vec<_> = tickets.iter().map(GrantHandoff::persona).collect();
    donors.sort();
    assert_eq!(donors, vec![a, b], "both donors in one evaluation: 20 free + 40 + 40 covers her 80");
    let first = tickets.iter().find(|t| t.persona() == a).expect("test: a's handoff"); // test: a is a donor
    bench.owner.lock().reclaimed.insert(a, first.lease());
    let later = bench.publish(&[need(a, 40), need(b, 40), need(c, 80), need(d, 30)], 100);
    assert!(later.is_empty(), "b's cells are still on their way to c: nothing more is reclaimed ({later:?})");
    assert_eq!(bench.dispatch.reclaiming.get(&b), Some(&c), "b's seat stays counted for c");
    bench.publish(&[need(a, 40), need(b, 40), need(d, 30)], 100);
    assert!(!bench.dispatch.reclaiming.values().any(|beneficiary| *beneficiary == c), "c left: nothing is reclaimed for her ({:?})", bench.dispatch.reclaiming);
}

// what this catches: a refused handoff returns authority, never free capacity, and is not retried
// at every evaluation; the holder is asked again only after her next act.
#[test]
fn refusal_restores_the_holder_and_is_not_retried_before_her_next_act() {
    let mut bench = Bench::new();
    let (a, b) = (mind(1), mind(2));
    bench.tick(&[a, b]);
    bench.serve(a);
    let ticket = bench.tick(&[a, b]).pop().expect("test: a handoff"); // test: a served and is not acting
    bench.owner.lock().refused.insert(a, ticket.lease());
    assert!(bench.tick(&[a, b]).is_empty());
    assert_eq!(bench.grants(), BTreeMap::from([(a, 60)]));
    assert!(bench.owner.lock().retiring.is_empty());
    let restored = bench.lease(a);
    bench.owner.reserve_against(a, restored, 50, || Some(restored)).expect("test: refusal is not retirement").release_unsent(); // test: her restored grant
    assert!(bench.tick(&[a, b]).is_empty(), "no retry without a new completed act");
    bench.serve(a);
    assert_eq!(bench.tick(&[a, b]).len(), 1, "after her next act she is asked again");
}

// what this catches (the M5, d6c0dd892, 2026-11-14 09:37:41Z): an act holds its grant: her growth
// waits for her next act, the request dispatches on the lease it was priced on.
#[test]
fn a_need_that_grows_under_a_rendered_act_moves_her_grant_at_the_next_act() {
    let mut bench = Bench::new();
    let a = mind(1);
    bench.publish(&[demand(a, 88, 200)], 200);
    let rendered = bench.lease(a);
    let act = bench.owner.hold_act(a, || Some(rendered)).expect("test: her act holds her grant"); // test: granted above
    assert!(bench.publish(&[demand(a, 90, 200)], 200).is_empty());
    assert_eq!(bench.lease(a), rendered, "the same grant, the same revision, under the act");
    let current = bench.current.clone();
    let request = bench.owner.reserve_against(a, rendered, 80, || current.as_ref().and_then(|c| c.lease_for(a))).expect("test: dispatched on her lease"); // test: unchanged above
    settle(request, Some(&TerminalReceipt { finish: "stop".into() }));
    drop(act);
    bench.publish(&[demand(a, 90, 200)], 200);
    assert_eq!(bench.grants().get(&a), Some(&90), "her grant grows at her next act");
}

// what this catches (the M5, 2026-11-15 15:20-15:45Z): a holder pinned by her own act while her need
// grew kept every other mind out of free cells for 24 minutes. Her act blocks only her own growth.
#[test]
fn a_mind_pinned_by_her_own_act_never_blocks_others_from_free_cells() {
    let mut bench = Bench::new();
    let (a, b, c) = (mind(1), mind(2), mind(3));
    bench.publish_in(&[demand(a, 150, 300)], 600, 300);
    let rendered = bench.lease(a);
    let _act = bench.owner.hold_act(a, || Some(rendered)).expect("test: her act holds her grant"); // test: granted above
    bench.publish_in(&[demand(a, 250, 300), demand(b, 100, 300), demand(c, 100, 300)], 600, 300);
    let grants = bench.grants();
    assert_eq!(grants.get(&a), Some(&150), "her act keeps her grant");
    assert_eq!((grants.get(&b), grants.get(&c)), (Some(&100), Some(&100)), "free cells seat the others: {grants:?}");
}

#[tokio::test]
async fn http_reclamation_proof_publishes_the_waiter_without_freeing_early() {
    use crate::inference::slots::grant_handoff::{fixture_for_publication, reclaim_for_publication};
    let (root, pool, key, server) = fixture_for_publication().await;
    let port = reqwest::Url::parse(&root).expect("test: fixture URL").port().expect("test: fixture port"); // test: the fixture's own URL
    let mut bench = Bench::new();
    bench.on = Some(EngineIncarnation { pid: 7, started_s: 123, port });
    let (a, b) = (key.persona(), uuid::Uuid::new_v4());
    assert!(bench.tick(&[a, b]).is_empty());
    let lease = bench.lease(a);
    bench.serve(a);
    let ticket = bench.tick(&[a, b]).pop().expect("test: a handoff"); // test: a served and is not acting
    assert_eq!(bench.owner.lock().retiring[&a].tokens, 60);
    assert!(!bench.grants().contains_key(&b));
    let proof = tokio::time::timeout(std::time::Duration::from_secs(5), reclaim_for_publication(&root, ticket))
        .await
        .expect("test: reclaim completes") // test: event-exempt: the fixture server, its proof watched first
        .expect("test: verified HTTP proof"); // test: the fixture answers
    bench.owner.record_reclaimed(proof).expect("test: retained proof"); // test: the ticket's own proof
    assert_eq!(bench.owner.lock().retiring[&a].tokens, 60, "proof alone has not published replacement authority");
    assert!(pool.confirms(&key), "resident save remains available after erase");
    assert!(bench.tick(&[a, b]).is_empty());
    assert_eq!(bench.grants(), BTreeMap::from([(b, 60)]));
    let next = bench.current.clone();
    assert!(bench.owner.reserve_against(a, lease, 50, || next.as_ref().and_then(|n| n.lease_for(a))).is_err());
    let lease_b = bench.lease(b);
    bench.owner.reserve_against(b, lease_b, 50, || Some(lease_b)).expect("test: the waiter may dispatch").release_unsent(); // test: her new grant
    server.abort();
}

#[test]
fn waiter_publication_requires_terminal_then_physical_receipt() {
    let mut bench = Bench::new();
    let (a, b) = (mind(1), mind(2));
    assert!(bench.tick(&[a, b]).is_empty());
    let lease = bench.lease(a);
    let reservation = bench.owner.reserve_against(a, lease, 50, || Some(lease)).expect("test: reserved"); // test: her grant
    assert!(bench.tick(&[a, b]).is_empty(), "mid-act: no ticket");
    assert_eq!(bench.grants().get(&a), Some(&60));
    settle(reservation, Some(&TerminalReceipt { finish: "stop".into() }));
    let tickets = bench.tick(&[a, b]);
    assert_eq!(tickets.iter().map(GrantHandoff::persona).collect::<Vec<_>>(), vec![a]);
    assert!(bench.grants().is_empty(), "neither resident may spend capacity awaiting erase");
    assert!(bench.owner.reserve_against(a, lease, 50, || Some(lease)).is_err(), "even a stale publication cannot reacquire");
    assert!(bench.tick(&[a, b]).is_empty());
    assert_eq!(bench.owner.lock().retiring[&a].tokens, 60);
    // Model ONLY the physical receipt here; real HTTP save/erase ordering is covered by
    // slots::grant_handoff tests. No public API can fabricate this sealed proof.
    bench.owner.lock().reclaimed.insert(a, tickets[0].lease());
    assert!(bench.tick(&[a, b]).is_empty());
    assert_eq!(bench.grants(), BTreeMap::from([(b, 60)]));
    assert!(bench.owner.lock().retiring.is_empty());
    let published = bench.current.clone();
    assert!(bench.owner.reserve_against(a, lease, 50, || published.as_ref().and_then(|p| p.lease_for(a))).is_err());
}

// what this catches (fix for orphan-only holders, card 30caaf17):
// an abandoned request does not itself trigger reclamation.
// But when a holder has ONLY unresolved entries — no Live/Rendered custody at any engine —
// their grant is reclaimable for waiters needing capacity. The old request will never complete,
// so it should not pin the seat forever (Fix A: !holds_active_custody admits orphan-only).
#[test]
fn cancellation_does_not_authorize_retirement_or_waiter_admission() {
    let mut bench = Bench::new();
    let (a, b) = (mind(1), mind(2));
    bench.tick(&[a, b]);  // a gets 60
    let lease = bench.lease(a);

    // Create both reservations first (same order as original test)
    let finished = bench.owner.reserve_against(a, lease, 50, || Some(lease)).expect("test: first request");
    let unresolved = bench.owner.reserve_against(a, lease, 50, || Some(lease)).expect("test: an overlapping second request");

    // Settle the first (removes Live entry id=1), abandon the second (changes Live id=2 to Unresolved)
    settle(finished, Some(&TerminalReceipt { finish: "stop".into() }));
    drop(unresolved);

    // With Fix A: orphan-only holder is reclaimable on next tick when waiter needs capacity
    let tickets = bench.tick(&[a, b]);
    assert_eq!(tickets.iter().map(GrantHandoff::persona).collect::<Vec<_>>(), vec![a], "orphan-only holder reclaimed for b on first tick");

    // Consume the reclaim through the real receipt path (exercise record_reclaimed):
    let ticket = tickets.into_iter().next().unwrap();
    crate::capacity::window_grant::record_reclaimed_grant(
        crate::inference::slots::grant_handoff::ReclaimedGrant::scripted(ticket)
    ).expect("test: reclaim recorded");

    // After consuming the reclaim, a's Unresolved entries are dropped and b is seated on next tick.
    assert!(bench.tick(&[a, b]).is_empty());
    assert_eq!(bench.grants(), BTreeMap::from([(b, 60)]), "b seated after orphan reclaim consumed via real receipt path");
}

#[test]
fn completion_bars_reacquisition_before_the_next_evaluation() {
    let mut bench = Bench::new();
    let (a, b) = (mind(1), mind(2));
    bench.tick(&[a, b]);
    let lease = bench.lease(a);
    let unsent = bench.owner.reserve_against(a, lease, 50, || Some(lease)).expect("test: first attempt"); // test: her grant
    unsent.release_unsent();
    let served = bench.owner.reserve_against(a, lease, 50, || Some(lease)).expect("test: an unsent attempt spends no turn"); // test: her grant
    settle(served, Some(&TerminalReceipt { finish: "stop".into() }));
    assert!(matches!(bench.owner.reserve_against(a, lease, 50, || Some(lease)), Err(ReserveRefused::Retiring { .. })),
        "no evaluation has run, but the terminal boundary already bars reacquisition while a mind is owed her seat");
    assert_eq!(bench.grants().get(&a), Some(&60), "capacity remains charged");
    // A departed waiter disarms the obligation at the next evaluation without physical reclamation.
    assert!(bench.tick(&[a]).is_empty());
    let refreshed = bench.lease(a);
    bench.owner.reserve_against(a, refreshed, 50, || Some(refreshed)).expect("test: no remaining waiter").release_unsent(); // test: her grant
}

// what this catches: a reclaimed holder jumping ahead of minds that waited longer. Each seat goes to
// the oldest owed mind, and the holder that gave it up waits from then.
#[test]
fn a_reclaimed_holder_waits_behind_minds_owed_longer() {
    let mut bench = Bench::new();
    let (a, b, c) = (mind(1), mind(2), mind(3));
    bench.tick(&[a, b, c]);
    for (holder, next) in [(a, b), (b, c), (c, a)] {
        bench.serve(holder);
        let tickets = bench.tick(&[a, b, c]);
        assert_eq!(tickets.iter().map(GrantHandoff::persona).collect::<Vec<_>>(), vec![holder]);
        bench.owner.lock().reclaimed.insert(holder, tickets[0].lease());
        assert!(bench.tick(&[a, b, c]).is_empty());
        assert_eq!(bench.grants(), BTreeMap::from([(next, 60)]));
    }
}

// what this catches (the M5, 2026-11-16 21:59-22:12Z, build 2695c5fd1; Codex owns the fix): Aiko and Atlas owed a
// window for the whole log (Aiko's one turn open ~7,183 s) while a big holder (8044b75b, 170,828 of a 301,056 pool,
// `armed`) re-entered its next act within 5 s of its act's end (22:00:53 live0 armed -> 22:00:58 live1) and a small
// holder (2f50b223) was re-granted its ~85k within 2-5 s of every act end (22:03:56 .. 22:12:27), ahead of both.
// Shape, scaled to a pool of 100: the big holder's seat must reach the oldest owed mind at its act's end, even when
// the big holder asks again at once and the small holder keeps cycling.
#[test]
fn a_big_holder_acting_back_to_back_yields_its_seat_to_the_oldest_owed_mind() {
    let mut bench = Bench::new();
    let (big, small, oldest) = (mind(1), mind(2), mind(3));
    // the oldest mind needs BOTH holders' cells (90 of 100), so no evaluation can seat her while either acts
    let demands = [demand(big, 57, 100), demand(small, 28, 100), demand(oldest, 90, 100)];
    bench.publish(&[demand(big, 57, 100), demand(small, 28, 100)], 100);
    assert_eq!(bench.grants(), BTreeMap::from([(big, 57), (small, 28)]));
    let (big_lease, small_lease) = (bench.lease(big), bench.lease(small));
    let big_act = bench.owner.reserve_against(big, big_lease, 50, || Some(big_lease)).expect("test: the big holder mid-act"); // test: its grant
    let small_act = bench.owner.reserve_against(small, small_lease, 20, || Some(small_lease)).expect("test: the small holder mid-act"); // test: its grant
    bench.publish(&demands, 100);
    assert_eq!(bench.dispatch.waiting(oldest), Some(DispatchWait::BehindActs { acting: 2 }));
    assert!(bench.owner.lock().yield_after.contains_key(&big), "the blocking big holder is armed");

    // the big holder's act ends while the small one still acts: an evaluation runs BETWEEN the terminal receipt and
    // the big holder's next reservation, and cannot reclaim enough for her yet (Codex: on 2695c5fd1 every publish
    // cleared yield_after and re-armed from the current receipt, erasing the yield the big holder had earned)
    settle(big_act, Some(&TerminalReceipt { finish: "stop".into() }));
    let partial = bench.publish(&demands, 100);
    assert_ne!(bench.grants().get(&oldest), Some(&90), "she cannot be seated while the small holder acts");
    for ticket in partial {
        bench.owner.lock().reclaimed.insert(ticket.persona(), ticket.lease());
    }

    // the big holder asks again at once (the M5's 5 s re-entry): its earned yield survived that evaluation
    let again = bench.current.as_ref().and_then(|c| c.lease_for(big));
    if let Some(again) = again {
        assert!(matches!(bench.owner.reserve_against(big, again, 50, || Some(again)), Err(ReserveRefused::Retiring { .. })),
            "an armed holder whose act ended cannot re-enter while an older mind is owed its seat, across evaluations");
    }

    // the small holder's act ends; the oldest owed mind is seated once the receipts arrive
    settle(small_act, Some(&TerminalReceipt { finish: "stop".into() }));
    for _ in 0..3 {
        let tickets = bench.publish(&demands, 100);
        for ticket in tickets {
            bench.owner.lock().reclaimed.insert(ticket.persona(), ticket.lease());
        }
    }
    assert_eq!(bench.grants().get(&oldest), Some(&90), "the oldest owed mind is seated: {:?}", bench.grants());
}

// what this catches (the same M5 log): a younger mind that fits being re-granted freed cells again and again while
// the oldest owed mind, who needs those cells plus a busy holder's, waits behind the busy holder forever. Freed
// cells an older owed mind will need are held for her, never backfilled past her.
#[test]
fn a_completed_holder_cannot_restart_while_an_older_mind_needs_her_cells() {
    let mut bench = Bench::new();
    let (big, small, oldest) = (mind(1), mind(2), mind(3));
    bench.publish(&[demand(big, 57, 100), demand(small, 28, 100)], 100);
    let big_lease = bench.lease(big);
    let _big_act = bench.owner.reserve_against(big, big_lease, 50, || Some(big_lease)).expect("test: the big holder mid-act"); // test: its grant
    let small_lease = bench.lease(small);
    let small_act = bench.owner.reserve_against(small, small_lease, 20, || Some(small_lease)).expect("test: small mid-act"); // test: its grant
    let demands = [demand(big, 57, 100), demand(small, 28, 100), demand(oldest, 60, 100)];
    bench.publish(&demands, 100); // the oldest mind is owed from here
    assert!(bench.owner.lock().yield_after.contains_key(&small));
    settle(small_act, Some(&TerminalReceipt { finish: "stop".into() }));
    for round in 0..5 {
        let tickets = bench.publish(&demands, 100);
        for ticket in tickets {
            bench.owner.lock().reclaimed.insert(ticket.persona(), ticket.lease());
        }
        bench.publish(&demands, 100);
        if let Some(lease) = bench.current.as_ref().and_then(|c| c.lease_for(small)) {
            assert!(matches!(bench.owner.reserve_against(small, lease, 20, || Some(lease)), Err(ReserveRefused::Retiring { .. })),
                "round {round}: a cached seat cannot admit another act ahead of the older waiter");
            assert!(bench.owner.hold_act(small, || Some(lease)).is_none(), "the act boundary also respects the earned yield");
        }
    }
}

// Resizing issues a new grant revision but does not pay the holder's debt to an older waiter.
#[test]
fn resizing_a_continuously_held_seat_preserves_its_earned_yield() {
    for resized in [50, 65] {
        let mut bench = Bench::new();
        let (big, small, oldest) = (mind(1), mind(2), mind(3));
        bench.publish(&[demand(big, 57, 100), demand(small, 28, 100)], 100);
        let (big_lease, small_lease) = (bench.lease(big), bench.lease(small));
        let big_act = bench.owner.reserve_against(big, big_lease, 40, || Some(big_lease)).expect("test: big acting"); // test: published grant
        let _small_act = bench.owner.reserve_against(small, small_lease, 20, || Some(small_lease)).expect("test: small acting"); // test: published grant
        bench.publish(&[demand(big, 57, 100), demand(small, 28, 100), demand(oldest, 90, 100)], 100);
        settle(big_act, Some(&TerminalReceipt { finish: "stop".into() }));
        bench.publish(&[demand(big, resized, 100), demand(small, 28, 100), demand(oldest, 90, 100)], 100);
        let next = bench.lease(big);
        assert_eq!(next.tokens, resized);
        assert!(!next.same_identity(&big_lease), "resize stamps a new revision");
        assert!(matches!(bench.owner.reserve_against(big, next, 40, || Some(next)), Err(ReserveRefused::Retiring { .. })),
            "resizing to {resized} must not erase the completed act's yield");
        assert!(bench.owner.hold_act(big, || Some(next)).is_none());
    }
}
