//! `sage/profile`: where this node's time goes, part by part. Every SAGE component's inputs are timed
//! by the base (their wait in its intake, their time to handle), and each owner reads its parts'
//! numbers through its profiler: the node's owner and every resident's. This reads them all now,
//! without waiting on any part, the longest-waiting first.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

/// One part's measured numbers.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/sage/PartProfile.ts")]
pub struct PartProfile {
    pub component: String,
    #[ts(type = "number")]
    pub handled: u64,
    #[ts(type = "number")]
    pub last_wait_us: u64,
    #[ts(type = "number")]
    pub max_wait_us: u64,
    #[ts(type = "number")]
    pub last_handle_us: u64,
    #[ts(type = "number")]
    pub max_handle_us: u64,
    #[ts(type = "number")]
    pub waiting: u64,
    #[ts(type = "number")]
    pub frames_skipped: u64,
}

impl PartProfile {
    fn of((component, t): (&'static str, continuum_sage::component::Telemetry)) -> Self {
        let us = |d: std::time::Duration| d.as_micros().min(u64::MAX as u128) as u64;
        Self {
            component: component.to_owned(),
            handled: t.handled,
            last_wait_us: us(t.last_wait),
            max_wait_us: us(t.max_wait),
            last_handle_us: us(t.last_handle),
            max_handle_us: us(t.max_handle),
            waiting: t.waiting as u64,
            frames_skipped: t.frames_skipped,
        }
    }
}

/// One resident's parts.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/sage/ResidentProfile.ts")]
pub struct ResidentProfile {
    #[ts(type = "string")]
    pub persona: uuid::Uuid,
    pub parts: Vec<PartProfile>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/sage/SageProfileParams.ts")]
pub struct SageProfileParams {}

/// What `sage/profile` answers.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/sage/SageProfileResult.ts")]
pub struct SageProfileResult {
    /// The node's parts, the longest-waiting first; empty before the node's owner is built.
    pub node: Vec<PartProfile>,
    /// Each resident's parts, the longest-waiting first within each.
    pub residents: Vec<ResidentProfile>,
}

/// Stateless: the node's profiler sits beside the node's bus, residents' in the resident index.
#[derive(Default)]
pub struct SageProfile;

#[async_trait]
impl ActionCommand for SageProfile {
    const NAME: &'static str = "sage/profile";
    // An operator diagnostic across every resident's parts, never her tool surface.
    const NATIVE: bool = false;
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str = "Where this node's time goes, part by part: every SAGE component of the node and of each resident, with inputs handled, the last and longest wait in its intake, the last and longest time to handle one, inputs waiting, and frames skipped. Longest-waiting first: the top line is the bottleneck.";
    type Params = SageProfileParams;
    type Output = SageProfileResult;

    async fn run(&self, _ctx: &Ctx, _p: Self::Params) -> Result<Self::Output, CommandError> {
        let node = continuum_sage::component::node_profile().unwrap_or_default().into_iter().map(PartProfile::of).collect(); // JUSTIFIED: no node owner yet is no node parts, not an error
        let residents = crate::persona::senses::profiles()
            .into_iter()
            .map(|(persona, parts)| ResidentProfile { persona, parts: parts.into_iter().map(PartProfile::of).collect() })
            .collect();
        Ok(SageProfileResult { node, residents })
    }
}

crate::register_stateless_command!(SageProfile);
