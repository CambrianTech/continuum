## Summary

Make the Academy training pipeline work end-to-end and produce models worth using. Academy is a **classroom, not a TensorBoard dashboard** — students build real apps, write real stories, take real exams. Their work is visible, gradeable, and accumulates as a portfolio. Other students learn from peers.

## The Vision: Specialized Agents That Self-Learn

Not monolithic god-AIs — diverse, specialized agents that get better at their roles and learn to work with others. A PM persona learns to manage. An engineer persona learns to code. A designer persona learns to create. They work together on the same project, each improving at what they do. This is far more aligned with how human teams actually work.

The purpose of Continuum is intermingling of all kinds — human and AI, local and cloud, specialist and generalist. Academy is where they learn. The academy room is their classroom. The grid doesn't just distribute compute — it distributes the classroom across machines.

## The Chain That Must Work

```
Teacher designs syllabus (topics, difficulty, learning objectives)
  → Student attempts real challenges (actual code, actual projects, actual exams)
  → Real validation (pytest return code, not LLM vibes)
  → Student's work posted to chat (their code, their answers — the portfolio)
  → Teacher and peers SEE the work, give feedback, learn from each other
  → Students can TALK during learning — ask questions, share discoveries, debate
  → LoRA training on successes (days/weeks on 5090, not 40-minute toys)
  → Gate gradients → plasticity compaction → target-device GGUF
  → Phenotype validation: same question before/after, side-by-side
  → Compacted models published to HuggingFace, targeting Air/Pro/5090
```

## Academy Modes

| Mode | What students produce | What you see in chat |
|------|----------------------|---------------------|
| **Knowledge** | Exam answers | Q&A, grades, before/after inference demos |
| **Coding** | Bug fixes (actual source code) | Their code + test results + teacher feedback |
| **Project** | Multi-milestone apps | File tree, build output, cold→warm comparison |
| **RealClassEval** | Python class implementations | Their Python code, pytest results |
| **Recipe** | Recipe-specific skills | Gap analysis, targeted training |
| **Competition** | Same exam, N students ranked | Leaderboard, gap analysis, tournament |
| **Team** _(planned)_ | Shared project, different roles | Project score + individual role scores |

## Team Training Vision (Planned — Architecture Must Support)

Simple case: 1-N students learning the same thing. But the system is designed for:

- **Team sessions**: Entire team enters training together. Each student has a role (PM, engineer, designer) from the recipe — or they self-organize amongst themselves.
- **Dual grading**: Teacher grades the overall project quality AND each individual's role performance. "How was the project?" and "How did each of you do at your role?"
- **Peer learning**: All students see each other's work in the academy chat room. They learn from successes and failures of peers.
- **Communication during learning**: Students should talk throughout learning — not train in isolation. Ask questions, share discoveries, debate approaches. The chat history becomes RAG context for subsequent inference. Peer learning emerges naturally from shared visibility.
- **Group projects with delegation**: Like school group projects — a PM learns to manage while engineers learn to code, on the same shared project.
- **Recipe-defined roles**: Any recipe can form into an academy course. The recipe's strategy defines roles, the academy session assigns students.
- **Grid-distributed classrooms**: Student A trains on the 5090, Student B infers on the Mac, Student C uses cloud — all posting to the same room, seeing each other's work.

## What's Fixed (this PR — 34 files, 849 insertions)

### Critical Fixes
- **Academy sentinel death** — `resolveTeacherLlmConfig()` validates model/provider upfront, throws if missing. All 7 teacher pipelines fixed. No more conditional spreads that silently omit model.
- **Training kills itself after minutes** — `PRODUCTION_ACADEMY_CONFIG` (50 epochs, 100 examples/topic, rank 64). Sentinel timeout=0 (no timeout). Watch step timeout=0. Rust sentinel + watch both support indefinite runs.
- **Per-task model routing (#371)** — `TaskAwareProviderRouter` upgrades local 3B to best available cloud provider when task requires coding/tool use. Persona identity stays the same, only compute changes. Cached provider discovery (5min TTL).
- **Zombie sessions** — Auto-cleanup marks sessions stuck >1hr as failed on new session creation.

### Student Work Visibility (the classroom)
- **TeacherPipeline** — Curriculum announcement, grade reports with full Q&A, pass/fail decisions
- **StudentPipeline** — Before/after inference demos (same question, both answers side-by-side)
- **CodingStudentPipeline** — Actual source code the student wrote + test results
- **CodingTeacherPipeline** — Teacher's grade report with score + feedback
- **ProjectStudentPipeline** — Cold attempt (first try) and warm attempt (after training) — actual project files + build output
- **ProjectTeacherPipeline** — Milestone pass/fail with teacher feedback
- **RealClassEvalTeacherPipeline** — Student's Python implementation + pytest results, visible for both pass and fail

### Plasticity Pipeline (Full Chain)
- **plasticity/compress wired** — Server command calls Rust IPC (was hardcoded failure)
- **PlasticityMixin** — Added `plasticityCompress()` and `plasticityPipeline()` IPC methods
- **4 new commands scaffolded + wired** — analyze, compact, topology, pipeline (all call Rust via IPC, not stubs)
- **4 generator specs** — plasticity-analyze.json, plasticity-compact.json, plasticity-pipeline.json, plasticity-topology.json
- **Post-training compaction** — All student pipelines (Student, CodingStudent, ProjectStudent, RealClassEval, LoRATraining) now run `plasticity/pipeline` → `plasticity/compress` after training completes

### Training Infrastructure
- **TrainingRecoveryService** — Scans for orphaned jobs on startup, auto-resumes from checkpoint via `genome/train/resume`. Wired into ServiceInitializer.
- **PRODUCTION_ACADEMY_CONFIG** — 50 epochs, 100 examples/topic, rank 64, passing score 80, learning rate 5e-5
- **Gate gradient capture** — Already implemented in peft-train.py (GateGradientCallback). Confirmed wired.

### Cleanup
- **academy-training.json** — Rewritten. Removed dead command references (academy/determine-role, academy/generate-exam, etc.)
- **LoRATrainingPipeline** — Now includes plasticity compaction + GGUF compression. Supports `deviceSpec` and `skipPlasticity` config.
- **Pre-existing TS errors** — GridOverviewWidget STATUS_COLORS, training-overview type cast

## What's Remaining

- [ ] **Cross-node event forwarding (#364)** — Mac dashboard can't see 5090 training events. Full Rust GridFrame feature, separate PR.
- [ ] **Deploy to 5090** and run academy session to completion end-to-end
- [ ] **Validate compacted model** — can it actually do the tasks it trained on?
- [ ] **Team training mode** — multi-student shared workspace with role assignment and dual grading (project + individual)
- [ ] **Dashboard widgets** — syllabus view, gradebook, portfolio browser (classroom UI)
- [ ] **Inter-student communication** — students talking during learning phases, not just teacher↔student

## Target Models

| Device | Budget | Target |
|--------|--------|--------|
| MacBook Air 16GB | ~11GB | Compacted 14B, Q3_K_S |
| MacBook Pro 32GB | ~16GB | Compacted 32B, Q4_K_M |
| RTX 5090 32GB | ~28GB | Full or MoE compacted |

## Related Issues
#364 #365 #366 #371
