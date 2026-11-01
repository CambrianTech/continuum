use super::*;

impl std::str::FromStr for ServiceProfile {
    type Err = ServiceProfileParseError;
    /// Parse a comma-separated group list (`runtime-shell,grid-transport,cognition`).
    /// Empty / `all` / `full` → every group (the monolith default).
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty() || s.eq_ignore_ascii_case("all") || s.eq_ignore_ascii_case("full") {
            return Ok(ServiceProfile::all());
        }
        let groups = s
            .split(',')
            .map(|g| g.trim().parse::<ServiceGroup>())
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ServiceProfile::from_groups(groups))
    }
}

#[derive(Debug, thiserror::Error)]
#[error("unknown service group {0:?} — valid: runtime-shell, resource-gov, inference, cognition, forge, grid-transport, live (or 'all')")]
pub struct ServiceProfileParseError(pub String);

/// Every module name the substrate knows about — derived from
/// `MODULES`, not a parallel list. Used by the
/// `expected_modules_snapshot` integration test (A.2.2) as the
/// snapshot baseline.
pub fn all_known_modules() -> Vec<&'static str> {
    MODULES.iter().map(|spec| spec.name).collect()
}

// MODULES is the substrate's ONE source of truth — `required_modules`
// and `all_known_modules` both derive from it. No parallel list to
// drift against.
/// The runtime the SIGNAL handlers shut down. Installed once at wiring (after
/// modules register), read by main.rs's SIGTERM/SIGINT arms. Until 2026-09-02
/// those arms killed sentinels, slept a flat 2s, and `_exit`ed — the graceful
/// broadcast existed and NOTHING on the production stop path called it, so no
/// module ever saved on a real stop ("no lifecycle at all, random bullshit").
pub(super) static SIGNAL_RUNTIME: std::sync::OnceLock<Arc<Runtime>> = std::sync::OnceLock::new();

pub fn install_signal_shutdown(rt: Arc<Runtime>) {
    let _ = SIGNAL_RUNTIME.set(rt);
}

/// Run the save-and-join broadcast from a signal handler, bounded overall —
/// a stop must complete in seconds even if the runtime misbehaves.
/// What one module did when the node was told to stop.
///
/// Three phases, in the order the runtime broadcasts them, each with its own outcome —
/// because they fail differently and a caller that only learns "shutdown finished" cannot
/// tell a clean stop from one that abandoned a half-written turn.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/system/ModuleStopOutcome.ts"
)]
pub enum ModuleStopOutcome {
    /// Drained, saved and joined within the bound.
    Clean,
    /// `save_state` did not finish inside its bound. THE STATE IS UNKNOWN, not merely
    /// old: the write may have been half-applied. This is the case a caller must not
    /// report as success.
    SaveTimedOut,
    /// `save_state` returned an error and said why.
    SaveFailed { error: String },
    /// Saved, but `shutdown` did not return inside its bound. Its final flush is
    /// unconfirmed, so the receipt cannot claim durable state.
    JoinTimedOut,
    /// `shutdown` returned an error after a successful save.
    JoinFailed { error: String },
}

impl ModuleStopOutcome {
    /// Did EVERY phase this module ran complete? The only outcome that can support a
    /// durability claim.
    ///
    /// An earlier version answered `true` for `JoinTimedOut` and `JoinFailed`, on the
    /// reasoning that a module which saved and then failed to let go is untidy rather
    /// than lossy. **That was wrong, and the trait's own contract says so:** `shutdown`
    /// is documented as "release resources, FLUSH BUFFERS", and the logger's
    /// implementation is literally `flush_all`. A module whose join failed is a module
    /// whose final flush may not have happened, so its last writes are exactly as gone as
    /// a failed save's. Found by Astra, who read the contract I had written and I had not.
    ///
    /// The phase distinction is still carried in the variant, because it tells a reader
    /// WHERE the stop broke. It just cannot decide whether the state is on disk.
    pub fn completed(&self) -> bool {
        matches!(self, ModuleStopOutcome::Clean)
    }
}

/// What the DRAIN phase achieved, kept separate from the save/join outcome.
///
/// These were one enum and it lost information: an incomplete drain followed by a join
/// timeout produced `JoinTimedOut`, the drain result was discarded, and the module then
/// reported its state durable — a save taken mid-turn, described as clean. They are
/// orthogonal facts about the same module and neither may overwrite the other.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/system/DrainOutcome.ts"
)]
pub enum DrainOutcome {
    /// Nothing was in flight when the module stopped taking work.
    Drained,
    /// The bound expired with work outstanding, so whatever was saved next is a snapshot
    /// taken mid-turn.
    Incomplete { in_flight: u32 },
    /// `drain` itself errored or timed out. How much was in flight is UNKNOWN — which is
    /// why this is not `Incomplete { in_flight: 0 }`, a number nobody measured.
    Unknown { reason: String },
}

impl DrainOutcome {
    /// Did the module stop taking work with nothing outstanding? Anything else means the
    /// save that followed may be torn.
    pub fn is_quiet(&self) -> bool {
        matches!(self, DrainOutcome::Drained)
    }
}

/// One module's line in the receipt.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/system/ModuleStop.ts"
)]
pub struct ModuleStop {
    pub module: String,
    pub drain: DrainOutcome,
    pub outcome: ModuleStopOutcome,
    #[ts(type = "number")]
    pub ms: u64,
}

impl ModuleStop {
    /// Did this module's state reach disk INTACT? Both halves must hold: every phase
    /// completed, AND the module was quiet when its state was taken.
    ///
    /// A save that succeeded over a still-running turn wrote something — it just did not
    /// write a consistent something. A join that failed may have skipped the flush that
    /// puts the save on disk. Either way the answer is no, and reporting either as
    /// durable is the failure this whole receipt exists to prevent.
    pub fn state_is_durable(&self) -> bool {
        self.outcome.completed() && self.drain.is_quiet()
    }
}

/// WHAT ACTUALLY HAPPENED when the node stopped.
///
/// `Runtime::shutdown` used to return `()`. It logged non-ok modules to the core's own
/// log and told the caller nothing, so `continuum stop` printed a success line and exited
/// 0 whether every module had saved or none had. A stop whose result only exists in the
/// log of the process that just exited is not a result anybody can act on.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/system/ShutdownReceipt.ts"
)]
pub struct ShutdownReceipt {
    /// Background acceptance and terminal persistence must settle before modules stop.
    pub tasks: DrainOutcome,
    pub modules: Vec<ModuleStop>,
    #[ts(type = "number")]
    pub total_ms: u64,
}

impl ShutdownReceipt {
    /// Modules whose durable state did NOT reach disk intact. Empty is the only value
    /// that justifies reporting a clean stop.
    pub fn unsaved(&self) -> Vec<&ModuleStop> {
        self.modules
            .iter()
            .filter(|m| !m.state_is_durable())
            .collect()
    }

    /// Every module drained, saved, and completed its final shutdown flush.
    pub fn state_is_durable(&self) -> bool {
        self.tasks.is_quiet() && self.unsaved().is_empty()
    }

    /// One line a human can act on, without reading the process's log after it exited.
    pub fn summary(&self) -> String {
        if !self.tasks.is_quiet() {
            return format!(
                "module shutdown withheld: background task custody {:?}",
                self.tasks
            );
        }
        let unsaved = self.unsaved();
        if unsaved.is_empty() {
            return format!(
                "{} modules stopped, all state durable, {}ms",
                self.modules.len(),
                self.total_ms
            );
        }
        let names: Vec<String> = unsaved
            .iter()
            .map(|m| format!("{} (drain {:?}, save {:?})", m.module, m.drain, m.outcome))
            .collect();
        format!(
            "{} of {} modules did NOT save: {}",
            unsaved.len(),
            self.modules.len(),
            names.join(", ")
        )
    }
}

/// ONE shutdown, owned.
///
/// This was a bare `OnceLock` plus a free function, and the tests for it had to build
/// their own `watch` channel — so they asserted a property of `tokio::sync::watch` and
/// would have stayed green if the production publisher regressed from `send_replace` back
/// to `send`. Established by Astra reading the source, NOT by an executed mutation run —
/// nobody has yet watched that test go red, and the weaker claim is the true one.
///
/// As an instance, a test can construct the SAME owner over a real `Runtime` and drive the
/// real `begin` / publisher, instead of a look-alike. Same reason `AdmissionGate` is a
/// type: an invariant that can only be reached through a global is an invariant whose
/// tests drift into testing something adjacent.
pub(super) struct ShutdownOperation {
    pub(super) result: tokio::sync::watch::Sender<Option<ShutdownReceipt>>,
    pub(super) started: Arc<parking_lot::Mutex<bool>>,
}

impl ShutdownOperation {
    pub(super) fn new() -> Self {
        Self {
            result: tokio::sync::watch::channel(None).0,
            started: Arc::new(parking_lot::Mutex::new(false)),
        }
    }

    /// Start the broadcast over `rt` if it has not started, and return a view of the
    /// result. Idempotent: a second caller — a signal racing the stop verb, a retried
    /// request — joins the first broadcast rather than running `save_state` twice over
    /// the same state.
    /// A custody refusal stops no modules and permits an explicit new attempt;
    /// once module shutdown starts its receipt remains the single retained result.
    ///
    /// `settle` is how long the citizens' turns may still finish before the modules drain
    /// (card 32fa22ba): the turns' measured remaining time, never more than this. ZERO =
    /// none — the signal path, whose own deadline belongs to the supervisor. Only the
    /// FIRST caller's settle applies, as only its broadcast runs.
    pub(super) fn begin(
        &self,
        rt: Option<Arc<Runtime>>,
        settle: std::time::Duration,
    ) -> tokio::sync::watch::Receiver<Option<ShutdownReceipt>> {
        let mut running = self.started.lock();
        if !*running {
            *running = true;
            // A retry is permitted only after custody withheld every module stop.
            // Do not hand its caller the previous attempt's terminal receipt.
            self.result.send_replace(None);
            match rt {
                Some(rt) => {
                    let tx = self.result.clone();
                    let started = Arc::clone(&self.started);
                    // Detached ON PURPOSE: this is the task whose whole point is that no
                    // connection owns it. A socket handler is cancelled when its client
                    // goes away, and a shutdown cancelled after ingress closed leaves the
                    // node refusing work with nothing saved.
                    tokio::spawn(async move {
                        // In THIS task, never a connection's: the settle closes the turn
                        // door, and a door closed by a handler whose client went away
                        // would leave every citizen refusing work with no stop behind it.
                        if !settle.is_zero() {
                            crate::runtime::extensions::settle_before_stop(settle).await;
                        }
                        let receipt = rt.shutdown().await;
                        // send_replace, NEVER send: `send` returns Err and DROPS the value
                        // when the last receiver has gone — and the receiver that goes
                        // away is the disconnecting CLI, so `send` loses the terminal
                        // receipt in precisely the case this design exists for. A later
                        // subscriber would then wait forever on a `None` that never
                        // changes.
                        // Serialize publication with a new begin: a caller that
                        // observes the withheld receipt can immediately retry,
                        // without racing an old publisher over its new attempt.
                        let mut running = started.lock();
                        let withheld = !receipt.tasks.is_quiet();
                        let _ = tx.send_replace(Some(receipt));
                        if withheld {
                            *running = false;
                        }
                    });
                }
                None => {
                    // No runtime: nothing to save AND nothing that could have saved.
                    // Publishing an empty receipt says exactly that, rather than leaving
                    // every observer waiting on a broadcast that will never run.
                    let _ = self.result.send_replace(Some(ShutdownReceipt {
                        tasks: DrainOutcome::Drained,
                        modules: Vec::new(),
                        total_ms: 0,
                    }));
                }
            }
        }
        self.result.subscribe()
    }
}

impl Default for ShutdownOperation {
    fn default() -> Self {
        Self::new()
    }
}

/// Initialize the process owner on first use; observers receive an owned watch handle.
pub(super) static SHUTDOWN: std::sync::OnceLock<ShutdownOperation> = std::sync::OnceLock::new();

pub(super) fn process_shutdown() -> &'static ShutdownOperation {
    SHUTDOWN.get_or_init(ShutdownOperation::new)
}

/// START the node's shutdown if it has not started, and hand back a view of its result.
///
/// Delegates to the process's single [`ShutdownOperation`]. See that type for why the
/// operation is owned rather than free-standing.
pub fn begin_shutdown(
    settle: std::time::Duration,
) -> tokio::sync::watch::Receiver<Option<ShutdownReceipt>> {
    process_shutdown().begin(signal_runtime(), settle)
}

/// Observe the retained terminal receipt without starting a shutdown.
pub fn shutdown_receipt() -> Option<ShutdownReceipt> {
    process_shutdown().result.borrow().clone()
}

/// Wait for the shutdown to finish, up to `budget`.
///
/// `None` means it is still running — NOT that it failed and not that state is durable.
/// The distinction matters to the caller's exit code: a stop we stopped waiting for is
/// unknown, and unknown is not success.
pub async fn await_shutdown(
    mut rx: tokio::sync::watch::Receiver<Option<ShutdownReceipt>>,
    budget: std::time::Duration,
) -> Option<ShutdownReceipt> {
    if let Some(r) = rx.borrow_and_update().clone() {
        return Some(r);
    }
    let deadline = tokio::time::Instant::now() + budget;
    loop {
        match tokio::time::timeout_at(deadline, rx.changed()).await {
            Ok(Ok(())) => {
                if let Some(r) = rx.borrow_and_update().clone() {
                    return Some(r);
                }
                // A change that is still `None`: keep waiting.
            }
            // THE SENDER IS GONE. `changed()` returns `Err(RecvError)` immediately and
            // forever once the owner drops, so testing only for the TIMEOUT spun this loop
            // at full speed until the deadline — burning a core for the whole budget on
            // the one path where nothing can ever arrive. Found in source review by Astra.
            Ok(Err(_)) => return None,
            // Budget spent. The operation is still running, un-cancelled; unknown is not
            // failure and not success.
            Err(_) => return None,
        }
    }
}

/// The runtime the signal handlers stop, for callers that need the SAME one — the
/// resident `system/shutdown` verb runs the identical broadcast, so it must not reach
/// for a second registry. One runtime, one stop path, two triggers.
pub fn signal_runtime() -> Option<Arc<Runtime>> {
    SIGNAL_RUNTIME.get().cloned()
}

pub async fn run_signal_shutdown() {
    // Through the SAME idempotent operation the `system/shutdown` verb triggers, so a
    // signal arriving while the verb is running joins that broadcast instead of starting
    // a second one over the same state. The bound is deliberately larger than one phase:
    // three 2s phases run in parallel across modules, so a healthy stop is ~6s worst case
    // and a tighter cap here would cut off the save phase on any node slow enough to
    // need it.
    // No settle: a signal's deadline is the supervisor's (launchd's ExitTimeOut, a
    // console close), not ours to spend waiting on turns.
    let rx = begin_shutdown(std::time::Duration::ZERO);
    match await_shutdown(rx, std::time::Duration::from_secs(8)).await {
        Some(receipt) => {
            // A signal handler cannot return an exit code to anyone, so the receipt's only
            // readers are the log and the probe ledger — which is exactly why it is
            // written here rather than dropped. `_exit` follows immediately.
            if !receipt.state_is_durable() {
                eprintln!(
                    "[continuum-core] STOPPED WITH UNSAVED STATE: {}",
                    receipt.summary()
                );
            }
        }
        None => {
            eprintln!(
                "[continuum-core] shutdown did not finish within 8s — state durability is UNKNOWN"
            );
        }
    }
}
