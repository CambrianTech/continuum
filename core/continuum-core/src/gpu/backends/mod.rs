//! **The one place a GPU platform is selected.**
//!
//! Every backend here implements [`super::device_probe::GpuDeviceProbe`] — it supplies
//! *how to ask this device*, and [`super::device_probe::MonitoredGpu`] owns everything
//! else (retention, the unknown state, the tick, pressure, the `GpuMonitor` impl). A
//! backend that reaches for `GpuMonitor` directly is bypassing the base; the guard in
//! `device_probe` catches that.
//!
//! # Why this module exists rather than flat siblings
//!
//! `metal_monitor` and `nvidia_monitor` used to sit flat beside the base with their
//! `#[cfg]` gates sprinkled at each `mod` and each `pub use`. That is the same concern
//! — *which platform am I building for* — decided in four places, and on 2026-08-20 one
//! of them drifted: the attribute binds to the NEXT item only, so inserting a line
//! above `pub use metal_monitor::…` silently moved the gate onto the platform-neutral
//! base and left the macOS-only adapter exported on every target. It compiled on the
//! Mac and died with E0432 on Linux AND Windows — both CI jobs, one cause.
//!
//! The fix is structural, not vigilance: **platform selection happens HERE and nowhere
//! else.** `gpu/mod.rs` re-exports this module's surface with no `cfg` of its own,
//! because by the time it does, the platform question is already answered. There is no
//! second gate to put on the wrong line.
//!
//! Adding a backend (Vulkan, ROCm, MLX) is therefore a local edit: a file here, a
//! `mod` + `pub use` pair below, and nothing anywhere else changes.

/// Apple Metal. macOS only — it links Mach FFI, so it cannot build elsewhere.
#[cfg(target_os = "macos")]
pub mod metal;

/// NVIDIA. Pure subprocess (`nvidia-smi`) + parsing, no NVIDIA FFI, so it compiles on
/// EVERY platform and its parser tests run on the Mac dev box. `new()` returns `None`
/// where `nvidia-smi` is absent, so building it everywhere costs nothing on non-NVIDIA
/// hosts. Deliberately ungated — do not "tidy" a `cfg` onto it.
pub mod nvidia;

#[cfg(target_os = "macos")]
pub use metal::{MetalMonitor, MetalProbe};
pub use nvidia::{NvidiaMonitor, NvidiaProbe};

#[cfg(test)]
mod tests {
    /// What this catches: a backend's `pub use` drifting out of agreement with its
    /// `mod` declaration — the E0432 that took down both CI jobs on 2026-08-20 and was
    /// invisible on a Mac, because the gate that was missing is one macOS satisfies.
    ///
    /// Reading this file's own source is the only way to assert it from a single
    /// target: a compile-time check would need the other platform to run on.
    #[test]
    fn every_backend_export_carries_the_same_gate_as_its_module() {
        let src = include_str!("mod.rs");
        let code: Vec<&str> = src
            .lines()
            .map(|l| crate::source_hygiene::split_code_and_comment(l).0.trim())
            .collect();

        // For each `pub mod NAME;` remember whether a cfg attribute sits directly above
        // it, then require the matching `pub use NAME::` to carry the same.
        for (i, line) in code.iter().enumerate() {
            let Some(name) = line
                .strip_prefix("pub mod ")
                .and_then(|r| r.strip_suffix(';'))
            else {
                continue;
            };
            let mod_gated = i > 0 && code[i - 1].starts_with("#[cfg(");
            let use_idx = code
                .iter()
                .position(|l| l.starts_with(&format!("pub use {name}::")))
                .unwrap_or_else(|| panic!("backend `{name}` is declared but never exported"));
            let use_gated = use_idx > 0 && code[use_idx - 1].starts_with("#[cfg(");

            assert_eq!(
                mod_gated, use_gated,
                "backend `{name}`: `pub mod` is {} but `pub use` is {} — a gate on one \
                 and not the other is E0432 on the platforms this host cannot build. \
                 The cfg attribute binds to the NEXT item only; check what is directly \
                 above each.",
                if mod_gated { "gated" } else { "ungated" },
                if use_gated { "gated" } else { "ungated" },
            );
        }
    }
}
