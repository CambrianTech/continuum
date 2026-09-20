//! Sentinel type definitions with ts-rs exports

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use ts_rs::TS;

/// Sentinel execution handle
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/sentinel/SentinelHandle.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct SentinelHandle {
    pub id: String,
    pub sentinel_type: String,
    pub status: SentinelStatus,
    pub progress: u8,
    pub start_time: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub end_time: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
    pub working_dir: String,
    pub logs_dir: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/sentinel/SentinelStatus.ts"
)]
#[serde(rename_all = "lowercase")]
pub enum SentinelStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Pipeline lifecycle status — richer than SentinelStatus for checkpoint persistence
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/sentinel/PipelineStatus.ts"
)]
#[serde(rename_all = "lowercase")]
pub enum PipelineStatus {
    Running,
    Paused,
    WaitingApproval,
    BudgetExhausted,
    Completed,
    Failed,
    Cancelled,
    /// Set on startup for pipelines that were Running when process died
    Interrupted,
}

/// Budget consumed so far during pipeline execution
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/sentinel/BudgetConsumed.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct BudgetConsumed {
    #[ts(type = "number")]
    pub elapsed_secs: u64,
    pub cost_usd: f64,
    #[ts(type = "number")]
    pub tokens_used: u64,
    pub iterations: u32,
}

/// Budget limits — any field None means unlimited
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/sentinel/BudgetLimits.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct BudgetLimits {
    /// e.g. 3600 (1 hour)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(type = "number | undefined")]
    pub max_time_secs: Option<u64>,
    /// e.g. 5.00
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub max_cost_usd: Option<f64>,
    /// e.g. 1_000_000
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(type = "number | undefined")]
    pub max_tokens: Option<u64>,
    /// Full pipeline loop iterations (NOT agent turns)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub max_iterations: Option<u32>,
}

/// Durable checkpoint for pipeline resume after restart
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/sentinel/PipelineCheckpoint.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct PipelineCheckpoint {
    pub sentinel_handle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pipeline_name: Option<String>,
    /// Resume from this step index (next step to execute)
    pub step_index: usize,
    pub step_results: Vec<StepResult>,
    pub budget_consumed: BudgetConsumed,
    pub budget_limits: BudgetLimits,
    /// ISO 8601
    pub started_at: String,
    /// ISO 8601
    pub last_checkpoint_at: String,
    pub status: PipelineStatus,
    /// The full pipeline definition — needed to resume
    pub pipeline: Pipeline,
    /// Working directory at time of checkpoint
    pub working_dir: String,
    /// Escalation metadata for persona routing on resume
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub escalation: Option<SentinelEscalation>,
}

/// Log stream info
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/sentinel/LogStreamInfo.ts"
)]
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
#[ts(
    export,
    export_to = "../../../protocol/typescript/sentinel/PipelineStep.ts"
)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PipelineStep {
    /// Execute a shell command as an isolated child process
    Shell {
        cmd: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        #[serde(rename = "timeoutSecs")]
        timeout_secs: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        #[serde(rename = "workingDir")]
        working_dir: Option<String>,
        /// If true, non-zero exit code doesn't mark the step as failed.
        /// The exit code is still recorded in data.exitCode for condition steps.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        #[serde(rename = "allowFailure")]
        allow_failure: Option<bool>,
        /// Environment variables set on the child process. Values are interpolated.
        /// Use this to pass arbitrary data (code, JSON) safely — env vars bypass
        /// shell quoting issues that break heredocs and embedded strings.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        env: Option<std::collections::HashMap<String, String>>,
    },

    /// LLM inference via AIProviderModule (default) or agentic loop via ai/agent command
    ///
    /// When `agentMode` is false/absent: fast in-process Rust call to ai/generate.
    /// When `agentMode` is true: routes to TypeScript ai/agent command via CommandExecutor
    /// for full tool-calling loop with 243+ discoverable tools.
    Llm {
        prompt: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        model: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        provider: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        #[serde(rename = "maxTokens")]
        max_tokens: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        temperature: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        #[serde(rename = "systemPrompt")]
        system_prompt: Option<String>,
        /// Tool subset for agent mode (undefined = all public, [] = none)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        tools: Option<Vec<String>>,
        /// Enable agentic loop: LLM can call tools, see results, re-generate
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        #[serde(rename = "agentMode")]
        agent_mode: Option<bool>,
        /// Override safety cap for tool iterations in agent mode
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        #[serde(rename = "maxIterations")]
        max_iterations: Option<u32>,
        /// Active LoRA adapters to apply during inference.
        /// Each entry: { name, path, domain?, scale? }
        /// Values are interpolated, so pipeline steps can reference trained adapter paths.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        #[serde(rename = "activeAdapters")]
        #[ts(type = "Array<{ name: string; path: string; domain?: string; scale?: number }>")]
        active_adapters: Option<Vec<serde_json::Value>>,
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
        #[serde(default)]
        #[serde(rename = "else")]
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
        #[ts(optional)]
        count: Option<usize>,
        steps: Vec<PipelineStep>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        #[serde(rename = "while")]
        while_condition: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        until: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        #[serde(rename = "maxIterations")]
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
        #[serde(default)]
        #[serde(rename = "failFast")]
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        #[serde(rename = "timeoutSecs")]
        timeout_secs: Option<u64>,
    },

    /// Execute a nested pipeline inline (recursive composition)
    Sentinel {
        /// The nested pipeline to execute
        pipeline: Box<Pipeline>,
    },

    /// Pause pipeline and wait for human or persona approval before continuing.
    /// Pipeline status becomes WaitingApproval; durable across restart via checkpoint.
    Approve {
        /// What to show the approver: "Review the architecture plan before proceeding"
        prompt: String,
        /// PeerId UUIDs or "human" for Joel
        #[serde(default)]
        approvers: Vec<String>,
        /// Auto-approve after this many seconds (optional — None means wait forever)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        #[serde(rename = "timeoutSecs")]
        timeout_secs: Option<u64>,
    },

    /// Search the web and extract information. Dispatches to TypeScript
    /// sentinel/web-research command (LightPanda headless browser).
    #[serde(rename = "webresearch")]
    WebResearch {
        /// Search query — interpolated: "{{steps.2.data.stderr}} fix for rust"
        query: String,
        /// Max pages to load (default 3)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        #[serde(rename = "maxPages")]
        max_pages: Option<u32>,
        /// What to extract from pages: "code examples", "error solutions", etc.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        extract: Option<String>,
    },

    /// Execute an external coding agent (Claude Code, Codex, etc.)
    ///
    /// Provider selection via `provider` param. Delegates entirely to TypeScript
    /// via `execute_ts_json("sentinel/coding-agent", ...)`.
    /// Every session captures interactions for LoRA training when personaId is set.
    #[serde(rename = "codingagent")]
    CodingAgent {
        /// Task prompt — what the agent should do (interpolated)
        prompt: String,
        /// Which provider: "claude-code" (default), future: "codex", "aider"
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        provider: Option<String>,
        /// Working directory
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        #[serde(rename = "workingDir")]
        working_dir: Option<String>,
        /// System prompt override
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        #[serde(rename = "systemPrompt")]
        system_prompt: Option<String>,
        /// Model override (e.g., "sonnet", "opus")
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        model: Option<String>,
        /// Allowed tools (provider-specific names)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        #[serde(rename = "allowedTools")]
        allowed_tools: Option<Vec<String>>,
        /// Max conversation turns
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        #[serde(rename = "maxTurns")]
        max_turns: Option<u32>,
        /// Max budget in USD
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        #[serde(rename = "maxBudgetUsd")]
        max_budget_usd: Option<f64>,
        /// Permission mode: "default", "acceptEdits", "bypassPermissions"
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        #[serde(rename = "permissionMode")]
        permission_mode: Option<String>,
        /// Resume a prior session
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        #[serde(rename = "resumeSessionId")]
        resume_session_id: Option<String>,
        /// Capture interactions for LoRA training (default: true if personaId set)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        #[serde(rename = "captureTraining")]
        capture_training: Option<bool>,
        /// Persona ID for training attribution
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        #[serde(rename = "personaId")]
        persona_id: Option<String>,
        /// Path to git repo — triggers project worktree workspace (proper git isolation)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        #[serde(rename = "repoPath")]
        repo_path: Option<String>,
        /// Branch slug: ai/sentinel-{handle}/{slug} (default: "work")
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        #[serde(rename = "taskSlug")]
        task_slug: Option<String>,
    },
}

/// A complete pipeline definition
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/sentinel/Pipeline.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct Pipeline {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    pub steps: Vec<PipelineStep>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub working_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    #[ts(type = "Record<string, unknown>")]
    pub inputs: HashMap<String, Value>,
}

/// Result of a single step execution
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/sentinel/StepResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct StepResult {
    pub step_index: usize,
    pub step_type: String,
    pub success: bool,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub exit_code: Option<i32>,
    /// Full result data for complex outputs
    #[serde(default, skip_serializing_if = "Value::is_null")]
    #[ts(type = "unknown")]
    pub data: Value,
}

/// Result of pipeline execution
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/sentinel/PipelineResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct PipelineResult {
    pub handle: String,
    pub success: bool,
    pub total_duration_ms: u64,
    pub steps_completed: usize,
    pub steps_total: usize,
    pub step_results: Vec<StepResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
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
    /// Path to steps.jsonl for real-time sub-step logging.
    /// When set, loop/condition/parallel steps flush results as they complete.
    pub steps_log_path: Option<&'a std::path::Path>,
    /// Substrate-wide command executor — threaded through from the
    /// owning `SentinelModule` so steps that delegate to TS (web-research,
    /// escalation) call `executor.execute_ts_json(...)` instead of the
    /// deleted free-function helper (task #224). `None` only in tests
    /// that don't exercise the TS bridge.
    pub executor: Option<&'a std::sync::Arc<crate::runtime::CommandExecutor>>,
}

/// When a sentinel's terminal state should wake up the owning persona's
/// consciousness. Mirrors the TS `EscalationCondition` union; ts-rs
/// emits the same string literals.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/sentinel/EscalationCondition.ts"
)]
#[serde(rename_all = "snake_case")]
pub enum EscalationCondition {
    Error,
    Timeout,
    Unfamiliar,
    ApprovalNeeded,
    Complete,
}

/// What to do when the escalation condition fires.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/sentinel/EscalationAction.ts"
)]
#[serde(rename_all = "lowercase")]
pub enum EscalationAction {
    Pause,
    Notify,
    Abort,
}

/// How urgently to alert the persona — maps to numeric inbox priority
/// in [`EscalationPriority::inbox_priority`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/sentinel/EscalationPriority.ts"
)]
#[serde(rename_all = "lowercase")]
pub enum EscalationPriority {
    Low,
    Normal,
    High,
    Urgent,
}

impl EscalationPriority {
    /// Inbox priority scalar in `[0.0, 1.0]`. Mirrors the TS
    /// `PRIORITY_MAP` in `SentinelEscalationService.ts`.
    pub fn inbox_priority(self) -> f64 {
        match self {
            Self::Low => 0.3,
            Self::Normal => 0.5,
            Self::High => 0.7,
            Self::Urgent => 0.9,
        }
    }
}

/// One escalation rule. The dispatcher picks the first rule whose
/// `condition` matches the terminal status and acts on it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/sentinel/EscalationRule.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct EscalationRule {
    pub condition: EscalationCondition,
    pub action: EscalationAction,
    pub priority: EscalationPriority,
}

/// Default rules applied when the caller doesn't supply any. Mirrors
/// `DEFAULT_ESCALATION_RULES` in `entities/SentinelEntity.ts`.
pub fn default_escalation_rules() -> Vec<EscalationRule> {
    vec![
        EscalationRule {
            condition: EscalationCondition::Error,
            action: EscalationAction::Notify,
            priority: EscalationPriority::High,
        },
        EscalationRule {
            condition: EscalationCondition::Timeout,
            action: EscalationAction::Notify,
            priority: EscalationPriority::Normal,
        },
        EscalationRule {
            condition: EscalationCondition::Complete,
            action: EscalationAction::Notify,
            priority: EscalationPriority::Low,
        },
    ]
}

/// Escalation metadata captured at sentinel start. The substrate
/// dispatcher in [`super::escalation`] reads this on completion to
/// route the terminal event into the persona's inbox + memory.
///
/// Pre-#225 this was the wire payload of the TS `sentinel/escalate`
/// command. Now everything is in-process Rust; the field set is the
/// same so existing call sites compile unchanged.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/sentinel/SentinelEscalation.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct SentinelEscalation {
    /// Owning persona for inbox delivery
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub parent_persona_id: Option<String>,
    /// SentinelEntity ID for execution history persistence
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub entity_id: Option<String>,
    /// Human-readable name for escalation messages
    pub sentinel_name: String,
    /// Caller-supplied escalation rules. `None` -> dispatcher uses
    /// [`default_escalation_rules`]. The wire shape is typed (not
    /// `Value` pass-through) now that TS no longer owns the schema.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub escalation_rules: Option<Vec<EscalationRule>>,
}

/// Internal state for a running sentinel
pub struct RunningSentinel {
    pub handle: SentinelHandle,
    /// Channel to send cancellation signal
    pub cancel_tx: Option<tokio::sync::mpsc::Sender<()>>,
    /// Escalation metadata — pushed to TypeScript on completion
    pub escalation: Option<SentinelEscalation>,
    /// Completion signal — subscribers receive () when sentinel finishes.
    /// Replaces the TS polling loop with a proper async wait.
    pub completion_tx: Option<tokio::sync::watch::Sender<bool>>,
    pub completion_rx: tokio::sync::watch::Receiver<bool>,
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
        PipelineStep::Approve { .. } => "approve",
        PipelineStep::WebResearch { .. } => "webresearch",
        PipelineStep::CodingAgent { .. } => "codingagent",
    }
}

/// Format a sentinel step error with consistent `[handle_id] context: error` format.
///
/// Used by all step implementations to produce uniform error messages.
pub fn step_err(handle_id: &str, context: &str, error: impl std::fmt::Display) -> String {
    format!("[{handle_id}] {context}: {error}")
}
