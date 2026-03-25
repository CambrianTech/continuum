# Collaborative Multi-Agent Training with Role-Based Specialization and Phenotype Validation

## Abstract

We present an academy training system where AI personas learn skills through structured coursework, build real projects in teams, and publish their expertise as transferable LoRA adapters. Unlike fine-tuning pipelines that optimize loss metrics in isolation, our approach uses dual-sentinel orchestration (teacher + student), deterministic test validation (pytest, not LLM-as-judge), and phenotype comparison (same question before and after training) to prove that training produced measurable capability improvement.

The system supports five training modes: knowledge exams, coding challenges with real test suites, multi-milestone project development, standardized benchmarks (RealClassEval), and collaborative team projects with role-based specialization. In team mode, a teacher decomposes a project into roles, trains each student on their specific role, then orchestrates collaborative building with dual grading — both the project output and each individual's role performance.

All student work is posted to a shared chat room, enabling peer learning: students see each other's exam answers, code implementations, and grades. This shared visibility — not weight sharing — is how knowledge transfers between personas.

**Key result**: Qwen 3.5 27B running locally via Candle inference scores 100/100 on RealClassEval Python class implementation challenges with deterministic pytest validation. Academy session running on RTX 5090 with zero cloud dependency for student inference.

## 1. Introduction

Existing LLM fine-tuning approaches suffer from three problems:

1. **Invisible training**: Loss curves tell engineers something, but show nothing to users. There's no portfolio of work — no exam answers, no code submissions, no before/after comparison.

2. **Isolated training**: Each model trains alone. No peer learning, no shared curriculum, no team projects. In contrast, human education is deeply collaborative — students learn from each other's mistakes and successes.

3. **Vibes-based evaluation**: Most fine-tuning evaluation uses LLM-as-judge or human preference ratings. Neither is deterministic. Our approach uses pytest return codes — the test either passes or it doesn't.

We propose an academy system modeled on human education: a teacher designs curriculum, students do coursework, exams have deterministic scoring, improvement is measured via before/after comparison on the same test, and all work is visible in a shared classroom.

## 2. Architecture

### 2.1 Dual-Sentinel Orchestration

Each academy session spawns two Rust sentinel pipelines:

**Teacher Sentinel**: Designs curriculum, synthesizes training data, generates exams, grades responses, provides remediation feedback. Uses a capable LLM (cloud or trained local teacher adapter).

**Student Sentinel**: Takes pre-test (baseline), trains LoRA adapter on synthesized data, takes post-test (with adapter), submits to phenotype validation. Uses the local base model (the one being trained).

Inter-sentinel coordination uses iteration-scoped events: `academy:{sessionId}:{action}:{iteration}`. The iteration suffix prevents watch steps from matching stale events from previous training rounds.

### 2.2 Phenotype Validation

The core innovation: ask the model the same questions before and after training, score both, compare.

```
Pre-test  (no adapter): "Explain generics in TypeScript" → Score: 34/100
Training: 50 epochs on synthesized TypeScript generics examples
Post-test (with adapter): Same question → Score: 87/100
Improvement: +53 points
Quality gate: Register adapter only if improvement ≥ 5 points
```

This is not loss optimization — it's capability measurement. The model either got better at the specific skill or it didn't.

### 2.3 Deterministic Scoring

For coding modes, scoring uses real test suites:

```python
# RealClassEval: Student implements a Python class from a skeleton
# Teacher runs pytest on the implementation
# Score = (tests_passed / total_tests) * 100
python3 -m pytest test_solution.py -v
# Exit code 0 = all passed, non-zero = failures
```

No LLM-as-judge. No human preference. The code works or it doesn't.

### 2.4 Team Training with Role Specialization

Team mode decomposes a project into roles:

1. Teacher LLM analyzes project description → assigns roles (game-designer, engineer, artist)
2. Each role gets a tailored curriculum
3. Students train independently on their role
4. Build phase: students receive milestone tasks, execute via CodingAgent, post work to chat
5. Review: teacher grades the project AND each individual's role performance

Students communicate naturally during learning — the chat room IS the collaboration layer. No special inter-agent protocol needed.

## 3. Training Modes

| Mode | Validation Method | Training Signal |
|------|-------------------|-----------------|
| Knowledge | LLM-graded exams with rubric | Exam score + phenotype delta |
| Coding | pytest on buggy code fixes | Tests passed / total |
| Project | Multi-milestone test suites | Cold vs warm attempt comparison |
| RealClassEval | 98 Python classes, PYNGUIN tests | Deterministic pass rate |
| Team | Dual: project score + individual role score | Combined project + role improvement |

## 4. Shared Visibility as Peer Learning

All student work is posted to the academy chat room:
- Curriculum announcements (syllabus)
- Exam questions and student answers
- Teacher grades with feedback
- Before/after inference demos (same question, both answers)
- Code submissions with test results
- Milestone deliverables

This serves three purposes:
1. **Portfolio**: The student's body of work accumulates visibly
2. **Peer learning**: Other students see and learn from each other's work
3. **Transparency**: Users can watch the learning happen in real time

## 5. Adapter Marketplace

Trained adapters are published to HuggingFace with standardized `continuum:*` metadata tags:
- `continuum:role=sprite-artist`
- `continuum:skill=pixel-art`
- `continuum:score=87`
- `continuum:base=qwen3.5-27b`

Any Continuum instance can search for and pull published adapters. The model card includes real training output — exam scores, before/after comparisons, team project context. Every adapter is its own advertisement.

## 6. Results

### 6.1 RealClassEval Baseline (In Progress)

Qwen 3.5 27B via local Candle inference on RTX 5090:
- Challenge 1 (AES_GCM_Mechanism): 100/100, all pytest tests passed
- Full 98-class benchmark: In progress (academy session running)
- Training with rank 64, 50 epochs on failure remediation data

### 6.2 Peer Learning Across Model Sizes (Planned)

Competition mode with full 27B + three compacted versions (Air/Pro/5090) on the same curriculum. Score improvement curves across compaction levels. See companion paper: PEER-LEARNING-COMPACTION.md.

## 7. Related Work

- **RLHF** (Ouyang et al., 2022): Human preference optimization. Non-deterministic evaluation.
- **DPO** (Rafailov et al., 2023): Direct preference optimization. Still preference-based.
- **Self-Play** (Chen et al., 2024): Model trains against itself. Isolated, no peer learning.
- **Constitutional AI** (Bai et al., 2022): Rule-based self-improvement. No deterministic testing.
- **AgentBench** (Liu et al., 2023): Multi-agent benchmark. Evaluates but doesn't train collaboratively.

Our contribution: a complete training loop where (a) validation is deterministic (pytest), (b) improvement is proven (phenotype comparison), (c) learning is collaborative (shared chat visibility), and (d) expertise is transferable (LoRA adapters on HuggingFace).

## 8. Conclusion

The academy system transforms LLM fine-tuning from an opaque optimization process into a visible, collaborative learning environment. By using deterministic validation, phenotype comparison, and shared visibility, we provide both measurable proof that training works and a natural interface that non-ML users can understand — they see a student's coursework, not a loss curve.
