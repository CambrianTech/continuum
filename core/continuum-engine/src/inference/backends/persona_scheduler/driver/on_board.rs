//! What the Core holds, on the resource board (UNIFIED-PAGING.md, slice 3b, first cut).
//!
//! The 5090 on 2026-11-26 (Codex, candidate 4c3e3eb54): the core held 30.7 GB of the card, and the
//! board could name 633 MB of it (the embedder's residency) and counted the rest as external. A
//! learner's seat waited for room 27 times in 900 s and nothing could ever free it, because no
//! owner of those bytes was known. Here the Core says what it holds, read from its context per
//! allocator (`llama::Context::held`), as its residency in the pool each buffer lives in: weights,
//! KV and recurrent state, the compute workspace serving's graphs and a reply's share, the output
//! buffer. It is measured, never estimated, and re-read where it can change (the start, and every
//! learning job, which grows the workspace or adds a seat's buffers).
//!
//! This makes the bytes owned, not free: the board's available is the same. What the Core can give
//! back (a resting mind's KV, workspace past need) is the next cut, the Core as a consumer of the
//! Placer.

use std::sync::Arc;

use continuum_persona_llama::UnifiedTier;
use llama::{BufferHeld, Held};

use crate::resources::{ResidencyGuard, ResidentFloor, ResourceDaemon, ResourceKind};

/// The Core's identity on the resource board.
pub const CORE_CONSUMER_ID: &str = "core";

/// One reading, summed per pool and by what the bytes are.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Pool {
    model: u64,
    context: u64,
    compute: u64,
    output: u64,
}

impl Pool {
    fn add(&mut self, buffer: &BufferHeld) {
        self.model = self.model.saturating_add(buffer.model);
        self.context = self.context.saturating_add(buffer.context);
        self.compute = self.compute.saturating_add(buffer.compute);
        self.output = self.output.saturating_add(buffer.output);
    }

    fn total(&self) -> u64 {
        self.model.saturating_add(self.context).saturating_add(self.compute).saturating_add(self.output)
    }
}

/// The Core's residency on its process's resource authority (none on a node without one).
pub(super) struct CoreHeld {
    daemon: Option<Arc<ResourceDaemon>>,
    reading: Held,
    device: Pool,
    host: Pool,
    claim: Option<ResidencyGuard<2>>,
}

impl CoreHeld {
    pub(super) fn new() -> Self {
        Self { daemon: ResourceDaemon::global(), reading: Held::default(), device: Pool::default(), host: Pool::default(), claim: None }
    }

    /// Read what the context holds now and, if it changed, hold exactly that. A context that cannot
    /// account keeps the last residency: an old real number keeps the board conservative, where
    /// dropping it would count the Core's bytes as free.
    pub(super) fn read(&mut self, tier: &UnifiedTier<'_, '_>) {
        let Some(daemon) = &self.daemon else { return };
        if let Err(unaccounted) = tier.held(&mut self.reading) {
            continuum_base::probe!(class = "core.held.unaccounted", error = %unaccounted, "the Core could not read what its context holds: its last residency stands");
            return;
        }
        let (mut device, mut host) = (Pool::default(), Pool::default());
        for buffer in self.reading.buffers() {
            if buffer.buffer.is_host() { host.add(&buffer) } else { device.add(&buffer) }
        }
        if (device, host) == (self.device, self.host) {
            return;
        }
        // the new residency is held before the old is let go: for that instant the board counts both,
        // never neither, so no waiter is granted bytes the Core still holds
        // its weights are fixed while it serves; its KV, workspace and output are what the law may demote
        let claim = daemon.hold_residency(CORE_CONSUMER_ID, [
            ResidentFloor { kind: ResourceKind::Vram, resident: device.total(), headroom: 0, fixed: device.model },
            ResidentFloor { kind: ResourceKind::Ram, resident: host.total(), headroom: 0, fixed: host.model },
        ]);
        self.claim = Some(claim);
        (self.device, self.host) = (device, host);
        continuum_base::probe!(class = "core.held", consumer = CORE_CONSUMER_ID,
            device_model = device.model, device_context = device.context, device_compute = device.compute, device_output = device.output,
            host_model = host.model, host_context = host.context, host_compute = host.compute, host_output = host.output,
            "the Core holds this, measured from its context per pool");
    }
}
