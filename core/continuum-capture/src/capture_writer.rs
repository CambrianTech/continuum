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

use crate::capture_log::{self, CaptureLog, Entry};
use crate::frame::FrameKind;

/// The topic every capture record is emitted on, on the node bus.
pub const CAPTURE: TopicName = TopicName("capture/records");

/// One record to capture: its owner, its kind, its header already encoded (`frame` keeps it as given)
/// and its bulk as shared byte runs (handed over, never copied on the turn path).
#[derive(Debug, Clone)]
pub struct CaptureRecord {
    pub owner: Uuid,
    pub kind: FrameKind,
    pub header: Vec<u8>,
    pub runs: Vec<Arc<[u8]>>,
    pub captured_at_ms: u64,
}

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
    logs: Arc<parking_lot::Mutex<HashMap<Uuid, CaptureLog>>>,
    sizing: Sizing,
    /// The newest revision handled: a jump past it is records the window displaced.
    last: u64,
    /// The bytes the window may hold, claimed from the node's resource authority (none on a node without one).
    _claim: Option<continuum_kernel::resources::ResidencyGuard<1>>,
}

impl CaptureWriter {
    /// The writer for `class`, sized from what it wrote before, its window's bytes claimed as RAM.
    fn new(class: ClassWriter, bus: &'static Bus) -> Self {
        let sizing = sizing(&newest_entries(&class));
        // nothing written yet: no measurement, so no claim until a later start has one
        let bytes = sizing.mean_frame.map(|mean| (mean * sizing.window.get() as f64) as u64);
        let _claim = bytes.zip(continuum_kernel::resources::ResourceDaemon::global()).map(|(resident, daemon)| {
            daemon.hold_residency("capture-writer", [continuum_kernel::resources::ResidentFloor {
                kind: continuum_kernel::resources::ResourceKind::Ram,
                resident,
                headroom: 0,
            }])
        });
        continuum_base::probe!(class = "capture.writer.sized", window = sizing.window.get() as u64, mean_frame = ?sizing.mean_frame, bytes = ?bytes,
            "the capture writer's window, measured from the frames it wrote before");
        Self { class, bus, logs: Default::default(), sizing, last: 0, _claim }
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
        continuum_sage::component::start(CaptureWriter::new(class, bus), bus, continuum_sage::dataflow::OwnerId(Uuid::nil()), 0)
    });
    if !started {
        continuum_base::probe!(class = "capture.writer.started_twice", "the node's capture writer was started a second time; the first keeps running");
    }
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
        let (class, logs, record) = (self.class.clone(), Arc::clone(&self.logs), Arc::clone(&event.value));
        let written = tokio::task::spawn_blocking(move || {
            let mut logs = logs.lock();
            let log = match logs.entry(record.owner) {
                std::collections::hash_map::Entry::Occupied(open) => open.into_mut(),
                std::collections::hash_map::Entry::Vacant(slot) => slot.insert(CaptureLog::open(class.owned_by(record.owner))?),
            };
            let runs: Vec<&[u8]> = record.runs.iter().map(AsRef::as_ref).collect();
            log.append_parts(crate::frame::Parts { kind: record.kind, header: &record.header, runs: &runs }, record.captured_at_ms).map(|_| ()).map_err(std::io::Error::other)
        })
        .await;
        let failure = match written {
            Ok(Ok(())) => None,
            Ok(Err(error)) => Some(error.to_string()),
            Err(join) => Some(join.to_string()),
        };
        if let Some(error) = failure {
            continuum_base::probe!(class = "capture.write_failed", owner = %event.value.owner, error = %error,
                "a capture record was heard but not written: this record is lost, loudly");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
