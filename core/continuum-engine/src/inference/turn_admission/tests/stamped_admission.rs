//! A resident act's endpoint admission names the engine process it is bound to on every path that
//! readies a shared-pool engine, so the grant lease's binding (`lane_send`: a lease on another or
//! no incarnation is refused) can never refuse every act. The stamp is the pool's
//! (`KvSlotPool::incarnation`), independent of the page ledger:
//!
//! - an OWNED engine verified ready: `llama_server/ready_tail.rs` calls
//!   `slots::stamp_engine_incarnation` with the child's pid and OS start time, which stamps the
//!   endpoint's pool (`slots/engine_generation.rs`, `stamp_engine_incarnation` ->
//!   `KvSlotPool::stamp_incarnation` -> `mark_incarnation`), with or without a page dir;
//! - a relaunch (the S2 growth relaunch, any replacement) installs a NEW pool for the new engine,
//!   stamped by the same ready path; the old pool's stamp never carries over;
//! - an ADOPTED running engine (a deploy kept it): `slots/discover.rs` marks the pool with the live
//!   lane's verified pid + start time before its page ledger is adopted, so a lane with no page
//!   dir is stamped too;
//! - warm-ahead, page restore, the scratch slot and the borrow path all admit through the same
//!   endpoint and pool (`admit` in `turn_admission.rs`), so they read the same stamp.
#![cfg(test)]
use super::*;

// what this catches: "refuse when unstamped" refusing every act. A pool readied with no page
// ledger is stamped all the same and an admission on it carries the stamp; a pool names one engine
// for its life (a second stamp is ignored), and a replacement engine's new pool starts unstamped
// until its own ready stamps it, never inheriting the old engine's.
#[tokio::test]
async fn every_ready_shared_pool_admission_names_its_engine() {
    let root = format!("test://stamped-{}", Uuid::new_v4());
    let pool = crate::inference::slots::directory().ensure_pool(&root, 2);
    let sem = Arc::new(Semaphore::new(2));
    let client = reqwest::Client::new();
    let key = PageKey::mind(Uuid::from_u128(7)).expect("test: a non-nil id"); // JUSTIFIED: test fixture
    let unstamped = admit_turn(&sem, crate::inference::slots::SlotClass::Turn, Some(key), Some(pool.clone()), &client, &root, 100, std::time::Duration::ZERO)
        .await
        .expect("test: admitted"); // JUSTIFIED: test fixture
    assert_eq!(unstamped.engine_incarnation(), None, "precondition: no engine has been readied here yet");
    drop(unstamped);
    // The owned engine verified ready, with no page ledger attached.
    crate::inference::slots::stamp_engine_incarnation(&root, 4_242, 1_800_000_000);
    let admitted = admit_turn(&sem, crate::inference::slots::SlotClass::Turn, Some(key), Some(pool.clone()), &client, &root, 100, std::time::Duration::ZERO)
        .await
        .expect("test: admitted"); // JUSTIFIED: test fixture
    assert_eq!(admitted.engine_incarnation(), Some((4_242, 1_800_000_000)), "an act on a ready pool names its engine");
    pool.mark_incarnation((9_999, 1));
    assert_eq!(pool.incarnation(), Some((4_242, 1_800_000_000)), "a pool names one engine for its life");
    drop(admitted);
    // A replacement engine (the growth relaunch) comes with a new pool, stamped by its own ready.
    let replacement = KvSlotPool::new(&root, 2);
    assert_eq!(replacement.incarnation(), None, "the old engine's stamp never carries over");
    replacement.mark_incarnation((4_243, 1_800_000_100));
    assert_eq!(replacement.incarnation(), Some((4_243, 1_800_000_100)), "the relaunched engine's own stamp");
}
