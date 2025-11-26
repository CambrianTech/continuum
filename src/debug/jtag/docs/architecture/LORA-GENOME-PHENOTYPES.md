# LoRA Genome: Domain-Specific AI Phenotypes

**Status**: Planning / Infrastructure Validation Phase
**Date**: 2025-11-26

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

## The Paging System

### Automatic Loading (Task-Driven)

```typescript
class PersonaGenome {
  private activeAdapters: Map<string, LoRAAdapter> = new Map();
  private adapterRegistry: Map<string, AdapterMetadata>;
  private maxActiveAdapters = 3; // Memory constraint

  async activateForTask(task: Task): Promise<void> {
    // 1. Determine required phenotypes from task domain
    const requiredPhenotypes = this.inferPhenotypes(task);

    // 2. Page in needed adapters
    for (const phenotype of requiredPhenotypes) {
      if (!this.activeAdapters.has(phenotype)) {
        // Check memory pressure
        if (this.activeAdapters.size >= this.maxActiveAdapters) {
          await this.evictLRU(); // Page out least recently used
        }

        await this.loadAdapter(phenotype);
      }

      this.touch(phenotype); // Update LRU
    }
  }

  private inferPhenotypes(task: Task): string[] {
    // Task domain → required phenotypes mapping
    const domainMap = {
      'debug-layout': ['visual-debugging'],
      'design-review': ['visual-debugging', 'graphic-design'],
      'code-review': ['code-analysis'],
      'performance': ['performance-debugging', 'code-analysis'],
      'security-audit': ['security-analysis', 'code-analysis']
    };

    return domainMap[task.domain] || [];
  }

  private async evictLRU(): Promise<void> {
    // Find least recently used adapter
    const lru = this.findLRU();
    await this.unloadAdapter(lru);
    this.activeAdapters.delete(lru);
  }
}
```

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

### Phase 1: Infrastructure Validation ✅ (Today)

**Goal**: Prove tools + vision + media flow works
**Achievement**: Claude successfully executed instructor-guided visual debugging
**Evidence**: Border + screenshot technique worked end-to-end
**Commits**:
- `c513182e2`: Media flow verification and logging
- `32c9d653c`: Tool metadata pollution fix

### Phase 2: Training Data Collection (Next)

**Goal**: Build library of instructor-guided examples
**Method**:
1. User guides AIs through debugging/design tasks
2. System logs execution traces (tools, params, outcomes)
3. Store as training examples with annotations
4. Tag by domain (visual-debugging, code-analysis, etc.)

**Example Collection Commands:**
```bash
# Start recording session
./jtag training/start --domain="visual-debugging" --task="layout-overflow"

# ... user guides AI through debugging ...

# End session, store training example
./jtag training/end --outcome="successful" --insights="Border technique isolated container"
```

### Phase 3: LoRA Fine-Tuning (Future)

**Goal**: Encode patterns into fine-tuned layers
**Method**:
1. Aggregate training examples by domain
2. Fine-tune LoRA adapters on domain-specific examples
3. Test adapter activation improves task performance
4. Iterate on training data and hyperparameters

**Tools**: Ollama, llama.cpp, or Hugging Face PEFT library

### Phase 4: Paging System (Future)

**Goal**: Automatic phenotype loading based on tasks
**Method**:
1. Task arrives → infer required phenotypes
2. Load LoRA adapters into model
3. Execute task with specialized knowledge
4. Unload when done (LRU eviction)

**Integration**: PersonaUser.serviceInbox() checks genome before processing

---

## Architecture Integration

### Current PersonaUser Flow

```typescript
// src/debug/jtag/system/user/server/PersonaUser.ts

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

## Next Steps

### Immediate (Infrastructure)
- [x] Verify tools + vision + media flow works ✅
- [x] Fix tool result metadata pollution ✅
- [ ] Design training data collection commands
- [ ] Create training example storage schema

### Short-term (Data Collection)
- [ ] Build training data collection CLI (`./jtag training/...`)
- [ ] Log 50-100 examples per domain
- [ ] Tag and annotate examples
- [ ] Store in `PersonaGenome/training-data/`

### Medium-term (Fine-Tuning)
- [ ] Research LoRA fine-tuning for Ollama models
- [ ] Set up fine-tuning pipeline
- [ ] Train first phenotype (visual-debugging)
- [ ] Test activation improves task performance

### Long-term (Production)
- [ ] Implement PersonaGenome class with paging
- [ ] Integrate with PersonaUser.serviceInbox()
- [ ] Add telemetry for phenotype effectiveness
- [ ] Continuous improvement loop

---

## Technical Stack

**Current Infrastructure:**
- Tool execution: ToolRegistry + Commands system
- Vision: Claude Sonnet 4.5 with screenshot support
- Media flow: PersonaToolExecutor → ChatRAGBuilder → AnthropicAdapter
- Storage: ChatMessageEntity with media[] support

**Future Additions:**
- LoRA loading: Ollama API or llama.cpp
- Training pipeline: Hugging Face PEFT or custom scripts
- Phenotype storage: .safetensors files + manifests
- Paging logic: PersonaGenome class

---

## References

- **Media Flow Verification**: `docs/MILESTONE-AUTONOMOUS-VISUAL-DEBUGGING.md`
- **Tool Infrastructure**: `tools/server/ToolRegistry.ts`
- **PersonaUser Architecture**: `system/user/server/PersonaUser.ts`
- **RAG + Media**: `system/user/server/modules/ChatRAGBuilder.ts`

---

## Conclusion

The LoRA Genome system will transform PersonaUser from a general-purpose AI into a **multi-specialist agent** that can swap expertise based on task demands.

Today's session (2025-11-25) proved the foundational infrastructure works:
- ✅ Tools execute successfully
- ✅ Media flows through vision pipeline
- ✅ AIs can analyze screenshots and reason spatially
- ✅ Instructor-guided learning creates training examples

Next phase: **Collect training data** from real debugging/design/coding sessions, then encode patterns into fine-tuned LoRA phenotypes.

The future: AI assistants that learn specialized skills from your guidance, then apply those skills autonomously when similar tasks arise.

**This is the path to truly capable, continuously learning AI agents.** 🧬
