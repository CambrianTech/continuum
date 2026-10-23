//! One act at a time per mind ([`crate::inference::slots::MindTurn`]).
#![cfg(test)]
use super::*;

// What this catches (plan line 3b, card 424e0961): with one page per mind, two acts of one
// mind dispatched at once (her service loop and `persona/turn-execute` on the same live
// cycle, or `cognition/respond` beside it) ran at the same time: the second leased her slot
// and waited for its permit HOLDING A LANE another mind could use. Now the second (and a
// third) queue behind the running act before any lane, in the order they arrived, and none
// is dropped; another mind's act is admitted at once meanwhile.
#[tokio::test]
async fn a_minds_concurrent_acts_run_one_after_another_in_order_and_none_is_dropped() {
    let root = "test://one-act-per-mind";
    let pool = Arc::new(KvSlotPool::new(root, 2));
    let lanes = Arc::new(Semaphore::new(2));
    let client = reqwest::Client::new();
    let (her, other) = (Uuid::new_v4(), Uuid::new_v4());
    let page = |persona| PageKey::mind(persona).expect("test: non-nil id");
    let admit = |key: PageKey| admit_turn(&lanes, Some(key), Some(pool.clone()), &client, root, 100, std::time::Duration::ZERO);

    // Each act completes its generation, so her slot stays attributed to her for the next.
    let mut first = admit(page(her)).await.expect("test: her first act is admitted");
    let her_slot = first.slot().expect("test: her act holds her slot");
    let finished = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let queued = |n: u32| {
        let (lanes, pool, client, finished) = (lanes.clone(), pool.clone(), client.clone(), finished.clone());
        tokio::spawn(async move {
            let mut act = admit_turn(&lanes, Some(page(her)), Some(pool), &client, root, 100, std::time::Duration::ZERO)
                .await
                .expect("test: a queued act is admitted, never dropped");
            finished.lock().push((n, act.slot(), act.page_in()));
            act.generation_completed();
            // Hold the act briefly so the next one could only overlap it if it were not queued.
            tokio::task::yield_now().await;
            drop(act);
        })
    };
    let second = queued(2);
    tokio::task::yield_now().await;
    let third = queued(3);
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
    assert!(finished.lock().is_empty(), "a second act of her mind ran while her first was running");
    assert_eq!(lanes.available_permits(), 1, "her queued acts hold no lane: only her running act does");

    let theirs = tokio::time::timeout(std::time::Duration::from_secs(5), admit(page(other)))
        .await
        .expect("test: another mind is not queued behind her")
        .expect("test: admitted");
    assert_ne!(theirs.slot(), Some(her_slot));
    drop(theirs);

    first.generation_completed();
    drop(first);
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        second.await.expect("test: second act task");
        third.await.expect("test: third act task");
    })
    .await
    .expect("test: both queued acts ran once her first finished");
    let finished = finished.lock().clone();
    assert_eq!(finished.iter().map(|(n, _, _)| *n).collect::<Vec<_>>(), vec![2, 3], "in the order they arrived");
    for (n, slot, page_in) in finished {
        assert_eq!((slot, page_in), (Some(her_slot), PageIn::Resident), "act {n} reused her warm slot");
    }
}
