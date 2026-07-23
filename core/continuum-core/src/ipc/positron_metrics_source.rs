//! System-metrics emitter — the SYS gauge's source (brick 2 of
//! POSITRON-WIDGET-SOPHISTICATION.md: the old sidebar's CPU/MEM sparkline,
//! define-once).
//!
//! Own task + `tokio::time::interval` + a store into the served [`Substrate`]
//! — the same emit shape as [`crate::ipc::vitals_emitter`]. It samples the ONE
//! shared [`SystemResourceMonitor`] the boot already owns (never a second
//! probe — CONCURRENCY-STYLE-GUIDE's reuse rule) inside `spawn_blocking`
//! (sysinfo refresh walks /proc / host_statistics — keep the runtime worker
//! threads clean), keeps a bounded ring of normalized samples, and stores a
//! [`SystemMetricsViewState`] under `kind="system-metrics"` each tick.
//!
//! Core-carried history (not client-accumulated): every surface — web
//! sparkline, terminal bars, a persona's `CPU 58% · MEM 25/32G` grounding —
//! renders the SAME series, and a reconnect resyncs the window instead of
//! starting a blank graph.

use std::sync::Arc;
use std::time::Duration;

use continuum_positron::system_metrics::{MetricSeriesView, SystemMetricsViewState};
use continuum_positron::{StateBuilder, Substrate};

use crate::system_resources::SystemResourceMonitor;

/// Sample cadence. The old sidebar graph breathed at seconds-scale; 2s matches
/// the vitals radiator so the rail animates in one rhythm.
const SAMPLE_INTERVAL: Duration = Duration::from_secs(2);

/// Ring length — 90 samples × 2s = a 3-minute window, the glanceable "is the
/// machine busy" horizon. Bounded so the envelope stays small (≈ two hundred
/// floats), per the perception resolution contract's byte-bound rule.
const WINDOW: usize = 90;

/// Format bytes as the legend's compact `25.3/32G` reading.
fn gb(used: u64, total: u64) -> String {
    const G: f64 = 1024.0 * 1024.0 * 1024.0;
    format!("{:.1}/{:.0}G", used as f64 / G, total as f64 / G)
}

/// Push onto a bounded ring (oldest dropped).
fn push_ring(ring: &mut Vec<f32>, v: f32) {
    if ring.len() == WINDOW {
        ring.remove(0);
    }
    ring.push(v);
}

/// Spawn the emitter: every [`SAMPLE_INTERVAL`] refresh the shared monitor off
/// the async threads, fold the reading into the rings, and store the view.
/// Runs for the process lifetime.
pub fn spawn_system_metrics_emitter(
    rt: &tokio::runtime::Handle,
    monitor: Arc<SystemResourceMonitor>,
    substrate: Substrate,
) {
    rt.spawn(async move {
        // Sole writer of the "system-metrics" kind → its own standalone
        // Revisions well (same discipline as chat / nav / kanban).
        let builder = StateBuilder::standalone();
        let mut ticker = tokio::time::interval(SAMPLE_INTERVAL);
        let mut cpu_ring: Vec<f32> = Vec::with_capacity(WINDOW);
        let mut mem_ring: Vec<f32> = Vec::with_capacity(WINDOW);
        loop {
            ticker.tick().await;
            // sysinfo refresh is a syscall walk — off the async workers, with
            // the standard probe timeout so a wedged sampler quarantines
            // itself instead of stalling the emitter forever.
            let m = Arc::clone(&monitor);
            let sampled = tokio::time::timeout(
                Duration::from_millis(500),
                tokio::task::spawn_blocking(move || m.refresh()),
            )
            .await;
            let snap = match sampled {
                Ok(Ok(snap)) => snap,
                Ok(Err(join_err)) => {
                    tracing::warn!("system-metrics sampler panicked: {join_err} — skipping tick");
                    continue;
                }
                Err(_elapsed) => {
                    tracing::warn!("system-metrics sample exceeded 500ms — skipping tick");
                    continue;
                }
            };
            let cpu_pct = (snap.cpu.global_usage * 100.0).clamp(0.0, 100.0);
            let mem_pct = (snap.memory.pressure * 100.0).clamp(0.0, 100.0);
            push_ring(&mut cpu_ring, cpu_pct);
            push_ring(&mut mem_ring, mem_pct);
            let view = SystemMetricsViewState {
                series: vec![
                    MetricSeriesView {
                        label: "cpu".into(),
                        points: cpu_ring.clone(),
                        current: format!("{cpu_pct:.0}%"),
                    },
                    MetricSeriesView {
                        label: "mem".into(),
                        points: mem_ring.clone(),
                        current: gb(snap.memory.used_bytes, snap.memory.total_bytes),
                    },
                    // GPU series joins here when the GpuMemoryManager exposes a
                    // public stats read — absent until then, never fabricated.
                ],
                sample_interval_ms: SAMPLE_INTERVAL.as_millis() as u64,
            };
            substrate.store(builder.session(view));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the ring is bounded at WINDOW (the envelope byte-bound
    // per the perception resolution contract) and drops oldest-first.
    #[test]
    fn ring_is_bounded_and_drops_oldest() {
        let mut ring = Vec::new();
        for i in 0..(WINDOW + 10) {
            push_ring(&mut ring, i as f32);
        }
        assert_eq!(ring.len(), WINDOW);
        assert_eq!(ring[0], 10.0, "oldest dropped");
        assert_eq!(*ring.last().unwrap(), (WINDOW + 9) as f32);
    }

    // what this catches: the legend string formats bytes as the compact G
    // reading both surfaces share ("25.3/32G") — source-formatted, no per-
    // renderer unit math to drift.
    #[test]
    fn legend_formats_bytes_compactly() {
        let g = 1024_u64 * 1024 * 1024;
        assert_eq!(gb(25_395_000_000, 32 * g), "23.7/32G");
        assert_eq!(gb(6 * g + g / 2, 25 * g), "6.5/25G");
    }
}
