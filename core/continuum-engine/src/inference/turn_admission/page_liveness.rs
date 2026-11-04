//! Whether the engine under a page switch is working or gone, read where its request queue cannot
//! hide the answer.
//!
//! llama-server answers `/slots` as a task on its MAIN LOOP's queue, so while one long CPU batch
//! runs the probe cannot answer inside its timeout, and a working engine read as stopped. The
//! IntelMac, 2026-11-04 16:24:20Z (`~/.continuum/evidence/intelmac-stall-20261104/`): a page
//! restore went uncertain, the endpoint was quarantined and the engine replaced, while a stack
//! sample taken just before showed its main thread in `update_slots -> decode`; every resident
//! then cold-prefilled 24-32k tokens at 20-28 tok/s (~20 min an act), which recreated the load.
//!
//! So a switch reads the engine PROCESS's own CPU through the kernel's one process-table owner
//! ([`crate::system_resources::process_tree::CpuWatch`], fenced by pid AND start time). The HTTP
//! fingerprint stays the richer positive signal when it answers; it is never required:
//! - the process is gone (exited, or its pid names another start time): stopped, at once;
//! - it spent CPU since the last checkpoint and `/slots` is silent: a processing engine whose
//!   counters have not moved, which the wait's measured `unmoved` window already bounds;
//! - it spent no CPU and `/slots` is silent: stopped.
//!
//! CPU is evidence AGAINST replacing the engine, never proof that the switch progresses (a spin or
//! another slot's batch burns CPU too; Codex). A wait that ends while the process still spends CPU
//! is [`Receipt::Unresolved`]: the engine is not replaced on that alone, the endpoint keeps the
//! request until its receipt arrives, and the page is held as an uncertain switch leaves it.

use std::future::Future;
use std::pin::Pin;

use crate::inference::llama_server::{EngineProbe, EngineProgress, WaitBounds};
use crate::system_resources::process_tree::{CpuSample, CpuWatch, ProcessIdentity};

/// How many consecutive checkpoints a switch may wait on an engine that is doing nothing
/// else — no counter moving, no slot processing — before it is called stuck. On such an
/// engine the queue head is the switch itself (a save/restore moves no slot counter), so
/// "quiet" is not evidence of a stall; this caps how long quiet may last.
// derived-or-floor: a CEILING on a wait that has no progress signal to read — the slot counters cannot see a save/restore, so the bound (itself derived from this node's measured switch p90) times this count is the patience for the engine's own queue head; it only ever ENDS a wait, never shortens one that shows progress.
const QUIET_ENGINE_CHECKPOINTS: u32 = 8;

/// A page switch's wait: [`crate::inference::llama_server::wait_while_engine_progresses`]
/// with this module's quiet allowance, because a save/restore moves no slot counter and a
/// quiet engine here is working the switch itself.
pub(super) async fn wait_while_engine_progresses<T, W, P, PF>(
    work: W,
    bounds: impl Into<WaitBounds>,
    progress: P,
    on_busy: impl FnMut(u64),
) -> Option<T>
where
    W: Future<Output = T>,
    P: FnMut() -> PF,
    PF: Future<Output = EngineProbe>,
{
    crate::inference::llama_server::wait_while_engine_progresses(work, bounds, QUIET_ENGINE_CHECKPOINTS, progress, on_busy).await
}

/// Where a switch reads its engine's CPU: `None` when there is no process this node can read
/// (an endpoint it did not launch), and the switch is judged by HTTP alone.
pub(super) type CpuSource = Box<dyn FnMut() -> Option<CpuSample> + Send>;

/// What the engine's process said at the latest checkpoint.
#[derive(Debug, Clone, Copy, PartialEq)]
enum CpuEvidence {
    /// Nothing to read, or a first read with no interval behind it.
    Unread,
    /// The same process accumulated this much CPU time since the previous read.
    Spent(std::time::Duration),
    /// The same process spent none.
    Flat,
    /// Exited, or its pid names another process.
    Gone,
}

struct Seen {
    cpu: CpuEvidence,
    /// The probe the wait was last handed: what ended it, when it ended on a probe.
    verdict: Option<EngineProbe>,
    /// The last fingerprint `/slots` answered with: a silent checkpoint on a working process
    /// repeats it, so the wait reads "processing, unmoved" and its measured window bounds it.
    anchor: Option<u64>,
}

/// One switch's liveness evidence: its engine's CPU source and what each checkpoint saw. Shared
/// by the quote wait and the receipt wait, so the CPU interval runs across both.
pub(super) struct PageLiveness {
    cpu: parking_lot::Mutex<CpuSource>,
    seen: parking_lot::Mutex<Seen>,
}

impl PageLiveness {
    pub(super) fn new(cpu: CpuSource) -> Self {
        Self {
            cpu: parking_lot::Mutex::new(cpu),
            seen: parking_lot::Mutex::new(Seen { cpu: CpuEvidence::Unread, verdict: None, anchor: None }),
        }
    }

    /// The live lane's own process when `root` is that lane (its record's pid and start time,
    /// as adoption fences it); HTTP alone for any other endpoint.
    pub(super) fn for_root(root: &str) -> Self {
        let lane = crate::inference::lane_registry::live_incarnation()
            // `LaneRecord::root_url`'s spelling: the one every in-process client addresses it by
            .filter(|lane| format!("http://127.0.0.1:{}", lane.port) == root.trim_end_matches('/'));
        match lane {
            Some(lane) => {
                let mut watch = CpuWatch::new(ProcessIdentity { pid: lane.pid, start_time: lane.started_s });
                Self::new(Box::new(move || Some(watch.sample())))
            }
            None => Self::new(Box::new(|| None)),
        }
    }

    /// Fold one `/slots` probe with the engine process's CPU since the previous checkpoint.
    pub(super) fn observe(&self, http: EngineProbe) -> EngineProbe {
        // One pid's row from the process table: a syscall, not a wait, at checkpoint cadence.
        let sample = {
            let mut read = self.cpu.lock();
            (*read)()
        };
        let cpu = match sample {
            None | Some(CpuSample::Alive { spent: None }) => CpuEvidence::Unread,
            Some(CpuSample::Gone) => CpuEvidence::Gone,
            Some(CpuSample::Alive { spent: Some(spent) }) if !spent.is_zero() => CpuEvidence::Spent(spent),
            Some(CpuSample::Alive { spent: Some(_) }) => CpuEvidence::Flat,
        };
        let mut seen = self.seen.lock();
        if let EngineProbe::Progress(progress) = http {
            seen.anchor = Some(progress.fingerprint);
        }
        let verdict = match (cpu, http) {
            (CpuEvidence::Gone, _) => EngineProbe::Stopped,
            (CpuEvidence::Spent(_), EngineProbe::Busy) => EngineProbe::Progress(EngineProgress {
                // no answer yet: any stable token works, the wait compares it only to itself
                fingerprint: *seen.anchor.get_or_insert(0),
                any_processing: true,
            }),
            (CpuEvidence::Flat, EngineProbe::Busy) => EngineProbe::Stopped,
            (_, http) => http,
        };
        seen.cpu = cpu;
        seen.verdict = Some(verdict);
        verdict
    }

    /// The CPU the engine's process spent over the latest checkpoint (0 when flat), for the probes;
    /// `None` when nothing was read.
    pub(super) fn cpu_spent(&self) -> Option<std::time::Duration> {
        match self.seen.lock().cpu {
            CpuEvidence::Spent(spent) => Some(spent),
            CpuEvidence::Flat => Some(std::time::Duration::ZERO),
            CpuEvidence::Unread | CpuEvidence::Gone => None,
        }
    }

    /// A wait ended without a receipt: was it the switch's own bound while the process still spent
    /// CPU (`Some`, the CPU it spent), rather than the engine stopping, vanishing or being condemned?
    fn unresolved(&self) -> Option<std::time::Duration> {
        let seen = self.seen.lock();
        match (seen.cpu, seen.verdict) {
            (CpuEvidence::Spent(spent), Some(EngineProbe::Progress(_) | EngineProbe::Busy)) => Some(spent),
            _ => None,
        }
    }
}

/// A page switch's reply: status and body.
pub(super) type PageReply = Result<(u16, Option<serde_json::Value>), reqwest::Error>;

/// Work a wait gave up on without cancelling it.
pub(super) type Pending<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// How a bounded wait for a switch's receipt ended.
pub(super) enum Receipt<T> {
    /// The engine answered (or the transport failed) inside the wait.
    Answered(T),
    /// The engine stopped, vanished, stopped answering its socket, or is condemned.
    Stopped,
    /// Its bound passed while the engine's process kept spending CPU: neither a receipt nor a dead
    /// engine. The request is handed back still in flight, never cancelled under the engine.
    Unresolved { cpu_spent: std::time::Duration, pending: Pending<T> },
}

/// Await `work` while the engine progresses, by `/slots` when it answers and by its process's CPU
/// when it cannot. The seam [`bounded_page_receipt`] and the tests share.
pub(super) async fn await_receipt<T, P, PF>(
    mut work: Pending<T>,
    bounds: impl Into<WaitBounds>,
    liveness: &PageLiveness,
    mut http: P,
    on_busy: impl FnMut(u64),
) -> Receipt<T>
where
    P: FnMut() -> PF,
    PF: Future<Output = EngineProbe>,
{
    let answered = wait_while_engine_progresses(
        &mut work,
        bounds,
        || {
            let probe = http();
            async move { liveness.observe(probe.await) }
        },
        on_busy,
    )
    .await;
    match answered {
        Some(reply) => Receipt::Answered(reply),
        None => match liveness.unresolved() {
            Some(cpu_spent) => Receipt::Unresolved { cpu_spent, pending: work },
            None => Receipt::Stopped,
        },
    }
}

// Keep receipt consumption inside the watchdog's owned future. Tests use this
// same HTTP boundary with local progress evidence, without global serving state
// or the concurrently updated switch-cost distribution.
pub(super) async fn bounded_page_receipt<P, PF>(
    request: reqwest::RequestBuilder,
    bounds: impl Into<WaitBounds>,
    liveness: &PageLiveness,
    progress: P,
    on_busy: impl FnMut(u64),
) -> Receipt<PageReply>
where
    P: FnMut() -> PF,
    PF: Future<Output = EngineProbe>,
{
    let receipt: Pending<PageReply> = Box::pin(async move {
        let response = request.send().await?;
        let status = response.status().as_u16();
        let body = response.json::<serde_json::Value>().await.ok();
        Ok::<_, reqwest::Error>((status, body))
    });
    await_receipt(receipt, bounds, liveness, progress, on_busy).await
}

/// The endpoint owns an unresolved switch's request until its engine answers it: the IO was
/// dispatched and the engine is still working, so it is never cancelled under it. The answer is
/// recorded, and custody is NOT re-promoted from it: a later save of the same page may have
/// started since, so a late "saved" could name a file that is being rewritten.
pub(super) fn retain_unresolved(root: &str, action: &str, slot: u32, pending: Pending<PageReply>) {
    let started = std::time::Instant::now();
    let endpoint = root.to_owned();
    let action = action.to_owned();
    crate::inference::slots::directory().endpoint(root).retain_paging(async move {
        let status = pending.await.map_or(0, |(status, _)| status); // map_or: status zero records a transport failure, never success
        continuum_base::probe!(
            class = "inference.kv_page.unresolved_receipt",
            endpoint = %endpoint,
            action = %action,
            slot = slot as u64,
            status = status as u64,
            late_ms = started.elapsed().as_millis() as u64,
            "an unresolved page switch got its receipt: recorded; the page stays held as the unresolved switch left it"
        );
    });
}
