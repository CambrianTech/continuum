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

use crate::gpu::device_probe::{GpuDeviceProbe, GpuSample, MonitoredGpu};
use async_trait::async_trait;
use std::sync::Arc;

// Cadence is the base's (`device_probe::DEFAULT_TICK`, 1Hz) and this backend
// accepts it: 1Hz keeps parity with `nvidia-smi -l 1` and is the practical floor
// for a subprocess query — faster ticks just multiply child-process churn for no
// extra signal, since the driver's counters don't update meaningfully faster under
// normal load. Nothing NVIDIA-specific to override.

/// One parsed `--query-gpu` CSV row. Purely NVIDIA's wire shape; `None` fields
/// mean the GPU didn't expose that sensor (datacenter cards without a power cap
/// report `[N/A]`). Converted to the backend-neutral [`GpuSample`] in `sample()`.
struct SmiReading {
    free_bytes: u64,
    total_bytes: u64,
    utilization: f32,
    temperature_c: Option<f32>,
    power_watts: Option<f32>,
}

/// The NVIDIA-SPECIFIC half of GPU monitoring, and nothing else: the device name
/// and total confirmed once at construction, plus how to ask `nvidia-smi`.
///
/// Everything that used to sit beside it here — the atomics, the last-good
/// retention, the sensor sentinel encoding, pressure derivation, the tick — was
/// identical to the Metal backend's copy and now lives once in
/// [`MonitoredGpu`](crate::gpu::device_probe::MonitoredGpu).
pub struct NvidiaProbe {
    device_name: String,
    total_bytes: u64,
}

/// An NVIDIA device under the shared monitoring machinery.
pub type NvidiaMonitor = MonitoredGpu<NvidiaProbe>;

impl NvidiaProbe {
    /// Probe for an NVIDIA device. `None` when `nvidia-smi` is absent or its
    /// output doesn't parse — i.e. this is not an NVIDIA host. That is a fail-loud
    /// capability gap for the caller, NOT a cue to substitute a CPU monitor or a
    /// fabricated number.
    ///
    /// One synchronous probe here so a non-NVIDIA host returns `None` immediately
    /// rather than spawning a daemon that can never sample; every refresh after is
    /// async.
    pub fn detect() -> Option<Self> {
        let (device_name, reading) = probe_blocking()?;
        Some(Self {
            device_name,
            total_bytes: reading.total_bytes,
        })
    }
}

impl NvidiaMonitor {
    /// Detect an NVIDIA device and put it under the shared monitor.
    pub fn new() -> Option<Arc<Self>> {
        NvidiaProbe::detect().map(MonitoredGpu::spawn)
    }
}

#[async_trait]
impl GpuDeviceProbe for NvidiaProbe {
    fn platform(&self) -> &'static str {
        "cuda"
    }

    fn daemon_name(&self) -> &'static str {
        "nvidia-gpu"
    }

    fn device_name(&self) -> &str {
        &self.device_name
    }

    fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    /// Two `nvidia-smi` child processes, awaited off the reactor
    /// (`tokio::process`), so this never blocks a runtime thread.
    ///
    /// The two queries fail INDEPENDENTLY, and the `GpuSample` shape carries that
    /// faithfully: a `--query-gpu` hiccup leaves `free`/`utilization`/thermals
    /// `None` while a successful compute-apps query still reports
    /// `process_bytes`. Neither failure invents a value — the base keeps whatever
    /// it last measured for the terms that went missing.
    async fn sample(&self) -> GpuSample {
        let gpu = query_gpu().await;
        GpuSample {
            free_bytes: gpu.as_ref().map(|r| r.free_bytes),
            process_bytes: query_process_bytes().await,
            utilization: gpu.as_ref().map(|r| r.utilization),
            temperature_c: gpu.as_ref().and_then(|r| r.temperature_c),
            power_watts: gpu.as_ref().and_then(|r| r.power_watts),
        }
    }
}

// ─── NVIDIA-SPECIFIC parsing — unit-tested without a GPU ───────────────
//
// Pressure derivation and the None-vs-0 sensor encoding used to live here too,
// in a copy that existed identically in the Metal backend. Both are LOGIC, not
// eccentricity — one device's `[N/A]` means what another's missing syscall means —
// so they moved to `device_probe::MonitoredGpu`, which now owns them for CUDA,
// Metal, and the Vulkan/MLX adapters to come. What remains below is the genuinely
// NVIDIA part: the shape of `nvidia-smi`'s CSV.

/// Parse one `--query-gpu` CSV line:
/// `memory.free, memory.total, utilization.gpu, temperature.gpu, power.draw`
/// in `csv,noheader,nounits` form (MiB, MiB, %, °C, W). Non-numeric cells
/// (`[N/A]`, `[Not Supported]`) parse to `None` for the optional sensors;
/// the two memory columns are required (return `None` for the whole line if
/// either is missing — a reading with no memory total is useless).
fn parse_gpu_csv_line(line: &str) -> Option<SmiReading> {
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
    Some(SmiReading {
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
fn probe_blocking() -> Option<(String, SmiReading)> {
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
async fn query_gpu() -> Option<SmiReading> {
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
