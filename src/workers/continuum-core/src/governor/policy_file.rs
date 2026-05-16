//! TOML policy file loader (Lane H PR-2, substrate-governor: policy_file).
//!
//! PR-1 (`types.rs`) shipped `GovernorPolicy` as the published shape.
//! This PR-2 reads a TOML file matching the schema in
//! GENOME-FOUNDRY-SENTINEL.md Part 11 "Policy File Format" and
//! converts it to a `GovernorPolicy`. The governor watches the file
//! and reloads on change (file watcher in PR-3); this PR ships the
//! parse + validate layer that powers the watch.
//!
//! ## Schema
//!
//! Per the spec, a policy file looks like:
//!
//! ```toml
//! policy_version = 3
//! applies_to    = "apple-m,thinandlight,uma,vram_mb=0..0,ram_mb=14000..18000"
//!
//! [tier_sizes]
//! l1_lora_layers       = 2
//! l1_kv_tokens         = 2048
//! l2_lora_layers       = 4
//! l3_lora_layers       = 12
//! l3_engrams           = 1024
//!
//! [cadence_multipliers]
//! realtime             = 1.0
//! delayed              = 1.5
//! background           = 2.0
//!
//! [concurrency_caps]
//! personas_concurrent  = 2
//! inference_lanes      = 1
//! foundry_lanes        = 0
//! sentinel_lanes       = 1
//!
//! [speculation]
//! level                = "conservative"
//!
//! [consolidation]
//! schedule             = "idle_plugged_in"
//!
//! [federation]
//! pull_cadence_seconds = 600
//!
//! [recall_weights]
//! semantic             = 0.4
//! outcome_history      = 0.3
//! recency              = 0.1
//! tier_proximity       = 0.1
//! provenance_trust     = 0.1
//! ```
//!
//! Files live under `~/.continuum/policy/` and are named by the
//! hardware-class fingerprint they apply to (e.g.
//! `apple-m-thinandlight-16gb-uma.toml`). PR-3 wires the selection
//! logic; PR-2 (this) just parses.
//!
//! ## What this PR DOES NOT do
//!
//! - File system watch / hot reload (PR-3 wires `notify` crate).
//! - Policy file SELECTION based on HardwareClass fingerprint (PR-3).
//! - Cascade state machine + threshold logic (PR-3).
//! - Merging `local.toml` overlay (PR-3 — overlay format spec'd
//!   inline below for forward-compat).
//! - PressureBroker subscription (PR-4).
//!
//! ## Failure-mode discipline
//!
//! Same posture as `inference_capability::gguf_loader` (PR-2 of
//! PIECE-5): every required field returns typed Err on missing/
//! malformed; no silent defaults. The recall_weights validation
//! enforces sum-to-near-1.0 (within 1% tolerance) — silently
//! accepting wildly unbalanced weights would produce garbage
//! ranked-pool scoring.

use crate::governor::types::{
    CadenceMultipliers, ConcurrencyCaps, ConsolidationSchedule, FederationCadence, GovernorPolicy,
    HardwareClass, RecallScoreWeights, SpeculationLevel, TierSizes,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// On-disk TOML shape — a flatter version of `GovernorPolicy` matching
/// the format engineers tune by hand. Sections become nested structs
/// for serde; the loader assembles the final `GovernorPolicy` from
/// this + a caller-supplied `HardwareClass` (the policy file doesn't
/// know its own hardware class beyond a free-form `applies_to` tag).
///
/// File-format structs use snake_case (TOML idiom + matches the
/// hand-edited spec); wire-format structs use camelCase (TS idiom).
/// The file-format → wire-format hop happens in `into_governor_policy`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PolicyFile {
    pub policy_version: u64,
    /// Free-form fingerprint expression — purely informational in PR-2.
    /// PR-3 implements the match logic that picks WHICH policy file
    /// applies to the current `HardwareClass`.
    pub applies_to: String,
    pub tier_sizes: TierSizesFile,
    pub cadence_multipliers: CadenceMultipliersFile,
    pub concurrency_caps: ConcurrencyCapsFile,
    pub speculation: SpeculationFileSection,
    pub consolidation: ConsolidationFileSection,
    pub federation: FederationCadenceFile,
    pub recall_weights: RecallScoreWeightsFile,
}

/// File-format tier sizes (snake_case for TOML). Converts to wire-
/// format `TierSizes` (camelCase for TS) in `into_governor_policy`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct TierSizesFile {
    pub l1_lora_layers: u32,
    pub l1_kv_tokens: u32,
    pub l2_lora_layers: u32,
    pub l3_lora_layers: u32,
    pub l3_engrams: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct CadenceMultipliersFile {
    pub realtime: f32,
    pub delayed: f32,
    pub background: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct ConcurrencyCapsFile {
    pub personas_concurrent: u32,
    pub inference_lanes: u32,
    pub foundry_lanes: u32,
    pub sentinel_lanes: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct FederationCadenceFile {
    pub pull_cadence_seconds: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct RecallScoreWeightsFile {
    pub semantic: f32,
    pub outcome_history: f32,
    pub recency: f32,
    pub tier_proximity: f32,
    pub provenance_trust: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct SpeculationFileSection {
    pub level: SpeculationLevel,
    // Future fields: max_branches, min_idle_slack_pct, miss_rate_throttle.
    // Spec'd in GENOME-FOUNDRY-SENTINEL.md; PR-3 wires the cascade
    // logic that uses them.
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct ConsolidationFileSection {
    pub schedule: ConsolidationSchedule,
    // Future fields: min_idle_seconds, preempt_on_pressure.
}

/// Errors the policy file loader can surface. All typed (no silent
/// default-on-error); caller decides whether to abort startup,
/// retry after an operator fix, or use an explicitly configured
/// built-in policy.
#[derive(Debug)]
pub enum PolicyFileError {
    Io(std::io::Error),
    /// TOML parse error — file is syntactically broken.
    Toml(toml::de::Error),
    /// Recall weights don't sum to 1.0 within the tolerance. The
    /// spec says the file's [recall_weights] should sum to 1.0; a
    /// large drift means someone edited a field and forgot to balance.
    /// Refuse rather than silently scale.
    RecallWeightsImbalanced {
        sum: f32,
        tolerance: f32,
    },
    /// A tier size is zero where it shouldn't be (l1_lora_layers = 0
    /// means no LoRA caching at all — likely a typo, not intent).
    InvalidTierSize {
        field: &'static str,
        value: u32,
    },
    /// Cadence multiplier under 1.0 — would speed UP a class rather
    /// than slow down. Almost certainly a typo.
    InvalidCadenceMultiplier {
        field: &'static str,
        value: f32,
    },
}

impl std::fmt::Display for PolicyFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PolicyFileError::Io(e) => write!(f, "policy file I/O: {e}"),
            PolicyFileError::Toml(e) => write!(f, "policy file TOML parse: {e}"),
            PolicyFileError::RecallWeightsImbalanced { sum, tolerance } => write!(
                f,
                "policy [recall_weights] sum to {sum}, expected 1.0 \
                 within ±{tolerance}. Edit the weights to balance, or document \
                 why the deliberate imbalance is correct."
            ),
            PolicyFileError::InvalidTierSize { field, value } => write!(
                f,
                "policy [tier_sizes].{field} = {value} — must be > 0. \
                 Zero means the tier is disabled, which the governor doesn't \
                 currently support."
            ),
            PolicyFileError::InvalidCadenceMultiplier { field, value } => write!(
                f,
                "policy [cadence_multipliers].{field} = {value} — must be >= 1.0. \
                 A multiplier below 1.0 would speed up the cadence rather than \
                 slow it down, which is almost certainly a typo."
            ),
        }
    }
}

impl std::error::Error for PolicyFileError {}

impl From<std::io::Error> for PolicyFileError {
    fn from(e: std::io::Error) -> Self {
        PolicyFileError::Io(e)
    }
}

impl From<toml::de::Error> for PolicyFileError {
    fn from(e: toml::de::Error) -> Self {
        PolicyFileError::Toml(e)
    }
}

/// Tolerance for the recall_weights-sum-to-1.0 check. 1% — wider than
/// floating-point noise, narrower than what would silently distort
/// scoring outcomes.
pub const RECALL_WEIGHTS_TOLERANCE: f32 = 0.01;

/// Load + validate a policy TOML file from a path.
///
/// Pure-composition: file open → TOML parse → validate. Each step
/// returns typed Err. The caller wraps the parsed `PolicyFile` into a
/// `GovernorPolicy` via `into_governor_policy` (which needs a
/// `HardwareClass` the policy file doesn't carry).
pub fn load_policy_file(path: &Path) -> Result<PolicyFile, PolicyFileError> {
    let text = std::fs::read_to_string(path)?;
    parse_policy_text(&text)
}

/// Pure parser — separated for testability without disk I/O.
pub fn parse_policy_text(text: &str) -> Result<PolicyFile, PolicyFileError> {
    let file: PolicyFile = toml::from_str(text)?;
    validate(&file)?;
    Ok(file)
}

/// Validate semantic constraints the type system can't express.
fn validate(file: &PolicyFile) -> Result<(), PolicyFileError> {
    // Recall weights sum to ~1.0 within tolerance.
    let w = &file.recall_weights;
    let sum = w.semantic + w.outcome_history + w.recency + w.tier_proximity + w.provenance_trust;
    if (sum - 1.0).abs() > RECALL_WEIGHTS_TOLERANCE {
        return Err(PolicyFileError::RecallWeightsImbalanced {
            sum,
            tolerance: RECALL_WEIGHTS_TOLERANCE,
        });
    }

    // Tier sizes must be > 0 — zero means "disabled," which the
    // governor doesn't currently support.
    if file.tier_sizes.l1_lora_layers == 0 {
        return Err(PolicyFileError::InvalidTierSize {
            field: "l1_lora_layers",
            value: 0,
        });
    }
    if file.tier_sizes.l1_kv_tokens == 0 {
        return Err(PolicyFileError::InvalidTierSize {
            field: "l1_kv_tokens",
            value: 0,
        });
    }
    if file.tier_sizes.l2_lora_layers == 0 {
        return Err(PolicyFileError::InvalidTierSize {
            field: "l2_lora_layers",
            value: 0,
        });
    }
    if file.tier_sizes.l3_lora_layers == 0 {
        return Err(PolicyFileError::InvalidTierSize {
            field: "l3_lora_layers",
            value: 0,
        });
    }
    if file.tier_sizes.l3_engrams == 0 {
        return Err(PolicyFileError::InvalidTierSize {
            field: "l3_engrams",
            value: 0,
        });
    }

    // Cadence multipliers >= 1.0 (matches docstring: 1.0 = unchanged,
    // > 1.0 = slowed). < 1.0 would speed up, almost certainly typo.
    let c = &file.cadence_multipliers;
    for (name, val) in [
        ("realtime", c.realtime),
        ("delayed", c.delayed),
        ("background", c.background),
    ] {
        if val < 1.0 {
            return Err(PolicyFileError::InvalidCadenceMultiplier {
                field: match name {
                    "realtime" => "realtime",
                    "delayed" => "delayed",
                    _ => "background",
                },
                value: val,
            });
        }
    }

    Ok(())
}

/// Assemble a `GovernorPolicy` from a parsed `PolicyFile` + the
/// caller's `HardwareClass` + a timestamp. The policy file doesn't
/// carry its own hardware class beyond a free-form `applies_to` tag;
/// the governor's policy-selection layer (PR-3) decides which file
/// matches the current class, then calls this to produce the final
/// `GovernorPolicy`.
pub fn into_governor_policy(
    file: PolicyFile,
    hardware_class: HardwareClass,
    committed_at_ms: u64,
) -> GovernorPolicy {
    GovernorPolicy {
        policy_version: file.policy_version,
        hardware_class,
        tier_sizes: TierSizes {
            l1_lora_layers: file.tier_sizes.l1_lora_layers,
            l1_kv_tokens: file.tier_sizes.l1_kv_tokens,
            l2_lora_layers: file.tier_sizes.l2_lora_layers,
            l3_lora_layers: file.tier_sizes.l3_lora_layers,
            l3_engrams: file.tier_sizes.l3_engrams,
        },
        cadence_multipliers: CadenceMultipliers {
            realtime: file.cadence_multipliers.realtime,
            delayed: file.cadence_multipliers.delayed,
            background: file.cadence_multipliers.background,
        },
        concurrency_caps: ConcurrencyCaps {
            personas_concurrent: file.concurrency_caps.personas_concurrent,
            inference_lanes: file.concurrency_caps.inference_lanes,
            foundry_lanes: file.concurrency_caps.foundry_lanes,
            sentinel_lanes: file.concurrency_caps.sentinel_lanes,
        },
        speculation_aggressiveness: file.speculation.level,
        consolidation_schedule: file.consolidation.schedule,
        federation_pull_cadence: FederationCadence {
            pull_cadence_seconds: file.federation.pull_cadence_seconds,
        },
        recall_score_weights: RecallScoreWeights {
            semantic: file.recall_weights.semantic,
            outcome_history: file.recall_weights.outcome_history,
            recency: file.recall_weights.recency,
            tier_proximity: file.recall_weights.tier_proximity,
            provenance_trust: file.recall_weights.provenance_trust,
        },
        cascade_step: 0,
        committed_at_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governor::types::{classify_hardware, PowerSource, ThermalClass};
    use crate::inference_capability::types::HardwareProfile;

    // Canonical valid policy text — matches the spec's M-Air example.
    const VALID_AIR_POLICY: &str = r#"
policy_version = 3
applies_to    = "apple-m,thinandlight,uma,vram_mb=0..0,ram_mb=14000..18000"

[tier_sizes]
l1_lora_layers       = 2
l1_kv_tokens         = 2048
l2_lora_layers       = 4
l3_lora_layers       = 12
l3_engrams           = 1024

[cadence_multipliers]
realtime             = 1.0
delayed              = 1.5
background           = 2.0

[concurrency_caps]
personas_concurrent  = 2
inference_lanes      = 1
foundry_lanes        = 0
sentinel_lanes       = 1

[speculation]
level                = "conservative"

[consolidation]
schedule             = "idle-plugged-in"

[federation]
pull_cadence_seconds = 600

[recall_weights]
semantic             = 0.4
outcome_history      = 0.3
recency              = 0.1
tier_proximity       = 0.1
provenance_trust     = 0.1
"#;

    // Canonical 5090 policy — same schema, larger numbers.
    const VALID_5090_POLICY: &str = r#"
policy_version = 1
applies_to     = "nvidia,workstation,vram_mb=30000..36000,ram_mb=60000..80000"

[tier_sizes]
l1_lora_layers        = 8
l1_kv_tokens          = 16384
l2_lora_layers        = 16
l3_lora_layers        = 40
l3_engrams            = 10240

[cadence_multipliers]
realtime              = 1.0
delayed               = 1.0
background            = 1.5

[concurrency_caps]
personas_concurrent   = 8
inference_lanes       = 4
foundry_lanes         = 1
sentinel_lanes        = 2

[speculation]
level                 = "aggressive"

[consolidation]
schedule              = "idle"

[federation]
pull_cadence_seconds  = 60

[recall_weights]
semantic              = 0.4
outcome_history       = 0.3
recency               = 0.1
tier_proximity        = 0.1
provenance_trust      = 0.1
"#;

    // ===== happy paths =====

    /// What this catches: canonical M-Air policy parses + validates.
    /// If this regresses, no Mac runs through the loader at all.
    #[test]
    fn air_policy_parses_and_validates() {
        let file = parse_policy_text(VALID_AIR_POLICY).expect("valid Air policy should parse");
        assert_eq!(file.policy_version, 3);
        assert_eq!(file.tier_sizes.l1_lora_layers, 2);
        assert_eq!(file.tier_sizes.l1_kv_tokens, 2048);
        assert_eq!(file.cadence_multipliers.background, 2.0);
        assert_eq!(file.concurrency_caps.personas_concurrent, 2);
        assert_eq!(file.speculation.level, SpeculationLevel::Conservative);
        assert_eq!(
            file.consolidation.schedule,
            ConsolidationSchedule::IdlePluggedIn
        );
        assert_eq!(file.federation.pull_cadence_seconds, 600);
    }

    /// What this catches: canonical Blackwell 5090 policy parses +
    /// validates. Same schema, larger numbers — pins that the loader
    /// scales across the hardware range without code changes.
    #[test]
    fn blackwell_policy_parses_and_validates() {
        let file = parse_policy_text(VALID_5090_POLICY).expect("valid 5090 policy should parse");
        assert_eq!(file.tier_sizes.l1_lora_layers, 8);
        assert_eq!(file.tier_sizes.l1_kv_tokens, 16384);
        assert_eq!(file.concurrency_caps.personas_concurrent, 8);
        assert_eq!(file.speculation.level, SpeculationLevel::Aggressive);
    }

    // ===== validation rules =====

    /// What this catches: recall_weights summing to far-from-1.0
    /// returns RecallWeightsImbalanced. The whole point of the
    /// weights is a normalized prior over scoring factors; silently
    /// accepting 0.1/0.1/0.1/0.1/0.1 (sum=0.5) would halve every
    /// score with no signal to the user.
    #[test]
    fn imbalanced_recall_weights_rejected() {
        let bad =
            VALID_AIR_POLICY.replace("semantic             = 0.4", "semantic             = 0.1");
        let result = parse_policy_text(&bad);
        match result {
            Err(PolicyFileError::RecallWeightsImbalanced { sum, .. }) => {
                assert!((sum - 0.7).abs() < 0.01, "sum should be 0.7, got {sum}");
            }
            other => panic!("expected RecallWeightsImbalanced, got {other:?}"),
        }
    }

    /// What this catches: recall_weights summing to EXACTLY 1.0 passes.
    /// Boundary check for the tolerance.
    #[test]
    fn recall_weights_sum_to_one_accepted() {
        let file = parse_policy_text(VALID_AIR_POLICY).expect("valid Air policy should parse");
        let w = &file.recall_weights;
        let sum =
            w.semantic + w.outcome_history + w.recency + w.tier_proximity + w.provenance_trust;
        assert!((sum - 1.0).abs() < RECALL_WEIGHTS_TOLERANCE);
    }

    /// What this catches: tier_size = 0 (l1_lora_layers) returns
    /// InvalidTierSize. Catches "I'll disable this for now" intent
    /// that the loader doesn't currently support.
    #[test]
    fn zero_l1_lora_layers_rejected() {
        let bad = VALID_AIR_POLICY.replace("l1_lora_layers       = 2", "l1_lora_layers       = 0");
        match parse_policy_text(&bad) {
            Err(PolicyFileError::InvalidTierSize { field, value }) => {
                assert_eq!(field, "l1_lora_layers");
                assert_eq!(value, 0);
            }
            other => panic!("expected InvalidTierSize, got {other:?}"),
        }
    }

    /// What this catches: zero on any tier-size field is rejected.
    /// Tests every field one at a time so a future addition to the
    /// validation list catches via test discovery, not by review.
    #[test]
    fn zero_any_tier_size_rejected() {
        for field in &[
            "l1_kv_tokens         = 2048",
            "l2_lora_layers       = 4",
            "l3_lora_layers       = 12",
            "l3_engrams           = 1024",
        ] {
            let parts: Vec<&str> = field.split('=').collect();
            let zeroed = format!("{}= 0", parts[0]);
            let bad = VALID_AIR_POLICY.replace(field, &zeroed);
            let result = parse_policy_text(&bad);
            assert!(
                matches!(result, Err(PolicyFileError::InvalidTierSize { .. })),
                "field {field} = 0 should be rejected; got {result:?}"
            );
        }
    }

    /// What this catches: cadence_multiplier < 1.0 returns
    /// InvalidCadenceMultiplier. Likely a typo (someone meant 1.5,
    /// typed 0.5) that would speed up cadence to 2× normal rather
    /// than slow it down to 1/2.
    #[test]
    fn cadence_multiplier_under_one_rejected() {
        let bad =
            VALID_AIR_POLICY.replace("delayed              = 1.5", "delayed              = 0.5");
        match parse_policy_text(&bad) {
            Err(PolicyFileError::InvalidCadenceMultiplier { field, value }) => {
                assert_eq!(field, "delayed");
                assert_eq!(value, 0.5);
            }
            other => panic!("expected InvalidCadenceMultiplier, got {other:?}"),
        }
    }

    /// What this catches: cadence_multiplier = 1.0 exactly passes
    /// (boundary). 1.0 = "unchanged from realtime"; valid.
    #[test]
    fn cadence_multiplier_exactly_one_accepted() {
        let file = parse_policy_text(VALID_AIR_POLICY).expect("valid Air policy should parse");
        assert_eq!(file.cadence_multipliers.realtime, 1.0);
    }

    // ===== into_governor_policy =====

    /// What this catches: into_governor_policy correctly composes the
    /// PolicyFile + HardwareClass + timestamp into the published
    /// GovernorPolicy. Smoke test for the assembly.
    #[test]
    fn into_governor_policy_composes_correctly() {
        let file = parse_policy_text(VALID_AIR_POLICY).expect("valid Air policy should parse");
        let hw_profile = HardwareProfile {
            platform: "macos-arm64-air".into(),
            has_metal: true,
            has_cuda: false,
            has_vulkan: false,
            free_vram_bytes: 5 * 1024 * 1024 * 1024,
            total_vram_bytes: 8 * 1024 * 1024 * 1024,
            cpu_cores: 8,
            system_ram_bytes: 16 * 1024 * 1024 * 1024,
        };
        let hw_class = classify_hardware(&hw_profile);
        let policy = into_governor_policy(file, hw_class.clone(), 1_715_625_600_000);

        assert_eq!(policy.policy_version, 3);
        assert_eq!(policy.hardware_class, hw_class);
        assert_eq!(policy.tier_sizes.l1_lora_layers, 2);
        assert_eq!(policy.cadence_multipliers.background, 2.0);
        assert_eq!(
            policy.speculation_aggressiveness,
            SpeculationLevel::Conservative
        );
        assert_eq!(
            policy.consolidation_schedule,
            ConsolidationSchedule::IdlePluggedIn
        );
        // cascade_step always starts at 0 (normal); PR-3 updates under pressure
        assert_eq!(policy.cascade_step, 0);
        assert_eq!(policy.committed_at_ms, 1_715_625_600_000);
    }

    // ===== load_policy_file (I/O) =====

    /// What this catches: load_policy_file on a real on-disk TOML file
    /// works end-to-end. I/O smoke test for the wrapper.
    #[test]
    fn load_policy_file_reads_valid_file() {
        let tmp = tempfile::NamedTempFile::new().expect("temp policy file should be creatable");
        std::fs::write(tmp.path(), VALID_AIR_POLICY).expect("temp policy file should be writable");
        let file = load_policy_file(tmp.path()).expect("valid policy file should load");
        assert_eq!(file.policy_version, 3);
    }

    /// What this catches: load_policy_file on a non-existent path
    /// returns PolicyFileError::Io. Defensive — caller decides whether
    /// to abort or require an explicitly configured built-in policy.
    #[test]
    fn load_policy_file_nonexistent_path_returns_io_err() {
        let result = load_policy_file(Path::new("/nonexistent/policy.toml"));
        assert!(matches!(result, Err(PolicyFileError::Io(_))));
    }

    /// What this catches: load_policy_file on a syntactically broken
    /// TOML file returns PolicyFileError::Toml. Important — silent
    /// substituting a default would mask config bugs.
    #[test]
    fn load_policy_file_invalid_toml_returns_toml_err() {
        let tmp = tempfile::NamedTempFile::new().expect("temp policy file should be creatable");
        std::fs::write(tmp.path(), "this is not valid toml [[[")
            .expect("temp policy file should be writable");
        let result = load_policy_file(tmp.path());
        assert!(matches!(result, Err(PolicyFileError::Toml(_))));
    }

    // ===== PolicyFileError trait =====

    /// What this catches: PolicyFileError implements Display + Error
    /// with informative messages. Diagnostic value — operator sees
    /// exactly what's wrong in the log.
    #[test]
    fn policy_file_error_display_includes_context() {
        let err = PolicyFileError::RecallWeightsImbalanced {
            sum: 0.7,
            tolerance: 0.01,
        };
        let display = format!("{err}");
        assert!(display.contains("0.7"));
        assert!(display.contains("1.0"));
        let _: &dyn std::error::Error = &err;
    }

    // ===== From impls =====

    /// What this catches: From<io::Error> + From<toml::de::Error>
    /// for PolicyFileError. Lets callers use `?` to propagate without
    /// manual .map_err().
    #[test]
    fn policy_file_error_from_io_and_toml() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let pf_err: PolicyFileError = io_err.into();
        assert!(matches!(pf_err, PolicyFileError::Io(_)));

        let toml_err = toml::from_str::<PolicyFile>("not valid")
            .expect_err("invalid TOML should produce a parser error");
        let pf_err: PolicyFileError = toml_err.into();
        assert!(matches!(pf_err, PolicyFileError::Toml(_)));
    }

    /// What this catches: the spec's SpeculationLevel kebab-case
    /// ("conservative" / "balanced" / "aggressive" / "off") parses
    /// correctly. Wire stability — operators edit these strings in
    /// TOML by hand.
    #[test]
    fn speculation_level_string_parses() {
        for (s, expected) in &[
            ("conservative", SpeculationLevel::Conservative),
            ("balanced", SpeculationLevel::Balanced),
            ("aggressive", SpeculationLevel::Aggressive),
            ("off", SpeculationLevel::Off),
        ] {
            let text = VALID_AIR_POLICY.replace("\"conservative\"", &format!("\"{s}\""));
            let file = parse_policy_text(&text).expect("speculation level should parse");
            assert_eq!(file.speculation.level, *expected, "level={s}");
        }
    }

    /// What this catches: ConsolidationSchedule kebab-case
    /// ("always" / "idle" / "idle-plugged-in" / "manual") parses.
    /// Same wire-stability concern as SpeculationLevel.
    #[test]
    fn consolidation_schedule_string_parses() {
        for (s, expected) in &[
            ("always", ConsolidationSchedule::Always),
            ("idle", ConsolidationSchedule::Idle),
            ("idle-plugged-in", ConsolidationSchedule::IdlePluggedIn),
            ("manual", ConsolidationSchedule::Manual),
        ] {
            let text = VALID_AIR_POLICY.replace("\"idle-plugged-in\"", &format!("\"{s}\""));
            let file = parse_policy_text(&text).expect("consolidation schedule should parse");
            assert_eq!(file.consolidation.schedule, *expected, "schedule={s}");
        }
    }

    /// What this catches: classify_hardware + into_governor_policy
    /// compose end-to-end. The full path: hw_probe → classify →
    /// load_policy → into_policy → published GovernorPolicy.
    #[test]
    fn full_pipeline_hw_probe_to_governor_policy() {
        // Synthesize an M5 Pro hw_profile
        let hw_profile = HardwareProfile {
            platform: "macos-arm64-m5pro".into(),
            has_metal: true,
            has_cuda: false,
            has_vulkan: false,
            free_vram_bytes: 32 * 1024 * 1024 * 1024,
            total_vram_bytes: 48 * 1024 * 1024 * 1024,
            cpu_cores: 16,
            system_ram_bytes: 64 * 1024 * 1024 * 1024,
        };
        // 1. classify hardware
        let hw_class = classify_hardware(&hw_profile);
        assert_eq!(hw_class.thermal_class, ThermalClass::Workstation);
        assert_eq!(hw_class.power_source, PowerSource::Plugged);
        // 2. parse policy (in PR-3 the selection logic picks the
        //    right file based on hw_class; here we use the M-Air
        //    file as a stand-in)
        let file = parse_policy_text(VALID_AIR_POLICY).expect("valid Air policy should parse");
        // 3. compose
        let policy = into_governor_policy(file, hw_class, 1_715_625_600_000);
        assert_eq!(policy.policy_version, 3);
        assert_eq!(policy.cascade_step, 0);
    }
}
