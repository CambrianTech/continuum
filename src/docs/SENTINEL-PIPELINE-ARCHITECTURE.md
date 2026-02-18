# Sentinel Pipeline Architecture

## Problem Statement

The current SentinelModule only executes shell commands. Pipeline interpretation (multi-step with LLM, conditions, loops) was attempted in TypeScript but:
1. **IPC deadlock**: TypeScript calling `./jtag inference/generate` blocks the server waiting for itself
2. **Fragile**: Each step type requires TypeScript-to-Rust IPC round-trips
3. **Wrong layer**: Rust modules can call each other DIRECTLY via ModuleRegistry

## Solution: Rust Pipeline Interpreter

Move pipeline interpretation INTO the Rust SentinelModule. The sentinel can:
- Execute shell steps (existing capability)
- Call `ai/generate` via `registry.route_command()` (no IPC, direct call)
- Call any command via `registry.route_command()` (DataModule, CodeModule, etc.)
- Evaluate conditions and loops locally

## Pipeline Schema

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PipelineStep {
    // Execute shell command
    Shell {
        cmd: String,
        args: Vec<String>,
        #[serde(default)]
        timeout_secs: Option<u64>,
        #[serde(default)]
        working_dir: Option<String>,
    },

    // LLM inference (calls AIProviderModule directly)
    Llm {
        prompt: String,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        provider: Option<String>,
        #[serde(default)]
        max_tokens: Option<u32>,
        #[serde(default)]
        temperature: Option<f32>,
        #[serde(default)]
        system_prompt: Option<String>,
    },

    // Call any command (routes via ModuleRegistry)
    Command {
        command: String,
        #[serde(default)]
        params: Value,
    },

    // Conditional execution
    Condition {
        #[serde(rename = "if")]
        condition: String,  // e.g., "{{steps.0.success}}"
        then_steps: Vec<PipelineStep>,
        #[serde(default)]
        else_steps: Vec<PipelineStep>,
    },

    // Loop with count
    Loop {
        count: usize,
        steps: Vec<PipelineStep>,
    },

    // Parallel execution (tokio::join!)
    Parallel {
        steps: Vec<PipelineStep>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pipeline {
    pub name: Option<String>,
    pub steps: Vec<PipelineStep>,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}
```

## Execution Flow

```
sentinel/pipeline command received
    ↓
Parse Pipeline JSON
    ↓
Create ExecutionContext { variables, step_results }
    ↓
For each step:
    ├── Shell → execute_isolated_static() (existing)
    ├── Llm → registry.route_command("ai/generate")
    ├── Command → registry.route_command(cmd)
    ├── Condition → evaluate, then/else recursion
    ├── Loop → iterate, recursive step execution
    └── Parallel → tokio::join! on step futures
    ↓
Return PipelineResult { traces, final_result, success }
```

## Module-to-Module Calls

The SentinelModule stores `ModuleContext` during initialize:

```rust
pub struct SentinelModule {
    sentinels: Arc<DashMap<String, RunningSentinel>>,
    workspaces_dir: RwLock<PathBuf>,
    max_concurrent: usize,
    bus: RwLock<Option<Arc<MessageBus>>>,
    ctx: RwLock<Option<Arc<ModuleContext>>>,  // NEW
}
```

For LLM steps:
```rust
async fn execute_llm_step(&self, step: &LlmStep, ctx: &mut ExecutionContext) -> Result<Value, String> {
    let module_ctx = self.ctx.read().as_ref().ok_or("Not initialized")?;

    let (module, cmd) = module_ctx.registry.route_command("ai/generate")
        .ok_or("ai module not found")?;

    let params = json!({
        "prompt": self.interpolate(&step.prompt, ctx),
        "model": step.model,
        "provider": step.provider,
        "max_tokens": step.max_tokens,
        "temperature": step.temperature,
        "system_prompt": step.system_prompt,
    });

    let result = module.handle_command(&cmd, params).await?;
    // ... extract result
}
```

## Variable Interpolation

Steps can reference previous results:
- `{{steps.0.text}}` - output from step 0
- `{{steps.build.exit_code}}` - named step result
- `{{env.HOME}}` - environment variable
- `{{input.message}}` - pipeline input parameter

## Olympics Validation Cases

The pipeline system must support these use cases:

1. **Category 4.1: Build Pipeline (Pure Script)**
   ```json
   { "steps": [
     { "type": "shell", "cmd": "npm", "args": ["run", "build"] },
     { "type": "shell", "cmd": "npm", "args": ["run", "test"] }
   ]}
   ```

2. **Category 3.1: Commit Message Generator (Local Models)**
   ```json
   { "steps": [
     { "type": "shell", "cmd": "git", "args": ["diff", "--staged"] },
     { "type": "llm", "prompt": "Generate commit message for:\n{{steps.0.output}}",
       "provider": "local" }
   ]}
   ```

3. **Category 2.1: PR Review (Medium Models)**
   ```json
   { "steps": [
     { "type": "command", "command": "code/read", "params": {"path": "{{input.file}}"} },
     { "type": "llm", "prompt": "Review this code:\n{{steps.0.content}}",
       "model": "claude-3-5-haiku" }
   ]}
   ```

## Implementation Order

1. Add `PipelineStep` and `Pipeline` structs to sentinel.rs
2. Store `ModuleContext` in SentinelModule during initialize()
3. Add `execute_pipeline()` method with step dispatch
4. Add `sentinel/pipeline` command handler
5. Implement variable interpolation
6. Test each Olympics category

## TypeScript Wrapper (Minimal)

TypeScript only provides the CLI wrapper:
```typescript
// commands/sentinel/run/server/SentinelRunServerCommand.ts
// Just calls: Commands.execute('sentinel/pipeline', pipelineJson)
```

No pipeline interpretation in TypeScript. Rust does all the work.
