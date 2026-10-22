//! The real serving scheduler retains FIFO ownership across a diagnostic estimate.
use super::observe_wait;
use crate::inference::resource_admission::{LaneAdmission, LanePriority};
use std::time::Duration;

async fn queued(gate: &LaneAdmission, count: usize) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while gate.work_waiting() != count {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("work queue reached expected depth");
}

#[tokio::test]
async fn estimate_expiry_retains_fifo_position_before_later_arrival() {
    let gate: &'static LaneAdmission = Box::leak(Box::new(LaneAdmission::new()));
    gate.set_served_lane_count(2); // one ordinary work lane, one directed reserve
    let held = gate
        .acquire_serving_lane(None, LanePriority::Work, None)
        .await;
    let (expire, expiry) = tokio::sync::oneshot::channel();
    let (observed, observation) = tokio::sync::oneshot::channel();
    let first = tokio::spawn(observe_wait(
        gate.acquire_serving_lane(None, LanePriority::Work, None),
        async {
            expiry.await.expect("test supplies estimate expiry");
        },
        move || {
            let _ = observed.send(());
        },
    ));
    queued(gate, 1).await;
    expire.send(()).expect("estimate listener alive");
    observation
        .await
        .expect("estimate observed without cancellation");
    assert_eq!(
        gate.work_waiting(),
        1,
        "first resident keeps her live queue entry"
    );
    let second = tokio::spawn(gate.acquire_serving_lane(None, LanePriority::Work, None));
    queued(gate, 2).await;
    drop(held);
    let first_permit = tokio::time::timeout(Duration::from_secs(2), first)
        .await
        .expect("first queued resident gets released lane")
        .expect("first task");
    assert!(
        !second.is_finished(),
        "later arrival cannot overtake the observed wait"
    );
    drop(first_permit);
    let second_permit = tokio::time::timeout(Duration::from_secs(2), second)
        .await
        .expect("next resident proceeds after release")
        .expect("second task");
    drop(second_permit);
    assert_eq!(gate.work_waiting(), 0);
}

#[tokio::test]
async fn real_cancellation_releases_waiter_before_and_after_estimate() {
    for observe_first in [false, true] {
        let gate: &'static LaneAdmission = Box::leak(Box::new(LaneAdmission::new()));
        gate.set_served_lane_count(2);
        let held = gate
            .acquire_serving_lane(None, LanePriority::Work, None)
            .await;
        let (expire, expiry) = tokio::sync::oneshot::channel();
        let (observed, observation) = tokio::sync::oneshot::channel();
        let waiting = tokio::spawn(observe_wait(
            gate.acquire_serving_lane(None, LanePriority::Work, None),
            async {
                let _ = expiry.await;
            },
            move || {
                let _ = observed.send(());
            },
        ));
        queued(gate, 1).await;
        if observe_first {
            expire.send(()).expect("estimate listener alive");
            observation.await.expect("estimate observed");
        }
        waiting.abort();
        assert!(waiting.await.expect_err("cancelled task").is_cancelled());
        assert_eq!(
            gate.work_waiting(),
            0,
            "cancel/despawn drops original queue entry"
        );
        drop(held);
        let next = tokio::time::timeout(
            Duration::from_secs(2),
            gate.acquire_serving_lane(None, LanePriority::Work, None),
        )
        .await
        .expect("no leaked budget");
        drop(next);
    }
}

#[tokio::test]
async fn directed_arrival_interrupts_an_observed_wait_and_cleans_its_entry() {
    let gate: &'static LaneAdmission = Box::leak(Box::new(LaneAdmission::new()));
    gate.set_served_lane_count(2);
    let held = gate
        .acquire_serving_lane(None, LanePriority::Work, None)
        .await;
    let persona = uuid::Uuid::new_v4();
    let (observed, observation) = tokio::sync::oneshot::channel();
    let waiter = tokio::spawn(async move {
        tokio::select! {
            _ = observe_wait(gate.acquire_serving_lane(Some(persona), LanePriority::Work, None),
                std::future::ready(()), move || { let _ = observed.send(()); }) => false,
            _ = crate::cognition::directed_pending::wait(persona) => true,
        }
    });
    observation
        .await
        .expect("original acquisition queued before observation");
    assert_eq!(gate.work_waiting(), 1);
    crate::cognition::directed_pending::signal(persona);
    assert!(tokio::time::timeout(Duration::from_secs(2), waiter)
        .await
        .expect("directed wake remains responsive")
        .expect("waiter task"));
    crate::cognition::directed_pending::clear(persona);
    assert_eq!(gate.work_waiting(), 0);
    drop(held);
}
