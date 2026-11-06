//! Slot exhaustion must park residents without a self-sustaining wake cycle.
//! Exercises the production admission path, not a simulated scheduler.
use super::*;
use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Wake, Waker};

#[derive(Default)]
struct WakeCount(AtomicUsize);

#[tokio::test]
async fn scratch_release_does_not_wake_residents_with_every_citizen_slot_pinned(
) -> Result<(), Box<dyn std::error::Error>> {
    let root = format!("test://scratch-contention-{}", Uuid::new_v4());
    let pool = Arc::new(KvSlotPool::new(&root, 3));
    let sem = Arc::new(Semaphore::new(3));
    let client = reqwest::Client::new();
    let mut holders = Vec::new();
    for _ in 0..2 {
        let key = PageKey::mind(Uuid::new_v4()).ok_or("owner key")?;
        holders.push(
            admit_turn(
                &sem,
                crate::inference::slots::SlotClass::Turn,
                Some(key),
                Some(pool.clone()),
                &client,
                &root,
                100,
                std::time::Duration::ZERO,
            )
            .await?,
        );
    }
    let key = PageKey::mind(Uuid::new_v4()).ok_or("waiting key")?;
    let mut waiting = Box::pin(admit_turn(
        &sem,
        crate::inference::slots::SlotClass::Turn,
        Some(key),
        Some(pool.clone()),
        &client,
        &root,
        100,
        std::time::Duration::ZERO,
    ));
    let counter = Arc::new(WakeCount::default());
    let waker = Waker::from(counter.clone());
    let mut context = Context::from_waker(&waker);
    assert!(waiting.as_mut().poll(&mut context).is_pending());
    let scratch = pool.scratch_slot().ok_or("reserved scratch")?;
    // Both acquisition APIs must avoid advertising a free citizen slot.
    let busy_scratch = pool.acquire_slot(scratch).await?;
    let mut next_scratch = Box::pin(pool.acquire_slot(scratch));
    assert!(futures::poll!(&mut next_scratch).is_pending());
    drop(busy_scratch);
    // Scratch consumers still wake on their own semaphore after the change.
    drop(tokio::time::timeout(std::time::Duration::from_secs(2), next_scratch).await??);
    assert_eq!(
        counter.0.load(Ordering::SeqCst),
        0,
        "scratch cannot admit a resident"
    );
    drop(pool.try_acquire_slot(scratch).ok_or("free scratch")?);
    assert_eq!(
        counter.0.load(Ordering::SeqCst),
        0,
        "scratch try-acquire has the same release contract"
    );
    drop(holders);
    assert!(
        counter.0.load(Ordering::SeqCst) > 0,
        "citizen release still wakes admission"
    );
    Ok(())
}

impl Wake for WakeCount {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn same_mind_wait_releases_the_callers_serving_lane() -> Result<(), Box<dyn std::error::Error>>
{
    use crate::inference::resource_admission::{with_serving_lane, LaneAdmission, LanePriority};
    let root = format!("test://same-mind-lane-{}", Uuid::new_v4());
    let key = PageKey::mind(Uuid::new_v4()).ok_or("mind key")?;
    let pool = Arc::new(KvSlotPool::new(&root, 1));
    let sem = Arc::new(Semaphore::new(2));
    let client = reqwest::Client::new();
    let held = admit_turn(
        &sem,
        crate::inference::slots::SlotClass::Turn,
        Some(key),
        Some(pool.clone()),
        &client,
        &root,
        100,
        std::time::Duration::ZERO,
    )
    .await?;
    let lanes: &'static LaneAdmission = Box::leak(Box::new(LaneAdmission::new()));
    lanes.set_served_lane_count(1);
    let lane = lanes
        .acquire_serving_lane(None, LanePriority::Directed, None)
        .await;
    let mut waiting = Box::pin(with_serving_lane(
        lane,
        admit_turn(
            &sem,
            crate::inference::slots::SlotClass::Turn,
            Some(key),
            Some(pool),
            &client,
            &root,
            100,
            std::time::Duration::ZERO,
        ),
    ));
    assert!(futures::poll!(&mut waiting).is_pending());
    let mut borrowed = Box::pin(lanes.acquire_serving_lane(None, LanePriority::Directed, None));
    assert!(
        futures::poll!(&mut borrowed).is_ready(),
        "same-mind wait must not retain a lane"
    );
    drop(waiting);
    drop(held);
    Ok(())
}

// Compose the real two owners as cognition's SharedLocal call does: its lane
// surrounds adapter admission. This is a custody reproduction, not an HTTP or
// full cognition integration test. An inner permit drop cannot release it.
#[tokio::test]
async fn all_pinned_wait_releases_the_outer_serving_lane() -> Result<(), Box<dyn std::error::Error>>
{
    use crate::inference::resource_admission::{LaneAdmission, LanePriority};
    let root = format!("test://outer-lane-{}", Uuid::new_v4());
    let pool = Arc::new(KvSlotPool::new(&root, 1));
    let sem = Arc::new(Semaphore::new(2));
    let client = reqwest::Client::new();
    let owner = PageKey::mind(Uuid::new_v4()).ok_or("owner key")?;
    let held = admit_turn(
        &sem,
        crate::inference::slots::SlotClass::Turn,
        Some(owner),
        Some(pool.clone()),
        &client,
        &root,
        100,
        std::time::Duration::ZERO,
    )
    .await?;
    let lanes: &'static LaneAdmission = Box::leak(Box::new(LaneAdmission::new()));
    lanes.set_served_lane_count(1);
    let outer = lanes
        .acquire_serving_lane(None, LanePriority::Directed, None)
        .await;
    let key = PageKey::mind(Uuid::new_v4()).ok_or("waiting key")?;
    let mut waiting = Box::pin(crate::inference::resource_admission::with_serving_lane(
        outer,
        async {
            admit_turn(
                &sem,
                crate::inference::slots::SlotClass::Turn,
                Some(key),
                Some(pool.clone()),
                &client,
                &root,
                100,
                std::time::Duration::ZERO,
            )
            .await
        },
    ));
    assert!(futures::poll!(&mut waiting).is_pending());
    assert_eq!(
        sem.available_permits(),
        1,
        "inner adapter permit was released"
    );
    let mut next = Box::pin(lanes.acquire_serving_lane(None, LanePriority::Directed, None));
    let std::task::Poll::Ready(other_lane) = futures::poll!(&mut next) else {
        return Err("all-pinned wait retained the outer serving lane".into());
    };
    drop(held);
    assert!(
        futures::poll!(&mut waiting).is_pending(),
        "slot release cannot bypass lane authority"
    );
    drop(other_lane);
    let admitted = tokio::time::timeout(std::time::Duration::from_secs(2), waiting).await??;
    assert_eq!(admitted.slot(), Some(0));
    let mut following = Box::pin(lanes.acquire_serving_lane(None, LanePriority::Directed, None));
    assert!(
        futures::poll!(&mut following).is_pending(),
        "physical owner retains reacquired lane"
    );
    drop(admitted);
    drop(tokio::time::timeout(std::time::Duration::from_secs(2), following).await?);
    Ok(())
}

// Intel's all-pinned storm: nine contenders must not repeatedly wake each other
// while the sole physical slot remains pinned. Cancellation must not manufacture
// a slot release; a real release must still make the survivor eligible.
#[tokio::test]
async fn all_pinned_waiters_park_without_self_waking() -> Result<(), Box<dyn std::error::Error>> {
    let root = format!("test://contention-{}", Uuid::new_v4());
    let pool = Arc::new(KvSlotPool::new(&root, 1));
    let sem = Arc::new(Semaphore::new(10));
    let client = reqwest::Client::new();
    let owner = PageKey::mind(Uuid::new_v4()).ok_or("owner key")?;
    let held = admit_turn(
        &sem,
        crate::inference::slots::SlotClass::Turn,
        Some(owner),
        Some(pool.clone()),
        &client,
        &root,
        100,
        std::time::Duration::ZERO,
    )
    .await?;
    let counter = Arc::new(WakeCount::default());
    let waker = Waker::from(counter.clone());
    let mut context = Context::from_waker(&waker);
    let mut waiting = Vec::new();
    for _ in 0..9 {
        let key = PageKey::mind(Uuid::new_v4()).ok_or("waiter key")?;
        let mut turn = Box::pin(admit_turn(
            &sem,
            crate::inference::slots::SlotClass::Turn,
            Some(key),
            Some(pool.clone()),
            &client,
            &root,
            100,
            std::time::Duration::ZERO,
        ));
        assert!(turn.as_mut().poll(&mut context).is_pending());
        waiting.push(turn);
    }
    assert_eq!(sem.available_permits(), 9, "waiters retain no lane");
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    assert_eq!(
        counter.0.load(Ordering::SeqCst),
        0,
        "no slot changed: no wake is due"
    );
    let survivor = waiting.pop().ok_or("survivor")?;
    drop(waiting);
    assert_eq!(
        counter.0.load(Ordering::SeqCst),
        0,
        "cancelling unpinned waiters frees no slot"
    );
    drop(held);
    assert!(
        counter.0.load(Ordering::SeqCst) > 0,
        "real release wakes the survivor"
    );
    let admitted = tokio::time::timeout(std::time::Duration::from_secs(2), survivor).await??;
    assert_eq!(admitted.slot(), Some(0));
    drop(admitted);
    assert_eq!(sem.available_permits(), 10);
    Ok(())
}

// A queued lifecycle writer must not deadlock with residents awakened by the old
// slot's release. They must relinquish the old endpoint before reacquiring it.
#[tokio::test]
async fn pinned_waiters_do_not_retain_the_endpoint_during_transition(
) -> Result<(), Box<dyn std::error::Error>> {
    let root = format!("test://contention-transition-{}", Uuid::new_v4());
    let pool = Arc::new(KvSlotPool::new(&root, 1));
    let sem = Arc::new(Semaphore::new(10));
    let client = reqwest::Client::new();
    let owner = PageKey::mind(Uuid::new_v4()).ok_or("owner key")?;
    let held = admit_turn(
        &sem,
        crate::inference::slots::SlotClass::Turn,
        Some(owner),
        Some(pool.clone()),
        &client,
        &root,
        100,
        std::time::Duration::ZERO,
    )
    .await?;
    let mut waiting = Vec::new();
    for _ in 0..9 {
        let key = PageKey::mind(Uuid::new_v4()).ok_or("waiter key")?;
        let mut turn = Box::pin(admit_turn(
            &sem,
            crate::inference::slots::SlotClass::Turn,
            Some(key),
            Some(pool.clone()),
            &client,
            &root,
            100,
            std::time::Duration::from_secs(10),
        ));
        assert!(futures::poll!(&mut turn).is_pending());
        waiting.push(turn);
    }
    let endpoint = crate::inference::slots::directory().endpoint(&root);
    let mut transition = Box::pin(endpoint.transition());
    assert!(
        futures::poll!(&mut transition).is_pending(),
        "live owner fences transition"
    );
    drop(held);
    for turn in &mut waiting {
        assert!(futures::poll!(turn.as_mut()).is_pending());
    }
    let guard = tokio::time::timeout(std::time::Duration::from_secs(2), transition).await?;
    drop(waiting);
    drop(guard);
    assert_eq!(
        sem.available_permits(),
        10,
        "cancelled waiters release every lane"
    );
    Ok(())
}
