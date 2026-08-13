//! `NvidiaMonitor` — `GpuMonitor` impl for NVIDIA hosts (CUDA and
//! NVIDIA-on-Vulkan).
//!
//! This is the live-scanning sibling of `detect_cuda()` in
//! `memory_manager.rs`: where that captures a STATIC total at boot, this
//! re-samples free / utilization / thermals every tick — the signal that
//! lets the resource governor SEE a game or a second process grabbing our
//! VRAM headroom (the whole point of the net-of-external ceiling in
//! `GpuCapacitySource`).
//!
//! ## Why subprocess (`nvidia-smi`), not NVML FFI
//!
//! The crate deliberately has no `nvml-wrapper` / `cudarc` dependency —
//! `detect_cuda()` already shells out to `nvidia-smi`, and the live
//! monitor stays on the SAME mechanism so there is one NVIDIA code path,
//! no new untestable FFI surface, and parity with the detection that
//! decided this host was CUDA in the first place. `nvidia-smi` at ~1Hz is
//! more expensive per call than NVML's in-process query, but the cost is a
//! short-lived child process awaited off the reactor (`tokio::process`),
//! never a blocked thread, and the governor's cadence is seconds, not
//! milliseconds. If a future slice wants 10Hz cheap sampling, NVML behind
//! the same trait is the drop-in — the policy never sees the difference.
//!
//! ## Signals
//!
//! - `total_bytes` → `memory.total` (confirmed live each tick; NVIDIA VRAM
//!   total is fixed, but reading it costs nothing extra in the same query
//!   and guards against a driver reporting a changed budget).
//! - `free_bytes` → `memory.free`. The load-bearing signal: system-wide
//!   free VRAM on the device, so another process eating headroom shows up.
//! - `process_bytes` → summed `used_memory` of compute-apps whose `pid`
//!   matches THIS process. Distinguishes "we are tight" from "the box is
//!   tight" so the governor can spill our own slots vs back off globally.
//! - `utilization` / `temperature_c` / `power_watts` → the matching
//!   `nvidia-smi` query columns, parsed defensively ("[N/A]" → None/0.0 so
//!   a GPU that doesn't expose a sensor never poisons the pressure signal).
//!
//! There is deliberately NO fabricated fallback: if `nvidia-smi` is absent
//! or unparseable at construction, `new()` returns `None` and the caller
//! fails loud by name — it must never substitute a stale or invented
//! number (that stale-`free`-that-never-drops bug is exactly what the whole
//! live-monitor layer exists to kill).

use crate::gpu::monitor::GpuMonitor;
use crate::runtime::{spawn_daemon, Daemon, DaemonChannel};
use async_trait::async_trait;
use std::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::watch;
use tokio::time::Duration;

/// Tick cadence for the background sampler. 1Hz keeps parity with
/// Activity-Monitor / `nvidia-smi -l 1` and is the practical floor for a
/// subprocess query — faster ticks just multiply child-process churn for
/// no extra signal (the driver's counters don't update meaningfully
/// faster than this under normal load).
const TICK_INTERVAL: Duration = Duration::from_secs(1);

/// One sampled reading of the `--query-gpu` columns. Parsed from a single
/// `nvidia-smi` CSV line; `None` fields mean the GPU didn't expose that
/// sensor (e.g. datacenter cards without a power cap report `[N/A]`).
struct GpuSample {
    free_bytes: u64,
    total_bytes: u64,
    utilization: f32,
    temperature_c: Option<f32>,
    power_watts: Option<f32>,
}

pub struct NvidiaMonitor {
    device_name: String,
    total_bytes: u64,
    free_bytes: AtomicU64,
    process_bytes: AtomicU64,
    /// Utilization stored as parts-per-thousand (0..1000) so it fits an
    /// atomic; `utilization()` divides back to 0.0..1.0.
    utilization_x1000: AtomicU32,
    /// Temperature in milli-Celsius, or `i32::MIN` sentinel for "no sensor".
    temperature_mc: AtomicI32,
    /// Power in milli-watts, or `i32::MIN` sentinel for "no sensor".
    power_mw: AtomicI32,
    /// Embedded publish channel carrying derived pressure (`1 - free/total`).
    /// Ungated — the monitor reports a continuous signal; the governor /
    /// pressure-broker decides what level warrants backoff.
    channel: DaemonChannel<f32>,
}

/// Sentinel stored in the temperature / power atomics meaning "the GPU did
/// not report this sensor on the last tick". Distinct from a real 0.
const NO_SENSOR: i32 = i32::MIN;

impl NvidiaMonitor {
    /// Construct an `NvidiaMonitor` and spawn it on the shared `Daemon`
    /// runner. Returns `None` when `nvidia-smi` is absent or its output
    /// doesn't parse — i.e. this is not an NVIDIA host (or the tool isn't
    /// installed). `None` is a fail-loud capability gap for the caller, NOT
    /// a cue to substitute a CPU monitor or a fabricated number.
    ///
    /// Performs one synchronous probe at construction (so a non-NVIDIA host
    /// returns `None` immediately rather than spawning a daemon that can
    /// never sample); the per-tick refresh thereafter is fully async.
    pub fn new() -> Option<Arc<Self>> {
        let (name, sample) = probe_blocking()?;

        let monitor = Arc::new(Self {
            device_name: name,
            total_bytes: sample.total_bytes,
            free_bytes: AtomicU64::new(sample.free_bytes),
            process_bytes: AtomicU64::new(0),
            utilization_x1000: AtomicU32::new((sample.utilization * 1000.0) as u32),
            temperature_mc: AtomicI32::new(to_milli(sample.temperature_c)),
            power_mw: AtomicI32::new(to_milli(sample.power_watts)),
            channel: DaemonChannel::ungated(derive_pressure(sample.free_bytes, sample.total_bytes)),
        });

        // The shared runner owns the interval + per-tick catch_unwind: a
        // panic in parsing loses one tick and resumes against the last-good
        // snapshot rather than killing GPU monitoring for the whole process.
        let _ = spawn_daemon(monitor.clone());
        Some(monitor)
    }
}

#[async_trait]
impl Daemon for NvidiaMonitor {
    type Snapshot = f32;

    fn name(&self) -> &'static str {
        "nvidia-gpu"
    }

    fn cadence(&self) -> Duration {
        TICK_INTERVAL
    }

    fn channel(&self) -> &DaemonChannel<f32> {
        &self.channel
    }

    /// Refresh free / process / util / thermals from `nvidia-smi` and
    /// publish derived pressure. The two child processes are awaited off
    /// the reactor (`tokio::process`), so the tick never blocks a runtime
    /// thread; a transient `nvidia-smi` hiccup leaves the last-good atomics
    /// in place (we only store fields we successfully parsed).
    async fn tick(&self) {
        if let Some(sample) = query_gpu().await {
            self.free_bytes.store(sample.free_bytes, Ordering::Relaxed);
            self.utilization_x1000
                .store((sample.utilization * 1000.0) as u32, Ordering::Relaxed);
            self.temperature_mc
                .store(to_milli(sample.temperature_c), Ordering::Relaxed);
            self.power_mw
                .store(to_milli(sample.power_watts), Ordering::Relaxed);
            self.channel
                .publish(derive_pressure(sample.free_bytes, self.total_bytes));
        }
        if let Some(proc_bytes) = query_process_bytes().await {
            self.process_bytes.store(proc_bytes, Ordering::Relaxed);
        }
    }
}

impl GpuMonitor for NvidiaMonitor {
    fn platform(&self) -> &'static str {
        "cuda"
    }
    fn device_name(&self) -> &str {
        &self.device_name
    }
    fn total_bytes(&self) -> u64 {
        self.total_bytes
    }
    fn free_bytes(&self) -> u64 {
        self.free_bytes.load(Ordering::Relaxed)
    }
    fn process_bytes(&self) -> u64 {
        self.process_bytes.load(Ordering::Relaxed)
    }
    fn utilization(&self) -> f32 {
        self.utilization_x1000.load(Ordering::Relaxed) as f32 / 1000.0
    }
    fn temperature_c(&self) -> Option<f32> {
        from_milli(self.temperature_mc.load(Ordering::Relaxed))
    }
    fn power_watts(&self) -> Option<f32> {
        from_milli(self.power_mw.load(Ordering::Relaxed))
    }
    fn pressure_rx(&self) -> watch::Receiver<f32> {
        self.channel.handle().subscribe()
    }
}

// ─── pure helpers (parse + derive) — unit-tested without a GPU ──────────

/// Derive pressure `1 - free/total`, clamped to [0,1]. `total == 0` (no
/// device / parse failure) yields 0.0 rather than a divide-by-zero.
fn derive_pressure(free: u64, total: u64) -> f32 {
    if total == 0 {
        return 0.0;
    }
    (1.0 - (free as f32 / total as f32)).clamp(0.0, 1.0)
}

/// `Option<f32>` → milli-units in an `i32` atomic; `None` → `NO_SENSOR`.
fn to_milli(v: Option<f32>) -> i32 {
    match v {
        Some(x) => (x * 1000.0) as i32,
        None => NO_SENSOR,
    }
}

/// Inverse of [`to_milli`]: the `NO_SENSOR` sentinel decodes to `None`.
fn from_milli(mc: i32) -> Option<f32> {
    if mc == NO_SENSOR {
        None
    } else {
        Some(mc as f32 / 1000.0)
    }
}

/// Parse one `--query-gpu` CSV line:
/// `memory.free, memory.total, utilization.gpu, temperature.gpu, power.draw`
/// in `csv,noheader,nounits` form (MiB, MiB, %, °C, W). Non-numeric cells
/// (`[N/A]`, `[Not Supported]`) parse to `None` for the optional sensors;
/// the two memory columns are required (return `None` for the whole line if
/// either is missing — a reading with no memory total is useless).
fn parse_gpu_csv_line(line: &str) -> Option<GpuSample> {
    let cols: Vec<&str> = line.split(',').map(|c| c.trim()).collect();
    if cols.len() < 5 {
        return None;
    }
    let mib_to_bytes = |s: &str| -> Option<u64> { s.parse::<u64>().ok().map(|m| m * 1024 * 1024) };
    let free_bytes = mib_to_bytes(cols[0])?;
    let total_bytes = mib_to_bytes(cols[1])?;
    let utilization = cols[2]
        .parse::<f32>()
        .ok()
        .map(|p| p / 100.0)
        .unwrap_or(0.0);
    let temperature_c = cols[3].parse::<f32>().ok();
    let power_watts = cols[4].parse::<f32>().ok();
    Some(GpuSample {
        free_bytes,
        total_bytes,
        utilization,
        temperature_c,
        power_watts,
    })
}

/// Sum `used_memory` (MiB) over `--query-compute-apps=pid,used_memory`
/// lines whose pid equals `our_pid`. Returns 0 if no matching app (we hold
/// no VRAM right now) — distinct from `None`, which the caller uses for
/// "couldn't read at all, keep the last value".
fn sum_process_bytes_for_pid(stdout: &str, our_pid: u32) -> u64 {
    stdout
        .lines()
        .filter_map(|line| {
            let cols: Vec<&str> = line.split(',').map(|c| c.trim()).collect();
            if cols.len() < 2 {
                return None;
            }
            let pid = cols[0].parse::<u32>().ok()?;
            if pid != our_pid {
                return None;
            }
            cols[1].parse::<u64>().ok().map(|mib| mib * 1024 * 1024)
        })
        .sum()
}

// ─── subprocess shells (the only impure surface) ───────────────────────

const QUERY_GPU_ARGS: [&str; 2] = [
    "--query-gpu=memory.free,memory.total,utilization.gpu,temperature.gpu,power.draw",
    "--format=csv,noheader,nounits",
];

/// Synchronous one-shot probe used at construction. Mirrors
/// `detect_cuda()`'s blocking `std::process::Command` shape so `new()` can
/// decide "is this an NVIDIA host" before committing a daemon task.
fn probe_blocking() -> Option<(String, GpuSample)> {
    use std::process::Command;
    let out = Command::new("nvidia-smi")
        .args(QUERY_GPU_ARGS)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8(out.stdout).ok()?;
    let line = stdout.lines().next()?;
    let sample = parse_gpu_csv_line(line)?;

    // Device name via a separate column query (kept out of the numeric
    // line so a name with a comma can't shift the CSV columns).
    let name = Command::new("nvidia-smi")
        .args(["--query-gpu=name", "--format=csv,noheader"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "NVIDIA GPU".to_string());

    Some((name, sample))
}

/// Async per-tick GPU query (off the reactor — never blocks a runtime
/// thread). `None` on any failure so the tick keeps the last-good atomics.
async fn query_gpu() -> Option<GpuSample> {
    let out = tokio::process::Command::new("nvidia-smi")
        .args(QUERY_GPU_ARGS)
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8(out.stdout).ok()?;
    parse_gpu_csv_line(stdout.lines().next()?)
}

/// Async per-tick process-VRAM query. Sums compute-apps owned by our PID.
async fn query_process_bytes() -> Option<u64> {
    let out = tokio::process::Command::new("nvidia-smi")
        .args([
            "--query-compute-apps=pid,used_memory",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8(out.stdout).ok()?;
    Some(sum_process_bytes_for_pid(&stdout, std::process::id()))
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// What this catches: the CSV parser mis-mapping columns, dropping the
    /// MiB→bytes conversion, or not tolerating `[N/A]` optional sensors.
    /// This is the load-bearing parse — a column shift would feed the
    /// governor wrong free VRAM (the whole reason this monitor exists).
    #[test]
    fn parses_a_well_formed_gpu_csv_line() {
        // memory.free, memory.total, util%, temp°C, power W
        let s = parse_gpu_csv_line("8192, 24564, 37, 52, 210.5").expect("parses");
        assert_eq!(s.free_bytes, 8192 * 1024 * 1024);
        assert_eq!(s.total_bytes, 24564 * 1024 * 1024);
        assert!((s.utilization - 0.37).abs() < 1e-6, "util 37% → 0.37");
        assert_eq!(s.temperature_c, Some(52.0));
        assert_eq!(s.power_watts, Some(210.5));
    }

    /// What this catches: a GPU that reports `[N/A]` for power/temp (common
    /// on datacenter cards / VMs) poisoning the signal. Memory must still
    /// parse; the optional sensors must degrade to None, not 0-that-looks-real.
    #[test]
    fn tolerates_na_optional_sensors_but_requires_memory() {
        let s = parse_gpu_csv_line("1000, 2000, [N/A], [N/A], [N/A]").expect("parses");
        assert_eq!(s.free_bytes, 1000 * 1024 * 1024);
        assert_eq!(s.total_bytes, 2000 * 1024 * 1024);
        assert_eq!(s.utilization, 0.0, "unparseable util → 0.0");
        assert_eq!(s.temperature_c, None);
        assert_eq!(s.power_watts, None);

        // A line missing the memory columns is useless → whole line rejected.
        assert!(parse_gpu_csv_line("[N/A], [N/A], 0, 0, 0").is_none());
        assert!(parse_gpu_csv_line("only,three,cols").is_none());
    }

    /// What this catches: pressure math regressing (sign flip, no clamp, or
    /// divide-by-zero when total is 0). Pressure is what the governor acts
    /// on, so the boundaries matter.
    #[test]
    fn pressure_derivation_is_sane_and_clamped() {
        assert!(
            (derive_pressure(0, 100) - 1.0).abs() < 1e-6,
            "no free → full pressure"
        );
        assert!(
            (derive_pressure(100, 100) - 0.0).abs() < 1e-6,
            "all free → zero pressure"
        );
        assert!(
            (derive_pressure(25, 100) - 0.75).abs() < 1e-6,
            "25% free → 0.75 pressure"
        );
        assert_eq!(
            derive_pressure(50, 0),
            0.0,
            "total 0 must not divide-by-zero"
        );
        // free briefly exceeding total (driver reporting race) clamps, not negative.
        assert_eq!(derive_pressure(200, 100), 0.0);
    }

    /// What this catches: the None-vs-0 distinction for optional sensors
    /// round-tripping through the atomic milli-encoding. A real 0°C must
    /// stay Some(0.0); a missing sensor must stay None — conflating them
    /// would make the governor think a sensorless GPU is freezing.
    #[test]
    fn sensor_milli_encoding_round_trips_none_and_zero() {
        assert_eq!(from_milli(to_milli(None)), None);
        assert_eq!(from_milli(to_milli(Some(0.0))), Some(0.0));
        assert_eq!(from_milli(to_milli(Some(52.5))), Some(52.5));
    }

    /// What this catches: process-VRAM summing matching the wrong pid, or
    /// failing to sum multiple contexts of our own pid (a process can hold
    /// several CUDA contexts, each a compute-app row).
    #[test]
    fn sums_only_our_pids_compute_apps() {
        let stdout = "111, 100\n222, 250\n111, 50\n333, 999";
        // our pid 111 holds two contexts: 100 + 50 MiB.
        assert_eq!(
            sum_process_bytes_for_pid(stdout, 111),
            150 * 1024 * 1024,
            "should sum both rows for pid 111"
        );
        // a pid with no compute-apps holds nothing (0, not a crash).
        assert_eq!(sum_process_bytes_for_pid(stdout, 999), 0);
    }
}
