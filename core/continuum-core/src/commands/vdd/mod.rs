//! `vdd/<verb>` — the VDD (Verifiable Deployment Data) telemetry surface as typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand)s.
//!
//! Two verbs, two shapes:
//! - **`vdd/report`** (dep-holding) — reads telemetry records from the artifact
//!   store (`~/.continuum/vdd`). Captures the artifact root the
//!   [`VddModule`](crate::modules::vdd::VddModule) owns, so tests can point it at a
//!   temp dir. Contributed via [`command_objects`].
//! - **`vdd/score`** (stateless) — a pure deterministic scorer for the
//!   self-evolving-genome A/B. No module state, so it self-registers via
//!   `register_stateless_command!` — it does NOT appear in [`command_objects`].

use std::path::PathBuf;
use std::sync::Arc;

use crate::sdk_codegen::DynCommand;

pub mod report;
pub mod score;

use report::VddReportQuery;

/// The dep-holding `vdd/*` family the [`VddModule`](crate::modules::vdd::VddModule)
/// contributes to the kernel's typed object map. Only `vdd/report` holds state (the
/// artifact root); `vdd/score` is stateless and self-registers.
pub fn command_objects(artifact_root: PathBuf) -> Vec<Arc<dyn DynCommand>> {
    vec![Arc::new(VddReportQuery { artifact_root })]
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the dep-holding family exposes exactly the stateful verb
    // (`vdd/report`). `vdd/score` is stateless and must NOT be here — a regression
    // that double-registers it (descriptor + family) trips the duplicate-name panic.
    #[test]
    fn family_exposes_only_the_stateful_report_verb() {
        let objs = command_objects(PathBuf::from("/tmp/vdd-test-root"));
        let names: Vec<&str> = objs.iter().map(|o| o.name()).collect();
        assert_eq!(names, vec!["vdd/report"]);
    }
}
