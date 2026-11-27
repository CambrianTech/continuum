//! The capture's write-behind (card 699578cb, slice 3): a SAGE component on the node bus that hears
//! every capture record and appends it to its owner's binary log. The turn path emits a record and
//! never waits: an emit is a queue insert into this component's window, and a full window displaces its
//! oldest unwritten record. A displaced record is never silent: the gap in revisions is counted and said
//! as `capture.dropped` (the count, and an estimate of its bytes from the measured mean frame, labelled as
//! an estimate; the stream carries typed records, so it cannot know what it displaced).
//!
//! The window is measured, never a constant (Fable, 2026-11-22): at start, from the owners' newest
//! generations, the most records that ever arrived within one p99 append (by their index timestamps and
//! measured `write_us`) is what the writer must hold while one write is in flight; its bytes (window x
//! measured mean frame) are claimed from the node's resource authority as a RAM residency. Until SAGE's
//! windows resize live, the next start recomputes it.

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::Arc;

use continuum_kernel::system_resources::disk_reporters::ClassWriter;
use continuum_sage::component::{Bus, Component, Ctx, Wiring};
use continuum_sage::dataflow::{Event, TopicName};
use continuum_sage::Qos;
use uuid::Uuid;

use crate::capture_log::{self, CaptureRecord, Entry, OpenLogs};

/// The topic every capture record is emitted on, on the node bus.
pub const CAPTURE: TopicName = TopicName("capture/records");

/// What the writer measured at start, from the frames it actually wrote before.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sizing {
    /// The mean bytes of a written frame; `None` before anything was written.
    pub mean_frame: Option<f64>,
    /// The records the window holds: the most that arrived within one p99 append; at least one.
    pub window: NonZeroUsize,
}

/// PURE: the window and mean frame from written index entries (any order). The p99 append time is
/// the nearest-rank 99th percentile of their `write_us`; the window is the most entries whose
/// timestamps fall within one such span of each other (at least one).
pub fn sizing(entries: &[Entry]) -> Sizing {
    if entries.is_empty() {
        return Sizing { mean_frame: None, window: NonZeroUsize::MIN };
    }
    let mean_frame = entries.iter().map(|e| f64::from(e.length)).sum::<f64>() / entries.len() as f64;
    let mut writes: Vec<u32> = entries.iter().map(|e| e.write_us).collect();
    writes.sort_unstable();
    let p99_ms = u64::from(writes[(writes.len() - 1) * 99 / 100]).div_ceil(1000);
    let mut times: Vec<u64> = entries.iter().map(|e| e.captured_at_ms).collect();
    times.sort_unstable();
    let (mut most, mut from) = (1usize, 0usize);
    for (to, &t) in times.iter().enumerate() {
        while t - times[from] > p99_ms {
            from += 1;
        }
        most = most.max(to - from + 1);
    }
    Sizing { mean_frame: Some(mean_frame), window: NonZeroUsize::new(most).unwrap_or(NonZeroUsize::MIN) } // unwrap_or: `most` starts at one
}

/// Every written entry of each owner's newest generation in `class`: what the next start sizes from.
pub fn newest_entries(class: &ClassWriter) -> Vec<Entry> {
    let owners = class.owners().unwrap_or_default(); // unwrap_or_default: an unreadable class root has written nothing to size from
    owners
        .into_iter()
        .flat_map(|owner| {
            let scoped = class.owned_by(owner);
            let newest = capture_log::generations(&scoped).ok().and_then(|g| g.last().copied());
            newest.and_then(|g| capture_log::entries(&scoped, g).ok()).unwrap_or_default() // unwrap_or_default: an owner with no readable generation adds nothing
        })
        .collect()
}

/// The write-behind: one log per owner, appended off the async workers.
struct CaptureWriter {
    class: ClassWriter,
    bus: &'static Bus,
    logs: Arc<OpenLogs>,
    sizing: Sizing,
    /// The newest revision handled: a jump past it is records the window displaced.
    last: u64,
    /// The bytes the window may hold, claimed from the node's resource authority (none on a node without one).
    _claim: Option<continuum_kernel::resources::ResidencyGuard<1>>,
    /// Held for as long as this writer runs: the room sees it through a weak handle, so a writer
    /// whose worker ended (it panicked, or the runtime it ran on shut down) is seen gone at the next
    /// reservation, never filled silently.
    _alive: Arc<()>,
}

impl CaptureWriter {
    /// The writer for `class`, sized from what it wrote before, its window's bytes claimed as RAM.
    fn new(class: ClassWriter, bus: &'static Bus, alive: Arc<()>) -> Self {
        let sizing = sizing(&newest_entries(&class));
        // nothing written yet: no measurement, so no claim until a later start has one
        let bytes = sizing.mean_frame.map(|mean| (mean * sizing.window.get() as f64) as u64);
        let _claim = bytes.zip(continuum_kernel::resources::ResourceDaemon::global()).map(|(resident, daemon)| {
            daemon.hold_residency("capture-writer", [continuum_kernel::resources::ResidentFloor {
                kind: continuum_kernel::resources::ResourceKind::Ram,
                resident,
                headroom: 0,
                fixed: resident, // the writer's window is held for as long as it writes: nothing demotes it
            }])
        });
        continuum_base::probe!(class = "capture.writer.sized", window = sizing.window.get() as u64, mean_frame = ?sizing.mean_frame, bytes = ?bytes,
            "the capture writer's window, measured from the frames it wrote before");
        let logs = OpenLogs::shared();
        // Earlier builds' JSONL captures move into frames through these same open logs, off the
        // workers, while the writer serves (card 699578cb, slice 5).
        let (moving, into) = (class.clone(), Arc::clone(&logs));
        tokio::task::spawn_blocking(move || crate::jsonl_migration::migrate(&moving, &into));
        Self { class, bus, logs, sizing, last: 0, _claim, _alive: alive }
    }

}

/// Where a capture's records go.
#[derive(Clone)]
pub enum Delivery {
    /// Emitted on the node bus for the node's writer: the turn never waits on the disk.
    Node,
    /// Appended in the caller, for a process with no node writer (a test, an offline tool): the
    /// same append the writer makes, readable the moment it returns.
    Inline(Arc<Inline>),
}

/// An inline delivery's own open logs.
pub struct Inline {
    class: ClassWriter,
    logs: Arc<OpenLogs>,
}

impl Delivery {
    /// Inline delivery under `class`. Every inline delivery to one class root shares its open logs,
    /// so two sinks of one owner never each open a generation and append over each other.
    pub fn inline(class: ClassWriter) -> Self {
        static SHARED: std::sync::LazyLock<parking_lot::Mutex<HashMap<std::path::PathBuf, std::sync::Weak<Inline>>>> = std::sync::LazyLock::new(Default::default);
        let root = class.dir();
        let mut shared = SHARED.lock();
        if let Some(open) = shared.get(&root).and_then(std::sync::Weak::upgrade) {
            return Self::Inline(open);
        }
        let inline = Arc::new(Inline { class, logs: OpenLogs::shared() });
        shared.retain(|_, held| held.strong_count() > 0);
        shared.insert(root, Arc::downgrade(&inline));
        Self::Inline(inline)
    }

    /// Reserve a place for each of a call's `N` records before the first is delivered: a node delivery
    /// grows the writer's window now, so none of them can be displaced; an inline one appends in the
    /// caller and holds nothing.
    pub fn reserve<const N: usize>(&self) -> Result<[crate::capture_room::Slot; N], crate::capture_room::Unreserved> {
        match self {
            Self::Node => crate::capture_room::reserve::<N>(),
            Self::Inline(_) => Ok(std::array::from_fn(|_| crate::capture_room::Slot::inline())),
        }
    }

    /// Deliver `record` into the place `slot` reserved for it; a record that cannot go is said,
    /// loudly, and lost.
    pub fn deliver(&self, record: CaptureRecord, slot: crate::capture_room::Slot) {
        match self {
            Self::Node => {
                emit(record);
                slot.spend();
            }
            Self::Inline(inline) => {
                if let Err(error) = inline.logs.append(&inline.class, &record) {
                    continuum_base::probe!(class = "capture.write_failed", owner = %record.owner, error = %error,
                        "a capture record was not written: this record is lost, loudly");
                }
            }
        }
    }
}

/// The node bus's one emitter on [`CAPTURE`], claimed on first use; every sink emits through it.
static EMITTER: std::sync::OnceLock<Option<parking_lot::Mutex<continuum_sage::dataflow::Emitter<CaptureRecord>>>> = std::sync::OnceLock::new();

/// Say `record` on the node bus; never waits on the disk (the lock covers a queue insert only).
fn emit(record: CaptureRecord) {
    match EMITTER.get_or_init(|| continuum_sage::component::node().emitter::<CaptureRecord>(CAPTURE).ok().map(parking_lot::Mutex::new)) {
        Some(emitter) => {
            emitter.lock().emit(record);
        }
        None => continuum_base::probe!(class = "capture.topic_taken", owner = %record.owner,
            "the capture topic's emitter is held elsewhere: this record reaches no writer"),
    }
}

/// The node's one capture writer, held for the process.
static NODE: std::sync::OnceLock<continuum_sage::component::Running<CaptureWriter>> = std::sync::OnceLock::new();

/// Start the node's capture writer for `class` on the node bus, once; a second call is a wiring
/// defect, probed, and changes nothing. Called inside the runtime its worker runs on; outside one
/// it is refused, loudly, and nothing starts.
pub fn start_node(class: ClassWriter) {
    if tokio::runtime::Handle::try_current().is_err() {
        continuum_base::probe!(class = "capture.writer.no_runtime", "the capture writer was started outside a runtime: captures have no writer this run");
        return;
    }
    let mut started = false;
    NODE.get_or_init(|| {
        started = true;
        let bus = continuum_sage::component::node();
        let alive = Arc::new(());
        let gone_when = Arc::downgrade(&alive);
        let writer = CaptureWriter::new(class, bus, alive);
        let base = writer.sizing.window;
        let running = continuum_sage::component::start(writer, bus, continuum_sage::dataflow::OwnerId(Uuid::nil()), 0);
        // Calls reserve their records in this window before delivering any (capture_room).
        match running.take_window(CAPTURE) {
            Some(window) => {
                crate::capture_room::install(window, base, gone_when);
            }
            None => continuum_base::probe!(class = "capture.writer.no_window",
                "the capture writer hears no capture topic: no call can reserve room, so none is recorded"),
        }
        running
    });
    if !started {
        continuum_base::probe!(class = "capture.writer.started_twice", "the node's capture writer was started a second time; the first keeps running");
    }
}

/// Whether this node's capture writer was started: a caller that must not serve a call
/// unrecorded refuses it when not.
pub fn running() -> bool {
    NODE.get().is_some()
}

impl From<Event<CaptureRecord>> for Heard {
    fn from(event: Event<CaptureRecord>) -> Self {
        Heard(event)
    }
}

/// A capture record as heard, with its revision.
struct Heard(Event<CaptureRecord>);

impl Component for CaptureWriter {
    type Input = Heard;
    const NAME: &'static str = "capture-writer";
    const QOS: Qos = Qos::Background;

    fn wire(&self, wiring: &mut Wiring<'_, Self>) {
        wiring.hear_on::<CaptureRecord>(self.bus, CAPTURE, self.sizing.window);
    }

    async fn handle(&mut self, Heard(event): Heard, _ctx: &mut Ctx<Self>) {
        let through = event.revision.through;
        let displaced = through.saturating_sub(self.last).saturating_sub(1);
        if displaced > 0 {
            let estimated_bytes = self.sizing.mean_frame.map(|mean| (mean * displaced as f64) as u64);
            continuum_base::probe!(class = "capture.dropped", count = displaced, estimated_bytes = ?estimated_bytes,
                "capture records were displaced from the writer's window before they were written (estimated_bytes = count x measured mean frame: an estimate)");
        }
        self.last = through;
        if displaced > 0 {
            crate::capture_room::free(displaced as usize);
        }
        let (class, logs, record) = (self.class.clone(), Arc::clone(&self.logs), Arc::clone(&event.value));
        let written = tokio::task::spawn_blocking(move || logs.append(&class, &record)).await;
        let failure = match written {
            Ok(Ok(())) => None,
            Ok(Err(error)) => Some(error.to_string()),
            Err(join) => Some(join.to_string()),
        };
        // its place is free once it is written, or failed loudly below
        crate::capture_room::free(1);
        if let Some(error) = failure {
            continuum_base::probe!(class = "capture.write_failed", owner = %event.value.owner, error = %error,
                "a capture record was heard but not written: this record is lost, loudly");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::FrameKind;

    fn entry(captured_at_ms: u64, write_us: u32, length: u32) -> Entry {
        Entry { offset: 0, length, kind: FrameKind::Submitted as u8, captured_at_ms, write_us }
    }

    // what this catches: a window that is a constant instead of the writer's measured history (too
    // small and a burst is displaced while one write is in flight; too large and memory is held for
    // nothing), a p99 read off the wrong end, or a mean frame that is not the frames' own.
    #[test]
    fn the_window_is_the_most_records_within_one_p99_append() {
        assert_eq!(sizing(&[]), Sizing { mean_frame: None, window: NonZeroUsize::MIN }, "nothing written: one record, no claim");
        // appends take 2 ms at p99; records arrive at 0, 1, 2 (a burst of three within 2 ms), then 10, 20
        let mut entries: Vec<Entry> = [0, 1, 2, 10, 20].iter().map(|&t| entry(t, 2_000, 100)).collect();
        entries[4].write_us = 500;
        let s = sizing(&entries);
        assert_eq!(s.window.get(), 3, "the burst that arrives within one p99 append");
        assert_eq!(s.mean_frame, Some(100.0));
        // slow appends (12 ms) put everything within one span: the window holds all five
        let slow: Vec<Entry> = [0, 1, 2, 10, 12].iter().map(|&t| entry(t, 12_000, 300)).collect();
        assert_eq!(sizing(&slow).window.get(), 5);
    }

    // what this catches (the IntelMac and the 5090, 2026-11-23: a fresh node's window held one
    // record, so a call's Submitted and Terminal arriving within one append lost the Submitted):
    // the production path, a sink delivering to the node, losing a reserved record; a reservation
    // that never frees (the window only ever grows); an unspent slot that never frees; or N calls'
    // pairs delivered at once, before the writer runs, losing any of them.
    #[test]
    fn every_reserved_record_reaches_the_node_writer_and_its_place_is_freed() {
        // the writer runs on a runtime of its own, ended below: a reservation after it is refused
        let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().expect("test: runtime");
        runtime.block_on(async {
        use continuum_kernel::system_resources::disk_reporters::TrackedDir;
        use continuum_sage::component::{presence_key, Telemetry, TELEMETRY};

        let dir = tempfile::tempdir().expect("test: dir");
        let class = || TrackedDir::new("captures-node-test", dir.path().to_path_buf()).writer();
        start_node(class());
        assert!(running(), "the node writer runs once started");

        let record = |owner, kind| CaptureRecord { owner, kind, header: vec![0x90], runs: vec![Arc::from(&b"prompt"[..])], captured_at_ms: 1 };
        // a slot a call never spends frees itself
        drop(Delivery::Node.reserve::<2>().expect("test: room"));
        assert_eq!(crate::capture_room::reserved(), Some(0), "unspent slots free themselves");

        // five calls, each delivering its pair back to back before the writer has run once
        let owners: Vec<Uuid> = (0..5).map(|_| Uuid::new_v4()).collect();
        for &owner in &owners {
            let [submitted, terminal] = Delivery::Node.reserve::<2>().expect("test: room for the pair");
            Delivery::Node.deliver(record(owner, FrameKind::Submitted), submitted);
            Delivery::Node.deliver(record(owner, FrameKind::Terminal), terminal);
        }

        let mut telemetry = continuum_sage::component::node().reader::<Telemetry>(TELEMETRY, presence_key(CaptureWriter::NAME));
        while telemetry.latest().is_none_or(|t| t.value.handled < 10) {
            telemetry.changed().await.expect("test: the node bus outlives the writer");
        }
        for owner in owners {
            let mut kinds = Vec::new();
            capture_log::scan(&class().owned_by(owner), |_, frame| kinds.push(frame.kind)).expect("test: the owner's log reads");
            assert_eq!(kinds, vec![FrameKind::Submitted, FrameKind::Terminal], "both of the call's records, in order");
        }
        assert_eq!(crate::capture_room::reserved(), Some(0), "every written record freed its place: the window is back to its base");
        });
        drop(runtime);
        // what this catches too (Fable on #6236): a writer whose runtime ended, its worker gone, still
        // granting room, so every later record fills a window nothing writes.
        assert!(matches!(Delivery::Node.reserve::<2>(), Err(crate::capture_room::Unreserved::WriterGone)), "a gone writer grants no room");
    }
}
