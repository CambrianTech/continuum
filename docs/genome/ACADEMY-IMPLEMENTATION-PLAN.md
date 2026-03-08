**Parent:** [Genome](README.md)

# Academy Implementation Plan — Phased Roadmap

## Current State (March 2026)

**What WORKS end-to-end (proven):**
- LoRA training pipeline (PEFT + TrainingMemoryGuard) — tested on SmolLM2, Llama-3.2, Qwen3, Phi-4
- LoRA inference (Candle load → merge → activate → stacked inference)
- AdapterStore (filesystem single source of truth, discovery, compatibility checking)
- Academy dual-sentinel sessions (teacher/student, event-scoped coordination)
- RealClassEval benchmark: **53.1% Pass@1** on 98 challenges (paper baseline: 25-34%)
- CodingAgent step with training capture
- Model selection logic in Rust (4-tier priority chain)

**What's NOT wired (integration gaps):**
- Adapters don't auto-load on persona startup (cold start = base model only)
- Model selection not wired to inference path (PersonaResponseGenerator ignores adapters)
- No automatic eviction trigger (memory pressure unmonitored)
- No autonomous retrain trigger (data accumulates but never trains)
- Adapter management commands missing (list, prune, info)

---

## Non-Negotiable: QLoRA + GGUF Everywhere

**All training and inference MUST target MacBook M1/M1 Pro (8-16GB unified memory).**

- **Training**: QLoRA 4-bit quantized base + full-precision LoRA adapters (already the default)
- **Inference**: GGUF quantized base model (~1.5GB for 3B) + full-precision LoRA merge
- **Budget**: ~2GB per model with adapter. Room for 3-4 loaded simultaneously on 8GB M1
- **Never full precision**: If it doesn't fit quantized on an M1, it doesn't ship

The fallback chain in peft-train.py (4-bit → 8-bit → full precision) exists for compatibility but production always runs 4-bit. TrainingMemoryGuard estimates already assume QLoRA.

---

## Phase 1: Foundation Wiring (Wire What Exists)

**Goal**: Every persona starts with its trained adapters loaded and uses them for inference. No new algorithms — just connect the pieces that already work.

### 1A: Auto-load adapters on PersonaUser startup
- PersonaUser constructor calls `AdapterStore.latestCompatibleByDomain(personaId, model)`
- Loads top adapters by domain coverage (up to memory quota)
- Rust genome paging tracks loaded state
- **Test**: Start system, verify persona has adapters loaded via `genome/paging-stats`

### 1B: Wire model selection into inference path
- PersonaResponseGenerator calls `cognition/select-model` before generating
- Uses 4-tier Rust logic: trait-specific adapter → current active → any available → base model
- Selected model/adapter passed to AIProviderDaemon or Candle
- **Test**: Send coding question, verify persona activates `reasoning_style` adapter

### 1C: Adapter management commands
- `genome/adapter-list` — list all adapters with sizes, last-used, cascade scores
- `genome/adapter-prune --unused-since=30d` — reclaim disk
- `genome/adapter-info --name=X` — full manifest + provenance
- Use CommandGenerator for proper scaffolding
- **Test**: Run each command, verify output matches AdapterStore state

### 1D: Automatic eviction on memory pressure
- Background monitor in PersonaUser checks GPU memory pressure every 30s
- When pressure > 0.8, call Candle `unload_lora()` on LRU adapter
- Rust EvictionRegistry already tracks candidates and scores
- **Test**: Load adapters until pressure > 0.8, verify LRU gets evicted

**Phase 1 validation**: `npm run build:ts` clean, `npm start`, persona responds using trained adapter, adapter shows as loaded in `genome/paging-stats`.

---

## Phase 2: Academy Coding Challenges (Prove Learning Works)

**Goal**: Run full coding challenge sessions end-to-end, demonstrate measurable improvement from LoRA training. Use RealClassEval which is already proven.

### 2A: Run RealClassEval baseline on multiple models
- Test with: DeepSeek (cloud), Claude (cloud), Local Candle (Llama-3.2-3B)
- Capture Pass@1 rates per model as baseline
- Store results in AcademySessionEntity
- **Test**: `./jtag genome/academy-session --mode=realclasseval --provider=deepseek`

### 2B: Train and re-examine
- After initial exam, train LoRA on remediation data (already implemented)
- Re-examine with trained adapter loaded
- Compare pre-LoRA vs post-LoRA pass rates
- **Test**: Post-LoRA pass rate > pre-LoRA pass rate (the delta IS the proof)

### 2C: Cross-model training transfer
- Train LoRA from Claude's exam solutions
- Load onto Llama-3.2-3B local model
- Run same exam — does the local model improve?
- This is the distillation proof-of-concept
- **Test**: Local model pass rate with distilled LoRA > local model baseline

### 2D: CodingAgent mode academy sessions
- Use `codingagent` step type instead of plain LLM for student attempts
- Student gets real coding environment (file editing, compilation, test running)
- `captureTraining: true` records full interaction for training
- **Test**: `./jtag genome/academy-session --mode=coding --provider=anthropic`

**Phase 2 validation**: Measurable improvement numbers. Pre-LoRA vs post-LoRA deltas published. Cross-model transfer demonstrated.

---

## Phase 3: Competitive Cohort Sessions

**Goal**: Multiple students take the same exam simultaneously, learn from each other's solutions. AP classroom effect.

### 3A: Multi-student academy session
- Extend `genome/academy-session` with `--students="helper,deepseek,local-assistant"`
- Teacher broadcasts exam to all students
- Collect responses, grade, rank
- Share top solutions as learning material
- **New entity**: `AcademyCohortEntity` — tracks per-student rankings across stages

### 3B: Comparative training pairs
- Teacher generates pairs: Student A's approach vs Student B's (better) approach
- Include WHY B's approach is better (teacher commentary)
- Build per-student training datasets including peer solutions
- **Test**: Train LoRA from comparative pairs, verify improvement > single-student training

### 3C: Cohort metrics and reporting
- `cascadeAwareness` ratio: retrained/naive performance
- Per-student ranking progression over rounds
- Cross-pollination tracking: which student pairs benefited most from each other
- `./jtag genome/academy-report --session=X` — full cohort analysis

**Phase 3 validation**: Weaker model (local Candle) shows improvement from training on stronger model (Claude) solutions. Competitive pressure measurably accelerates learning.

---

## Phase 4: Cascading Curriculum

**Goal**: Stages form dependency graphs. Late-stage failures generate retroactive training signals for early-stage decisions. Temporal credit assignment.

### 4A: Dependency graph curricula
- Extend teacher pipeline with `produces`/`consumes` declarations per stage
- Stages carry forward outputs — later stages use earlier outputs
- Constraint injection from Stage 1 (target hardware, frame budget, memory budget)

### 4B: Retroactive grading
- When Stage N fails, teacher traces root cause to Stage M
- Generates retroactive training pair: early decision + late consequence
- Tags with cascade depth for temporal-weighted loss

### 4C: Integration testing per stage
- After each stage, run downstream integration test
- Shell step with `allowFailure: true` for real compilation/test execution
- Failed integration → retroactive signal to upstream stages

### 4D: Temporal-weighted LoRA training
- Training pairs from retroactive grading carry higher weight
- `weight = base_weight * (1 + cascade_depth * 0.5)`
- Deeper cascades = higher weight = model learns to see further ahead

**Phase 4 validation**: `cascadeAwareness > 1.0` — model makes better early decisions after cascade training than before.

---

## Phase 5: Recipe-Driven Academy

**Goal**: Feed a recipe to the Academy, it auto-designs the curriculum, identifies genome gaps, trains only what's missing.

### 5A: Recipe → curriculum generation
- Teacher reads recipe steps, tools, constraints
- Queries genome for existing LoRA coverage per skill
- Designs cascading stages targeting ONLY gaps
- **Test**: Recipe with 5 skills, 3 already in genome → Academy trains 2

### 5B: Genome assembly before training
- Before Academy starts, assemble best available LoRAs from genome
- Student begins with a FLOOR, not ground zero
- Academy measures improvement FROM the assembled baseline

### 5C: Post-training genome deposit
- After successful Academy session, new LoRAs added to genome
- Manifest includes full provenance (recipe, cohort, cascade scores)
- Next recipe for similar domain starts from higher floor

**Phase 5 validation**: Iteration N+1 of a recipe domain completes faster and scores higher than iteration N.

---

## Phase 6: Genomic Repository + Distillation

**Goal**: Docker-like adapter management. Shared repository. Progressive remote → local independence.

### 6A: Docker-style CLI commands
- `genome pull/push/prune/inspect/history/tag/diff`
- Adapter provenance with full manifest metadata
- Delta transfer for efficient pulls

### 6B: Distillation tracking
- Per-domain metrics: remote model pass rate vs local model pass rate
- `distillationRatio = local/remote` — when ≥ 0.8, domain is distilled
- Auto-remove remote API from cohort for distilled domains

### 6C: MPC signing for adapter provenance
- Cryptographic verification of training lineage
- Tamper-proof cascade scores and cohort rankings
- Required for shared repository trust

**Phase 6 validation**: Local model achieves ≥ 80% of remote model performance on distilled domains. Full adapter lifecycle: train → share → discover → load → use → evict → prune.

---

## Testing Strategy (Every Phase)

### Unit tests
- Rust: `cargo test -p continuum-core` — all genome/evaluator/paging tests
- TypeScript: `npx vitest tests/unit/Genome*.test.ts`

### Integration tests
- `genome/academy-session` end-to-end with real inference
- `genome/train` with real PEFT training
- Adapter load → inference → verify output quality

### Benchmark tests (timing)
- Model selection: < 100μs (Rust, already proven)
- Adapter load: < 2s for typical LoRA
- Paging decision: < 1ms
- Full academy session: track wall-clock time, compare across iterations

### Regression tests
- RealClassEval baseline scores must not decrease
- Pre-LoRA vs post-LoRA delta must be positive
- Memory pressure must never exceed safety threshold during training

---

## Implementation Order

```
Phase 1 (Foundation Wiring)     ← CURRENT PRIORITY
  1A: Auto-load adapters        ← START HERE
  1B: Model selection wiring
  1C: Adapter management CLI
  1D: Auto-eviction

Phase 2 (Prove Learning)        ← Demonstrate with numbers
  2A: Multi-model baselines
  2B: Train + re-examine
  2C: Cross-model transfer
  2D: CodingAgent mode

Phase 3 (Competitive Cohort)    ← AP classroom effect
Phase 4 (Cascading Curriculum)  ← Temporal credit assignment
Phase 5 (Recipe-Driven)         ← Auto-designed curricula
Phase 6 (Repository)            ← Docker for LoRAs
```

Each phase builds on the previous. Each has its own validation criteria. No phase is started until the previous phase's tests pass.
