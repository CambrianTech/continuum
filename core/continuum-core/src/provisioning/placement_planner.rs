//! `PlacementPlanner` — the RESOLUTION half of `catalog = f(system × storage × grid)`.
//!
//! Given what THIS box ([`SystemProfile`]) and the grid can do, resolve WHERE a model
//! runs — and when it doesn't fit at full fidelity here, resolve it DOWN (grid node /
//! smaller variant) rather than EXCLUDING it. There is deliberately **no `Excluded`
//! variant**: storage and grid are RESOLUTION FIELDS, never gates
//! ([[public-project-not-joels-machines]], solve-for-public-users). The default must
//! work on any laptop — a box that can't serve a model locally still gets an answer.
//!
//! It REUSES the fit primitives rather than re-deriving them:
//!   - [`SystemProfile::serving_budget_bytes`] — the VRAM budget (the one 0.80
//!     `vram_headroom` via `host_budget_from`, config-inherited, no second const).
//!   - [`footprint_for`] — the `Model` → [`ModelFootprint`] bridge (GGUF on disk →
//!     size). `None` ⇒ not provisioned yet.
//!   - [`plan_serving`] — the honest fit oracle (`fits_on_gpu`).
//!
//! The grid verdict is an INPUT (`grid_has_fit`), derived by the caller from
//! `plan_grid_placement` over a [`GridSnapshot`](crate::capacity::grid::GridSnapshot),
//! so the resolution core stays pure and testable (a solo box passes `false`).

use crate::capacity::grid::GridSnapshot;
use crate::capacity::SystemProfile;
use crate::cognition::serving_plan::{plan_serving, HostBudget, ModelFootprint};
use crate::model_registry::types::Model;

/// How a model resolves against a system + grid. Every case is an ANSWER — there is
/// no "excluded": the offering resolves up (local) or down (grid / provision /
/// degrade), never out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlacementResolution {
    /// Fits this node's serving budget — the recommended experience, served HERE.
    LocalRecommended { budget_bytes: u64, weights_bytes: u64 },
    /// No artifact on disk yet — resolve by PROVISIONING it, to the COLD tier when
    /// this box has one (big models / MoE expert sets belong on the offload drive),
    /// else the system drive. "Not here yet", never "can't run".
    NeedsProvisioning { to_cold_tier: bool },
    /// An MoE model too big to fit WHOLE in the VRAM budget, but this box has a COLD
    /// tier: resolve to EXPERT-PAGED serving — the base + all experts FROZEN on the
    /// cold drive (`TierRole::Frozen`), the hot subset paged into VRAM on demand
    /// (`PageKind::MoEExpert` over `ArtifactSource::Mapped`, sentinel-PGO picking the
    /// subset). The K3 path: a 594 GB model "fits" a 32 GB GPU by RESIDENCY + paging,
    /// not by shrinking. Preferred over grid/degrade when a cold tier exists — local
    /// frontier intelligence, no cloud. (Prefer-grid-if-a-peer-serves-it-whole-faster
    /// is a throughput-policy follow-up, not this slice.)
    MoePaged { vram_budget_bytes: u64, weights_bytes: u64 },
    /// Too big for THIS node's budget → resolve to a grid node that fits. The grid is
    /// a resolution field: too-big-HERE becomes served-THERE, not excluded.
    GridRouted { local_budget_bytes: u64, weights_bytes: u64 },
    /// Doesn't fit locally AND no grid node fits → degrade to a smaller variant /
    /// cloud. The FLOOR — still an answer (the caller owns the smaller-variant / cloud
    /// choice), never an exclusion.
    Degraded { local_budget_bytes: u64, weights_bytes: u64 },
}

/// PURE resolution — the testable core. Feeds on an already-resolved footprint
/// (`None` = no artifact on disk) plus the caller's grid verdict, so it does zero
/// I/O and every branch is exercised with synthetic inputs.
pub fn resolve_from_footprint(
    profile: &SystemProfile,
    footprint: Option<&ModelFootprint>,
    is_moe: bool,
    grid_has_fit: bool,
) -> PlacementResolution {
    let usable = profile.serving_budget_bytes();

    let Some(fp) = footprint else {
        // Not on disk → provision it. Prefer the cold tier when present (that's where
        // big GGUFs / MoE expert sets live), else the system drive. Never excluded.
        return PlacementResolution::NeedsProvisioning {
            to_cold_tier: profile.has_cold_tier(),
        };
    };

    let host = HostBudget {
        usable_bytes: usable,
        perf_cores: profile.perf_cores,
    };
    // `plan_serving` returns `Some` even when nothing fits (its honest-degrade path,
    // `fits_on_gpu = false`); `None` only for an empty candidate slice, which a
    // single-element slice never is — so `unwrap_or(false)` is the not-empty floor.
    let fits = plan_serving(host, std::slice::from_ref(fp), 1)
        .map(|p| p.fits_on_gpu)
        .unwrap_or(false);

    if fits {
        PlacementResolution::LocalRecommended {
            budget_bytes: usable,
            weights_bytes: fp.weights_bytes,
        }
    } else if is_moe && profile.has_cold_tier() {
        // The K3 magic: an MoE model that can't fit whole in VRAM still serves LOCALLY
        // by freezing its experts on the cold drive and paging the hot subset — no
        // grid hop, no cloud. Preferred over grid/degrade because local frontier
        // intelligence is the whole thesis.
        PlacementResolution::MoePaged {
            vram_budget_bytes: usable,
            weights_bytes: fp.weights_bytes,
        }
    } else if grid_has_fit {
        PlacementResolution::GridRouted {
            local_budget_bytes: usable,
            weights_bytes: fp.weights_bytes,
        }
    } else {
        PlacementResolution::Degraded {
            local_budget_bytes: usable,
            weights_bytes: fp.weights_bytes,
        }
    }
}

/// Resolve a model's placement on this box. Reuses [`footprint_for`] (GGUF → size)
/// then the pure core.
///
/// `is_moe` is the caller's GGUF-derived verdict — MoE-ness is a FACT the GGUF
/// carries (`{arch}.expert_count`), read via
/// [`locate_layer_sets`](crate::genome::expert_layout::locate_layer_sets) /
/// `gguf_keys::expert_count` at the site that already loads the header — NOT a
/// hand-authored catalog field (the catalog's "ask the GGUF" discipline). `grid` is
/// the live [`GridSnapshot`] (`None` on a solo box); the grid verdict is derived from
/// it via [`grid_has_fit`].
///
/// [`footprint_for`]: crate::modules::serving_daemon::footprint_for
pub fn resolve_placement(
    profile: &SystemProfile,
    model: &Model,
    is_moe: bool,
    grid: Option<&GridSnapshot>,
) -> PlacementResolution {
    let fp = crate::modules::serving_daemon::footprint_for(model);
    let grid_fit = match (grid, fp.as_ref()) {
        (Some(snapshot), Some(f)) => grid_has_fit(snapshot, f),
        _ => false,
    };
    resolve_from_footprint(profile, fp.as_ref(), is_moe, grid_fit)
}

/// Does ANY reachable grid peer's live-free VRAM hold this model (weights + one
/// lane's KV)? The grid verdict for [`resolve_placement`], honoring the "sum of
/// per-node FITS, never an aggregate pool" doctrine ([`GridSnapshot`]): each reachable
/// peer is tested with the SAME [`plan_serving`] oracle used locally, against its own
/// live-free bytes. A peer that could only CPU-spill does NOT count as a fit
/// (`fits_on_gpu = false`) — the grid resolves a model to a node that can actually
/// GPU-serve it, or it degrades; it never routes to a node that can't.
pub fn grid_has_fit(snapshot: &GridSnapshot, footprint: &ModelFootprint) -> bool {
    snapshot
        .peers
        .iter()
        .filter(|p| p.reachable)
        .any(|p| {
            let host = HostBudget {
                usable_bytes: p.capacity.gpu_free_bytes_live,
                perf_cores: 1,
            };
            plan_serving(host, std::slice::from_ref(footprint), 1)
                .map(|plan| plan.fits_on_gpu)
                .unwrap_or(false)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capacity::{DeviceCapacity, DriveInfo, DriveRole};
    use crate::governor::types::{HardwareClass, PowerSource, TargetSilicon, ThermalClass};

    const GB: u64 = 1024 * 1024 * 1024;

    fn discrete(vram_gb: u64, gpu_free_gb: u64, with_cold: bool) -> SystemProfile {
        let mut drives = vec![DriveInfo {
            mount: "C:\\".into(),
            total_bytes: 2000 * GB,
            available_bytes: 20 * GB,
            role: DriveRole::System,
        }];
        if with_cold {
            drives.push(DriveInfo {
                mount: "D:\\continuum-cold".into(),
                total_bytes: 16_000 * GB,
                available_bytes: 15_000 * GB,
                role: DriveRole::Cold,
            });
        }
        SystemProfile::from_parts(
            HardwareClass {
                silicon: TargetSilicon::NvidiaCuda,
                silicon_model: "test".into(),
                vram_mb: vram_gb * 1024,
                system_ram_mb: 128 * 1024,
                power_source: PowerSource::Plugged,
                thermal_class: ThermalClass::Workstation,
                battery_pct: None,
                thermal_headroom_pct: None,
            },
            DeviceCapacity {
                gpu_total_bytes: vram_gb * GB,
                gpu_free_bytes_live: gpu_free_gb * GB,
                system_ram_free_bytes: 100 * GB,
            },
            drives,
            24,
        )
    }

    fn footprint(weights_gb: u64) -> ModelFootprint {
        ModelFootprint {
            model_id: "test-model".into(),
            weights_bytes: weights_gb * GB,
            kv_per_token: 1024, // ~2 MiB KV at the 2048 MIN_SERVE_CTX — weights dominate
            context_window: 32_768,
            capability_rank: 5,
        }
    }

    // what this catches: a model whose weights fit the 0.80 VRAM budget resolves to
    // LocalRecommended — served HERE. (BigMama: 32 GiB VRAM, 30 free → 24 GiB budget;
    // a 10 GiB model fits.)
    #[test]
    fn fitting_model_resolves_local_recommended() {
        let p = discrete(32, 30, true);
        let r = resolve_from_footprint(&p, Some(&footprint(10)), false, false);
        assert!(matches!(r, PlacementResolution::LocalRecommended { .. }), "got {r:?}");
    }

    // what this catches: THE NEVER-EXCLUDE INVARIANT. A model too big for THIS node
    // resolves to the grid when a peer fits, and DEGRADES (not excludes) when none
    // does — the same oversized model, two resolutions, never a wall.
    #[test]
    fn oversized_model_routes_to_grid_then_degrades_never_excludes() {
        let p = discrete(32, 30, true); // 24 GiB budget
        let huge = footprint(80); // 80 GiB weights — cannot fit locally

        let with_grid = resolve_from_footprint(&p, Some(&huge), false, true);
        assert!(matches!(with_grid, PlacementResolution::GridRouted { .. }), "got {with_grid:?}");

        let solo = resolve_from_footprint(&p, Some(&huge), false, false);
        assert!(matches!(solo, PlacementResolution::Degraded { .. }), "got {solo:?}");
        // The invariant: BOTH are answers. Neither errors, panics, or "excludes".
    }

    // what this catches: an un-provisioned model (no GGUF on disk) resolves to
    // PROVISION-to-cold-tier on a box with a big drive, and provision-to-system when
    // there's no cold drive — storage RESOLVES the destination, never gates the model.
    #[test]
    fn missing_artifact_provisions_to_cold_tier_when_available() {
        let with_cold = discrete(32, 30, true);
        assert_eq!(
            resolve_from_footprint(&with_cold, None, false, false),
            PlacementResolution::NeedsProvisioning { to_cold_tier: true }
        );

        let no_cold = discrete(8, 6, false);
        assert_eq!(
            resolve_from_footprint(&no_cold, None, false, false),
            PlacementResolution::NeedsProvisioning { to_cold_tier: false }
        );
    }

    // what this catches: a modest laptop (8 GiB VRAM, 6 free → ~4.8 GiB budget) and a
    // BigMama box run the SAME resolver over the SAME oversized model and BOTH get a
    // valid resolution — the laptop is DEGRADED, never excluded (solve-for-public-users).
    #[test]
    fn laptop_and_workstation_both_resolve_the_same_oversized_model() {
        let laptop = discrete(8, 6, false);
        let workstation = discrete(32, 30, true);
        let big = footprint(40);
        // Neither fits locally; with no grid, both DEGRADE (an answer), never exclude.
        assert!(matches!(
            resolve_from_footprint(&laptop, Some(&big), false, false),
            PlacementResolution::Degraded { .. }
        ));
        assert!(matches!(
            resolve_from_footprint(&workstation, Some(&big), false, false),
            PlacementResolution::Degraded { .. }
        ));
    }

    // what this catches: THE K3 ON-RAMP. An MoE model too big to fit whole in VRAM
    // resolves to MoePaged (experts frozen on the cold drive, hot subset paged) on a
    // box WITH a cold tier — served LOCALLY, not routed away — and correctly does NOT
    // when there's no cold tier (degrades/routes like any oversized model). This is
    // the difference between "594GB can't run here" and "594GB runs here by paging".
    #[test]
    fn oversized_moe_pages_locally_when_a_cold_tier_exists() {
        let big_moe = footprint(80); // 80 GiB — cannot fit a 24 GiB budget whole

        // Cold tier present → page the experts locally (the K3 magic), NOT grid/degrade.
        let with_cold = discrete(32, 30, true);
        assert!(
            matches!(
                resolve_from_footprint(&with_cold, Some(&big_moe), true, true),
                PlacementResolution::MoePaged { .. }
            ),
            "MoE + cold tier must page locally, preferred even over an available grid node"
        );

        // No cold tier → MoE has nowhere to freeze experts, so it resolves like any
        // oversized model: grid when a peer fits, degrade otherwise. Never excluded.
        let no_cold = discrete(8, 6, false);
        assert!(matches!(
            resolve_from_footprint(&no_cold, Some(&big_moe), true, true),
            PlacementResolution::GridRouted { .. }
        ));
        assert!(matches!(
            resolve_from_footprint(&no_cold, Some(&big_moe), true, false),
            PlacementResolution::Degraded { .. }
        ));

        // And an MoE that FITS whole just serves normally — paging is only for the
        // doesn't-fit case.
        let small_moe = footprint(10);
        assert!(matches!(
            resolve_from_footprint(&with_cold, Some(&small_moe), true, false),
            PlacementResolution::LocalRecommended { .. }
        ));
    }

    // what this catches: the grid verdict is a PER-NODE fit, never an aggregate pool
    // (the GridSnapshot doctrine). grid_has_fit is true iff SOME reachable peer's
    // live-free VRAM holds the model WHOLE — two small peers that "sum" to enough do
    // NOT fit, an unreachable peer that would fit does NOT count, and a peer that could
    // only CPU-spill does NOT count (fits_on_gpu = false).
    #[test]
    fn grid_has_fit_is_per_node_never_a_pool() {
        use crate::capacity::grid::{GridSnapshot, PeerCapacity};
        use crate::identity::PeerId;

        let peer = |id: u128, free_gb: u64, reachable: bool| PeerCapacity {
            peer: PeerId::from_u128(id),
            capacity: DeviceCapacity {
                gpu_total_bytes: 80 * GB,
                gpu_free_bytes_live: free_gb * GB,
                system_ram_free_bytes: 0,
            },
            reachable,
        };
        let local = DeviceCapacity {
            gpu_total_bytes: 32 * GB,
            gpu_free_bytes_live: 4 * GB,
            system_ram_free_bytes: 0,
        };
        let fp = footprint(40); // needs ~40 GiB resident whole

        // Two reachable 20 GiB peers — an aggregate POOL would "fit", per-node does NOT.
        let pooled = GridSnapshot {
            local,
            peers: vec![peer(1, 20, true), peer(2, 20, true)],
        };
        assert!(!grid_has_fit(&pooled, &fp), "two 20GiB peers must NOT fit a 40GiB model — never a pool");

        // One reachable peer with room fits.
        let has_big = GridSnapshot {
            local,
            peers: vec![peer(3, 20, true), peer(4, 60, true)],
        };
        assert!(grid_has_fit(&has_big, &fp));

        // A peer big enough but UNREACHABLE is a memory, not an offer.
        let unreachable = GridSnapshot {
            local,
            peers: vec![peer(5, 60, false)],
        };
        assert!(!grid_has_fit(&unreachable, &fp));
    }
}
