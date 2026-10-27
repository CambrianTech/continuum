//! THE BOOT CLOCK — how long this core has been up, marked once by `main`.
//!
//! One process-wide instant so any seam can ask "are we still inside the boot window?"
//! (board projections rebuilding, seventeen minds reseating, the desktop dist building in
//! the background) without each inventing its own start time. `elapsed()` is `None`
//! when nothing marked it (tests, embedded uses) — an unknown boot is reported as
//! unknown, never as "long ago".

use std::sync::OnceLock;
use std::time::{Duration, Instant};

static BOOT: OnceLock<Instant> = OnceLock::new();

/// Mark the boot. Idempotent: the first call wins, later calls are no-ops.
pub fn mark() {
    let _ = BOOT.set(Instant::now());
}

/// Time since the boot mark, or `None` if this process never marked one.
pub fn elapsed() -> Option<Duration> {
    BOOT.get().map(|b| b.elapsed())
}
