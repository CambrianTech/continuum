//! Module readiness: whether each module's `initialize` has returned, as one typed table the
//! dispatch paths, the tick loops, the stop broadcast and `health-check` all read.
//!
//! Pattern 2 of docs/architecture/DEGRADE-NEVER-REFUSE.md (fault domains): one module that never
//! finishes initializing costs its own commands, never the node. On the M5 (2026-11-07 19:11Z) a
//! relock in the serving reconcile blocked `initialize` forever; the core never bound its socket
//! and the 300 s boot deadline restarted it into the same wedge. Now the core binds first, each
//! module initializes on its own thread under an [`InitDeadline`], and a module past it is
//! [`ModuleState::Degraded`]: its commands are refused by type ([`ModuleUnavailable`]) while the
//! rest serve, and it becomes `Ready` the moment its late initialize returns.
//!
//! A module with no row has never been asked to initialize (a test runtime, or boot before
//! [`Runtime::begin_initialize`](super::Runtime::begin_initialize)); it is admitted, which is
//! exactly what dispatch did before this table existed.

use dashmap::DashMap;
use serde::Serialize;
use std::sync::atomic::{fence, AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::Notify;
use ts_rs::TS;

/// How long the runtime waits for a module's `initialize` before reporting it degraded, and
/// where that number came from. Waiting past it cancels nothing: the module keeps initializing
/// and is admitted when it returns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "basis", rename_all = "snake_case")]
#[ts(export, export_to = "../../../protocol/typescript/runtime/InitDeadline.ts")]
pub enum InitDeadline {
    /// RFC 6298's timeout over this node's boots: the smoothed slowest healthy init plus four
    /// times its mean deviation (the saved `module-init` record, folded by every boot, a late
    /// init included), so the bound follows the node both ways.
    Measured {
        #[ts(type = "number")]
        ms: u64,
        #[ts(type = "number")]
        smoothed_ms: u64,
        #[ts(type = "number")]
        deviation_ms: u64,
        slowest_module: String,
    },
    /// No record on this node yet (first boot on this home, or a record that did not match): the
    /// backstop the runtime used before the record existed. This boot's inits write the record,
    /// so the next boot is `Measured`.
    Unmeasured {
        #[ts(type = "number")]
        backstop_ms: u64,
    },
}

impl InitDeadline {
    /// The per-module backstop the runtime used before the record existed (60 s, set as a
    /// wedged-init bound that never false-positives a slow init). Only an unrecorded node uses it.
    pub const UNMEASURED_BACKSTOP: Duration = Duration::from_secs(60);

    /// The bound for a node with no record yet.
    pub fn unmeasured() -> Self {
        InitDeadline::Unmeasured { backstop_ms: Self::UNMEASURED_BACKSTOP.as_millis() as u64 }
    }

    /// A measured bound of exactly `ms`, for a test that needs a short deadline.
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn fixture(ms: u64, slowest_module: &str) -> Self {
        InitDeadline::Measured { ms, smoothed_ms: ms, deviation_ms: 0, slowest_module: slowest_module.to_string() }
    }

    pub fn duration(&self) -> Duration {
        match self {
            InitDeadline::Measured { ms, .. } => Duration::from_millis(*ms),
            InitDeadline::Unmeasured { backstop_ms } => Duration::from_millis(*backstop_ms),
        }
    }
}

impl std::fmt::Display for InitDeadline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InitDeadline::Measured { ms, smoothed_ms, deviation_ms, slowest_module } => write!(
                f,
                "{ms} ms (RFC 6298 over this node's boots: smoothed slowest healthy init \
                 {smoothed_ms} ms + 4 x deviation {deviation_ms} ms; last slowest {slowest_module})"
            ),
            InitDeadline::Unmeasured { backstop_ms } => {
                write!(f, "{backstop_ms} ms backstop: no init record on this node yet")
            }
        }
    }
}

/// Why a module is degraded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../../protocol/typescript/runtime/DegradedReason.ts")]
pub enum DegradedReason {
    /// `initialize` had not returned when the deadline passed. It is still running.
    InitDeadline { deadline: InitDeadline },
}

impl std::fmt::Display for DegradedReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DegradedReason::InitDeadline { deadline } => {
                write!(f, "initialize did not return within {deadline}")
            }
        }
    }
}

/// One module's initialization state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export, export_to = "../../../protocol/typescript/runtime/ModuleState.ts")]
pub enum ModuleState {
    /// `initialize` is running and its deadline has not passed. A command for it waits, bounded
    /// by the deadline.
    Initializing,
    /// `initialize` (and `load_state`) returned Ok.
    Ready {
        #[ts(type = "number")]
        init_ms: u64,
    },
    /// `initialize` returned an error, or its thread panicked. Dispatch admits it as before:
    /// the module answers its own commands with what it can do.
    Failed {
        #[ts(type = "number")]
        init_ms: u64,
        error: String,
    },
    /// `initialize` is still running past its deadline. Its commands are refused by type.
    Degraded { reason: DegradedReason },
}

impl ModuleState {
    /// Did `initialize` return (whatever it returned)?
    pub fn initialize_returned(&self) -> bool {
        matches!(self, ModuleState::Ready { .. } | ModuleState::Failed { .. })
    }
}

/// One row of the readiness report (`health-check`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/runtime/ModuleReadinessRow.ts")]
pub struct ModuleReadinessRow {
    pub module: String,
    pub state: ModuleState,
}

/// A command refused because the module that owns it cannot take it yet.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ModuleUnavailable {
    #[error(
        "module '{module}' is degraded: {reason}. Its commands are refused until its initialize \
         returns; the rest of the core is serving. Next: retry later, or run `health-check` to \
         read each module's state"
    )]
    Degraded { module: String, reason: DegradedReason },
}

/// The table. One per registry, shared by `Arc`.
#[derive(Default)]
pub struct ModuleReadiness {
    states: DashMap<&'static str, ModuleState>,
    /// Woken on every state change; a command waiting on an initializing module re-reads.
    changed: Notify,
    /// Set once the boot has started tick loops; a module that becomes ready later starts its own.
    ticks_open: AtomicBool,
    /// Modules whose tick loop has been spawned, so the boot pass and a late init never both do.
    ticking: DashMap<&'static str, ()>,
}

impl ModuleReadiness {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn set(&self, module: &'static str, state: ModuleState) {
        self.states.insert(module, state);
        // Pairs with the fence in `open_ticks`: whichever of "ready" and "ticks open" is
        // published second sees the other, so a late module's tick loop is never missed.
        fence(Ordering::SeqCst);
        self.changed.notify_waiters();
    }

    pub fn state(&self, module: &str) -> Option<ModuleState> {
        self.states.get(module).map(|s| s.value().clone())
    }

    /// Would a command for `module` dispatch right now without waiting?
    pub fn admits_now(&self, module: &str) -> bool {
        self.state(module).is_none_or(|s| s.initialize_returned())
    }

    /// Admit a command for `module`: at once when its initialize returned (or it was never
    /// asked to initialize); after waiting while it is initializing, which the deadline bounds
    /// because the runtime moves every module out of `Initializing` by then; refused by type
    /// when it is degraded.
    pub async fn admit(&self, module: &str) -> Result<(), ModuleUnavailable> {
        loop {
            // Register for the wake BEFORE reading, so a change between the read and the await
            // is not lost.
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            match self.state(module) {
                None | Some(ModuleState::Ready { .. }) | Some(ModuleState::Failed { .. }) => {
                    return Ok(())
                }
                Some(ModuleState::Degraded { reason }) => {
                    return Err(ModuleUnavailable::Degraded { module: module.to_string(), reason })
                }
                Some(ModuleState::Initializing) => changed.await,
            }
        }
    }

    /// Every module's state, by name.
    pub fn report(&self) -> Vec<ModuleReadinessRow> {
        let mut rows: Vec<ModuleReadinessRow> = self
            .states
            .iter()
            .map(|e| ModuleReadinessRow { module: e.key().to_string(), state: e.value().clone() })
            .collect();
        rows.sort_by(|a, b| a.module.cmp(&b.module));
        rows
    }

    pub(crate) fn open_ticks(&self) {
        self.ticks_open.store(true, Ordering::SeqCst);
        fence(Ordering::SeqCst);
    }

    pub(crate) fn ticks_open(&self) -> bool {
        self.ticks_open.load(Ordering::SeqCst)
    }

    /// True the first time it is asked for `module`: the caller spawns its tick loop.
    pub(crate) fn claim_tick(&self, module: &'static str) -> bool {
        self.ticking.insert(module, ()).is_none()
    }
}
