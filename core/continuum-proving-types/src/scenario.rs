//! A scenario is data, not code (THE-PROVING-GROUND P5, 4a): the world, the workload, the events,
//! the policy and what passes. A live incident becomes a file the same day, so it cannot return.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::closed::{GitSha, Sha256, Slug};
use crate::profile::Tier;

/// A node profile by its node and the content hash of the profile file, so a scenario names
/// exactly the measurements it ran against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileRef {
    pub node: Slug,
    pub profile: Sha256,
}

/// The person at the machine: what their apps take, and how long they will wait.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PersonEvent {
    /// An app (a game, a build) holds memory and cores for a while.
    App {
        name: Slug,
        node: Slug,
        tier: Tier,
        bytes: u64,
        cores: u32,
        from_ms: u64,
        until_ms: u64,
    },
    /// The person writes into a room, and waits this long before it hurts.
    Message {
        room: Uuid,
        at_ms: u64,
        patience_ms: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct World {
    pub nodes: Vec<ProfileRef>,
    pub person: Vec<PersonEvent>,
}

/// Which acts of a reduction set to replay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkloadRef {
    /// Content hash of the reduction set file.
    pub reductions: Sha256,
    /// The minds it replays; empty is every mind in the set.
    pub minds: Vec<Uuid>,
}

/// Something that happens to the world at a time (THE-PROVING-GROUND 2, 8a's admission events).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorldEvent {
    CapacityDrop { node: Slug, tier: Tier, bytes: u64 },
    CapacityRecover { node: Slug, tier: Tier, bytes: u64 },
    CoreRestart { node: Slug },
    EngineRelaunch { node: Slug },
    NodeJoin { node: Slug },
    NodeLeave { node: Slug },
    Partition { a: Slug, b: Slug },
    Heal { a: Slug, b: Slug },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimedEvent {
    pub at_ms: u64,
    pub event: WorldEvent,
}

/// The policy under test: a revision, with typed overrides of its knobs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyRef {
    pub revision: GitSha,
    pub overrides: Vec<Override>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Override {
    pub knob: Slug,
    pub value: OverrideValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case", deny_unknown_fields)]
pub enum OverrideValue {
    Int(i64),
    Bool(bool),
}

/// A metric the live health line, the 15-minute read and every run report alike
/// (THE-PROVING-GROUND 5), plus the mechanisms the known-bad suite fails on (4a).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "metric", content = "of", rename_all = "snake_case", deny_unknown_fields)]
pub enum Metric {
    // per mind
    Acts,
    Writes,
    ActSecondsP50,
    ActSecondsP90,
    PrefillSecondsPerAct,
    DecodeSecondsPerAct,
    PrefixReuse,
    Faults,
    Repeats,
    Cuts { stop_class: Slug },
    StarvationSeconds,
    // per loop
    Reversals,
    // per node
    Relaunches,
    MemoryReturnedBytes,
    MemoryReturnSeconds,
    // per grid
    Placements,
    Rehomes,
    TransferBytes,
    TransferSeconds,
    // mechanisms (4a's known-bad suite)
    TurnoversPerAct,
    UnallocatedZeroWithTurnover,
    ReasoningBudgetAboveGrant,
    HotWriterCutWhileSlotsIdle,
    RepeatsOfUnfulfilled,
}

/// A measured baseline: the scenario and policy it ran, and the receipt it produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Baseline {
    pub scenario: Slug,
    pub policy: GitSha,
    pub receipt: Sha256,
}

/// What passes. An invariant, or a comparison with a named measured baseline; there is no variant
/// that carries a number, so a threshold someone picked cannot be written (R1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "criterion", rename_all = "snake_case", deny_unknown_fields)]
pub enum Criterion {
    /// The metric is zero over the run.
    Never {},
    /// Not worse than the baseline beyond the published error on that metric for that node.
    NoWorseThan { baseline: Baseline },
    /// Better than the baseline by more than the published error.
    BetterThan { baseline: Baseline },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PassCriterion {
    pub metric: Metric,
    pub criterion: Criterion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ScenarioKind {
    Scenario {},
    /// A live incident: the prover must report `fails` as failing under the incident's policy, or
    /// it has either broken or shown the incident is no longer bad (10.4: a reviewer decides).
    KnownBad { incident: Slug, policy: GitSha, fails: PassCriterion },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub name: Slug,
    pub kind: ScenarioKind,
    pub world: World,
    pub workload: Vec<WorkloadRef>,
    pub events: Vec<TimedEvent>,
    pub policy: PolicyRef,
    pub pass: Vec<PassCriterion>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known_bad() -> serde_json::Value {
        let h = |b: u8| format!("{b:02x}").repeat(32);
        serde_json::json!({
            "name": "think-cut-963-grant",
            "kind": { "kind": "known_bad", "incident": "2026-10-30-think-cuts", "policy": "b7879d85c",
                      "fails": { "metric": { "metric": "reasoning_budget_above_grant" }, "criterion": { "criterion": "never" } } },
            "world": { "nodes": [{ "node": "m5", "profile": h(1) }], "person": [] },
            "workload": [{ "reductions": h(2), "minds": [] }],
            "events": [{ "at_ms": 0, "event": { "kind": "core_restart", "node": "m5" } }],
            "policy": { "revision": "b7879d85c", "overrides": [] },
            "pass": [{ "metric": { "metric": "reasoning_budget_above_grant" }, "criterion": { "criterion": "never" } }]
        })
    }

    // what this catches: the known-bad file shape drifting from what the harness reads.
    #[test]
    fn a_known_bad_file_round_trips() {
        let s: Scenario = serde_json::from_value(known_bad()).expect("a known-bad scenario");
        let back = serde_json::to_value(&s).expect("ser");
        assert_eq!(serde_json::from_value::<Scenario>(back).expect("de"), s);
    }

    // what this catches: a picked threshold (R1). A criterion carrying a number does not parse,
    // whatever its field is called.
    #[test]
    fn a_picked_threshold_cannot_be_written() {
        for picked in [
            serde_json::json!({ "criterion": "at_least", "value": 40 }),
            serde_json::json!({ "criterion": "never", "note": "the words" }),
            serde_json::json!({ "criterion": "never", "value": 0 }),
            serde_json::json!({ "criterion": "no_worse_than", "baseline": { "scenario": "s", "policy": "b7879d85c", "receipt": "ab".repeat(32) }, "by": 0.1 }),
        ] {
            let mut v = known_bad();
            v["pass"][0]["criterion"] = picked.clone();
            assert!(serde_json::from_value::<Scenario>(v).is_err(), "{picked} parsed");
        }
    }

    // what this catches: words beside a metric or a plain scenario kind (see provenance's
    // nothing_rides_beside_an_empty_variant for why these shapes are the ones serde enforces).
    #[test]
    fn nothing_rides_beside_a_metric_or_a_kind() {
        for (path, planted) in [
            (&["pass", "0", "metric"][..], serde_json::json!({ "metric": "acts", "note": "the words" })),
            (&["pass", "0", "metric"][..], serde_json::json!({ "metric": "acts", "of": "the words" })),
            (&["kind"][..], serde_json::json!({ "kind": "scenario", "note": "the words" })),
        ] {
            let mut v = known_bad();
            let mut at = &mut v;
            for key in path {
                at = match key.parse::<usize>() {
                    Ok(i) => &mut at[i],
                    Err(_) => &mut at[*key],
                };
            }
            *at = planted.clone();
            assert!(serde_json::from_value::<Scenario>(v).is_err(), "{planted} parsed");
        }
    }
}
