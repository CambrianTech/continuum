#![cfg(test)]
//! From `cognition::llm_deliberation_faculty`: the tool surface and the prompt budget priced
//! against the REAL registry.

use std::collections::BTreeSet;
use std::sync::Arc;

use continuum_base::token_estimate::estimate_guard_tokens;
use serde_json::json;
use uuid::Uuid;

use crate::ai::adapter::AIProviderAdapter;
use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
use crate::ai::types::{ChatMessage, FinishReason, NativeToolSpec, ToolInputSchema};
use crate::cognition::llm_deliberation_faculty::test_support::*;
use crate::cognition::llm_deliberation_faculty::*;
use crate::cognition::persona_tools;
use crate::cognition::workspace::{BurstTurn, Contribution, Faculty, FacultyId, Workspace};

mod budget;
mod prompt_shaping;
mod surface;
mod verdicts;
