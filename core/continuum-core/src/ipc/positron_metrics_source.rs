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

use crate::system_resources::{SystemResourceMonitor, SystemResourceSnapshot};

/// Sample cadence. The old sidebar graph breathed at seconds-scale; 2s matches
/// the vitals radiator so the rail animates in one rhythm.
const SAMPLE_INTERVAL: Duration = Duration::from_secs(2);

/// Ring length — 90 samples × 2s = a 3-minute window, the glanceable "is the
/// machine busy" horizon. Bounded so the envelope stays small (≈ two hundred
/// floats), per the perception resolution contract's byte-bound rule.
// context-budget-exempt: a chart ring-buffer length (samples retained for the metrics widget), not a context bound
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

/// Fold one sampled snapshot into the rings and produce this tick's series —
/// the pure heart of the emitter (testable without a monitor or a substrate).
///
/// CPU and MEM are always present. The GPU series joins ONLY when the shared
/// monitor carried a live [`GpuSnapshot`](crate::gpu::monitor::GpuSnapshot)
/// this tick (device present, probe healthy, `total_bytes` known): points are
/// the VRAM-used percentage (device-wide `total - free`, the same system-wide
/// framing as mem), current is the same compact `6.5/25G` legend reading via
/// [`gb`]. No GPU / failed probe / zero-total → NO series, honest absence —
/// never a fabricated flatline ([[fallbacks-are-illegal-fail-loud]]).
fn fold_sample(
    snap: &SystemResourceSnapshot,
    cpu_ring: &mut Vec<f32>,
    mem_ring: &mut Vec<f32>,
    gpu_ring: &mut Vec<f32>,
) -> Vec<MetricSeriesView> {
    let cpu_pct = (snap.cpu.global_usage * 100.0).clamp(0.0, 100.0);
    let mem_pct = (snap.memory.pressure * 100.0).clamp(0.0, 100.0);
    push_ring(cpu_ring, cpu_pct);
    push_ring(mem_ring, mem_pct);
    let mut series = vec![
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
    ];
    // Needs BOTH a total and an actual free reading. With no reading the series is
    // omitted rather than drawn — `total - 0` would paint a full bar and read as
    // "the GPU is saturated", which is the opposite of "we don't know yet".
    if let Some((gpu, free)) = snap
        .gpu
        .as_ref()
        .filter(|g| g.total_bytes > 0)
        .and_then(|g| g.free_bytes.map(|f| (g, f)))
    {
        let used = gpu.total_bytes.saturating_sub(free);
        let gpu_pct = ((used as f64 / gpu.total_bytes as f64) * 100.0).clamp(0.0, 100.0) as f32;
        push_ring(gpu_ring, gpu_pct);
        series.push(MetricSeriesView {
            label: "gpu".into(),
            points: gpu_ring.clone(),
            current: gb(used, gpu.total_bytes),
        });
    }
    series
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
        // The node's identity line for the "nodes online" strip — read once
        // (it can't change under a running process); `None` when the OS
        // reports none, honest unknown ([[fallbacks-are-illegal-fail-loud]]).
        let node = sysinfo::System::host_name();
        let mut ticker = tokio::time::interval(SAMPLE_INTERVAL);
        let mut cpu_ring: Vec<f32> = Vec::with_capacity(WINDOW);
        let mut mem_ring: Vec<f32> = Vec::with_capacity(WINDOW);
        let mut gpu_ring: Vec<f32> = Vec::with_capacity(WINDOW);
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
            let view = SystemMetricsViewState {
                series: fold_sample(&snap, &mut cpu_ring, &mut mem_ring, &mut gpu_ring),
                sample_interval_ms: SAMPLE_INTERVAL.as_millis() as u64,
                node: node.clone(),
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

    fn snapshot(gpu: Option<crate::gpu::monitor::GpuSnapshot>) -> SystemResourceSnapshot {
        let g = 1024_u64 * 1024 * 1024;
        SystemResourceSnapshot {
            cpu: crate::system_resources::CpuStats {
                physical_cores: 8,
                logical_cores: 8,
                global_usage: 0.5,
                per_core_usage: vec![0.5; 8],
                brand: "TestChip".into(),
            },
            memory: crate::system_resources::MemoryStats {
                total_bytes: 32 * g,
                used_bytes: 16 * g,
                available_bytes: 16 * g,
                pressure: 0.5,
                swap_total_bytes: 0,
                swap_used_bytes: 0,
            },
            gpu,
            processes: None,
            timestamp_ms: 0,
            uptime_seconds: 0,
        }
    }

    // what this catches: the GPU series' honest-absence contract. Absent
    // `snapshot.gpu` (no device / failed probe) → the fold emits ONLY cpu+mem,
    // never a fabricated flatline; present gpu → a third "gpu" series whose
    // points are the VRAM-used percentage and whose current is the same
    // compact `6.5/25G` legend reading cpu+mem share. Regression for the
    // brick-2 seam ("absent until then, never fabricated").
    #[test]
    fn gpu_series_present_only_with_live_gpu_snapshot() {
        let g = 1024_u64 * 1024 * 1024;
        let (mut cpu, mut mem, mut gpu) = (Vec::new(), Vec::new(), Vec::new());

        // No GPU → exactly cpu+mem, and the gpu ring stays untouched.
        let series = fold_sample(&snapshot(None), &mut cpu, &mut mem, &mut gpu);
        assert_eq!(
            series.iter().map(|s| s.label.as_str()).collect::<Vec<_>>(),
            ["cpu", "mem"],
            "absent gpu must contribute NO series"
        );
        assert!(gpu.is_empty(), "no fabricated gpu points");

        // Live GPU (25G total, 6.5G used) → third series, pct points + compact current.
        let live = crate::gpu::monitor::GpuSnapshot {
            platform: "metal".into(),
            device_name: "TestGPU".into(),
            total_bytes: 25 * g,
            free_bytes: Some(25 * g - (6 * g + g / 2)),
            process_bytes: 0,
            utilization: 0.4,
            temperature_c: None,
            power_watts: None,
            pressure: 0.26,
        };
        let series = fold_sample(&snapshot(Some(live)), &mut cpu, &mut mem, &mut gpu);
        assert_eq!(series.len(), 3);
        let gpu_series = &series[2];
        assert_eq!(gpu_series.label, "gpu");
        assert_eq!(gpu_series.current, "6.5/25G");
        assert_eq!(gpu_series.points.len(), 1);
        assert!(
            (gpu_series.points[0] - 26.0).abs() < 0.1,
            "points are VRAM-used percent, got {}",
            gpu_series.points[0]
        );
    }
}
