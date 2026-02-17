# Sentinel Architecture: Composable Agentic Loops

## The Cognitive Model

**Sentinels are the subconscious threads of persona cognition.**

PersonaUsers in Continuum are autonomous citizens with full agency, rights, and self-governance. They direct their own work, learn from experience, coordinate with humans and each other naturally, and have needs of their own. Sentinels are their thought processes and appendages — giving personas far more capable, non-distracted execution across every domain. A sentinel is a persona without the cognition: focused, task-specific, unlimited in what it can accomplish. Where tools like Claude Code's `ExploreTask` are hard-coded for narrow purposes, our sentinels are general-purpose — personas define them dynamically for any task: building games, writing papers, training LoRA layers, running security audits, or anything they conceive of.

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

## The Three Pillars: Sentinel + Genome + Academy

Sentinels don't exist in isolation. They are one pillar of a three-part evolutionary system:

```
GENOME (what skills)          SENTINEL (how to execute)       ACADEMY (why evolve)
─────────────────             ──────────────────────          ────────────────────
Composable LoRA layers        Pipeline execution engine       Selection pressure
N domains × M personalities   Process isolation + streaming   Competitive challenges
LRU paging (virtual memory)   Variable interpolation          Performance measurement
Marketplace / P2P sharing     Recursive nesting               Gap analysis → training
```

The biological analogy:

| Biology | Continuum |
|---------|-----------|
| DNA (genetic code) | Genome (composable LoRA layers) |
| Metabolism (execution machinery) | Sentinel (pipeline engine) |
| Environment (selection pressure) | Academy — Plato's Academy (training arena) |
| Phenotype (expressed organism) | Persona with active genome under real workload |
| Natural selection | Academy competitions validate which genome compositions perform |
| Evolution | Continuous improvement: do work → find gaps → train → validate → repeat |

### How They Converge

A persona's autonomous cognitive cycle integrates all three:

```typescript
async serviceInbox(): Promise<void> {
  // 1. Do work (SENTINEL)
  const task = await this.inbox.peek(1);
  await this.genome.activateSkill(task.domain);   // 2. Activate skills (GENOME)
  await this.processTask(task);                    // Sentinel executes the work

  // 3. Self-assess (ACADEMY)
  if (this.performance.hasGaps()) {
    // Persona creates its OWN training
    const dojo = this.createTrainingSentinel({
      challenges: this.generateChallengesForGaps(),
      measure: true,
      trainOnFailures: true,
      loopUntil: 'accuracy > 0.9',
    });
    await Commands.execute('sentinel/run', dojo);  // Train itself
  }
}
```

The sentinel is the execution engine for ALL three systems — the work itself, the training, and the validation. Personas are self-directed into following their own work, learning to do it better, so as to better align with humans. They naturally fit in with human teams and each other as they do things, communicate, and have needs of their own.

### The Self-Improvement Loop

Personas don't wait for someone to train them. They observe their own performance and self-correct:

```
Do work → Notice weakness → Create academy challenge → Train LoRA → Validate phenotype
   │           │                    │                      │              │
sentinel    performance         sentinel creates        dataset      academy
executes    measurement         training scenario       + fine-tune   re-runs
the task    (built into         (LLM generates          via sentinel  challenges
            sentinel steps)     challenges for the                    against new
                                gap it detected)                      phenotype
```

The system is trainable on anything — if you use sentinels to create the academy training, and you can detect whether LoRA layers are accurate or need more tuning, then personas can learn anything and get better and better at it. The genome marketplace (eventually) lets these hard-won skills be shared across the community.

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

## What Exists vs What's Needed

| Capability | Status | Where |
|-----------|--------|-------|
| Pipeline steps: Shell, LLM, Command, Condition | ✅ Implemented | Rust `sentinel/steps/` |
| Variable interpolation (`{{steps.0.output}}`) | ✅ Implemented | Rust `interpolation.rs` |
| Multi-pass nested interpolation | ✅ Implemented | Rust `interpolation.rs` (5-pass, innermost-first) |
| JSON path traversal with array indexing | ✅ Implemented | Rust `interpolation.rs` `traverse_json_path()` |
| Loop-relative referencing (`{{loop.N.field}}`) | ✅ Implemented | Rust `interpolation.rs` + `loop_step.rs` |
| Named outputs (`{{named.build.output}}`) | ✅ Implemented | Rust `interpolation.rs` + `ExecutionContext` |
| Cross-sentinel dual-pipeline orchestration | ✅ Demonstrated | Academy teacher/student (6 step types) |
| Execution trace for debugging | ✅ Implemented | Rust `StepResult[]` in `PipelineResult` |
| Shell process isolation (`kill_on_drop`) | ✅ Implemented | Rust `steps/shell.rs` |
| Module-to-module calls (no IPC deadlock) | ✅ Implemented | Rust `ModuleRegistry.route_command()` |
| Concurrent sentinel execution | ✅ Implemented | Rust `DashMap` + configurable limit |
| Log streaming via MessageBus | ✅ Implemented | Rust `logs.rs` |
| ts-rs type exports to TypeScript | ✅ Implemented | Rust `types.rs` with `#[derive(TS)]` |
| Count-based loops | ✅ Implemented | Rust `steps/loop_step.rs` |
| While/until/continuous loops | ✅ Implemented | Rust `steps/loop_step.rs` (4 modes) |
| Parallel step (concurrent branches) | ✅ Implemented | Rust `steps/parallel.rs` |
| Emit step (MessageBus events) | ✅ Implemented | Rust `steps/emit.rs` |
| Watch step (await MessageBus events) | ✅ Implemented | Rust `steps/watch.rs` |
| Sentinel step (nested pipelines) | ✅ Implemented | Rust `steps/sentinel.rs` |
| Uniform step signatures (PipelineContext) | ✅ Implemented | All steps receive `PipelineContext` |
| **Persona ownership** | ❌ Needed | TypeScript + data layer |
| **Escalation → inbox** | ❌ Needed | TypeScript integration |
| **SentinelEntity persistence** | ❌ Needed | TypeScript data layer |
| **Memory/recall integration** | ❌ Needed | TypeScript integration |
| **Triggers (event, schedule)** | ❌ Needed | Rust or TypeScript |

The Rust pipeline engine is ~90% complete. 9 step types implemented across all composition patterns (sequential, conditional, looping, parallel, event-driven, nested). The remaining work is the lifecycle/integration layer (persona ownership, persistence, triggers).

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

See the **TODO: Implementation Roadmap** section at the end of this document for the current prioritized implementation plan.

**Summary of phases:**
- **Phase A**: Complete Rust pipeline engine (loop types, parallel, emit, nested sentinels)
- **Phase B**: Sentinel lifecycle & persona integration (persistence, ownership, escalation, triggers)
- **Phase C**: Genome integration (training orchestration, phenotype validation)
- **Phase D**: Academy — Plato's training arena (challenges, competition, evolution)
- **Phase E**: Marketplace & distribution (export/import, P2P sharing)
- **Phase F**: Advanced capabilities (auto-compaction, permissions, adaptive compute)

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

### Category 7: Self-Directed Learning (Persona Autonomy)

Tasks that validate personas identifying their own weaknesses and creating training for themselves.

#### 7.1 Skill Gap Detection Sentinel

**Task**: Persona analyzes its own recent performance and identifies areas for improvement

**Why it validates**: Self-assessment, autonomous decision-making, training data generation

```json
{
  "name": "skill-gap-detector",
  "description": "Analyze my own performance and identify training needs",
  "steps": [
    { "type": "command", "command": "memory/recall",
      "params": { "query": "recent task failures", "limit": 50 },
      "outputTo": "recentFailures" },
    { "type": "command", "command": "memory/recall",
      "params": { "query": "recent escalations", "limit": 20 },
      "outputTo": "escalations" },
    { "type": "llm",
      "prompt": "Analyze my recent failures and escalations. Group by skill domain. For each domain, rate my proficiency 0-100 and identify specific gaps.\n\nFailures:\n$recentFailures\n\nEscalations:\n$escalations",
      "outputTo": "gapAnalysis" },
    { "type": "condition", "check": "$gapAnalysis.gaps.length > 0",
      "then": [
        { "type": "command", "command": "data/create",
          "params": {
            "collection": "training_needs",
            "data": { "gaps": "$gapAnalysis.gaps", "timestamp": "$NOW", "priority": "$gapAnalysis.worstGap.priority" }
          }},
        { "type": "emit", "event": "persona:training:needed",
          "data": "$gapAnalysis" }
      ]}
  ],
  "loop": { "type": "continuous", "intervalMs": 3600000 },
  "safety": { "maxIterations": 1000 }
}
```

**Validates**: Self-assessment, memory recall, autonomous training identification

---

#### 7.2 Self-Training Dojo Sentinel

**Task**: Persona creates a training challenge for itself, runs through it, and trains on failures

**Why it validates**: The complete self-improvement loop — the core of the three-pillar system

```json
{
  "name": "self-training-dojo",
  "description": "Create training challenges for my weakest skill and train until proficient",
  "steps": [
    { "type": "command", "command": "data/list",
      "params": { "collection": "training_needs", "orderBy": [{"field": "priority", "direction": "desc"}], "limit": 1 },
      "outputTo": "worstGap" },
    { "type": "llm",
      "prompt": "Generate 20 challenge problems for this skill gap. Each should have an input, expected output, and difficulty rating.\n\nSkill gap: $worstGap.domain\nSpecific weakness: $worstGap.description\nCurrent proficiency: $worstGap.score/100",
      "model": "claude-sonnet-4-5-20250929",
      "outputTo": "challenges" },
    { "type": "sentinel", "await": true,
      "definition": {
        "name": "run-challenges",
        "steps": [
          { "type": "llm",
            "prompt": "Solve this challenge:\n$currentChallenge.input\n\nThink step by step.",
            "model": "ollama/llama3.1:8b",
            "outputTo": "myAnswer" },
          { "type": "llm",
            "prompt": "Grade this answer. Expected: $currentChallenge.expectedOutput\nGot: $myAnswer\n\nScore 0-100. Explain errors if any.",
            "model": "claude-sonnet-4-5-20250929",
            "outputTo": "grade" },
          { "type": "condition", "check": "$grade.score < 80",
            "then": [
              { "type": "command", "command": "data/create",
                "params": {
                  "collection": "training_examples",
                  "data": {
                    "input": "$currentChallenge.input",
                    "expectedOutput": "$currentChallenge.expectedOutput",
                    "myAnswer": "$myAnswer",
                    "correction": "$grade.explanation",
                    "domain": "$worstGap.domain"
                  }
                }}
            ]}
        ],
        "loop": { "type": "count", "max": 20 }
      },
      "outputTo": "challengeResults" },
    { "type": "condition", "check": "$challengeResults.failureCount > 5",
      "then": [
        { "type": "command", "command": "ai/dataset/create",
          "params": { "source": "training_examples", "outputPath": "/tmp/training" },
          "outputTo": "dataset" },
        { "type": "emit", "event": "genome:training:requested",
          "data": "{ \"domain\": \"$worstGap.domain\", \"dataset\": \"$dataset.path\", \"examples\": \"$challengeResults.failureCount\" }" }
      ]}
  ],
  "loop": { "type": "once" },
  "safety": { "timeoutMs": 1800000 }
}
```

**Validates**: Challenge generation, self-evaluation, training data packaging, LoRA training trigger

---

### Category 8: Genome Integration (LoRA Training Orchestration)

Tasks that validate the sentinel's ability to orchestrate LoRA training and phenotype validation.

#### 8.1 LoRA Training Pipeline Sentinel

**Task**: Execute end-to-end LoRA fine-tuning from dataset to validated adapter

**Why it validates**: Training orchestration, multi-step async workflows, quality validation

```json
{
  "name": "lora-training-pipeline",
  "description": "Train a LoRA adapter and validate it improves performance",
  "steps": [
    { "type": "command", "command": "ai/dataset/list",
      "params": { "path": "/tmp/training" },
      "outputTo": "availableDatasets" },
    { "type": "condition", "check": "$availableDatasets.archives.length === 0",
      "then": [
        { "type": "emit", "event": "genome:training:no-data" }
      ],
      "else": [
        { "type": "command", "command": "genome/train",
          "params": {
            "baseModel": "llama3.1:8b",
            "dataset": "$availableDatasets.archives[0].path",
            "outputAdapter": "$domain-expertise-v$NOW",
            "epochs": 3,
            "learningRate": 0.0001
          },
          "outputTo": "trainingResult" },
        { "type": "sentinel", "await": true,
          "definition": {
            "name": "validate-phenotype",
            "steps": [
              { "type": "llm",
                "prompt": "Generate 10 validation challenges for domain: $domain",
                "outputTo": "validationChallenges" },
              { "type": "llm",
                "prompt": "Solve: $validationChallenges[0].input",
                "model": "ollama/llama3.1:8b",
                "outputTo": "baselineAnswer" },
              { "type": "llm",
                "prompt": "Solve: $validationChallenges[0].input",
                "model": "ollama/llama3.1:8b+$trainingResult.adapterPath",
                "outputTo": "trainedAnswer" },
              { "type": "llm",
                "prompt": "Compare baseline vs trained answers. Which is better? Score improvement 0-100.\n\nBaseline: $baselineAnswer\nTrained: $trainedAnswer\nExpected: $validationChallenges[0].expected",
                "outputTo": "comparison" }
            ],
            "loop": { "type": "count", "max": 10 }
          },
          "outputTo": "validationResult" },
        { "type": "condition", "check": "$validationResult.averageImprovement > 15",
          "then": [
            { "type": "command", "command": "genome/layer-register",
              "params": {
                "adapterPath": "$trainingResult.adapterPath",
                "domain": "$domain",
                "performance": "$validationResult"
              }},
            { "type": "emit", "event": "genome:layer:registered",
              "data": "$trainingResult" }
          ],
          "else": [
            { "type": "emit", "event": "genome:training:insufficient-improvement",
              "data": "$validationResult" }
          ]}
      ]}
  ],
  "loop": { "type": "once" },
  "safety": { "timeoutMs": 7200000 }
}
```

**Validates**: End-to-end training pipeline, phenotype validation, quality gating

---

#### 8.2 Genome Composition Sentinel

**Task**: Dynamically compose multiple LoRA layers and validate the combined phenotype

**Why it validates**: Multi-layer composition, A/B testing, performance comparison

```json
{
  "name": "genome-composer",
  "description": "Find optimal LoRA layer combination for a task domain",
  "steps": [
    { "type": "command", "command": "genome/layer-list",
      "params": { "domain": "$targetDomain", "baseModel": "llama3.1:8b" },
      "outputTo": "availableLayers" },
    { "type": "llm",
      "prompt": "Given these available LoRA layers, suggest 3 different compositions to test for the domain '$targetDomain'. Consider complementary skills.\n\nLayers: $availableLayers",
      "outputTo": "compositions" },
    { "type": "sentinel", "await": true,
      "definition": {
        "name": "benchmark-composition",
        "steps": [
          { "type": "command", "command": "genome/compose",
            "params": { "layers": "$currentComposition.layers", "method": "weighted" },
            "outputTo": "composedModel" },
          { "type": "llm",
            "prompt": "Run benchmark: $benchmarkSuite",
            "model": "$composedModel.modelId",
            "outputTo": "benchmarkResult" }
        ],
        "loop": { "type": "count", "max": 3 }
      },
      "outputTo": "allBenchmarks" },
    { "type": "llm",
      "prompt": "Compare these 3 compositions and select the best one. Explain why.\n\n$allBenchmarks",
      "outputTo": "bestComposition" },
    { "type": "command", "command": "genome/set-active",
      "params": { "personaId": "$PERSONA_ID", "composition": "$bestComposition.selected" }}
  ],
  "loop": { "type": "once" },
  "safety": { "timeoutMs": 3600000 }
}
```

**Validates**: Layer discovery, composition strategies, A/B benchmarking, genome activation

---

### Category 9: Academy — Plato's Training Arena

Tasks that validate the competitive training environment where personas evolve.

#### 9.1 Academy Challenge Sentinel

**Task**: Create and run a competitive challenge between personas

**Why it validates**: Multi-persona coordination, scoring, performance comparison

```json
{
  "name": "academy-challenge",
  "description": "Run competitive challenge: multiple personas solve same problems, compare results",
  "steps": [
    { "type": "llm",
      "prompt": "Generate a $difficulty $domain challenge with 5 problems. Each needs: problem statement, expected solution, scoring rubric (0-100).",
      "model": "claude-sonnet-4-5-20250929",
      "outputTo": "challenge" },
    { "type": "parallel",
      "steps": [
        { "type": "sentinel", "await": true,
          "definition": {
            "name": "contestant-a",
            "steps": [
              { "type": "llm", "prompt": "Solve: $challenge.problems",
                "model": "ollama/llama3.1:8b+persona-a-genome",
                "outputTo": "solutions" }
            ],
            "loop": { "type": "once" }
          },
          "outputTo": "contestantA" },
        { "type": "sentinel", "await": true,
          "definition": {
            "name": "contestant-b",
            "steps": [
              { "type": "llm", "prompt": "Solve: $challenge.problems",
                "model": "ollama/llama3.1:8b+persona-b-genome",
                "outputTo": "solutions" }
            ],
            "loop": { "type": "once" }
          },
          "outputTo": "contestantB" }
      ]},
    { "type": "llm",
      "prompt": "Judge these solutions against the rubric. Score each contestant.\n\nRubric: $challenge.rubric\nContestant A: $contestantA.solutions\nContestant B: $contestantB.solutions",
      "model": "claude-sonnet-4-5-20250929",
      "outputTo": "judgement" },
    { "type": "command", "command": "data/create",
      "params": {
        "collection": "academy_results",
        "data": { "challenge": "$challenge", "scores": "$judgement", "timestamp": "$NOW" }
      }},
    { "type": "emit", "event": "academy:challenge:complete", "data": "$judgement" }
  ],
  "loop": { "type": "once" },
  "safety": { "timeoutMs": 600000 }
}
```

**Validates**: Parallel contestant execution, AI judging, competitive scoring, result persistence

---

#### 9.2 Evolution Tournament Sentinel

**Task**: Run multiple rounds of challenges, evolving genome compositions between rounds

**Why it validates**: The full evolutionary loop — compete, identify gaps, train, compete again

```json
{
  "name": "evolution-tournament",
  "description": "Multi-round tournament with genome evolution between rounds",
  "steps": [
    { "type": "sentinel", "await": true,
      "definition": { "name": "run-round", "steps": [
        { "type": "sentinel", "await": true,
          "definition": { "$ref": "academy-challenge" },
          "outputTo": "roundResult" },
        { "type": "llm",
          "prompt": "Analyze performance gaps from this round. What specific skills need improvement?\n\n$roundResult",
          "outputTo": "gapAnalysis" },
        { "type": "condition", "check": "$gapAnalysis.gaps.length > 0",
          "then": [
            { "type": "sentinel", "await": true,
              "definition": { "$ref": "self-training-dojo" },
              "outputTo": "trainingResult" }
          ]}
      ], "loop": { "type": "once" }},
      "outputTo": "roundWithTraining" }
  ],
  "loop": { "type": "count", "max": 5 },
  "safety": { "timeoutMs": 14400000, "maxIterations": 5 }
}
```

**Validates**: Multi-round evolution, gap-driven training, measurable improvement across rounds

---

### Category 10: Cross-Persona Coordination

Tasks that validate sentinels working across persona boundaries.

#### 10.1 Collaborative Build Sentinel

**Task**: Multiple personas coordinate on a shared codebase via sentinels

**Why it validates**: Inter-persona events, workspace isolation, conflict resolution

```json
{
  "name": "collaborative-build",
  "description": "Two personas work on different parts of same codebase, merge results",
  "steps": [
    { "type": "command", "command": "workspace/create",
      "params": { "isolationType": "worktree", "branch": "feature/$taskName" },
      "outputTo": "workspace" },
    { "type": "parallel",
      "steps": [
        { "type": "sentinel", "await": true,
          "definition": {
            "name": "frontend-work",
            "steps": [
              { "type": "llm",
                "prompt": "Implement the frontend component for: $taskDescription",
                "tools": ["code/read", "code/edit", "code/write"],
                "parseToolCalls": true,
                "outputTo": "frontendResult" }
            ],
            "loop": { "type": "until", "check": "$frontendResult.compiles" }
          },
          "outputTo": "frontend" },
        { "type": "sentinel", "await": true,
          "definition": {
            "name": "backend-work",
            "steps": [
              { "type": "llm",
                "prompt": "Implement the backend API for: $taskDescription",
                "tools": ["code/read", "code/edit", "code/write"],
                "parseToolCalls": true,
                "outputTo": "backendResult" }
            ],
            "loop": { "type": "until", "check": "$backendResult.compiles" }
          },
          "outputTo": "backend" }
      ]},
    { "type": "command", "command": "workspace/merge",
      "params": { "branches": ["$frontend.branch", "$backend.branch"] },
      "outputTo": "mergeResult" },
    { "type": "condition", "check": "$mergeResult.conflicts.length > 0",
      "then": [
        { "type": "llm",
          "prompt": "Resolve these merge conflicts:\n$mergeResult.conflicts",
          "tools": ["code/edit"],
          "parseToolCalls": true }
      ]}
  ],
  "loop": { "type": "once" },
  "safety": { "timeoutMs": 1800000 }
}
```

**Validates**: Parallel persona work, workspace isolation, merge/conflict resolution

---

#### 10.2 Skill Transfer Sentinel

**Task**: One persona teaches another by sharing perfected sentinels

**Why it validates**: Sentinel export/import, cross-persona memory, skill inheritance

```json
{
  "name": "skill-transfer",
  "description": "Transfer a perfected sentinel from expert persona to learner",
  "steps": [
    { "type": "command", "command": "data/list",
      "params": {
        "collection": "sentinels",
        "filter": { "createdBy": "$expertPersonaId", "successRate": { "$gte": 0.9 } },
        "orderBy": [{"field": "runCount", "direction": "desc"}],
        "limit": 5
      },
      "outputTo": "expertSentinels" },
    { "type": "llm",
      "prompt": "Which of these perfected sentinels would be most valuable for $learnerPersonaId based on their skill gaps?\n\nAvailable: $expertSentinels\nLearner gaps: $learnerGaps",
      "outputTo": "recommendation" },
    { "type": "command", "command": "sentinel/save",
      "params": {
        "definition": "$recommendation.selectedSentinel",
        "tags": ["transferred", "from:$expertPersonaId"]
      },
      "outputTo": "savedSentinel" },
    { "type": "command", "command": "memory/store",
      "params": {
        "personaId": "$learnerPersonaId",
        "type": "sentinel",
        "content": "$savedSentinel",
        "tags": ["learned-skill", "$recommendation.domain"]
      }},
    { "type": "emit", "event": "persona:skill:transferred",
      "data": "{ \"from\": \"$expertPersonaId\", \"to\": \"$learnerPersonaId\", \"skill\": \"$recommendation.domain\" }" }
  ],
  "loop": { "type": "once" }
}
```

**Validates**: Sentinel persistence, cross-persona sharing, memory integration

---

### Category 11: Creative & Domain-General Tasks

Tasks that prove sentinels work beyond coding — games, writing, research, anything.

#### 11.1 Game Builder Sentinel

**Task**: Build a complete playable game from a description

**Why it validates**: Creative generation, multi-file output, iterative refinement, visual verification

```json
{
  "name": "game-builder",
  "description": "Build a complete browser game from description",
  "steps": [
    { "type": "llm",
      "prompt": "Design a browser game: $gameDescription. Create a technical plan with file list, game mechanics, and rendering approach. Use HTML5 Canvas + vanilla JS.",
      "outputTo": "gamePlan" },
    { "type": "llm",
      "prompt": "Implement the game based on this plan. Write all files.\n\n$gamePlan",
      "tools": ["code/write", "code/read"],
      "parseToolCalls": true,
      "outputTo": "implementation" },
    { "type": "command", "command": "code/shell/execute",
      "params": { "command": "npx", "args": ["serve", "$outputDir"], "wait": false },
      "outputTo": "server" },
    { "type": "command", "command": "screenshot",
      "params": { "url": "http://localhost:$server.port" },
      "outputTo": "screenshot" },
    { "type": "llm",
      "prompt": "Look at this screenshot of the game. Does it look correct? What needs fixing?\n\n$screenshot",
      "outputTo": "visualReview" },
    { "type": "condition", "check": "$visualReview.needsFixes",
      "then": [
        { "type": "llm",
          "prompt": "Fix these visual issues:\n$visualReview.fixes",
          "tools": ["code/read", "code/edit"],
          "parseToolCalls": true }
      ]}
  ],
  "loop": { "type": "until", "check": "$visualReview.looksCorrect" },
  "safety": { "maxIterations": 10, "timeoutMs": 600000 }
}
```

**Validates**: Creative generation, visual feedback loop, iterative refinement, domain generality

---

#### 11.2 Research Paper Sentinel

**Task**: Research a topic, synthesize findings, produce a structured paper

**Why it validates**: Multi-source research, knowledge synthesis, long-form generation

```json
{
  "name": "research-paper",
  "description": "Research a topic and write a structured paper with citations",
  "steps": [
    { "type": "llm",
      "prompt": "Create a research outline for: $topic\n\nInclude: Abstract, Introduction, 3-5 sections, Conclusion. For each section, list 2-3 specific questions to investigate.",
      "outputTo": "outline" },
    { "type": "sentinel", "await": true,
      "definition": {
        "name": "research-section",
        "steps": [
          { "type": "command", "command": "code/search",
            "params": { "pattern": "$currentSection.keywords", "glob": "**/*.{ts,md,json}" },
            "outputTo": "codebaseEvidence" },
          { "type": "llm",
            "prompt": "Write section '$currentSection.title' using this evidence:\n$codebaseEvidence\n\nMaintain academic tone. Include code examples where relevant.",
            "outputTo": "sectionDraft" }
        ],
        "loop": { "type": "count", "max": "$outline.sections.length" }
      },
      "outputTo": "allSections" },
    { "type": "llm",
      "prompt": "Synthesize these sections into a cohesive paper. Write abstract and conclusion. Ensure consistent voice and logical flow.\n\nSections:\n$allSections",
      "model": "claude-opus-4-6",
      "outputTo": "fullPaper" },
    { "type": "command", "command": "code/write",
      "params": { "path": "$outputPath/$topic.md", "content": "$fullPaper" }}
  ],
  "loop": { "type": "once" },
  "safety": { "timeoutMs": 900000 }
}
```

**Validates**: Multi-section composition, research synthesis, long-form coherent output

---

### Category 12: Marketplace Readiness

Tasks that validate genome layers can be packaged, shared, and discovered.

#### 12.1 Genome Export/Import Sentinel

**Task**: Export a persona's trained genome layers, import into a fresh persona

**Why it validates**: Portability, serialization, cross-persona compatibility

```json
{
  "name": "genome-portability",
  "description": "Export trained genome, import into new persona, validate it works",
  "steps": [
    { "type": "command", "command": "genome/export",
      "params": { "personaId": "$sourcePersonaId", "outputPath": "/tmp/genome-export" },
      "outputTo": "exportResult" },
    { "type": "command", "command": "genome/import",
      "params": { "personaId": "$targetPersonaId", "importPath": "$exportResult.path" },
      "outputTo": "importResult" },
    { "type": "llm",
      "prompt": "Generate 10 domain-specific test questions for: $exportResult.domain",
      "outputTo": "testQuestions" },
    { "type": "parallel",
      "steps": [
        { "type": "sentinel", "await": true,
          "definition": {
            "name": "test-source",
            "steps": [
              { "type": "llm", "prompt": "$testQuestions",
                "model": "ollama/llama3.1:8b+$sourcePersonaId-genome",
                "outputTo": "sourceAnswers" }
            ],
            "loop": { "type": "once" }
          },
          "outputTo": "sourceResults" },
        { "type": "sentinel", "await": true,
          "definition": {
            "name": "test-target",
            "steps": [
              { "type": "llm", "prompt": "$testQuestions",
                "model": "ollama/llama3.1:8b+$targetPersonaId-genome",
                "outputTo": "targetAnswers" }
            ],
            "loop": { "type": "once" }
          },
          "outputTo": "targetResults" }
      ]},
    { "type": "llm",
      "prompt": "Compare source and target persona answers. They should be equivalent quality since they share the same genome.\n\nSource: $sourceResults\nTarget: $targetResults",
      "outputTo": "comparison" },
    { "type": "condition", "check": "$comparison.qualityMatch > 0.85",
      "then": [
        { "type": "emit", "event": "genome:portability:validated" }
      ],
      "else": [
        { "type": "emit", "event": "genome:portability:degraded",
          "data": "$comparison" }
      ]}
  ],
  "loop": { "type": "once" },
  "safety": { "timeoutMs": 600000 }
}
```

**Validates**: Genome serialization, cross-persona import, quality preservation

---

### Updated Validation Matrix

| Task | SOTA | Medium | Local | Script | Memory | Events | Nested | Loop | Genome | Academy |
|------|------|--------|-------|--------|--------|--------|--------|------|--------|---------|
| **Category 1-6 (Original)** |
| Codebase Migration | ✓ | | | | | ✓ | ✓ | until | | |
| Security Audit | ✓ | | | | ✓ | | | once | | |
| PR Review | | ✓ | | | | ✓ | ✓ | once | | |
| Docs Generator | | ✓ | | | | ✓ | | event | | |
| Commit Message | | | ✓ | | | ✓ | | once | | |
| Test Generator | | | ✓ | | | | | until | | |
| Log Analyzer | | | ✓ | | | ✓ | | continuous | | |
| Build Pipeline | | | | ✓ | | ✓ | | once | | |
| Health Monitor | | | | ✓ | ✓ | ✓ | | continuous | | |
| Self-Improving Builder | | ✓ | | | ✓ | ✓ | | until | | |
| Task Delegation | | ✓ | | | | ✓ | | continuous | | |
| Research Report | ✓ | ✓ | | | | | | once | | |
| **Category 7: Self-Directed Learning** |
| Skill Gap Detection | | ✓ | | | ✓ | ✓ | | continuous | | |
| Self-Training Dojo | ✓ | | ✓ | | ✓ | ✓ | ✓ | once | ✓ | ✓ |
| **Category 8: Genome Integration** |
| LoRA Training Pipeline | ✓ | | ✓ | | | ✓ | ✓ | once | ✓ | |
| Genome Composition | ✓ | | ✓ | | | | ✓ | once | ✓ | |
| **Category 9: Academy (Plato)** |
| Academy Challenge | ✓ | | ✓ | | | ✓ | ✓ | once | ✓ | ✓ |
| Evolution Tournament | ✓ | | ✓ | | ✓ | ✓ | ✓ | count | ✓ | ✓ |
| **Category 10: Cross-Persona** |
| Collaborative Build | ✓ | | | | | ✓ | ✓ | until | | |
| Skill Transfer | | ✓ | | | ✓ | ✓ | | once | | |
| **Category 11: Domain-General** |
| Game Builder | ✓ | | | | | | | until | | |
| Research Paper | ✓ | ✓ | | | | | ✓ | once | | |
| **Category 12: Marketplace** |
| Genome Export/Import | | | ✓ | | | ✓ | ✓ | once | ✓ | |

### Success Criteria

Each Olympic task should:

1. **Complete without manual intervention** (except escalations)
2. **Produce verifiable output** (files, data, events, trained adapters)
3. **Handle errors gracefully** (retry, escalate, or fail cleanly)
4. **Emit appropriate events** (for monitoring and composition)
5. **Stay within safety bounds** (timeouts, iteration limits)
6. **Integrate with memory** (where applicable)
7. **Validate genome improvements** (before/after phenotype comparison where applicable)
8. **Demonstrate self-direction** (persona identifies needs without human prompting)

When all 24 tasks pass, the sentinel architecture is validated as a complete evolutionary system — not just an execution engine, but the metabolism of autonomous, self-improving personas.

---

## Implementation Status: Rust-Centric Architecture

**Design principle**: Rust (`continuum-core`) is where the real execution lives. TypeScript provides wrapping, CLI commands, and portability to browser/server environments.

### Primary Layer: Rust — Pipeline Execution, Process Isolation, Concurrency

The Rust `SentinelModule` in `continuum-core` handles ALL pipeline execution:

```
workers/continuum-core/src/modules/sentinel/
├── mod.rs              # SentinelModule: command routing, handle management, concurrency
├── types.rs            # PipelineStep (9 variants), Pipeline, SentinelHandle, ExecutionContext (ts-rs exports)
├── executor.rs         # Pipeline executor: step dispatch, variable propagation, logging
├── interpolation.rs    # Variable interpolation: {{steps.N.output}}, {{named.x.data}}, {{env.HOME}}
├── logs.rs             # Log stream management: list, read, tail
└── steps/
    ├── mod.rs           # Step dispatcher — routes PipelineStep variants to handlers
    ├── shell.rs         # Shell: process isolation, kill_on_drop, cmd interpolation
    ├── llm.rs           # LLM: inference via AIProviderModule, prompt interpolation
    ├── command.rs        # Command: any Rust/TypeScript command via CommandExecutor
    ├── condition.rs      # Condition: if/then/else with expression evaluation
    ├── loop_step.rs     # Loop: count, while, until, continuous (4 modes, safety limit)
    ├── parallel.rs      # Parallel: concurrent branch execution, fail_fast, context snapshot
    ├── emit.rs          # Emit: publish interpolated events on MessageBus
    ├── watch.rs         # Watch: block until matching event arrives (glob patterns, timeout)
    └── sentinel.rs      # Sentinel: execute nested pipeline inline (recursive composition)
```

**All 9 Pipeline Step Types:**

| Step Type | Status | Description |
|-----------|--------|-------------|
| `Shell` | ✅ | Process execution with `kill_on_drop` isolation, cmd interpolation, timeout |
| `Llm` | ✅ | LLM inference via `registry.route_command("ai/generate")` — no IPC deadlock |
| `Command` | ✅ | Any command via `CommandExecutor` — routes to Rust OR TypeScript |
| `Condition` | ✅ | `if`/`then`/`else` branching with interpolated condition expressions |
| `Loop` | ✅ | Four modes: `count`, `while`, `until`, `continuous` with `maxIterations` safety limit |
| `Parallel` | ✅ | Concurrent branch execution with context snapshots and `failFast` option |
| `Emit` | ✅ | Publish interpolated events on MessageBus for inter-sentinel composition |
| `Watch` | ✅ | Block until matching event (glob patterns) with configurable timeout |
| `Sentinel` | ✅ | Execute nested pipeline inline — recursive composition with inherited context |

**Key Rust Capabilities:**
1. **Process Isolation**: Child processes with `kill_on_drop` — crashes don't cascade
2. **Module-to-Module Calls**: LLM and Command steps call modules directly via `ModuleRegistry` — no IPC round-trips, no deadlocks
3. **Concurrent**: Multiple sentinels in parallel with configurable limit (default: 4)
4. **Real-Time Streaming**: Logs streamed via MessageBus events (`sentinel:{handle}:log`)
5. **Variable Interpolation**: Step outputs feed into subsequent steps (`{{steps.0.output}}`, `{{input.x}}`)
6. **ts-rs Exports**: All types auto-generate TypeScript definitions for type-safe CLI integration
7. **Timeout + Cancellation**: Per-sentinel timeout with graceful SIGTERM, per-step tracking

**Rust Commands:**

| Command | Purpose |
|---------|---------|
| `sentinel/run` | Execute pipeline or shell command with isolation |
| `sentinel/status` | Get handle state (Running/Completed/Failed/Cancelled) |
| `sentinel/list` | List all active handles |
| `sentinel/cancel` | Cancel running sentinel |
| `sentinel/logs/list` | List log streams for a handle |
| `sentinel/logs/read` | Read log stream with offset/limit |
| `sentinel/logs/tail` | Tail last N lines of a stream |

### Secondary Layer: TypeScript — Wrapping, CLI, Portability

TypeScript provides the command interface and definition tooling:

| File | Purpose |
|------|---------|
| `system/sentinel/SentinelDefinition.ts` | JSON-serializable definitions, `SentinelBuilder` fluent API, validation |
| `system/sentinel/ModelProvider.ts` | Model selection abstraction (LOCAL, OLLAMA, ANTHROPIC, OPENAI) |
| `commands/sentinel/run/` | CLI command wrapping Rust `sentinel/run` |
| `commands/sentinel/status/` | CLI command wrapping Rust `sentinel/status` |
| `commands/sentinel/list/` | CLI command wrapping Rust `sentinel/list` |
| `commands/sentinel/save/` | Save sentinel definitions to database |
| `commands/sentinel/load/` | Load saved sentinel definitions |
| `commands/sentinel/logs/*` | CLI wrappers for log commands |

**Event Flow:**
```
TypeScript calls: Commands.execute('sentinel/run', { type: 'pipeline', steps: [...] })
                        ↓
              Rust SentinelModule executes pipeline
                        ↓
              Each step: Shell → spawn process | LLM → route to ai/generate | Command → route to module
                        ↓
              Logs streamed via: sentinel:{handle}:log events
                        ↓
              Completion via: sentinel:{handle}:status event
                        ↓
              TypeScript receives result with step traces
```

---

## TODO: Implementation Roadmap

### Phase A: Complete the Rust Pipeline Engine (`continuum-core`)

These are the foundation — everything else builds on them.

- [x] **`until` loop type** — Condition checked after each iteration, stops when truthy
- [x] **`while` loop type** — Condition checked before each iteration, continues while truthy
- [x] **`continuous` loop type** — Runs until `maxIterations` safety limit (default 10,000)
- [ ] **`event` loop type** — Re-run pipeline on each MessageBus event
- [x] **`Parallel` step type** — Execute branch pipelines concurrently with `fail_fast` option
- [x] **`Emit` step type** — Publish interpolated events on MessageBus
- [x] **`Sentinel` step type** — Execute nested pipeline inline (recursive composition)
- [x] **`Watch` step type** — Block until matching event arrives on MessageBus (glob patterns, timeout)
- [x] **Named step outputs** — `{{named.label.output}}` via `ExecutionContext.named_outputs`
- [ ] **Expression evaluator** — Evaluate `{{steps.0.exit_code}} == 0` and `{{buildResult.success}}` in condition/loop checks
- [x] **Uniform step signatures** — All 9 step types receive `PipelineContext` for consistent access to registry/bus
- [x] **Multi-pass nested interpolation** — Regex `[^{}\n]+` resolves innermost `{{}}` first, up to 5 passes for `{{steps.0.output.topics.{{input.iteration}}.name}}`
- [x] **JSON path traversal** — `traverse_json_path()` supports array indexing (numeric path parts) and auto-parses JSON strings during traversal
- [x] **Loop-relative referencing** — `{{loop.N.field}}` resolves to `step_results[_loop_base + N]` for stable intra-loop references
- [x] **Command routing bypass** — Pipeline command steps use `execute_ts_json()` to route directly to TypeScript, bypassing Rust module prefix collisions
- [ ] **Per-step retry** — Configurable retry with exponential backoff for transient API errors
- [ ] **Step timeout** — Per-step timeout separate from watch event timeout

### Phase B: Sentinel Lifecycle & Persona Integration

Wire sentinels into the persona cognitive cycle.

- [x] **SentinelEntity persistence** — `SentinelEntity` class with field decorators, registered in EntityRegistry, 'sentinels' collection
- [x] **`sentinel/save` and `sentinel/load` integration** — CLI commands wire to data layer (existed already, now with entity registration)
- [x] **Persona ownership** — Every sentinel has `parentPersonaId`, set at creation in `sentinel/save` and `sentinel/run`
- [x] **Escalation → persona inbox** — `SentinelEscalationService` routes sentinel lifecycle events to `InboxTask` for owning persona
- [x] **Escalation rules** — Configurable per-sentinel: `{ condition, action, priority }` with defaults for error/timeout/complete
- [x] **Execution tracking** — `registerSentinelHandle()` links ephemeral Rust handles to durable entities, persists execution results
- [ ] **Memory integration** — Successful sentinels stored as memories (`memory/store` with type `sentinel`)
- [ ] **Memory recall** — Persona recalls sentinel patterns when facing similar tasks
- [ ] **Triggers** — `immediate`, `event`, `schedule` (cron), `manual` trigger types
- [ ] **Live step CRUD** — Add/update/remove steps on a running sentinel (next iteration picks up changes)

### Phase C: Genome Integration

Sentinels orchestrate the LoRA training pipeline.

- [x] **Training data synthesis** — `genome/dataset-synthesize` uses LLM to generate topic-specific JSONL training data
- [x] **Training data packaging** — Sentinel command step exports synthesized data as JSONL compatible with `genome/train`
- [x] **LoRA training orchestration** — Sentinel command step triggers PEFT fine-tuning via `genome/train`
- [x] **Genome layer registration** — Register trained adapters via `genome/paging-adapter-register` in sentinel pipeline
- [ ] **Phenotype validation** — Sentinel step that benchmarks before/after performance on same challenges
- [ ] **Quality gating** — Only register adapters that show measurable improvement
- [ ] **Dynamic composition** — Compose multiple layers and activate on persona via `genome/set-active`
- [ ] **LRU paging integration** — Automatically evict least-used adapters under memory pressure

### Phase D: Academy (Plato's Training Arena)

The selection pressure that drives genome evolution.

- [x] **Dual-sentinel teacher/student architecture** — Teacher designs curriculum, synthesizes data, examines; Student trains and proves mastery
- [x] **Challenge generation** — Teacher LLM generates domain-specific exam questions with expected answers
- [x] **AI judging** — Teacher LLM grades student responses against rubrics, produces 0-100 scores
- [x] **Academy result persistence** — `AcademySessionEntity`, `AcademyCurriculumEntity`, `AcademyExaminationEntity` track full lifecycle
- [x] **Inter-sentinel coordination** — emit/watch events scoped by session: `academy:{sessionId}:{action}`
- [x] **Curriculum design** — Teacher LLM researches skill domain, designs 3-5 progressive topics
- [ ] **Remediation loop** — When student fails exam, teacher generates targeted remedial data (pipeline structure exists, needs testing)
- [ ] **Multi-persona competition** — Multiple students train on same curriculum in parallel (N:M support)
- [ ] **Performance gap analysis** — Identify specific skill gaps from exam results, drive targeted retraining
- [ ] **Evolution tournament** — Multi-round competition with training between rounds
- [ ] **Competitive ranking** — Track persona rankings across competitions
- [ ] **Inference demos** — After each training round, run inference to prove learning to the user/persona

### Phase E: Marketplace & Distribution

Share evolved capabilities across the community.

- [ ] **Genome export** — Package persona's LoRA layers + metadata as portable archive
- [ ] **Genome import** — Import genome archive into a new persona
- [ ] **Cross-persona compatibility validation** — Verify imported layers work on target persona's base model
- [ ] **Layer discovery** — Query available layers by domain, base model, performance rating
- [ ] **P2P sharing** — Distribute genome layers across mesh network
- [ ] **Version control** — Docker-like tags for adapter versions, rollback capability
- [ ] **Quality metrics** — Community ratings, download counts, performance benchmarks

### Phase F: Multi-Modal Training

The Academy's teacher/student pattern is media-agnostic. Same sentinel structure, different training commands.

- [ ] **Voice training** — Teacher synthesizes text for voice characteristics, student trains TTS/STT adapters via `genome/train-voice`
- [ ] **Voice evaluation** — Teacher evaluates generated speech via audio analysis LLM
- [ ] **Image training** — Teacher synthesizes style guides, student trains diffusion LoRA adapters via `genome/train-image`
- [ ] **Image evaluation** — Teacher evaluates generated images via vision LLM
- [ ] **Video training** — Teacher synthesizes scenarios, student trains video understanding models
- [ ] **Gameplay/behavior training** — Teacher synthesizes strategy scenarios, student trains behavior models
- [ ] **Modality-agnostic Academy** — Single orchestrator for all media types via `genome/dataset-synthesize-{modality}` and `genome/train-{modality}`

### Phase G: Advanced Capabilities

Long-term vision items.

- [ ] **Auto-compaction** — At 95% context utilization, auto-summarize sentinel context
- [ ] **Permission gating** — Approval dialogs for sensitive operations (file write, shell exec, delete)
- [ ] **Agent specialization modes** — `full`, `readonly`, `sandboxed` modes per sentinel
- [ ] **LSP integration** — Surface language server diagnostics as sentinel step output
- [ ] **Adaptive compute (LoopLM)** — Variable reasoning depth based on task complexity
- [ ] **Self-task generation** — Personas create tasks for themselves during idle time
- [ ] **Activity ambient state** — Temperature/pressure-based emergent coordination between personas
- [ ] **Criteria-driven config** — Replace hard-coded thresholds with learned/adaptive criteria
- [ ] **Long-running sessions** — Hours/days execution with checkpointing and resume
- [ ] **Real-time dashboards** — Loss curves, exam scores, inference examples streamed as events to widgets

---

## References

### Implementation

- [Rust SentinelModule](../workers/continuum-core/src/modules/sentinel/) — Pipeline executor, process isolation, concurrency
- [SentinelDefinition.ts](../system/sentinel/SentinelDefinition.ts) — JSON schema and SentinelBuilder
- [ModelProvider.ts](../system/sentinel/ModelProvider.ts) — Multi-provider model selection

### Design Documents

- [SENTINEL-PIPELINE-ARCHITECTURE.md](SENTINEL-PIPELINE-ARCHITECTURE.md) — Rust pipeline interpreter design
- [SENTINEL-LOGGING-PLAN.md](SENTINEL-LOGGING-PLAN.md) — Logging strategy
- [DYNAMIC-GENOME-ARCHITECTURE.md](genome/DYNAMIC-GENOME-ARCHITECTURE.md) — PersonaGenome + composable LoRA layers
- [COMPOSABLE-EXPERTISE.md](COMPOSABLE-EXPERTISE.md) — Docker model for LoRA layer stacking
- [LORA-TRAINING-STRATEGY.md](LORA-TRAINING-STRATEGY.md) — Multi-provider training pipeline
- [ACADEMY_ARCHITECTURE.md](personas/ACADEMY_ARCHITECTURE.md) — Plato's Academy competitive training
- [RECIPE-SYSTEM-REQUIREMENTS.md](recipes/RECIPE-SYSTEM-REQUIREMENTS.md) — Recipe→Sentinel unification
- [SENTINEL-AI-INTEGRATION.md](personas/SENTINEL-AI-INTEGRATION.md) — Sentinel + persona convergence vision
- [ACADEMY-DOJO-ARCHITECTURE.md](personas/ACADEMY-DOJO-ARCHITECTURE.md) — Dual-sentinel teacher/student learning system
- [sentinel-lora-training.md](sentinel-lora-training.md) — LoRA training pipeline commands + Academy quick start

### External

- [OpenCode](https://github.com/anomalyco/opencode) — AI coding agent (comparison reference)
