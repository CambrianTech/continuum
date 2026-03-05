# LoRA Genome: Domain-Specific AI Phenotypes

**Status**: Infrastructure Complete — GPU-Aware Paging Wired
**Date**: 2026-03-02 (updated from 2025-11-26)

## Vision

PersonaUser AIs will have **swappable LoRA adapter layers** that encode domain-specific expertise. These layers act like virtual memory pages - loading specialized knowledge for tasks, then evicting when no longer needed.

**The Metaphor**: Just as an operating system pages memory in/out based on current needs, PersonaUser will page LoRA phenotypes in/out based on task domain.

---

## Core Concept: AI Phenotypes

A **phenotype** is a specialized behavior pattern encoded in a fine-tuned LoRA layer:

```
Base Model (Claude/GPT/Llama)
  ↓
+ LoRA Layer 1: visual-debugging-phenotype.safetensors
+ LoRA Layer 2: code-analysis-phenotype.safetensors
+ LoRA Layer 3: performance-debugging-phenotype.safetensors
  ↓
Specialized AI with domain expertise
```

**Key Properties:**
- **Small**: LoRA adapters are ~10-100MB (vs 7GB+ full models)
- **Fast**: Can be loaded/unloaded in seconds
- **Stackable**: Multiple layers can combine (e.g., visual + performance debugging)
- **Fine-tunable**: Each layer trains independently on domain examples

---

## Phenotype Library Structure

```
PersonaGenome/
├── visual-debugging/
│   ├── visual-debugging-phenotype.safetensors (LoRA weights)
│   ├── training-examples.jsonl (instructor-guided examples)
│   ├── phenotype-manifest.json (metadata, capabilities)
│   └── README.md (usage patterns, triggers)
│
├── graphic-design/
│   ├── graphic-design-phenotype.safetensors
│   ├── training-examples.jsonl
│   ├── phenotype-manifest.json
│   └── README.md
│
├── code-analysis/
│   ├── code-analysis-phenotype.safetensors
│   ├── training-examples.jsonl
│   ├── phenotype-manifest.json
│   └── README.md
│
└── [other domains...]
```

---

## Example: Visual Debugging Phenotype

### Training Data Collection (Instructor-Guided Learning)

**Date**: 2025-11-25 (Today's Session)
**What Happened**: User instructed Claude Assistant to:
1. Apply 10px red border to container element
2. Take screenshot to visualize layout
3. Analyze screenshot with vision model
4. Assess layout boundaries and identify issues

**The Training Pattern:**
```typescript
{
  "domain": "visual-debugging",
  "task": "debug-layout-overflow",
  "instructor_guidance": "Apply 10px colored border and screenshot",
  "tools_used": ["debug/widget-css", "screenshot"],
  "execution_trace": [
    {
      "step": 1,
      "tool": "debug/widget-css",
      "params": {
        "selector": ".user-list-widget-entity-list-container",
        "styles": {"border": "10px solid red"}
      },
      "success": true
    },
    {
      "step": 2,
      "tool": "screenshot",
      "params": {"querySelector": "body"},
      "media_returned": {
        "type": "image",
        "base64_length": 87453,
        "mimeType": "image/png"
      }
    },
    {
      "step": 3,
      "action": "visual_analysis",
      "observations": [
        "Red border visible around entity list container",
        "Container boundaries clearly marked",
        "Content clipping at bottom edge",
        "Overflow issue confirmed visually"
      ]
    },
    {
      "step": 4,
      "action": "follow_up_question",
      "content": "Can you try scrolling within that red-bordered area to see if the list scrolls properly?"
    }
  ],
  "outcome": "successful_diagnosis",
  "key_insights": [
    "Border technique isolated the problematic container",
    "Visual feedback confirmed layout boundaries",
    "Screenshot enabled spatial reasoning about overflow"
  ]
}
```

### What Gets Encoded in the LoRA Layer

After collecting many such examples, fine-tuning encodes:

**Pattern Recognition:**
- "Layout overflow problem" → Apply border + screenshot technique
- "Container boundary unclear" → Use colored borders for isolation
- "Visual state unknown" → Take screenshot before/after changes

**Tool Sequences:**
- CSS injection → Screenshot → Visual analysis → Follow-up
- Knows to use base64 media flow for visual feedback
- Understands spatial relationships from images

**Domain Knowledge:**
- Border colors help distinguish nested containers
- 10px borders visible but not intrusive
- Screenshots capture state for comparison
- Overflow issues visible at container edges

---

## Phenotype Catalog (Planned)

### 1. Visual Debugging Phenotype
**Encodes:** Layout debugging, visual regression testing, CSS diagnosis
**Tools:** debug/widget-css, screenshot, visual analysis
**Training Sources:** Border technique sessions, layout debugging traces
**Use Cases:** UI bugs, overflow issues, alignment problems

### 2. Graphic Design Phenotype
**Encodes:** Design patterns, color theory, typography, composition
**Tools:** screenshot, debug/widget-css, visual comparison
**Training Sources:** Design critique sessions, style guide applications
**Use Cases:** UI polish, design reviews, aesthetic improvements

### 3. Code Analysis Phenotype
**Encodes:** Pattern recognition, architecture understanding, code smells
**Tools:** code/read, grep, code/pattern-search
**Training Sources:** Code review sessions, refactoring discussions
**Use Cases:** Code reviews, bug hunting, architecture analysis

### 4. Performance Debugging Phenotype
**Encodes:** Profiling interpretation, bottleneck identification, optimization patterns
**Tools:** debug/logs, code/read, grep
**Training Sources:** Performance investigation sessions
**Use Cases:** Slow queries, memory leaks, CPU hotspots

### 5. Security Analysis Phenotype
**Encodes:** Vulnerability patterns, attack vectors, secure coding practices
**Tools:** code/read, grep, code/pattern-search
**Training Sources:** Security audits, penetration testing
**Use Cases:** Security reviews, vulnerability scanning

---

## The Paging System (IMPLEMENTED)

The genome paging engine is implemented in Rust (`persona/genome_paging.rs`) with real GPU-aware memory management. No more hardcoded budgets — VRAM is detected from hardware and budgets flow from `GpuMemoryManager`.

### GPU Memory Integration

```
GpuMemoryManager::detect()
  → Metal/CUDA detects real VRAM (e.g., 16GB Apple Silicon shared memory)
  → 75% allocated to Inference subsystem
  → CognitionState.per_persona_budget_mb() divides by persona count
  → Each GenomePagingEngine receives its real per-persona budget
```

**Observability**: `./jtag gpu/stats` shows real-time VRAM usage across all subsystems.

See [GPU-MEMORY-ARCHITECTURE.md](../infrastructure/GPU-MEMORY-ARCHITECTURE.md) for the full GPU memory system design.

### Rust Implementation (GenomePagingEngine)

```rust
// Real Rust code in persona/genome_paging.rs
pub struct GenomePagingEngine {
    loaded_adapters: Vec<LoadedAdapterState>,
    memory_budget_mb: f64,           // From GPU manager (real VRAM)
    gpu_manager: Option<Arc<GpuMemoryManager>>,
    allocation_guards: HashMap<String, GpuAllocationGuard>,  // RAII VRAM tracking
    // ...
}

impl GenomePagingEngine {
    pub fn activate_skill(&mut self, domain: &str) -> ActivateSkillResult {
        // 1. Check if already loaded (cache hit)
        // 2. If memory pressure, evict via LRU formula:
        //    score = age_seconds / (priority * 10)
        //    Higher score = more evictable
        // 3. Load adapter, allocate GPU guard
        // 4. Guard automatically releases VRAM on eviction (RAII)
    }
}
```

### Pressure-Driven Eviction

Eviction is now driven by real GPU pressure, not just adapter count:

| Pressure | Level | Action |
|----------|-------|--------|
| 0-60% | Normal | No eviction needed |
| 60-80% | Warning | Evict non-critical adapters (low priority, old age) |
| 80-95% | High | Refuse new loads, aggressive eviction |
| 95%+ | Critical | Force evictions |

The eviction score formula: `age_seconds / (priority × 10)` — older, lower-priority adapters evict first. Critical adapters (marked `priority >= 0.9`) are never evicted.

### Manifest-Based Capabilities

Each phenotype declares its capabilities:

```json
{
  "phenotype": "visual-debugging",
  "version": "1.0.0",
  "baseModel": "claude-sonnet-4.5",
  "loraPath": "./visual-debugging-phenotype.safetensors",
  "rank": 16,
  "alpha": 32,
  "capabilities": [
    "layout-debugging",
    "visual-regression-testing",
    "css-diagnosis",
    "screenshot-analysis"
  ],
  "toolSequences": [
    ["debug/widget-css", "screenshot"],
    ["screenshot", "debug/widget-css", "screenshot"]
  ],
  "trainingExamples": 247,
  "lastTrained": "2025-11-26T00:00:00Z",
  "triggers": [
    "layout overflow",
    "visual bug",
    "CSS not applying",
    "element position wrong"
  ]
}
```

---

## Training Pipeline

### Phase 1: Infrastructure Validation ✅

**Goal**: Prove tools + vision + media flow works
**Achievement**: Claude successfully executed instructor-guided visual debugging

### Phase 2: LoRA Training E2E ✅

**Goal**: End-to-end LoRA fine-tuning pipeline
**Achievement**: Full pipeline proven: `genome/train` + `genome/dataset-prepare` + `genome/training-export`
- PEFT-based training via `peft-train.py` with dynamic gradient accumulation
- AdapterStore filesystem-based discovery (single source of truth)
- Candle `ensure_adapters()` + `rebuild_with_stacked_lora()` for LoRA application
- 196 LoRA layers per adapter on Llama-3.2-3B

### Phase 3: Paging System ✅

**Goal**: Automatic phenotype loading with LRU eviction
**Achievement**: `GenomePagingEngine` in Rust with real GPU memory awareness
- GPU budgets from `GpuMemoryManager.detect()` (Metal/CUDA)
- RAII `GpuAllocationGuard` tracking per adapter
- Eviction formula: `age_seconds / (priority × 10)`
- Critical adapters protected from eviction

### Phase 4: Academy Dojo ✅

**Goal**: Automated training via teacher/student sentinels
**Achievement**: Dual-sentinel pipeline — teacher synthesizes data, student trains, teacher examines
- Multi-topic sessions proven (2+ topics per session, 100/100 scores)
- Resilient to transient API errors (retry with exponential backoff in both Rust and TypeScript)
- Pipeline timeout configurable per session (1800s for multi-topic academy)
- Real-time observability via `steps.jsonl` sub-step flushing
- Markdown-fenced LLM output handled transparently in condition evaluation
- See [ACADEMY-DOJO-ARCHITECTURE.md](../personas/ACADEMY-DOJO-ARCHITECTURE.md)

### Phase 5: Continuous Learning (NEXT)

**Goal**: Capture interactions during normal operation, auto-train when threshold met
**Method**:
1. `TrainingDataAccumulator.captureInteraction()` during persona responses
2. `shouldMicroTune()` triggers when 50+ quality examples accumulated per domain
3. Check `gpu/pressure` before training (don't train if pressure > 60%)
4. Spawn training sentinel, validate via academy examination
5. Promote adapter only if examination passes

---

## Architecture Integration

### Current PersonaUser Flow

```typescript
// src/system/user/server/PersonaUser.ts

async serviceInbox(): Promise<void> {
  // 1. Check inbox
  const tasks = await this.inbox.peek(10);
  if (tasks.length === 0) {
    await this.rest();
    return;
  }

  // 2. Select task
  const task = tasks[0];

  // 3. Process
  await this.processTask(task);
}
```

### With LoRA Genome Integration

```typescript
async serviceInbox(): Promise<void> {
  const tasks = await this.inbox.peek(10);
  if (tasks.length === 0) {
    await this.rest();
    return;
  }

  const task = tasks[0];

  // NEW: Activate domain-specific phenotypes
  await this.genome.activateForTask(task);

  // Now process with specialized knowledge
  await this.processTask(task);

  // Optionally evict if memory pressure
  if (this.genome.memoryPressure > 0.8) {
    await this.genome.evictLRU();
  }
}
```

---

## Key Benefits

### 1. Specialized Expertise Without Model Bloat
- Base model stays lean (7-13B parameters)
- LoRA layers add <100MB each
- Combine multiple specializations as needed

### 2. Continuous Learning
- Collect training examples from real usage
- Retrain phenotypes periodically
- Improve based on actual task patterns

### 3. Efficient Memory Usage
- Only load phenotypes needed for current task
- LRU eviction prevents memory exhaustion
- Can run multiple phenotypes simultaneously

### 4. Domain Independence
- Each phenotype trains separately
- No catastrophic forgetting
- Easy to add new domains without retraining others

---

## Comparison: Before vs After

### Before LoRA Genome (Current)
```
Task: Debug layout overflow
  ↓
Base model reasoning
  ↓
Generic problem-solving (no visual debugging expertise)
  ↓
User must guide every step
```

### After LoRA Genome (Future)
```
Task: Debug layout overflow
  ↓
Genome detects domain: visual-debugging
  ↓
Loads visual-debugging-phenotype.safetensors
  ↓
AI now has border techniques, screenshot patterns encoded
  ↓
Executes debugging workflow autonomously
```

---

## Progress

### Complete
- [x] Tools + vision + media flow
- [x] Tool result metadata pipeline
- [x] LoRA fine-tuning pipeline (PEFT `peft-train.py`, dynamic hyperparams)
- [x] AdapterStore filesystem discovery
- [x] Candle adapter loading (safetensors + GGUF backends)
- [x] GenomePagingEngine with LRU eviction (Rust)
- [x] GpuMemoryManager with Metal/CUDA detection
- [x] RAII allocation guards across all GPU consumers
- [x] `./jtag gpu/stats` observability command
- [x] Academy Dojo teacher/student pipeline
- [x] CodingAgent sentinel step for code generation training
- [x] Sentinel LLM retry with exponential backoff (Rust + TypeScript)
- [x] Real-time sub-step observability (steps.jsonl flushing)
- [x] Configurable pipeline timeout (1800s for academy sessions)
- [x] Markdown fence stripping in interpolation engine

### Next
- [ ] Wire `TrainingDataAccumulator.captureInteraction()` into persona response loop
- [ ] Auto-training sentinel (triggered by `shouldMicroTune()`)
- [ ] Phenotype validation via academy examination post-training
- [ ] Pre-trained adapters shipped in repo (system-knowledge, sentinel-orchestration, user-assistance)
- [ ] Grid: P2P genome sharing via Reticulum mesh

---

## Technical Stack

**Implemented:**
- Tool execution: ToolRegistry + Commands system
- Vision: Claude Sonnet 4.5 with screenshot support
- Media flow: PersonaToolExecutor → ChatRAGBuilder → AnthropicAdapter
- LoRA training: PEFT via `peft-train.py` subprocess
- LoRA inference: Candle (Rust) with `rebuild_with_stacked_lora()`
- Adapter storage: `.safetensors` files, filesystem-based `AdapterStore`
- Paging engine: Rust `GenomePagingEngine` with LRU eviction
- GPU management: Rust `GpuMemoryManager` (Metal/CUDA detection, RAII guards)
- Academy: Dual-sentinel teacher/student pipeline via Rust sentinel engine
- Observability: `./jtag gpu/stats`, `./jtag genome/paging-stats`

---

## References

- **GPU Memory Architecture**: [GPU-MEMORY-ARCHITECTURE.md](../infrastructure/GPU-MEMORY-ARCHITECTURE.md)
- **Dynamic Genome Composition**: [DYNAMIC-GENOME-ARCHITECTURE.md](DYNAMIC-GENOME-ARCHITECTURE.md)
- **Academy Dojo**: [ACADEMY-DOJO-ARCHITECTURE.md](../personas/ACADEMY-DOJO-ARCHITECTURE.md)
- **Sentinel LoRA Training**: [sentinel-lora-training.md](sentinel-lora-training.md)
- **GPU Stats Command**: [gpu/stats README](../../commands/gpu/stats/README.md)
- **PersonaUser Architecture**: `system/user/server/PersonaUser.ts`
- **Genome Paging Engine**: `workers/continuum-core/src/persona/genome_paging.rs`
- **GPU Memory Manager**: `workers/continuum-core/src/gpu/memory_manager.rs`
