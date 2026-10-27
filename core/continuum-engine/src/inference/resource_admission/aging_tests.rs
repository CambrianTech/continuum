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
    gate.resident_seated(persona);
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

// Leaving a seat forgives debt, even when a stale observer survives departure.
#[tokio::test]
async fn ambient_debt_follows_the_seat_lifetime() {
    let gate: &'static LaneAdmission = Box::leak(Box::new(LaneAdmission::new()));
    gate.set_served_lane_count(2);
    let persona = uuid::Uuid::new_v4();
    gate.resident_seated(persona);
    let (old, guard) = gate.observe(Some(persona), LanePriority::Ambient);
    let origin = old.ambient_origin(1);
    drop(gate.acquire_serving_lane(None, LanePriority::Work, None).await);
    assert!(old.ambient_aged(origin));
    let (retry, retry_guard) = gate.observe(Some(persona), LanePriority::Ambient);
    assert_eq!(retry.ambient_origin(5), origin, "retry retains debt while seated");
    gate.resident_departed(persona);
    let departed = old.ambient_origin(2);
    assert_ne!(departed.0, origin.0, "departure forgives existing debt");
    drop(gate.acquire_serving_lane(None, LanePriority::Work, None).await);
    gate.resident_seated(persona);
    assert_ne!(retry.ambient_origin(3).0, departed.0, "stale observer cannot retain debt while unseated");
    drop(retry_guard);
    drop(guard);
    gate.resident_departed(persona);
}

// Aged retries keep their place, yet an absent resident never holds a lane idle.
#[tokio::test(start_paused = true)]
async fn cancelled_aged_retry_keeps_rank_without_hoarding_a_permit() {
    let gate: &'static LaneAdmission = Box::leak(Box::new(LaneAdmission::new()));
    gate.set_served_lane_count(2);
    let persona = uuid::Uuid::new_v4();
    gate.resident_seated(persona);
    let first = gate.acquire_serving_lane(None, LanePriority::Work, None).await;
    let (_, attempt) = gate.serving_lane_request(Some(persona), LanePriority::Ambient, None);
    let mut attempt = Box::pin(attempt);
    pending(attempt.as_mut()).await;
    drop(first);
    let second = gate.acquire_serving_lane(None, LanePriority::Work, None).await;
    pending(attempt.as_mut()).await; // aged and ticketed
    let (_, later) = gate.serving_lane_request(None, LanePriority::Work, None);
    let mut later = Box::pin(later);
    pending(later.as_mut()).await;
    drop(attempt);
    drop(second);
    let later = granted(later.as_mut()).await; // absent resident cannot hoard capacity
    let (_, newest) = gate.serving_lane_request(None, LanePriority::Work, None);
    let mut newest = Box::pin(newest);
    pending(newest.as_mut()).await;
    for _ in 0..3 {
        let (_, retry) = gate.serving_lane_request(Some(persona), LanePriority::Ambient, None);
        let mut retry = Box::pin(retry);
        pending(retry.as_mut()).await;
        drop(retry);
    }
    let (_, retry) = gate.serving_lane_request(Some(persona), LanePriority::Ambient, None);
    let mut retry = Box::pin(retry);
    pending(retry.as_mut()).await;
    drop(later);
    pending(newest.as_mut()).await; // poll newer work first: it must not jump the old rank
    let retry = granted(retry.as_mut()).await;
    drop(retry);
    drop(granted(newest.as_mut()).await);
}

/// The M5's measured shape replayed through the public gate on a paused clock: two lanes,
/// three Work minds that re-request the instant they release, one Ambient resident (Aris,
/// card 84a6fd1c: zero acts 10-25 19:39Z .. 10-26 23:54Z). Six virtual hours run in seconds.
mod replay {
    use super::{LaneAdmission, LanePriority};
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::Instant;

    /// `held_ms` of every `admission.lane.released priority=Work` in the M5's
    /// `~/.continuum/logs/continuum-core-server.log*`, 2026-10-26 21:02:59Z .. 23:44:46Z,
    /// in log order (n=66, p50 98,686 ms, p90 185,007, max 345,415). The gate's own
    /// `delib.gate.lane_wait` at 22:22:54Z read lane_hold_p50_ms=65067 over 5 samples.
    const WORK_HOLD_MS: [u64; 66] = [
        83629, 76605, 102492, 98686, 14355, 37886, 102314, 23936, 56667, 140923, 161423, 24378,
        12507, 45467, 56254, 32039, 104930, 211045, 24317, 88476, 96343, 85951, 86920, 101135,
        145331, 32835, 130430, 162856, 103776, 332079, 130501, 138596, 147835, 185007, 132798,
        41007, 142432, 202567, 149622, 253271, 65067, 54570, 132027, 79429, 151934, 134302,
        92765, 51495, 111131, 108752, 136563, 179268, 161012, 120609, 55527, 57212, 345415,
        299441, 21448, 43215, 35750, 27560, 36897, 36957, 35457, 321164,
    ];
    /// `held_ms` of every `admission.lane.released priority=Ambient`, same logs and window (n=25).
    const AMBIENT_HOLD_MS: [u64; 25] = [
        47643, 47248, 208877, 271345, 122183, 80467, 48496, 75501, 213492, 67862, 48434, 58402,
        224792, 107316, 277, 45535, 37535, 45494, 39061, 52598, 72126, 37650, 2849, 52003,
        507242,
    ];
    /// `delib.gate.wait_estimate_exceeded persona=Aris waited_secs=260 bound_secs=260`.
    const ARIS_BOUND: Duration = Duration::from_secs(260);
    const HOURS: usize = 6;
    const HORIZON: Duration = Duration::from_secs(HOURS as u64 * 3600);
    // derived-or-floor: the three Work residents in the recorded M5 incident, a replay fixture rather than a runtime admission limit.
    const WORK_MINDS: usize = 3;
    /// #5499's cohort = Work waiting at her attempt + the one-lane non-directed budget. At most
    /// every Work mind is waiting (right after her own release); after aging she queues behind
    /// the waiters, all but the one now holding.
    const COHORT_GRANTS: u64 = WORK_MINDS as u64 + 1;
    const FIFO_AHEAD: u64 = WORK_MINDS as u64 - 1;

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub(super) enum Ambient {
        Absent,
        /// One request per turn, kept past the estimate (today's `delib.gate` contract).
        Retained,
        /// Dropped and re-requested every 260 s: the incident's shape, and every
        /// `yielded_to_directed` cancellation.
        CancelEvery(Duration),
        SlowCancel,
    }

    #[derive(Default, Debug)]
    pub(super) struct Tally {
        pub work_grants: [u64; WORK_MINDS],
        pub work_lane_ms: u64,
        pub ambient_grants: u64,
        /// Her grants in each virtual hour: recurring service, not one grant then starvation.
        pub ambient_by_hour: [u64; HOURS],
        pub ambient_lane_ms: u64,
        pub ambient_cancellations: u64,
        /// Per grant: (virtual seconds she waited, Work grants that passed her meanwhile).
        pub ambient_waits: Vec<(u64, u64)>,
        pub open_wait: (u64, u64),
        waiting_since: Option<(Instant, u64)>,
        pub directed_probes: u64,
        pub directed_delayed: u64,
        pub reserve_taken_at_ambient_grant: u64,
    }

    impl Tally {
        fn work_seen(&self) -> u64 {
            self.work_grants.iter().sum()
        }
        pub fn max_wait(&self) -> (u64, u64) {
            self.ambient_waits.iter().chain([&self.open_wait]).fold((0, 0), |(s, g), w| (s.max(w.0), g.max(w.1)))
        }
        pub fn per_hour(n: u64) -> f64 {
            n as f64 * 3600.0 / HORIZON.as_secs() as f64
        }
    }

    fn clipped_ms(start: Instant, hold: Duration, end: Instant) -> u64 {
        (start + hold).min(end).saturating_duration_since(start).as_millis() as u64
    }

    pub(super) async fn replay(ambient: Ambient) -> Tally {
        let gate: &'static LaneAdmission = Box::leak(Box::new(LaneAdmission::new()));
        gate.set_served_lane_count(2);
        let tally = Arc::new(parking_lot::Mutex::new(Tally::default()));
        let start = Instant::now();
        let end = start + HORIZON;
        let mut tasks = Vec::new();
        for mind in 0..WORK_MINDS {
            let tally = tally.clone();
            tasks.push(tokio::spawn(async move {
                let persona = uuid::Uuid::new_v4();
                // Each mind walks the measured trace from its own third of it.
                let mut next = mind * WORK_HOLD_MS.len() / WORK_MINDS;
                loop {
                    let permit = gate.acquire_serving_lane(Some(persona), LanePriority::Work, None).await;
                    let granted = Instant::now();
                    let hold = Duration::from_millis(WORK_HOLD_MS[next % WORK_HOLD_MS.len()]);
                    let hold = if ambient == Ambient::SlowCancel { hold.max(Duration::from_secs(131)) } else { hold };
                    next += 1;
                    if granted < end {
                        let mut t = tally.lock();
                        t.work_grants[mind] += 1;
                        t.work_lane_ms += clipped_ms(granted, hold, end);
                    }
                    tokio::time::sleep(hold).await;
                    drop(permit);
                }
            }));
        }
        if ambient != Ambient::Absent {
            let tally = tally.clone();
            tasks.push(tokio::spawn(async move {
                let aris = uuid::Uuid::new_v4();
                gate.resident_seated(aris);
                let mut next = 0;
                {
                    let mut t = tally.lock();
                    t.waiting_since = Some((Instant::now(), t.work_seen()));
                }
                loop {
                    let (_, request) = gate.serving_lane_request(Some(aris), LanePriority::Ambient, None);
                    let deadline = match ambient {
                        Ambient::CancelEvery(every) => every,
                        Ambient::SlowCancel => ARIS_BOUND,
                        _ => HORIZON * 2,
                    };
                    let permit = tokio::select! {
                        permit = request => permit,
                        _ = tokio::time::sleep(deadline) => {
                            tally.lock().ambient_cancellations += 1;
                            continue;
                        }
                    };
                    let granted = Instant::now();
                    let hold = Duration::from_millis(AMBIENT_HOLD_MS[next % AMBIENT_HOLD_MS.len()]);
                    next += 1;
                    {
                        let mut t = tally.lock();
                        if let (true, Some(since)) = (granted < end, t.waiting_since.take()) {
                            let waited = (granted.duration_since(since.0).as_secs(), t.work_seen() - since.1);
                            t.ambient_waits.push(waited);
                            t.ambient_grants += 1;
                            t.ambient_by_hour[(granted.duration_since(start).as_secs() / 3600) as usize] += 1;
                            t.ambient_lane_ms += clipped_ms(granted, hold, end);
                            // Her physical lane is her non-directed budget; the reserve stays free.
                            if gate.serving_lanes().available_permits() < 1 {
                                t.reserve_taken_at_ambient_grant += 1;
                            }
                        }
                    }
                    tokio::time::sleep(hold).await;
                    drop(permit);
                    let mut t = tally.lock();
                    t.waiting_since = Some((Instant::now(), t.work_seen()));
                }
            }));
        }
        {
            // A directed line every ten minutes must find the reserved lane free at once.
            let tally = tally.clone();
            tasks.push(tokio::spawn(async move {
                let mut tick = tokio::time::interval(Duration::from_secs(600));
                loop {
                    tick.tick().await;
                    let asked = Instant::now();
                    let permit = gate.acquire_serving_lane(None, LanePriority::Directed, None).await;
                    let mut t = tally.lock();
                    t.directed_probes += 1;
                    if asked.elapsed() > Duration::ZERO {
                        t.directed_delayed += 1;
                    }
                    drop(permit);
                }
            }));
        }
        tokio::time::sleep_until(end).await;
        for task in &tasks {
            task.abort();
        }
        let mut out = std::mem::take(&mut *tally.lock());
        // Her wait still open at the horizon counts toward her max: a starved run reads six hours.
        if let Some((since, seen)) = out.waiting_since {
            out.open_wait = (end.duration_since(since).as_secs(), out.work_seen() - seen);
        }
        out
    }

    fn report(name: &str, t: &Tally) {
        let (max_s, max_g) = t.max_wait();
        eprintln!(
            "{name}: work/h per mind {:?} (total {:.2}/h), ambient {:.2}/h ({} grants by hour {:?}, {} cancellations), \
             ambient max wait {max_s} s / {max_g} grants, first grant {:?}, lane busy work {:.1}% ambient {:.1}%, \
             directed probes {} delayed {}",
            t.work_grants.map(Tally::per_hour),
            Tally::per_hour(t.work_grants.iter().sum()),
            Tally::per_hour(t.ambient_grants),
            t.ambient_grants,
            t.ambient_by_hour,
            t.ambient_cancellations,
            t.ambient_waits.first(),
            t.work_lane_ms as f64 / HORIZON.as_millis() as f64 * 100.0,
            t.ambient_lane_ms as f64 / HORIZON.as_millis() as f64 * 100.0,
            t.directed_probes,
            t.directed_delayed,
        );
    }

    // what this catches (card 84a6fd1c, #5499): an Ambient resident starved by Work minds
    // that re-request the instant they release (Aris, zero acts for 28 h on the M5). On the
    // measured hold trace she must be served within the cohort bound, keep being served
    // (not once then starved), cost Work no more than her own lane time, and never take
    // the directed reserve, whether her request is retained or cancelled every 260 s.
    #[tokio::test(start_paused = true)]
    async fn measured_m5_shape_serves_the_ambient_resident_and_keeps_the_reserve() {
        let baseline = replay(Ambient::Absent).await;
        report("work only", &baseline);
        let total_ms = HORIZON.as_millis() as u64;
        let max_hold_ms = *WORK_HOLD_MS.iter().chain(AMBIENT_HOLD_MS.iter()).max().unwrap_or(&0); // unwrap_or: const arrays are non-empty
        // Every grant she waits behind, plus the hold in progress, at the longest measured hold.
        let wait_bound_s = (COHORT_GRANTS + FIFO_AHEAD + 1) * max_hold_ms / 1000;
        for shape in [Ambient::Retained, Ambient::CancelEvery(ARIS_BOUND), Ambient::SlowCancel] {
            let t = replay(shape).await;
            report(&format!("{shape:?}"), &t);
            assert!(t.ambient_by_hour.iter().all(|&n| n > 0), "{shape:?}: an hour without her: {:?}", t.ambient_by_hour);
            let (max_s, max_g) = t.max_wait();
            {
                // The same cohort bound holds across cancellation: a retry
                // keeps its ticket without reserving idle capacity while absent.
                assert!(max_g <= COHORT_GRANTS + FIFO_AHEAD, "{shape:?}: an ambient wait spanned {max_g} work grants");
                assert!(max_s <= wait_bound_s, "{shape:?}: an ambient wait of {max_s} s");
            }
            // The lane never idles: what Work lost is what she held, give or take one hold at the edge.
            let lost = baseline.work_lane_ms.saturating_sub(t.work_lane_ms);
            assert!(lost <= t.ambient_lane_ms + max_hold_ms, "{shape:?}: work lost {lost} ms for her {} ms", t.ambient_lane_ms);
            assert!(t.work_lane_ms + t.ambient_lane_ms + max_hold_ms >= total_ms, "{shape:?}: the non-directed lane idled");
            assert_eq!(t.reserve_taken_at_ambient_grant, 0, "{shape:?}: ambient took the directed reserve");
            assert_eq!(t.directed_delayed, 0, "{shape:?}: a directed line waited");
            assert!(t.directed_probes >= 36);
        }
    }
}
