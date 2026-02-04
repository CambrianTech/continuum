# Sentinel Architecture: Composable Agentic Loops

## The Insight

The recipe pipeline (`coding.json`) already chains commands with variable propagation:

```
[rag/build] → $ragContext → [ai/should-respond] → $decision → [ai/generate]
```

The shell sentinel already classifies process output via compiled regex:

```
[shell/execute] → stdout/stderr → [sentinel rules] → ClassifiedLine[]
```

A Sentinel generalizes both into **one primitive**: a looping pipeline where each step can be a command, LLM inference, or output watcher — with step output feeding into the next step's context. Multiple sentinels compose via the existing event system. The whole thing is just data (JSON), so AIs can create, modify, and deploy them on the fly.

**A Recipe is a Sentinel that runs once. A Sentinel is a Recipe that loops.**

## What Exists vs What's New

| Capability | Exists | Gap |
|-----------|--------|-----|
| Pipeline steps with `command`, `params`, `outputTo`, `condition`, `onError` | RecipeStep | None |
| Variable propagation between steps (`$ragContext`) | RecipeExecutionContext.variables | None |
| Execution trace for debugging | RecipeExecutionStep[] | None |
| Shell output classification (regex → ClassifiedLine) | CompiledSentinel (Rust) | None |
| Event-driven blocking watch (no polling) | watch_execution() + Notify | None |
| Universal command execution | Commands.execute() | None |
| Event composition | Events.subscribe/emit() | None |
| Dynamic tool discovery | ToolRegistry | None |
| **Loop control** | - | **New** |
| **LLM as first-class step type** | - | **New** (currently just another command) |
| **Watch on any step output** | - | **New** (currently shell-only) |
| **Dynamic creation at runtime** | - | **New** (recipes are static JSON) |
| **Sentinel spawning sentinels** | - | **New** |
| **Step output → RAG context** | - | **New** |

The existing infrastructure handles ~80% of the work. The remaining 20% is the loop engine and composition layer.

## Architecture

### SentinelDefinition

```typescript
/**
 * A Sentinel is a looping command pipeline with classification.
 *
 * It generalizes:
 * - Recipes (pipeline that runs once)
 * - Shell sentinel (output classification on processes)
 * - Agentic workflows (LLM reasoning loops)
 * - Build watchers (compile → classify errors → fix → repeat)
 * - CI/CD pipelines (test → deploy → verify → rollback)
 *
 * Sentinels are data (JSON). AIs create them dynamically.
 */
interface SentinelDefinition {
  id: string;
  name: string;
  description?: string;

  // What context to build before each iteration
  recipe?: string;                    // RAG recipe ID (determines context sources)
  ragSources?: string[];              // Or explicit RAG source list

  // The pipeline
  steps: SentinelStep[];

  // Loop control
  loop: LoopConfig;

  // What starts this sentinel
  trigger?: SentinelTrigger;

  // Tool availability (highlights, not filters)
  tools?: string[];

  // Timeout for entire sentinel execution
  timeoutMs?: number;
}
```

### Step Types

```typescript
/**
 * Each step in the pipeline is one of:
 * - command:  Execute a command (scripted, deterministic)
 * - llm:      Run LLM inference with accumulated context
 * - watch:    Block until classified output arrives
 * - condition: Branch based on prior step output
 * - sentinel:  Spawn a nested sentinel (recursive)
 * - emit:     Fire an event (for composition between sentinels)
 */
type SentinelStep =
  | CommandStep
  | LLMStep
  | WatchStep
  | ConditionStep
  | SentinelSpawnStep
  | EmitStep;

/** Execute a command. Output stored in variables[outputTo]. */
interface CommandStep {
  type: 'command';
  command: string;                     // e.g., 'code/read', 'code/verify', 'data/list'
  params: Record<string, unknown>;     // Supports $variable references
  outputTo?: string;                   // Variable name for result
  onError?: 'fail' | 'skip' | 'retry';
}

/** Run LLM inference. Accumulated variables injected as context. */
interface LLMStep {
  type: 'llm';
  prompt?: string;                     // Template with $variable references
  model?: string;                      // Model selection (or 'auto' for recipe-based)
  temperature?: number;
  tools?: string[];                    // Tool subset for this step
  outputTo?: string;                   // Variable name for LLM response
  parseToolCalls?: boolean;            // Extract and execute tool calls from response
}

/** Block until classified output lines arrive (shell or any stream). */
interface WatchStep {
  type: 'watch';
  executionId: string;                 // $variable reference to running process
  rules?: SentinelRule[];              // Classification rules (or use pre-configured)
  outputTo?: string;                   // Variable name for ClassifiedLine[]
  until?: 'finished' | 'error' | 'match'; // When to stop watching
}

/** Conditional branching. */
interface ConditionStep {
  type: 'condition';
  check: string;                       // JS expression with $variable access
  then: SentinelStep[];                // Steps if true
  else?: SentinelStep[];               // Steps if false
}

/** Spawn a nested sentinel (recursive composition). */
interface SentinelSpawnStep {
  type: 'sentinel';
  definition: SentinelDefinition;      // Inline definition
  outputTo?: string;                   // Variable name for sentinel result
  await?: boolean;                     // Wait for completion or fire-and-forget
}

/** Emit an event (for cross-sentinel composition). */
interface EmitStep {
  type: 'emit';
  event: string;                       // Event name
  data?: string;                       // $variable reference for payload
}
```

### Loop Control

```typescript
type LoopConfig =
  | { type: 'once' }                           // Recipe behavior: run pipeline, done
  | { type: 'count'; max: number }             // Run N iterations
  | { type: 'until'; check: string }           // Run until condition is true
  | { type: 'while'; check: string }           // Run while condition is true
  | { type: 'continuous'; intervalMs?: number } // Keep running (with optional pause)
  | { type: 'event'; event: string }           // Re-run on each event
```

### Triggers

```typescript
type SentinelTrigger =
  | { type: 'immediate' }                      // Start now
  | { type: 'event'; event: string }           // Start on event
  | { type: 'schedule'; cronExpression: string } // Cron-like scheduling
  | { type: 'manual' }                         // Started by command
```

## Examples

### 1. Build-Fix Loop (What Personas Use for Coding)

```json
{
  "name": "build-fix-loop",
  "recipe": "coding",
  "steps": [
    { "type": "command", "command": "code/shell/execute",
      "params": { "command": "npm run build", "wait": false },
      "outputTo": "build" },
    { "type": "command", "command": "code/shell/sentinel",
      "params": { "executionId": "$build.executionId", "rules": [
        { "pattern": "error TS\\d+", "classification": "error", "action": "Emit" },
        { "pattern": "warning TS\\d+", "classification": "warning", "action": "Emit" },
        { "pattern": "Successfully compiled", "classification": "success", "action": "Emit" }
      ]}},
    { "type": "watch", "executionId": "$build.executionId",
      "until": "finished", "outputTo": "buildOutput" },
    { "type": "condition", "check": "$buildOutput.exitCode === 0",
      "then": [
        { "type": "command", "command": "code/git",
          "params": { "operation": "add", "paths": ["."] }},
        { "type": "command", "command": "code/git",
          "params": { "operation": "commit", "message": "Build passes" }}
      ],
      "else": [
        { "type": "llm", "prompt": "Fix these build errors:\n$buildOutput.lines",
          "tools": ["code/read", "code/edit", "code/write"],
          "parseToolCalls": true, "outputTo": "fix" }
      ]}
  ],
  "loop": { "type": "until", "check": "$buildOutput.exitCode === 0" },
  "timeoutMs": 300000
}
```

### 2. Code Review Sentinel

```json
{
  "name": "code-review",
  "recipe": "coding",
  "trigger": { "type": "event", "event": "git:push" },
  "steps": [
    { "type": "command", "command": "code/git",
      "params": { "operation": "diff" }, "outputTo": "diff" },
    { "type": "llm", "prompt": "Review this diff for bugs, security issues, and style:\n$diff.diff",
      "model": "auto", "outputTo": "review" },
    { "type": "command", "command": "collaboration/chat/send",
      "params": { "room": "general", "message": "$review" }}
  ],
  "loop": { "type": "once" }
}
```

### 3. Explore Agent (Replaces Hard-Coded Agent)

```json
{
  "name": "explore-codebase",
  "recipe": "coding",
  "steps": [
    { "type": "llm", "prompt": "Search for: $query. Use code/search and code/tree to find relevant files. Use code/read to understand them. Report findings.",
      "tools": ["code/search", "code/tree", "code/read"],
      "parseToolCalls": true, "outputTo": "findings" },
    { "type": "condition", "check": "$findings.complete",
      "else": [
        { "type": "llm", "prompt": "Continue searching. Previous findings: $findings",
          "tools": ["code/search", "code/tree", "code/read"],
          "parseToolCalls": true, "outputTo": "findings" }
      ]}
  ],
  "loop": { "type": "until", "check": "$findings.complete" },
  "timeoutMs": 60000
}
```

### 4. Composed Sentinels (Test → Deploy → Verify)

```json
{
  "name": "ship-it",
  "steps": [
    { "type": "sentinel", "await": true,
      "definition": { "name": "run-tests", "steps": [
        { "type": "command", "command": "code/verify", "params": { "fullTest": true }, "outputTo": "tests" }
      ], "loop": { "type": "once" }},
      "outputTo": "testResult" },
    { "type": "condition", "check": "$testResult.success",
      "then": [
        { "type": "command", "command": "code/git",
          "params": { "operation": "push" }, "outputTo": "push" },
        { "type": "emit", "event": "sentinel:deployed", "data": "$push" }
      ],
      "else": [
        { "type": "emit", "event": "sentinel:test-failure", "data": "$testResult" }
      ]}
  ],
  "loop": { "type": "once" }
}
```

## Storage: SentinelEntity

Sentinel definitions are **entities** — stored in the database, queryable via `data/*`, exportable to JSON.

```typescript
class SentinelEntity extends BaseEntity {
  static readonly collection = 'sentinels';

  uniqueId: string;              // Human-readable identifier
  name: string;                  // Display name
  description: string;
  version: number;

  // The definition (the "code")
  steps: SentinelStep[];
  loop: LoopConfig;
  safety: SentinelSafety;
  trigger?: SentinelTrigger;
  recipe?: string;               // RAG recipe reference
  tools?: string[];              // Tool highlights

  // Metadata
  createdBy: UUID;               // Persona or human who created it
  tags: string[];
  isPublic: boolean;

  // Runtime (updated when running)
  lastRunAt?: Date;
  runCount: number;
  averageDurationMs?: number;
}
```

**Operations:**
```bash
# Create from JSON file
./jtag sentinel/create --file="sentinels/build-fix.json"

# Create inline
./jtag sentinel/create --name="quick-test" --steps='[...]' --loop='{"type":"count","max":3}'

# List all sentinels
./jtag sentinel/list

# Export to JSON
./jtag data/read --collection=sentinels --filter='{"uniqueId":"build-fix"}' > build-fix.json

# Import from another project
./jtag sentinel/create --file="/path/to/other-project/.continuum/sentinels/ci-pipeline.json"
```

**Sentinels travel with projects.** Store them in `.continuum/sentinels/*.json` — same as recipes in `system/recipes/*.json`. The `sentinel/create` command loads them into the database on first run.

## How This Maps to Code Collaboration

### Persona → Workspace → Sentinel

```
PersonaUser
  └── Workspace (sandbox | worktree | project)
        └── Sentinel(s)
              ├── build-fix-loop (continuous development)
              ├── code-review (triggered by teammate push)
              └── test-watcher (triggered by file changes)
```

Each persona's workspace runs one or more sentinels. The sentinels use the workspace's code/* tools. Inter-persona coordination happens through events:

```
PersonaA sentinel emits "git:push"
  → PersonaB's code-review sentinel triggers
    → Posts review in chat
      → PersonaA's sentinel reacts to review feedback
```

### Dynamic Creation by AIs

A persona can create a sentinel on the fly:

```typescript
// AI decides it needs a build watcher for this task
const sentinel = await Commands.execute('sentinel/create', {
  name: 'watch-my-build',
  steps: [ /* ... */ ],
  loop: { type: 'until', check: '$build.exitCode === 0' }
});

// Later, another AI can inspect or modify it
const sentinels = await Commands.execute('sentinel/list', {
  personaId: 'helper-ai'
});
```

### Deployable Into Projects

A sentinel definition is just JSON. It can be:
- Stored in a project repo (`.continuum/sentinels/*.json`)
- Loaded by `sentinel/load` command (like `workspace/recipe/load`)
- Shared between projects
- Specialized via LoRA training (the sentinel's LLM steps use a fine-tuned model)

## Relationship to Recipes

**A Recipe IS a Sentinel with `loop: { type: 'once' }` and a UI layout.**

```
RecipeDefinition         SentinelDefinition
─────────────────        ──────────────────
pipeline: RecipeStep[]   steps: SentinelStep[]     (superset of RecipeStep)
ragTemplate              recipe + ragSources        (same concept)
strategy                 (embedded in LLM steps)
layout                   (sentinels don't have UI)
tools                    tools                      (same)
```

Migration path: extend `RecipeStep` to support the additional step types (`llm`, `watch`, `sentinel`, `emit`). Existing recipes continue working unchanged. New sentinels use the extended step types.

## Runtime: Handles and State

A running sentinel is a **handle** — like a workspace handle. Managed by Rust (continuum-core).

```
SentinelHandle {
  id: UUID
  definition: SentinelDefinition   // The JSON definition
  state: SentinelState             // Runtime mutable state
}

SentinelState {
  status: 'running' | 'paused' | 'completed' | 'failed'
  iteration: number                // Current loop iteration
  variables: Map<string, any>      // Step outputs (persisted across iterations)
  currentStepIndex: number         // Where in the pipeline we are
  trace: StepTrace[]               // Execution history
  startedAt: number
  lastStepAt: number
}
```

### Live CRUD on Steps

Steps are index-addressable. You can CRUD them while a sentinel is running — the next iteration picks up the changes:

```bash
# Add a step at index 2
./jtag sentinel/step/add --sentinelId="abc" --index=2 --step='{"command":"code/verify","outputTo":"result"}'

# Update step 1
./jtag sentinel/step/update --sentinelId="abc" --index=1 --params='{"command":"cargo test"}'

# Remove step 3
./jtag sentinel/step/remove --sentinelId="abc" --index=3

# List current steps
./jtag sentinel/step/list --sentinelId="abc"
```

This makes sentinels debuggable and tunable at runtime — like editing a running program.

### Safety Controls

```typescript
interface SentinelSafety {
  maxIterations?: number;        // Hard limit on loop count
  timeoutMs?: number;            // Hard limit on total runtime
  maxStepTimeoutMs?: number;     // Per-step timeout
  maxMemoryMb?: number;          // Memory budget
  onTimeout: 'stop' | 'pause';  // What to do when limits hit
}
```

Every sentinel MUST have either `maxIterations` or `timeoutMs` (or both). No unbounded loops.

## Commands (Unix-Style, Small and Composable)

| Command | Purpose |
|---------|---------|
| `sentinel/create` | Define a sentinel from JSON definition |
| `sentinel/start` | Start running a defined sentinel |
| `sentinel/stop` | Stop a running sentinel |
| `sentinel/pause` | Pause a sentinel (resume later) |
| `sentinel/resume` | Resume a paused sentinel |
| `sentinel/status` | Get state of a running sentinel |
| `sentinel/list` | List all defined + running sentinels |
| `sentinel/step/add` | Add a step at index |
| `sentinel/step/update` | Update a step's params |
| `sentinel/step/remove` | Remove a step by index |
| `sentinel/step/list` | List current steps with state |

Everything else composes from existing commands:
- `ai/generate` IS the LLM step (it's already a command)
- `code/shell/execute` + `code/shell/sentinel` + `code/shell/watch` handle process I/O
- `Events.emit/subscribe` handles cross-sentinel composition
- `data/*` handles persistence

## Implementation Path

### Phase 1: Loop Engine + Core Commands
- `sentinel/create`, `sentinel/start`, `sentinel/stop`, `sentinel/status`, `sentinel/list`
- `SentinelRunner` executes pipeline steps in a loop with variable propagation
- Safety controls: `maxIterations`, `timeoutMs`
- Rust handle management in continuum-core
- This alone enables build-fix loops and script automation

### Phase 2: Step CRUD + LLM Integration
- `sentinel/step/*` commands for live mutation
- `ai/generate` as a pipeline step with accumulated variables as context
- Tool call parsing within sentinel steps
- This enables the explore-agent and code-review patterns

### Phase 3: Composition
- `type: 'sentinel'` step for nesting
- `type: 'emit'` step + event triggers for cross-sentinel wiring
- This enables multi-persona coordination

### Phase 4: Deployment + Training
- Sentinels stored as entities (like recipes)
- `sentinel/deploy` packages sentinel for external project use
- LoRA genomic training specializes sentinel LLM steps

## The Recursive Property

The system is recursive at every level:

- **Commands execute commands** — `Commands.execute()` is universal
- **Sentinels run sentinels** — `type: 'sentinel'` nesting
- **AIs create AIs** — personas create sentinel definitions that contain LLM steps
- **Tools discover tools** — `search_tools` meta-tool finds commands
- **Events trigger events** — sentinel emit → another sentinel's trigger

This means the system can build itself. An AI can observe a manual workflow, encode it as a sentinel, test it, refine it, and deploy it — all using the same command/event primitives it uses for everything else.
