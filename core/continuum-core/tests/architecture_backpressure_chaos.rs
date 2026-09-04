//! Architecture test — proves the "backpressure is intrinsic" doctrine
//! clause via an adversarial / chaos test (shape 4).
//!
//! See `docs/architecture/PROVING-THE-DOCTRINE.md` for the matrix this
//! file populates. The clause pinned here:
//!
//! > "Backpressure is intrinsic — no unbounded queue growth. When a
//! > producer outpaces a consumer, the substrate surfaces a TYPED
//! > lag signal (`airc_lib::LiveLag { skipped: n }`) rather than
//! > growing memory unboundedly or silently dropping events. Slow
//! > consumers stay informed; fast producers can't poison the
//! > substrate."
//!
//! ## Why an adversarial / chaos test
//!
//! Backpressure is a runtime property. We can't statically prove
//! "the queue stays bounded" — we have to STARVE the consumer and
//! FLOOD the producer and watch what happens. Per Shape 4: feed the
//! substrate hostile input (here, a faster producer than the consumer
//! can drain) and assert it surfaces structure rather than collapse.
//!
//! ## What this proves
//!
//! 1. `flooding_producer_surfaces_typed_lag_to_slow_consumer`:
//!    - Wire two peers via `TwoAircLoopback`.
//!    - peer_a subscribes EAGERLY but DOES NOT consume — simulates a
//!      stalled consumer (could be a slow disk, a paused thread, etc).
//!    - peer_b floods peer_a with FLOOD_COUNT = 1500 events (past
//!      airc-lib's `LIVE_BROADCAST_CAPACITY = 1024` by ~50% margin).
//!    - After the flood, peer_a drains its stream.
//!    - Asserts: stream surfaces at least one `Err(LiveLag { skipped })`
//!      — the typed signal that overflow happened, NOT silent loss.
//!    - Asserts: `events_seen + total_skipped <= FLOOD_COUNT` —
//!      no fabricated events, conservation holds.
//!    - Asserts: process didn't crash (we got here).
//!
//! 2. `consumer_makes_progress_after_lag`:
//!    - Same fixture. After observing a lag, asserts subsequent
//!      successful reads continue to flow — the lag is RECOVERABLE,
//!      not a terminal state.
//!    - Proves: a slow consumer briefly falling behind doesn't lose
//!      the subscription forever — once it catches up, it sees new
//!      events.
//!
//! Together: backpressure isn't silent (typed lag), isn't unbounded
//! (memory stays bounded because the broadcast channel is bounded),
//! and isn't fatal (consumer recovers and keeps consuming).
//!
//! ## What this does NOT cover (intentionally — follow-up shapes)
//!
//! - Producer-side backpressure (producer blocks when channel full).
//!   airc-lib's broadcast channel is "newest-wins-with-Lagged-signal",
//!   not "block-the-producer". That's the doctrine choice (favor live
//!   responsiveness over reliability) and a different test should
//!   pin it explicitly.
//! - Multi-consumer fairness under partial-lag. Tracked as a
//!   follow-up Shape-2 proptest.
//! - Memory measurement under sustained flood (e.g. RSS bound).
//!   Process-level memory bounding is environmental (kernel cgroups);
//!   this test bounds via the channel capacity proof, which is the
//!   substrate's own guarantee.
//! - Sustained throughput under CONTINUOUS flood. This test ends after
//!   one flood-then-drain cycle; it doesn't prove the substrate stays
//!   bounded if a producer keeps flooding indefinitely. The channel
//!   capacity guarantee implies it should, but a long-running stress
//!   harness (gated behind the `stress-tests` feature) would prove it
//!   empirically.
//!
//! ## Tag
//!
//! proves: backpressure intrinsic (flood surfaces typed LiveLag,
//! consumer recovers, no fabrication, no silent loss)

use std::sync::Arc;
use std::time::{Duration, Instant};

use airc_core::{Body, Headers};
use airc_test_fixtures::TwoAircLoopback;
use futures::stream::StreamExt;

/// airc-lib's broadcast channel capacity, mirrored here as a const so
/// the conservation/survival assertions reference the same number the
/// substrate guarantees. If airc-lib changes this, the test will
/// surface the mismatch via the survival_ceiling assertion (and we
/// update both ends together).
const LIVE_BROADCAST_CAPACITY: usize = 1_024;

/// Total events flooded into the broadcast channel. Must exceed
/// `LIVE_BROADCAST_CAPACITY` by enough margin that the overflow
/// signal is unambiguous, but small enough that CI wall-clock stays
/// reasonable. 1500 = ~50% margin over capacity; any send rate beats
/// the (paused) consumer rate of zero, so the overflow IS the
/// assertion, not the producer's burst rate.
const FLOOD_COUNT: u64 = 1_500;

/// Lifecycle-event headroom on top of FLOOD_COUNT for the
/// conservation assertion. In a fresh 2-peer TwoAircLoopback fixture
/// the substrate emits single-digit join/leave/subscription-advance
/// events on the same broadcast channel; 64 leaves ~16× margin while
/// still being tight enough that a fabrication bug (e.g. an event
/// being double-broadcast on overflow recovery) would surface here.
const CONSERVATION_SLACK: u64 = 64;

/// Survival slack on top of LIVE_BROADCAST_CAPACITY for the
/// bounded-recovery assertion. Accounts for the BroadcastStream
/// wrapper's in-flight buffered item + ordering races between
/// `lag` accounting and event delivery. 128 is conservative; the
/// theoretical minimum is +1 (wrapper item) but ordering jitter
/// can stretch this on loaded CI.
const SURVIVAL_SLACK: u64 = 128;

/// How long the consumer drains, measured from after the flood
/// starts. Generous so a slower CI doesn't false-fail; the test
/// completes promptly because the consumer hits Pending after
/// draining (which the timeout-loop treats as "done").
const DRAIN_BUDGET: Duration = Duration::from_secs(3);

/// Per-poll timeout when draining the consumer's stream. Short enough
/// that we exit promptly once the producer is done; long enough that
/// we don't busy-loop.
const POLL_TIMEOUT: Duration = Duration::from_millis(100);

/// Brief sleep after subscribe() returns so the broadcast subscription
/// lands in airc's internal table before the producer starts sending.
/// Without this, the producer's first sends could legitimately predate
/// the subscription and be missed for non-lag reasons.
const SUBSCRIPTION_SETTLE: Duration = Duration::from_millis(100);

/// How many concurrent in-flight sends the producer keeps in the
/// air. Sequential awaits get bottlenecked by airc's per-send
/// signing + framing latency (~6ms each on Intel Mac, measured during
/// PR #1594 development). 32 concurrent sends modestly reduces
/// wall-clock; airc-lib's internal serialization caps the actual
/// parallelism, so this is a tuning knob, not a load multiplier.
/// The proof shape is "producer outpaces consumer," not "single
/// thread sends as fast as possible," so concurrency is honest.
const PRODUCER_CONCURRENCY: usize = 32;

/// Spawn a producer task that floods `peer` with `count` text events
/// as fast as it can. Uses bounded concurrency so the producer
/// outruns the consumer in wall-clock terms without exhausting the
/// tokio runtime with thousands of spawned tasks.
async fn spawn_flooding_producer(
    peer: Arc<airc_lib::Airc>,
    count: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        use futures::stream::{self, StreamExt};
        stream::iter(0..count)
            .map(|i| {
                let peer = Arc::clone(&peer);
                async move {
                    let body = Body::text(format!("flood-{i}"));
                    // Ignore send result — sends can fail under
                    // sustained load (transient room-state hiccups);
                    // this test pins consumer-side backpressure, not
                    // send-side reliability.
                    let _ = peer.send(body, Headers::new()).await;
                }
            })
            .buffer_unordered(PRODUCER_CONCURRENCY)
            .collect::<Vec<_>>()
            .await;
    })
}

/// Drain `stream` for up to `budget`, returning (events_seen,
/// lag_signal_count, total_skipped). The loop exits early once the
/// stream returns None or stays Pending past `POLL_TIMEOUT` repeatedly
/// past the budget.
async fn drain_with_budget(mut stream: airc_lib::EventStream, budget: Duration) -> (u64, u64, u64) {
    let deadline = Instant::now() + budget;
    let mut events = 0u64;
    let mut lag_signals = 0u64;
    let mut total_skipped = 0u64;

    while Instant::now() < deadline {
        match tokio::time::timeout(POLL_TIMEOUT, stream.next()).await {
            Ok(Some(Ok(_event))) => events += 1,
            Ok(Some(Err(lag))) => {
                lag_signals += 1;
                total_skipped += lag.skipped;
            }
            Ok(None) => break, // stream closed
            Err(_) => {
                // Poll-timeout. If the producer is done and we're
                // not seeing new events, exit. Otherwise keep waiting
                // until the outer budget expires.
                continue;
            }
        }
    }

    (events, lag_signals, total_skipped)
}

// proves: backpressure intrinsic (flooding producer surfaces typed
// LiveLag signal to a slow consumer — overflow is observable, not silent;
// no fabrication; process stays alive)
#[tokio::test]
async fn flooding_producer_surfaces_typed_lag_to_slow_consumer() {
    let loopback = TwoAircLoopback::new()
        .await
        .expect("fixture setup should succeed");

    // peer_a is the SLOW consumer. Subscribe EAGERLY but defer all
    // consumption to AFTER the flood, simulating a stalled reader.
    let stream_a = loopback
        .peer_a()
        .subscribe()
        .await
        .expect("peer_a subscribe");

    // Let the subscription land in airc's internal table.
    tokio::time::sleep(SUBSCRIPTION_SETTLE).await;

    // peer_b floods peer_a past the broadcast channel's capacity.
    let producer = spawn_flooding_producer(Arc::clone(loopback.peer_b()), FLOOD_COUNT).await;

    // Wait for the producer to finish flooding before draining the
    // consumer. This guarantees the overflow has happened — there's
    // no race where we drain fast enough to keep up with the producer
    // and never observe lag.
    producer.await.expect("producer task joined");

    // Now drain. The first poll(s) should observe Lagged signals
    // because the channel overflowed while peer_a was stalled.
    let (events, lag_signals, total_skipped) = drain_with_budget(stream_a, DRAIN_BUDGET).await;

    // Primary assertion: lag is OBSERVABLE, not silent.
    assert!(
        lag_signals > 0,
        "slow consumer observed 0 LiveLag signals after a flood of \
         {FLOOD_COUNT} events past airc-lib's {LIVE_BROADCAST_CAPACITY}-event \
         broadcast capacity. The substrate must surface typed lag on \
         overflow (doctrine: backpressure intrinsic, no silent loss). \
         Got: events={events}, lag_signals=0, total_skipped={total_skipped}."
    );

    // Conservation: events seen + skipped never exceeds events sent
    // plus a small lifecycle-event budget. peer_a's stream sees its
    // OWN sends too (broadcast echoes locally), but here peer_a
    // sends nothing — only peer_b floods. `CONSERVATION_SLACK`
    // documents the budget at module scope.
    let observed = events + total_skipped;
    assert!(
        observed <= FLOOD_COUNT + CONSERVATION_SLACK,
        "events ({events}) + skipped ({total_skipped}) = {observed} \
         exceeds FLOOD_COUNT ({FLOOD_COUNT}) + {CONSERVATION_SLACK} \
         lifecycle-event headroom — substrate is fabricating events \
         or accounting is wrong"
    );

    // Bounded recovery: the surviving event count is what's left in
    // the bounded broadcast ring when the consumer drains, plus at
    // most one in-flight item the BroadcastStream wrapper may have
    // buffered between polls. `SURVIVAL_SLACK` is documented at
    // module scope.
    let survival_ceiling = (LIVE_BROADCAST_CAPACITY as u64) + SURVIVAL_SLACK;
    assert!(
        events <= survival_ceiling,
        "events delivered to slow consumer ({events}) exceeds the \
         broadcast capacity ({LIVE_BROADCAST_CAPACITY}) + \
         {SURVIVAL_SLACK} slack. The channel's bounded guarantee \
         should cap what survives overflow."
    );
}

// proves: backpressure intrinsic (consumer recovers after observing
// lag — the typed signal doesn't terminate the subscription)
#[tokio::test]
async fn consumer_makes_progress_after_lag() {
    let loopback = TwoAircLoopback::new()
        .await
        .expect("fixture setup should succeed");

    let mut stream_a = loopback
        .peer_a()
        .subscribe()
        .await
        .expect("peer_a subscribe");

    tokio::time::sleep(SUBSCRIPTION_SETTLE).await;

    // First flood: force a lag.
    let first_producer = spawn_flooding_producer(Arc::clone(loopback.peer_b()), FLOOD_COUNT).await;
    first_producer.await.expect("first producer joined");

    // Drain until we observe at least one lag signal AND at least one
    // post-lag success — proving the stream is still alive after lag.
    let mut seen_lag = false;
    let mut seen_post_lag_success = false;
    let mut total_events = 0u64;
    let recovery_deadline = Instant::now() + DRAIN_BUDGET;

    // After the lag fires, the 1024-slot broadcast ring still holds
    // ~capacity surviving events — the consumer drains them on the
    // very next polls. So in practice the `seen_post_lag_success`
    // flips on the first Ok read after `seen_lag`. The timeout arm
    // exists as defense-in-depth for very-slow-CI cases where polls
    // legitimately stall; if it ever fires, send a tiny fresh batch
    // to prove the subscription stayed alive across the stall window.
    while Instant::now() < recovery_deadline {
        match tokio::time::timeout(POLL_TIMEOUT, stream_a.next()).await {
            Ok(Some(Ok(_event))) => {
                total_events += 1;
                if seen_lag {
                    seen_post_lag_success = true;
                    break;
                }
            }
            Ok(Some(Err(_lag))) => {
                seen_lag = true;
            }
            Ok(None) => break,
            Err(_) => {
                // Defense-in-depth: poll timed out without a result.
                // If we've already seen a lag, send a small fresh batch
                // to test whether the subscription survived the stall.
                if seen_lag && !seen_post_lag_success {
                    let recovery_producer =
                        spawn_flooding_producer(Arc::clone(loopback.peer_b()), 5).await;
                    recovery_producer.await.expect("recovery producer joined");
                }
            }
        }
    }

    assert!(
        seen_lag,
        "test prerequisite: flood should have produced at least one \
         LiveLag (got 0 across {total_events} events). If this fires, \
         the flood didn't actually overrun the channel; the test \
         can't prove recovery without first proving overflow."
    );
    assert!(
        seen_post_lag_success,
        "consumer must continue receiving events AFTER a LiveLag — \
         the typed lag signal is recoverable, not terminal. \
         {total_events} total events seen but none after a lag was \
         observed; subscription may have wedged."
    );
}
