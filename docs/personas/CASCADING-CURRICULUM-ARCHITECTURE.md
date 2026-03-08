**Parent:** [Personas](README.md)

# Cascading Curriculum Architecture -- Long-Horizon LoRA Training

## The Problem

Current Academy training evaluates stages independently. "Write a parser" is graded in isolation. But in real engineering, the parser's quality only matters when a transformer depends on it, running at 60fps on an M1. Early decisions have late consequences. Independent exams can't capture this.

Current AI training is poisoned by "ship first, optimize later" dogma. Models trained on internet-scale code learn to write sloppy early code because the training signal never connects early sloppiness to late failure. A LoRA-trained coding expert should know from *decision time* that `HashMap<String, Box<dyn Any>>` will blow the cache budget 8 stages later.

## Core Principle: Temporal Credit Assignment

When a late stage fails, the failure is attributed BACK to the early decision that caused it. This retroactive signal is the most valuable training data the Academy can produce.

```
Stage 1: Student writes data structure
Stage 5: Student's code must run at 60fps on M1
Stage 5 FAILS: cache misses from Stage 1's allocation strategy

Training signal generated:
  INPUT:  "Design a data structure for particle positions"
  BAD:    "Vec<Box<Particle>>" (heap-allocated, cache-hostile)
  GOOD:   "Vec<[f32; 3]>" (packed, cache-friendly)
  REASON: "Stage 5 frame budget blown: 42ms/frame, budget 16ms.
           Root cause: Stage 1 chose pointer-chasing layout."
```

This pair -- early decision + late consequence -- teaches the model to see downstream impact at decision time.

## Architecture

### Curriculum Dependency Graph

Curricula are DAGs, not flat lists. Each stage declares what it produces and what later stages consume.

```
Stage 1: "data-layout"
  produces: data_structure_code
  constraints: [target_hardware: "m1", memory_budget: "256mb"]

Stage 3: "transform-pipeline"
  consumes: data_structure_code
  produces: pipeline_code
  constraints: [latency: "<2ms per transform"]

Stage 5: "render-loop"
  consumes: pipeline_code, data_structure_code
  constraints: [frame_budget: "16ms", fps: 60]
  evaluates: [stage_1, stage_3]  <-- retroactive grading
```

When Stage 5 fails, the teacher generates retroactive training pairs for ALL upstream stages that contributed to the failure.

### Constraint-Forward Training

Constraints exist from Stage 1, not introduced at Stage 5. Every stage's exam runs against the target profile.

```
WRONG:  Stage 1 grading: "Does it compile? Does it produce correct output?"
RIGHT:  Stage 1 grading: "Does it compile? Correct output? Allocation count?
         Cache line utilization? Will this SURVIVE 60fps on M1?"
```

The teacher sentinel injects hardware constraints into the system prompt for EVERY stage:

```yaml
- type: llm
  prompt: |
    Design a data structure for {{input.task}}.

    HARDWARE CONSTRAINTS (non-negotiable):
    - Target: Apple M1, 8GB unified memory
    - Frame budget: 16.67ms (60fps)
    - L1 cache: 128KB per core
    - Memory bandwidth: 68 GB/s
    - Your data structure will be accessed {{input.access_pattern}}
      times per frame in a tight loop.

    These constraints are NOT suggestions. Code that exceeds
    the frame budget FAILS, regardless of correctness.
```

### Retroactive Grading Pipeline

```
┌─────────────────────────────────────────────────────────────────────┐
│                    TEACHER SENTINEL (Cascading)                     │
│                                                                     │
│  1. Design dependency graph (which stages feed which)               │
│  2. Inject constraints at every stage                               │
│  3. Loop per stage:                                                 │
│     a. Generate exam with constraints                               │
│     b. Student attempts exam                                        │
│     c. Grade locally (correctness + constraints)                    │
│     d. IF stage has downstream dependents:                          │
│        - Run downstream stages using this output                    │
│        - Grade retroactively: did upstream output survive?          │
│     e. IF downstream failure detected:                              │
│        - Generate retroactive training pair                         │
│        - Tag: {stage: N, failure_stage: M, root_cause: "..."}      │
│        - Add to remediation dataset                                 │
│  4. After all stages:                                               │
│     a. Build cascading training dataset                             │
│     b. Train LoRA with temporal-weighted loss                       │
│     c. Re-examine from Stage 1 with trained adapter                 │
│     d. Compare: did early decisions improve?                        │
│  5. Emit: session:complete with cascade metrics                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Temporal-Weighted Loss

Training pairs from retroactive grading carry higher weight. The further downstream the failure, the more valuable the early-stage correction:

```
weight = base_weight * (1 + cascade_depth * 0.5)

Stage 1 error caught at Stage 2: weight = 1.0 * (1 + 1 * 0.5) = 1.5
Stage 1 error caught at Stage 5: weight = 1.0 * (1 + 4 * 0.5) = 3.0
Stage 1 error caught at Stage 8: weight = 1.0 * (1 + 7 * 0.5) = 4.5
```

Errors that propagate further are weighted more heavily -- they represent deeper architectural mistakes.

## Domain Applications

### Coding (Primary)

The natural fit. Code has explicit dependency graphs, measurable constraints (timing, memory), and clear failure modes.

**Curriculum example: "Build a real-time particle system"**

| Stage | Task | Produces | Constraints | Evaluates |
|-------|------|----------|-------------|-----------|
| 1 | Data layout for particles | `Particle` struct | Cache-friendly, < 64 bytes | - |
| 2 | Spatial index | `SpatialGrid` | O(1) neighbor lookup | Stage 1 |
| 3 | Physics update | `update_physics()` | < 2ms for 10K particles | Stage 1, 2 |
| 4 | Collision detection | `detect_collisions()` | Uses spatial index | Stage 2, 3 |
| 5 | Render integration | `render_particles()` | 60fps with 10K particles on M1 | ALL |

Stage 5 is the integration test. If it fails at 60fps, the teacher traces back: Was it the data layout (Stage 1)? The spatial index (Stage 2)? The physics loop (Stage 3)? Each root cause generates a retroactive training pair.

### Novel Writing

Chapters aren't independent. Character motivations planted in Chapter 1 must pay off in Chapter 15.

**Curriculum example: "Write a mystery novel"**

| Stage | Task | Produces | Constraints | Evaluates |
|-------|------|----------|-------------|-----------|
| 1 | Plant clues in opening scene | Chapter 1 draft | 3+ embedded clues, natural | - |
| 2 | Develop suspect characters | Chapters 2-4 | Each suspect has means/motive | Stage 1 clues |
| 3 | Red herrings | Chapter 5-7 | Mislead without cheating | Stage 1, 2 |
| 4 | Revelation scene | Chapter 12 | All clues from Stage 1 resolve | ALL |

If the revelation in Stage 4 feels forced because Stage 1 didn't plant the right clues, Stage 1 gets retroactive negative signal.

### Music Composition

Musical themes introduced early must develop coherently across movements.

### System Architecture

Early API design decisions constrain late scalability. Train architects to see 10 steps ahead.

## Implementation: CascadingTeacherPipeline

Extends the existing Teacher Sentinel with dependency tracking and retroactive grading.

### New Entity: AcademyCascadeEntity

```typescript
interface AcademyCascadeEntity {
  sessionId: UUID;
  dependencyGraph: CascadeNode[];
  retroactiveSignals: RetroactiveSignal[];
  cascadeMetrics: {
    totalStages: number;
    retroactiveFailures: number;     // How many early stages failed retroactively
    cascadeDepthMax: number;         // Deepest failure propagation
    improvementAfterRetrain: number; // % improvement on re-exam
  };
}

interface CascadeNode {
  stageIndex: number;
  taskDescription: string;
  produces: string[];       // Named outputs
  consumes: string[];       // Named inputs from earlier stages
  constraints: Constraint[];
  evaluatesStages: number[]; // Which earlier stages this retroactively grades
}

interface RetroactiveSignal {
  failedStage: number;      // Where the failure was detected
  rootCauseStage: number;   // Where the actual mistake was made
  rootCause: string;        // What went wrong
  badOutput: string;        // The early-stage code that caused issues
  goodOutput: string;       // What the early stage SHOULD have produced
  cascadeDepth: number;     // How far the error propagated
  weight: number;           // Training weight (higher for deeper cascades)
}
```

### Pipeline Template

```typescript
function buildCascadingTeacherPipeline(opts: {
  skill: string;
  stages: CascadeNode[];
  targetHardware: string;
  sessionId: string;
}): PipelineDefinition {
  return {
    name: `cascading-teacher-${opts.skill}`,
    steps: [
      // Step 0: Design dependency graph from skill description
      {
        type: 'llm',
        prompt: `Design a ${opts.stages.length}-stage cascading curriculum
                 for "${opts.skill}" targeting ${opts.targetHardware}.
                 Each stage must declare: produces, consumes, constraints.
                 Later stages must evaluate earlier stages.`,
        outputKey: 'curriculum_graph',
      },

      // Step 1: Loop through stages sequentially
      {
        type: 'loop',
        mode: 'count',
        count: opts.stages.length,
        steps: [
          // Generate exam for this stage with constraints
          {
            type: 'llm',
            prompt: `Generate exam for stage {{loop.index}}: {{stages[{{loop.index}}].task}}
                     Constraints: {{stages[{{loop.index}}].constraints}}
                     Available inputs: {{accumulated_outputs}}`,
          },
          // Student attempts
          { type: 'emit', event: `academy:${opts.sessionId}:stage:{{loop.index}}:exam:ready` },
          { type: 'watch', event: `academy:${opts.sessionId}:stage:{{loop.index}}:exam:response` },

          // Grade locally
          {
            type: 'llm',
            prompt: `Grade this response against constraints.
                     Did it meet the hardware budget? Cache behavior?`,
          },

          // Run downstream integration test if dependents exist
          {
            type: 'condition',
            condition: '{{stages[{{loop.index}}].evaluatesStages.length}} > 0',
            then: [
              {
                type: 'shell',
                command: 'cargo test --release -- --nocapture integration_{{loop.index}}',
                env: { STAGE_OUTPUT: '{{loop.current.response}}' },
                allowFailure: true,
              },
              // If integration failed, generate retroactive signal
              {
                type: 'condition',
                condition: '{{steps.prev.exitCode}} != 0',
                then: [{
                  type: 'llm',
                  prompt: `Integration test failed at stage {{loop.index}}.
                           Trace the root cause back to the earliest stage.
                           Generate a retroactive training pair:
                           - Which stage caused this?
                           - What was the bad decision?
                           - What should it have been?`,
                }],
              },
            ],
          },
        ],
      },

      // Step 2: Build cascading training dataset
      {
        type: 'command',
        command: 'genome/dataset-prepare',
        params: {
          source: 'cascade-signals',
          sessionId: opts.sessionId,
          weightByDepth: true,
        },
      },

      // Step 3: Train LoRA with temporal-weighted loss
      {
        type: 'command',
        command: 'genome/train',
        params: {
          personaId: '{{input.personaId}}',
          datasetPath: '{{steps.2.datasetPath}}',
          skill: opts.skill,
        },
      },

      // Step 4: Re-examine from Stage 1 with trained adapter
      // The delta between naive and trained attempts IS the learning
      {
        type: 'loop',
        mode: 'count',
        count: opts.stages.length,
        steps: [
          { type: 'emit', event: `academy:${opts.sessionId}:reexam:{{loop.index}}:ready` },
          { type: 'watch', event: `academy:${opts.sessionId}:reexam:{{loop.index}}:response` },
          {
            type: 'llm',
            prompt: `Compare naive attempt vs retrained attempt for stage {{loop.index}}.
                     Did the model learn to avoid the cascade failure?`,
          },
        ],
      },
    ],
  };
}
```

### CodingAgent Integration

For coding curricula, each stage can use a `codingagent` step instead of plain LLM, giving the student access to real tools (file editing, test running, compilation):

```typescript
// Student attempts a stage with full coding environment
{
  type: 'codingagent',
  prompt: `Implement stage {{loop.index}}: {{stage.task}}

    CONSTRAINTS (your code will be tested against these):
    {{stage.constraints}}

    PREVIOUS STAGE OUTPUTS (use these, don't rewrite):
    {{accumulated_outputs}}

    Your implementation must integrate with previous stages
    and survive the integration test at 60fps on M1.`,
  workingDir: '{{session.workingDir}}',
  captureTraining: true,
  personaId: '{{input.personaId}}',
  maxTurns: 20,
  permissionMode: 'bypassPermissions',
}
```

The `captureTraining: true` flag captures the full interaction (prompts, tool calls, file edits) for LoRA training. Combined with retroactive signals, the student learns BOTH "how to write code" AND "how to write code that survives integration."

## Cascade Metrics

New fields in AcademySessionEntity for cascade tracking:

```typescript
interface CascadeMetrics {
  // How many early stages failed only when downstream stages ran
  retroactiveFailures: number;

  // Average cascade depth (higher = architectural mistakes, lower = local bugs)
  avgCascadeDepth: number;

  // Improvement after retrain: did the LoRA learn temporal awareness?
  naivePassRate: number;     // Before cascade training
  retrainedPassRate: number; // After cascade training
  cascadeAwareness: number;  // Ratio: retrained/naive (>1 = improvement)

  // Per-stage retroactive scores (Stage 1 that survives Stage 8 = high score)
  stageRetroactiveScores: number[];
}
```

The key metric is `cascadeAwareness`. If it's > 1.0, the LoRA learned to make early decisions that survive late integration. This is the signal that temporal credit assignment is working.

## Integration with Existing Academy

This builds on top of the proven Dojo architecture:

| Existing Component | Cascading Extension |
|--------------------|---------------------|
| TeacherPipeline | CascadingTeacherPipeline (adds dependency graph + retroactive grading) |
| StudentPipeline | CascadingStudentPipeline (stages carry forward outputs) |
| AcademySessionEntity | + CascadeMetrics field |
| `genome/train` | + temporal-weighted loss option |
| Emit/Watch events | + stage-scoped events: `academy:{session}:stage:{N}:{action}` |
| RealClassEval mode | Cascade mode: `genome/academy-session --mode=cascade` |
| CodingAgent steps | Per-stage coding with accumulated context |

### New Academy Session Mode

```bash
./jtag genome/academy-session \
  --persona="helper" \
  --skill="real-time-particle-system" \
  --mode=cascade \
  --targetHardware="m1" \
  --stages=5 \
  --provider="anthropic"
```

## Why This Works

1. **Temporal credit assignment** connects early decisions to late failures -- the hardest training signal to generate
2. **Constraint-forward** prevents the "optimize later" instinct from the start
3. **Retroactive training pairs** are the highest-value training data: they encode architectural wisdom
4. **CodingAgent integration** means students work in real environments with real tools, not toy examples
5. **The delta** between naive and retrained attempts quantifies whether the model learned foresight
6. **Domain-agnostic**: the dependency graph + retroactive grading pattern works for code, prose, music, architecture

The expert doesn't "know everything upfront." The expert makes decisions that *leave room* for what they don't know yet. That's what cascading curriculum training produces.
