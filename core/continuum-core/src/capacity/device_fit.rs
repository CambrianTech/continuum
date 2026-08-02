//! Device-fit VRAM planning — the governor's answer to "how does THIS model's
//! resident (non-expert) tier meet THIS device's VRAM budget, and how much VRAM
//! is left to hold hot experts on-device?"
//!
//! A streaming MoE serve splits into two tiers with different homes:
//!   * **experts** (`*_exps`) — paged; the [`ServingExpertPager`] plans which
//!     layers sit HOT in VRAM and streams the cold complement from NVMe.
//!   * **resident** (attention / embeddings / output / dense / norms /
//!     shared-experts) — must live in VRAM for fast compute, offloaded WHOLE.
//!     A fused attention op (K3's Gated Delta Net) CANNOT span CPU/GPU, so the
//!     resident tier is all-or-nothing on the device: a partial `-ngl` corrupts
//!     the graph (`buffer->buft` assertion). This file decides how the whole
//!     resident tier fits, and — crucially — RECONCILES it with the expert tier.
//!
//! The reconciliation is the point. Resident weights, the KV cache, the compute
//! graph, AND the hot experts all sit on the SAME device budget. Before this,
//! the expert pager was handed the full VRAM ceiling while resident silently ate
//! most of it — the two tiers double-counted the card. Here the budget is
//! partitioned ONCE: compute reserve, then resident, then a SUFFICIENT context's
//! KV, and **everything left over is the hot-expert VRAM budget**. Maximizing
//! that leftover is "as much GPU as possible": the more experts resident, the
//! fewer NVMe fetches per token — the structural win over WASTE (which keeps the
//! trunk in CPU RAM and streams every expert). [[k3-beats-waste-decisively]],
//! [[pager-control-law-is-fractal-to-grid]], task #34.
//!
//! Three resident outcomes, no guessing ([[no-masking-fallbacks-my-style-tell]]):
//!   * [`ResidentFit::Native`] — resident fits as-shipped → offload all, no override.
//!   * [`ResidentFit::Override`] — resident overflows, but a device-fit artifact
//!     supplies a precision-shrunk resident that fits → offload all, load resident
//!     from the override (`LLAMA_RESIDENT_OVERRIDE`), experts stream from the
//!     primary. The misfit-design move: fit the model to the owned device.
//!   * [`ResidentFit::Unfittable`] — resident overflows and no fitting override
//!     resolves → this device cannot GPU-serve this model. Route to CPU/grid,
//!     LOUD; never silently OOM a doomed launch.
//!
//! Context is DERIVED, never hand-picked (#31): a desired window clamped to what
//! the leftover-after-resident affords and to the model's own max. Pure over its
//! inputs; the artifact resolver is INJECTED (this module never knows a path, a
//! url, or a cache layout).

use std::path::PathBuf;

const GB: u64 = 1024 * 1024 * 1024;

/// A resolved device-fit resident-override artifact: where its first shard lives
/// and how many bytes its (precision-shrunk) resident tier occupies on-device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidentOverride {
    /// First shard of the device-fit GGUF whose resident tensors the loader
    /// sources (`LLAMA_RESIDENT_OVERRIDE`); its experts are ignored (the primary
    /// streams those). May sit on cold storage — only its resident bytes are
    /// mapped, once, at load (the loader hook's lazy, prefetch-0 mmap).
    pub path: PathBuf,
    /// The resident (non-expert) byte total this override loads onto the device.
    pub resident_bytes: u64,
}

/// How the model's resident tier meets the VRAM budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResidentFit {
    /// Resident fits as-shipped — offload all resident to GPU, no override file.
    Native,
    /// Resident overflows; this override supplies a shrunk resident that fits.
    Override(ResidentOverride),
    /// Resident overflows and no fitting override resolves. Route to CPU/grid.
    Unfittable {
        /// The as-shipped resident tier that did not fit.
        resident_bytes: u64,
        /// The budget it had to fit under (VRAM − compute reserve).
        usable_bytes: u64,
    },
}

impl ResidentFit {
    /// The resident bytes that actually load onto the device under this fit
    /// (as-shipped for `Native`, shrunk for `Override`). `None` when unfittable.
    pub fn device_resident_bytes(&self, native_bytes: u64) -> Option<u64> {
        match self {
            Self::Native => Some(native_bytes),
            Self::Override(o) => Some(o.resident_bytes),
            Self::Unfittable { .. } => None,
        }
    }

    /// The override GGUF the launcher exports as `LLAMA_RESIDENT_OVERRIDE`, if any.
    pub fn override_path(&self) -> Option<&PathBuf> {
        match self {
            Self::Override(o) => Some(&o.path),
            _ => None,
        }
    }
}

/// The device + serve facts a device-fit decision needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceFitInputs {
    /// As-shipped resident (non-expert) weight bytes (`weights_bytes − expert_bytes_total`).
    pub resident_bytes: u64,
    /// Governed VRAM budget for this serve (`SystemProfile::serving_budget_bytes`).
    pub vram_budget_bytes: u64,
    /// KV cache bytes per ONE token per lane (from the model's attention geometry).
    pub kv_bytes_per_token: u64,
    /// Fixed compute-graph + backend-context VRAM beyond weights and KV.
    pub compute_reserve_bytes: u64,
    /// The context window we'd LIKE to serve (the planner's target). Clamped down
    /// to what leftover-after-resident affords and to `model_max_context`.
    pub desired_context: u32,
    /// The model's own maximum context (`{arch}.context_length`) — the ceiling.
    pub model_max_context: u32,
    /// Concurrent decode lanes (`--parallel`); KV is per-lane, experts are shared.
    pub lanes: u32,
}

/// The complete governed VRAM serving plan: resident source, derived context, and
/// the hot-expert VRAM budget the leftover affords. The launcher turns this into
/// `LLAMA_RESIDENT_OVERRIDE` + `-c`; the expert pager consumes `expert_vram_budget_bytes`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceFitPlan {
    /// How the resident tier fits (native / override / unfittable).
    pub resident: ResidentFit,
    /// The per-lane context window the plan serves (derived, clamped). Zero iff
    /// the resident tier is `Unfittable`.
    pub context_window: u32,
    /// VRAM left for HOT experts after resident + KV + compute reserve — the
    /// budget handed to the [`ServingExpertPager`]. This is the number to MAXIMIZE
    /// (more on-GPU experts = fewer NVMe streams = beats WASTE). Zero iff unfittable.
    pub expert_vram_budget_bytes: u64,
}

impl DeviceFitPlan {
    /// True when this model can be GPU-served on this device (native or via override).
    pub fn is_gpu_servable(&self) -> bool {
        !matches!(self.resident, ResidentFit::Unfittable { .. })
    }
}

/// Runtime reserve when the caller has no measured compute figure: mirrors
/// fit-device.cpp's default (`max(2 GiB, 10% of budget)`) so the Rust plan and the
/// C++ artifact planner hold back the same headroom.
pub fn default_compute_reserve_bytes(vram_budget_bytes: u64) -> u64 {
    (vram_budget_bytes / 10).max(2 * GB)
}

/// Largest per-lane context (in tokens) whose KV fits `kv_budget_bytes`, clamped
/// to the model max. Zero when KV or lanes are zero.
fn context_fitting(
    kv_budget_bytes: u64,
    kv_bytes_per_token: u64,
    lanes: u32,
    model_max_context: u32,
) -> u32 {
    if kv_bytes_per_token == 0 || lanes == 0 {
        return 0;
    }
    let per_lane = kv_budget_bytes / lanes as u64;
    let tokens = per_lane / kv_bytes_per_token;
    u32::try_from(tokens).unwrap_or(u32::MAX).min(model_max_context)
}

/// Plan how a model's resident tier fits the device, the context the leftover
/// affords, and the hot-expert VRAM budget. `resolve_override` is the adapter:
/// given the usable byte budget (VRAM − compute reserve) it returns an override
/// whose resident is PROMISED to fit — the plan re-verifies and rejects an
/// oversized one.
///
/// Budget partition (in order): compute reserve → resident → KV for a sufficient
/// context → **everything left = hot-expert VRAM budget** (maximized).
pub fn plan_device_fit(
    inputs: &DeviceFitInputs,
    resolve_override: impl FnOnce(u64) -> Option<ResidentOverride>,
) -> DeviceFitPlan {
    let usable = inputs
        .vram_budget_bytes
        .saturating_sub(inputs.compute_reserve_bytes);

    let resident = if inputs.resident_bytes <= usable {
        ResidentFit::Native
    } else {
        match resolve_override(usable) {
            // Trust-but-verify: a shrunk resident that STILL overflows is a broken
            // artifact — refuse it, don't OOM the launch on a false promise.
            Some(ov) if ov.resident_bytes <= usable => ResidentFit::Override(ov),
            _ => ResidentFit::Unfittable {
                resident_bytes: inputs.resident_bytes,
                usable_bytes: usable,
            },
        }
    };

    let Some(on_device_resident) = resident.device_resident_bytes(inputs.resident_bytes) else {
        return DeviceFitPlan {
            resident,
            context_window: 0,
            expert_vram_budget_bytes: 0,
        };
    };

    // VRAM after the resident weights: shared between the KV cache and hot experts.
    let after_resident = usable.saturating_sub(on_device_resident);

    // Reserve KV for the DESIRED context, but never more than after_resident can
    // hold — a tight resident (K3 ~30 GB on 32 GB) squeezes the window rather than
    // overflowing. The context is the floor we give up first so experts keep VRAM.
    let desired = inputs.desired_context.min(inputs.model_max_context);
    let desired_kv = inputs
        .kv_bytes_per_token
        .saturating_mul(desired as u64)
        .saturating_mul(inputs.lanes.max(1) as u64);

    let (context_window, kv_bytes) = if desired_kv <= after_resident {
        (desired, desired_kv)
    } else {
        // Squeeze: the largest window whose KV fits what's left after resident.
        let ctx = context_fitting(
            after_resident,
            inputs.kv_bytes_per_token,
            inputs.lanes.max(1),
            desired,
        );
        let kv = inputs
            .kv_bytes_per_token
            .saturating_mul(ctx as u64)
            .saturating_mul(inputs.lanes.max(1) as u64);
        (ctx, kv)
    };

    // Everything the resident + KV didn't claim is the hot-expert budget. This is
    // the lever we maximize: more on-GPU experts, fewer streams, beat WASTE.
    let expert_vram_budget_bytes = after_resident.saturating_sub(kv_bytes);

    DeviceFitPlan {
        resident,
        context_window,
        expert_vram_budget_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn override_of(bytes: u64) -> impl FnOnce(u64) -> Option<ResidentOverride> {
        move |_usable| {
            Some(ResidentOverride {
                path: PathBuf::from("D:/k3-fit/Kimi-K3-fit-00001-of-00016.gguf"),
                resident_bytes: bytes,
            })
        }
    }
    fn no_override() -> impl FnOnce(u64) -> Option<ResidentOverride> {
        |_usable| None
    }

    // what this catches: a resident tier that fits as-shipped serves NATIVE (no
    // override artifact) and hot experts get the leftover VRAM — the small-MoE path.
    #[test]
    fn resident_under_budget_is_native_and_experts_get_leftover() {
        let plan = plan_device_fit(
            &DeviceFitInputs {
                resident_bytes: 20 * GB,
                vram_budget_bytes: 32 * GB,
                kv_bytes_per_token: 128 * 1024,
                compute_reserve_bytes: 2 * GB, // usable 30
                desired_context: 8192,
                model_max_context: 262_144,
                lanes: 1,
            },
            no_override(),
        );
        assert_eq!(plan.resident, ResidentFit::Native);
        assert!(plan.is_gpu_servable());
        assert_eq!(plan.context_window, 8192);
        // usable 30 − resident 20 − KV(8192×128KiB=1GiB) = ~9 GiB for hot experts.
        assert!(plan.expert_vram_budget_bytes > 8 * GB);
    }

    // what this catches: THE K3 case — 58 GB resident on 32 GB overflows, the 29 GB
    // device-fit override is selected (not a partial -ngl, not an OOM), and the
    // leftover funds hot experts.
    #[test]
    fn overflowing_resident_takes_override_and_frees_expert_vram() {
        let plan = plan_device_fit(
            &DeviceFitInputs {
                resident_bytes: 58 * GB,
                vram_budget_bytes: 32 * GB,
                kv_bytes_per_token: 64 * 1024,
                compute_reserve_bytes: 2 * GB, // usable 30
                desired_context: 4096,
                model_max_context: 262_144,
                lanes: 1,
            },
            override_of(24 * GB), // aggressive shrink → room for experts
        );
        match &plan.resident {
            ResidentFit::Override(o) => assert_eq!(o.resident_bytes, 24 * GB),
            other => panic!("expected Override, got {other:?}"),
        }
        assert!(plan.resident.override_path().is_some());
        // usable 30 − resident 24 − small KV → ~6 GiB hot experts (the WASTE-beating margin).
        assert!(plan.expert_vram_budget_bytes > 5 * GB);
        assert_eq!(plan.context_window, 4096);
    }

    // what this catches: a shrunk override that STILL overflows is rejected as a
    // broken artifact rather than OOMing the launch on a false promise.
    #[test]
    fn oversized_override_is_rejected_as_unfittable() {
        let plan = plan_device_fit(
            &DeviceFitInputs {
                resident_bytes: 58 * GB,
                vram_budget_bytes: 32 * GB,
                kv_bytes_per_token: 64 * 1024,
                compute_reserve_bytes: 2 * GB,
                desired_context: 4096,
                model_max_context: 262_144,
                lanes: 1,
            },
            override_of(40 * GB),
        );
        assert!(matches!(plan.resident, ResidentFit::Unfittable { .. }));
        assert!(!plan.is_gpu_servable());
        assert_eq!(plan.context_window, 0);
        assert_eq!(plan.expert_vram_budget_bytes, 0);
    }

    // what this catches: overflow with no override → Unfittable, so the caller
    // routes to CPU/grid instead of launching a doomed GPU serve.
    #[test]
    fn overflow_without_override_is_unfittable() {
        let plan = plan_device_fit(
            &DeviceFitInputs {
                resident_bytes: 58 * GB,
                vram_budget_bytes: 32 * GB,
                kv_bytes_per_token: 64 * 1024,
                compute_reserve_bytes: 2 * GB,
                desired_context: 4096,
                model_max_context: 262_144,
                lanes: 1,
            },
            no_override(),
        );
        assert!(matches!(
            plan.resident,
            ResidentFit::Unfittable { resident_bytes, usable_bytes }
                if resident_bytes == 58 * GB && usable_bytes == 30 * GB
        ));
    }

    // what this catches: when resident nearly fills the card, the context is
    // SQUEEZED to fit the sliver left (never overflow), and experts get ~nothing —
    // the honest tight-fit signal that says "shrink resident more to beat WASTE."
    #[test]
    fn tight_resident_squeezes_context_not_overflow() {
        let plan = plan_device_fit(
            &DeviceFitInputs {
                resident_bytes: 58 * GB,
                vram_budget_bytes: 32 * GB,
                kv_bytes_per_token: 64 * 1024,
                compute_reserve_bytes: 2 * GB, // usable 30
                desired_context: 32_768,       // wants a big window…
                model_max_context: 262_144,
                lanes: 1,
            },
            override_of(29 * GB), // …but resident eats 29 of 30, ~1 GiB left
        );
        assert!(matches!(plan.resident, ResidentFit::Override(_)));
        // 1 GiB / 64 KiB ≈ 16k tokens max, below the 32k desired → squeezed, not overflowed.
        assert!(plan.context_window > 0 && plan.context_window < 32_768);
        // KV ate the sliver → almost nothing for experts (the tight-fit truth).
        assert!(plan.expert_vram_budget_bytes < 1 * GB);
    }

    // what this catches: more lanes → smaller per-lane window from the same VRAM.
    #[test]
    fn more_lanes_shrink_the_per_lane_window() {
        let mk = |lanes| {
            plan_device_fit(
                &DeviceFitInputs {
                    resident_bytes: 20 * GB,
                    vram_budget_bytes: 32 * GB,
                    kv_bytes_per_token: 256 * 1024,
                    compute_reserve_bytes: 2 * GB,
                    desired_context: 262_144, // want max so VRAM is the binding limit
                    model_max_context: 262_144,
                    lanes,
                },
                no_override(),
            )
            .context_window
        };
        assert!(mk(4) < mk(1), "more lanes must shrink the per-lane window");
    }

    // what this catches: the default reserve tracks fit-device.cpp (max(2 GiB, 10%)).
    #[test]
    fn default_reserve_matches_fit_device_cpp() {
        assert_eq!(default_compute_reserve_bytes(32 * GB), 3 * GB + GB / 5); // 10% of 32 = 3.2
        assert_eq!(default_compute_reserve_bytes(8 * GB), 2 * GB); // 10% of 8 = 0.8 → floored to 2
    }
}
