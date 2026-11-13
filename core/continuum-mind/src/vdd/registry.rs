use serde::Serialize;
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HarnessId {
    ChatRoundtripLive,
}

impl HarnessId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ChatRoundtripLive => "chat-roundtrip-live",
        }
    }
}

impl fmt::Display for HarnessId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for HarnessId {
    type Err = UnknownHarness;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "chat-roundtrip-live" => Ok(Self::ChatRoundtripLive),
            other => Err(UnknownHarness {
                requested: other.to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown continuum-vdd harness: {requested}")]
pub struct UnknownHarness {
    pub requested: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct HarnessSpec {
    pub id: HarnessId,
    pub scenario: &'static str,
    pub cadence: HarnessCadence,
    pub requires_live_substrate: bool,
    pub command: &'static str,
    pub description: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HarnessCadence {
    PerPr,
}

pub const CHAT_ROUNDTRIP_LIVE_SPEC: HarnessSpec = HarnessSpec {
    id: HarnessId::ChatRoundtripLive,
    scenario: "chat-roundtrip-live-harness",
    cadence: HarnessCadence::PerPr,
    requires_live_substrate: true,
    command: "cargo continuum-vdd chat-roundtrip-live",
    description: "Verifies the live chat substrate can admit a probe and observe persona replies without counting missing prerequisites as success.",
};

pub const HARNESS_SPECS: &[HarnessSpec] = &[CHAT_ROUNDTRIP_LIVE_SPEC];

pub fn harness_spec(id: HarnessId) -> HarnessSpec {
    match id {
        HarnessId::ChatRoundtripLive => CHAT_ROUNDTRIP_LIVE_SPEC,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_canonical_harness_id() {
        assert_eq!(
            "chat-roundtrip-live".parse::<HarnessId>(),
            Ok(HarnessId::ChatRoundtripLive)
        );
    }

    #[test]
    fn rejects_unknown_harness_ids() {
        let err = "chat".parse::<HarnessId>().unwrap_err();

        assert_eq!(err.requested, "chat");
    }

    #[test]
    fn registry_has_stable_command_and_scenario() {
        let spec = harness_spec(HarnessId::ChatRoundtripLive);

        assert_eq!(HARNESS_SPECS, &[spec]);
        assert_eq!(spec.command, "cargo continuum-vdd chat-roundtrip-live");
        assert_eq!(spec.scenario, "chat-roundtrip-live-harness");
        assert!(spec.requires_live_substrate);
    }
}
