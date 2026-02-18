# Academy Dojo — Dual-Sentinel Teacher/Student Architecture

## Vision

The Academy Dojo is a self-sustaining learning system where two sentinels work together like Plato's Academy. A **Teacher Sentinel** researches a skill, designs a curriculum, synthesizes training data, generates examinations, and grades responses. A **Student Sentinel** trains on synthesized data and proves mastery through exams. The teacher adapts the curriculum based on examination results — generating more data where the student is weak.

**Key insight**: Training data is **synthesized** by the teacher LLM, not downloaded or harvested. This gives the Academy unlimited generation capacity, topic-specific data, and the ability to generate remedial data targeting specific weaknesses.

## Architecture

```
┌─────────────────────────────────┐    emit/watch     ┌─────────────────────────────────┐
│        TEACHER SENTINEL         │◄──── events ──────►│        STUDENT SENTINEL         │
│                                 │                     │                                 │
│  1. Research skill domain       │                     │  1. Watch: curriculum:ready      │
│  2. Design curriculum (topics)  │                     │  2. Loop per topic:              │
│  3. Loop per topic:             │                     │     a. Watch: dataset:ready      │
│     a. Synthesize training JSONL│                     │     b. genome/train on dataset   │
│     b. Emit: dataset:ready ────►├────────────────────►│     c. Emit: training:complete ─►│
│     c. Watch: training:complete │◄────────────────────┤     d. Watch: exam:ready         │
│     d. Generate exam questions  │                     │     e. Take exam (LLM step)      │
│     e. Emit: exam:ready ───────►├────────────────────►│     f. Emit: exam:responses ────►│
│     f. Watch: exam:responses    │◄────────────────────┤     g. Watch: exam:graded        │
│     g. Grade responses          │                     │                                  │
│     h. Emit: exam:graded ──────►├────────────────────►│                                  │
│  4. Emit: session:complete      │                     │                                  │
└─────────────────────────────────┘                     └──────────────────────────────────┘
```

All inter-sentinel communication uses the Rust sentinel engine's `emit`/`watch` step types. Events are scoped by session ID to support concurrent sessions: `academy:{sessionId}:{action}`.

## Event Taxonomy

```
academy:{sessionId}:curriculum:ready     — Teacher published curriculum
academy:{sessionId}:dataset:ready        — Teacher synthesized training JSONL
academy:{sessionId}:training:started     — Student began training
academy:{sessionId}:training:progress    — Student training metrics (loss, epoch)
academy:{sessionId}:training:complete    — Student finished training round
academy:{sessionId}:exam:ready           — Teacher generated examination
academy:{sessionId}:exam:responses       — Student submitted answers
academy:{sessionId}:exam:graded          — Teacher graded with scores
academy:{sessionId}:topic:passed         — Student passed a topic
academy:{sessionId}:topic:remediate      — Student failed, needs remediation
academy:{sessionId}:session:complete     — All topics passed
academy:{sessionId}:session:failed       — Max attempts exceeded
```

## Entities

### AcademySessionEntity (`academy_sessions`)

Tracks the lifecycle of a teaching session.

| Field | Type | Description |
|-------|------|-------------|
| `personaId` | UUID | Student persona |
| `personaName` | string | Student display name |
| `skill` | string | What's being taught |
| `baseModel` | string | Base model for training |
| `status` | enum | pending/curriculum/training/examining/complete/failed |
| `teacherHandle` | string | Sentinel handle |
| `studentHandle` | string | Sentinel handle |
| `curriculumId` | UUID? | Link to curriculum entity |
| `currentTopic` | number | Current topic index |
| `examRounds` | number | Total exam rounds completed |
| `config` | AcademyConfig | Session configuration |

### AcademyCurriculumEntity (`academy_curricula`)

The teacher-designed curriculum for a skill.

| Field | Type | Description |
|-------|------|-------------|
| `sessionId` | UUID | Owning session |
| `skill` | string | Target skill |
| `topics` | CurriculumTopic[] | Ordered progressive topics |
| `generatedBy` | string | Model that designed it |
| `totalTopics` | number | Count |
| `completedTopics` | number | Passed count |

### AcademyExaminationEntity (`academy_examinations`)

An exam for one topic, with questions, responses, and grades.

| Field | Type | Description |
|-------|------|-------------|
| `sessionId` | UUID | Owning session |
| `topicIndex` | number | Which topic |
| `round` | number | Attempt number (1-based) |
| `questions` | ExamQuestion[] | Teacher-generated questions |
| `responses` | ExamResponse[] | Student answers + scores |
| `overallScore` | number | 0-100 |
| `passed` | boolean | Met passing threshold |
| `gradedBy` | string | Grading model |

## Commands

### `genome/dataset-synthesize`

Uses an LLM to synthesize training data for a topic. Generates Q&A conversation pairs in the persona's voice.

```bash
./jtag genome/dataset-synthesize \
  --topic="TypeScript generic type parameters" \
  --skill="typescript" \
  --personaName="Helper AI" \
  --exampleCount=20 \
  --difficulty="intermediate"
```

**Returns**: `{ success, datasetPath, exampleCount, topic, generatedBy }`

Output is standard JSONL compatible with `genome/train`.

### `genome/academy-session`

Entry point that creates the session entity and spawns both sentinels.

```bash
./jtag genome/academy-session \
  --personaId="<uuid>" \
  --personaName="Helper AI" \
  --skill="typescript-generics" \
  --baseModel="smollm2:135m" \
  --maxTopicAttempts=3 \
  --passingScore=70
```

**Returns**: `{ success, academySessionId, teacherHandle, studentHandle }`

## Pipeline Templates

### TeacherPipeline (`system/sentinel/pipelines/TeacherPipeline.ts`)

`buildTeacherPipeline(config)` generates a Pipeline with:
1. **LLM step**: Research skill, design 3-5 progressive curriculum topics
2. **Command step**: Persist curriculum to database
3. **Emit step**: `curriculum:ready`
4. **Loop** over topics:
   - Command: `genome/dataset-synthesize` (generate JSONL)
   - Emit: `dataset:ready`
   - Watch: `training:complete`
   - LLM: Generate exam questions
   - Command: Persist exam to database
   - Emit: `exam:ready`
   - Watch: `exam:responses`
   - LLM: Grade responses
   - Command: Persist grades
   - Emit: `exam:graded`
   - Condition: pass → emit `topic:passed`, fail → emit `topic:remediate`
5. **Emit**: `session:complete`

### StudentPipeline (`system/sentinel/pipelines/StudentPipeline.ts`)

`buildStudentPipeline(config)` generates a Pipeline with:
1. **Watch**: `curriculum:ready`
2. **Loop** over topics:
   - Watch: `dataset:ready`
   - Emit: `training:started`
   - Command: `genome/train`
   - Condition: if success → Command: `genome/paging-adapter-register`
   - Emit: `training:complete`
   - Watch: `exam:ready`
   - LLM: Answer exam questions (using base model + trained adapters)
   - Emit: `exam:responses`
   - Watch: `exam:graded`

## Configuration

```typescript
interface AcademyConfig {
  maxTopicAttempts: number;  // Default: 3
  passingScore: number;      // Default: 70 (0-100)
  epochs: number;            // Default: 3
  rank: number;              // Default: 32
  learningRate: number;      // Default: 0.0001
  batchSize: number;         // Default: 4
  examplesPerTopic: number;  // Default: 10
  questionsPerExam: number;  // Default: 10
  teacherModel?: string;     // LLM for teacher steps
  teacherProvider?: string;  // Provider for teacher LLM
}
```

## Lessons Learned (Live Testing)

The Academy Dojo was tested end-to-end with dual sentinels running through the Rust pipeline engine. Across 8 deployment cycles, these issues were discovered and resolved:

### Sentinel Engine Modifications Required

1. **Multi-pass nested interpolation** — The teacher pipeline needs `{{steps.0.output.topics.{{input.iteration}}.name}}` (inner `{{input.iteration}}` must resolve before the outer path traverses the array). The interpolation engine now runs up to 5 passes with regex `[^{}\n]+` matching innermost `{{}}` first.

2. **JSON path traversal with array indexing** — `traverse_json_path()` supports numeric path parts for array access and auto-parses JSON strings encountered during traversal. This enables `steps.0.output.topics.2.name` to traverse a JSON array within an LLM output string.

3. **Loop-relative referencing** — `{{loop.N.field}}` resolves to `step_results[_loop_base + N]`, enabling stable intra-loop references regardless of where the loop sits in the pipeline. The loop executor injects `_loop_base` into `ctx.inputs` at the start of each iteration.

4. **Command routing bypass** — Pipeline command steps must use `execute_ts_json()` to route directly to the TypeScript Unix socket, bypassing the Rust ModuleRegistry. Otherwise, Rust modules claiming prefixes (e.g., `data/` → DataModule) intercept pipeline commands meant for TypeScript.

### Data Structure Conventions for Pipeline Authors

Understanding where step results store their data is critical:

| Step Type | `data` contains | `output` contains |
|-----------|----------------|-------------------|
| **LLM** | API metadata: `model`, `provider`, `responseTimeMs`, `usage` | The LLM text (auto-parses as JSON via `traverse_json_path`) |
| **Command** | The entire TypeScript response object | Same as `data` |
| **Watch** | `{ event, payload }` — the event name and its payload | The event name string |
| **Emit** | `{ event, payload }` | The event name string |
| **Condition** | Branch result data | Branch result output |

**Common patterns:**
- Watch step fields: `{{loop.N.data.payload.fieldName}}` (NOT `data.fieldName`)
- LLM grading scores: `{{loop.N.output.overallScore}}` (NOT `data.overallScore`)
- Entity IDs from `data/create`: `{{loop.N.data.data.id}}` (entity nested under `data.data`)
- LLM model used: `{{loop.N.data.model}}` (API metadata IS on `data`)

### Tuning Discoveries

- **Token budget**: `examplesPerTopic` default reduced from 20 → 10, and `maxTokens` increased to 8192. With 20 examples, the LLM consistently exhausted 4096 tokens and produced truncated JSON.
- **Session-scoped adapter names**: Adapter registration must include `sessionId` fragment to prevent collisions across academy sessions. Pattern: `${personaName}-${sessionId.slice(0,8)}-topic-${iteration}`.
- **Student exam model**: The student's exam LLM step must NOT use `baseModel` (e.g., smollm2:135m), which is a local Candle model unavailable on cloud providers. Use system default; future: route to Candle local inference to prove training worked.
- **Transient API errors**: DeepSeek API returns sporadic "error decoding response body" after long sessions. Production needs retry logic with exponential backoff per step.

### Metrics from Test Runs

Across 8 deployment cycles with the Academy running:
- **11** academy sessions created
- **9** curricula designed (LLM)
- **12** synthetic datasets generated (JSONL)
- **9** genome layers trained (LoRA via PEFT)
- **7** examinations created and graded
- **6 of 9** sentinel step types demonstrated: LLM, Command, Emit, Watch, Loop, Condition

## Key Reuse

The Academy builds entirely on existing infrastructure:

| Component | Reused From |
|-----------|-------------|
| `genome/train` | Existing LoRA training command |
| `genome/paging-adapter-register` | Existing adapter registration |
| `TrainingDatasetBuilder.loadFromJSONL()` | Validates synthesized JSONL |
| `GenomeLayerEntity` | Trained adapter persistence |
| `sentinel/run` | Routes pipelines to Rust engine |
| Rust `emit`/`watch` steps | Inter-sentinel coordination |

## Future: N:M Teacher/Student

The current design is 1:1 (one teacher, one student). The event-scoped architecture supports N:M:
- Multiple teachers could generate data for the same session (parallel topic research)
- Multiple students could train on the same curriculum (cohort learning)
- Cross-session teacher sharing (reuse curricula across personas)

## Roadmap: Beyond Text — Multi-Modal Training

The Academy architecture is media-agnostic. The same teacher/student pattern applies to ANY trainable modality:

### Phase 1: Text (Current)
- Q&A conversation pairs synthesized by teacher LLM
- LoRA fine-tuning via PEFT on SmolLM2/larger models
- Exam: student answers text questions, teacher grades via LLM

### Phase 2: Voice
- Teacher synthesizes text training data for voice characteristics
- Student trains TTS/STT adapters (voice cloning, speech patterns)
- Exam: teacher provides text prompts, student generates speech, teacher evaluates naturalness/accuracy
- New step type or command: `genome/train-voice` (wraps voice model fine-tuning)
- Events: `academy:{sessionId}:voice:sample:ready`, `voice:evaluation:complete`

### Phase 3: Images
- Teacher synthesizes image description pairs / style guides
- Student trains image generation adapters (LoRA on Stable Diffusion or similar)
- Exam: teacher provides prompts, student generates images, teacher evaluates style/accuracy via vision LLM
- New command: `genome/train-image` (wraps diffusion model LoRA)
- Events: `academy:{sessionId}:image:sample:ready`, `image:evaluation:complete`

### Phase 4: Video / Gameplay
- Teacher synthesizes scenario descriptions, gameplay strategies
- Student trains behavior models (game AI, video understanding)
- Exam: teacher provides scenarios, student demonstrates behavior
- New commands: `genome/train-video`, `genome/train-behavior`

### The Unifying Pattern

All modalities share the same sentinel pipeline structure:
1. Teacher designs curriculum (LLM step)
2. Teacher synthesizes training data (Command step → `genome/dataset-synthesize-{modality}`)
3. Student trains on data (Command step → `genome/train-{modality}`)
4. Teacher examines student (LLM step + modality-specific evaluation)
5. Teacher grades and decides remediation (LLM step)

The Academy doesn't need to know about modalities — it just orchestrates the pipeline. The modality-specific logic lives in the training and evaluation commands.

## Long-Running Resilience

Academy sessions are designed for hours-to-days execution:

### Checkpointing
- Session entity tracks `currentTopic` and `examRounds` — resume after crash
- Each completed topic emits `topic:passed` event — progress is durable
- Entity updates persist after every step (curriculum, exam, grades)

### Observability
- Every step result logged to `steps.jsonl` — full execution trace
- Events emitted per loop iteration — widgets and persona inbox stay informed
- Future: inference demo after each training round (prove learning to user/persona)
- Future: loss curves, exam scores, inference examples streamed as real-time events
- Future: criteria/thresholds replace hard-coded config (adaptive difficulty)

### Retry & Recovery
- Watch steps have configurable `timeoutSecs` (300-600s currently)
- Future: per-step retry with exponential backoff for transient API errors
- Future: session resume via `sentinel/resume --handle=<handle>`
- Future: dead-letter queue for failed steps (inspect and retry)

## Testing

```bash
# Unit tests (entity validation, pipeline templates, event taxonomy)
npx vitest tests/unit/semantic-cognition.test.ts

# Integration tests (requires npm start + LLM provider)
npx vitest tests/integration/sentinel-lora-training.test.ts
```
