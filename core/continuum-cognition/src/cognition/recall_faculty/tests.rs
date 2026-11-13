#![cfg(test)]
    use super::*;

    use crate::cognition::engram::{ChatMessageRef, Engram, EngramKind, EngramOrigin, TrustState};

    use crate::cognition::recall_metadata::{RecallMetadata, RecallMetadataRegistry};

    // ---- The mind in action: real hippocampus → workspace → informed decision ----
    use super::super::workspace::{Decision, NoopWorkspaceCaptureSink, WorkspaceCaptureSink, WorkspaceCycle, WorkspaceTrace};

mod near_identical_own_turns_collapse_but_distinct_memories_survive;
use near_identical_own_turns_collapse_but_distinct_memories_survive::*;
mod mind_in_action_recall_informs_the_decision;
