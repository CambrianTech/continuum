//! The real serving scheduler retains FIFO ownership across a diagnostic estimate.
use super::observe_wait;
use crate::inference::resource_admission::{LaneAdmission, LanePriority};
use std::time::Duration;

async fn queued(gate: &LaneAdmission, count: usize) -> Result<(), tokio::time::error::Elapsed> {
    tokio::time::timeout(Duration::from_secs(2), async {
        while gate.work_waiting() != count {
            tokio::task::yield_now().await;
        }
    })
    .await
}

#[tokio::test]
async fn estimate_expiry_retains_fifo_position_before_later_arrival(
) -> Result<(), Box<dyn std::error::Error>> {
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
            assert!(expiry.await.is_ok(), "test supplies estimate expiry");
        },
        move || {
            let _ = observed.send(());
        },
    ));
    queued(gate, 1).await?;
    assert!(expire.send(()).is_ok(), "estimate listener alive");
    observation.await?;
    assert_eq!(
        gate.work_waiting(),
        1,
        "first resident keeps her live queue entry"
    );
    let second = tokio::spawn(gate.acquire_serving_lane(None, LanePriority::Work, None));
    queued(gate, 2).await?;
    drop(held);
    let first_permit = tokio::time::timeout(Duration::from_secs(2), first).await??;
    assert!(
        !second.is_finished(),
        "later arrival cannot overtake the observed wait"
    );
    drop(first_permit);
    let second_permit = tokio::time::timeout(Duration::from_secs(2), second).await??;
    drop(second_permit);
    assert_eq!(gate.work_waiting(), 0);
    Ok(())
}

#[tokio::test]
async fn real_cancellation_releases_waiter_before_and_after_estimate(
) -> Result<(), Box<dyn std::error::Error>> {
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
        queued(gate, 1).await?;
        if observe_first {
            assert!(expire.send(()).is_ok(), "estimate listener alive");
            observation.await?;
        }
        waiting.abort();
        assert!(matches!(waiting.await, Err(error) if error.is_cancelled()));
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
        .await?;
        drop(next);
    }
    Ok(())
}

#[tokio::test]
async fn directed_arrival_interrupts_an_observed_wait_and_cleans_its_entry(
) -> Result<(), Box<dyn std::error::Error>> {
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
    observation.await?;
    assert_eq!(gate.work_waiting(), 1);
    crate::cognition::directed_pending::signal(persona);
    assert!(tokio::time::timeout(Duration::from_secs(2), waiter).await??);
    crate::cognition::directed_pending::clear(persona);
    assert_eq!(gate.work_waiting(), 0);
    drop(held);
    Ok(())
}
