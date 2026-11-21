//! The fork's engine-step stream into the core's capture (CONTINUUM-ENGINE 4.10 P5 slice 1, #6055).
//!
//! The fork publishes `GET /engine/steps`: one SSE record per line of its own step capture, beside
//! (never through) `/learn/events`. It opens with the engine's identity, a capture id and its clock
//! origin, then sends every record with a running sequence number; a run it could not keep is sent
//! as `missed`. This module reads that stream for one engine incarnation and records each record into
//! a [`CaptureSink`] on the core's clock: the engine's monotonic time is mapped onto the core's
//! `Instant` once, at the opening record, so every record keeps the order and spacing it happened
//! with. A `missed` record, or a gap in the sequence numbers, is reported to the sink as loss, so the
//! interval it falls in can never gate. The stream ending ends the capture; a reconnect is a new one.

use std::time::{Duration, Instant};

use continuum_base::capture::CaptureSink;
use continuum_sage::dataflow::OwnerId;

use super::{EngineIncarnation, SequenceChange, SequenceId, StepId, StepWork, WorkloadEvent};

/// What a sequence's change is on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum WireChange {
    Arrived { resident: u32, fresh: u32 },
    Ready,
    BlockedOnMemory,
    BlockedOnPageIn,
    Resumed,
    Completed { decoded: u32 },
    Cancelled,
    PagedOut { tokens: u32 },
    PagedIn { tokens: u32 },
}

impl WireChange {
    fn typed(self) -> SequenceChange {
        use super::BlockedOn;
        match self {
            WireChange::Arrived { resident, fresh } => SequenceChange::Arrived { resident, fresh },
            WireChange::Ready => SequenceChange::Ready,
            WireChange::BlockedOnMemory => SequenceChange::Blocked { on: BlockedOn::Memory },
            WireChange::BlockedOnPageIn => SequenceChange::Blocked { on: BlockedOn::PageIn },
            WireChange::Resumed => SequenceChange::Resumed,
            WireChange::Completed { decoded } => SequenceChange::Completed { decoded },
            WireChange::Cancelled => SequenceChange::Cancelled,
            WireChange::PagedOut { tokens } => SequenceChange::PagedOut { tokens },
            WireChange::PagedIn { tokens } => SequenceChange::PagedIn { tokens },
        }
    }
}

/// One sequence's share of a step, on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
struct WireWork {
    task: u64,
    depth: u32,
    prefilled: u32,
    decoded: u32,
    accepted_draft: Option<u32>,
}

/// One record of the fork's step stream (the engine process boundary).
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Record {
    /// The stream's first record: the clock origin every `at_us` counts from.
    Opened { origin_us: u64 },
    Began { seq: u64, at_us: u64, step: u64, chosen: Vec<u64> },
    Ended { seq: u64, at_us: u64, step: u64, work: Vec<WireWork> },
    Sequence { seq: u64, at_us: u64, task: u64, mind: uuid::Uuid, change: WireChange },
    /// `count` records the engine's ring could not keep, at this position.
    Missed { seq: u64, count: u64 },
}

/// Why a step stream ended.
#[derive(Debug)]
pub enum StepStreamEnd {
    /// The engine closed it (its incarnation ended, or it stopped publishing).
    Closed,
    /// The engine could not be reached, or its stream broke.
    Unreachable(reqwest::Error),
    /// The engine refused the subscription (another subscriber holds it, or no bound was named).
    Refused(reqwest::StatusCode),
    /// A record the core could not read: the capture cannot be trusted past it.
    Unreadable(serde_json::Error),
    /// The stream did not open with its clock origin.
    NoOrigin,
}

/// Where a stream's records land: the engine they came from and the clock that maps them.
struct Reader<'a> {
    engine: EngineIncarnation,
    sink: &'a dyn CaptureSink<WorkloadEvent>,
    /// The core's `Instant` matching the engine's `origin_us`, fixed when the stream opened.
    anchor: Option<(Instant, u64)>,
    next_seq: u64,
}

impl Reader<'_> {
    fn when(&self, at_us: u64) -> Result<Instant, StepStreamEnd> {
        let (anchor, origin_us) = self.anchor.ok_or(StepStreamEnd::NoOrigin)?;
        Ok(anchor + Duration::from_micros(at_us.saturating_sub(origin_us)))
    }

    /// Count `seq`, reporting any records skipped before it as loss.
    fn sequenced(&mut self, seq: u64) {
        if seq > self.next_seq {
            self.sink.missed(seq - self.next_seq);
        }
        self.next_seq = self.next_seq.max(seq + 1);
    }

    fn read(&mut self, record: Record) -> Result<(), StepStreamEnd> {
        let engine = self.engine;
        match record {
            Record::Opened { origin_us } => self.anchor = Some((Instant::now(), origin_us)),
            Record::Began { seq, at_us, step, chosen } => {
                self.sequenced(seq);
                let chosen = chosen.into_iter().map(SequenceId).collect();
                self.sink.record(self.when(at_us)?, WorkloadEvent::StepBegan { engine, step: StepId(step), chosen });
            }
            Record::Ended { seq, at_us, step, work } => {
                self.sequenced(seq);
                let work = work
                    .into_iter()
                    .map(|w| StepWork { sequence: SequenceId(w.task), depth: w.depth, prefilled: w.prefilled, decoded: w.decoded, accepted_draft: w.accepted_draft })
                    .collect();
                self.sink.record(self.when(at_us)?, WorkloadEvent::StepEnded { engine, step: StepId(step), work });
            }
            Record::Sequence { seq, at_us, task, mind, change } => {
                self.sequenced(seq);
                let event = WorkloadEvent::Sequence { engine, sequence: SequenceId(task), mind: OwnerId(mind), change: change.typed() };
                self.sink.record(self.when(at_us)?, event);
            }
            Record::Missed { seq, count } => {
                self.sequenced(seq);
                self.sink.missed(count);
            }
        }
        Ok(())
    }
}

/// One engine's step stream: where it serves, which incarnation it is, and the bound the engine's
/// ring keeps for this subscriber (the core's memory to spend on records not yet read).
#[derive(Debug, Clone, Copy)]
pub struct StepSource<'a> {
    pub root: &'a str,
    pub engine: EngineIncarnation,
    pub capacity: std::num::NonZeroUsize,
}

/// Record `source`'s step stream into `sink` until it ends.
pub async fn record_steps(client: &reqwest::Client, source: StepSource<'_>, sink: &dyn CaptureSink<WorkloadEvent>) -> StepStreamEnd {
    use futures::StreamExt;
    let StepSource { root, engine, capacity } = source;
    let response = match client.get(format!("{root}/engine/steps")).query(&[("capacity", capacity.get())]).send().await {
        Ok(response) if response.status().is_success() => response,
        Ok(response) => return StepStreamEnd::Refused(response.status()),
        Err(error) => return StepStreamEnd::Unreachable(error),
    };
    let mut reader = Reader { engine, sink, anchor: None, next_seq: 0 };
    let mut pending = String::new();
    let mut body = response.bytes_stream();
    while let Some(chunk) = body.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(error) => return StepStreamEnd::Unreachable(error),
        };
        pending.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(end) = pending.find("\n\n") {
            let frame: String = pending.drain(..end + 2).collect();
            for data in frame.lines().filter_map(|line| line.strip_prefix("data: ")) {
                // the engine process boundary: its records arrive as JSON and are typed here, once
                let record: Record = match serde_json::from_str(data) {
                    Ok(record) => record,
                    Err(error) => return StepStreamEnd::Unreadable(error),
                };
                if let Err(end) = reader.read(record) {
                    return end;
                }
            }
        }
    }
    StepStreamEnd::Closed
}

#[cfg(test)]
mod tests;
