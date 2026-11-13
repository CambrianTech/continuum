//! The declared dataflow graph — CBAR gap #2's missing half.
//!
//! Every module already declares what it CONSUMES (`artifact_subscriptions`). This adds
//! what it PRODUCES (`ServiceModule::emissions`) and finds the one-sided wires between
//! them: a consumer no module declares it feeds, and a production no module reads. Those
//! are the defects a compiler cannot see and this codebase keeps shipping (on
//! 2026-09-25 alone: a decode knee measured but never read by the allocator's lane path,
//! a checkpoint primitive with one caller, a teacher batch the curriculum bypassed).
//!
//! Design: `docs/architecture/CBAR-SUBSTRATE-ARCHITECTURE.md` § The Declared Dataflow
//! Graph. Pure: the runtime collects each module's edges at boot and reports the result
//! as the `runtime.event_graph` probe.

use super::artifact_handle::ArtifactSelector;

/// One module's declared edges.
#[derive(Debug, Clone)]
pub struct ModuleEdges {
    pub module: &'static str,
    /// What it declares it publishes (`ServiceModule::emissions`).
    pub emits: Vec<ArtifactSelector>,
    /// What it declares it consumes (`ServiceModule::artifact_subscriptions`).
    pub consumes: Vec<ArtifactSelector>,
}

/// The one-sided wires in a declared graph.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct GraphOrphans {
    /// A consumer that no module declares it feeds: waiting for nothing.
    pub unfed: Vec<(&'static str, ArtifactSelector)>,
    /// A production no module reads and no external sink takes: a measurement nobody
    /// consumes.
    pub unread: Vec<(&'static str, ArtifactSelector)>,
}

/// Could any key be matched by both selectors? `Exact`/`Exact` when equal; `Exact`
/// against `Prefix` when the key starts with the prefix; two prefixes when one extends
/// the other.
pub fn overlaps(a: &ArtifactSelector, b: &ArtifactSelector) -> bool {
    use ArtifactSelector::{Exact, Prefix};
    match (a, b) {
        (Exact(x), Exact(y)) => x == y,
        (Exact(k), Prefix(p)) | (Prefix(p), Exact(k)) => k.as_str().starts_with(p.as_str()),
        (Prefix(p), Prefix(q)) => p.starts_with(q.as_str()) || q.starts_with(p.as_str()),
    }
}

/// Find the one-sided wires. `external_sinks` are productions declared as leaving the
/// process (a positron ViewState, the airc wire, a client); an emission they take is
/// read, not orphaned. A module feeding its own subscription counts as fed.
pub fn orphans(modules: &[ModuleEdges], external_sinks: &[ArtifactSelector]) -> GraphOrphans {
    let mut out = GraphOrphans::default();
    for m in modules {
        for want in &m.consumes {
            let fed = modules.iter().any(|p| p.emits.iter().any(|e| overlaps(e, want)));
            if !fed {
                out.unfed.push((m.module, want.clone()));
            }
        }
        for made in &m.emits {
            let read = modules.iter().any(|c| c.consumes.iter().any(|w| overlaps(w, made)))
                || external_sinks.iter().any(|s| overlaps(s, made));
            if !read {
                out.unread.push((m.module, made.clone()));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::artifact_handle::ArtifactKey;

    fn exact(k: &str) -> ArtifactSelector {
        ArtifactSelector::Exact(ArtifactKey(k.into()))
    }
    fn prefix(p: &str) -> ArtifactSelector {
        ArtifactSelector::Prefix(p.into())
    }

    // what this catches: the matcher agrees with ArtifactSelector::matches on every
    // pairing, so a declared edge is found exactly when delivery would happen.
    #[test]
    fn selectors_overlap_exactly_when_a_key_could_match_both() {
        assert!(overlaps(&exact("serving/knee"), &exact("serving/knee")));
        assert!(!overlaps(&exact("serving/knee"), &exact("serving/plan")));
        assert!(overlaps(&exact("serving/knee"), &prefix("serving/")));
        assert!(overlaps(&prefix("serving/"), &exact("serving/knee")));
        assert!(!overlaps(&exact("genome/x"), &prefix("serving/")));
        assert!(overlaps(&prefix("serving/"), &prefix("serving/kv/")));
        assert!(!overlaps(&prefix("serving/"), &prefix("genome/")));
    }

    // what this catches (2026-09-25, the knee the allocator never read): a production
    // with no reader and a consumer with no producer are both named, with their module;
    // a matched pair, a self-fed module, and an emission taken by an external sink are not.
    #[test]
    fn one_sided_wires_are_named_and_closed_ones_are_not() {
        let modules = vec![
            ModuleEdges { module: "knee", emits: vec![exact("serving/knee")], consumes: vec![] },
            ModuleEdges { module: "planner", emits: vec![exact("serving/plan")], consumes: vec![prefix("serving/kn")] },
            ModuleEdges { module: "teacher", emits: vec![], consumes: vec![exact("genome/curriculum")] },
            ModuleEdges { module: "view", emits: vec![exact("positron/serving")], consumes: vec![] },
            ModuleEdges { module: "loop", emits: vec![exact("loop/tick")], consumes: vec![exact("loop/tick")] },
        ];
        let got = orphans(&modules, &[prefix("positron/")]);
        assert_eq!(got.unfed, vec![("teacher", exact("genome/curriculum"))], "a consumer nobody feeds");
        assert_eq!(got.unread, vec![("planner", exact("serving/plan"))], "a production nobody reads");
    }
}
