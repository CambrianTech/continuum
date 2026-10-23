//! The vocabulary a throughput lane is named in: what KIND of work a demand is
//! ([`ResourceClass`]) and what silicon it occupies ([`TargetSilicon`]). Plain data the
//! resource authority and the paging revocation policy both speak, so it lives with the
//! authority; the inference planner (which plans jobs over these lanes) imports it.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/ResourceClass.ts"
)]
pub enum ResourceClass {
    Cpu,
    Data,
    Gpu,
    Embedding,
    LocalGeneration,
    CloudProvider,
    Io,
    Media,
    Render,
    Memory,
    Background,
}

#[derive(
    Debug,
    Clone,
    Copy,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Serialize,
    Deserialize,
    TS,
    schemars::JsonSchema,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/TargetSilicon.ts"
)]
pub enum TargetSilicon {
    Cpu,
    Gpu,
    UnifiedMemory,
    Network,
    Disk,
    Cloud,
    Background,
}
