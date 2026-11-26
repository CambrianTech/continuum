//! Control requests and the receipts that prove a stop. A request is not a stop: only a receipt
//! from the run's owner proves which stop happened.

use super::identity::RunId;

/// What a running stage may be asked to do at its next yield point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlRequest {
    Continue,
    Pause,
    Yield,
    Cancel,
}

/// Proof of a stop, owned by the task and runtime owner across a caller returning, a forgotten
/// handle and restart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReceipt {
    /// Quiescent; its continuation is retained; execution and residency both stay charged.
    Paused { run: RunId },
    /// Its continuation is retained; execution capacity was released; residency stays as named.
    Yielded {
        run: RunId,
        residency_kept: super::cost::ResidencyCost,
    },
    /// Terminal after quiescence; the effects it had already committed are recorded.
    Cancelled { run: RunId, committed_effects: u64 },
    /// The request could not be honoured as asked; never a silent drop presented as a stop.
    Refused { run: RunId, refusal: StopRefusal },
}

/// Why a stop request was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopRefusal {
    /// This stage, or the engine under it, cannot pause the same execution.
    PauseUnsupported,
    /// The run had already ended.
    AlreadyTerminal,
    /// The request came from a superseded incarnation.
    StaleIncarnation,
}
