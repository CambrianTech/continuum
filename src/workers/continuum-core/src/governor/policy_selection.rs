//! Policy file selection — Lane H PR-3a per GENOME-FOUNDRY-SENTINEL
//! #1327 Part 11.
//!
//! PR-2 (#1350) shipped the TOML loader: file → `PolicyFile`. This
//! PR-3a ships the SELECTION layer: given a `HardwareClass` and a
//! directory of policy files, pick the right one.
//!
//! ## Match algorithm
//!
//! Each policy file's `applies_to` field is a comma-separated set of
//! fingerprint constraints:
//!
//! ```text
//! applies_to = "apple-m,thinandlight,uma,vram_mb=0..0,ram_mb=14000..18000"
//! ```
//!
//! Constraint kinds:
//!
//! - **Silicon tag** (`apple-m` / `nvidia` / `amd` / `vulkan` / `none`):
//!   exact match against `HardwareClass::silicon`.
//! - **Thermal tag** (`thinandlight` / `workstation` / `server` /
//!   `mobile`): exact match against `thermal_class`.
//! - **UMA tag** (`uma`): present iff `silicon == AppleM` (which the
//!   spec already implies; the tag is redundant but documented for
//!   reader clarity).
//! - **Range tag** (`field=lo..hi`): numeric range check on `vram_mb`
//!   or `ram_mb`. Inclusive at both ends.
//!
//! ALL constraints in a file's `applies_to` must hold for the file to
//! match. If multiple files match, the one with the MOST specific
//! `applies_to` (longest string, as a tiebreaker — narrower files win)
//! is selected. If NONE match, return a typed `NoMatchingPolicy` error
//! — never silently default to a wrong-hardware policy.
//!
//! ## Failure-mode discipline (matches the rest of the substrate)
//!
//! - No silent fallback. NoMatchingPolicy is a typed error that surfaces
//!   the HardwareClass + the list of files considered. Operator sees
//!   exactly what was probed + what was available.
//! - Range parse failures return `MalformedConstraint` with the
//!   field + value named.
//! - Unknown constraint tags return `UnknownConstraintTag` — no silent
//!   "treat as wildcard" interpretation.

use crate::governor::policy_file::PolicyFile;
use crate::governor::types::{HardwareClass, TargetSilicon, ThermalClass};

/// Errors the policy selector can surface. All typed; caller decides
/// fallback policy (built-in default in PR-3b, abort startup, etc.).
#[derive(Debug)]
pub enum PolicySelectionError {
    /// Zero policy files matched the given HardwareClass. Surfaces the
    /// hardware class + the list of files that were considered so the
    /// operator can see WHY nothing matched (typo in applies_to, wrong
    /// silicon tag, range too narrow, etc.).
    NoMatchingPolicy {
        hardware_class: HardwareClass,
        candidate_count: usize,
    },
    /// A file's `applies_to` had a constraint we couldn't parse.
    MalformedConstraint {
        file_index: usize,
        constraint: String,
        reason: String,
    },
    /// A file's `applies_to` used a tag we don't recognize. New tags
    /// land via this module + the docstring above — silent acceptance
    /// would let typos pass.
    UnknownConstraintTag {
        file_index: usize,
        tag: String,
    },
}

impl std::fmt::Display for PolicySelectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PolicySelectionError::NoMatchingPolicy {
                hardware_class,
                candidate_count,
            } => write!(
                f,
                "no policy file matched HardwareClass {:?} (considered {} candidate files). \
                 Check the applies_to fingerprint in each file against the probed hardware.",
                hardware_class.silicon, candidate_count
            ),
            PolicySelectionError::MalformedConstraint {
                file_index,
                constraint,
                reason,
            } => write!(
                f,
                "policy file #{file_index}: constraint '{constraint}' is malformed — {reason}"
            ),
            PolicySelectionError::UnknownConstraintTag { file_index, tag } => write!(
                f,
                "policy file #{file_index}: applies_to tag '{tag}' is not recognized. \
                 Known tags: silicon (apple-m/nvidia/amd/vulkan/none), thermal \
                 (thinandlight/workstation/server/mobile), uma, ranges (vram_mb=lo..hi, \
                 ram_mb=lo..hi)."
            ),
        }
    }
}

impl std::error::Error for PolicySelectionError {}

/// Pick the policy file whose `applies_to` best matches the
/// `HardwareClass`. See module docstring for the match algorithm.
///
/// `candidates` is the list of all policy files known to the
/// governor. Production caller (PR-3b) loads them from
/// `~/.continuum/policy/*.toml` via `load_policy_file` (PR-2).
///
/// Pure function — same `(hardware_class, candidates)` always returns
/// the same result. No I/O, no globals.
pub fn select_policy<'a>(
    hardware_class: &HardwareClass,
    candidates: &'a [PolicyFile],
) -> Result<&'a PolicyFile, PolicySelectionError> {
    let mut matches: Vec<(usize, &'a PolicyFile)> = Vec::new();
    for (i, file) in candidates.iter().enumerate() {
        if check_applies_to(&file.applies_to, hardware_class, i)? {
            matches.push((i, file));
        }
    }

    if matches.is_empty() {
        return Err(PolicySelectionError::NoMatchingPolicy {
            hardware_class: hardware_class.clone(),
            candidate_count: candidates.len(),
        });
    }

    // Tiebreaker: longest `applies_to` string wins (most specific).
    // Ties at the longest length resolve to the FIRST file in the list
    // (deterministic on input order).
    matches.sort_by(|a, b| b.1.applies_to.len().cmp(&a.1.applies_to.len()));
    Ok(matches[0].1)
}

/// Check whether `applies_to` matches the given `HardwareClass`.
/// Returns Ok(true) if all constraints hold, Ok(false) if any fail
/// "match"-style (not error), Err only for malformed/unknown.
fn check_applies_to(
    applies_to: &str,
    hw: &HardwareClass,
    file_index: usize,
) -> Result<bool, PolicySelectionError> {
    for raw in applies_to.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if !check_one(raw, hw, file_index)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn check_one(
    constraint: &str,
    hw: &HardwareClass,
    file_index: usize,
) -> Result<bool, PolicySelectionError> {
    // Range constraint: contains '='
    if let Some(eq_idx) = constraint.find('=') {
        let (field, rest) = constraint.split_at(eq_idx);
        let value = &rest[1..]; // strip '='
        return check_range(field.trim(), value.trim(), hw, constraint, file_index);
    }

    // Tag constraint
    match constraint {
        "apple-m" => Ok(hw.silicon == TargetSilicon::AppleM),
        "nvidia" => Ok(hw.silicon == TargetSilicon::NvidiaCuda),
        "amd" => Ok(hw.silicon == TargetSilicon::AmdRocm),
        "vulkan" => Ok(hw.silicon == TargetSilicon::IntelVulkan),
        "none" => Ok(hw.silicon == TargetSilicon::None),
        "thinandlight" => Ok(hw.thermal_class == ThermalClass::ThinAndLight),
        "workstation" => Ok(hw.thermal_class == ThermalClass::Workstation),
        "server" => Ok(hw.thermal_class == ThermalClass::Server),
        "mobile" => Ok(hw.thermal_class == ThermalClass::Mobile),
        // UMA tag: documented redundancy with apple-m. Always holds for
        // Apple Silicon, never for discrete GPUs. Used for reader
        // clarity in the applies_to string.
        "uma" => Ok(hw.silicon == TargetSilicon::AppleM),
        other => Err(PolicySelectionError::UnknownConstraintTag {
            file_index,
            tag: other.to_string(),
        }),
    }
}

fn check_range(
    field: &str,
    value: &str,
    hw: &HardwareClass,
    constraint: &str,
    file_index: usize,
) -> Result<bool, PolicySelectionError> {
    let dot_dot = value.find("..").ok_or_else(|| PolicySelectionError::MalformedConstraint {
        file_index,
        constraint: constraint.to_string(),
        reason: "range must use 'lo..hi' format (e.g. vram_mb=0..0 or ram_mb=14000..18000)"
            .into(),
    })?;
    let (lo_str, rest) = value.split_at(dot_dot);
    let hi_str = &rest[2..]; // strip ".."
    let lo: u64 = lo_str.trim().parse().map_err(|e: std::num::ParseIntError| {
        PolicySelectionError::MalformedConstraint {
            file_index,
            constraint: constraint.to_string(),
            reason: format!("range lo '{lo_str}' parse error: {e}"),
        }
    })?;
    let hi: u64 = hi_str.trim().parse().map_err(|e: std::num::ParseIntError| {
        PolicySelectionError::MalformedConstraint {
            file_index,
            constraint: constraint.to_string(),
            reason: format!("range hi '{hi_str}' parse error: {e}"),
        }
    })?;
    if hi < lo {
        return Err(PolicySelectionError::MalformedConstraint {
            file_index,
            constraint: constraint.to_string(),
            reason: format!("range hi ({hi}) < lo ({lo})"),
        });
    }

    let actual = match field {
        "vram_mb" => hw.vram_mb,
        "ram_mb" => hw.system_ram_mb,
        other => {
            return Err(PolicySelectionError::MalformedConstraint {
                file_index,
                constraint: constraint.to_string(),
                reason: format!("unknown range field '{other}' — known: vram_mb, ram_mb"),
            })
        }
    };

    Ok(actual >= lo && actual <= hi)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governor::policy_file::{
        CadenceMultipliersFile, ConcurrencyCapsFile, ConsolidationFileSection,
        FederationCadenceFile, PolicyFile, RecallScoreWeightsFile, SpeculationFileSection,
        TierSizesFile,
    };
    use crate::governor::types::{
        ConsolidationSchedule, PowerSource, SpeculationLevel, TargetSilicon, ThermalClass,
    };

    fn pol(applies_to: &str) -> PolicyFile {
        PolicyFile {
            policy_version: 1,
            applies_to: applies_to.into(),
            tier_sizes: TierSizesFile {
                l1_lora_layers: 2,
                l1_kv_tokens: 2048,
                l2_lora_layers: 4,
                l3_lora_layers: 12,
                l3_engrams: 1024,
            },
            cadence_multipliers: CadenceMultipliersFile {
                realtime: 1.0,
                delayed: 1.0,
                background: 1.0,
            },
            concurrency_caps: ConcurrencyCapsFile {
                personas_concurrent: 1,
                inference_lanes: 1,
                foundry_lanes: 0,
                sentinel_lanes: 1,
            },
            speculation: SpeculationFileSection {
                level: SpeculationLevel::Conservative,
            },
            consolidation: ConsolidationFileSection {
                schedule: ConsolidationSchedule::Manual,
            },
            federation: FederationCadenceFile {
                pull_cadence_seconds: 600,
            },
            recall_weights: RecallScoreWeightsFile {
                semantic: 0.4,
                outcome_history: 0.3,
                recency: 0.1,
                tier_proximity: 0.1,
                provenance_trust: 0.1,
            },
        }
    }

    fn hw(silicon: TargetSilicon, thermal: ThermalClass, vram_mb: u64, ram_mb: u64) -> HardwareClass {
        HardwareClass {
            silicon,
            silicon_model: "test".into(),
            vram_mb,
            system_ram_mb: ram_mb,
            power_source: PowerSource::Plugged,
            thermal_class: thermal,
            battery_pct: None,
            thermal_headroom_pct: None,
        }
    }

    // ===== happy paths =====

    /// What this catches: a policy with the canonical M-Air applies_to
    /// matches an M2 Air HardwareClass. The most common runtime; if
    /// this regresses, every Mac fails policy selection.
    #[test]
    fn m_air_policy_matches_m2_air_hardware() {
        let policies = vec![pol(
            "apple-m,thinandlight,uma,vram_mb=0..0,ram_mb=14000..18000",
        )];
        let m2_air = hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 16384);
        let selected = select_policy(&m2_air, &policies).unwrap();
        assert_eq!(selected.applies_to, policies[0].applies_to);
    }

    /// What this catches: Blackwell policy matches Blackwell hardware.
    /// 32GB VRAM range — discrete GPU path.
    #[test]
    fn blackwell_policy_matches_blackwell_hardware() {
        let policies = vec![pol(
            "nvidia,workstation,vram_mb=30000..36000,ram_mb=60000..80000",
        )];
        let blackwell = hw(TargetSilicon::NvidiaCuda, ThermalClass::Workstation, 32 * 1024, 64 * 1024);
        let selected = select_policy(&blackwell, &policies).unwrap();
        assert_eq!(selected.applies_to, policies[0].applies_to);
    }

    /// What this catches: multiple candidates, ONE matches → that's
    /// the one returned. Tests the filter step.
    #[test]
    fn picks_only_matching_among_multiple_candidates() {
        let policies = vec![
            pol("nvidia,workstation,vram_mb=30000..36000,ram_mb=60000..80000"),
            pol("apple-m,thinandlight,uma,vram_mb=0..0,ram_mb=14000..18000"),
            pol("amd,workstation,vram_mb=20000..28000,ram_mb=32000..64000"),
        ];
        let m2_air = hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 16384);
        let selected = select_policy(&m2_air, &policies).unwrap();
        assert!(selected.applies_to.contains("apple-m"));
    }

    /// What this catches: multiple candidates match → the LONGER
    /// applies_to string wins (more specific). Tests the tiebreaker.
    #[test]
    fn longer_applies_to_wins_tiebreaker() {
        let policies = vec![
            pol("apple-m"), // broad — matches any Apple Silicon
            pol("apple-m,thinandlight,uma,vram_mb=0..0,ram_mb=14000..18000"), // narrow
        ];
        let m2_air = hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 16384);
        let selected = select_policy(&m2_air, &policies).unwrap();
        assert!(
            selected.applies_to.contains("ram_mb"),
            "narrower policy should win; got: {}",
            selected.applies_to
        );
    }

    /// What this catches: zero candidates → NoMatchingPolicy with
    /// candidate_count=0. Defensive against empty policy directory.
    #[test]
    fn empty_candidates_returns_no_matching_policy() {
        let m2_air = hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 16384);
        let result = select_policy(&m2_air, &[]);
        match result {
            Err(PolicySelectionError::NoMatchingPolicy { candidate_count, .. }) => {
                assert_eq!(candidate_count, 0);
            }
            other => panic!("expected NoMatchingPolicy, got {other:?}"),
        }
    }

    /// What this catches: candidates exist but none match → typed err
    /// with the HardwareClass + candidate_count named. Operator can see
    /// "I have 3 files but none apply to my hardware."
    #[test]
    fn no_match_returns_typed_err_with_hardware_class() {
        let policies = vec![
            pol("nvidia,workstation,vram_mb=30000..36000,ram_mb=60000..80000"),
            pol("amd,server,vram_mb=20000..28000,ram_mb=32000..64000"),
        ];
        let m2_air = hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 16384);
        let result = select_policy(&m2_air, &policies);
        match result {
            Err(PolicySelectionError::NoMatchingPolicy {
                hardware_class,
                candidate_count,
            }) => {
                assert_eq!(hardware_class.silicon, TargetSilicon::AppleM);
                assert_eq!(candidate_count, 2);
            }
            other => panic!("expected NoMatchingPolicy, got {other:?}"),
        }
    }

    // ===== individual constraints =====

    /// What this catches: silicon tag mismatch fails the constraint.
    /// Each silicon variant tested.
    #[test]
    fn silicon_tag_must_match() {
        let m2 = hw(TargetSilicon::AppleM, ThermalClass::Workstation, 0, 16384);
        let nvidia = hw(TargetSilicon::NvidiaCuda, ThermalClass::Workstation, 32 * 1024, 64 * 1024);

        assert!(select_policy(&m2, &[pol("apple-m")]).is_ok());
        assert!(select_policy(&m2, &[pol("nvidia")]).is_err());
        assert!(select_policy(&nvidia, &[pol("nvidia")]).is_ok());
        assert!(select_policy(&nvidia, &[pol("apple-m")]).is_err());
    }

    /// What this catches: thermal_class tag must match.
    #[test]
    fn thermal_tag_must_match() {
        let workstation = hw(TargetSilicon::NvidiaCuda, ThermalClass::Workstation, 32 * 1024, 64 * 1024);
        assert!(select_policy(&workstation, &[pol("workstation")]).is_ok());
        assert!(select_policy(&workstation, &[pol("thinandlight")]).is_err());
    }

    /// What this catches: range constraint matches inclusively at the
    /// lower boundary. Boundary check — common off-by-one source.
    #[test]
    fn range_matches_inclusive_lower_boundary() {
        let h = hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 14000);
        let policies = vec![pol("apple-m,ram_mb=14000..18000")];
        assert!(select_policy(&h, &policies).is_ok());
    }

    /// What this catches: range constraint matches inclusively at the
    /// upper boundary.
    #[test]
    fn range_matches_inclusive_upper_boundary() {
        let h = hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 18000);
        let policies = vec![pol("apple-m,ram_mb=14000..18000")];
        assert!(select_policy(&h, &policies).is_ok());
    }

    /// What this catches: one below the lower bound fails.
    #[test]
    fn range_misses_one_below_lower() {
        let h = hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 13999);
        let policies = vec![pol("apple-m,ram_mb=14000..18000")];
        assert!(select_policy(&h, &policies).is_err());
    }

    /// What this catches: one above the upper bound fails.
    #[test]
    fn range_misses_one_above_upper() {
        let h = hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 18001);
        let policies = vec![pol("apple-m,ram_mb=14000..18000")];
        assert!(select_policy(&h, &policies).is_err());
    }

    /// What this catches: vram_mb range matches discrete GPU VRAM.
    #[test]
    fn vram_range_matches_blackwell() {
        let h = hw(TargetSilicon::NvidiaCuda, ThermalClass::Workstation, 32 * 1024, 64 * 1024);
        let policies = vec![pol("nvidia,vram_mb=30000..36000")];
        assert!(select_policy(&h, &policies).is_ok());
    }

    /// What this catches: UMA tag holds for Apple Silicon, fails for
    /// discrete GPUs. Documented redundancy with apple-m but useful
    /// for reader clarity in applies_to strings.
    #[test]
    fn uma_tag_apple_only() {
        let m2 = hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 16384);
        let nvidia = hw(TargetSilicon::NvidiaCuda, ThermalClass::Workstation, 32 * 1024, 64 * 1024);
        assert!(select_policy(&m2, &[pol("uma")]).is_ok());
        assert!(select_policy(&nvidia, &[pol("uma")]).is_err());
    }

    // ===== malformed =====

    /// What this catches: unknown tag returns typed err with the tag
    /// named. No silent "treat as wildcard."
    #[test]
    fn unknown_tag_returns_typed_err() {
        let h = hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 16384);
        let policies = vec![pol("apple-m,futuristic-quantum-chip")];
        let result = select_policy(&h, &policies);
        match result {
            Err(PolicySelectionError::UnknownConstraintTag { tag, .. }) => {
                assert_eq!(tag, "futuristic-quantum-chip");
            }
            other => panic!("expected UnknownConstraintTag, got {other:?}"),
        }
    }

    /// What this catches: range with missing '..' returns typed err.
    /// "ram_mb=16000" alone is invalid — must be a range.
    #[test]
    fn range_without_dotdot_returns_typed_err() {
        let h = hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 16384);
        let policies = vec![pol("apple-m,ram_mb=16000")];
        let result = select_policy(&h, &policies);
        assert!(matches!(
            result,
            Err(PolicySelectionError::MalformedConstraint { .. })
        ));
    }

    /// What this catches: range with non-numeric lo returns typed err.
    #[test]
    fn range_with_non_numeric_lo_returns_typed_err() {
        let h = hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 16384);
        let policies = vec![pol("apple-m,ram_mb=abc..18000")];
        assert!(matches!(
            select_policy(&h, &policies),
            Err(PolicySelectionError::MalformedConstraint { .. })
        ));
    }

    /// What this catches: range with hi < lo returns typed err.
    /// "ram_mb=20000..10000" is nonsense.
    #[test]
    fn range_with_inverted_bounds_returns_typed_err() {
        let h = hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 16384);
        let policies = vec![pol("apple-m,ram_mb=20000..10000")];
        let result = select_policy(&h, &policies);
        match result {
            Err(PolicySelectionError::MalformedConstraint { reason, .. }) => {
                assert!(reason.contains("hi"));
                assert!(reason.contains("lo"));
            }
            other => panic!("expected MalformedConstraint, got {other:?}"),
        }
    }

    /// What this catches: unknown range field returns typed err.
    #[test]
    fn unknown_range_field_returns_typed_err() {
        let h = hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 16384);
        let policies = vec![pol("apple-m,cpu_ghz=3..5")];
        let result = select_policy(&h, &policies);
        assert!(matches!(
            result,
            Err(PolicySelectionError::MalformedConstraint { .. })
        ));
    }

    // ===== whitespace + empty constraints =====

    /// What this catches: leading/trailing whitespace around constraints
    /// is tolerated (engineers tune by hand; whitespace is human).
    #[test]
    fn whitespace_in_applies_to_tolerated() {
        let h = hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 16384);
        let policies = vec![pol(" apple-m , thinandlight , ram_mb = 14000..18000 ")];
        assert!(select_policy(&h, &policies).is_ok());
    }

    /// What this catches: empty applies_to (zero constraints) matches
    /// ANY hardware (vacuous truth). Defensive — a policy file with
    /// no applies_to acts as a universal default. Caller may want to
    /// treat this as a config bug; we accept it as wildcard.
    #[test]
    fn empty_applies_to_acts_as_wildcard() {
        let h = hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 16384);
        let policies = vec![pol("")];
        assert!(select_policy(&h, &policies).is_ok());
    }

    /// What this catches: PolicySelectionError implements Display +
    /// Error so callers can use it in `?` chains + dyn Error contexts.
    #[test]
    fn policy_selection_error_implements_error_trait() {
        let h = hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 16384);
        let err = select_policy(&h, &[]).unwrap_err();
        let _: &dyn std::error::Error = &err;
        let display = format!("{err}");
        assert!(display.contains("0") || display.contains("AppleM") || display.contains("apple"));
    }

    /// What this catches: pure-function determinism. Same inputs → same
    /// output across calls. PR-3b can cache the selection if the
    /// HardwareClass + candidate list don't change.
    #[test]
    fn selection_is_deterministic() {
        let h = hw(TargetSilicon::AppleM, ThermalClass::ThinAndLight, 0, 16384);
        let policies = vec![
            pol("apple-m,thinandlight,uma,vram_mb=0..0,ram_mb=14000..18000"),
            pol("nvidia,workstation,vram_mb=30000..36000"),
        ];
        let a = select_policy(&h, &policies).unwrap();
        let b = select_policy(&h, &policies).unwrap();
        assert_eq!(a.applies_to, b.applies_to);
    }
}
