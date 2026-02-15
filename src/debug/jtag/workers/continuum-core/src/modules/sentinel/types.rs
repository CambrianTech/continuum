//! Sentinel type definitions with ts-rs exports

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use ts_rs::TS;

/// Sentinel execution handle
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/sentinel/SentinelHandle.ts")]
#[serde(rename_all = "camelCase")]
pub struct SentinelHandle {
    pub id: String,
    pub sentinel_type: String,
    pub status: SentinelStatus,
    pub progress: u8,
    pub start_time: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub working_dir: String,
    pub logs_dir: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/sentinel/SentinelStatus.ts")]
#[serde(rename_all = "lowercase")]
pub enum SentinelStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Log stream info
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/sentinel/LogStreamInfo.ts")]
#[serde(rename_all = "camelCase")]
pub struct LogStreamInfo {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub modified_at: String,
}

/// A single step in a pipeline
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/sentinel/PipelineStep.ts")]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PipelineStep {
    /// Execute a shell command
    Shell {
        cmd: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout_secs: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "workingDir")]
        working_dir: Option<String>,
    },

    /// LLM inference (calls AIProviderModule directly)
    Llm {
        prompt: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        provider: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "maxTokens")]
        max_tokens: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        temperature: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "systemPrompt")]
        system_prompt: Option<String>,
    },

    /// Call any command via ModuleRegistry
    Command {
        command: String,
        #[serde(default)]
        #[ts(type = "Record<string, unknown>")]
        params: Value,
    },

    /// Conditional execution
    Condition {
        #[serde(rename = "if")]
        condition: String,
        #[serde(rename = "then")]
        then_steps: Vec<PipelineStep>,
        #[serde(default, rename = "else")]
        else_steps: Vec<PipelineStep>,
    },

    /// Loop with count
    Loop {
        count: usize,
        steps: Vec<PipelineStep>,
    },
}

/// A complete pipeline definition
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/sentinel/Pipeline.ts")]
#[serde(rename_all = "camelCase")]
pub struct Pipeline {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub steps: Vec<PipelineStep>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    #[ts(type = "Record<string, unknown>")]
    pub inputs: HashMap<String, Value>,
}

/// Result of a single step execution
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/sentinel/StepResult.ts")]
#[serde(rename_all = "camelCase")]
pub struct StepResult {
    pub step_index: usize,
    pub step_type: String,
    pub success: bool,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Full result data for complex outputs
    #[serde(default, skip_serializing_if = "Value::is_null")]
    #[ts(type = "unknown")]
    pub data: Value,
}

/// Result of pipeline execution
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/sentinel/PipelineResult.ts")]
#[serde(rename_all = "camelCase")]
pub struct PipelineResult {
    pub handle: String,
    pub success: bool,
    pub total_duration_ms: u64,
    pub steps_completed: usize,
    pub steps_total: usize,
    pub step_results: Vec<StepResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Execution context for variable interpolation
#[derive(Debug, Clone, Default)]
pub struct ExecutionContext {
    /// Results from previous steps (by index)
    pub step_results: Vec<StepResult>,
    /// Pipeline inputs
    pub inputs: HashMap<String, Value>,
    /// Working directory
    pub working_dir: PathBuf,
}

/// Immutable context shared across all step executions in a pipeline.
/// Groups the references that every recursive step needs.
pub struct PipelineContext<'a> {
    pub handle_id: &'a str,
    pub registry: &'a std::sync::Arc<crate::runtime::ModuleRegistry>,
    pub bus: Option<&'a std::sync::Arc<crate::runtime::message_bus::MessageBus>>,
}

/// Internal state for a running sentinel
pub struct RunningSentinel {
    pub handle: SentinelHandle,
    /// Channel to send cancellation signal
    pub cancel_tx: Option<tokio::sync::mpsc::Sender<()>>,
}

/// Get the type name of a step for logging
pub fn step_type_name(step: &PipelineStep) -> &'static str {
    match step {
        PipelineStep::Shell { .. } => "shell",
        PipelineStep::Llm { .. } => "llm",
        PipelineStep::Command { .. } => "command",
        PipelineStep::Condition { .. } => "condition",
        PipelineStep::Loop { .. } => "loop",
    }
}
