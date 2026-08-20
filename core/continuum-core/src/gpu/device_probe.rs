//! The GPU adapter contract: **backends supply a sample, the base owns everything else.**
//!
//! # Why this module exists
//!
//! Before it, every backend implemented [`GpuMonitor`](super::monitor::GpuMonitor)
//! *whole* — its own atomics, its own last-good retention, its own tick, its own
//! pressure derivation. Four copies of one concern, and each was free to express it
//! differently. It went wrong exactly the way that always goes wrong: the Metal
//! backend answered a failed Mach syscall with `total_bytes` — "the sensor broke, so
//! assume every byte is free" — and handed that straight to the ResourceGovernor,
//! while the NVIDIA backend, sampling at construction, never had the same hole. One
//! backend lying and the other not is not two facts about two backends; it is one
//! missing constraint. A Vulkan or MLX adapter written against the old trait would
//! have been free to invent the same lie a third time — and `monitor.rs`'s own
//! Vulkan TODO names that exact hazard ("a stale `free` that never drops is the
//! exact bug this whole live-monitor layer exists to kill").
//!
//! So the split is by NATURE, not by convenience:
//!
//! | Concern | Where it lives | Why |
//! |---|---|---|
//! | How to ASK this device (Mach VM stats, `nvidia-smi` CSV, `VK_EXT_memory_budget`, MLX) | the backend's [`GpuDeviceProbe`] | genuinely differs per platform |
//! | What a missing reading MEANS | [`GpuSample`] — `None`, never a substitute | one rule, enforced by the type |
//! | Retaining the last good reading | [`MonitoredGpu`] | identical for every device |
//! | The unknown-until-first-sample state | [`MonitoredGpu`] | identical for every device |
//! | Tick cadence, panic isolation, publishing | [`MonitoredGpu`] via [`Daemon`] | identical for every device |
//! | Deriving pressure from free/total | [`MonitoredGpu`] | identical for every device |
//!
//! A backend cannot express the substitution bug because a backend never touches the
//! stored value. It returns what the device said, `None` for what the device didn't,
//! and the base decides the rest — once, for CUDA, Metal, Vulkan and MLX alike.

use super::monitor::{GpuMonitor, MemoryMode};
use crate::runtime::{spawn_daemon, Daemon, DaemonChannel};
use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::watch;
use tokio::time::Duration;

/// Default cadence. 1Hz matches what the OS counters actually refresh at on every
/// backend measured so far (`host_vm_info` internally, NVML's driver counters), so
/// faster ticks buy no signal. A backend whose device disagrees overrides
/// [`GpuDeviceProbe::cadence`].
const DEFAULT_TICK: Duration = Duration::from_secs(1);

/// One sampling ATTEMPT against a physical device.
///
/// Every term is `Option` and every `None` means the same thing on every backend:
/// **the platform did not report it this tick.** It never means zero, and it never
/// licenses a stand-in. A backend that cannot read free bytes returns
/// `free_bytes: None` — it does not return the total, the last value it remembers,
/// or a guess. Remembering is [`MonitoredGpu`]'s job and it does it identically for
/// everyone.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GpuSample {
    /// Free bytes on the device, system-wide (ours and every other process's).
    pub free_bytes: Option<u64>,
    /// Bytes attributable to THIS process.
    pub process_bytes: Option<u64>,
    /// Compute utilization, 0.0..1.0.
    pub utilization: Option<f32>,
    /// Die temperature in Celsius, when the device exposes a sensor.
    pub temperature_c: Option<f32>,
    /// Instantaneous power draw in watts, when the device exposes a sensor.
    pub power_watts: Option<f32>,
}

/// What a GPU backend supplies — **and all it supplies.**
///
/// Implement this, not [`GpuMonitor`]. The device-specific half is the two
/// constants (`platform`, `device_name`, `total_bytes`, `memory_mode`) plus
/// [`sample`](Self::sample); wrap it in [`MonitoredGpu`] and the monitor contract
/// is satisfied by machinery you did not write and cannot get wrong.
///
/// `sample` is async because some devices are queried by subprocess (`nvidia-smi`)
/// and some by a direct FFI call (Mach, and a future Vulkan `ash` call). The async
/// shape is the general one; an in-process backend simply returns immediately.
#[async_trait]
pub trait GpuDeviceProbe: Send + Sync + 'static {
    /// `"metal"` | `"cuda"` | `"vulkan"` | `"mlx"` | `"mock"`.
    fn platform(&self) -> &'static str;

    /// Daemon name for the runtime's task registry and logs.
    fn daemon_name(&self) -> &'static str;

    /// Human-readable device name, fixed at construction.
    fn device_name(&self) -> &str;

    /// Total addressable device memory. A CONSTANT for the device — probed once at
    /// construction, which is why it is not a [`GpuSample`] term.
    fn total_bytes(&self) -> u64;

    /// Whether this device shares one physical pool with the host (Apple Silicon,
    /// ARM iGPUs) or owns separate VRAM. The governor's VRAM and RAM ledgers are the
    /// same pool or two pools depending on this answer, so it is DETECTED by the
    /// backend, never inferred from `target_os`.
    fn memory_mode(&self) -> MemoryMode {
        MemoryMode::Discrete
    }

    /// How often to sample. See [`DEFAULT_TICK`].
    fn cadence(&self) -> Duration {
        DEFAULT_TICK
    }

    /// One attempt to read the device. Report what it said; report `None` for what it
    /// didn't. Do not substitute, do not remember — see [`GpuSample`].
    async fn sample(&self) -> GpuSample;
}

/// Sentinel in the temperature / power atomics meaning "no sensor reading has ever
/// landed". Distinct from a real 0°C / 0W.
const NO_SENSOR: i32 = i32::MIN;

/// The common half of every GPU monitor: last-good retention, the unknown state,
/// the tick, and the published pressure signal — written ONCE.
///
/// Wraps any [`GpuDeviceProbe`] and implements [`GpuMonitor`] on its behalf.
pub struct MonitoredGpu<P: GpuDeviceProbe> {
    probe: P,
    /// Last MEASURED free bytes. Meaningless until `free_sampled`; never written
    /// from a `None` sample term.
    free_bytes: AtomicU64,
    /// Has a real free-bytes reading ever landed? This is what makes "unknown"
    /// representable, and its absence is what let a backend seed a fabricated
    /// full-tank reading at construction.
    free_sampled: AtomicBool,
    process_bytes: AtomicU64,
    /// Utilization × 1000 so it fits an atomic.
    utilization_x1000: AtomicU32,
    /// Milli-Celsius, or [`NO_SENSOR`].
    temperature_mc: AtomicI32,
    /// Milli-watts, or [`NO_SENSOR`].
    power_mw: AtomicI32,
    /// Derived pressure (`1 − free/total`). Ungated: this is a continuous signal and
    /// the pressure-broker owns the policy of what level warrants backoff.
    channel: DaemonChannel<f32>,
}

impl<P: GpuDeviceProbe> MonitoredGpu<P> {
    /// Wrap a probe and spawn it on the shared [`Daemon`] runner.
    ///
    /// No reading exists until the first tick, and [`GpuMonitor::free_bytes`]
    /// reports exactly that. The alternative — seeding from `total_bytes` — is the
    /// bug this type exists to make unrepresentable; the alternative of a blocking
    /// probe here would put device I/O in a constructor. One tick is the window, and
    /// the consumers already model unknown.
    pub fn spawn(probe: P) -> Arc<Self> {
        let monitor = Arc::new(Self {
            probe,
            free_bytes: AtomicU64::new(0),
            free_sampled: AtomicBool::new(false),
            process_bytes: AtomicU64::new(0),
            utilization_x1000: AtomicU32::new(0),
            temperature_mc: AtomicI32::new(NO_SENSOR),
            power_mw: AtomicI32::new(NO_SENSOR),
            channel: DaemonChannel::ungated(0.0f32),
        });
        // The shared runner owns the interval + per-tick catch_unwind, so a panic in
        // one backend's FFI or parsing loses a single tick and resumes against the
        // last-good snapshot instead of killing GPU monitoring process-wide.
        let _ = spawn_daemon(monitor.clone());
        monitor
    }

    /// Apply one sample: store the terms the device reported, leave the rest
    /// standing. Split out from `tick` so the retention rule — the thing that was
    /// wrong per-backend — is directly testable without a device.
    pub(crate) fn absorb(&self, sample: GpuSample) {
        if let Some(free) = sample.free_bytes {
            self.free_bytes.store(free, Ordering::Relaxed);
            self.free_sampled.store(true, Ordering::Relaxed);
        }
        if let Some(proc_bytes) = sample.process_bytes {
            self.process_bytes.store(proc_bytes, Ordering::Relaxed);
        }
        if let Some(util) = sample.utilization {
            self.utilization_x1000
                .store((util.clamp(0.0, 1.0) * 1000.0) as u32, Ordering::Relaxed);
        }
        if let Some(temp) = sample.temperature_c {
            self.temperature_mc
                .store((temp * 1000.0) as i32, Ordering::Relaxed);
        }
        if let Some(power) = sample.power_watts {
            self.power_mw.store((power * 1000.0) as i32, Ordering::Relaxed);
        }
    }

    /// Pressure from the last MEASURED free, or `None` while there is none.
    ///
    /// Clamped to [0,1]: on unified memory `free` can briefly exceed `total` when an
    /// inactive→free transition races the read, and the clamp keeps the shape uniform
    /// across backends.
    pub(crate) fn derived_pressure(&self) -> Option<f32> {
        let total = self.probe.total_bytes();
        if total == 0 {
            return None;
        }
        self.free_bytes()
            .map(|free| 1.0 - (free as f32 / total as f32).clamp(0.0, 1.0))
    }

    /// The wrapped backend, for tests and for callers that need the concrete probe.
    pub fn probe(&self) -> &P {
        &self.probe
    }
}

#[async_trait]
impl<P: GpuDeviceProbe> Daemon for MonitoredGpu<P> {
    type Snapshot = f32;

    fn name(&self) -> &'static str {
        self.probe.daemon_name()
    }

    fn cadence(&self) -> Duration {
        self.probe.cadence()
    }

    fn channel(&self) -> &DaemonChannel<f32> {
        &self.channel
    }

    async fn tick(&self) {
        self.absorb(self.probe.sample().await);
        // Only publish when a reading exists — a tick that measured nothing leaves
        // the previous pressure standing rather than republishing a number derived
        // from a substitute.
        if let Some(pressure) = self.derived_pressure() {
            self.channel.publish(pressure);
        }
    }
}

impl<P: GpuDeviceProbe> GpuMonitor for MonitoredGpu<P> {
    fn platform(&self) -> &'static str {
        self.probe.platform()
    }

    fn memory_mode(&self) -> MemoryMode {
        self.probe.memory_mode()
    }

    fn device_name(&self) -> &str {
        self.probe.device_name()
    }

    fn total_bytes(&self) -> u64 {
        self.probe.total_bytes()
    }

    fn free_bytes(&self) -> Option<u64> {
        self.free_sampled
            .load(Ordering::Relaxed)
            .then(|| self.free_bytes.load(Ordering::Relaxed))
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

/// Milli-units back to units, mapping the [`NO_SENSOR`] sentinel to `None`.
fn from_milli(v: i32) -> Option<f32> {
    (v != NO_SENSOR).then(|| v as f32 / 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A probe whose every sample is scripted — including the failures no real
    /// device produces on demand. This is the fixture that makes the retention rule
    /// testable for EVERY backend at once, which is the whole point of the split.
    struct ScriptedProbe {
        total: u64,
        script: Mutex<Vec<GpuSample>>,
    }

    impl ScriptedProbe {
        fn new(total: u64, script: Vec<GpuSample>) -> Self {
            Self {
                total,
                script: Mutex::new(script),
            }
        }
    }

    #[async_trait]
    impl GpuDeviceProbe for ScriptedProbe {
        fn platform(&self) -> &'static str {
            "mock"
        }
        fn daemon_name(&self) -> &'static str {
            "scripted-gpu"
        }
        fn device_name(&self) -> &str {
            "Scripted Device"
        }
        fn total_bytes(&self) -> u64 {
            self.total
        }
        async fn sample(&self) -> GpuSample {
            let mut s = self.script.lock().expect("scripted probe lock");
            if s.is_empty() {
                GpuSample::default()
            } else {
                s.remove(0)
            }
        }
    }

    fn monitor_with(total: u64, script: Vec<GpuSample>) -> MonitoredGpu<ScriptedProbe> {
        // Constructed directly, NOT via `spawn` — these tests drive `absorb` by hand
        // so they assert the retention rule itself, with no runtime or timing in the
        // way.
        MonitoredGpu {
            probe: ScriptedProbe::new(total, script),
            free_bytes: AtomicU64::new(0),
            free_sampled: AtomicBool::new(false),
            process_bytes: AtomicU64::new(0),
            utilization_x1000: AtomicU32::new(0),
            temperature_mc: AtomicI32::new(NO_SENSOR),
            power_mw: AtomicI32::new(NO_SENSOR),
            channel: DaemonChannel::ungated(0.0f32),
        }
    }

    /// What this catches: a backend or the base seeding a fabricated free-bytes
    /// reading before any sample lands. This is the exact shape of the Metal bug —
    /// `free_bytes: AtomicU64::new(total_bytes)` — which told the governor the pool
    /// was entirely free for the first tick of every boot. Now unrepresentable: the
    /// only way to get `Some` is to absorb a sample that carried one.
    #[test]
    fn free_bytes_is_unknown_until_a_real_sample_lands() {
        let m = monitor_with(64, vec![]);
        assert_eq!(m.free_bytes(), None, "no sample yet — must be unknown");
        assert_eq!(m.derived_pressure(), None, "no reading, no pressure");

        m.absorb(GpuSample {
            free_bytes: Some(16),
            ..Default::default()
        });
        assert_eq!(m.free_bytes(), Some(16));
    }

    /// What this catches: a failed read overwriting a good reading — the second half
    /// of the same bug. `unwrap_or(total)` did this on EVERY failed Mach call, not
    /// just the first, so a mid-life sensor failure read as "the pool just emptied".
    /// The rule is: a `None` term does not refresh, it does not clear, and it does
    /// not substitute.
    #[test]
    fn a_failed_read_leaves_the_last_measured_value_standing() {
        let m = monitor_with(64, vec![]);
        m.absorb(GpuSample {
            free_bytes: Some(16),
            process_bytes: Some(8),
            ..Default::default()
        });
        m.absorb(GpuSample::default()); // the whole device went unreadable

        assert_eq!(
            m.free_bytes(),
            Some(16),
            "a failed sample must not fabricate, clear, or refresh"
        );
        assert_eq!(m.process_bytes(), 8);
    }

    /// What this catches: sensors that a device exposes only sometimes being read as
    /// a real 0 once they drop out. Temperature and power are genuinely absent on
    /// many datacenter cards (`[N/A]` in nvidia-smi), so the sentinel has to survive
    /// round-tripping.
    #[test]
    fn absent_sensors_report_none_not_zero() {
        let m = monitor_with(64, vec![]);
        assert_eq!(m.temperature_c(), None);
        assert_eq!(m.power_watts(), None);

        m.absorb(GpuSample {
            temperature_c: Some(41.5),
            power_watts: Some(120.0),
            ..Default::default()
        });
        assert_eq!(m.temperature_c(), Some(41.5));
        assert_eq!(m.power_watts(), Some(120.0));
    }

    /// What this catches: pressure derived from a free reading that exceeds total —
    /// real on unified memory, where an inactive→free transition can race the read.
    /// Must clamp rather than produce a negative pressure the broker would compare
    /// against thresholds.
    #[test]
    fn pressure_clamps_when_free_briefly_exceeds_total() {
        let m = monitor_with(100, vec![]);
        m.absorb(GpuSample {
            free_bytes: Some(120),
            ..Default::default()
        });
        assert_eq!(m.derived_pressure(), Some(0.0));

        m.absorb(GpuSample {
            free_bytes: Some(25),
            ..Default::default()
        });
        assert_eq!(m.derived_pressure(), Some(0.75));
    }

    /// What this catches: **a new backend bypassing this module entirely.**
    ///
    /// The split above is only load-bearing if backends actually go through it. A
    /// Vulkan / MLX / ROCm author who writes `impl GpuMonitor for VulkanMonitor`
    /// gets a compiling, plausible-looking adapter that re-owns retention and the
    /// unknown state — and is free to re-invent the `unwrap_or(total)` lie a third
    /// time. Prose in a module header cannot stop that; this can.
    ///
    /// The rule: exactly TWO `impl GpuMonitor` sites exist tree-wide — this
    /// module's blanket impl for `MonitoredGpu<P>`, and `MockMonitor` (a test
    /// double, scripted by definition, which is why it is exempt and named here
    /// rather than pattern-excused). Everything else implements `GpuDeviceProbe`.
    ///
    /// Comment-stripped before matching so a doc mention of `impl GpuMonitor`
    /// cannot satisfy OR trip the check — the same predicate discipline as the
    /// module-wiring audit in `registry.rs`.
    #[test]
    fn every_gpu_backend_goes_through_the_shared_base() {
        /// The only impls allowed to exist, each with the reason it is not a
        /// backend. Adding a row is a DESIGN decision, not a formality.
        const SANCTIONED: &[(&str, &str)] = &[
            (
                "gpu/device_probe.rs",
                "the blanket impl every backend inherits — this IS the base",
            ),
            (
                "gpu/monitor.rs",
                "MockMonitor: a scripted test double with no device behind it",
            ),
            (
                "cognition/host_capability_probe.rs",
                "a #[cfg(test)] fake device inside that module's own tests — no hardware, \
                 no sampling loop. Found BY this guard on its first run, which is the point. \
                 It is a second test double next to MockMonitor and should collapse into it \
                 (CLAUDE.md: one fixture per concern) — sanctioned as a test, not as a backend.",
            ),
        ];

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut offenders = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().is_none_or(|e| e != "rs") {
                    continue;
                }
                let Ok(src) = std::fs::read_to_string(&path) else {
                    continue;
                };
                // Strip line comments + doc comments so prose can neither fake
                // nor trip the check.
                let code: String = src
                    .lines()
                    .filter(|l| !l.trim_start().starts_with("//"))
                    .collect::<Vec<_>>()
                    .join("\n");
                if !code.contains("impl GpuMonitor for") {
                    continue;
                }
                let rel = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                if !SANCTIONED.iter().any(|(f, _)| rel == *f) {
                    offenders.push(rel);
                }
            }
        }

        assert!(
            offenders.is_empty(),
            "these files implement GpuMonitor directly, bypassing the shared base: {offenders:?}\n\
             A GPU backend implements `GpuDeviceProbe` (how to ask THIS device) and wraps in \
             `MonitoredGpu` — it does NOT own retention, the unknown state, the tick, or pressure. \
             That ownership split is what makes the `unwrap_or(total)` class unrepresentable; \
             see this module's header. If a new impl is genuinely not a backend, add it to \
             SANCTIONED with its reason."
        );
    }

    /// What this catches: a device reporting `total_bytes == 0` (no device, or a
    /// parse failure) producing a divide-by-zero or a NaN pressure.
    #[test]
    fn zero_total_yields_no_pressure_rather_than_nan() {
        let m = monitor_with(0, vec![]);
        m.absorb(GpuSample {
            free_bytes: Some(0),
            ..Default::default()
        });
        assert_eq!(m.derived_pressure(), None);
    }
}
