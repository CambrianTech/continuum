//! AgentModule — Autonomous AI coding agent with structured tool calling.
//!
//! Unlike sentinels which are script-based, agents:
//! 1. Have STRUCTURED TOOL DEFINITIONS - LLM sees proper tool schemas
//! 2. Use TOOL CALLING protocol - Real structured output parsing
//! 3. Maintain FULL CONTEXT - Conversation history
//! 4. ITERATE until done - Build, test, fix loop
//! 5. Handle REAL TASKS - Multi-file changes, codebase exploration
//!
//! Commands:
//! - agent/start: Start an autonomous agent (returns handle immediately)
//! - agent/status: Get agent status and progress
//! - agent/stop: Stop a running agent
//! - agent/list: List all agents (running and completed)
//!
//! Events emitted:
//! - agent:{handle}:progress - Progress updates
//! - agent:{handle}:action - Tool calls and results
//! - agent:{handle}:complete - Agent finished (success or failure)
//!
//! Priority: Normal — agents are long-running background tasks.

use crate::ai::adapter::{AIProviderAdapter, InferenceDevice};
use crate::log_info;
use crate::runtime::{
    CommandResult, MessageBus, ModuleConfig, ModuleContext, ModulePriority, ServiceModule,
};
use async_trait::async_trait;
use dashmap::DashMap;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::any::Any;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Notify;
use ts_rs::TS;
use uuid::Uuid;

// ============================================================================
// TOOL DEFINITIONS - Simple structs, generated at runtime when needed
// ============================================================================

/// Available tool names - the source of truth
pub const TOOL_NAMES: &[&str] = &[
    "read_file",
    "write_file",
    "edit_file",
    "search_files",
    "list_files",
    "run_command",
    "git_status",
    "git_diff",
    "complete",
    "give_up",
];

// ============================================================================
// AGENT STATE
// ============================================================================

/// Agent execution status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "../../../protocol/typescript/agent/AgentStatus.ts")]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Running,
    Completed,
    Failed,
    Stopped,
}

/// A single tool call made by the agent
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/agent/AgentToolCall.ts")]
pub struct ToolCall {
    pub name: String,
    #[ts(type = "Record<string, unknown>")]
    pub arguments: Value,
}

/// Result of executing a tool
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/agent/AgentToolResult.ts"
)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}

/// A single action taken by the agent
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/agent/AgentAction.ts")]
pub struct AgentAction {
    pub timestamp: String,
    pub action_type: String,
    #[ts(optional)]
    pub tool_name: Option<String>,
    #[ts(optional, type = "Record<string, unknown>")]
    pub tool_args: Option<Value>,
    #[ts(optional)]
    pub result: Option<ToolResult>,
    #[ts(optional)]
    pub thought: Option<String>,
}

/// A typed snapshot of an agent's progress — the wire output of `agent/status`,
/// `agent/list`, and `agent/wait`. Replaces the old ad-hoc `to_status_json` Value
/// so every reader gets a contracted shape + a ts-rs binding.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/agent/AgentStatusInfo.ts"
)]
pub struct AgentStatusInfo {
    pub handle: String,
    pub task: String,
    pub working_dir: String,
    pub status: AgentStatus,
    pub iteration: u32,
    pub max_iterations: u32,
    #[ts(type = "number")]
    pub elapsed_ms: u64,
    pub actions_count: u32,
    pub files_created: Vec<String>,
    pub files_modified: Vec<String>,
    #[ts(optional)]
    pub summary: Option<String>,
    #[ts(optional)]
    pub error: Option<String>,
}

/// Full agent state
#[derive(Debug)]
pub struct AgentState {
    pub handle: String,
    pub task: String,
    pub working_dir: PathBuf,
    pub status: AgentStatus,
    pub iteration: u32,
    pub max_iterations: u32,
    pub started_at: Instant,
    pub completed_at: Option<Instant>,
    pub actions: Vec<AgentAction>,
    pub files_created: Vec<String>,
    pub files_modified: Vec<String>,
    pub summary: Option<String>,
    pub error: Option<String>,
    /// Notify when agent completes (for waiters)
    pub completion_notify: Arc<Notify>,
    /// Stop signal
    pub stop_requested: bool,
}

impl AgentState {
    pub fn new(handle: String, task: String, working_dir: PathBuf, max_iterations: u32) -> Self {
        Self {
            handle,
            task,
            working_dir,
            status: AgentStatus::Running,
            iteration: 0,
            max_iterations,
            started_at: Instant::now(),
            completed_at: None,
            actions: Vec::new(),
            files_created: Vec::new(),
            files_modified: Vec::new(),
            summary: None,
            error: None,
            completion_notify: Arc::new(Notify::new()),
            stop_requested: false,
        }
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.started_at.elapsed().as_millis() as u64
    }

    pub fn to_status_info(&self) -> AgentStatusInfo {
        AgentStatusInfo {
            handle: self.handle.clone(),
            task: self.task.clone(),
            working_dir: self.working_dir.to_string_lossy().into_owned(),
            status: self.status.clone(),
            iteration: self.iteration,
            max_iterations: self.max_iterations,
            elapsed_ms: self.elapsed_ms(),
            actions_count: self.actions.len() as u32,
            files_created: self.files_created.clone(),
            files_modified: self.files_modified.clone(),
            summary: self.summary.clone(),
            error: self.error.clone(),
        }
    }
}

// ============================================================================
// MODULE
// ============================================================================

/// Shared agent-service state captured by the typed `agent/*` command objects and
/// the host [`AgentModule`]. Holds the live agent map, the runtime handle used to
/// spawn background loops, and the event bus (set at module init). Wrapped in `Arc`
/// so every command + the module share ONE map: one caller's `agent/start` and
/// another's `agent/status` see the same agents.
pub struct AgentService {
    /// Active and completed agents
    agents: Arc<DashMap<String, std::sync::Mutex<AgentState>>>,
    /// Tokio runtime for spawning agent tasks
    rt_handle: tokio::runtime::Handle,
    /// Message bus for events
    bus: std::sync::OnceLock<Arc<MessageBus>>,
}

impl AgentService {
    pub fn new(rt_handle: tokio::runtime::Handle) -> Self {
        Self {
            agents: Arc::new(DashMap::new()),
            rt_handle,
            bus: std::sync::OnceLock::new(),
        }
    }

    /// Set the event bus (called once from module init). Idempotent.
    pub fn set_bus(&self, bus: Arc<MessageBus>) {
        let _ = self.bus.set(bus);
    }

    /// Look up an agent's status snapshot, or `None` if no such handle.
    pub fn status_of(&self, handle: &str) -> Option<AgentStatusInfo> {
        self.agents
            .get(handle)
            .and_then(|entry| entry.lock().ok().map(|state| state.to_status_info()))
    }

    /// Snapshot every live agent.
    pub fn list(&self) -> Vec<AgentStatusInfo> {
        self.agents
            .iter()
            .filter_map(|entry| entry.lock().ok().map(|state| state.to_status_info()))
            .collect()
    }

    /// Request a running agent stop. Returns `true` if the handle was found.
    pub fn request_stop(&self, handle: &str) -> bool {
        if let Some(entry) = self.agents.get(handle) {
            if let Ok(mut state) = entry.lock() {
                state.stop_requested = true;
                log_info!("module", "agent", "Stop requested for agent {}", handle);
                return true;
            }
        }
        false
    }

    /// Wait for an agent to finish, up to `timeout_ms`. `Ok(Some(_))` is the final
    /// status, `Ok(None)` means the handle was unknown (or vanished post-completion,
    /// since completed agents are evicted to free their conversation history), and
    /// `Err` is a timeout.
    pub async fn wait_for(
        &self,
        handle: &str,
        timeout_ms: u64,
    ) -> Result<Option<AgentStatusInfo>, String> {
        // Snapshot the completion-notify under the lock, releasing it before we await
        // (never hold a Mutex guard across an await point). Already-finished agents
        // return immediately.
        let notify = {
            match self.agents.get(handle) {
                Some(entry) => match entry.lock() {
                    Ok(state) => {
                        if state.status != AgentStatus::Running {
                            return Ok(Some(state.to_status_info()));
                        }
                        state.completion_notify.clone()
                    }
                    Err(_) => return Ok(None),
                },
                None => return Ok(None),
            }
        };

        match tokio::time::timeout(Duration::from_millis(timeout_ms), notify.notified()).await {
            // Completed: re-read the final state. May already be evicted (None).
            Ok(_) => Ok(self.status_of(handle)),
            Err(_) => Err("Timeout waiting for agent".to_string()),
        }
    }

    /// Start an autonomous agent in the background. Generates a fresh handle,
    /// inserts the initial state, spawns the agent loop, and returns the handle
    /// immediately so the caller can poll `status`/`wait`.
    pub fn spawn_agent(
        &self,
        task: String,
        working_dir: PathBuf,
        max_iterations: u32,
        model: String,
    ) -> String {
        let handle = format!("agent-{}", &Uuid::new_v4().to_string()[..8]);
        let agents = self.agents.clone();
        let bus = self.bus.get().cloned();

        // Create initial state
        let state = AgentState::new(
            handle.clone(),
            task.clone(),
            working_dir.clone(),
            max_iterations,
        );
        let completion_notify = state.completion_notify.clone();
        agents.insert(handle.clone(), std::sync::Mutex::new(state));

        // The handle returned to the caller; `handle` itself is moved into the loop.
        let returned = handle.clone();

        // Spawn background task
        self.rt_handle.spawn(async move {
            let result = run_agent_loop(
                handle.clone(),
                task,
                working_dir,
                max_iterations,
                model,
                agents.clone(),
                bus,
            )
            .await;

            // Update final state, notify waiters, then remove from map.
            // Keeping completed agents in the DashMap leaks memory — each holds
            // full conversation history, tool call results, and file contents.
            // With 14 personas doing constant tool calls, this is GB/hour.
            if let Some(entry) = agents.get(&handle) {
                if let Ok(mut state) = entry.lock() {
                    state.completed_at = Some(Instant::now());
                    match result {
                        Ok((summary, files_created, files_modified)) => {
                            state.status = AgentStatus::Completed;
                            state.summary = Some(summary);
                            state.files_created = files_created;
                            state.files_modified = files_modified;
                        }
                        Err(e) => {
                            if state.stop_requested {
                                state.status = AgentStatus::Stopped;
                            } else {
                                state.status = AgentStatus::Failed;
                            }
                            state.error = Some(e);
                        }
                    }
                }
            }

            // Notify waiters
            completion_notify.notify_waiters();

            // Remove completed agent from map — frees conversation history,
            // tool results, and all accumulated state.
            agents.remove(&handle);
        });

        returned
    }
}

/// The host shell for the `agent/*` surface. Holds the shared [`AgentService`]
/// (the live agent map + runtime handle + bus) that the typed command objects
/// capture; the verbs themselves are migrated to [`crate::commands::agent`].
pub struct AgentModule {
    service: Arc<AgentService>,
}

impl AgentModule {
    pub fn new(rt_handle: tokio::runtime::Handle) -> Self {
        Self {
            service: Arc::new(AgentService::new(rt_handle)),
        }
    }
}

/// Main agent loop - runs in background tokio task
async fn run_agent_loop(
    handle: String,
    task: String,
    working_dir: PathBuf,
    max_iterations: u32,
    model: String,
    agents: Arc<DashMap<String, std::sync::Mutex<AgentState>>>,
    bus: Option<Arc<MessageBus>>,
) -> Result<(String, Vec<String>, Vec<String>), String> {
    let mut conversation: Vec<Value> = vec![
        json!({
            "role": "system",
            "content": build_system_prompt(&working_dir)
        }),
        json!({
            "role": "user",
            "content": task
        }),
    ];

    let mut files_created: Vec<String> = Vec::new();
    let mut files_modified: Vec<String> = Vec::new();

    for iteration in 1..=max_iterations {
        // Check for stop request
        {
            if let Some(entry) = agents.get(&handle) {
                if let Ok(mut state) = entry.lock() {
                    if state.stop_requested {
                        return Err("Agent stopped by user".to_string());
                    }
                    state.iteration = iteration;
                }
            }
        }

        // Emit progress event
        if let Some(ref b) = bus {
            b.publish_async_only(
                &format!("agent:{}:progress", handle),
                json!({
                    "handle": handle,
                    "iteration": iteration,
                    "max_iterations": max_iterations,
                }),
            );
        }

        // Get LLM response
        let response = call_llm(&conversation, &model, &working_dir).await?;

        // Parse tool calls from response
        let tool_calls = parse_tool_calls(&response);

        if tool_calls.is_empty() {
            // No tool calls - LLM is just thinking, add to context and continue
            conversation.push(json!({
                "role": "assistant",
                "content": response
            }));

            // Record thought action
            if let Some(entry) = agents.get(&handle) {
                if let Ok(mut state) = entry.lock() {
                    state.actions.push(AgentAction {
                        timestamp: chrono::Utc::now().to_rfc3339(),
                        action_type: "thought".to_string(),
                        tool_name: None,
                        tool_args: None,
                        result: None,
                        thought: Some(response.chars().take(500).collect()),
                    });
                }
            }
            continue;
        }

        // Record assistant message
        conversation.push(json!({
            "role": "assistant",
            "content": response,
            "tool_calls": tool_calls
        }));

        // Execute tools IN PARALLEL using Rayon
        let tool_results: Vec<(ToolCall, ToolResult)> = tool_calls
            .par_iter()
            .map(|call| {
                let result = execute_tool(call, &working_dir);
                (call.clone(), result)
            })
            .collect();

        // Process results
        for (call, result) in &tool_results {
            // Add tool result to conversation
            conversation.push(json!({
                "role": "tool",
                "name": call.name,
                "content": result.output
            }));

            // Track file changes
            if call.name == "write_file" {
                if let Some(path) = call.arguments.get("path").and_then(|v| v.as_str()) {
                    if !files_created.contains(&path.to_string())
                        && !files_modified.contains(&path.to_string())
                    {
                        // Check if file existed before
                        let full_path = working_dir.join(path);
                        if full_path.exists() {
                            files_modified.push(path.to_string());
                        } else {
                            files_created.push(path.to_string());
                        }
                    }
                }
            }

            if call.name == "edit_file" {
                if let Some(path) = call.arguments.get("path").and_then(|v| v.as_str()) {
                    if !files_modified.contains(&path.to_string()) {
                        files_modified.push(path.to_string());
                    }
                }
            }

            // Record action
            if let Some(entry) = agents.get(&handle) {
                if let Ok(mut state) = entry.lock() {
                    state.actions.push(AgentAction {
                        timestamp: chrono::Utc::now().to_rfc3339(),
                        action_type: "tool_call".to_string(),
                        tool_name: Some(call.name.clone()),
                        tool_args: Some(call.arguments.clone()),
                        result: Some(result.clone()),
                        thought: None,
                    });
                }
            }

            // Emit action event
            if let Some(ref b) = bus {
                b.publish_async_only(
                    &format!("agent:{}:action", handle),
                    json!({
                        "handle": handle,
                        "tool": call.name,
                        "success": result.success,
                    }),
                );
            }

            // Check for completion
            if call.name == "complete" {
                let summary = call
                    .arguments
                    .get("summary")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Task completed")
                    .to_string();

                // Emit complete event
                if let Some(ref b) = bus {
                    b.publish_async_only(
                        &format!("agent:{}:complete", handle),
                        json!({
                            "handle": handle,
                            "success": true,
                            "summary": summary,
                            "files_created": files_created,
                            "files_modified": files_modified,
                        }),
                    );
                }

                return Ok((summary, files_created, files_modified));
            }

            if call.name == "give_up" {
                let reason = call
                    .arguments
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Task failed")
                    .to_string();

                // Emit complete event (failure)
                if let Some(ref b) = bus {
                    b.publish_async_only(
                        &format!("agent:{}:complete", handle),
                        json!({
                            "handle": handle,
                            "success": false,
                            "reason": reason,
                        }),
                    );
                }

                return Err(reason);
            }
        }
    }

    Err(format!("Max iterations ({}) reached", max_iterations))
}

/// Build system prompt with tool definitions
fn build_system_prompt(working_dir: &Path) -> String {
    // Tool documentation - the single source of truth for tool descriptions
    let tool_docs = r#"read_file: Read a file from the workspace. Returns content with line numbers.
  - path: (required) Relative path to the file
  - start_line: Start line (1-indexed, optional)
  - end_line: End line (1-indexed, optional)

write_file: Write content to a file. Creates directories if needed.
  - path: (required) Relative path to the file
  - content: (required) File content to write

edit_file: Make a targeted edit to a file. Use search/replace pattern.
  - path: (required) Relative path to the file
  - search: (required) Exact text to find (must be unique in file)
  - replace: (required) Text to replace it with

search_files: Search for a regex pattern across all files. Returns matching lines.
  - pattern: (required) Regex pattern to search for
  - file_glob: Glob pattern to filter files (e.g., "*.ts")
  - max_results: Maximum matches to return (default: 50)

list_files: List files in a directory. Shows directory tree structure.
  - path: Directory path (default: workspace root)
  - depth: Max depth to traverse (default: 3)
  - pattern: Glob pattern to filter (e.g., "*.ts")

run_command: Execute a shell command. Use for builds, tests, etc.
  - command: (required) Shell command to execute
  - timeout_ms: Timeout in milliseconds (default: 60000)

git_status: Show git status - modified, staged, and untracked files.
  (no parameters)

git_diff: Show git diff of changes.
  - path: Specific file to diff (optional)
  - staged: Show staged changes only

complete: Signal that the task is complete. Provide a summary.
  - summary: (required) Summary of what was accomplished
  - files_changed: List of files created or modified

give_up: Signal that the task cannot be completed. Explain why.
  - reason: (required) Why the task cannot be completed
  - attempted: What was attempted"#;

    format!(
        r#"You are an expert software engineer working on a coding task. You have access to the following tools:

{}

IMPORTANT RULES:
1. Always explore the codebase before making changes (use list_files, search_files, read_file)
2. Make targeted edits rather than rewriting entire files when possible
3. Test your changes by running builds/tests after modifications
4. If a build fails, read the error output carefully and fix the issues
5. When you're done, call 'complete' with a summary
6. If you truly cannot complete the task, call 'give_up' with an explanation

To use a tool, output a JSON block in this format:
```tool
{{"name": "tool_name", "arguments": {{"arg1": "value1"}}}}
```

You can call multiple tools in one response by including multiple tool blocks.

Working directory: {}"#,
        tool_docs,
        working_dir.display()
    )
}

/// Call the LLM for next response via AI provider module
async fn call_llm(
    conversation: &[Value],
    model: &str,
    _working_dir: &Path,
) -> Result<String, String> {
    use crate::ai::{
        AdapterRegistry, AnthropicAdapter, ChatMessage, MessageContent, OpenAICompatibleAdapter,
        TextGenerationRequest,
    };
    use crate::secrets::get_secret;

    // Build messages array for AI provider
    let messages: Vec<ChatMessage> = conversation
        .iter()
        .filter_map(|msg| {
            let role = msg.get("role").and_then(|v| v.as_str())?;
            let content = msg.get("content").and_then(|v| v.as_str())?;
            // Map tool role to user (tool results are context)
            let mapped_role = match role {
                "tool" => "user",
                r => r,
            };
            Some(ChatMessage {
                role: mapped_role.to_string(),
                content: MessageContent::Text(content.to_string()),
                name: None,
            })
        })
        .collect();

    // Create adapter registry with available providers
    let mut registry = AdapterRegistry::new();

    // Register adapters based on available API keys — init-then-
    // register per task #162 (registry stores Arc; no
    // initialize_all on the registry).
    if get_secret("DEEPSEEK_API_KEY").is_some() {
        let mut a = OpenAICompatibleAdapter::from_registry("deepseek");
        a.initialize().await?;
        registry.register(std::sync::Arc::new(a), 0);
    }
    if get_secret("ANTHROPIC_API_KEY").is_some() {
        let mut a = AnthropicAdapter::new();
        a.initialize().await?;
        registry.register(std::sync::Arc::new(a), 1);
    }
    if get_secret("OPENAI_API_KEY").is_some() {
        let mut a = OpenAICompatibleAdapter::from_registry("openai");
        a.initialize().await?;
        registry.register(std::sync::Arc::new(a), 2);
    }
    if get_secret("GROQ_API_KEY").is_some() {
        let mut a = OpenAICompatibleAdapter::from_registry("groq");
        a.initialize().await?;
        registry.register(std::sync::Arc::new(a), 3);
    }
    if get_secret("TOGETHER_API_KEY").is_some() {
        let mut a = OpenAICompatibleAdapter::from_registry("together");
        a.initialize().await?;
        registry.register(std::sync::Arc::new(a), 4);
    }

    // Select adapter based on model
    let (_provider_id, adapter) = registry
        .select(None, Some(model), InferenceDevice::default())
        .ok_or_else(|| {
            let available = registry.available();
            if available.is_empty() {
                "No AI providers available. Add API keys to ~/.continuum/config.env".to_string()
            } else {
                format!(
                    "Model {} not available. Available providers: {:?}",
                    model, available
                )
            }
        })?;

    // Use AI provider module - routes to DeepSeek, Anthropic, OpenAI, etc.
    let request = TextGenerationRequest {
        messages,
        system_prompt: None,
        model: Some(model.to_string()),
        provider: None, // Auto-select based on model name
        temperature: Some(0.7),
        // Model owns its length (None → adapter forwards no ceiling).
        max_tokens: None,
        top_p: None,
        top_k: None,
        repeat_penalty: None,
        frequency_penalty: None,
        repeat_last_n: None,
        stop_sequences: None,
        tools: None,
        tool_choice: None,
        request_id: None,
        user_id: None,
        room_id: None,
        active_adapters: None,
        response_format: None,
        purpose: None,
        // Agent-mode call from the IPC bridge — not a persona-owned conversation.
        persona_id: None,
    };

    let response = adapter.generate_text(request).await?;

    Ok(response.text)
}

/// Static regexes for tool call parsing (compiled once)
static TOOL_BLOCK_RE: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"```tool\s*\n?([\s\S]*?)```").unwrap());
static INLINE_TOOL_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r#"\{"name":\s*"(\w+)",\s*"arguments":\s*(\{[^}]+\})\}"#).unwrap()
});

/// Parse tool calls from LLM response
fn parse_tool_calls(response: &str) -> Vec<ToolCall> {
    let mut calls = Vec::new();

    for cap in TOOL_BLOCK_RE.captures_iter(response) {
        if let Some(json_str) = cap.get(1) {
            if let Ok(parsed) = serde_json::from_str::<ToolCall>(json_str.as_str().trim()) {
                calls.push(parsed);
            }
        }
    }

    for cap in INLINE_TOOL_RE.captures_iter(response) {
        if let (Some(name), Some(args)) = (cap.get(1), cap.get(2)) {
            if let Ok(arguments) = serde_json::from_str::<Value>(args.as_str()) {
                let call = ToolCall {
                    name: name.as_str().to_string(),
                    arguments,
                };
                // Avoid duplicates
                if !calls
                    .iter()
                    .any(|c| c.name == call.name && c.arguments == call.arguments)
                {
                    calls.push(call);
                }
            }
        }
    }

    calls
}

/// Execute a tool call (runs on Rayon thread pool)
fn execute_tool(call: &ToolCall, working_dir: &Path) -> ToolResult {
    match call.name.as_str() {
        "read_file" => tool_read_file(call, working_dir),
        "write_file" => tool_write_file(call, working_dir),
        "edit_file" => tool_edit_file(call, working_dir),
        "search_files" => tool_search_files(call, working_dir),
        "list_files" => tool_list_files(call, working_dir),
        "run_command" => tool_run_command(call, working_dir),
        "git_status" => tool_git_status(working_dir),
        "git_diff" => tool_git_diff(call, working_dir),
        "complete" | "give_up" => ToolResult {
            success: true,
            output: "Acknowledged".to_string(),
            error: None,
        },
        _ => ToolResult {
            success: false,
            output: format!("Unknown tool: {}", call.name),
            error: Some(format!("Unknown tool: {}", call.name)),
        },
    }
}

// ============================================================================
// TOOL IMPLEMENTATIONS
// ============================================================================

fn tool_read_file(call: &ToolCall, working_dir: &Path) -> ToolResult {
    let path = match call.arguments.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return ToolResult {
                success: false,
                output: "Missing path argument".to_string(),
                error: Some("Missing path".to_string()),
            };
        }
    };

    let full_path = working_dir.join(path);
    if !full_path.exists() {
        return ToolResult {
            success: false,
            output: format!("File not found: {}", path),
            error: Some("File not found".to_string()),
        };
    }

    match std::fs::read_to_string(&full_path) {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().collect();
            let start = call
                .arguments
                .get("start_line")
                .and_then(|v| v.as_u64())
                .map(|n| (n as usize).saturating_sub(1))
                .unwrap_or(0);
            let end = call
                .arguments
                .get("end_line")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize)
                .unwrap_or(lines.len());

            let selected: String = lines[start..end.min(lines.len())]
                .iter()
                .enumerate()
                .map(|(i, line)| format!("{:4}: {}", start + i + 1, line))
                .collect::<Vec<_>>()
                .join("\n");

            ToolResult {
                success: true,
                output: format!(
                    "File: {} ({} lines, showing {}-{})\n\n{}",
                    path,
                    lines.len(),
                    start + 1,
                    end.min(lines.len()),
                    selected
                ),
                error: None,
            }
        }
        Err(e) => ToolResult {
            success: false,
            output: format!("Error reading file: {}", e),
            error: Some(e.to_string()),
        },
    }
}

fn tool_write_file(call: &ToolCall, working_dir: &Path) -> ToolResult {
    let path = match call.arguments.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return ToolResult {
                success: false,
                output: "Missing path argument".to_string(),
                error: Some("Missing path".to_string()),
            };
        }
    };

    let content = match call.arguments.get("content").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => {
            return ToolResult {
                success: false,
                output: "Missing content argument".to_string(),
                error: Some("Missing content".to_string()),
            };
        }
    };

    let full_path = working_dir.join(path);

    // Create parent directories
    if let Some(parent) = full_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return ToolResult {
                success: false,
                output: format!("Failed to create directories: {}", e),
                error: Some(e.to_string()),
            };
        }
    }

    let existed = full_path.exists();
    match std::fs::write(&full_path, content) {
        Ok(_) => ToolResult {
            success: true,
            output: format!(
                "{} {} ({} bytes, {} lines)",
                if existed { "Updated" } else { "Created" },
                path,
                content.len(),
                content.lines().count()
            ),
            error: None,
        },
        Err(e) => ToolResult {
            success: false,
            output: format!("Error writing file: {}", e),
            error: Some(e.to_string()),
        },
    }
}

fn tool_edit_file(call: &ToolCall, working_dir: &Path) -> ToolResult {
    let path = match call.arguments.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return ToolResult {
                success: false,
                output: "Missing path argument".to_string(),
                error: Some("Missing path".to_string()),
            };
        }
    };

    let search = match call.arguments.get("search").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => {
            return ToolResult {
                success: false,
                output: "Missing search argument".to_string(),
                error: Some("Missing search".to_string()),
            };
        }
    };

    let replace = match call.arguments.get("replace").and_then(|v| v.as_str()) {
        Some(r) => r,
        None => {
            return ToolResult {
                success: false,
                output: "Missing replace argument".to_string(),
                error: Some("Missing replace".to_string()),
            };
        }
    };

    let full_path = working_dir.join(path);
    if !full_path.exists() {
        return ToolResult {
            success: false,
            output: format!("File not found: {}", path),
            error: Some("File not found".to_string()),
        };
    }

    match std::fs::read_to_string(&full_path) {
        Ok(content) => {
            let count = content.matches(search).count();
            if count == 0 {
                return ToolResult {
                    success: false,
                    output: format!(
                        "Search string not found in {}. Make sure to use exact text including whitespace.",
                        path
                    ),
                    error: Some("Search string not found".to_string()),
                };
            }
            if count > 1 {
                return ToolResult {
                    success: false,
                    output: format!(
                        "Search string found {} times in {}. Use a more specific search to match exactly one location.",
                        count, path
                    ),
                    error: Some("Multiple matches".to_string()),
                };
            }

            let new_content = content.replace(search, replace);
            match std::fs::write(&full_path, &new_content) {
                Ok(_) => ToolResult {
                    success: true,
                    output: format!(
                        "Edited {}: replaced {} chars with {} chars",
                        path,
                        search.len(),
                        replace.len()
                    ),
                    error: None,
                },
                Err(e) => ToolResult {
                    success: false,
                    output: format!("Error writing file: {}", e),
                    error: Some(e.to_string()),
                },
            }
        }
        Err(e) => ToolResult {
            success: false,
            output: format!("Error reading file: {}", e),
            error: Some(e.to_string()),
        },
    }
}

fn tool_search_files(call: &ToolCall, working_dir: &Path) -> ToolResult {
    let pattern = match call.arguments.get("pattern").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return ToolResult {
                success: false,
                output: "Missing pattern argument".to_string(),
                error: Some("Missing pattern".to_string()),
            };
        }
    };

    let max_results = call
        .arguments
        .get("max_results")
        .and_then(|v| v.as_u64())
        .unwrap_or(50);

    let file_glob = call
        .arguments
        .get("file_glob")
        .and_then(|v| v.as_str())
        .map(|g| format!("--include=\"{}\"", g))
        .unwrap_or_default();

    let cmd = format!(
        "grep -rn {} -E \"{}\" . 2>/dev/null | head -{}",
        file_glob,
        pattern.replace('"', "\\\""),
        max_results
    );

    match Command::new("sh")
        .arg("-c")
        .arg(&cmd)
        .current_dir(working_dir)
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let lines: Vec<&str> = stdout.lines().collect();

            ToolResult {
                success: true,
                output: format!(
                    "Found {} matches:\n{}",
                    lines.len(),
                    if stdout.is_empty() {
                        "No matches found".to_string()
                    } else {
                        stdout.to_string()
                    }
                ),
                error: None,
            }
        }
        Err(e) => ToolResult {
            success: false,
            output: format!("Search error: {}", e),
            error: Some(e.to_string()),
        },
    }
}

fn tool_list_files(call: &ToolCall, working_dir: &Path) -> ToolResult {
    let dir_path = call
        .arguments
        .get("path")
        .and_then(|v| v.as_str())
        .map(|p| working_dir.join(p))
        .unwrap_or_else(|| working_dir.to_path_buf());

    let depth = call
        .arguments
        .get("depth")
        .and_then(|v| v.as_u64())
        .unwrap_or(3);

    let pattern = call
        .arguments
        .get("pattern")
        .and_then(|v| v.as_str())
        .map(|p| format!("-name \"{}\"", p))
        .unwrap_or_default();

    let cmd = format!(
        "find \"{}\" -maxdepth {} -type f {} 2>/dev/null | head -100",
        dir_path.display(),
        depth,
        pattern
    );

    match Command::new("sh")
        .arg("-c")
        .arg(&cmd)
        .current_dir(working_dir)
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut paths: Vec<String> = stdout
                .lines()
                .filter(|l| !l.is_empty())
                .filter_map(|l| {
                    // Make paths relative to working_dir
                    let p = Path::new(l);
                    p.strip_prefix(working_dir)
                        .map(|r| r.to_string_lossy().to_string())
                        .ok()
                        .or_else(|| Some(l.to_string()))
                })
                .collect();
            paths.sort();

            ToolResult {
                success: true,
                output: format!(
                    "Files in {}:\n{}",
                    call.arguments
                        .get("path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("."),
                    if paths.is_empty() {
                        "No files found".to_string()
                    } else {
                        paths.join("\n")
                    }
                ),
                error: None,
            }
        }
        Err(e) => ToolResult {
            success: false,
            output: format!("List error: {}", e),
            error: Some(e.to_string()),
        },
    }
}

fn tool_run_command(call: &ToolCall, working_dir: &Path) -> ToolResult {
    let command = match call.arguments.get("command").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => {
            return ToolResult {
                success: false,
                output: "Missing command argument".to_string(),
                error: Some("Missing command".to_string()),
            };
        }
    };

    let _timeout_ms = call
        .arguments
        .get("timeout_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(60000);

    // Execute with timeout using std::process for Rayon compatibility
    match Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(working_dir)
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let success = output.status.success();

            if success {
                ToolResult {
                    success: true,
                    output: if stdout.is_empty() {
                        "Command completed (no output)".to_string()
                    } else {
                        stdout.chars().take(5000).collect()
                    },
                    error: None,
                }
            } else {
                ToolResult {
                    success: false,
                    output: format!(
                        "Exit code: {:?}\n\nSTDOUT:\n{}\n\nSTDERR:\n{}",
                        output.status.code(),
                        stdout.chars().take(2500).collect::<String>(),
                        stderr.chars().take(2500).collect::<String>()
                    ),
                    error: Some(format!(
                        "Command failed with exit code {:?}",
                        output.status.code()
                    )),
                }
            }
        }
        Err(e) => ToolResult {
            success: false,
            output: format!("Execution error: {}", e),
            error: Some(e.to_string()),
        },
    }
}

fn tool_git_status(working_dir: &Path) -> ToolResult {
    match Command::new("git")
        .args(["status", "--short"])
        .current_dir(working_dir)
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            ToolResult {
                success: true,
                output: if stdout.is_empty() {
                    "Working tree clean".to_string()
                } else {
                    stdout.to_string()
                },
                error: None,
            }
        }
        Err(e) => ToolResult {
            success: false,
            output: format!("Git error: {}", e),
            error: Some(e.to_string()),
        },
    }
}

fn tool_git_diff(call: &ToolCall, working_dir: &Path) -> ToolResult {
    let mut args = vec!["diff"];

    if call
        .arguments
        .get("staged")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        args.push("--staged");
    }

    let path = call.arguments.get("path").and_then(|v| v.as_str());

    let mut cmd = Command::new("git");
    cmd.args(&args).current_dir(working_dir);

    if let Some(p) = path {
        cmd.arg("--").arg(p);
    }

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            ToolResult {
                success: true,
                output: if stdout.is_empty() {
                    "No changes".to_string()
                } else {
                    stdout.chars().take(5000).collect()
                },
                error: None,
            }
        }
        Err(e) => ToolResult {
            success: false,
            output: format!("Git error: {}", e),
            error: Some(e.to_string()),
        },
    }
}

// ============================================================================
// SERVICE MODULE IMPLEMENTATION
// ============================================================================

#[async_trait]
impl ServiceModule for AgentModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "agent",
            priority: ModulePriority::Normal,
            command_prefixes: &["agent/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        self.service.set_bus(ctx.bus.clone());
        log_info!("module", "agent", "AgentModule initialized with event bus");
        Ok(())
    }

    async fn handle_command(
        &self,
        command: &str,
        _params: Value,
    ) -> Result<CommandResult, String> {
        // MIGRATED: every `agent/*` verb is a typed command object (see
        // `crate::commands::agent`), contributed via `commands()` below and winning
        // at `route_object`. Nothing should reach here. Fail loud — this legacy
        // `handle_command` retires entirely in Wave Z.
        Err(format!(
            "agent command surface is migrated to the typed registry; '{command}' has no legacy handler"
        ))
    }

    /// The migrated `agent/*` commands as typed self-routing objects on the ONE
    /// registry, sharing the module's [`AgentService`]. The executor routes these
    /// names directly here (winning over the dead legacy prefix arm), and their
    /// descriptors flow into `command_registry()` → the persona tool surface + grid
    /// ACL + codegen.
    fn commands(&self) -> Vec<Arc<dyn crate::sdk_codegen::DynCommand>> {
        crate::commands::agent::command_objects(self.service.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
