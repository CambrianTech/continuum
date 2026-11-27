//! Her routing as she lived it (IN-PROCESS-LEARNING.md, "Routing is part of what was lived"; the fork's
//! llama-lived-routing.h). On a mixture-of-experts base a near-tie in a row's top-k picks a different
//! expert when the arithmetic differs slightly, so a lesson that routed again would train a different
//! function from the one she served: the 35B's tiled update against its one-graph step was cosine 0.37
//! routed and 0.9932 replaying her routing (the M5, CPU), 0.9073 and 0.9994 on the 5090 (CUDA),
//! 2026-11-27. A serving context records each row's experts as she serves; a live lesson replays its
//! span's, and the fork refuses a lesson on such a base while nothing is recorded.

use super::Context;
use crate::sys;

/// What a context records once capture is asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingCapture {
    /// A mixture-of-experts base: serving records, lessons replay.
    Recording,
    /// A dense base: nothing to record, and its lessons need none.
    DenseBase,
}

/// Why a mixture-of-experts context cannot record. Its lessons are refused by name until it can.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RoutingCaptureRefused {
    #[error("the base has more experts than the lived-routing table names (256)")]
    TooManyExperts,
    #[error("the lived-routing capture buffer could not be allocated")]
    NoRoom,
    #[error("the engine answered capture with a status this binding does not know ({0})")]
    Unknown(u32),
}

impl Context<'_> {
    /// Record this context's routing as it serves, so its lessons replay what she lived.
    pub fn record_routing(&mut self) -> Result<RoutingCapture, RoutingCaptureRefused> {
        // SAFETY: the context outlives this synchronous call, which only flips its own capture state.
        let status = unsafe { sys::llama_set_routing_capture(self.ptr.as_ptr(), true) };
        match status {
            sys::llama_routing_capture_LLAMA_ROUTING_CAPTURE_RECORDING => Ok(RoutingCapture::Recording),
            sys::llama_routing_capture_LLAMA_ROUTING_CAPTURE_NO_EXPERTS => Ok(RoutingCapture::DenseBase),
            sys::llama_routing_capture_LLAMA_ROUTING_CAPTURE_TOO_MANY => Err(RoutingCaptureRefused::TooManyExperts),
            sys::llama_routing_capture_LLAMA_ROUTING_CAPTURE_NO_ROOM => Err(RoutingCaptureRefused::NoRoom),
            other => Err(RoutingCaptureRefused::Unknown(other)),
        }
    }

    /// The bytes the lived-routing table holds: memory the context's owner accounts.
    pub fn lived_routing_bytes(&self) -> usize {
        // SAFETY: a read of the context's own table size.
        unsafe { sys::llama_lived_routing_bytes(self.ptr.as_ptr()) }
    }
}
