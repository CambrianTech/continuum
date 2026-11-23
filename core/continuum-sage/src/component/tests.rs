//! The mechanism every component inherits, proven on components that never hold each other.

use std::time::Duration;

use super::*;
use crate::dataflow::{Event, OwnerId, Publisher, RunId, Snapshot, StageIncarnation, TopicName};
use uuid::Uuid;

const NUMBERS: TopicName = TopicName("test/numbers");
const TOTAL: TopicName = TopicName("test/total");

/// Hears every number on its owner's bus, says the running total.
struct Adder {
    total: u64,
    says: Publisher<u64>,
}

struct Number(u64);

impl From<Event<u64>> for Number {
    fn from(event: Event<u64>) -> Self {
        Number(*event.value)
    }
}

impl Component for Adder {
    type Input = Number;
    const NAME: &'static str = "adder";
    const QOS: Qos = Qos::Work;
    fn wire(&self, wiring: &mut Wiring<'_, Self>) {
        wiring.hear::<u64>(NUMBERS, std::num::NonZeroUsize::MIN.saturating_add(7));
    }
    async fn handle(&mut self, Number(n): Number, ctx: &mut Ctx<Self>) {
        self.total += n;
        ctx.publish(&mut self.says, self.total);
    }
}

// what this catches: a component wired by hand, or holding another to observe it. The adder names
// only the topic it hears and claims the topic it says; whoever reads the total names that topic.
// Its publication names the event it came from: provenance stamped by the engine, not the component.
#[tokio::test]
async fn a_part_hears_and_says_by_topic_with_provenance_from_the_engine() {
    let bus = Bus::new();
    let claimed = bus.publisher::<u64>(TOTAL, Key::ONE);
    assert_eq!(bus.publisher::<u64>(TOTAL, Key::ONE).err(), Some(Taken(TOTAL)), "a topic has one writer");
    let says = match claimed {
        Ok(says) => says,
        Err(taken) => return assert_eq!(Some(taken), None, "the total is unclaimed"),
    };
    let _adder = start(Adder { total: 0, says }, &bus, OwnerId(Uuid::from_u128(1)), 1);
    let mut total = bus.reader::<u64>(TOTAL, Key::ONE);
    let mut numbers = match bus.emitter::<u64>(NUMBERS) {
        Ok(numbers) => numbers,
        Err(taken) => return assert_eq!(Some(taken), None, "the numbers topic is unclaimed"),
    };
    numbers.emit(2);
    let last_sent = numbers.emit(3);
    let mut last: Option<Snapshot<u64>> = None;
    while last.as_ref().map(|s| *s.value) != Some(5) {
        last = total.changed().await.ok();
    }
    let provenance = last.map(|s| s.provenance);
    assert_eq!(provenance.as_ref().map(|p| p.by.incarnation.stage), Some(StageId::named("adder")));
    assert_eq!(provenance.map(|p| p.from), Some(vec![last_sent]));
}

/// Arms a timer when told, and reports what fell due.
struct Alarm {
    rang: tokio::sync::mpsc::UnboundedSender<&'static str>,
}

enum AlarmIn {
    Set { label: &'static str, after: Duration },
    Ring(&'static str),
}

impl Component for Alarm {
    type Input = AlarmIn;
    const NAME: &'static str = "alarm";
    const QOS: Qos = Qos::Work;
    fn wire(&self, _: &mut Wiring<'_, Self>) {}
    async fn handle(&mut self, input: AlarmIn, ctx: &mut Ctx<Self>) {
        match input {
            AlarmIn::Set { label, after } => ctx.after(Key::ONE, after, AlarmIn::Ring(label)),
            AlarmIn::Ring(label) => assert!(self.rang.send(label).is_ok(), "the test reads every ring"),
        }
    }
}

// what this catches: a component keeping a handle to itself for its deadline, or two deadlines both
// firing. A timer under the same key replaces the earlier one; only the newest rings, and nothing
// rings after the component stops.
#[tokio::test]
async fn a_newer_timer_under_the_same_key_replaces_the_older() {
    let (rang, mut rings) = tokio::sync::mpsc::unbounded_channel();
    let alarm = start(Alarm { rang }, &Bus::new(), OwnerId(Uuid::from_u128(2)), 1);
    assert!(alarm.tell(AlarmIn::Set { label: "first", after: Duration::from_millis(5) }).await);
    assert!(alarm.tell(AlarmIn::Set { label: "second", after: Duration::from_millis(10) }).await);
    assert_eq!(rings.recv().await, Some("second"));
    drop(alarm);
    assert_eq!(rings.recv().await, None, "the first never rang");
}

/// Holds a cache it builds from its inputs, and gives it back when asked to rest.
struct Cache {
    held: Vec<u64>,
    reported: tokio::sync::mpsc::UnboundedSender<usize>,
}

enum CacheIn {
    Add(u64),
    Report,
}

impl Component for Cache {
    type Input = CacheIn;
    const NAME: &'static str = "cache";
    const QOS: Qos = Qos::Work;
    fn wire(&self, _: &mut Wiring<'_, Self>) {}
    async fn handle(&mut self, input: CacheIn, _: &mut Ctx<Self>) {
        match input {
            CacheIn::Add(n) => self.held.push(n),
            CacheIn::Report => assert!(self.reported.send(self.held.len()).is_ok(), "the test reads every report"),
        }
    }
    async fn rest(&mut self) {
        self.held = Vec::new();
    }
}

// what this catches: rest as something only minds have, or a resting component that cannot be seen
// or woken. Every component inherits it: its presence is published by the engine (Ready, Acting,
// Resting) for anything to observe; resting gives back what it holds; its next input wakes it.
#[tokio::test]
async fn every_component_rests_visibly_and_its_next_input_wakes_it() {
    let bus = Bus::new();
    let mut presence = bus.reader::<Presence>(PRESENCE, presence_key("cache"));
    let (reported, mut reports) = tokio::sync::mpsc::unbounded_channel();
    let cache = start(Cache { held: Vec::new(), reported }, &bus, OwnerId(Uuid::from_u128(3)), 1);
    assert_eq!(presence.latest().map(|p| *p.value), Some(Presence::Ready), "published when it starts");
    assert!(cache.tell(CacheIn::Add(1)).await && cache.tell(CacheIn::Add(2)).await);
    assert!(cache.tell(CacheIn::Report).await);
    assert_eq!(reports.recv().await, Some(2));
    assert!(cache.rest().await);
    while presence.changed().await.map(|p| *p.value).ok() != Some(Presence::Resting) {}
    assert!(cache.tell(CacheIn::Report).await, "its next input wakes it");
    assert_eq!(reports.recv().await, Some(0), "resting gave back what it held");
    // Awake again once it has handled it: Acting while it ran, Ready after.
    while presence.changed().await.map(|p| *p.value).ok() != Some(Presence::Ready) {}
}

/// Says how much it wants to act from what it is handed.
struct Asker;

enum AskerIn {
    Addressed,
    Settled,
}

impl Component for Asker {
    type Input = AskerIn;
    const NAME: &'static str = "asker";
    const QOS: Qos = Qos::Work;
    fn wire(&self, _: &mut Wiring<'_, Self>) {}
    async fn handle(&mut self, input: AskerIn, ctx: &mut Ctx<Self>) {
        ctx.need(match input {
            AskerIn::Addressed => Urgency::Urgent,
            AskerIn::Settled => Urgency::Content,
        });
    }
}

// what this catches: a component's wish to act decided for it, or decided with no facts. It says its
// own urgency; the engine publishes it on its owner's bus with its intake depth beside it, starting
// Content, and a tree folds needs by urgency first.
#[tokio::test]
async fn a_component_says_its_own_need_and_the_engine_publishes_it_with_its_intake() {
    let bus = Bus::new();
    let mut need = bus.reader::<Need>(NEED, need_key("asker"));
    let asker = start(Asker, &bus, OwnerId(Uuid::from_u128(4)), 1);
    assert_eq!(need.latest().map(|n| *n.value), Some(Need { urgency: Urgency::Content, waiting: 0 }), "content until it says otherwise");
    assert!(asker.tell(AskerIn::Addressed).await);
    while need.changed().await.map(|n| n.value.urgency).ok() != Some(Urgency::Urgent) {}
    assert!(asker.tell(AskerIn::Settled).await);
    while need.changed().await.map(|n| n.value.urgency).ok() != Some(Urgency::Content) {}
    let busy = Need { urgency: Urgency::Want, waiting: 3 };
    let urgent = Need { urgency: Urgency::Urgent, waiting: 0 };
    assert_eq!(busy.max(urgent), urgent, "urgency decides before queue depth");
    assert_eq!(busy.max(Need { urgency: Urgency::Want, waiting: 1 }), busy, "among equals, more waiting");
}

/// Runs its heavy pass only where its terrain is new to it, like CBAR's color finder at the frontier.
struct Finder {
    covered: Seen<(i32, i32)>,
    heavy_passes: u32,
    reported: tokio::sync::mpsc::UnboundedSender<(Urgency, u32)>,
}

struct Tile(i32, i32);

impl Component for Finder {
    type Input = Tile;
    const NAME: &'static str = "finder";
    const QOS: Qos = Qos::Work;
    fn wire(&self, _: &mut Wiring<'_, Self>) {}
    async fn handle(&mut self, Tile(x, y): Tile, ctx: &mut Ctx<Self>) {
        let novelty = self.covered.novelty(&(x, y));
        let urgency = if novelty == Novelty::FRONTIER { Urgency::Urgent } else { Urgency::Content };
        if urgency == Urgency::Urgent {
            self.heavy_passes += 1;
        }
        self.covered.learn(&(x, y));
        ctx.need(urgency);
        assert!(self.reported.send((urgency, self.heavy_passes)).is_ok(), "the test reads every tile");
    }
}

// what this catches: a part told by a coordinator what is new, or one that only knows its last frame.
// It measures novelty against its own coverage over its whole history: an unseen tile is frontier (it
// runs its heavy pass, Urgent); a tile it covered before is familiar (Content, no pass), even after
// other tiles came between; and familiarity grows with each meeting.
#[tokio::test]
async fn a_part_decides_on_its_own_frontier_from_what_it_knows() {
    let (reported, mut tiles) = tokio::sync::mpsc::unbounded_channel();
    let finder = start(Finder { covered: Seen::default(), heavy_passes: 0, reported }, &Bus::new(), OwnerId(Uuid::from_u128(5)), 1);
    for tile in [Tile(0, 0), Tile(0, 1), Tile(0, 0)] {
        assert!(finder.tell(tile).await);
    }
    assert_eq!(tiles.recv().await, Some((Urgency::Urgent, 1)), "frontier");
    assert_eq!(tiles.recv().await, Some((Urgency::Urgent, 2)), "frontier");
    assert_eq!(tiles.recv().await, Some((Urgency::Content, 2)), "covered before, with another tile between: familiar, no pass");
    let mut seen = Seen::default();
    assert_eq!(seen.novelty(&"a newcomer"), Novelty::FRONTIER);
    seen.learn(&"a newcomer");
    let once = seen.novelty(&"a newcomer");
    seen.learn(&"a newcomer");
    assert!(seen.novelty(&"a newcomer") < once && once < Novelty::FRONTIER, "familiarity grows with each meeting");
    let mut last = LastSeen::default();
    last.learn(&7);
    assert_eq!((last.novelty(&7), last.novelty(&8)), (Novelty::KNOWN, Novelty::FRONTIER));
    assert_eq!(Novelty::new(1.5), None);
}

// what this catches: a bottleneck nobody can see because timing was off (2026-11-22: nothing in
// production ever switched it on). Every component's inputs are measured always (wait in its intake and
// time to handle) and published as state any reader watches live; the switch adds only a probe line per
// input. Numbers accumulate: each input counts, watched or not.
#[tokio::test]
async fn every_input_is_measured_and_the_switch_adds_only_a_probe_line() {
    let bus = Bus::new();
    let mut switch = match bus.publisher::<Observe>(OBSERVE, presence_key("alarm")) {
        Ok(switch) => switch,
        Err(taken) => return assert_eq!(Some(taken), None, "the switch is unclaimed"),
    };
    let mut telemetry = bus.reader::<Telemetry>(TELEMETRY, presence_key("alarm"));
    let (rang, mut rings) = tokio::sync::mpsc::unbounded_channel();
    let alarm = start(Alarm { rang }, &bus, OwnerId(Uuid::from_u128(6)), 1);
    let by = || crate::dataflow::Provenance { by: RunId { incarnation: StageIncarnation { stage: StageId::named("test"), owner: OwnerId(Uuid::from_u128(6)), epoch: 1 }, seq: 1 }, from: Vec::new() };
    let handled = |telemetry: &crate::dataflow::Reader<Telemetry>| telemetry.latest().map(|t| t.value.handled);
    assert!(alarm.tell(AlarmIn::Ring("unwatched")).await);
    assert_eq!(rings.recv().await, Some("unwatched"));
    while handled(&telemetry) != Some(1) {
        assert!(telemetry.changed().await.is_ok(), "its telemetry stays published while it runs");
    }
    switch.publish(Observe::Timing, by());
    assert!(alarm.tell(AlarmIn::Ring("watched")).await);
    assert_eq!(rings.recv().await, Some("watched"));
    while handled(&telemetry) != Some(2) {
        assert!(telemetry.changed().await.is_ok(), "its telemetry stays published while it runs");
    }
    let measured = telemetry.latest().map(|t| *t.value);
    assert!(measured.is_some_and(|t| t.max_wait >= t.last_wait && t.max_handle >= t.last_handle), "wait and handle times are kept: {measured:?}");
}

// what this catches: a component that awaits long work inside `handle` (disk relief held its worker
// 24 s a pass on the M5, 2026-11-23). Work handed off with `ctx.off` runs elsewhere; the component
// handles its other inputs meanwhile and takes the result as an input.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn work_handed_off_comes_back_as_an_input_while_the_part_keeps_hearing() {
    enum Job {
        Start(std::sync::mpsc::Receiver<()>),
        Ping,
        Done(bool),
    }
    struct Offloader {
        said: tokio::sync::mpsc::UnboundedSender<&'static str>,
    }
    impl Component for Offloader {
        type Input = Job;
        const NAME: &'static str = "offloader";
        const QOS: Qos = Qos::Work;
        fn wire(&self, _: &mut Wiring<'_, Self>) {}
        async fn handle(&mut self, input: Job, ctx: &mut Ctx<Self>) {
            match input {
                Job::Start(gate) => ctx.off(move || gate.recv().is_ok(), Job::Done),
                Job::Ping => assert!(self.said.send("ping").is_ok(), "the test reads every value"),
                Job::Done(opened) => assert!(self.said.send(if opened { "done" } else { "wrong" }).is_ok(), "the test reads every value"),
            }
        }
    }
    let (said, mut heard) = tokio::sync::mpsc::unbounded_channel();
    let part = start(Offloader { said }, &Bus::new(), OwnerId(Uuid::from_u128(11)), 1);
    let (open, gate) = std::sync::mpsc::channel();
    assert!(part.tell(Job::Start(gate)).await);
    assert!(part.tell(Job::Ping).await);
    assert_eq!(heard.recv().await, Some("ping"), "it hears other inputs while the work is held");
    assert!(open.send(()).is_ok(), "the work is still waiting on its gate");
    assert_eq!(heard.recv().await, Some("done"), "the result arrives as an input");
}

// Timing runs: what the base costs per input, against a bare stage doing the same work.
#[cfg(feature = "stress-tests")]
mod stress {
    use super::*;
    use crate::{spawn_stage, Capacity, Stage};

    const INPUTS: u64 = 200_000;

    const BURST: TopicName = TopicName("stress/burst");

    /// The window the hearer keeps of the burst: 256 events, this measurement's choice.
    fn window() -> std::num::NonZeroUsize {
        std::num::NonZeroUsize::MIN.saturating_add(255)
    }

    struct Hearer {
        handled: u64,
        done: Option<tokio::sync::oneshot::Sender<u64>>,
    }

    impl Component for Hearer {
        type Input = Event<u64>;
        const NAME: &'static str = "hearer";
        const QOS: Qos = Qos::Work;
        fn wire(&self, wiring: &mut Wiring<'_, Self>) {
            wiring.hear::<u64>(BURST, window());
        }
        async fn handle(&mut self, input: Event<u64>, _: &mut Ctx<Self>) {
            self.handled += 1;
            if *input.value == INPUTS {
                if let Some(done) = self.done.take() {
                    assert!(done.send(self.handled).is_ok(), "the test waits for the count");
                }
            }
        }
    }

    struct Bare {
        sum: u64,
        done: Option<tokio::sync::oneshot::Sender<u64>>,
    }

    impl Stage for Bare {
        type Input = u64;
        const QOS: Qos = Qos::Work;
        async fn handle(&mut self, input: u64) {
            self.sum += input;
            if input == INPUTS {
                if let Some(done) = self.done.take() {
                    assert!(done.send(self.sum).is_ok(), "the test waits for the sum");
                }
            }
        }
    }

    struct Counted {
        sum: u64,
        done: Option<tokio::sync::oneshot::Sender<u64>>,
    }

    impl Component for Counted {
        type Input = u64;
        const NAME: &'static str = "counted";
        const QOS: Qos = Qos::Work;
        fn wire(&self, _: &mut Wiring<'_, Self>) {}
        async fn handle(&mut self, input: u64, _: &mut Ctx<Self>) {
            self.sum += input;
            if input == INPUTS {
                if let Some(done) = self.done.take() {
                    assert!(done.send(self.sum).is_ok(), "the test waits for the sum");
                }
            }
        }
    }

    // what this catches: per-input cost the base adds to every component (publishes, clock reads,
    // allocations) growing unseen. Prints ns per input for a bare stage and for a component; run with
    // `cargo test -p continuum-sage --release --features stress-tests -- --nocapture per_input`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn per_input_cost_of_the_base_against_a_bare_stage() {
        let (done, sum) = tokio::sync::oneshot::channel();
        let (input, _worker) = spawn_stage(Bare { sum: 0, done: Some(done) }, Capacity::granted(std::num::NonZeroUsize::MIN));
        let started = std::time::Instant::now();
        for value in 1..=INPUTS {
            assert!(input.send(value).await.is_ok());
        }
        assert!(sum.await.is_ok());
        let bare = started.elapsed().as_nanos() as f64 / INPUTS as f64;

        let bus = Bus::new();
        let (done, sum) = tokio::sync::oneshot::channel();
        let part = start(Counted { sum: 0, done: Some(done) }, &bus, OwnerId(Uuid::from_u128(7)), 1);
        let started = std::time::Instant::now();
        for value in 1..=INPUTS {
            assert!(part.tell(value).await);
        }
        assert!(sum.await.is_ok());
        let component = started.elapsed().as_nanos() as f64 / INPUTS as f64;
        println!("per input: bare stage {bare:.0} ns, component {component:.0} ns, base adds {:.0} ns", component - bare);

        // A burst on a topic it hears: its intake holds its window, so a delivery moves many per wake.
        let (done, heard) = tokio::sync::oneshot::channel();
        let mut emitter = match bus.emitter::<u64>(BURST) {
            Ok(emitter) => emitter,
            Err(taken) => return assert_eq!(Some(taken), None, "the burst topic is unclaimed"),
        };
        let _hearer = start(Hearer { handled: 0, done: Some(done) }, &bus, OwnerId(Uuid::from_u128(8)), 1);
        let started = std::time::Instant::now();
        for value in 1..=INPUTS {
            emitter.emit(value);
        }
        let handled = match heard.await {
            Ok(handled) => handled,
            Err(gone) => return assert_eq!(Some(gone), None, "the hearer reports what it handled"),
        };
        let burst = started.elapsed().as_nanos() as f64 / INPUTS as f64;
        println!("burst heard: {burst:.0} ns per event emitted, {handled} of {INPUTS} handled (window {}; the rest dropped oldest-first)", window());
    }
}
