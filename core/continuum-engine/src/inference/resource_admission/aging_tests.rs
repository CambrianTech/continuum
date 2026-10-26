//! Regression: a continuously replenished work queue must not starve an ambient resident.
use super::{LaneAdmission, LanePriority};
use std::{future::Future, pin::Pin, task::Poll};

async fn pending<F: Future>(mut request: Pin<&mut F>) {
    assert!(std::future::poll_fn(|cx| Poll::Ready(request.as_mut().poll(cx).is_pending())).await);
}

async fn granted<F: Future>(mut request: Pin<&mut F>) -> F::Output {
    match std::future::poll_fn(|cx| Poll::Ready(request.as_mut().poll(cx))).await {
        Poll::Ready(permit) => permit,
        Poll::Pending => panic!("the released lane must serve the FIFO head"), // panic: regression assertion
    }
}

// Three workers keep one non-directed lane saturated. After the initial cohort,
// ambient queues behind existing work, ahead of subsequent work; reserve stays free.
#[tokio::test(start_paused = true)]
async fn ambient_ages_under_continuous_work_without_taking_the_directed_reserve() {
    let gate: &'static LaneAdmission = Box::leak(Box::new(LaneAdmission::new()));
    gate.set_served_lane_count(2);
    let first = gate.acquire_serving_lane(None, LanePriority::Work, None).await;
    let (_, second) = gate.serving_lane_request(None, LanePriority::Work, None);
    let (_, third) = gate.serving_lane_request(None, LanePriority::Work, None);
    tokio::pin!(second, third);
    pending(second.as_mut()).await;
    pending(third.as_mut()).await;
    let (_, ambient) = gate.serving_lane_request(Some(uuid::Uuid::new_v4()), LanePriority::Ambient, None);
    tokio::pin!(ambient);
    pending(ambient.as_mut()).await; // cohort = two waiters + one budget holder
    drop(first);
    let second = granted(second.as_mut()).await;
    pending(ambient.as_mut()).await;
    let (_, fourth) = gate.serving_lane_request(None, LanePriority::Work, None);
    tokio::pin!(fourth);
    pending(fourth.as_mut()).await;
    drop(second);
    let third = granted(third.as_mut()).await;
    pending(ambient.as_mut()).await;
    let (_, fifth) = gate.serving_lane_request(None, LanePriority::Work, None);
    tokio::pin!(fifth);
    pending(fifth.as_mut()).await;
    drop(third);
    let fourth = granted(fourth.as_mut()).await;
    pending(ambient.as_mut()).await; // three grants: joins FIFO behind fifth
    let (_, sixth) = gate.serving_lane_request(None, LanePriority::Work, None);
    tokio::pin!(sixth);
    pending(sixth.as_mut()).await;
    drop(fourth);
    let fifth = granted(fifth.as_mut()).await;
    drop(fifth);
    let ambient = granted(ambient.as_mut()).await;
    pending(sixth.as_mut()).await;
    let reserve = gate.acquire_serving_lane(None, LanePriority::Directed, None).await;
    drop(reserve);
    drop(ambient);
    drop(granted(sixth.as_mut()).await);
}

// A directed interruption drops the acquisition, not the resident's accrued age.
#[tokio::test(start_paused = true)]
async fn cancelled_ambient_retry_keeps_age_and_enters_fifo() {
    let gate: &'static LaneAdmission = Box::leak(Box::new(LaneAdmission::new()));
    gate.set_served_lane_count(2);
    let persona = uuid::Uuid::new_v4();
    let first = gate.acquire_serving_lane(None, LanePriority::Work, None).await;
    let (_, attempt) = gate.serving_lane_request(Some(persona), LanePriority::Ambient, None);
    let mut attempt = Box::pin(attempt);
    pending(attempt.as_mut()).await; // cohort of one
    drop(attempt);
    drop(first);
    let second = gate.acquire_serving_lane(None, LanePriority::Work, None).await;
    let (_, retry) = gate.serving_lane_request(Some(persona), LanePriority::Ambient, None);
    tokio::pin!(retry);
    pending(retry.as_mut()).await;
    assert_eq!(gate.work_waiting(), 1, "aged retry joins FIFO immediately");
    let (_, later) = gate.serving_lane_request(None, LanePriority::Work, None);
    tokio::pin!(later);
    pending(later.as_mut()).await;
    drop(second);
    let retry = granted(retry.as_mut()).await;
    pending(later.as_mut()).await;
    drop(retry);
    drop(granted(later.as_mut()).await);
}
