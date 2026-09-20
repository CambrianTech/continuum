//! `plasticity/topology` — read back the head topology of an already-compacted model.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ts_rs::TS;

use crate::modules::plasticity::topology as topology_io;
use crate::modules::plasticity::types::HeadTopology;

/// Params for `plasticity/topology`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/plasticity/TopologyParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct TopologyParams {
    /// Path to a `head_topology.json` written by a prior compaction.
    pub topology_path: String,
}

crate::action_command! {
    /// Read back the head topology of an already-compacted model — the per-head
    /// precision assignments, parameter-reduction ratio, and precision profile from
    /// a `head_topology.json` sidecar. Read-only.
    pub struct PlasticityTopology;
    name: "plasticity/topology",
    access: Privileged,
    params: TopologyParams,
    output: HeadTopology,
    run(_this, _ctx, p) => {
        let topo = topology_io::load_topology(&PathBuf::from(&p.topology_path))?;
        Ok(topo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: name/access wiring — reading a topology dereferences an
    // arbitrary fs path, so it is Privileged, never AiSafe.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(PlasticityTopology::NAME, "plasticity/topology");
        assert!(matches!(
            PlasticityTopology::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }
}
