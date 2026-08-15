//! Verifying that the lane's ACTUAL allocation matches the governor's placement.
//!
//! # The failure class (Joel, 2026-08-15)
//!
//! > "You ignore and miss major throughput issues including models that got fucked by
//! > serving and are on cpu. You never catch it and we are waiting an eternity."
//!
//! The governor plans a placement ([`super::llama_server::LanePlacement`]) and the spawn
//! passes flags — but llama.cpp decides the real allocation at load time, and it can land
//! somewhere else entirely (Metal init failure, VRAM exhaustion at map time, a build
//! without GPU kernels). When that happens the lane still answers `/health`, still
//! decodes, still reads `ready` — at a tenth of the planned speed, silently, for hours.
//! A lease nobody reads back is a suggestion: this module is the READBACK half of the
//! placement contract. Plan → actuate → **verify the engine's own account of what it
//! allocated** → mismatch is a lease violation, surfaced loud.
//!
//! # The signal
//!
//! llama.cpp's load banner states the allocation in one line:
//!
//! ```text
//! load_tensors: offloaded 63/63 layers to GPU
//! ```
//!
//! `offloaded 0/63` on a GPU-placement lane is not "slow" — it is the whole model on CPU,
//! the exact eternity-class failure quoted above. The stderr pump already reads every
//! line the engine emits ([`super::child_log`]), so the receipt costs one comparison per
//! line: no new probe, no new tick, no second connection to the engine.
//!
//! # What it does NOT do
//!
//! It does not kill anything and it does not decide policy. It records the engine's own
//! report into a shared cell; the spawn path (which knows the PLANNED placement) compares
//! and raises loud. Lane lifecycle stays with exactly one owner — the same doctrine as
//! [`super::wedge`].

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use super::child_log::LineWatch;

/// The engine's own report of how many layers it offloaded to GPU: `(offloaded, total)`.
///
/// Packed into one `AtomicU64` (`offloaded << 32 | total`) so readers get a coherent pair
/// without a lock — the pump thread writes once at load, everything after is reads.
/// `None` until the load banner has been observed (a lane mid-load has no report yet,
/// which is a different fact from "reported 0").
#[derive(Clone, Default)]
pub struct OffloadReport(Arc<AtomicU64>);

/// Sentinel for "no banner observed yet" — a real report always has `total > 0`.
const UNREPORTED: u64 = 0;

impl OffloadReport {
    pub fn new() -> Self {
        Self::default()
    }

    /// The engine's `(offloaded, total)` layer counts, once its load banner has printed.
    pub fn get(&self) -> Option<(u32, u32)> {
        let v = self.0.load(Ordering::Relaxed);
        if v == UNREPORTED {
            return None;
        }
        Some(((v >> 32) as u32, v as u32))
    }

    fn set(&self, offloaded: u32, total: u32) {
        self.0
            .store(((offloaded as u64) << 32) | total as u64, Ordering::Relaxed);
    }
}

/// The [`LineWatch`] that records the engine's offload banner into an [`OffloadReport`].
///
/// Cheap per line (a substring probe before any parsing) and never blocks — the pump's
/// contract. Repeated banners (a `-ot` split prints per-buffer lines on some builds)
/// keep the LAST parsed values; the final banner is the settled allocation.
pub struct OffloadWatch {
    report: OffloadReport,
}

impl OffloadWatch {
    pub fn new(report: OffloadReport) -> Self {
        Self { report }
    }
}

impl LineWatch for OffloadWatch {
    fn observe(&mut self, line: &str) {
        if let Some((offloaded, total)) = parse_offload_banner(line) {
            self.report.set(offloaded, total);
        }
    }
}

/// Chain two watches over one pump — the stderr sink asks BOTH questions (wedge +
/// placement) of every line without a second reader. Each stays a single-question
/// observer per the [`LineWatch`] doctrine; this is composition, not a mega-watcher.
pub struct ChainWatch(pub Box<dyn LineWatch>, pub Box<dyn LineWatch>);

impl LineWatch for ChainWatch {
    fn observe(&mut self, line: &str) {
        self.0.observe(line);
        self.1.observe(line);
    }
}

/// Parse llama.cpp's offload banner: `... offloaded X/Y layers to GPU`.
///
/// Pure (str in, pair out) so the shape is pinned by tests without a server. Tolerant of
/// the prefix (`load_tensors:` vs `llm_load_tensors:` has drifted across llama.cpp
/// versions) but strict about the phrase itself — a line mentioning "offloaded" in some
/// other grammar parses `None` rather than guessing.
pub fn parse_offload_banner(line: &str) -> Option<(u32, u32)> {
    let idx = line.find("offloaded ")?;
    let rest = &line[idx + "offloaded ".len()..];
    let (frac, tail) = rest.split_once(' ')?;
    if !tail.trim_start().starts_with("layers to GPU") {
        return None;
    }
    let (x, y) = frac.split_once('/')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the banner parse must match llama.cpp's real load line in both
    // prefix spellings, reject lookalike grammar, and round-trip through the shared cell
    // — this is the placement contract's READBACK, and a silent parse miss here means a
    // CPU-fallback lane reads as "unreported" forever (the eternity-class failure this
    // module exists to catch, Joel 2026-08-15).
    #[test]
    fn offload_banner_parses_and_reports() {
        assert_eq!(
            parse_offload_banner("load_tensors: offloaded 63/63 layers to GPU"),
            Some((63, 63))
        );
        assert_eq!(
            parse_offload_banner("llm_load_tensors: offloaded 0/33 layers to GPU"),
            Some((0, 33))
        );
        // Lookalike grammar must not parse — no guessing.
        assert_eq!(parse_offload_banner("offloaded all layers to GPU"), None);
        assert_eq!(parse_offload_banner("slot update: task offloaded 3/4"), None);

        let report = OffloadReport::new();
        assert_eq!(report.get(), None, "no banner yet = no report, not (0,0)");
        let mut watch = OffloadWatch::new(report.clone());
        watch.observe("noise line");
        watch.observe("load_tensors: offloaded 0/48 layers to GPU");
        assert_eq!(report.get(), Some((0, 48)));
    }
}
