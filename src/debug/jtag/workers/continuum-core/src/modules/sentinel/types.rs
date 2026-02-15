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

/// A single step in a pipeline.
///
/// Each variant maps to a JSON object with `"type": "<variant>"`.
/// Steps compose recursively — condition, loop, parallel, and sentinel
/// all contain nested steps, enabling arbitrarily complex pipelines.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/sentinel/PipelineStep.ts")]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PipelineStep {
    /// Execute a shell command as an isolated child process
    Shell {
        cmd: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "timeoutSecs")]
        timeout_secs: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "workingDir")]
        working_dir: Option<String>,
    },

    /// LLM inference via AIProviderModule
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

    /// Route to any command (Rust or TypeScript) via CommandExecutor
    Command {
        command: String,
        #[serde(default)]
        #[ts(type = "Record<string, unknown>")]
        params: Value,
    },

    /// Branch based on interpolated condition expression
    Condition {
        #[serde(rename = "if")]
        condition: String,
        #[serde(rename = "then")]
        then_steps: Vec<PipelineStep>,
        #[serde(default, rename = "else")]
        else_steps: Vec<PipelineStep>,
    },

    /// Iterate over sub-steps with flexible termination modes.
    ///
    /// Modes (exactly one should be specified):
    /// - `count`: fixed N iterations
    /// - `while`: condition checked before each iteration, continues while truthy
    /// - `until`: condition checked after each iteration, stops when truthy
    /// - none of the above + `maxIterations`: continuous loop with safety limit
    ///
    /// `maxIterations` provides a safety cap for while/until/continuous modes.
    /// Defaults to 10000 if omitted on non-count loops.
    Loop {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        count: Option<usize>,
        steps: Vec<PipelineStep>,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "while")]
        while_condition: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        until: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "maxIterations")]
        max_iterations: Option<usize>,
    },

    /// Execute multiple branch pipelines concurrently.
    ///
    /// Each branch is a sequence of steps. All branches start simultaneously.
    /// Each branch gets a snapshot of the execution context at fork time.
    Parallel {
        /// Each branch is a sequence of steps executed in order
        branches: Vec<Vec<PipelineStep>>,
        /// If true, cancel remaining branches on first failure (default: false)
        #[serde(default, rename = "failFast")]
        fail_fast: bool,
    },

    /// Publish an event on the MessageBus for inter-sentinel composition
    Emit {
        /// Event name (e.g. "build:complete", "sentinel:custom:done")
        event: String,
        /// Arbitrary JSON payload (interpolated before emission)
        #[serde(default)]
        #[ts(type = "Record<string, unknown>")]
        payload: Value,
    },

    /// Block until a matching event arrives on the MessageBus
    Watch {
        /// Event name pattern to match
        event: String,
        /// Timeout in seconds (default: 300)
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "timeoutSecs")]
        timeout_secs: Option<u64>,
    },

    /// Execute a nested pipeline inline (recursive composition)
    Sentinel {
        /// The nested pipeline to execute
        pipeline: Box<Pipeline>,
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

/// Execution context for variable interpolation.
///
/// Carried through the pipeline, accumulating step results.
/// Cloned at fork points (parallel branches) so branches share
/// a read-only snapshot but diverge independently.
#[derive(Debug, Clone, Default)]
pub struct ExecutionContext {
    /// Results from previous steps (by index)
    pub step_results: Vec<StepResult>,
    /// Pipeline inputs (also used for loop iteration variable)
    pub inputs: HashMap<String, Value>,
    /// Working directory for shell commands
    pub working_dir: PathBuf,
    /// Named outputs for cleaner interpolation: {{named.build.output}}
    pub named_outputs: HashMap<String, StepResult>,
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

/// Safety limit for while/until/continuous loops when maxIterations is omitted
pub const DEFAULT_MAX_ITERATIONS: usize = 10_000;

/// Get the type name of a step for logging
pub fn step_type_name(step: &PipelineStep) -> &'static str {
    match step {
        PipelineStep::Shell { .. } => "shell",
        PipelineStep::Llm { .. } => "llm",
        PipelineStep::Command { .. } => "command",
        PipelineStep::Condition { .. } => "condition",
        PipelineStep::Loop { .. } => "loop",
        PipelineStep::Parallel { .. } => "parallel",
        PipelineStep::Emit { .. } => "emit",
        PipelineStep::Watch { .. } => "watch",
        PipelineStep::Sentinel { .. } => "sentinel",
    }
}
