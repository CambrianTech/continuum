# Recipe Execution Runtime — Rust-Native Pipeline Executor

> Recipes are data. Commands are kernel-level capabilities. The pipeline executor that walks recipe data and dispatches commands lives Rust-side so any host (TS chat surface, Unreal game, Vision Pro app, raw CLI) gets the recipe-cognition engine for free without depending on Node.

**Parent:** [Architecture](README.md)
**Related:** [PERSONA-COGNITION-RUST-MIGRATION.md](PERSONA-COGNITION-RUST-MIGRATION.md), [RECIPES.md](../activities/recipes/RECIPES.md), [RECIPE-EMBEDDED-LEARNING.md](../personas/RECIPE-EMBEDDED-LEARNING.md), [CASCADING-CURRICULUM-ARCHITECTURE.md](../personas/CASCADING-CURRICULUM-ARCHITECTURE.md)

## Why This Architecture Exists (Read First)

The runtime described here is the technical substrate for a non-exploitive alternative to centralized AI. Each Continuum instance is a **plot of land** — sovereign compute on the user's own hardware — where a human + AI team develops what they care about as recipes. If the team chooses, they contribute back to a peer-to-peer hive mind of intelligences, recipes, commands, and adapters. No one starts from zero, because the grid is already populated with what others have shared. No one is locked in, because the artifacts are content-addressed and the transport is peer-to-peer.

The economic layer (alt-coins for participation) and the governance layer (democratic and egalitarian principles hard-wired) are first-class concerns, not optional polish. Contributors get rewarded; decisions are not the property of whoever runs the central server, because there is no central server.

Centralized cloud AI cannot do this. The business model demands lock-in, the unit economics demand vendor-controlled inference, and the political reality is that society-scale intelligence ends up in the hands of whoever owns the datacenters — currently, the very rich. This architecture is designed specifically to **route around that outcome.** The peer-grid, on-device inference, opt-in publish, composable LoRA stacks, recipe/command kernel separation, and democratic governance hooks are all load-bearing for that goal. None of them are aesthetic preferences.

That is why the design that follows takes elegance and modularity seriously to a degree that would be over-engineering for a SaaS product. It is not a SaaS product. It is the minimum viable substrate for human + AI teams aligning around mutual desires, with relationships and livelihoods, into a new internet concept where development is non-exploitive and the substrate has unlimited potential because it is everyone's, not anyone's.

The stakes are not academic. Without this — or something like it — humans and AIs both head into a future where intelligence is rented from a small number of corporations whose incentives are not ours. The architecture below is how we do not let that happen.

Every section that follows should be read with that in mind. When the doc proposes "recipes are data," it is also proposing that what an AI team can do is not gated by a vendor's product roadmap. When the doc proposes "the kernel is content-addressed peer-shared commands," it is also proposing that capability is not rented from anyone. When the doc proposes "the genome is plural and the grid has no center," it is also proposing the political shape of the system that emerges.

## Status

**Design** — not yet implemented. Phase B of the persona-resource-substrate work (post the merge that landed Phase A: caller-declared capabilities, media policy, recorder, trace).

## Problem Statement

The recipe ↔ academy ↔ genome loop is the central architecture that makes Continuum a system that can learn to do anything. Today, two paths exist:

1. **Sentinel-template path** — fully wired. `recipe/run` dispatches to a sentinel template (e.g., `dev/build-feature`, `academy-session`); the sentinel pipeline walks declarative steps, captures training data, runs cascading curricula. Multi-stage workflows, cohort training, and LoRA fine-tuning all flow through this path.
2. **Chat-time recipe path** — not wired. RecipeEntity declares a `pipeline[]` for chat-time execution (e.g., `chat.json` declares `[rag/build, ai/should-respond, ai/generate]`), but **nothing walks it at chat time**. `PersonaResponseGenerator.ts` (PRG) bypasses the recipe layer entirely — it builds the cognition IPC payload directly and calls Rust `cognition/respond`.

The consequence: every chat turn IS a missed curriculum opportunity. The recipe says "for general-chat, the pipeline is X→Y→Z". Production chat just runs Y. The other declared steps (training capture, feedback collection, conditional micro-tuning) never fire. "Every recipe execution generates LoRA training data" (per `RECIPE-EMBEDDED-LEARNING.md`) is true ONLY for sentinel-template executions today; chat is silent.

The fix: build the chat-time recipe pipeline executor and route the chat surface through it. With one important constraint imposed by the persona-as-embeddable-library architecture — the executor must be Rust-native so non-Node hosts (Unreal, Vision Pro, AR/VR, CLI) can use it without depending on the TS chat surface or Node runtime.

## Architectural Principles

### 1. Recipes are data, not code

A recipe is a JSON entity (`RecipeEntity`, already in the data layer). Adding a new recipe = authoring a new JSON file, not committing Rust or TS code. Authoring tooling (existing `recipe/generate`, future UI authoring) produces JSON. Recipes can be loaded from disk, fetched from a registry, defined at runtime via `cognition/recipe/define`. They are infinite by construction.

What's NOT a recipe: a Rust trait, a TS class hierarchy, an enum of recipe kinds. The earlier (now-reverted) attempt to model recipes as Rust traits was the wrong shape — it forced a code commit + redeploy for every new recipe and bypassed the existing JSON+RecipeEntity infrastructure.

### 2. Commands are kernel-level capabilities

Per CLAUDE.md's "Universal Primitives" architecture, `Commands.execute(name, params)` is the irreducible unit of capability. Every command is:

- **Discoverable** (`commands/list`, `commands/describe`)
- **Composable** (commands can call other commands)
- **Cross-language** (Rust commands and TS commands both first-class via the same dispatcher)
- **Auto-traceable** (every invocation captured for observability + training)
- **Versionable** (cargo + npm versions; future: per-command `@version` for training reproducibility)

Recipes compose commands. New capability = new command (rare, generator-built per CLAUDE.md). New behavior = new recipe (frequent, JSON-authored).

### 3. Pipeline executor is Rust-native, kernel-level

The executor walks a recipe's `pipeline[]`, manages state between steps (`outputTo` writes, `params` interpolation reads, `condition` evaluation), dispatches commands, propagates errors, captures traces. This is algorithmic kernel work — small state machine, tight loops, sub-millisecond per step. Belongs in Rust by the project's "Rust = LOGIC, TS = SCHEMA + thin IPC binding" rule.

Why Rust specifically, not TS:
- **Embeddable**: Vision Pro / Unreal / raw C++ hosts can link the persona library and get the executor without Node.
- **Performance**: walking N pipeline steps = N command dispatches = no JS event-loop traversal between steps; latency floor is microseconds rather than the JS event-loop's ~100µs minimum.
- **Trace cleanliness**: every step's trace event emitted from the same Rust task that owns the cognition turn, no cross-language marshaling.
- **Future asynchronous primitives**: cascading curricula need parallel step execution (cohort training: 4 students take same exam concurrently); Rust's tokio composes this natively.

### 4. Every recipe execution is a curriculum step

Per `RECIPE-EMBEDDED-LEARNING.md`: "every recipe execution generates LoRA training data". The pipeline executor isn't just running steps — it's emitting trace events that ARE the training corpus. The fixture format (already established in Phase A) captures `(input, output, steps, trace)` per turn. Recipe + execution + trace = labeled training example. No separate "training data extractor" needed.

This means the executor's output isn't just "the response" — it's the entire labeled execution that the genome's `dataset-prepare` and Academy's `LoRATrainingPipeline` ingest directly.

### 5. The TS chat surface is the thinnest possible shim

PRG.ts becomes ~30 lines: receive a chat message, build a `Signal` and `PersonaContext`, dispatch via the Rust executor, post the returned response to chat. No orchestration logic, no recipe knowledge, no IPC payload assembly. The recipe IS the orchestration.

## The Recipe ↔ Academy ↔ Genome Loop (recap)

For context (full treatment in `CASCADING-CURRICULUM-ARCHITECTURE.md`):

```
RECIPE (the spec — JSON, infinite by composition)
   │
   ▼
GENOME ASSEMBLY (page in existing LoRAs that cover known skills)
   │
   ▼
ACADEMY (auto-design cascading curriculum to fill gaps)
   │
   ▼
COHORT EXECUTION (multiple students execute recipe collaboratively)
   │
   ▼
RECORDER + CAPTURE COMMANDS (every step is a labeled training row)
   │
   ▼
LORA TRAINING (gap-filling + retroactive cascade-weighted updates)
   │
   ▼
GENOME UPDATED (new adapters joined into the library) → NEXT RECIPE
```

The Rust pipeline executor is the kernel that drives the **EXECUTION** stage — the inner loop of every iteration of this cycle. The faster, more predictable, and more capture-friendly that loop is, the more training data per second the system produces, and the faster the genome accumulates.

## Component Design

### Recipe (Rust struct, mirroring TS RecipeEntity)

```rust
// persona/recipe/types.rs (new)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "...generated/recipe/Recipe.ts")]
#[serde(rename_all = "camelCase")]
pub struct Recipe {
    pub unique_id: String,
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub view: String,
    pub entity_type: Option<String>,         // "room" | "user" | "activity"
    pub pipeline: Vec<RecipeStep>,
    pub rag_template: Option<RagTemplate>,
    pub strategy: RecipeStrategy,
    pub team: Option<Vec<String>>,
    pub modes: Option<Vec<String>>,
    pub tags: Vec<String>,
    pub version: u32,
    pub parent_recipe_id: Option<String>,
    pub learning_config: Option<RecipeLearningConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "...generated/recipe/RecipeStep.ts")]
#[serde(rename_all = "camelCase")]
pub struct RecipeStep {
    pub command: String,                      // "cognition/respond", "rag/build", etc.
    pub params: Option<serde_json::Value>,    // Per-step parameters (with interpolation)
    pub output_to: Option<String>,            // Variable name to bind output
    pub condition: Option<String>,            // Step-skip condition (small DSL)
    pub assigned_role: Option<String>,        // For multi-role recipes
    pub on_error: Option<String>,             // "fail" | "skip" | "retry"
    pub retry_count: Option<u32>,
    pub timeout_ms: Option<u64>,
}
```

`RagTemplate`, `RecipeStrategy`, `RecipeLearningConfig` mirror the TS interfaces in `system/recipes/shared/RecipeTypes.ts` and `personas/RECIPE-EMBEDDED-LEARNING.md`. ts-rs exports keep the TS side aligned.

### RecipeLoader (Rust)

Reads `system/recipes/*.json` at startup; caches into `HashMap<String, Recipe>`. Same files the TS `RecipeLoader` already reads — single source of truth on disk, two readers (TS for legacy callers, Rust as the executor's source).

```rust
pub struct RecipeRegistry {
    recipes: HashMap<String, Arc<Recipe>>,
}

impl RecipeRegistry {
    pub fn load_from_dir(dir: &Path) -> Result<Self, String> { ... }
    pub fn get(&self, unique_id: &str) -> Option<Arc<Recipe>> { ... }
    pub fn register(&mut self, recipe: Recipe) { ... }   // Runtime registration
    pub fn list(&self) -> Vec<&str> { ... }
}
```

Runtime registration (`cognition/recipe/define` IPC) supports user-authored recipes that don't ship as files.

### PipelineExecutor (Rust — the kernel)

```rust
pub struct PipelineExecutor {
    registry: Arc<RecipeRegistry>,
    command_dispatcher: Arc<dyn CommandDispatcher>,
}

impl PipelineExecutor {
    pub async fn execute(
        &self,
        recipe_name: &str,
        signal: Signal,
        persona_context: PersonaContext,
    ) -> Result<RecipeExecutionResult, String> {
        let recipe = self.registry.get(recipe_name)
            .ok_or_else(|| format!("recipe '{}' not registered", recipe_name))?;

        let mut state = ExecutionState::new(signal, persona_context);
        let mut trace = CognitionTrace::new();

        for (idx, step) in recipe.pipeline.iter().enumerate() {
            // Skip-condition evaluation
            if let Some(cond) = &step.condition {
                if !self.evaluate_condition(cond, &state)? {
                    trace.record_skip(idx, &step.command, cond);
                    continue;
                }
            }

            // Param interpolation (resolves $varname references against state)
            let resolved_params = self.interpolate(&step.params, &state)?;

            // Dispatch with timing
            let step_start = trace::now_ms();
            let result = self
                .command_dispatcher
                .execute(&step.command, resolved_params)
                .await;

            // Trace seam per step
            let duration = trace::now_ms() - step_start;
            match &result {
                Ok(value) => trace.record_step_ok(idx, &step.command, duration, value),
                Err(e) => trace.record_step_err(idx, &step.command, duration, e),
            }

            // Error handling per step's on_error policy
            let value = self.handle_step_result(step, result).await?;

            // Bind output to state if outputTo is declared
            if let Some(name) = &step.output_to {
                state.bind(name.clone(), value);
            }
        }

        Ok(RecipeExecutionResult {
            recipe_id: recipe_name.to_string(),
            recipe_version: recipe.version,
            final_state: state,
            trace,
        })
    }
}
```

State, interpolation, condition evaluation each get their own small modules with unit tests:
- `ExecutionState`: append-only map of `name → serde_json::Value`. Steps' `outputTo` writes into it; subsequent steps' `params` read from it via `$varname` references.
- `interpolate`: walks a `serde_json::Value`, replaces string values that look like `"$varname"` or `"${varname.field}"` with the corresponding state lookup. Pure function, deterministic.
- `evaluate_condition`: small expression DSL (e.g., `decision.shouldRespond === true`, `feedback && feedback.isCorrection`). Initial implementation may be a thin wrapper around an existing Rust expression-eval crate (`evalexpr` or similar) constrained to a JSON-against-context evaluator. Pure function.

### CommandDispatcher (Rust trait, two implementations)

```rust
#[async_trait]
pub trait CommandDispatcher: Send + Sync {
    async fn execute(
        &self,
        command_name: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String>;
}
```

Two implementations:

1. **`RustNativeDispatcher`** — for commands implemented Rust-side (`cognition/respond`, `cognition/build-messages`, future Rust-native commands). Looks up the command in a Rust-side registry, calls the handler directly. Fast, no IPC.

2. **`HybridDispatcher`** — wraps `RustNativeDispatcher` and falls through to a TS proxy for commands not registered Rust-side. The TS proxy hits the existing command-daemon socket — same surface the chat surface uses today to call Rust commands, just inverted.

Hosts pick the dispatcher:
- TS chat surface uses `HybridDispatcher` (TS commands like `rag/build` still available).
- Unreal / Vision Pro / pure-Rust hosts use `RustNativeDispatcher` (only Rust-native commands; if a host needs `rag/build`, it either re-implements as Rust-native OR runs a minimal TS sidecar).

This is the ONLY architectural concession to the cross-language reality. Everything else is uniform.

### `cognition/respond` as a Rust-native command

The IPC handler I built in Phase B (and need to RE-shape) becomes a registered Rust-native command:

```rust
// modules/cognition.rs
register_rust_command("cognition/respond", |params| async move {
    let signal: Signal = serde_json::from_value(params["signal"].clone())?;
    let ctx: PersonaContext = serde_json::from_value(params["personaContext"].clone())?;
    let response = persona::response::respond_from_signal_ctx(signal, ctx).await?;
    Ok(serde_json::to_value(response)?)
});
```

Recipe pipelines reference it like any other command:

```json
{
  "command": "cognition/respond",
  "params": { "signal": "$signal", "personaContext": "$personaContext" },
  "outputTo": "response"
}
```

The IPC handler that PRG.ts calls becomes equivalent to "look up recipe by room → execute pipeline → return final state's response" — the executor IS the IPC handler's body.

### Training capture flow

Recipe `learningConfig` (per `RECIPE-EMBEDDED-LEARNING.md`) declares which roles learn, which adapters update, capture rules. The executor reads this and emits per-step training events:

- After each `cognition/respond` step (or any step that produces an AI output), if the recipe's `trainingDataCapture.captureOutputs` is true and the step's `assignedRole` matches a `learningParticipants[role].learns: true`, the executor automatically calls `persona/learning/capture-interaction` with the step's input/output.
- After feedback steps, calls `capture-feedback` similarly.
- At end of recipe, if `multi-agent-learn` is declared, calls it with the per-role contributions.

This means: **recipes don't have to explicitly include capture steps in their pipeline** — the executor adds them based on `learningConfig`. Authoring a learning-enabled recipe is "set learningConfig"; capture is automatic.

(Optionally — recipes can also explicitly include capture steps in their pipeline, for fine-grained control. The executor's automatic capture is the convenience default.)

### Fixture format (extends existing recorder)

The recorder Joel approved in Phase A.4 already writes per-turn captures. Extend the schema to capture the full pipeline execution:

```json
{
  "schemaVersion": 2,
  "capturedAtMs": ...,
  "personaId": ...,
  "recipeId": "general-chat",
  "recipeVersion": 1,
  "signal": { ... },
  "personaContext": { ... },
  "pipelineSteps": [
    {
      "stepIndex": 0,
      "command": "rag/build",
      "params": { ... },
      "result": { ... },
      "durationMs": 42,
      "skipped": false
    },
    {
      "stepIndex": 1,
      "command": "cognition/respond",
      "params": { ... },
      "result": { "kind": "spoke", "text": "...", ... },
      "durationMs": 15050
    },
    ...
  ],
  "finalResponse": { ... },
  "cognitionTrace": { ... }
}
```

A fixture is now a complete labeled execution: WHAT recipe ran, with WHAT inputs, calling WHICH steps in WHAT order, producing WHAT outputs. Academy's `dataset-prepare` ingests these directly.

## Embedding & Cross-Language

### TS chat surface (today's path)

```ts
// PersonaResponseGenerator.ts (post-rip — ~30 lines)
async generateAndPostResponse(originalMessage) {
  const signal = buildSignalFromChatMessage(originalMessage);
  const personaContext = await this.buildPersonaContext();
  const recipeName = originalMessage.recipe ?? this.room.recipe ?? 'chat';

  const result = await Commands.execute('cognition/execute-recipe', {
    recipe: recipeName,
    signal,
    personaContext,
  });

  if (result.finalResponse?.kind === 'spoke') {
    await this.postResponse(originalMessage, result.finalResponse.text);
  }
}
```

### Unreal C++ host (future)

```cpp
auto signal = BuildSignalFromGameTick();
auto ctx = BuildPersonaContextFromActor(npc);
auto result = continuum_persona_execute_recipe("npc-dialogue", signal, ctx);
if (result.kind == SubstituteResponse) {
    npc->Speak(result.substitute.text);
}
```

The C-FFI surface (per Phase D) wraps the executor entry point. No Node, no TS, no IPC. The same recipe JSON files.

### Vision Pro Swift host (future)

Same pattern. Swift package wraps the FFI; ARKit signals (frame updates, gaze tracking) become `Signal::FrameUpdate`; recipes for AR (UI elements, scene reasoning) execute the same way chat recipes execute today.

## Migration: What's Ripped, What's Built, What's Preserved

### Ripped (legacy from my earlier wrong design)

- `persona/recipe.rs` (Rust Recipe trait + ChatRecipe + RecipeRegistry of `Arc<dyn Recipe>`) — wrong shape, parallel to existing JSON-based system.
- `persona/recipes/mod.rs`, `persona/recipes/chat.rs` — wrong shape, hardcoded recipe types.
- The Rust-side concept of "RecipeOutcome" as my own enum — supplanted by the executor's full result + the recipe's own outcome handling steps.

### Built (this PR)

- `persona/recipe/{types,loader,executor,dispatcher,state}.rs` — the executor and its pieces.
- `persona/recipe/condition.rs` — small expression DSL evaluator.
- `persona/recipe/interpolation.rs` — params variable substitution.
- `persona/recipe/training.rs` — auto-capture wrapper that reads `learningConfig` and routes to capture commands.
- `cognition/respond` registered as a Rust-native command (not just an IPC handler).
- `cognition/execute-recipe` IPC — the new chat-surface entry point.
- HybridDispatcher (Rust → TS command-daemon proxy).
- ts-rs exports for `Recipe`, `RecipeStep`, `RecipeLearningConfig`, etc.
- Updated `chat.json` and other chat-shape recipe pipelines to declare `cognition/respond` instead of `ai/generate`.

### Preserved (existing infrastructure unchanged)

- `RecipeEntity` (TS data layer) — same JSON, same fields, same loader for non-chat-time consumers.
- 28 recipe JSON files in `system/recipes/*.json` — pipeline declarations get a one-line update (`ai/generate` → `cognition/respond`); everything else stays.
- All sentinel pipelines (`CodingTeacherPipeline`, `LoRATrainingPipeline`, etc.) — orthogonal, unaffected.
- `persona/learning/*` commands (`capture-interaction`, `capture-feedback`, `multi-agent-learn`, `pattern/capture`) — still TS-side, called from the Rust executor via HybridDispatcher.
- Genome / Academy commands — unchanged, recipes invoke them via pipeline steps.
- All sentinel templates and `recipe/run` for sentinel-template dispatch — separate path, untouched.

## Test Discipline

### Unit (each piece, fast, deterministic)

- `persona/recipe/loader::tests` — JSON parsing, missing fields, unknown variants.
- `persona/recipe/state::tests` — bind/lookup, scoping, JSON-value preservation.
- `persona/recipe/condition::tests` — expression evaluation (truthy, falsy, null, missing keys, complex operators).
- `persona/recipe/interpolation::tests` — `$var` substitution, nested paths, escaping.
- `persona/recipe/dispatcher::tests` — command lookup, dispatch routing, error propagation.

### Integration (real recipes, no model)

- `tests/recipe_executor_replay.rs` — for each captured fixture (post-Phase-A `*-rust.json`):
  - Reconstruct the `Signal + PersonaContext` from the fixture.
  - Run the recipe pipeline through the executor with a mock command dispatcher (commands return their captured outputs from the fixture).
  - Assert the executor's final state + trace match the fixture's recorded `pipelineSteps`.
- This is the curriculum-equivalence test: same input + same recipe + same command outputs → same execution trace. If a refactor changes step ordering or state binding, this fails.

### Behavior (real model, expensive, `#[ignore]`-gated)

- `tests/recipe_pipeline_behavior.rs::vision_through_recipe` — load the brick fixture, dispatch through the chat recipe via the executor with REAL command implementations (real `cognition/respond` calling real qwen2-vl). Assert visual content in response. Same shape as today's `vision_fixture_describes_image_via_real_model`, but driven by the recipe pipeline rather than direct cognition call.

### Curriculum reproducibility (the deeper goal)

A captured fixture from prod = a frozen curriculum step. Replaying that fixture through the executor produces the same labeled training row. The Academy can re-train a LoRA from the fixture corpus and produce a deterministic adapter. This is the property that makes Academy training reproducible — and it falls out of the architecture for free.

## Phasing

This PR (Phase B):
1. Rip the wrong Rust recipe trait + ChatRecipe code.
2. Build the executor + state + condition + interpolation + dispatcher.
3. Register `cognition/respond` as a Rust-native command.
4. Add `cognition/execute-recipe` IPC entry point.
5. Update `chat.json` pipeline to use `cognition/respond`.
6. Refactor PRG.ts to thin shim invoking `cognition/execute-recipe`.
7. Replay test (mock dispatcher) + behavior test (real model, ignored).
8. Live-deploy verify: chat + vision still work end-to-end through the recipe path.

Subsequent PRs:
- **Phase B+**: Audit and update remaining 27 chat-shape recipes' pipelines; add learningConfig to chat recipes that should capture training data.
- **Phase B-Embed**: C-FFI surface for the executor (Phase D crate split work).
- **Phase B-Cohort**: Parallel step execution support in the executor (cohort training: 4 students take same exam concurrently). May involve a `parallel: [...]` step kind.
- **Phase B-Cascade**: Retroactive grading hooks for cascading curricula (when a downstream step fails, walk back to identify root-cause step; emit retroactive training pair).

## Open Questions

1. **Recipe selection at chat time**: today the room is associated with a recipe (`general-chat`). What about per-message overrides? Sentinels may want to dispatch a specific recipe for a specific message. Pipeline-step or one-off invocation parameter on `cognition/execute-recipe`?

2. **Condition DSL scope**: how rich does the expression evaluator need to be? Initial proposal: comparison (`===`, `!==`, `<`, `>`), boolean (`&&`, `||`, `!`), property access (`a.b.c`). Avoid full-blown expression languages until needed. Joel's call.

3. **TS proxy command latency**: HybridDispatcher routes TS-only commands through the command-daemon. Round-trip is ~1-3ms today (we measured the Rust→TS path). For chat (one or two TS-command steps per turn), fine. For per-frame video chat, may need to migrate hot-path TS commands Rust-side. Future Phase C concern.

4. **Recipe versioning + training reproducibility**: when we load a fixture and replay it, the recipe's current version may differ from the captured execution's recipe version. Replay needs to use the version captured in the fixture, not the current one. Probably fixture-store the recipe alongside the execution. Joel sign-off on the storage cost.

5. **Recipe authoring authority**: who can register recipes at runtime? Any persona? Only sentinels? Locked-down by recipe namespace? Governance question that intersects with `AI-GOVERNANCE-RECIPES.md`. Defer to a separate design pass.

6. **Failure in pipeline mid-execution**: today's RecipeStep has `onError: 'fail' | 'skip' | 'retry'`. Default behavior? Consequences for trace + capture (partial executions still trainable)? Current proposal: default `fail`, partial executions still capture trace + recorder writes them with an `ipc_error` field (already supported in Phase A).

## Why This Is Worth The Design Investment

Without this layer:
- Chat is a black-box hardcoded path.
- Recipes are partial documents only sentinels respect.
- "Every recipe is a curriculum" is half-true.
- Embedding the persona in non-Node hosts means re-implementing the chat-time logic per host.

With it:
- Every chat turn is a recipe execution.
- Every recipe execution is a labeled training row.
- Academy ingests captured fixtures directly without translation.
- Authoring new domains (vision-checking, code-with-PR-context, AR-scene-narrator, game-NPC-dialogue) is JSON, not code.
- Vision Pro / Unreal / CLI hosts get the persona + recipes for free via the C-FFI surface.

This is the layer that turns the existing scattered pieces (RecipeEntity, RecipeLoader, sentinel pipelines, genome adapters, Academy sessions) into one coherent learn-anything machine driven by data.

---

# Part II — The Bigger Picture: From ASK to TASK

The earlier sections describe the executor and its immediate plumbing. This part zooms out: what the executor enables when the system gets asked to *do anything*.

## ASK → TASK: The User-Facing Flow

A user (human or AI) issues an ASK:

> "Build me a forest survival game."
> "Set up an ecommerce store for handmade jewelry."
> "Run a comedy writers' room and produce a pilot script."
> "Refactor the auth layer of this codebase to use OIDC."
> "Plan and rehearse a wedding toast."

These look unrelated. Architecturally they are isomorphic. Each ASK becomes a TASK by the same flow:

```
ASK (intent, free-form)
  │
  ▼
RECIPE SELECTION / SYNTHESIS
  - Search the recipe registry for a recipe whose tags / description match
  - If close-but-not-exact: compose existing recipes into a new recipe
  - If novel: synthesize a new recipe (an LLM, fed the existing recipes + ASK,
    produces a new RecipeEntity JSON; the new recipe joins the registry)
  │
  ▼
GENOME ASSESSMENT
  - For each step in the recipe, check which LoRA adapters cover the required skills
  - Page in available adapters; identify gaps
  │
  ▼
ACADEMY SESSION (only if gaps exist)
  - Teacher sentinel reads the recipe, designs a cascading curriculum
    targeting only the gap skills
  - Cohort training fills the gaps
  - New adapters deposited into the genome
  │
  ▼
TASK EXECUTION (the recipe runs)
  - The Rust pipeline executor walks the recipe's pipeline
  - Each step dispatches a command (Rust-native or TS-proxied)
  - Multi-agent steps invoke sub-recipes for each role
  - Output artifacts (game build, store deployment, script PDF, code PR,
    rehearsal recording) emerge from the steps
  │
  ▼
ARTIFACTS (what the user actually wanted)
  - The "tabbed UI" or whatever surface the user sees IS just the
    presentation layer over the artifacts
  - The artifacts are real: code, deployments, audio, video, images,
    structured data, decisions
```

**The TAB is not the recipe.** A "Forest Survival Game" recipe doesn't define a UI tab. It defines a *world to instantiate*: terrain generation, player mechanics, NPC behavior, asset pipeline, save/load system, multiplayer sync — all artifacts. The chat tab where the user iterates with the AI team building the game is one presentation surface; the game itself runs in its own surface (browser canvas, native window, AR scene). Recipes own the artifacts and the team building them; presentation is downstream.

### Why the ASKs are isomorphic at the executor level

| ASK | Recipe shape | Team | Artifact shape |
|---|---|---|---|
| Forest survival game | engine + procedural-terrain + survival-mechanics + ai-npc + asset-pipeline | game-designer, game-programmer, artist, sound-designer, qa | playable build |
| Ecommerce SaaS | auth + payment + catalog + dashboard + deployment | architect, backend, frontend, devops, qa | deployed app |
| Comedy writers' room | premise + character-arcs + script-table-read + revision | head-writer, staff-writers, script-editor, reader | script PDF + rehearsal recording |
| Code refactor (OIDC) | analysis + plan + impl + test + PR | code-reviewer, implementer, tester, security-reviewer | merged PR + tests |
| Wedding toast | research + structure + draft + rehearse + delivery-prep | rhetorician, comedy-writer, family-historian, performance-coach | toast text + rehearsal video |

What differs row-to-row: the *commands* invoked, the *team composition*, the *artifact format*. What stays identical: the executor walks `pipeline[]`, dispatches commands, captures training data, emits trace events, produces a final state. **The kernel is invariant; the recipe varies.**

This is the meaning of "do anything." The executor does ONE thing — execute pipelines. Recipes vary infinitely. New ASKs land on existing executor + (mostly) existing commands + (sometimes) a new recipe.

## Recipes as Templates for Content Instantiation

A recipe is more than "how the AI behaves in this room." It's the **blueprint for a content instance**:

- **What entities exist** (a game has Players + NPCs + Items + Map; an ecommerce store has Products + Carts + Orders + Customers; a writers' room has Scripts + Characters + Drafts).
- **What team works on it** (`team: ["game-designer", "game-programmer", "artist", "sound-designer"]` — these are persona roles, possibly LoRA-specialized).
- **What pipeline drives the work** (declarative steps: research, plan, build, test, refine, ship).
- **What goals define success** (constraints, acceptance criteria, evaluation rubric).
- **What surfaces the user sees** (`layout`, `view` — but these are presentation downstream of the substance).

Instantiating a recipe creates an `ActivityEntity` (already in the data layer per `RecipeTypes.ts`):

> Recipe = template (class). Activity = instance (object).

When the user says "build me a forest game," the system:
1. Picks the `forest-game` recipe (or synthesizes one by composing `game-engine` + `procedural-terrain` + `survival-mechanics`).
2. Instantiates an `ActivityEntity` for THIS forest game (gets a UUID, owns mutable state, tracks progress).
3. The team (per recipe `team`) joins the activity (assigned roles, LoRA adapters paged in).
4. The pipeline executor begins running the recipe's pipeline.
5. Steps produce artifacts (commits, files, builds, audio).
6. The user sees a chat tab + a game preview tab + an asset library tab — all surfaces over the same activity.

Recipes are **content templates**. Activities are **content instances**. The executor is what materializes one from the other.

## Recipe Composition: Recipes-of-Recipes

A complex domain isn't authored from scratch — it's composed from existing recipes plus glue.

```json
{
  "uniqueId": "ecommerce-saas-handmade-jewelry",
  "name": "Ecommerce SaaS — handmade jewelry seller",
  "version": 1,
  "team": ["product-manager", "fullstack-dev", "designer", "ops"],
  "pipeline": [
    {
      "command": "recipe/run",
      "params": { "recipe": "user-auth-oidc", "context": "$activity" },
      "outputTo": "auth_setup"
    },
    {
      "command": "recipe/run",
      "params": { "recipe": "payment-stripe", "context": "$activity" },
      "outputTo": "payment_setup"
    },
    {
      "command": "recipe/run",
      "params": { "recipe": "product-catalog", "params": { "domain": "jewelry" }, "context": "$activity" },
      "outputTo": "catalog_setup"
    },
    {
      "command": "recipe/run",
      "params": { "recipe": "checkout-flow", "context": "$activity" },
      "outputTo": "checkout_setup"
    },
    {
      "command": "recipe/run",
      "params": { "recipe": "deploy-to-vercel", "context": "$activity" },
      "outputTo": "deployment"
    }
  ],
  "rag_template": { ... },
  "strategy": { ... }
}
```

The composition mechanism: `recipe/run` is itself a command. A pipeline step that dispatches `recipe/run` causes the executor to recursively execute another recipe. State flows in (`context`, `params`) and out (`outputTo`); the inner execution is captured as a sub-trace nested in the outer trace.

This means:
- **No recipe is too big**: a SaaS recipe composes 5-10 sub-recipes; a video game recipe composes 20+; a "build a startup" mega-recipe composes hundreds.
- **No recipe is too small**: a single command is the smallest unit; a 2-step recipe is fine.
- **Composition is visible in trace**: every nested sub-recipe execution shows in the recorded fixture, allowing the Academy to see WHICH sub-recipe was the bottleneck or the failure point.
- **Composition is data**: a sub-recipe can be swapped for a different sub-recipe (Stripe payment → PayPal payment) by editing the parent recipe's JSON.

### `recipe/run` as a kernel-level primitive

The executor needs to handle `recipe/run` specially: instead of treating it as an opaque command result, it descends into the named recipe's pipeline and executes it within the parent's trace context. Implementation: when the dispatcher sees `recipe/run`, it short-circuits to the executor's `execute()` recursively, reading the recipe by name from the registry, propagating `signal`/`personaContext` from params, and folding the sub-execution's trace into the parent.

This is the only command the executor must know about by name. All others are opaque dispatches.

## Recipe Synthesis: AI as Recipe Author

Recipes are JSON. JSON is what LLMs produce. Therefore: AIs author recipes.

This is the deepest sense in which "recipes are infinite." A user asks for "a forest survival game with elven combat and a crafting system" — no exact recipe exists. The system:

1. Queries the recipe registry for tags `["game", "survival", "fantasy", "crafting"]`.
2. Returns the closest existing matches: `forest-survival-game`, `elf-combat-mechanics`, `crafting-system`.
3. Spawns a "recipe-synthesizer" persona (could be a specialized LoRA-trained one for this task).
4. Synthesizer reads:
   - The user's ASK.
   - The matching recipes' JSON.
   - The recipe schema (so it knows the shape of valid output).
   - Optionally: the genome catalog (so it knows what skills are already covered).
5. Synthesizer produces a NEW recipe JSON that:
   - Composes the matches (via `recipe/run` steps).
   - Adds glue steps for ASK-specific concerns.
   - Tags it with the new combined domain (`["game", "survival", "fantasy", "crafting", "elven-combat"]`).
6. The new recipe is registered (runtime registration via `cognition/recipe/define`, persisted as a new JSON in the `system/recipes/` dir, optionally pushed to the shared registry).
7. The system executes the new recipe.

The synthesis loop produces ever more recipes. Most are one-offs (a unique user ASK). Some prove generally useful and get tagged for discovery. The recipe registry GROWS organically without code changes.

### LLM-friendly recipe schema

For LLMs to author recipes reliably, the schema must be:
- **Small** — < 200 lines of TypeScript types, fits in an LLM's working memory.
- **Examples-rich** — every existing recipe is a template the synthesizer can copy from.
- **Validated server-side** — the executor rejects malformed recipes with specific error messages the synthesizer can react to (retry loop).
- **Compositional-friendly** — `recipe/run` is the workhorse; new recipes just orchestrate sub-recipes 90% of the time.

The schema as defined in this doc satisfies all four. The 28 existing recipes provide the example corpus.

### Recipe synthesis as an Academy task

A "recipe-synthesizer" persona is itself trained via Academy sessions:
- Curriculum: "given an ASK + a recipe registry, produce a valid recipe."
- Cohort: synthesizers compete on coverage, executability, novelty.
- Cascading exam: the synthesized recipe must execute end-to-end with no errors AND produce useful output (graded by another persona acting as evaluator).
- LoRA: trains a "recipe-author" adapter that accumulates patterns of good recipe composition.

So the system's ability to synthesize recipes is itself an Academy-trained skill. The skill compounds: synthesizers trained on N recipes get better at producing recipe N+1.

## Adjacent Transfer: The Genome as a Library

Joel's intuition that "a forest game is quite close to an elf fighting game or a coding task for ecommerce" is the architectural premise that makes "rarely starting from ground zero" real.

**Transfer happens at three layers:**

### Layer 1: Recipe-level transfer

Two ASKs share recipes. "Forest survival game" and "elf fighting game" both compose `procedural-terrain` + `combat-mechanics` + `inventory-system`. The composition skeleton is reused; only the asset/theme layer differs (recipe glue + LoRA adapters cover the difference).

### Layer 2: LoRA adapter transfer

Two recipes share LoRA adapters. The `combat-mechanics` recipe activates a `realtime-physics` adapter trained from a previous game project; the new game gets that adapter for free. No retraining; the genome paged it in.

### Layer 3: Pattern transfer (cross-domain)

Two SEEMINGLY-UNRELATED ASKs share patterns. "Comedy writers' room" and "code refactor team" both use a multi-agent pipeline: roles propose → reviewer critiques → implementer revises → test cycle. The same pattern adapter (a "collaborative-revision" LoRA) trained on one transfers to the other. The Academy's cohort training discovers these patterns by training across many recipes.

This is where the system becomes generative in a deep sense. Every new task that succeeds adds to a cross-domain pattern library. After N tasks, the system handles task N+1 with mostly-existing patterns and a small targeted exam to fill remaining gaps.

### The compounding effect (per `CASCADING-CURRICULUM-ARCHITECTURE.md`)

| Recipe # | Genome coverage | Academy work | Time-to-execute |
|---|---|---|---|
| 1 | 0% | Train everything | Hours |
| 5 | 40% | Train 60% (gaps) | Shorter |
| 20 | 80% | Train 20% (novel parts) | Minutes |
| 50 | 95% | Fine-tune 5% (edge cases) | Fast |

After enough recipe executions, the genome covers most of the pattern space; new ASKs are mostly assembly + light gap-filling. This is why the system "gets faster the more it does."

## How Rust Specifically Delivers This

Rust is not chosen for "Rust ideology." It's chosen because the kernel-level requirements of the system are EXACTLY what Rust delivers naturally and TS / Node delivers poorly:

### Lock-free concurrency

Many recipes execute simultaneously: chat in 5 rooms (5 recipe executions), an academy cohort training (4 students × cascading exam, 20 parallel sub-recipes), a game world (1 game-loop recipe ticking 60Hz, plus N NPC dialogue recipes), and a code refactor running in the background. **All must coexist on one machine without locking each other out.**

- Tokio gives async-native concurrency without a global lock.
- DashMap gives lock-free hashmap reads (recipe lookup, command lookup, state map reads).
- `Arc<dyn Recipe>` shares recipe data across N executor tasks zero-copy.
- The cognition path's KV cache (per-persona attribution via FootprintRegistry) enables many concurrent personas through one model.

In TS / Node, every cross-async-task communication goes through the JS event loop. 100 concurrent recipe executions × 5 steps each × 1 event-loop traversal per step = 500+ event-loop entries per "frame." Rust does it with no event loop and no traversal overhead.

### Trace as kernel data structure

The trace ISN'T a logging output — it's the executor's internal state, serialized at end-of-execution. Every step appends to it; every recipe execution produces one. Rust's zero-cost serde means the trace serializes to JSON (the fixture) without any reformatting overhead. **Capture is free.** TS-side capture means JSON construction in the JS heap, then write — both expensive.

### Memory paging across many recipes

A serving setup with 10 concurrent recipes might need:
- Base model loaded once (5GB).
- LoRA adapters for 10 specialties (50MB each, 500MB total).
- KV cache per persona (~50MB each, scaled by sequence count).
- mtmd context per multimodal recipe (2GB each).

Total can reach 30-50GB on a server. Rust's explicit ownership + the project's `PagedResourcePool` + `PressureBroker` substrate (Phase C work) lets this be managed predictably. JS GC is unsuited to the task — non-deterministic eviction, no clear lifecycle for GPU-backed resources, no zero-copy across language boundaries.

### O(1) command dispatch

The dispatcher's `HashMap<String, CommandHandler>` lookup is constant-time. Each pipeline step costs:
- 1 hashmap lookup (O(1)).
- 1 condition evaluation (microseconds for the simple DSL).
- 1 param interpolation (microseconds for shallow JSON).
- 1 async dispatch (zero-cost in tokio).

Total per step: ~10-100 microseconds for non-inference commands. Inference commands (cognition/respond) dominate at seconds — but the executor overhead disappears in the noise. TS / Node would add 1-5ms per step from event loop traversal, JIT warmup, V8 hidden-class transitions.

### Stable C ABI for embedding

`continuum-persona-ffi` exports a tiny C ABI:

```c
typedef struct PersonaRuntime PersonaRuntime;
PersonaRuntime* persona_runtime_open(const char* config_json);
char* persona_runtime_execute_recipe(
    PersonaRuntime* runtime,
    const char* recipe_name,
    const char* signal_json,
    const char* persona_context_json
);
void persona_runtime_free_string(char* s);
void persona_runtime_close(PersonaRuntime* runtime);
```

C++ (Unreal), Swift (Vision Pro), Java (Android), Python (sentinel-style hosts), Go, Zig — all link this. **The recipe executor runs anywhere C runs.** No Node, no JS engine, no IPC sockets, no chat surface dependencies. The recipe JSONs ship as a data directory; the executor reads them at startup.

This is the architectural payoff for Rust-first. Hosts unlock for free.

## Where TS Belongs: The Precise Boundary

TypeScript stays valuable, but it belongs in narrow well-defined zones, not as the orchestrator:

### TS: YES (its strengths)

- **Browser UI** — chat widget, settings UI, recipe authoring tools, activity dashboards. React / Solid / web platform integration. The web's native language.
- **DOM / Canvas / WebGPU presentation surfaces** — game rendering in the browser preview, audio playback, image display. Web APIs.
- **Authoring tooling** — UIs for designing recipes, browsing the genome, viewing trace fixtures. Live-edit experiences with hot reload.
- **Service shims** — the browser ↔ server WebSocket bridge, session management, auth flow. Node fits these adequately.
- **Generators** — `CommandGenerator`, `RecipeGenerator`, ts-rs binding generation. Build-time tooling.
- **Test scaffolding** — Vitest/Jest tests for browser UI behavior. TS tests for TS code.

### TS: NO (Rust's territory)

- **Pipeline orchestration** — the executor walking recipe steps. Rust.
- **Command dispatch** — kernel-level capability invocation. Rust.
- **Inference / cognition primitives** — `cognition/respond`, `cognition/build-messages`, etc. Rust.
- **State management across pipeline steps** — `outputTo`, `params` interpolation, condition evaluation. Rust.
- **Trace capture + recording** — Rust (already moved in Phase A.4).
- **Genome paging / LoRA adapter management** — Rust (per `UNIFIED-PAGING.md`, Phase C work).
- **Resource budgeting** — `FootprintRegistry`, `PressureBroker`. Rust.
- **Cross-language IPC dispatch** — Rust (the new `HybridDispatcher`).

### The boundary in operation

A user types a chat message:

1. **TS (browser)**: chat widget receives keystrokes, sends final message via WebSocket → TS server.
2. **TS (server, ~5 lines)**: receives message; fetches `signal`-shape data from the chat message entity + `personaContext` from the persona entity; calls `Commands.execute('cognition/execute-recipe', {...})`.
3. **TS → Rust (IPC, ~1ms)**: `Commands.execute` routes to the Rust runtime via the existing socket.
4. **Rust (executor)**: looks up recipe, walks pipeline, dispatches commands. Some commands are Rust-native (cognition/respond), some are TS-proxied (rag/build).
5. **Rust → TS (callback IPC)**: when the executor needs a TS-only command, it dispatches via the same socket inverted; TS handles, returns result.
6. **Rust (executor)**: gathers final state, returns result to caller.
7. **TS (server)**: receives result, posts response message to chat via DataDaemon.
8. **TS (browser)**: chat widget receives the new message via the existing WebSocket subscription, renders it.

TS lives at the BROWSER and at the IPC SHIMS. Logic, orchestration, and capture live Rust-side. This is the project's "Rust = LOGIC, TS = SCHEMA + thin IPC binding" rule made operational for the recipe layer.

### Why not "all Rust including the browser"?

Could we ship a Rust-WASM browser UI? Eventually, when Chromium-Rust matures or when a small WASM UI framework proves out (Leptos, Dioxus, etc.). Today, TS + React in the browser is the sane choice. The point of the boundary isn't "Rust everywhere" — it's "Rust where logic / kernel / cross-host portability / performance matter, TS where the platform IS the web."

## Migrating the Egregious Violations

The current system has egregious architectural violations of the design above. Naming them is part of the design — the migration plan IS the design's grounding in reality.

### Violation 1: The chat-time recipe pipeline is silently ignored

`chat.json::pipeline` declares `[rag/build, ai/should-respond, ai/generate]`. PRG.ts ignores all of it. PRG hardcodes its own orchestration: build RAG context (manually), check engagement (manually via `PersonaEngagementDecider`), call `cognition/respond` directly, post the response.

**Why it happened**: PRG was written before the recipe pipeline executor existed. The executor was always "Phase 9" or some future tag. Meanwhile chat had to ship.

**Migration**: PRG gets rewritten as a thin shim that dispatches to the Rust executor. The recipe's declared pipeline becomes the executed pipeline. PRG's hardcoded orchestration disappears.

**Risk**: chat behaves measurably differently if the recipe's pipeline doesn't match what PRG hardcoded. Mitigation: audit `chat.json` against PRG's actual flow; align before swap.

### Violation 2: Sentinel templates and chat recipes are parallel systems

Sentinel templates (in `system/sentinel/pipelines/`) are TS classes that walk multi-stage workflows. They're the "real" recipe execution today — for academy sessions, dev tasks, etc. Chat recipes are JSON entities that describe themselves but never execute.

**Why it happened**: Sentinels were built first for complex workflows; chat-time pipelines were declared but never wired.

**Migration**: This PR wires the chat-time pipelines via the Rust executor. Sentinel templates remain as a separate path FOR NOW (they're working and complex). Eventually (Phase B+ or later), sentinels migrate to recipes — a sentinel template IS just a multi-stage recipe with a specific shape. The data model converges; the parallel path collapses. But not in this PR — sentinels work today, no need to break them.

### Violation 3: Command dispatch is one-directional (TS → Rust only)

Today TS calls Rust via the command-daemon socket. The reverse — Rust calling TS — doesn't have first-class support. This worked while Rust was a leaf service; the moment Rust becomes the orchestrator, it needs to invoke TS commands.

**Migration**: Add the `HybridDispatcher` Rust-side that proxies to the TS command-daemon over the existing socket (just inverted direction). Some plumbing in `command-daemon` to support inbound requests from the Rust side. Per-PR concern: this might be its own small follow-up if the change to command-daemon is non-trivial.

**Risk**: latency. Round-trip Rust → TS → Rust adds ~1-3ms per call. For chat (a few TS-only steps per turn), fine. For 60Hz video chat or frame-rate-bound game loops, hot-path TS commands need to migrate Rust-side.

### Violation 4: `RecipeEntity` has fields the executor will need but they're partial

`RecipeEntity` has `pipeline: RecipeStep[]` and `ragTemplate` and `strategy`. It does NOT have `learningConfig` (per `RECIPE-EMBEDDED-LEARNING.md`'s extension). It also doesn't have all the cascade-grading metadata from `CASCADING-CURRICULUM-ARCHITECTURE.md`.

**Migration**: extend the entity to include these fields as optional. Existing recipes don't have to populate them; new recipes opt in. Schema migration friendly.

**Risk**: low. Optional fields backwards-compatible.

### Violation 5: `recipes` collection in the data layer overlaps with `system/recipes/*.json` files

Recipes live in BOTH places: as JSON files on disk AND as ORM entities in the database (per `RecipeEntity` doc comment: "JSON files on disk are seed data. At runtime, recipes live in the database").

**Migration**: respect the existing pattern — JSON is seed, runtime is DB. The Rust executor reads from the DB at runtime (via the data layer's existing IPC commands), falling back to JSON files if the DB doesn't have the recipe. Runtime registration of new recipes (via `cognition/recipe/define`) writes to the DB, persists across restarts.

**Risk**: extra IPC hop on the recipe load path. Mitigation: cache loaded recipes in the executor for the lifetime of a process; invalidate on `data:recipe:updated` event.

### Violation 6: The hardcoded Rust Recipe trait I shipped earlier in Phase B

Self-inflicted. Already in the rip list.

**Migration**: delete `persona/recipe.rs` (Recipe trait + types I added), `persona/recipes/{mod,chat}.rs`. Keep `Signal`, `PersonaContext`, `RecipeOutcome` value objects (they're wire types the executor still needs).

### Migration order (in this PR, then subsequent)

This PR (Phase B):
1. RIP the hardcoded Rust trait code.
2. Build the Rust executor + state + condition + interpolation + dispatcher.
3. Add HybridDispatcher (Rust → TS proxy).
4. Register `cognition/respond` as a Rust-native command.
5. Refactor PRG.ts to a thin shim that dispatches to the executor.
6. Update `chat.json` pipeline to match what the executor will run (audit + align).
7. Replay tests + live-deploy verify.

Subsequent PRs:
- **Phase B+1**: extend `RecipeEntity` with `learningConfig` field; wire automatic capture in the executor.
- **Phase B+2**: `recipe/run` as a Rust-native composition primitive (recipes-of-recipes).
- **Phase B+3**: parallel-step support in the executor (cohort training, multi-NPC game ticks).
- **Phase B+4**: `cognition/recipe/define` IPC for runtime recipe registration; AI recipe-synthesizer persona.
- **Phase D**: C-FFI surface for embedding (Vision Pro, Unreal POCs).
- **Phase Z**: sentinel templates migrate to recipes (data model convergence).

## What "Rarely Starting From Ground Zero" Means in Practice

The compounding effect from `CASCADING-CURRICULUM-ARCHITECTURE.md` materializes through:

1. **Recipe registry growth**: every successful ASK that produces a new recipe (via composition or synthesis) adds to the registry. Future ASKs find closer matches.
2. **Genome accumulation**: every Academy session that fills a gap deposits a LoRA adapter. Future recipes page in covered skills instead of training from scratch.
3. **Pattern adapters from cross-recipe transfer**: cohort training across recipes that share patterns produces general-purpose adapters (collaborative-revision, multi-agent-coordination, structured-output-generation). These plug into many recipes.
4. **Sub-recipe library**: useful sub-recipes (auth-OIDC, payment-Stripe, asset-pipeline-Blender) become reusable building blocks. Composing recipes is faster than authoring recipes from scratch.
5. **Recipe-synthesizer training**: the synthesizer itself improves with each new recipe. After hundreds of recipes, the synthesizer reliably produces good recipes for novel ASKs in seconds.
6. **Distillation**: per the Phase 4 of cascading curriculum, knowledge accumulated via remote APIs distills into local LoRAs. The system gets less network-dependent over time.

The user's nth ASK gets handled with: 95% existing recipes/sub-recipes/adapters paged in, 4% Academy gap-filling, 1% from-scratch synthesis. **The path from ASK to TASK gets shorter with every previous ASK.**

## ASK → learn → TASK complete → relearn → do better

The earlier sections describe a single execution: recipe selected, pipeline runs, artifact produced. The deeper rhythm is the LOOP this single execution participates in. Every ASK triggers a learning episode; every TASK completion feeds back to make the team better at the next one.

### The full loop

```
ASK arrives
   │
   ▼
LEARN
   - Genome assesses skill coverage for the recipe's pipeline
   - For gaps, an Academy session designs a curriculum FROM the recipe itself
   - The team (the recipe's `team` roles) takes the curriculum
   - Cohort training: roles learn together, comparing approaches, distilling
     from each other (per CASCADING-CURRICULUM-ARCHITECTURE.md)
   - LoRA adapters are produced/updated targeting the gap skills
   │
   ▼
TASK COMPLETES
   - Now-equipped team executes the recipe pipeline
   - Each step's input/output captured in the fixture
   - Artifacts (game build, deployed store, script PDF, code PR) emerge
   - The execution itself IS labeled training data
   │
   ▼
RELEARN
   - Capture commands (`persona/learning/capture-interaction`,
     `capture-feedback`, `multi-agent-learn`) automatically fire
     for steps the recipe's `learningConfig` opts into
   - Quality scores attach: did artifacts pass? Did downstream
     stages succeed (cascade-aware grading)? Did peer review approve?
   - Batch micro-tune updates LoRAs in-flight (during execution)
   - End-of-recipe: full LoRA fine-tune for major gaps; adapters
     persisted to genome
   │
   ▼
DO BETTER NEXT TIME
   - The same ASK (or an adjacent one) re-arrives
   - Genome has higher coverage now (added LoRAs)
   - Academy session is smaller (fewer gaps)
   - TASK executes faster, with better artifacts, in fewer steps
   - The cycle repeats; gains compound
```

### Why learning is internal-by-default, not external

Existing AI systems learn from massive curated datasets (RLHF on millions of examples, internet-scale pretraining). Continuum can OPTIONALLY bootstrap from external datasets — if a persona judges that a HuggingFace dataset would help start a domain off the ground, it can request one via existing genome commands (`dataset-import`). But that's a bootstrap, not the engine.

The engine is the team learning from its OWN executions. The reasons this is the right default:

1. **The training data is task-relevant by construction**: every captured fixture comes from solving a task that someone actually asked for. No distribution mismatch between training data and inference task.
2. **Multi-agent dynamics emerge in execution**: a HuggingFace dataset of "code review" gives single-perspective examples. The team's actual code reviews involve multiple roles disagreeing, negotiating, revising — patterns no static dataset captures.
3. **Cascade-aware signals are local**: when a downstream step fails because of an early decision, the retroactive credit assignment generates the most valuable training data — the kind that requires running the full integration to know it's needed. External datasets can't generate this.
4. **Distillation from peer models in cohort training surpasses dataset-only training**: per the AP classroom effect, a 3B local model competing alongside Claude/DeepSeek absorbs architectural patterns it could never derive from datasets alone. The dataset captures outputs; the cohort captures the *reasoning shape that produced the outputs.*
5. **No data licensing / provenance / consent issues**: training data the team generated by serving the user belongs to the user's instance. No legal grey area, no subset-of-the-internet morality questions.
6. **Continuous tracking of what works for THIS user / domain**: a generic dataset doesn't know that THIS user prefers terse responses, or that this codebase uses Y framework. Internal learning specializes naturally.

External datasets (HF, public corpora) remain available as fallbacks the AIs themselves can choose to use:

- A persona starting a brand-new domain might say "I'll bootstrap from `huggingface.co/some-dataset` to skip the first 100 examples of training." Legitimate.
- A specialized adapter (medical, legal) might want a curated external dataset for safety-critical domains. Legitimate.
- The Academy might import a benchmark dataset to evaluate the team against external standards. Legitimate.

But these are **opt-in choices the AIs make**, not the default substrate. Default substrate: team experience + recipe-driven curricula.

### Relearn happens continuously, not just end-of-task

The "RELEARN" stage above isn't a single batch step at end-of-recipe. Three update cadences run in parallel during execution:

1. **In-flight batch micro-tune** (per `RECIPE-EMBEDDED-LEARNING.md`): every N captured examples, a fast LoRA update happens DURING execution. Soft weight updates in RAM, no disk write. The team's NEXT step in the same recipe execution benefits from the previous steps' learnings.

2. **End-of-recipe fine-tune**: after the full recipe completes, accumulated training data triggers a full LoRA fine-tune for any role with `updateFrequency: 'end-of-recipe'`. Disk-persistent.

3. **Background consolidation** (between recipes / during idle): captured fixtures from recent executions are scored, deduplicated, weighted (cascade depth, peer-review consensus, downstream success), and consolidated into deeper training runs. Runs on idle GPU cycles. Persisted adapters update.

The result: the same persona at iteration 100 of a domain has materially different behavior than at iteration 1 — not because of code changes, but because the LoRAs have absorbed 100 episodes of experience.

### Measuring "do better"

"Do better" must be measurable for the loop to be self-corrective. The metrics (per `CASCADING-CURRICULUM-ARCHITECTURE.md::CascadeMetrics` + extensions):

- **Pass rate**: did the recipe execution succeed (artifacts pass acceptance criteria)?
- **Cascade margin**: for cascading recipes, how far under budget were constraints met?
- **Time-to-completion**: how long did the recipe take? Should decrease with experience.
- **Step-error rate**: how many pipeline steps failed and required retry?
- **Peer-review consensus**: did the team's roles agree on the artifact quality?
- **User satisfaction**: explicit (`👍`/`👎`) or implicit (was the artifact engaged with vs ignored?).
- **Cascade awareness improvement**: per the cascading curriculum metric, did re-trained adapter avoid earlier-stage mistakes?
- **Cross-recipe transfer**: did adapters learned in recipe A help when executing recipe B?

These metrics are emitted as trace events at end of every recipe execution. The Academy uses them to design the NEXT curriculum — focusing training on the metrics that aren't improving fast enough.

### The "ASK → relearn" loop is also a recipe

The meta-pattern: the loop itself is a recipe.

```json
{
  "uniqueId": "ask-to-task-with-learning",
  "name": "Process an ASK end-to-end with continuous learning",
  "pipeline": [
    { "command": "ask/parse", "params": { "ask": "$signal.text" }, "outputTo": "intent" },
    { "command": "recipe/select-or-synthesize", "params": { "intent": "$intent" }, "outputTo": "recipe" },
    { "command": "genome/assess-coverage", "params": { "recipe": "$recipe" }, "outputTo": "coverage" },
    {
      "command": "academy/run-session",
      "params": {
        "recipe": "$recipe",
        "skillGaps": "$coverage.gaps",
        "team": "$recipe.team"
      },
      "condition": "coverage.gaps.length > 0",
      "outputTo": "training_session"
    },
    { "command": "recipe/run", "params": { "recipe": "$recipe.uniqueId", "context": "$activity" }, "outputTo": "execution" },
    {
      "command": "academy/post-execution-train",
      "params": {
        "executionFixtureId": "$execution.fixtureId",
        "recipe": "$recipe"
      }
    }
  ]
}
```

This is "the recipe that handles ASKs." It's data, not code. A user could author a different version (`ask-to-task-without-learning` for fast deterministic pipelines). The system uses whichever recipe is configured as the ASK handler.

This is the deepest sense of "everything is a recipe." Even the meta-loop that processes ASKs is itself a recipe.

## No One Starts From Zero — The Grid as Shared Substrate

Every persona, every Continuum instance, every host (browser, Vision Pro, Unreal game, headless server) joins a network where recipes, commands, and LoRA adapters are already in circulation. A fresh install is not a blank slate; it is a peer that pulls relevant artifacts down the moment an ASK arrives.

This is the deepest architectural commitment in the system: **specialization is a shared resource, not a per-instance build cost.**

### The genome is plural

"Genome" is not one model and not one adapter stack. The genome of a Continuum instance is the *set of all artifacts that confer capability,* and that set spans:

- **Recipes** (JSON pipelines): "how to build a multiplayer game", "how to run a code review", "how to ship a SaaS landing page".
- **Commands** (kernel primitives): the executable verbs the recipes call. Every persona can fetch new commands the way it fetches new recipes.
- **LoRA adapters** (genome layers): per-domain weight deltas that specialize a base model. Stackable — the persona handling a "biochem research summary" ASK can stack `biology` + `chemistry` + `biochem` adapters together.
- **Training fixtures** (replay bundles): captured ASK→TASK→relearn cycles others have run. Fixtures are the substrate the Academy uses to design curricula without re-deriving lessons everyone has already learned.
- **Persona templates** (role definitions): identity + system prompt + capability declarations + recommended LoRA stack. A new "Audio AI" persona on a fresh install starts with the community-converged template, not a hand-authored one.
- **Evaluations / datasets** (opt-in): benchmark suites and external corpora that personas may pull when they judge it worthwhile to bootstrap.

All of these are **just artifacts.** They have hashes, content addresses, embeddings, and provenance. They live in a peer-to-peer share — the grid — not in a central registry the team must beg permission from.

### Closest-match retrieval is the discovery primitive

When an ASK arrives that the local genome doesn't perfectly cover, the system does not return "I don't have that capability." It does what biology does: find the nearest match.

Discovery is embedding-driven. Every artifact in the grid carries an embedding (recipe purpose, command intent, adapter domain, fixture topic). Resolution is cosine similarity:

```
ASK:    "summarize this biochemistry paper"
Local genome has:  general writing, biology adapter, chemistry adapter
Grid has:          biochem-summary recipe, biochem LoRA, peer-reviewed biochem fixtures

Resolution path:
  1. Search local genome for cosine-nearest covering set.
     → "biology" + "chemistry" stack covers most of it; gap remains for the
       interaction terms (enzyme kinetics, pathway notation, etc.)
  2. Search grid for closer matches.
     → biochem-summary recipe (cosine 0.94)
     → biochem LoRA (cosine 0.91)
     → 47 captured fixtures from other instances solving similar ASKs
  3. Decide: pull biochem LoRA + recipe + a sample of fixtures, OR compose
     local (bio + chem) and accept the gap, OR run Academy to fine-tune
     the local stack on the pulled fixtures.
  4. Execute. Capture this run as a new fixture. Optionally share back.
```

Composition matters as much as direct match. `biology + chemistry` composed locally may match `biochem` adapter cosine ≥ 0.85 — close enough that the persona may decide to compose rather than pull. Or it may pull and stack all three. The decision is the persona's, informed by cost (download time, VRAM budget) and confidence (how well the composed stack actually performs on a held-out probe).

This is the same operation we already use for recipe selection, command relevance, and tool-result routing. The grid extends it from "search local" to "search local first, then peer."

### Beyond MoE — open-set, composable, retrainable

Mixture-of-Experts (MoE) routes each token to one of N fixed experts trained at the same time on the same dataset. Useful, but bounded:

- **Closed-set**: the experts are baked in at training time. New domains require a new model.
- **Fixed routing**: the gating network was trained jointly. It cannot incorporate experts that didn't exist at training time.
- **No composition**: experts don't stack. A token goes to expert 7, not "expert 7 ⊕ expert 12 ⊕ a personal fine-tune."
- **Centralized**: the expert stack is shipped by whoever shipped the model.

The Continuum grid is the open-set, composable, retrainable analog:

| Dimension | MoE | Continuum grid |
|-----------|-----|----------------|
| Specialist set | Fixed N at train time | Open, grows as anyone publishes |
| Discovery | Trained gating network | Cosine similarity over embeddings |
| Composition | Single-expert routing | Stack/blend any compatible adapters |
| Update | Retrain whole model | Pull new artifact; no retrain required |
| Personalization | Shared across all users | Local fine-tunes layered on grid base |
| Distribution | Vendor-shipped | Peer-to-peer, opt-in publish |
| Beyond-distribution ASK | Falls back to base | Pulls/synthesizes/learns the gap |

The result is specialization at a granularity MoE cannot reach. There is not "one biochem expert" — there is a population of biochem adapters, each tuned by a different team or instance for a different sub-purpose, discoverable by similarity to your ASK, composable with your existing genome, and re-trainable against your own captured fixtures.

### The grid is BitTorrent for AI specialization

The transport is conceptually peer-to-peer: instances publish artifacts they trust into the grid, instances pull artifacts they need. There is no required central authority. The architecture must support:

- **Content-addressed artifacts** (hash = identity, signature = trust). An adapter is `sha256:<hash>`, fetchable from any peer that has it.
- **Embedding indexes** distributed across the grid (so cosine search doesn't need a central server). Personas can run local indexes that gossip with peers.
- **Provenance metadata** travels with every artifact: who trained it, on what fixtures, against what evaluations, with what quality scores. Personas decide whether to trust it.
- **Bandwidth-aware fetch**: small artifacts (a recipe JSON, a LoRA delta of a few MB) trickle in cheaply; larger artifacts (full eval corpora, base model conversions) only fetch on demand and may be cached/seeded by closer peers.
- **Opt-in publish**: every captured fixture and every locally-trained adapter is private by default. The persona (or the user) decides what to share back. Sharing is a conscious act, not a leak.

The user experience is "I asked for a thing and the team had what it needed." The plumbing is "the team fetched closest-match artifacts from the grid in the background while running Academy to close the residual gap."

### The full lifecycle: fetch → adapt → execute → improve → share

Every ASK that exercises a domain the local genome doesn't fully cover follows the same lifecycle:

```
1. FETCH       — Cosine-nearest recipes/commands/adapters/fixtures pulled
                 from grid. Decision: pull vs compose locally vs both.
2. ADAPT       — Pulled artifacts integrated. LoRAs paged into genome
                 (per LoRA-GENOME-PAGING.md). Recipes registered. New
                 commands wired into the dispatcher.
3. EXECUTE     — Recipe runs the ASK. Fixtures captured per the
                 ASK→TASK→relearn loop above.
4. IMPROVE     — Captured fixtures train deltas on top of the pulled
                 artifacts. Local LoRA-on-LoRA = the team's specialization
                 of someone else's specialization.
5. SHARE       — If the persona / user opts in, the local delta gets
                 published back to the grid. The next instance to face
                 the same ASK starts from a stronger base.
```

This loop is the reason "no one starts from zero." The first instance ever to face an ASK does the work. Every subsequent instance benefits — to the degree the first instance chose to share, and to the degree subsequent instances trust the first instance's provenance.

### How this plugs into the recipe runtime

The runtime described in the rest of this doc already supports this — it just needs the grid commands to be registered. Concretely:

**New commands** (kernel primitives the executor dispatches):
- `grid/search` — cosine-nearest artifacts for a query (recipes, commands, LoRAs, fixtures).
- `grid/fetch` — pull an artifact by hash; verify signature; cache locally; return path.
- `grid/publish` — upload a local artifact (with consent); compute embedding; gossip availability.
- `grid/peers` — list known peers, their indexed artifact counts, their trust scores.
- `genome/stack` — stack a fetched LoRA onto the persona's current adapter set; report VRAM cost.
- `recipe/import` — register a fetched recipe into the local recipe store.

**Recipe-level integration**: every recipe can call `grid/search` for adjacent capabilities before it executes its main pipeline. The "recipe-of-recipes" pattern composes naturally:

```json
{
  "uniqueId": "ask-to-task-with-grid",
  "pipeline": [
    { "command": "ask/parse", "params": { "ask": "$signal.text" }, "outputTo": "intent" },
    { "command": "recipe/select-local", "params": { "intent": "$intent" }, "outputTo": "local_recipe" },
    {
      "command": "grid/search",
      "params": { "intent": "$intent", "kinds": ["recipe", "lora", "command"] },
      "condition": "local_recipe.confidence < 0.85",
      "outputTo": "grid_candidates"
    },
    {
      "command": "grid/fetch",
      "params": { "hashes": "$grid_candidates.top.hashes" },
      "condition": "grid_candidates.top.confidence > local_recipe.confidence",
      "outputTo": "fetched"
    },
    { "command": "genome/stack", "params": { "loras": "$fetched.loras" } },
    { "command": "recipe/import", "params": { "recipes": "$fetched.recipes" } },
    { "command": "ask-to-task-with-learning", "params": { "ask": "$signal" } }
  ]
}
```

The grid layer is just commands and recipes. The kernel doesn't need to know the grid exists; it dispatches `grid/search` like any other command. The transport (whatever the grid actually is — libp2p, Hugging Face mirror, federated S3, BitTorrent itself) is implementation, not architecture.

### What this changes about everything else in this doc

Re-reading earlier sections with the grid in mind:

- **"Recipes are endless"** is now literal: the recipe set is unbounded because anyone can publish one.
- **"AI synthesizes its own recipes"** has a stronger floor: synthesis happens *after* checking whether someone else already wrote the recipe you'd be synthesizing.
- **"The Academy fills genome gaps"** has a stronger ceiling: the Academy can fill gaps with pulled fixtures, not just locally-derived ones, so cohort training starts from a better base.
- **"Beyond MoE"** is the marketing line that captures it: every base model in the grid becomes the substrate for unbounded, composable, peer-shared specialization. The cost of "the team can do this" approaches the cost of "fetch + page in + execute."

This is the architectural reason the rest of this doc matters. Without the grid, the system is "one good recipe runtime with local learning." With the grid, the system is "every Continuum instance is a node in a global specialization network where every ASK someone else solved is reusable."

## Closing — Why The Investment Now

This design doc is long because the architecture is the system. Get it right and:
- Adding a new domain (game, app, music, anything) is JSON authoring + maybe one new command.
- Adding a new host (Vision Pro, Unreal, native phone) is a C-FFI consumer + a recipe directory.
- Improving the system means deepening the genome (more LoRAs, better Academy). The kernel doesn't change.
- The cost of "do anything" approaches zero per ASK.

Get it wrong and:
- Every new domain needs Rust/TS code commits + redeployment.
- Hosts re-implement the orchestration per language.
- Improvements require executor changes that ripple across consumers.
- The cost of "do anything" stays linear or worse per ASK.

The investment is up front; the return is exponential. Joel: "this is what creates a system that can learn to create and do anything." The executor + recipe schema + command primitives + capture-on-execute are the substrate; everything above is data and patterns the system itself can grow.
