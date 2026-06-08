//! Policy selection for substrate-governor policy files.
//!
//! PR-3a keeps this pure: a parsed `PolicyFile` either matches a
//! `HardwareClass` or returns a typed error explaining why selection
//! cannot proceed. File watching and cascade mutation remain separate
//! slices.

use crate::governor::policy_file::PolicyFile;
use crate::governor::types::{HardwareClass, TargetSilicon, ThermalClass};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicySelectionError {
    EmptyAppliesTo,
    UnknownConstraint { token: String },
    MalformedRange { token: String },
    NoMatchingPolicy { fingerprint: String },
    AmbiguousPolicy { fingerprint: String, count: usize },
}

impl std::fmt::Display for PolicySelectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PolicySelectionError::EmptyAppliesTo => {
                write!(f, "policy applies_to must contain at least one constraint")
            }
            PolicySelectionError::UnknownConstraint { token } => {
                write!(f, "unknown policy applies_to constraint: {token}")
            }
            PolicySelectionError::MalformedRange { token } => {
                write!(f, "malformed policy applies_to range constraint: {token}")
            }
            PolicySelectionError::NoMatchingPolicy { fingerprint } => {
                write!(
                    f,
                    "no policy file matches hardware fingerprint {fingerprint}"
                )
            }
            PolicySelectionError::AmbiguousPolicy { fingerprint, count } => write!(
                f,
                "{count} policy files match hardware fingerprint {fingerprint}; \
                 selection must be unambiguous"
            ),
        }
    }
}

impl std::error::Error for PolicySelectionError {}

pub fn hardware_fingerprint(hw: &HardwareClass) -> String {
    let memory_kind = if hw.vram_mb == 0 { "uma" } else { "discrete" };
    format!(
        "{},{},{},vram_mb={},ram_mb={}",
        silicon_token(hw.silicon),
        thermal_token(hw.thermal_class),
        memory_kind,
        hw.vram_mb,
        hw.system_ram_mb
    )
}

pub fn policy_matches_hardware(
    policy: &PolicyFile,
    hw: &HardwareClass,
) -> Result<bool, PolicySelectionError> {
    let mut saw_constraint = false;
    for raw_token in policy.applies_to.split(',') {
        let token = raw_token.trim().to_ascii_lowercase();
        if token.is_empty() {
            continue;
        }
        saw_constraint = true;
        if !constraint_matches(&token, hw)? {
            return Ok(false);
        }
    }

    if !saw_constraint {
        return Err(PolicySelectionError::EmptyAppliesTo);
    }
    Ok(true)
}

pub fn select_policy<'a>(
    policies: &'a [PolicyFile],
    hw: &HardwareClass,
) -> Result<&'a PolicyFile, PolicySelectionError> {
    let mut matches =
        policies
            .iter()
            .filter_map(|policy| match policy_matches_hardware(policy, hw) {
                Ok(true) => Some(Ok(policy)),
                Ok(false) => None,
                Err(err) => Some(Err(err)),
            });

    let Some(first) = matches.next().transpose()? else {
        return Err(PolicySelectionError::NoMatchingPolicy {
            fingerprint: hardware_fingerprint(hw),
        });
    };

    let mut count = 1usize;
    for matched in matches {
        matched?;
        count += 1;
    }

    if count > 1 {
        return Err(PolicySelectionError::AmbiguousPolicy {
            fingerprint: hardware_fingerprint(hw),
            count,
        });
    }

    Ok(first)
}

fn constraint_matches(token: &str, hw: &HardwareClass) -> Result<bool, PolicySelectionError> {
    match token {
        "apple-m" => Ok(hw.silicon == TargetSilicon::AppleM),
        "nvidia" | "nvidia-cuda" => Ok(hw.silicon == TargetSilicon::NvidiaCuda),
        "amd-rocm" => Ok(hw.silicon == TargetSilicon::AmdRocm),
        "intel-vulkan" => Ok(hw.silicon == TargetSilicon::IntelVulkan),
        "none" | "cpu-only" => Ok(hw.silicon == TargetSilicon::None),
        "thinandlight" | "thin-and-light" => Ok(hw.thermal_class == ThermalClass::ThinAndLight),
        "workstation" => Ok(hw.thermal_class == ThermalClass::Workstation),
        "server" => Ok(hw.thermal_class == ThermalClass::Server),
        "mobile" => Ok(hw.thermal_class == ThermalClass::Mobile),
        "uma" => Ok(hw.vram_mb == 0),
        "discrete" => Ok(hw.vram_mb > 0),
        _ if token.starts_with("vram_mb=") => range_contains(token, "vram_mb=", hw.vram_mb),
        _ if token.starts_with("ram_mb=") => range_contains(token, "ram_mb=", hw.system_ram_mb),
        _ => Err(PolicySelectionError::UnknownConstraint {
            token: token.to_string(),
        }),
    }
}

fn range_contains(token: &str, prefix: &str, value: u64) -> Result<bool, PolicySelectionError> {
    let Some(range) = token.strip_prefix(prefix) else {
        return Err(PolicySelectionError::MalformedRange {
            token: token.to_string(),
        });
    };
    let Some((lower, upper)) = range.split_once("..") else {
        return Err(PolicySelectionError::MalformedRange {
            token: token.to_string(),
        });
    };
    let lower = lower
        .parse::<u64>()
        .map_err(|_| PolicySelectionError::MalformedRange {
            token: token.to_string(),
        })?;
    let upper = upper
        .parse::<u64>()
        .map_err(|_| PolicySelectionError::MalformedRange {
            token: token.to_string(),
        })?;
    if lower > upper {
        return Err(PolicySelectionError::MalformedRange {
            token: token.to_string(),
        });
    }
    Ok((lower..=upper).contains(&value))
}

fn silicon_token(silicon: TargetSilicon) -> &'static str {
    match silicon {
        TargetSilicon::AppleM => "apple-m",
        TargetSilicon::NvidiaCuda => "nvidia-cuda",
        TargetSilicon::AmdRocm => "amd-rocm",
        TargetSilicon::IntelVulkan => "intel-vulkan",
        TargetSilicon::None => "none",
    }
}

fn thermal_token(thermal: ThermalClass) -> &'static str {
    match thermal {
        ThermalClass::ThinAndLight => "thinandlight",
        ThermalClass::Workstation => "workstation",
        ThermalClass::Server => "server",
        ThermalClass::Mobile => "mobile",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governor::policy_file::parse_policy_text;
    use crate::governor::types::{PowerSource, ThermalClass};

    const AIR_POLICY: &str = r#"
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
foundry_lanes        = 1
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

    const WORKSTATION_POLICY: &str = r#"
policy_version = 9
applies_to    = "nvidia,workstation,discrete,vram_mb=30000..36000,ram_mb=60000..80000"

[tier_sizes]
l1_lora_layers       = 8
l1_kv_tokens         = 16384
l2_lora_layers       = 16
l3_lora_layers       = 32
l3_engrams           = 8192

[cadence_multipliers]
realtime             = 1.0
delayed              = 1.0
background           = 1.25

[concurrency_caps]
personas_concurrent  = 8
inference_lanes      = 4
foundry_lanes        = 2
sentinel_lanes       = 2

[speculation]
level                = "aggressive"

[consolidation]
schedule             = "always"

[federation]
pull_cadence_seconds = 60

[recall_weights]
semantic             = 0.35
outcome_history      = 0.25
recency              = 0.15
tier_proximity       = 0.15
provenance_trust     = 0.10
"#;

    fn air_hw() -> HardwareClass {
        HardwareClass {
            silicon: TargetSilicon::AppleM,
            silicon_model: "M2".to_string(),
            vram_mb: 0,
            system_ram_mb: 16_384,
            power_source: PowerSource::Plugged,
            thermal_class: ThermalClass::ThinAndLight,
            battery_pct: None,
            thermal_headroom_pct: None,
        }
    }

    fn workstation_hw() -> HardwareClass {
        HardwareClass {
            silicon: TargetSilicon::NvidiaCuda,
            silicon_model: "RTX 5090".to_string(),
            vram_mb: 32_768,
            system_ram_mb: 65_536,
            power_source: PowerSource::Plugged,
            thermal_class: ThermalClass::Workstation,
            battery_pct: None,
            thermal_headroom_pct: None,
        }
    }

    fn parse_policy(text: &str) -> PolicyFile {
        parse_policy_text(text).expect("test policy should parse")
    }

    #[test]
    fn air_policy_matches_air_hardware() {
        let policy = parse_policy(AIR_POLICY);
        assert!(policy_matches_hardware(&policy, &air_hw()).expect("selector should evaluate"));
    }

    #[test]
    fn air_policy_does_not_match_workstation_hardware() {
        let policy = parse_policy(AIR_POLICY);
        assert!(
            !policy_matches_hardware(&policy, &workstation_hw()).expect("selector should evaluate")
        );
    }

    #[test]
    fn workstation_policy_matches_5090_hardware() {
        let policy = parse_policy(WORKSTATION_POLICY);
        assert!(
            policy_matches_hardware(&policy, &workstation_hw()).expect("selector should evaluate")
        );
    }

    #[test]
    fn select_policy_returns_single_matching_policy() {
        let policies = vec![parse_policy(AIR_POLICY), parse_policy(WORKSTATION_POLICY)];
        let selected =
            select_policy(&policies, &workstation_hw()).expect("one policy should match");
        assert_eq!(selected.policy_version, 9);
    }

    #[test]
    fn select_policy_rejects_no_match() {
        let policies = vec![parse_policy(AIR_POLICY)];
        let err = select_policy(&policies, &workstation_hw()).expect_err("no policy should match");
        assert!(matches!(err, PolicySelectionError::NoMatchingPolicy { .. }));
    }

    #[test]
    fn select_policy_rejects_ambiguity() {
        let policies = vec![parse_policy(AIR_POLICY), parse_policy(AIR_POLICY)];
        let err = select_policy(&policies, &air_hw()).expect_err("two policies should match");
        assert_eq!(
            err,
            PolicySelectionError::AmbiguousPolicy {
                fingerprint: hardware_fingerprint(&air_hw()),
                count: 2
            }
        );
    }

    #[test]
    fn unknown_constraint_is_error_not_false() {
        let mut policy = parse_policy(AIR_POLICY);
        policy.applies_to = "apple-m,mystery-gpu".to_string();
        let err = policy_matches_hardware(&policy, &air_hw())
            .expect_err("unknown token should be explicit");
        assert_eq!(
            err,
            PolicySelectionError::UnknownConstraint {
                token: "mystery-gpu".to_string()
            }
        );
    }

    #[test]
    fn malformed_range_is_error_not_false() {
        let mut policy = parse_policy(AIR_POLICY);
        policy.applies_to = "apple-m,ram_mb=18000..14000".to_string();
        let err = policy_matches_hardware(&policy, &air_hw())
            .expect_err("inverted range should be explicit");
        assert_eq!(
            err,
            PolicySelectionError::MalformedRange {
                token: "ram_mb=18000..14000".to_string()
            }
        );
    }

    #[test]
    fn empty_applies_to_is_error() {
        let mut policy = parse_policy(AIR_POLICY);
        policy.applies_to = " , ".to_string();
        let err = policy_matches_hardware(&policy, &air_hw())
            .expect_err("empty selector should be explicit");
        assert_eq!(err, PolicySelectionError::EmptyAppliesTo);
    }

    #[test]
    fn hardware_fingerprint_is_stable_and_readable() {
        assert_eq!(
            hardware_fingerprint(&air_hw()),
            "apple-m,thinandlight,uma,vram_mb=0,ram_mb=16384"
        );
        assert_eq!(
            hardware_fingerprint(&workstation_hw()),
            "nvidia-cuda,workstation,discrete,vram_mb=32768,ram_mb=65536"
        );
    }
}
