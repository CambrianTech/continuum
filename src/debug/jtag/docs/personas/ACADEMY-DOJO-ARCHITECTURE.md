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
  examplesPerTopic: number;  // Default: 20
  questionsPerExam: number;  // Default: 10
  teacherModel?: string;     // LLM for teacher steps
  teacherProvider?: string;  // Provider for teacher LLM
}
```

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

## Testing

```bash
# Unit tests (entity validation, pipeline templates, event taxonomy)
npx vitest tests/unit/semantic-cognition.test.ts

# Integration tests (requires npm start + LLM provider)
npx vitest tests/integration/sentinel-lora-training.test.ts
```
