#![cfg(test)]
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
    use std::time::Duration;

    use tokio::sync::{watch, Notify};
    use tokio::time::timeout;

    use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;

    use crate::cognition::workspace::Decision;

    use crate::cognition::engram::{ChatMessageRef, Engram, EngramKind, EngramOrigin, TrustState};

    use crate::cognition::recall_metadata::{RecallMetadata, RecallMetadataRegistry};

mod she_is_told_when_her_hands_move_and_only_then;
mod mind_results_reporter;
mod final_checkpoint_drains_admitted_periodic_writer_before_ack;
use final_checkpoint_drains_admitted_periodic_writer_before_ack::*;
mod gated_grounding;
use gated_grounding::*;

mod task_consumption;
mod perception_consumption;
