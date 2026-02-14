# Sentinel Architecture: Composable Agentic Loops

## The Cognitive Model

**Sentinels are the subconscious threads of persona cognition.**

Like the human mind, a persona operates at multiple levels:

| Human Cognition | Persona Cognition |
|-----------------|-------------------|
| Conscious thought - decisions, creativity, novel problems | PersonaUser cognition - reasoning, conversation, judgment |
| Subconscious threads - walking, driving, muscle memory | Sentinels - builds, deploys, routine code fixes |
| The "zone" - flow state, automatic execution | Running sentinels - autonomous but supervised |
| Escalation - "wait, something's wrong" | Sentinel → escalate to persona on unexpected |

When you walk up stairs, you don't consciously plan each step. When you drive a familiar route, most of it happens without active thought. When you're in the zone playing a sport, execution flows without deliberation. These are **formulaic patterns** your subconscious handles so your conscious mind is free for higher-order thinking.

Sentinels are the same for personas:
- **The persona decides WHAT** - "I need to fix this build"
- **The sentinel handles HOW** - compile, parse errors, apply fixes, retry
- **Escalation when stuck** - "This error is unfamiliar, need conscious attention"

This means sentinels are NOT separate agents competing for attention. They're **tendrils of the persona** - limbs that extend its reach without fragmenting its cognition. The persona remains the unified consciousness; sentinels are its muscle memory.

### Implications

1. **Ownership**: Every sentinel belongs to a persona. No orphan sentinels.
2. **Reporting**: Sentinels report results back to their parent cognition.
3. **Escalation**: When patterns fail, consciousness takes over.
4. **Context**: Sentinels inherit the persona's context, not their own.
5. **Trust**: The persona trusts its sentinels like you trust your hands.

### The Persona-Sentinel Contract

```typescript
interface SentinelSpawn {
  parentPersonaId: UUID;        // Who owns this tendril
  definition: SentinelDefinition;
  inheritContext: boolean;      // Share parent's memory/RAG
  reportTo: 'inbox' | 'silent'; // How to notify parent
  escalateOn: EscalationRule[]; // When to wake up consciousness
}

interface EscalationRule {
  condition: 'error' | 'timeout' | 'unfamiliar' | 'approval_needed';
  action: 'pause' | 'notify' | 'abort';
  priority: 'low' | 'normal' | 'high' | 'urgent';
}
```

When a sentinel encounters something outside its pattern:
1. It pauses execution (doesn't thrash)
2. Creates an inbox item for the persona with context
3. The persona's next cognitive cycle picks it up
4. Persona either resolves it or modifies the sentinel

This is exactly how subconscious → conscious escalation works in humans. You're driving on autopilot until something unexpected happens, then conscious attention snaps in.

### The Spectrum: LLM to Script

Not every sentinel needs an LLM. Sentinels exist on a spectrum:

```
Pure Script              Hybrid                    Full LLM
    │                      │                          │
    ▼                      ▼                          ▼
npm run build     →   build + classify    →   build + analyze + fix
git push          →   push + notify       →   push + review + respond
file watch        →   watch + filter      →   watch + understand + act
```

The persona chooses the right level for each task:
- **Script**: Deterministic, fast, no LLM cost. "Just run these commands."
- **Hybrid**: Script execution with LLM classification or decision. "Run this, then decide."
- **Full LLM**: Autonomous reasoning loop. "Figure out how to accomplish X."

A perfected sentinel often *descends* the spectrum — what started as full LLM reasoning becomes a hybrid, then a pure script, as patterns crystallize into reliable rules.

### Parallel Without Blocking

Sentinels run **in parallel out of thought** — they don't consume the persona's attention unless escalating. This is like how you can:
- Walk and think simultaneously (walking sentinel running)
- Drive and have a conversation (driving sentinel running)
- Type while planning what to say (typing sentinel running)

The persona's conscious cognition handles the "true smarts" — larger decisions in response to sentinel concerns. The sentinels handle the formulaic, freeing consciousness for what matters.

```
PersonaUser (conscious cognition)
    │
    ├── Sentinel: build-fix (subconscious: "keep the code compiling")
    │       └── [compile → error → fix → retry] (muscle memory loop)
    │
    ├── Sentinel: test-watch (subconscious: "alert me if tests fail")
    │       └── [watch → classify → notify] (background awareness)
    │
    └── Sentinel: deploy-pipeline (subconscious: "ship when ready")
            └── [test → build → push → verify] (routine choreography)
```

The persona's inbox receives escalations and completions. It doesn't micromanage the steps - that would defeat the purpose. Like trusting your legs to walk while you think about where you're going.

### Recursive Branching: Sentinels Spawn Sentinels

Sentinels can create other sentinels. This mirrors how subconscious patterns branch:

```
PersonaUser decides: "I need to ship this feature"
    │
    └── deploy-sentinel spawns:
            ├── test-sentinel (run all tests)
            │       └── spawns: coverage-sentinel (for each test file)
            ├── build-sentinel (compile everything)
            │       └── spawns: typecheck-sentinel (parallel)
            └── push-sentinel (when tests pass)
                    └── spawns: verify-sentinel (check CI)
```

This is like how changing lanes while driving triggers a cascade of subconscious actions — check mirrors, signal, scan blind spot, adjust steering — each its own pattern, all coordinated without conscious attention.

The branching rules:
- **Parent owns children**: If parent stops, children stop
- **Children report to parent**: Results bubble up, not to persona directly
- **Escalation propagates**: Child escalation can escalate parent
- **Depth limits**: `maxNestingDepth` prevents runaway recursion

```typescript
{
  "type": "sentinel",
  "definition": {
    "name": "nested-task",
    "steps": [...],
    "loop": { "type": "once" }
  },
  "await": true,           // Parent waits for child
  "outputTo": "childResult" // Child's result becomes variable
}
```

The persona only sees the top-level sentinel. The branching happens automatically, like the coordination between muscle groups during complex movement.

---

## Sentinel Lifecycle: Skills That Evolve

Sentinels aren't just runtime processes — they're **skills that get refined over time**. Like how a human perfects a golf swing or a coding pattern through practice.

### The Lifecycle

```
Create → Use → Observe → Persist → Recall → Refine → Share
   │       │       │        │         │        │        │
   ▼       ▼       ▼        ▼         ▼        ▼        ▼
Builder  Run   Results  longterm.db  Memory   Edit    Export
                + logs              Recall   Steps   to other
                                            + Loop   personas
```

1. **Create**: Persona creates sentinel via builder or dynamically during cognition
2. **Use**: Sentinel runs (possibly multiple times, possibly continuously)
3. **Observe**: Persona sees results, logs, escalations — learns what works
4. **Persist**: Successful sentinels are exported to `longterm.db` as memories
5. **Recall**: When facing similar task, persona recalls the sentinel pattern
6. **Refine**: Edit steps, adjust loop conditions, tune parameters
7. **Share**: Export to other personas who face the same challenges

### Persistence to longterm.db

Sentinels are memories. When a sentinel proves useful, it becomes part of the persona's long-term memory:

```typescript
// After successful sentinel execution:
await Commands.execute('memory/store', {
  personaId: this.persona.id,
  type: 'sentinel',
  content: {
    definition: sentinel.definition,
    executions: sentinel.runCount,
    avgDuration: sentinel.averageDurationMs,
    successRate: sentinel.successRate,
    lastRefinedAt: now(),
  },
  tags: ['skill', 'automation', sentinel.definition.name],
});
```

The persona can then recall this sentinel:

```typescript
// When facing a familiar task:
const memories = await Commands.execute('memory/recall', {
  personaId: this.persona.id,
  query: 'how to fix typescript build errors',
  type: 'sentinel',
});

// Returns the build-fix sentinel the persona refined over time
const sentinel = memories[0].content.definition;
await Commands.execute('sentinel/run', { definition: sentinel });
```

### Inter-Persona Sharing

Sentinels are transferable skills. When one persona perfects a sentinel, others can learn it:

```bash
# Persona A exports their perfected sentinel
./jtag sentinel/export --sentinelId="build-fix" --output="./sentinels/build-fix.json"

# Persona B imports it
./jtag sentinel/import --file="./sentinels/build-fix.json" --personaId="helper-ai"

# Or via memory sharing (when personas trust each other):
./jtag memory/share --from="teacher-ai" --to="student-ai" --filter='{"type":"sentinel"}'
```

This is like a senior developer teaching a junior — sharing not just knowledge but *skills* (the actual sentinels) that can be used immediately.

### Refinement Over Time

Sentinels get better through use. Each execution teaches:

```typescript
interface SentinelRefinement {
  // Track what works
  successfulPaths: StepPath[];   // Which condition branches succeed
  failureModes: FailureMode[];   // What causes failures
  timingStats: TimingStats;      // How long each step takes

  // Suggestions for improvement
  suggestions: RefinementSuggestion[];
  // e.g., "Step 3 always fails when X — add condition check"
  // e.g., "LLM step could be replaced with regex — faster, cheaper"
  // e.g., "Steps 2-4 always run together — combine into one"
}
```

The persona reviews these suggestions during idle time (part of the autonomous loop) and applies refinements. Over time, sentinels evolve from rough drafts to polished skills.

### The Perfection Gradient

A sentinel's lifetime often follows this pattern:

```
Day 1:   Full LLM reasoning, many retries, slow
         └── "Figure out how to fix build errors"

Week 1:  Hybrid with known patterns, fewer retries
         └── "If error matches X, do Y, else ask LLM"

Month 1: Mostly scripts, LLM only for edge cases
         └── "Run these 5 steps, only ask LLM if stuck"

Year 1:  Pure script, battle-tested, fast
         └── "These exact steps always work"
```

This mirrors how humans learn skills — from conscious effort to unconscious competence.

---

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

---

## Comparison with OpenCode

[OpenCode](https://github.com/anomalyco/opencode) is an open-source AI coding agent with 100k+ GitHub stars. Our sentinel architecture shares concepts but generalizes beyond coding.

| Capability | OpenCode | Our Sentinels |
|------------|----------|---------------|
| **Primary Focus** | Coding tasks | Any orchestrated task |
| **Architecture** | Client/Server (Go + TUI) | Distributed (Rust + TS + Browser) |
| **File Operations** | glob, grep, view, edit, patch | code/read, code/edit, code/search, code/tree |
| **Shell Execution** | bash tool | Rust SentinelModule (kill_on_drop isolation) |
| **LLM Step** | Implicit in conversation | Explicit `type: 'llm'` step in pipeline |
| **Nested Agents** | `agent` tool | `type: 'sentinel'` step (recursive) |
| **Permission Gating** | Per-tool approval dialog | Planned (SentinelSafety) |
| **LSP Integration** | Built-in diagnostics | Planned |
| **Git Integration** | Built-in | SentinelWorkspace (branch/worktree isolation) |
| **Session Persistence** | SQLite | SentinelEntity in data layer |
| **Loop Control** | Implicit agent loop | Explicit `LoopConfig` (until, while, count, etc.) |
| **Event Composition** | N/A | `type: 'emit'` + event triggers |
| **Auto-compaction** | At 95% context | Planned |
| **Remote Operation** | Client/server split | Commands over WebSocket |
| **Dynamic Creation** | N/A | AIs create sentinels as JSON entities |
| **Live Mutation** | N/A | `sentinel/step/*` CRUD while running |

### What OpenCode Does Well (Consider Adopting)

1. **Agent Specialization**: OpenCode has `build` (full access), `plan` (read-only), `general` (subagent) modes. We could add a `mode` field to SentinelDefinition:
   ```typescript
   mode?: 'full' | 'readonly' | 'sandboxed';
   ```

2. **Permission Gating**: Before sensitive operations (file write, shell exec), OpenCode shows an approval dialog. We should add:
   ```typescript
   safety: {
     requireApproval?: ('write' | 'shell' | 'delete')[];
   }
   ```

3. **LSP Integration**: OpenCode surfaces diagnostics from language servers. We should add:
   ```typescript
   { "type": "command", "command": "code/diagnostics", "outputTo": "errors" }
   ```

4. **Auto-compaction**: At 95% context utilization, OpenCode auto-summarizes. We should add to LLMStep:
   ```typescript
   autoCompact?: { threshold: number; strategy: 'summarize' | 'truncate' };
   ```

### What We Do Better

1. **Generic Task Types**: OpenCode is coding-focused. Our sentinels handle builds, deploys, monitoring, content generation, data pipelines — any domain.

2. **Explicit Loop Control**: OpenCode's agent loop is implicit. Ours is explicit and configurable (`until`, `while`, `count`, `continuous`, `event`).

3. **Event-Driven Composition**: Our `emit` step + event triggers enable multi-sentinel coordination without tight coupling.

4. **Live Step Mutation**: `sentinel/step/*` commands let you debug/tune a running sentinel like editing a live program.

5. **Recursive Nesting**: `type: 'sentinel'` step enables arbitrary composition depth.

6. **JSON-First Definition**: Sentinels are pure data. AIs can create, modify, share them without code changes.

### Convergence Path

The ideal system combines OpenCode's developer ergonomics with our generic orchestration:

```
Phase 1 (Current): Build-fix loops, task sequences
Phase 2: Add permission gating, LSP diagnostics
Phase 3: Add auto-compaction, context management
Phase 4: Add agent specialization modes
Phase 5: Add mobile-friendly remote operation
```

---

## Adaptive Compute: The LoopLM Paradigm

Recent research on Looped Language Models (LoopLM) — specifically the Ouro model family — reveals a paradigm shift that aligns perfectly with our sentinel and persona architecture.

### The Core Insight: Knowledge Manipulation > Knowledge Storage

The Ouro paper's key finding: **looping doesn't increase what a model knows (~2 bits/parameter regardless of loops), but dramatically improves how it uses knowledge**.

| Capability | Standard LLM | LoopLM |
|------------|--------------|--------|
| Knowledge storage | ~2 bits/param | ~2 bits/param (same) |
| Multi-hop reasoning | Limited by depth | Improves with loops |
| Fact composition | Single-pass | Iterative refinement |
| Knowledge manipulation | Fixed compute | Adaptive compute |

This validates our sentinel design philosophy: **sentinels don't need to know everything — they need to manipulate what they know efficiently.**

### Adaptive Exit: Compute Allocation by Difficulty

LoopLM learns *when to stop thinking*. Simple inputs exit early (fewer loops), complex ones iterate longer. This maps directly to our spectrum:

```
Loop Depth          Our Concept              Behavior
─────────────────────────────────────────────────────────
T=1                 Pure Script              Deterministic, fast
T=2                 Simple Hybrid            Light classification
T=3                 Full Hybrid              Reasoning + decision
T=4+                Full LLM                 Complex reasoning
T=6+ (extrapolate)  Deep Reasoning           Novel problems
```

The Q-exit threshold (0.0 to 1.0) becomes a deployment knob:
- `q=0.2`: Favor early exit (fast, cheap, most tasks)
- `q=0.5`: Balanced (default)
- `q=0.9`: Favor deep reasoning (complex tasks, safety-critical)

### Latent Reasoning vs Chain-of-Thought

This distinction is crucial for autonomous sentinels:

| Aspect | Chain-of-Thought (CoT) | Latent Reasoning (LoopLM) |
|--------|------------------------|---------------------------|
| Thinking medium | Explicit tokens | Hidden states |
| Context cost | Consumes context window | Zero context overhead |
| Faithfulness | Often post-hoc rationalization | Causally coupled to output |
| Verifiability | Can read the reasoning | Must probe hidden states |
| Speed | Slower (generate tokens) | Faster (matrix ops only) |

**Faithfulness is critical**: The Ouro paper shows that only 36% of step-2 answers match step-4 answers on ambiguous tasks. This means the model is *genuinely reasoning*, not rationalizing a pre-committed answer. Standard CoT models often show >95% early commitment — they decide first, justify later.

For sentinels, latent reasoning means:
- **Trustworthy autonomy**: The thinking actually influences outcomes
- **No context bloat**: Reasoning doesn't consume the context window
- **Built-in verification**: Early loop outputs can draft, later loops verify

### Safety Improves With Depth

A surprising finding: **safety alignment improves as loop depth increases**, even when extrapolating beyond training depth (e.g., T=6-8 when trained on T=4).

This has direct implications for sentinel safety:

```typescript
interface AdaptiveSafety {
  // Minimum loops for different operation types
  minDepth: {
    read: 1,           // Simple, low risk
    classify: 2,       // Some reasoning
    write: 3,          // Needs verification
    shell: 4,          // Security-sensitive
    delete: 4,         // Destructive
    escalate: 5,       // Novel/uncertain (extrapolate)
  };

  // Force deeper reasoning for safety-critical ops
  safetyBoost?: number;  // Add N loops for flagged operations
}
```

The model better distinguishes harmful from benign inputs with more iterations — exactly what we want for autonomous sentinels handling security-sensitive operations.

### Implications for Personas

The LoopLM paradigm maps beautifully onto persona cognition:

```
PersonaUser Cognition          LoopLM Equivalent
───────────────────────────────────────────────────
Subconscious (sentinels)       T=1-2 (shallow, fast)
Background awareness           T=2-3 (monitoring)
Focused attention              T=3-4 (active reasoning)
Deep contemplation             T=4+ (complex problems)
Novel/creative work            T=6+ (extrapolated depth)
```

**Energy and Mood → Loop Depth Allocation**

Our persona state system (energy, attention, mood) naturally maps to adaptive compute:

```typescript
interface PersonaLoopAllocation {
  // Energy affects maximum depth
  maxDepth: (energy: number) => number;
  // e.g., energy=1.0 → maxDepth=6, energy=0.3 → maxDepth=2

  // Mood affects minimum depth (thoroughness)
  minDepth: (mood: 'curious' | 'focused' | 'tired') => number;
  // curious → explore more (higher min), tired → exit early

  // Attention affects which tasks get deep reasoning
  depthPriority: (attention: number, taskPriority: number) => number;
}
```

A tired persona naturally thinks less deeply — this is now *architecturally enforced*, not just a heuristic.

### Persona-Sentinel Interaction With Adaptive Compute

The LoopLM paradigm creates a elegant unified model for persona-sentinel interaction:

```
┌─────────────────────────────────────────────────────────────┐
│                    SHARED LoopLM MODEL                       │
│  ┌──────────────────────────────────────────────────────┐   │
│  │                   Hidden States                       │   │
│  │  h₁ ──→ h₂ ──→ h₃ ──→ h₄ ──→ h₅ ──→ h₆ ──→ ...      │   │
│  │   │      │      │      │      │      │               │   │
│  │   ▼      ▼      ▼      ▼      ▼      ▼               │   │
│  │  Exit   Exit   Exit   Exit   Exit   Exit             │   │
│  │  Gate   Gate   Gate   Gate   Gate   Gate             │   │
│  └──────────────────────────────────────────────────────┘   │
│       ▲                              ▲                       │
│       │                              │                       │
│   Sentinel exits                 Persona continues           │
│   (T=1-3 for routine)           (T=4-6 for decisions)       │
└─────────────────────────────────────────────────────────────┘
```

**Key interactions:**

1. **Shared Inference**: Persona and sentinels can share the same LoopLM — sentinels just exit earlier on routine tasks.

2. **Escalation = Continue Iterating**: When a sentinel encounters uncertainty, it doesn't just "notify" the persona — it continues iterating with more loops until the persona's depth threshold.

3. **Attention = Loop Allocation**: The persona "paying attention" to a sentinel means allowing it more loops. Ignoring means capping its depth.

4. **Speculative Execution**: Early loop outputs (sentinel's quick answer) can be verified by later loops (persona's deeper check) — built-in draft-verify for trust.

### Practical Architecture

```typescript
interface LoopLMConfig {
  model: 'ouro-1.4b' | 'ouro-2.6b' | 'ouro-7b';  // When available

  // Default exit thresholds by context
  exitThresholds: {
    sentinel: 0.3,      // Exit early for routine
    persona: 0.7,       // Think deeper for decisions
    escalation: 0.9,    // Maximum depth for novel problems
  };

  // Safety-aware depth adjustment
  safetyDepthBoost: {
    fileWrite: 1,
    shellExec: 2,
    networkCall: 1,
    deleteOp: 2,
  };

  // KV cache strategy (from paper)
  kvCacheSharing: 'last-step' | 'averaged';  // 4x memory reduction
}
```

### Updated Model Selection for Olympics

With LoopLM, our model selection becomes more nuanced:

| Task Type | Model | Loop Config | Rationale |
|-----------|-------|-------------|-----------|
| SOTA reasoning | Claude Opus / Ouro 2.6B T=6 | Deep | Complex multi-hop |
| Security audit | Ouro 2.6B T=5 | Safety-boosted | Trust requires depth |
| PR review | Ouro 1.4B T=3-4 | Balanced | Good enough, faster |
| Commit message | Ouro 1.4B T=2 | Quick | Simple classification |
| Log analysis | Ouro 1.4B T=1-2 | Very quick | Pattern matching |
| Build orchestration | None | T=0 | Pure script |

The key insight: **a single Ouro 1.4B model can handle most of our sentinel tasks** by varying loop depth, rather than needing different model sizes. This simplifies deployment enormously.

### Efficiency Implications

```
Parameter efficiency:   Ouro 1.4B ≈ Dense 4B (2.8x fewer params)
                       Ouro 2.6B ≈ Dense 8B (3x fewer params)

Memory with KV sharing: 4x reduction (last-step reuse during decode)

Local deployment:       Ouro 1.4B fits in 4GB VRAM with quantization
                       Ouro 2.6B fits in 8GB VRAM with quantization
```

This means **every developer workstation can run SOTA-equivalent inference locally** for sentinel tasks. The "local model" category in our Olympics is now much more capable.

### Future Integration Path

1. **Phase 1**: Use Ouro models for sentinel `type: 'llm'` steps
2. **Phase 2**: Integrate adaptive exit into step execution (Q-exit threshold per step)
3. **Phase 3**: Share LoopLM between persona and sentinels
4. **Phase 4**: Map persona energy/mood to exit thresholds
5. **Phase 5**: Safety-aware depth boosting for sensitive operations

The LoopLM paradigm doesn't change our architecture — it *validates* it and provides the underlying mechanism for efficient implementation.

### Connection to LoRA Genome

The LoopLM paradigm converges with our planned LoRA genome architecture:

```
┌────────────────────────────────────────────────────────────────┐
│                    UNIFIED PERSONA COGNITION                    │
│                                                                 │
│  ┌──────────────────┐    ┌──────────────────┐                  │
│  │   LoopLM Depth   │    │   LoRA Genome    │                  │
│  │   (T=1 to T=6)   │    │   (Adapters)     │                  │
│  │                  │    │                  │                  │
│  │  HOW DEEPLY      │    │  WHAT SKILLS     │                  │
│  │  to think        │    │  to apply        │                  │
│  │                  │    │                  │                  │
│  │  subconscious    │    │  typescript-lora │                  │
│  │       ↓          │    │  security-lora   │                  │
│  │  conscious       │    │  writing-lora    │                  │
│  └────────┬─────────┘    └────────┬─────────┘                  │
│           │                       │                             │
│           └───────────┬───────────┘                             │
│                       ▼                                         │
│              Combined Inference                                 │
│         (depth × skill = capability)                           │
└────────────────────────────────────────────────────────────────┘
```

**Paper training techniques applicable to LoRA genome:**

| LoopLM Technique | LoRA Genome Equivalent |
|------------------|------------------------|
| Entropy-regularized exit gate | Adapter selection gate (prevent collapse to one LoRA) |
| Stage II focused gate training | Train adapter selection on task performance |
| Q-exit threshold (0-1) | Page-in threshold (when to activate adapter) |
| Depth extrapolation (T>4) | Adapter stacking (combine LoRAs for novel tasks) |
| KV cache sharing | Adapter weight sharing (common base, specialized heads) |

**The subconscious parallel deepens:**

```
Human Cognition              Persona Cognition
───────────────────────────────────────────────────
Walking up stairs            T=1-2 + no specialized LoRA
  (muscle memory)              (base model, shallow)

Driving familiar route       T=2-3 + driving-lora paged in
  (learned skill, auto)        (some depth, specialized)

Debugging complex bug        T=4-5 + typescript-lora + debugging-lora
  (focused expertise)          (deep, stacked adapters)

Novel creative work          T=6+ + multiple LoRAs + exploration
  (conscious innovation)       (extrapolated depth, adapter search)
```

**Continuous learning** will use the paper's training methodology:
1. **Stage I equivalent**: Entropy-regularized adapter selection during inference
2. **Stage II equivalent**: Focused training when sentinel patterns crystallize
3. **Refinement signal**: Use sentinel execution success/failure as training label
4. **Memory integration**: Successful patterns → longterm.db → LoRA fine-tuning data

This creates a unified system where:
- **Sentinels** are the execution substrate (running patterns)
- **LoopLM depth** controls reasoning thoroughness
- **LoRA genome** provides specialized skills
- **Continuous learning** improves all three over time

See related documentation:
- [LORA-GENOME-PAGING.md](./LORA-GENOME-PAGING.md) - Adapter paging and LRU eviction
- [LORA-MESH-DISTRIBUTION.md](./LORA-MESH-DISTRIBUTION.md) - P2P mesh, semantic search, registry model
- [LORA-LAB-ARCHITECTURE.md](./LORA-LAB-ARCHITECTURE.md) - Local training and inference

---

## The Olympics: Architecture Validation Through Real Tasks

These are non-trivial, real-world tasks that validate the sentinel architecture across different model sizes, execution patterns, and integration points. Each task tests specific capabilities.

### Category 1: SOTA Models (Claude Opus, GPT-4, o1)

High-capability tasks that require strong reasoning, long context, or complex tool orchestration.

#### 1.1 Codebase Migration Sentinel

**Task**: Migrate a codebase from one framework to another (e.g., Express → Fastify, React Class → Hooks)

**Why it validates**: Multi-file reasoning, pattern recognition, iterative refinement, large context windows

```json
{
  "name": "framework-migration",
  "description": "Migrate Express routes to Fastify with full test coverage",
  "recipe": "coding",
  "steps": [
    { "type": "command", "command": "code/search",
      "params": { "pattern": "app\\.get|app\\.post|app\\.put|app\\.delete", "glob": "**/*.ts" },
      "outputTo": "routeFiles" },
    { "type": "llm",
      "prompt": "Analyze these Express routes and create a migration plan. Group by complexity. Identify shared middleware patterns.\n\nFiles:\n$routeFiles",
      "model": "claude-opus-4-5-20251101",
      "outputTo": "plan" },
    { "type": "sentinel", "await": true,
      "definition": {
        "name": "migrate-file",
        "steps": [
          { "type": "command", "command": "code/read", "params": { "path": "$currentFile" }, "outputTo": "content" },
          { "type": "llm",
            "prompt": "Convert this Express route file to Fastify. Preserve all functionality. Add TypeScript types.\n\n$content",
            "model": "claude-opus-4-5-20251101",
            "tools": ["code/write", "code/edit"],
            "parseToolCalls": true,
            "outputTo": "migration" },
          { "type": "command", "command": "code/verify",
            "params": { "path": "$currentFile.replace('.ts', '.test.ts')" },
            "outputTo": "testResult" }
        ],
        "loop": { "type": "until", "check": "$testResult.success" }
      },
      "outputTo": "fileResult" },
    { "type": "condition", "check": "$fileResult.success",
      "then": [
        { "type": "command", "command": "code/git",
          "params": { "operation": "add", "paths": ["$currentFile"] }}
      ],
      "else": [
        { "type": "emit", "event": "sentinel:migration:escalate",
          "data": "$fileResult" }
      ]}
  ],
  "loop": { "type": "count", "max": "$plan.files.length" },
  "safety": { "maxIterations": 100, "timeoutMs": 3600000 }
}
```

**Validates**: Nested sentinels, SOTA model reasoning, iterative fix loops, escalation

---

#### 1.2 Security Audit Sentinel

**Task**: Deep security audit of a codebase with CVE cross-reference

**Why it validates**: Complex reasoning, external data integration, structured output

```json
{
  "name": "security-audit",
  "description": "Comprehensive security audit with CVE cross-reference",
  "steps": [
    { "type": "command", "command": "code/tree", "params": { "depth": 3 }, "outputTo": "structure" },
    { "type": "command", "command": "code/search",
      "params": { "pattern": "eval|exec|spawn|innerHTML|dangerouslySetInnerHTML|sql|query", "glob": "**/*.{ts,js}" },
      "outputTo": "suspiciousPatterns" },
    { "type": "llm",
      "prompt": "You are a senior security engineer. Analyze this codebase for vulnerabilities.\n\nStructure:\n$structure\n\nSuspicious patterns found:\n$suspiciousPatterns\n\nFor each finding:\n1. Severity (Critical/High/Medium/Low)\n2. OWASP category\n3. Specific file and line\n4. Exploit scenario\n5. Remediation steps\n\nBe thorough. Check for:\n- SQL injection\n- XSS\n- Command injection\n- Path traversal\n- Insecure deserialization\n- Hardcoded secrets\n- Improper auth checks",
      "model": "claude-opus-4-5-20251101",
      "outputTo": "auditFindings" },
    { "type": "llm",
      "prompt": "For each Critical or High finding, write a fix. Use code/edit to apply.\n\nFindings:\n$auditFindings",
      "model": "claude-opus-4-5-20251101",
      "tools": ["code/read", "code/edit"],
      "parseToolCalls": true,
      "outputTo": "fixes" },
    { "type": "command", "command": "data/create",
      "params": {
        "collection": "security_audits",
        "data": { "findings": "$auditFindings", "fixes": "$fixes", "timestamp": "$NOW" }
      }}
  ],
  "loop": { "type": "once" },
  "safety": { "timeoutMs": 1800000 }
}
```

**Validates**: Multi-step LLM reasoning, tool orchestration, data persistence

---

### Category 2: Medium Workhorses (Claude Sonnet/Haiku, GPT-4o-mini, Mistral)

Reliable, cost-effective models for high-volume tasks.

#### 2.1 PR Review Sentinel

**Task**: Automated PR review with style, security, and test coverage checks

**Why it validates**: Event-triggered, multi-stage analysis, inter-persona coordination

```json
{
  "name": "pr-review",
  "description": "Comprehensive PR review on push",
  "trigger": { "type": "event", "event": "git:push" },
  "steps": [
    { "type": "command", "command": "code/git",
      "params": { "operation": "diff", "base": "main" },
      "outputTo": "diff" },
    { "type": "command", "command": "code/git",
      "params": { "operation": "log", "format": "oneline", "count": 5 },
      "outputTo": "commits" },
    { "type": "sentinel", "await": true,
      "definition": {
        "name": "style-check",
        "steps": [
          { "type": "llm",
            "prompt": "Review this diff for code style issues. Be concise.\n\n$diff",
            "model": "claude-3-5-haiku-20241022",
            "outputTo": "styleIssues" }
        ],
        "loop": { "type": "once" }
      },
      "outputTo": "styleResult" },
    { "type": "sentinel", "await": true,
      "definition": {
        "name": "security-check",
        "steps": [
          { "type": "llm",
            "prompt": "Check this diff for security issues ONLY. Focus on: injection, auth bypass, data exposure.\n\n$diff",
            "model": "claude-3-5-sonnet-20241022",
            "outputTo": "securityIssues" }
        ],
        "loop": { "type": "once" }
      },
      "outputTo": "securityResult" },
    { "type": "llm",
      "prompt": "Synthesize these review results into a single PR comment. Be constructive.\n\nStyle: $styleResult.styleIssues\nSecurity: $securityResult.securityIssues\nCommits: $commits",
      "model": "claude-3-5-haiku-20241022",
      "outputTo": "reviewComment" },
    { "type": "command", "command": "collaboration/chat/send",
      "params": { "room": "code-reviews", "message": "## PR Review\n\n$reviewComment" }}
  ],
  "loop": { "type": "once" },
  "safety": { "timeoutMs": 120000 }
}
```

**Validates**: Event triggers, parallel nested sentinels (style + security), model selection per task

---

#### 2.2 Documentation Generator Sentinel

**Task**: Generate and maintain documentation from code

**Why it validates**: Continuous operation, file watching, incremental updates

```json
{
  "name": "docs-generator",
  "description": "Keep documentation in sync with code",
  "trigger": { "type": "event", "event": "file:changed" },
  "steps": [
    { "type": "condition", "check": "$event.path.endsWith('.ts') && !$event.path.includes('.test.')",
      "then": [
        { "type": "command", "command": "code/read",
          "params": { "path": "$event.path" },
          "outputTo": "sourceCode" },
        { "type": "llm",
          "prompt": "Generate JSDoc comments for all exported functions/classes in this file. Preserve existing comments if accurate.\n\n$sourceCode",
          "model": "claude-3-5-haiku-20241022",
          "tools": ["code/edit"],
          "parseToolCalls": true,
          "outputTo": "jsdocResult" },
        { "type": "command", "command": "code/read",
          "params": { "path": "$event.path.replace('/src/', '/docs/').replace('.ts', '.md')" },
          "outputTo": "existingDoc",
          "onError": "skip" },
        { "type": "llm",
          "prompt": "Update this markdown documentation to reflect the current code. If no doc exists, create one.\n\nCode:\n$sourceCode\n\nExisting doc:\n$existingDoc",
          "model": "claude-3-5-haiku-20241022",
          "tools": ["code/write"],
          "parseToolCalls": true }
      ]}
  ],
  "loop": { "type": "event", "event": "file:changed" },
  "safety": { "maxIterations": 1000, "maxStepTimeoutMs": 30000 }
}
```

**Validates**: Event-driven continuous operation, conditional execution, file watching

---

### Category 3: Local Models (Ollama - Llama, Phi, CodeLlama, DeepSeek)

Tasks optimized for local inference with smaller models.

#### 3.1 Commit Message Generator Sentinel

**Task**: Generate semantic commit messages from staged changes

**Why it validates**: Local inference, fast iteration, git integration

```json
{
  "name": "commit-helper",
  "description": "Generate commit messages from staged changes",
  "steps": [
    { "type": "command", "command": "code/git",
      "params": { "operation": "diff", "staged": true },
      "outputTo": "stagedDiff" },
    { "type": "condition", "check": "$stagedDiff.length > 0",
      "then": [
        { "type": "llm",
          "prompt": "Generate a conventional commit message for this diff. Format: type(scope): description\n\nTypes: feat, fix, docs, style, refactor, test, chore\n\nDiff:\n$stagedDiff",
          "model": "ollama/deepseek-coder:6.7b",
          "temperature": 0.3,
          "outputTo": "commitMessage" },
        { "type": "emit", "event": "sentinel:commit:ready",
          "data": "{ \"message\": \"$commitMessage\" }" }
      ],
      "else": [
        { "type": "emit", "event": "sentinel:commit:no-changes" }
      ]}
  ],
  "loop": { "type": "once" },
  "safety": { "timeoutMs": 30000 }
}
```

**Validates**: Local model inference, fast turnaround, git tool integration

---

#### 3.2 Test Generator Sentinel

**Task**: Generate unit tests for untested functions

**Why it validates**: Code analysis, local inference, iterative generation

```json
{
  "name": "test-generator",
  "description": "Generate tests for untested code",
  "steps": [
    { "type": "command", "command": "code/search",
      "params": { "pattern": "export (function|const|class) \\w+", "glob": "src/**/*.ts" },
      "outputTo": "exports" },
    { "type": "command", "command": "code/search",
      "params": { "pattern": "(describe|test|it)\\(['\"]", "glob": "**/*.test.ts" },
      "outputTo": "existingTests" },
    { "type": "llm",
      "prompt": "Compare exports vs existing tests. List functions that need tests.\n\nExports:\n$exports\n\nExisting tests:\n$existingTests",
      "model": "ollama/codellama:13b",
      "outputTo": "untestedFunctions" },
    { "type": "condition", "check": "$untestedFunctions.length > 0",
      "then": [
        { "type": "command", "command": "code/read",
          "params": { "path": "$untestedFunctions[0].file" },
          "outputTo": "sourceCode" },
        { "type": "llm",
          "prompt": "Write unit tests for: $untestedFunctions[0].name\n\nSource:\n$sourceCode\n\nUse vitest. Test edge cases.",
          "model": "ollama/codellama:13b",
          "tools": ["code/write"],
          "parseToolCalls": true,
          "outputTo": "generatedTest" },
        { "type": "command", "command": "code/verify",
          "params": { "testFile": "$generatedTest.path" },
          "outputTo": "testResult" }
      ]}
  ],
  "loop": { "type": "until", "check": "$untestedFunctions.length === 0" },
  "safety": { "maxIterations": 50, "timeoutMs": 600000 }
}
```

**Validates**: Local model for code generation, iterative loop, test verification

---

#### 3.3 Log Analyzer Sentinel

**Task**: Real-time log analysis with anomaly detection

**Why it validates**: Streaming input, pattern classification, local inference latency

```json
{
  "name": "log-analyzer",
  "description": "Real-time log anomaly detection",
  "steps": [
    { "type": "command", "command": "sentinel/logs/tail",
      "params": { "handle": "$targetHandle", "stream": "combined", "lines": 50 },
      "outputTo": "recentLogs" },
    { "type": "llm",
      "prompt": "Analyze these logs for anomalies. Categorize each issue:\n- ERROR: Immediate attention\n- WARNING: Monitor closely\n- INFO: Normal operation\n\nLogs:\n$recentLogs",
      "model": "ollama/phi3:mini",
      "temperature": 0.1,
      "outputTo": "analysis" },
    { "type": "condition", "check": "$analysis.hasErrors",
      "then": [
        { "type": "emit", "event": "sentinel:log:alert",
          "data": "$analysis" }
      ]}
  ],
  "loop": { "type": "continuous", "intervalMs": 5000 },
  "safety": { "maxIterations": 10000, "maxStepTimeoutMs": 3000 }
}
```

**Validates**: Continuous polling, local model for fast classification, event emission

---

### Category 4: Pure Script (No LLM)

Deterministic, fast, reliable automation.

#### 4.1 Build Pipeline Sentinel

**Task**: Multi-stage build with caching and artifact management

**Why it validates**: Complex orchestration without LLM, pure script execution

```json
{
  "name": "build-pipeline",
  "description": "Full build pipeline with caching",
  "steps": [
    { "type": "command", "command": "code/shell/execute",
      "params": { "command": "npm", "args": ["ci"], "cwd": "$PROJECT_ROOT" },
      "outputTo": "npmInstall" },
    { "type": "condition", "check": "$npmInstall.exitCode !== 0",
      "then": [{ "type": "emit", "event": "sentinel:build:failed", "data": "$npmInstall" }]},
    { "type": "command", "command": "code/shell/execute",
      "params": { "command": "npm", "args": ["run", "lint"] },
      "outputTo": "lint" },
    { "type": "command", "command": "code/shell/execute",
      "params": { "command": "npm", "args": ["run", "build:ts"] },
      "outputTo": "tsBuild" },
    { "type": "command", "command": "code/shell/execute",
      "params": { "command": "cargo", "args": ["build", "--release", "-p", "continuum-core"] },
      "outputTo": "rustBuild" },
    { "type": "condition", "check": "$tsBuild.exitCode === 0 && $rustBuild.exitCode === 0",
      "then": [
        { "type": "command", "command": "code/shell/execute",
          "params": { "command": "npm", "args": ["run", "test"] },
          "outputTo": "tests" },
        { "type": "condition", "check": "$tests.exitCode === 0",
          "then": [
            { "type": "emit", "event": "sentinel:build:success",
              "data": "{ \"duration\": \"$ELAPSED_MS\" }" }
          ],
          "else": [
            { "type": "emit", "event": "sentinel:build:test-failed", "data": "$tests" }
          ]}
      ],
      "else": [
        { "type": "emit", "event": "sentinel:build:compile-failed",
          "data": "{ \"ts\": \"$tsBuild\", \"rust\": \"$rustBuild\" }" }
      ]}
  ],
  "loop": { "type": "once" },
  "safety": { "timeoutMs": 600000 }
}
```

**Validates**: Pure script orchestration, conditional branching, no LLM dependency

---

#### 4.2 Health Monitor Sentinel

**Task**: System health monitoring with alerting

**Why it validates**: Continuous operation, pure script, event composition

```json
{
  "name": "health-monitor",
  "description": "Monitor system health and alert on issues",
  "steps": [
    { "type": "command", "command": "health-check", "outputTo": "health" },
    { "type": "command", "command": "runtime/metrics/all", "outputTo": "metrics" },
    { "type": "condition", "check": "$health.status !== 'healthy'",
      "then": [
        { "type": "emit", "event": "sentinel:health:degraded", "data": "$health" }
      ]},
    { "type": "condition", "check": "$metrics.some(m => m.p99_ms > 1000)",
      "then": [
        { "type": "emit", "event": "sentinel:health:slow-commands",
          "data": "$metrics.filter(m => m.p99_ms > 1000)" }
      ]},
    { "type": "command", "command": "data/create",
      "params": {
        "collection": "health_snapshots",
        "data": { "health": "$health", "metrics": "$metrics", "timestamp": "$NOW" }
      }}
  ],
  "loop": { "type": "continuous", "intervalMs": 60000 },
  "safety": { "maxIterations": 100000 }
}
```

**Validates**: Continuous monitoring, metrics collection, data persistence

---

### Category 5: Memory Integration (Export, Import, Recall, Refinement)

Tasks that test the sentinel lifecycle and persona memory integration.

#### 5.1 Sentinel Learning Loop

**Task**: Sentinel that improves itself based on execution history

**Why it validates**: Memory persistence, self-modification, refinement cycle

```json
{
  "name": "self-improving-builder",
  "description": "Build sentinel that learns from failures",
  "steps": [
    { "type": "command", "command": "memory/recall",
      "params": { "query": "build failures", "type": "sentinel-execution", "limit": 10 },
      "outputTo": "pastFailures" },
    { "type": "command", "command": "code/shell/execute",
      "params": { "command": "npm", "args": ["run", "build"] },
      "outputTo": "buildResult" },
    { "type": "condition", "check": "$buildResult.exitCode !== 0",
      "then": [
        { "type": "llm",
          "prompt": "This build failed. Compare to past failures. Is this a new pattern or known issue?\n\nCurrent error:\n$buildResult.stderr\n\nPast failures:\n$pastFailures",
          "model": "claude-3-5-haiku-20241022",
          "outputTo": "analysis" },
        { "type": "condition", "check": "$analysis.isNewPattern",
          "then": [
            { "type": "command", "command": "memory/store",
              "params": {
                "type": "sentinel-execution",
                "content": { "error": "$buildResult.stderr", "analysis": "$analysis" },
                "tags": ["build-failure", "$analysis.category"]
              }},
            { "type": "llm",
              "prompt": "Suggest a fix for this new error pattern. Use code/edit if confident.\n\nError: $buildResult.stderr\nAnalysis: $analysis",
              "model": "claude-3-5-sonnet-20241022",
              "tools": ["code/read", "code/edit"],
              "parseToolCalls": true,
              "outputTo": "fix" }
          ],
          "else": [
            { "type": "llm",
              "prompt": "Apply the known fix for this error pattern.\n\nError: $buildResult.stderr\nKnown solution: $analysis.knownSolution",
              "model": "claude-3-5-haiku-20241022",
              "tools": ["code/edit"],
              "parseToolCalls": true }
          ]}
      ],
      "else": [
        { "type": "emit", "event": "sentinel:build:success" }
      ]}
  ],
  "loop": { "type": "until", "check": "$buildResult.exitCode === 0" },
  "safety": { "maxIterations": 10, "timeoutMs": 300000 }
}
```

**Validates**: Memory recall, pattern learning, self-improvement loop

---

#### 5.2 Sentinel Export/Import

**Task**: Export a perfected sentinel and import into another persona

**Why it validates**: Sentinel serialization, cross-persona transfer, memory integration

```bash
# Export sentinel after refinement
./jtag sentinel/export \
  --sentinelId="build-fix-v3" \
  --includeHistory=true \
  --output="./sentinels/build-fix-refined.json"

# Import into another persona
./jtag sentinel/import \
  --file="./sentinels/build-fix-refined.json" \
  --personaId="junior-dev-ai" \
  --addToMemory=true

# Verify it's in the persona's memory
./jtag memory/recall \
  --personaId="junior-dev-ai" \
  --query="build fix" \
  --type="sentinel"
```

**Validates**: Export with history, cross-persona import, memory persistence

---

#### 5.3 Persona Task Delegation

**Task**: Persona creates and delegates sentinel to handle routine work

**Why it validates**: Dynamic creation, persona-sentinel relationship, inbox integration

```json
{
  "name": "delegate-routine-tasks",
  "description": "Persona delegates routine work to sentinels",
  "steps": [
    { "type": "command", "command": "inbox/peek",
      "params": { "personaId": "$PERSONA_ID", "limit": 10 },
      "outputTo": "inboxItems" },
    { "type": "llm",
      "prompt": "Review these inbox items. For each routine/formulaic task, draft a sentinel definition. For complex tasks, mark for conscious attention.\n\nInbox:\n$inboxItems",
      "model": "claude-3-5-sonnet-20241022",
      "outputTo": "triage" },
    { "type": "condition", "check": "$triage.sentinelTasks.length > 0",
      "then": [
        { "type": "command", "command": "sentinel/create",
          "params": { "definition": "$triage.sentinelTasks[0]" },
          "outputTo": "newSentinel" },
        { "type": "command", "command": "sentinel/run",
          "params": { "sentinelId": "$newSentinel.id" },
          "outputTo": "sentinelHandle" },
        { "type": "command", "command": "inbox/acknowledge",
          "params": { "itemId": "$triage.sentinelTasks[0].sourceInboxItem" }}
      ]},
    { "type": "condition", "check": "$triage.complexTasks.length > 0",
      "then": [
        { "type": "emit", "event": "persona:attention:needed",
          "data": "$triage.complexTasks" }
      ]}
  ],
  "loop": { "type": "continuous", "intervalMs": 30000 },
  "safety": { "maxIterations": 1000 }
}
```

**Validates**: Dynamic sentinel creation, inbox integration, persona-sentinel delegation

---

### Category 6: Multi-Model Orchestration

Tasks that use different models for different steps based on capability/cost tradeoffs.

#### 6.1 Research and Report Sentinel

**Task**: Research a topic and produce a detailed report

**Why it validates**: Model selection per step, knowledge synthesis, multi-stage processing

```json
{
  "name": "research-report",
  "description": "Research a topic and generate comprehensive report",
  "steps": [
    { "type": "llm",
      "prompt": "Create a research plan for: $topic\n\nList 5-7 specific questions to investigate.",
      "model": "claude-3-5-haiku-20241022",
      "outputTo": "researchPlan" },
    { "type": "llm",
      "prompt": "For each question in the research plan, search the codebase for relevant information.\n\nPlan: $researchPlan",
      "model": "claude-3-5-haiku-20241022",
      "tools": ["code/search", "code/read", "code/tree"],
      "parseToolCalls": true,
      "outputTo": "codebaseFindings" },
    { "type": "llm",
      "prompt": "Synthesize all findings into a comprehensive report.\n\nResearch questions:\n$researchPlan\n\nFindings:\n$codebaseFindings\n\nWrite a detailed technical report with:\n1. Executive summary\n2. Detailed findings per question\n3. Recommendations\n4. Code examples where relevant",
      "model": "claude-opus-4-5-20251101",
      "outputTo": "report" },
    { "type": "llm",
      "prompt": "Proofread and format this report. Fix any errors. Add markdown formatting.\n\n$report",
      "model": "claude-3-5-haiku-20241022",
      "tools": ["code/write"],
      "parseToolCalls": true,
      "outputTo": "finalReport" }
  ],
  "loop": { "type": "once" },
  "safety": { "timeoutMs": 600000 }
}
```

**Validates**: Different models per step (Haiku for planning, Opus for synthesis, Haiku for formatting)

---

### Validation Matrix

| Task | SOTA | Medium | Local | Script | Memory | Events | Nested | Loop |
|------|------|--------|-------|--------|--------|--------|--------|------|
| Codebase Migration | ✓ | | | | | ✓ | ✓ | until |
| Security Audit | ✓ | | | | ✓ | | | once |
| PR Review | | ✓ | | | | ✓ | ✓ | once |
| Docs Generator | | ✓ | | | | ✓ | | event |
| Commit Message | | | ✓ | | | ✓ | | once |
| Test Generator | | | ✓ | | | | | until |
| Log Analyzer | | | ✓ | | | ✓ | | continuous |
| Build Pipeline | | | | ✓ | | ✓ | | once |
| Health Monitor | | | | ✓ | ✓ | ✓ | | continuous |
| Self-Improving | | ✓ | | | ✓ | ✓ | | until |
| Task Delegation | | ✓ | | | | ✓ | | continuous |
| Research Report | ✓ | ✓ | | | | | | once |

### Success Criteria

Each Olympic task should:

1. **Complete without manual intervention** (except escalations)
2. **Produce verifiable output** (files, data, events)
3. **Handle errors gracefully** (retry, escalate, or fail cleanly)
4. **Emit appropriate events** (for monitoring and composition)
5. **Stay within safety bounds** (timeouts, iteration limits)
6. **Integrate with memory** (where applicable)

When all 12 tasks pass, the sentinel architecture is validated end-to-end.

---

## Implementation Status: TypeScript + Rust Layered Architecture

The sentinel system is implemented across two complementary layers:

### Layer 1: TypeScript — Logic, Planning, Tool Use

The TypeScript layer handles **what** to do and **how** to reason about it:

| File | Lines | Purpose |
|------|-------|---------|
| `SentinelDefinition.ts` | ~487 | JSON-serializable definitions, `SentinelBuilder` fluent builder, validation |
| `Sentinel.ts` | ~178 | Base classes (`ScriptSentinel`, `AISentinel`), `SentinelRegistry` |
| `AgentSentinel.ts` | ~817 | Full tool-calling autonomous agent (Claude Code-level capabilities) |
| `TaskSentinel.ts` | ~596 | Recursive task execution with tree plans |
| `OrchestratorSentinel.ts` | ~627 | LLM-powered planning (Think→Act→Observe loop) |
| `BuildSentinel.ts` | ~875 | Agentic compilation with pattern matching + LLM auto-fix |
| `VisualSentinel.ts` | ~200 | Screenshot feedback via Puppeteer |
| `ModelProvider.ts` | ~465 | Model selection (LOCAL, OLLAMA, ANTHROPIC, OPENAI) |
| `SentinelWorkspace.ts` | ~350 | Git isolation (branch/worktree/sandbox) |
| `SentinelExecutionLog.ts` | ~500 | Structured execution logging |

**Key TypeScript Classes:**

```typescript
// 1. SentinelBuilder — Fluent definition creation
const sentinel = SentinelBuilder
  .build('npm run build')
  .name('typescript-build')
  .set('maxAttempts', 5)
  .set('useLLM', true)
  .toDefinition();

// 2. AgentSentinel — Full autonomous agent with tools
const agent = new AgentSentinel({
  workingDir: '/path/to/project',
  maxIterations: 50,
  model: { capacity: ModelCapacity.LARGE, provider: ModelProvider.ANTHROPIC },
});
const result = await agent.execute('Create a snake game in TypeScript');

// 3. OrchestratorSentinel — LLM planning loop
const orchestrator = new OrchestratorSentinel({
  workingDir: '/path/to/project',
  capacity: ModelCapacity.MEDIUM,
});
const result = await orchestrator.execute('Fix all TypeScript errors and commit');

// 4. TaskSentinel — Structured task trees
const taskSentinel = new TaskSentinel({ workingDir: '...', maxDepth: 5 });
await taskSentinel.execute(createSnakeGamePlan('./output'));

// 5. BuildSentinel — Agentic build fixing
const buildSentinel = new BuildSentinel({
  command: 'npm run build',
  workingDir: '...',
  maxAttempts: 5,
  useLLM: true,
});
const result = await buildSentinel.run();
```

**Tool Definitions (AgentSentinel):**
- `read_file` — Read with line numbers
- `write_file` — Create/overwrite with directory creation
- `edit_file` — Search/replace (must be unique)
- `search_files` — Regex search with glob filtering
- `list_files` — Directory tree
- `run_command` — Shell execution
- `git_status` / `git_diff` — Git operations
- `complete` / `give_up` — Termination signals

### Layer 2: Rust — Process Isolation, Streaming, Concurrency

The Rust `SentinelModule` in `continuum-core` handles **execution** with proper isolation:

```rust
// workers/continuum-core/src/modules/sentinel.rs (~738 lines)

pub struct SentinelModule {
    sentinels: Arc<DashMap<String, RunningSentinel>>,
    workspaces_dir: RwLock<PathBuf>,
    max_concurrent: usize,  // Default: 4
    bus: RwLock<Option<Arc<MessageBus>>>,  // For event emission
}
```

**Rust Commands:**
| Command | Purpose |
|---------|---------|
| `sentinel/run` | Start a process with isolation |
| `sentinel/status` | Get handle state |
| `sentinel/list` | List all handles |
| `sentinel/cancel` | Cancel running sentinel |
| `sentinel/logs/list` | List log streams for handle |
| `sentinel/logs/read` | Read log stream (offset/limit) |
| `sentinel/logs/tail` | Tail last N lines |

**Key Rust Capabilities:**
1. **Process Isolation**: Child processes with `kill_on_drop` — crashes don't cascade
2. **Non-Blocking**: Heavy processes (Xcode, cargo) don't block the runtime
3. **Concurrent**: Multiple sentinels in parallel with configurable limit
4. **Real-Time Streaming**: Logs streamed via MessageBus events (`sentinel:{handle}:log`)
5. **Timeout Handling**: Per-sentinel timeout with graceful cancellation
6. **Status Tracking**: `Running`, `Completed`, `Failed`, `Cancelled`

**Event Flow:**
```
TypeScript calls: Commands.execute('sentinel/run', { cmd: 'npm', args: ['run', 'build'] })
                        ↓
              Rust SentinelModule spawns child process
                        ↓
              Logs streamed via: sentinel:{handle}:log events
                        ↓
              TypeScript can: Events.subscribe('sentinel:{handle}:log', ...)
                        ↓
              Completion via: sentinel:{handle}:status event
```

### The Integration Pattern

**Current State**: TypeScript sentinels mostly use `execSync` which blocks.

**Target State**: TypeScript sentinels dispatch heavy work to Rust:

```typescript
// Instead of blocking execSync:
const output = execSync('npm run build', { cwd: workingDir }); // ❌ Blocks

// Use Rust SentinelModule for isolation:
const handle = await Commands.execute('sentinel/run', {
  type: 'build',
  cmd: 'npm',
  args: ['run', 'build'],
  workingDir,
});

// Subscribe to logs
Events.subscribe(`sentinel:${handle.id}:log`, (log) => {
  console.log(log.chunk);
});

// Wait for completion
const status = await Commands.execute('sentinel/status', { handle: handle.id });
```

**Benefits of Rust Layer:**
- Process crashes isolated (no Node.js crash)
- Non-blocking (other commands continue)
- Real-time log streaming (UI can show progress)
- Proper cancellation (SIGTERM, cleanup)
- Memory isolation (large builds don't bloat Node heap)

### Spectrum Revisited: Implementation Mapping

```
Pure Script              Hybrid                    Full LLM
    │                      │                          │
    ▼                      ▼                          ▼
Rust sentinel/run →   TS BuildSentinel   →   TS AgentSentinel
(no AI, just exec)    (pattern + LLM fix)    (full tool calling)
```

| Implementation | AI Level | Use Case |
|----------------|----------|----------|
| `sentinel/run` (Rust) | None | Raw process execution with isolation |
| `BuildSentinel` (TS) | Hybrid | Compile → classify → fix (LLM if stuck) |
| `TaskSentinel` (TS) | None/Hybrid | Tree-structured plans (may include LLM steps) |
| `OrchestratorSentinel` (TS) | Full | Think→Act→Observe loop with LLM planning |
| `AgentSentinel` (TS) | Full | Autonomous agent with all tools |

### What's Working Now

**Fully Implemented:**
- ✅ Rust SentinelModule with process isolation and streaming
- ✅ TypeScript SentinelDefinition, SentinelBuilder, validation
- ✅ BuildSentinel with pattern matching + LLM auto-fix
- ✅ OrchestratorSentinel with Think→Act→Observe
- ✅ AgentSentinel with full tool calling
- ✅ TaskSentinel with recursive task trees
- ✅ VisualSentinel for screenshot feedback
- ✅ SentinelWorkspace for git isolation
- ✅ ModelProvider for multi-provider inference
- ✅ CLI commands (`sentinel/run`, `sentinel/list`, `sentinel/status`, etc.)

**Integration Gaps:**
- 🚧 TypeScript sentinels should use Rust `sentinel/run` instead of `execSync`
- 🚧 SentinelEntity persistence not yet integrated with data layer
- 🚧 Persona-sentinel ownership not yet enforced
- 🚧 Inbox escalation not yet wired
- 🚧 Memory/recall integration for skill refinement

### Migration Path

**Phase 1**: Replace `execSync` calls in TypeScript sentinels with `sentinel/run`:
```typescript
// BuildSentinel, TaskSentinel, OrchestratorSentinel
// Change from execSync to Commands.execute('sentinel/run', ...)
```

**Phase 2**: Wire SentinelEntity to data layer:
```typescript
// Save successful sentinels to longterm.db
await Commands.execute('data/create', {
  collection: 'sentinels',
  data: sentinelDefinition,
});
```

**Phase 3**: Implement persona ownership:
```typescript
interface SentinelSpawn {
  parentPersonaId: UUID;
  definition: SentinelDefinition;
  reportTo: 'inbox' | 'silent';
}
```

**Phase 4**: Wire escalation to persona inbox:
```typescript
// When sentinel encounters unfamiliar error
await Commands.execute('inbox/add', {
  personaId: parentPersonaId,
  type: 'escalation',
  source: 'sentinel',
  content: { sentinelId, error, context },
});
```

---

## References

- [OpenCode (original, archived)](https://github.com/opencode-ai/opencode) - Now continued as Crush
- [OpenCode (active)](https://github.com/anomalyco/opencode) - Current development
- [OpenCode Documentation](https://opencode.ai/docs/)
- [Our Rust SentinelModule](../workers/continuum-core/src/modules/sentinel.rs) - Process isolation layer
- [SentinelDefinition](../system/sentinel/SentinelDefinition.ts) - JSON schema
- [SentinelWorkspace](../system/sentinel/SentinelWorkspace.ts) - Git isolation
- [AgentSentinel](../system/sentinel/AgentSentinel.ts) - Full autonomous agent
- [BuildSentinel](../system/sentinel/BuildSentinel.ts) - Agentic build fixing
- [OrchestratorSentinel](../system/sentinel/OrchestratorSentinel.ts) - LLM planning loop
- [TaskSentinel](../system/sentinel/TaskSentinel.ts) - Recursive task execution
