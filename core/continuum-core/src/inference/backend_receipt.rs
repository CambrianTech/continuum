//! The serving binary's account of which compute backends it can actually use.
//!
//! # Why this exists
//!
//! 2026-09-05, three hosts, one seam. BigMama's Windows 5090: the installed
//! `llama-server.exe` printed `E load_backend: failed to load ggml-cuda.dll` and
//! `failed to find ggml_backend_init in ggml-cpu.dll`, fell back to its built-in CPU
//! backend, and served a 24B model on the CPU while the GPU sat at 674 MiB — the lane
//! wedged and was relaunched 13 times in 106 minutes before anyone read the server's
//! own log (card 2c33f3f0 → ce58031a). IntelMac's Intel UHD 630 Mac: Metal
//! initialisation hangs, so `--version` and `--list-devices` never return (card
//! cd8f0bc7). Both are the same fact — *what does this binary load* — and neither was
//! asked before the lane was declared ready.
//!
//! The core's own contract already says the host has a GPU (`gpu::detect_gpu` refuses
//! to boot without one), so the receipt only needs the SERVER's answer:
//!
//! | the server said                          | verdict                              |
//! |------------------------------------------|--------------------------------------|
//! | a GPU device line (`MTL0`, `CUDA0`, …)    | `Gpu { device }` — serve as planned   |
//! | answered, no GPU device                  | `Refused` — quote its load errors     |
//! | did not answer within the bound          | `ProbeHung` — CPU placement, say so   |
//!
//! Refusal, not a warning, for the answered-no-GPU case: a warning on a CUDA host is
//! the CPU-forever outage that took 13 kills to notice. Degrade, not refusal, for the
//! hung case: a box whose GPU cannot initialise still serves (the low-end contract),
//! and the receipt names why it is on the CPU.

/// One parsed `llama-server --list-devices` run (stdout + stderr, in one string).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BackendReceipt {
    /// Device lines that are real compute devices (`MTL0: Apple M5 Pro (…)`,
    /// `CUDA0: NVIDIA GeForce RTX 5090 (…)`). `BLAS`/`CPU` rows are not devices.
    pub gpu_devices: Vec<String>,
    /// `E load_backend: …` lines — the binary's own account of what it could not load.
    pub load_errors: Vec<String>,
}

/// GPU device-line prefixes llama.cpp's backends register under.
const GPU_DEVICE_PREFIXES: &[&str] = &["MTL", "CUDA", "ROCm", "HIP", "Vulkan", "SYCL", "OpenCL", "CANN"];

/// Parse a `--list-devices` transcript. Pure; the shapes are pinned from real runs
/// (M5: `  MTL0: Apple M5 Pro (53084 MiB, 53083 MiB free)`; BigMama: `E load_backend: …`).
pub fn parse_list_devices(out: &str) -> BackendReceipt {
    let mut r = BackendReceipt::default();
    for raw in out.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(err) = line.strip_prefix("E load_backend:") {
            r.load_errors.push(err.trim().to_string());
            continue;
        }
        if let Some((name, _rest)) = line.split_once(':') {
            let name = name.trim();
            let is_gpu = GPU_DEVICE_PREFIXES
                .iter()
                .any(|p| name.starts_with(p) && name[p.len()..].chars().all(|c| c.is_ascii_digit()));
            if is_gpu {
                r.gpu_devices.push(line.to_string());
            }
        }
    }
    r
}

/// What the launch does with the receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendVerdict {
    /// The server registers a GPU device — serve on it. `device` is the first line, verbatim.
    Gpu { device: String },
    /// The server answered and registers NO GPU device on a GPU host: a mis-installed
    /// binary (CPU-only build, or a backend DLL it cannot load). Refuse the lane.
    Refused { reason: String },
    /// The device probe did not answer within its bound (backend init hangs): serve on
    /// the CPU and say why — never block the launch, never pretend it is on the GPU.
    ProbeHung,
}

impl BackendVerdict {
    /// The one-word backend the serving snapshot shows.
    pub fn backend_label(&self) -> &'static str {
        match self {
            BackendVerdict::Gpu { device } if device.starts_with("MTL") => "metal",
            BackendVerdict::Gpu { device } if device.starts_with("CUDA") => "cuda",
            BackendVerdict::Gpu { .. } => "gpu",
            BackendVerdict::Refused { .. } => "refused",
            BackendVerdict::ProbeHung => "cpu",
        }
    }
}

/// `None` = the probe timed out (no transcript to read).
pub fn backend_verdict(bin: &str, receipt: Option<&BackendReceipt>) -> BackendVerdict {
    let Some(r) = receipt else {
        return BackendVerdict::ProbeHung;
    };
    if let Some(device) = r.gpu_devices.first() {
        return BackendVerdict::Gpu {
            device: device.clone(),
        };
    }
    let errors = if r.load_errors.is_empty() {
        "it listed no GPU device and reported no load error — a CPU-only build".to_string()
    } else {
        format!("its own load errors: {}", r.load_errors.join(" | "))
    };
    BackendVerdict::Refused {
        reason: format!(
            "{bin} registers no GPU backend on a GPU host — {errors}. A lane on the CPU is the \
             13-kills-in-106-minutes outage (2c33f3f0); reinstall a server built with this \
             host's backend (GGML_CUDA=ON / Metal) and matching ggml-*.dll files, then relaunch."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the two real transcripts of 2026-09-05 read as the two
    // opposite verdicts, and a BLAS/CPU row never counts as a GPU.
    #[test]
    fn m5_metal_transcript_is_a_gpu_and_bigmamas_broken_dlls_are_a_refusal() {
        let m5 = "Available devices:\n  MTL0: Apple M5 Pro (53084 MiB, 53083 MiB free)\n  BLAS: Accelerate (0 MiB, 0 MiB free)\n";
        let r = parse_list_devices(m5);
        assert_eq!(r.gpu_devices.len(), 1, "BLAS is not a device: {r:?}");
        assert!(r.load_errors.is_empty());
        let v = backend_verdict("llama-server", Some(&r));
        assert!(matches!(v, BackendVerdict::Gpu { ref device } if device.starts_with("MTL0")));
        assert_eq!(v.backend_label(), "metal");

        let bigmama = "E load_backend: failed to load ggml-cuda.dll: The specified module could not be found\nE load_backend: failed to find ggml_backend_init in ggml-cpu.dll\nAvailable devices:\n";
        let r = parse_list_devices(bigmama);
        assert!(r.gpu_devices.is_empty());
        assert_eq!(r.load_errors.len(), 2);
        match backend_verdict("llama-server.exe", Some(&r)) {
            BackendVerdict::Refused { reason } => {
                assert!(reason.contains("ggml-cuda.dll"), "the refusal quotes the server's own error: {reason}");
                assert!(reason.contains("GGML_CUDA=ON"));
            }
            other => panic!("a GPU host with no GPU backend must be refused, got {other:?}"),
        }
    }

    // what this catches: a CUDA device line reads as cuda, and a hung probe is CPU
    // placement, never a refusal and never a guess of "gpu".
    #[test]
    fn cuda_line_is_cuda_and_a_hung_probe_degrades_to_cpu() {
        let r = parse_list_devices("Available devices:\n  CUDA0: NVIDIA GeForce RTX 5090 (32607 MiB, 31900 MiB free)\n");
        assert_eq!(backend_verdict("x", Some(&r)).backend_label(), "cuda");
        let hung = backend_verdict("x", None);
        assert_eq!(hung, BackendVerdict::ProbeHung);
        assert_eq!(hung.backend_label(), "cpu");
    }

    // what this catches: an answered transcript with no devices and no errors (a
    // plain CPU-only build) is still a refusal — silence about the GPU is not a GPU.
    #[test]
    fn a_silent_cpu_only_build_is_refused_too() {
        let r = parse_list_devices("Available devices:\n  CPU: (0 MiB, 0 MiB free)\n");
        assert!(matches!(backend_verdict("x", Some(&r)), BackendVerdict::Refused { .. }));
    }
}
