//! Division actuation — the serving_daemon's half of the resident/cache split (#2 of the
//! governor's VRAM-division rung; BigMama's `DivisionPolicy` in `expert_pager_policy::division`
//! is the brain, contract agreed 2026-08-03).
//!
//! This module is the PURE part: discover the `--resident-only` tier manifests the quantize
//! tool (#40) writes beside each override GGUF, derive the MoE shape from the serving
//! geometry, and turn the fork's trace-tail token counter into a measured decode-tok/s
//! reward. The daemon glue ([`DivisionActuator`]) owns the bandit and the sticky publish
//! band; `publish_moe_host_cache_lease` calls it once per tick and stamps the chosen
//! `resident_tier` onto the SAME governed plan file the fetcher mtime-polls.
//!
//! Division of authority (compression — one decision per concern): the LIVE device
//! budget stays #305's (free-VRAM-after-fit off the resource board, sticky-banded in the
//! daemon) — this module NEVER publishes a budget. It owns only the TIER choice: which
//! resident precision the next spawn/relaunch should load. When a relaunch adopts a
//! smaller resident, the board frees VRAM and #305's budget grows into it organically —
//! the loop closes through measured reality, not predicted arithmetic. Two-speed
//! contract: publishing `resident_tier` never triggers a relaunch ([[never-thrash]]);
//! it actuates only when a relaunch happens for its own reasons.

use std::path::{Path, PathBuf};
use std::time::Instant;

use expert_pager_policy::division::{
    feasible_divisions, CoverageModel, DivisionBandit, HardwareBudget, MoeShape, ResidentTier,
};

/// A discovered resident-precision tier: the policy-facing tier plus the artifact the
/// launcher loads when this tier is chosen. `gguf_path: None` = the model's own as-shipped
/// resident (tier 0, always present — serving it needs no override).
#[derive(Debug, Clone)]
pub struct ResidentTierArtifact {
    pub tier: ResidentTier,
    pub gguf_path: Option<PathBuf>,
}

/// Label the as-shipped resident carries in the tier catalog (index 0 by construction).
pub const NATIVE_TIER_LABEL: &str = "native";

/// Parse one `<out>.resident.json` sidecar the `--resident-only` quantize tool emits:
/// `{"resident_only":1,"tier_label":"<type>","resident_bytes":<N>}`. Tolerant key-scan —
/// unknown keys are ignored, but BOTH `tier_label` and a positive `resident_bytes` are
/// required (a manifest we can't price is refused, never guessed —
/// [[no-masking-fallbacks-my-style-tell]]).
pub fn parse_resident_manifest(json: &str) -> Option<ResidentTier> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let label = v.get("tier_label")?.as_str()?.trim();
    let resident_bytes = v.get("resident_bytes")?.as_u64()?;
    if label.is_empty() || resident_bytes == 0 {
        return None;
    }
    Some(ResidentTier {
        label: label.to_string(),
        resident_bytes,
    })
}

/// Discover the tier catalog for a model: the as-shipped resident first (index 0), then
/// every `*.resident.json` manifest under `dir` in path order (deterministic across runs).
/// Each manifest's artifact is the file it sits beside (`<out>.resident.json` → `<out>`);
/// a manifest whose GGUF vanished is dropped — a tier we can't load is not a tier.
pub fn discover_resident_tiers(dir: &Path, native_resident_bytes: u64) -> Vec<ResidentTierArtifact> {
    let mut out = vec![ResidentTierArtifact {
        tier: ResidentTier {
            label: NATIVE_TIER_LABEL.to_string(),
            resident_bytes: native_resident_bytes,
        },
        gguf_path: None,
    }];
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    let mut manifests: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".resident.json"))
        })
        .collect();
    manifests.sort();
    for manifest in manifests {
        let Ok(json) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        let Some(tier) = parse_resident_manifest(&json) else {
            continue;
        };
        // `<out>.resident.json` sits beside `<out>` — strip the sidecar suffix.
        let gguf = manifest
            .to_str()
            .and_then(|s| s.strip_suffix(".resident.json"))
            .map(PathBuf::from);
        match gguf {
            Some(g) if g.is_file() => out.push(ResidentTierArtifact {
                tier,
                gguf_path: Some(g),
            }),
            _ => continue,
        }
    }
    out
}

/// Per-token MoE shape from the live serving geometry ([`MoeServingContext`] fields).
/// `expert_bytes` is one expert record (Σ expert bytes ÷ record count); `experts_per_token`
/// is router top-k × layer count (K3-class: every block routes).
pub fn shape_from_geometry(
    expert_bytes_total: u64,
    n_layers: u32,
    n_experts_per_layer: u32,
    top_k: u32,
) -> Option<MoeShape> {
    let records = n_layers as u64 * n_experts_per_layer as u64;
    if records == 0 || expert_bytes_total == 0 {
        return None;
    }
    Some(MoeShape {
        expert_bytes: expert_bytes_total / records,
        experts_per_token: top_k as u64 * n_layers as u64,
    })
}

/// Warm-start priors for the offline tok/s predictor. These are PRIORS, not truths — the
/// bandit replaces them with the measured tok/s on the first real serve of each arm
/// (`DivisionBandit::observe`), so their only job is a sane initial ranking. Values are the
/// measured K3/5090 numbers behind `CoverageModel::k3_measured` (≈25 GB/s effective H2D,
/// ≈10 ms irreducible per-token compute).
pub const PRIOR_H2D_BPS: f64 = 25.0e9;
pub const PRIOR_COMPUTE_FLOOR_S: f64 = 0.010;

/// Reject reward samples built from fewer decode tokens than this — a 3-token tick delta
/// is timer noise, not a throughput measurement.
const MIN_REWARD_TOKENS: u64 = 64;

/// The daemon-held actuator: tier catalog + warm-started bandit + the trace-tail token
/// watermark the reward is measured against. Rebuilt whenever the active model changes;
/// `None` inside the daemon until a MoE serve with a tier catalog exists.
pub struct DivisionActuator {
    model_id: String,
    tiers: Vec<ResidentTierArtifact>,
    bandit: DivisionBandit,
    /// Last published tier label — the change detector for the plan-write axis.
    published: Option<String>,
    /// Trace-tail watermark for the measured-tok/s reward.
    last_tokens: u64,
    last_instant: Option<Instant>,
    /// Which tier the RUNNING serve actually loaded (index into `tiers`) — rewards are
    /// credited here, never to the bandit's latest choice (two-speed: a published tier
    /// takes effect only at relaunch, so choice ≠ served until then).
    served_tier_idx: usize,
}

impl DivisionActuator {
    /// Build for the active model: enumerate feasible divisions over the discovered tier
    /// catalog and warm-start every arm from the offline predictor. Returns `None` when no
    /// division is feasible (every tier's resident starves the cache) — the caller keeps
    /// the pre-division plan exactly as before.
    pub fn build(
        model_id: &str,
        tiers: Vec<ResidentTierArtifact>,
        hw: &HardwareBudget,
        shape: &MoeShape,
        coverage: &CoverageModel,
        served_resident_path: Option<&Path>,
    ) -> Option<Self> {
        let policy_tiers: Vec<ResidentTier> = tiers.iter().map(|t| t.tier.clone()).collect();
        let divisions = feasible_divisions(&policy_tiers, hw, shape);
        if divisions.is_empty() {
            return None;
        }
        let bandit =
            DivisionBandit::warm_start(divisions, coverage, shape, PRIOR_H2D_BPS, PRIOR_COMPUTE_FLOOR_S);
        let served_tier_idx = served_tier_index(&tiers, served_resident_path);
        Some(Self {
            model_id: model_id.to_string(),
            tiers,
            bandit,
            published: None,
            last_tokens: 0,
            last_instant: None,
            served_tier_idx,
        })
    }

    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    /// The tier to publish this tick: the bandit's argmax arm's label, plus whether it
    /// MOVED since the last publish (the plan-write trigger). The device budget is NOT
    /// returned — #305's live board-derived budget is the one budget authority.
    pub fn choose(&mut self) -> Option<(String, bool)> {
        let tier_idx = self.bandit.choose()?.tier_idx;
        let label = self.tiers.get(tier_idx)?.tier.label.clone();
        let moved = self.published.as_ref() != Some(&label);
        self.published = Some(label.clone());
        Some((label, moved))
    }

    /// Feed one tick's trace-tail token watermark. Computes decode tok/s from the delta
    /// since the previous tick and credits it to the SERVED tier's arm. Non-monotonic
    /// watermarks (fork restart) re-seed without rewarding. Returns the measured tok/s
    /// when a reward was recorded (for the probe).
    pub fn observe_tick(&mut self, tokens_observed: u64, now: Instant) -> Option<f64> {
        let prev_tokens = self.last_tokens;
        let prev_instant = self.last_instant.replace(now);
        self.last_tokens = tokens_observed;
        let prev_instant = prev_instant?;
        if tokens_observed < prev_tokens {
            return None; // trace reset (relaunch) — re-seed the watermark
        }
        let delta = tokens_observed - prev_tokens;
        if delta < MIN_REWARD_TOKENS {
            return None; // idle or noise-sized sample
        }
        let elapsed = now.duration_since(prev_instant).as_secs_f64();
        if elapsed <= 0.0 {
            return None;
        }
        let tok_s = delta as f64 / elapsed;
        let tier_idx = self.tiers.get(self.served_tier_idx).map(|_| self.served_tier_idx)?;
        self.bandit.observe(tier_idx, tok_s);
        Some(tok_s)
    }

    /// Re-resolve which tier the running serve loaded (called when the daemon learns the
    /// spawn's resident override changed).
    pub fn set_served_resident(&mut self, path: Option<&Path>) {
        self.served_tier_idx = served_tier_index(&self.tiers, path);
    }

    pub fn served_tier_label(&self) -> &str {
        self.tiers
            .get(self.served_tier_idx)
            .map(|t| t.tier.label.as_str())
            .unwrap_or(NATIVE_TIER_LABEL)
    }
}

/// Map the spawn's applied resident-override path back to its catalog index; no override
/// (or an unknown path) = the native tier at index 0.
fn served_tier_index(tiers: &[ResidentTierArtifact], path: Option<&Path>) -> usize {
    let Some(path) = path else {
        return 0;
    };
    tiers
        .iter()
        .position(|t| t.gguf_path.as_deref() == Some(path))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k3_hw() -> HardwareBudget {
        HardwareBudget {
            vram_total_bytes: 32 * 1024 * 1024 * 1024,
            kv_bytes: 0,
            compute_reserve_bytes: 2 * 1024 * 1024 * 1024,
        }
    }

    fn k3_shape() -> MoeShape {
        MoeShape {
            expert_bytes: 8_093_696,
            experts_per_token: 1472,
        }
    }

    fn write_tier(dir: &Path, stem: &str, label: &str, bytes: u64) -> PathBuf {
        let gguf = dir.join(format!("{stem}.gguf"));
        std::fs::write(&gguf, b"gguf").unwrap();
        std::fs::write(
            dir.join(format!("{stem}.gguf.resident.json")),
            format!(r#"{{"resident_only":1,"tier_label":"{label}","resident_bytes":{bytes}}}"#),
        )
        .unwrap();
        gguf
    }

    // what this catches: the manifest contract with the --resident-only quantize tool
    // (#40) — tolerant of extra keys, but refuses a manifest missing either priced field.
    #[test]
    fn manifest_parse_is_tolerant_but_requires_priced_fields() {
        let ok = parse_resident_manifest(
            r#"{"resident_only":1,"tier_label":"q4_K","resident_bytes":123,"extra":"x"}"#,
        )
        .unwrap();
        assert_eq!(ok.label, "q4_K");
        assert_eq!(ok.resident_bytes, 123);
        assert!(parse_resident_manifest(r#"{"tier_label":"q4_K"}"#).is_none());
        assert!(parse_resident_manifest(r#"{"resident_bytes":123}"#).is_none());
        assert!(parse_resident_manifest(r#"{"tier_label":"","resident_bytes":123}"#).is_none());
        assert!(parse_resident_manifest("not json").is_none());
    }

    // what this catches: discovery yields native at index 0, manifests in path order, and
    // drops a manifest whose GGUF vanished — the launcher must never be handed a tier it
    // cannot load.
    #[test]
    fn discovery_orders_tiers_and_drops_orphan_manifests() {
        let dir = tempfile::tempdir().unwrap();
        write_tier(dir.path(), "a-q3", "q3_K", 16_000_000_000);
        write_tier(dir.path(), "b-q4", "q4_K", 25_000_000_000);
        // orphan: manifest without its GGUF
        std::fs::write(
            dir.path().join("c-q5.gguf.resident.json"),
            r#"{"resident_only":1,"tier_label":"q5_K","resident_bytes":28000000000}"#,
        )
        .unwrap();
        let tiers = discover_resident_tiers(dir.path(), 30_000_000_000);
        let labels: Vec<&str> = tiers.iter().map(|t| t.tier.label.as_str()).collect();
        assert_eq!(labels, vec![NATIVE_TIER_LABEL, "q3_K", "q4_K"]);
        assert!(tiers[0].gguf_path.is_none());
        assert!(tiers[1].gguf_path.as_ref().unwrap().is_file());
    }

    // what this catches: the reward loop's honesty properties — no reward on the seed
    // tick, none on idle/noise deltas, a real decode delta credits the SERVED arm (not the
    // bandit's chosen arm), and a watermark reset (fork relaunch) re-seeds silently.
    #[test]
    fn observe_tick_rewards_served_arm_from_real_deltas_only() {
        let dir = tempfile::tempdir().unwrap();
        write_tier(dir.path(), "a-q3", "q3_K", 16 * 1024 * 1024 * 1024);
        let tiers = discover_resident_tiers(dir.path(), 25 * 1024 * 1024 * 1024);
        let shape = k3_shape();
        let mut act = DivisionActuator::build(
            "k3",
            tiers,
            &k3_hw(),
            &shape,
            &CoverageModel::k3_measured(),
            None, // serving the native resident
        )
        .unwrap();
        let t0 = Instant::now();
        assert!(act.observe_tick(1000, t0).is_none(), "seed tick never rewards");
        assert!(
            act.observe_tick(1010, t0 + std::time::Duration::from_secs(5)).is_none(),
            "10-token delta is noise, not a measurement"
        );
        let tok_s = act
            .observe_tick(1010 + 500, t0 + std::time::Duration::from_secs(15))
            .expect("500-token decode delta is a real sample");
        assert!(tok_s > 0.0);
        // reward went to the SERVED (native, idx 0) arm — its value is now the measured
        // number, not the offline prior
        assert_eq!(act.served_tier_label(), NATIVE_TIER_LABEL);
        // watermark reset re-seeds without panicking or rewarding
        assert!(act
            .observe_tick(50, t0 + std::time::Duration::from_secs(20))
            .is_none());
    }

    // what this catches: the publish contract — the first choice publishes (moved), an
    // identical re-choice does NOT (no plan churn), and the warm-start prior prefers the
    // tier that frees more cache (a smaller resident). If the preference inverts, the
    // governor "optimizes" toward starving the cache.
    #[test]
    fn choose_reports_moved_only_on_real_change() {
        let dir = tempfile::tempdir().unwrap();
        write_tier(dir.path(), "a-q3", "q3_K", 16 * 1024 * 1024 * 1024);
        let tiers = discover_resident_tiers(dir.path(), 25 * 1024 * 1024 * 1024);
        let shape = k3_shape();
        let mut act = DivisionActuator::build(
            "k3",
            tiers,
            &k3_hw(),
            &shape,
            &CoverageModel::k3_measured(),
            None,
        )
        .unwrap();
        let (label, moved) = act.choose().unwrap();
        assert!(moved, "first publish must report moved");
        // q3 frees more cache than native → the warm-start prior picks it
        assert_eq!(label, "q3_K");
        let (label2, moved2) = act.choose().unwrap();
        assert_eq!(label, label2);
        assert!(!moved2, "identical re-choice must not churn the plan");
    }
}
