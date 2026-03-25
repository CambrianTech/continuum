# Benchmarking Strategy

## Two Categories

### 1. Standard Model Benchmarks (Individual Capability)

Prove the compacted/fine-tuned model is good. Run industry-standard evals, publish results to model card and leaderboards.

| Benchmark | What it tests | Leaderboard |
|-----------|--------------|-------------|
| **HumanEval** | Python code generation (164 problems) | HuggingFace Open LLM |
| **MBPP** | Python programming (974 problems) | HuggingFace Open LLM |
| **SWE-bench** | Real GitHub issue fixing | swe-bench.com |
| **RealClassEval** | Python class implementation (98 classes) | Our own (already built into academy) |
| **MMLU** | General knowledge (57 subjects) | HuggingFace Open LLM |
| **MT-Bench** | Multi-turn conversation quality | LMSys |
| **IFEval** | Instruction following | HuggingFace Open LLM |

**When to run:** After plasticity compaction (proves compaction didn't destroy capability) and after LoRA training (proves training improved capability).

**How it flows:**
```
genome/train → adapter produced
  → genome/benchmark --model=compacted+adapter --suite=humaneval,mbpp
  → Results stored in adapter manifest + model card
  → adapter/publish includes benchmark scores
  → HuggingFace model card shows: "HumanEval: 72.5%, MBPP: 68.1%"
```

### 2. Continuum Collaborative Benchmark (Team Capability)

Nobody else has this. We define the benchmark, we own the leaderboard.

| Benchmark | What it tests |
|-----------|--------------|
| **Project Delivery** | N personas, M milestones, real project — does it build? do tests pass? |
| **Role Performance** | Individual scores per role in team context |
| **Learning Velocity** | How fast did the persona improve (phenotype delta per epoch) |
| **Collaboration Quality** | Did team members communicate effectively? governance proposals? |
| **Adapter Transferability** | Can this adapter be pulled by someone else and still perform? |

**Phenotype validation IS a benchmark.** The before/after comparison from academy training is a genuine measurement of capability improvement. We already capture this — it just needs to be formatted as a benchmark result.

## Command: `genome/benchmark`

```bash
# Run standard benchmarks against a model
./jtag genome/benchmark \
  --model="~/.continuum/genome/models/qwen14b-compacted.gguf" \
  --suite="humaneval,mbpp" \
  --output="benchmark-results.json"

# Run against a base model + LoRA adapter
./jtag genome/benchmark \
  --model="Qwen/Qwen2.5-Coder-14B-Instruct" \
  --adapter="~/.continuum/genome/adapters/helper-ai/coding-expertise" \
  --suite="humaneval,realclasseval"

# Run collaborative benchmark (uses academy session data)
./jtag genome/benchmark \
  --teamProjectId="abc123" \
  --suite="collaborative"
```

## Implementation Notes

- Standard benchmarks (HumanEval, MBPP) have open-source harnesses: `lm-evaluation-harness` (EleutherAI)
- SWE-bench has its own harness
- RealClassEval is already built into the academy pipeline
- Collaborative benchmarks are computed from existing TeamProjectEntity + AcademySessionEntity data
- Results stored in a BenchmarkResultEntity (collection: `benchmark_results`)
- Results embedded into adapter manifest for model card generation
- `adapter/publish` reads benchmark results automatically

## Leaderboard Strategy

**Phase 1 (now):** Phenotype scores in model cards. No external leaderboard.

**Phase 2 (soon):** Submit compacted base models to HuggingFace Open LLM Leaderboard. Prove compaction works.

**Phase 3 (later):** Create "Continuum Collaborative Benchmark" leaderboard on our website. Teams of specialized models scored on project delivery. We define the rules, we host the leaderboard.

**Phase 4 (vision):** Third-party sites (Papers With Code, etc.) recognize Continuum Collaborative Benchmark as a category. Other systems try to compete.

## What Already Exists

| Component | Status |
|-----------|--------|
| RealClassEval benchmark (Python classes) | ✅ Built into academy pipeline |
| Phenotype validation (before/after) | ✅ In StudentPipeline |
| Academy session scoring | ✅ Teacher grading with exams |
| Team project scoring (dual grading) | ✅ TeamTeacherPipeline |
| Adapter manifest with training metrics | ✅ AdapterManifest |
| Model card generation with scores | ✅ hf-publish.py |
| `lm-evaluation-harness` integration | 🔲 Not yet |
| BenchmarkResultEntity | 🔲 Not yet |
| `genome/benchmark` command | 🔲 Scaffolded below |
