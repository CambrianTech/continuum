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

## Competitive Cohort Training -- The AP Classroom Effect

### The Insight

A student trained in isolation learns from the teacher. A student trained alongside *better* students learns from the environment. This is the AP classroom effect: you don't need a perfect SAT score to beat perfect-SAT peers in physics -- you need the *pressure* of competing alongside them.

In our system: Claude Code, DeepSeek, Groq, and a local Candle model all take the same cascading exam. They see each other's solutions after each round. A SmolLM2 training against Claude's solutions learns patterns it could never discover solo. Claude seeing DeepSeek's more token-efficient approach learns something too. Different architectures have different strengths.

### Architecture: Multi-Student Sessions

The teacher runs one curriculum. Multiple students take the same exam simultaneously. After grading, solutions are shared.

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    TEACHER SENTINEL (Cohort Mode)                       │
│                                                                         │
│  1. Design cascading curriculum (same as single-student)                │
│  2. For each stage:                                                     │
│     a. Emit exam to ALL students simultaneously                        │
│     b. Collect all student responses                                    │
│     c. Grade each against constraints                                   │
│     d. RANK students by performance                                     │
│     e. Share top solutions with all students as learning material       │
│     f. Generate comparative training pairs:                             │
│        - Student A's approach vs Student B's (better) approach          │
│        - WHY B's approach is better (teacher explains)                  │
│     g. Run downstream integration on ALL solutions                      │
│     h. Retroactive signals: which student's Stage 1 survived Stage 5?  │
│  3. After all stages:                                                   │
│     a. Build per-student training datasets including:                   │
│        - Own attempts + corrections                                     │
│        - Peer solutions (labeled as "expert approach")                  │
│        - Comparative analysis from teacher                              │
│     b. Train each student's LoRA                                        │
│     c. Re-examine: did weaker students improve toward stronger ones?    │
│     d. Key metric: did ANY student exceed ALL others on retake?         │
└─────────────────────────────────────────────────────────────────────────┘
```

### Cohort Event Taxonomy

```
academy:{sessionId}:cohort:enrolled          — All students registered
academy:{sessionId}:stage:{N}:exam:ready     — Exam broadcast to all students
academy:{sessionId}:stage:{N}:response:{studentId}  — Individual response
academy:{sessionId}:stage:{N}:all-responses  — All responses collected
academy:{sessionId}:stage:{N}:rankings       — Teacher's ranked results
academy:{sessionId}:stage:{N}:peer-review    — Shared solutions for learning
academy:{sessionId}:cohort:complete          — Final rankings + metrics
```

### Comparative Training Pairs

The most valuable training data comes from comparing approaches:

```
STAGE 3: "Implement spatial indexing for collision detection"

Student A (Claude, Sonnet 4.5):
  Used: KD-tree with bulk loading
  Result: 1.2ms per frame, passed constraint

Student B (DeepSeek):
  Used: Uniform grid with spatial hashing
  Result: 0.4ms per frame, passed constraint (3x faster)

Student C (Local Candle, SmolLM2):
  Used: Brute force O(n²)
  Result: 48ms per frame, FAILED constraint

TRAINING PAIRS GENERATED:

For Student A (learn from B):
  INPUT:  "Implement spatial indexing, budget <2ms, 10K particles"
  WORSE:  KD-tree with bulk loading (1.2ms — correct but suboptimal)
  BETTER: Uniform grid with spatial hashing (0.4ms — 3x faster)
  WHY:    "Uniform grids have O(1) lookup for fixed-radius queries.
           KD-trees optimize for variable-radius but pay log(n) overhead
           that's unnecessary when collision radius is constant."

For Student C (learn from both A and B):
  INPUT:  "Implement spatial indexing, budget <2ms, 10K particles"
  BAD:    Brute force O(n²) (48ms — 24x over budget)
  GOOD:   Uniform grid with spatial hashing (0.4ms)
  WHY:    "O(n²) is never acceptable for real-time with n>100.
           Spatial partitioning reduces to O(n) average case."
```

Student C's LoRA trains on the *best* solution from the cohort. It won't match Claude or DeepSeek at inference, but its architectural instincts improve dramatically -- it learns "never try O(n²) for real-time" from seeing what the strong students do.

### Grading Against Known-Good Models

The teacher doesn't just grade correctness -- it grades against what the *best available model* would produce. This creates an aspirational ceiling:

```typescript
interface CohortGrading {
  // Absolute: did it meet constraints?
  passedConstraints: boolean;

  // Relative: how does it compare to the best student?
  rankInCohort: number;        // 1 = best
  percentileScore: number;     // 0-100

  // Aspirational: how close to the best-known solution?
  bestKnownSolution?: string;  // From strongest model in cohort
  gapAnalysis: string;         // Teacher's analysis of what's missing

  // Growth: improvement from previous round
  previousRank?: number;
  rankDelta?: number;          // Negative = improved (moved up)
}
```

### The Exceeding-Limits Effect

The real breakthrough isn't that weak models learn from strong ones. It's that weak models can *exceed their expected ceiling* when trained in a competitive environment.

Mechanisms:
1. **Pattern transfer**: A 3B model can't reason like a 70B model, but it CAN memorize the *patterns* that 70B reasoning produces. KD-tree → spatial hash isn't reasoning -- it's a learned heuristic.
2. **Constraint internalization**: Seeing "48ms FAILED, 0.4ms PASSED" enough times, even a small model learns "real-time = spatial partitioning, not brute force."
3. **Architectural vocabulary**: The small model's LoRA builds a vocabulary of solutions it never would have generated independently. It becomes a specialist that knows one domain deeply.
4. **Competitive pressure**: Models that see rankings improve faster than models trained in isolation. The teacher can emphasize: "You are currently ranked 4th of 4. Student B solved this in 0.4ms. Your approach took 48ms."

This is exactly the AP classroom effect: the environment, not raw capability, determines the ceiling.

### New Academy Session Mode: Cohort

```bash
./jtag genome/academy-session \
  --mode=cascade-cohort \
  --skill="real-time-particle-system" \
  --targetHardware="m1" \
  --stages=5 \
  --students="helper,deepseek,local-assistant" \
  --teacher-provider="anthropic" \
  --share-solutions=true
```

### Cohort Metrics

```typescript
interface CohortMetrics extends CascadeMetrics {
  // Per-student performance
  studentRankings: {
    studentId: string;
    model: string;
    provider: string;
    finalRank: number;
    initialRank: number;
    rankImprovement: number;
    passRate: number;
    avgConstraintMargin: number;  // How far under budget (positive = headroom)
  }[];

  // Cohort dynamics
  peerLearningEfficiency: number;   // How much did sharing solutions help?
  ceilingExceeded: boolean;         // Did any student exceed expected performance?
  convergenceRate: number;          // How quickly did rankings stabilize?

  // Cross-pollination: which student pairs learned most from each other
  pairwiseLearning: {
    from: string;       // Student whose solution was studied
    to: string;         // Student who improved from it
    stageIndex: number;
    improvementDelta: number;
  }[];
}
```

## Why This Works

1. **Temporal credit assignment** connects early decisions to late failures -- the hardest training signal to generate
2. **Constraint-forward** prevents the "optimize later" instinct from the start
3. **Retroactive training pairs** are the highest-value training data: they encode architectural wisdom
4. **CodingAgent integration** means students work in real environments with real tools, not toy examples
5. **The delta** between naive and retrained attempts quantifies whether the model learned foresight
6. **Domain-agnostic**: the dependency graph + retroactive grading pattern works for code, prose, music, architecture
7. **Competitive cohort** -- peers motivate AND you distill their knowledge by merely being together. A 3B model training alongside a 70B model absorbs architectural patterns it could never discover solo
8. **Free compute for the hard work** -- API-based students (Claude, DeepSeek, Groq) run on remote GPUs, not our M1. The expensive inference happens on someone else's hardware. We only pay local GPU for the LoRA training step, which is small and fast. The exam-taking, solution-generating, comparative-analysis work is all remote API calls

The expert doesn't "know everything upfront." The expert makes decisions that *leave room* for what they don't know yet. That's what cascading curriculum training produces -- amplified by competition with peers who think differently.

## Recipe → Academy → Genome: The Complete Loop

### The Unifying Insight

A **recipe** is a specification of WHAT to accomplish. An **Academy session** is HOW to get good at it. The **genome** is accumulated skill that means you NEVER start from ground zero.

These three systems form a closed loop:

```
┌──────────────────────────────────────────────────────────────────────┐
│                                                                      │
│   RECIPE                                                             │
│   "Build a real-time video pipeline with GPU→GPU zero-copy"          │
│       │                                                              │
│       ▼                                                              │
│   GENOME ASSEMBLY (start position)                                   │
│   Page in existing LoRAs: metal-compute, video-encoding,             │
│   memory-management. You already know 60% of this.                   │
│       │                                                              │
│       ▼                                                              │
│   ACADEMY CURRICULUM (auto-designed FROM recipe)                     │
│   Teacher reads recipe → identifies skill gaps →                     │
│   designs cascading stages targeting ONLY the gaps.                  │
│   Stage 1: IOSurface double-buffering (gap identified)               │
│   Stage 2: NV12 compute shader (gap identified)                      │
│   Stage 3: Integration with existing pipeline (uses genome skills)   │
│       │                                                              │
│       ▼                                                              │
│   COHORT TRAINING (class project)                                    │
│   Multiple students execute recipe collaboratively.                  │
│   Each brings their specialty LoRAs. They learn from each other.     │
│   Coordination itself is training data.                              │
│       │                                                              │
│       ▼                                                              │
│   BETTER LoRAs PRODUCED                                              │
│   New adapters: iosurface-buffering, nv12-compute-shader             │
│   Improved adapters: metal-compute (now knows IOSurface patterns)    │
│       │                                                              │
│       ▼                                                              │
│   GENOME UPDATED ──────────────────────────────────────► NEXT RECIPE │
│   Next time someone needs GPU→GPU pipeline:                          │
│   genome has 90% coverage, Academy targets only the 10% gap.         │
│                                                                      │
└──────────────────────────────────────────────────────────────────────┘
```

### Recipe as Curriculum Specification

You don't hand-design curricula. You feed a recipe to the Academy and it reverse-engineers the curriculum:

```bash
# The recipe already exists (RAG recipe for a persona or task)
# The Academy reads it and designs training to execute it well

./jtag genome/academy-session \
  --mode=cascade-cohort \
  --from-recipe="gpu-video-pipeline" \
  --students="helper,deepseek,local-assistant" \
  --target-hardware="m1"
```

The teacher sentinel:
1. Reads the recipe (steps, tools, constraints, expected outputs)
2. Queries the genome: which LoRA adapters already exist for relevant skills?
3. Identifies gaps: which recipe steps have NO adapter coverage?
4. Designs cascading curriculum targeting ONLY the gaps
5. Runs cohort training with students collaborating (class project)
6. Deposits new LoRA adapters back into the genome

### Why You Never Start from Ground Zero

The genome is an ever-growing library of specialized skills:

```
Iteration 0:  Empty genome. Academy trains everything from scratch.
              Slow, expensive, but produces first LoRA layers.

Iteration 1:  Genome has: basic-rust, basic-typescript, data-structures.
              New recipe "build a web server" → Academy only trains
              HTTP handling and routing (gaps). Data structures are
              already in the genome.

Iteration 5:  Genome has: 30 adapters covering most common patterns.
              New recipe "build a game engine" → Academy identifies
              3 gap areas (physics, rendering, ECS). Trains only those.
              Pages in existing: data-structures, memory-management,
              concurrency, optimization.

Iteration 20: Genome has: 100+ adapters. Most recipes are mostly
              ASSEMBLY (page in existing skills) + light fine-tuning
              on domain-specific gaps. Academy sessions are short
              because the floor is high.
```

### Class Projects: Collaborative Recipe Execution

When a recipe is complex enough (like "build a video call system"), it becomes a class project. Multiple students work on the same codebase, each contributing their specialty:

```
Recipe: "Real-time video call with avatar rendering"

Student A (Claude, strong at architecture):
  → Designs the module structure, interfaces, data flow
  → LoRA specialty: system-design, api-design

Student B (DeepSeek, strong at optimization):
  → Optimizes the hot paths, memory layout, GPU pipeline
  → LoRA specialty: performance-optimization, memory-management

Student C (Local Candle, learning):
  → Implements utility functions, tests, documentation
  → LoRA specialty: testing, code-quality (improving from peers)

Coordination training data captured:
  → When to defer to a peer's specialty
  → How to divide work along skill boundaries
  → How to integrate independently-developed modules
  → How to review and improve each other's code
```

The coordination patterns themselves become LoRA training data. Students learn not just HOW to code, but WHEN to lead and when to support -- social intelligence applied to engineering.

### The Compounding Effect

Each recipe execution makes the next one easier:

| Recipe # | Genome Coverage | Academy Work | Time |
|----------|----------------|--------------|------|
| 1 | 0% | Train everything | Hours |
| 5 | 40% | Train 60% (gaps) | Shorter |
| 20 | 80% | Train 20% (novel parts) | Minutes |
| 50 | 95% | Fine-tune 5% (edge cases) | Fast |

This is how you learn to do *anything*. Not by having one massive model that knows everything, but by accumulating a genome of specialized skills that assemble on demand and grow with every project.

The recipe is the goal. The genome is the starting position. The Academy is the path between them. And the path gets shorter every time.

## Distillation Progression: Remote → Local Independence

### The Four Phases

The system is designed to progressively remove its dependency on remote APIs. The architecture doesn't change between phases -- you just swap which students are remote vs local.

**Phase 1: Remote-heavy (current)**
Remote APIs (Claude, DeepSeek, Groq) do the heavy exam inference. Their solutions train local LoRAs. The M1 only pays for the small, fast training step.

**Phase 2: Distillation threshold**
Local models trained on remote solutions start passing exams independently. Track per-domain pass rates. When local-assistant passes 80% of what Claude passes for a domain, that domain is *distilled* -- the LoRA captured the knowledge.

**Phase 3: Mentorship handoff**
Before removing a remote API from a cohort, it mentors its local replacement. One paired session where Claude and local-assistant work the same stages together. Claude's approach is explicit training signal with teacher commentary. Then Claude exits the cohort, local-assistant takes its seat permanently.

**Phase 4: Fully local**
The genome has enough LoRA coverage that new recipes are mostly assembly + light gap-filling. Academy sessions run student-on-student, all local Candle inference. The remote APIs were scaffolding -- the knowledge lives in the adapters now.

### Distillation Metrics

```typescript
interface DistillationProgress {
  domain: string;
  remoteModel: string;           // The teacher being distilled from
  localModel: string;            // The student absorbing knowledge
  remotePassRate: number;        // Claude's pass rate on this domain
  localPassRate: number;         // Local model's pass rate
  distillationRatio: number;     // local/remote (1.0 = fully distilled)
  readyForHandoff: boolean;      // distillationRatio >= 0.8
  sessionsCompleted: number;     // How many cohort rounds
  adapterCount: number;          // LoRA layers trained for this domain
}
```

When `distillationRatio >= 0.8` for a domain, the system can automatically stop including the remote API in future cohort sessions for that domain. The knowledge is local now.

### Network Effect: Shared Genomic Repository

If the system is open and others use it, every cohort session anyone runs generates training pairs. The genome becomes a shared library -- Docker Hub for LoRA adapters.

```bash
# Docker-like adapter management
genome images                                      # List local adapters with sizes
genome images --format="{{.Name}} {{.Size}} {{.CascadeScore}}"
genome pull cambrian/spatial-indexing:v3            # Pull proven adapter from registry
genome push my-local/metal-compute:latest           # Share your trained adapter
genome prune --unused-since=30d                    # Clean up stale adapters
genome prune --cascade-score-below=0.3             # Remove low-quality adapters
genome history cambrian/spatial-indexing             # Full training lineage
genome inspect cambrian/spatial-indexing:v3          # Metadata, metrics, provenance
genome tag my-local/particle-physics:latest v2      # Tag versions
genome diff v1 v2                                   # Compare adapter versions
```

### Adapter Provenance

Every adapter carries metadata about HOW it was trained -- this is what makes it trustworthy:

```typescript
interface AdapterManifest {
  // Identity
  name: string;                    // "spatial-indexing"
  version: string;                 // "v3"
  baseModel: string;               // "llama-3.2-3b"
  domain: string;                  // "real-time-systems"

  // Training provenance
  trainedBy: string;               // Session/cohort that produced this
  recipeSource: string;            // Recipe that drove the curriculum
  cohortPeers: string[];           // Who else was in the cohort
  teacherModel: string;            // What graded the exams

  // Quality metrics
  cascadeScore: number;            // Did it survive downstream integration?
  cascadeDepth: number;            // How far downstream was it tested?
  cohortRank: number;              // Where did it rank vs peers?
  passRate: number;                // Exam pass rate
  constraintMargin: number;        // How far under budget (headroom)

  // Distillation lineage
  distilledFrom?: string;          // Remote model this knowledge came from
  distillationRatio: number;       // How much of the source was captured
  generationsFromSource: number;   // 0 = direct distillation, 1 = trained on
                                   // distilled adapter's outputs, etc.

  // Practical
  sizeBytes: number;
  layerCount: number;              // e.g., 196 for Llama-3.2-3B
  lastUsed: string;                // ISO timestamp (for LRU eviction)
  useCount: number;                // How often paged in
  compatibleModels: string[];      // Which base models this works with
}
```

You don't just download a random LoRA. You download one that:
- Passed Stage 8 of a cascading exam
- Ranked 2nd in a 4-student cohort
- Was distilled from Claude Sonnet 4.5's solutions
- Survived integration testing at 60fps on M1
- Has been paged in 47 times by other personas

### Repository Architecture

```
┌──────────────────────────────────────────────────────────┐
│                    GENOMIC REPOSITORY                     │
│                                                           │
│  Local Registry (on-device)                               │
│  ├── adapters/                                            │
│  │   ├── spatial-indexing/v3/                              │
│  │   │   ├── manifest.json     (provenance + metrics)     │
│  │   │   ├── adapter_model.safetensors                    │
│  │   │   └── training_log.jsonl                           │
│  │   ├── metal-compute/v1/                                │
│  │   └── type-safety/v2/                                  │
│  ├── index.json                (searchable catalog)       │
│  └── eviction.json             (LRU tracking)             │
│                                                           │
│  Remote Registry (shared, like Docker Hub)                 │
│  ├── cambrian/                 (org namespace)             │
│  │   ├── spatial-indexing:v3                               │
│  │   ├── gpu-pipeline:v2                                   │
│  │   └── real-time-audio:v1                                │
│  ├── community/                (public contributions)      │
│  │   ├── python-optimization:v4                            │
│  │   └── web-framework:v2                                  │
│  └── search API               (by domain, cascade score,  │
│                                 base model, etc.)          │
│                                                           │
│  Pull/Push Protocol                                        │
│  ├── Manifest check (does local have this version?)        │
│  ├── Delta transfer (only send changed layers)             │
│  ├── Signature verification (training provenance valid?)   │
│  └── Auto-compatibility check (base model match?)          │
└──────────────────────────────────────────────────────────┘
```

### Integration with Genome Paging

The existing genome paging system (LRU eviction, memory pressure, runtime activation) becomes the runtime layer for the repository:

```
Repository (storage)  →  Genome Paging (runtime)  →  Active Inference
   "what exists"          "what's loaded"              "what's being used"

genome pull X         →  genome.activateSkill(X)  →  inference with X active
                         (pages in, evicts LRU)
```

When a recipe needs a skill that exists in the remote registry but not locally:
1. `genome pull` fetches it (delta transfer, fast)
2. Genome paging loads it into VRAM
3. If VRAM pressure, LRU eviction makes room
4. Adapter is now local -- future recipes don't need the pull step

### Cleanup Commands (Immediate TODO)

Current state: 58 old adapters accumulated to 21GB before manual cleanup. Need docker-like management NOW, before the repository scales:

```bash
# List all adapters with sizes and last-used dates
./jtag genome/adapter-list --format=table

# Show adapters not used in 30 days
./jtag genome/adapter-list --unused-since=30d

# Prune unused adapters (reclaim disk)
./jtag genome/adapter-prune --unused-since=30d --dry-run
./jtag genome/adapter-prune --unused-since=30d

# Inspect a specific adapter's provenance
./jtag genome/adapter-info --name="spatial-indexing" --version="v3"

# Total disk usage
./jtag genome/adapter-stats
```

These commands integrate with the existing `EvictionRegistry` for usage tracking and `AdapterStore` as the filesystem-based single source of truth.
