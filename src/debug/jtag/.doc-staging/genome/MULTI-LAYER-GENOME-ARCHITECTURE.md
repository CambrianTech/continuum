# Multi-Layer Genome Architecture - N-Layer LoRA Composition

**Date**: 2025-11-22
**Status**: Design Document - Not Yet Implemented
**Context**: Evolution from single-layer paging to N-layer PEFT composition

## Executive Summary

**Current**: PersonaGenome.ts implements single-layer virtual memory paging (one adapter active at a time)

**Target**: N-layer PEFT composition enabling hot-swappable phenotypes across three deployment scenarios (local, hybrid, cloud-only)

**Key Innovation**: Create N×M persona combinations from N domain layers + M personality layers without retraining

**Example**: 70% wine-expertise + 30% vin-diesel-personality = Vin Diesel sommelier

---

## Current Implementation Analysis

### PersonaGenome.ts (347 lines) - Single-Layer Paging

**Architecture**: Virtual memory pattern for LoRA adapters

```typescript
// ONE adapter active at a time
await genome.activateSkill("wine-expertise");        // Load adapter
// ... later ...
await genome.activateSkill("typescript-expertise");  // Evict + load different adapter
```

**What Works** ✅:
- **LRU eviction**: Least-recently-used adapter evicted when memory full
- **Memory budget tracking**: Configurable max memory (MB)
- **Disk-based storage**: Adapters stored at local paths
- **Activation/deactivation**: Load from disk, unload to disk
- **Metadata tracking**: Size, lastUsed, domain, importance

**Architectural Limitations** ❌:
1. **Single adapter only** - `this.currentAdapter` holds ONE adapter
2. **No composition** - Cannot combine multiple adapters simultaneously
3. **No PEFT integration** - No `set_adapters()` with dynamic weights
4. **Local-only storage** - Only disk paths, no cloud/hybrid
5. **No dynamic weighting** - Cannot adjust layer influence per task
6. **No hot-swap phenotypes** - Must retrain to change behavior mix

**Key Methods**:
```typescript
class PersonaGenome {
  private currentAdapter: LoRAAdapter | null = null;  // ⚠️ SINGLE adapter
  private activeAdapters: Map<string, LoRAAdapter>;   // In-memory cache
  private availableAdapters: Map<string, LoRAAdapter>; // On-disk registry

  async activateSkill(skillName: string): Promise<void> {
    // Swap to different adapter (evict old if needed)
  }

  async evictLRU(): Promise<void> {
    // Free memory by unloading least-used adapter
  }
}
```

---

## Desired Architecture - N-Layer Composition

### The Vision: Hot-Swappable Phenotypes

**Biological Analogy**:
- **Genotype**: LoRA layer weights (fixed, trained once)
- **Phenotype**: Active behavioral expression (dynamic, composable)
- **Example**: Humans have ONE genome but express different traits in different contexts

**Engineering Goal**: Separate WHAT from HOW MUCH
- **WHAT**: Domain layers (wine-expertise, typescript-mastery, legal-knowledge)
- **HOW MUCH**: Personality layers (vin-diesel-style, shakespeare-eloquence, teacher-patience)
- **Composition**: Mix at runtime with dynamic weights

### N×M Combination Explosion

**Training Efficiency**:
```typescript
// OLD WAY (Single-layer): Need N×M training jobs
await trainLoRA("wine-expertise");                    // 1
await trainLoRA("wine-expertise-vin-diesel");         // 2
await trainLoRA("wine-expertise-shakespeare");        // 3
await trainLoRA("typescript-expertise");              // 4
await trainLoRA("typescript-expertise-vin-diesel");   // 5
await trainLoRA("typescript-expertise-shakespeare");  // 6
// Result: 6 training jobs → 6 personas

// NEW WAY (Multi-layer): Need N+M training jobs
await trainLoRA({ traitType: "wine-expertise" });      // Domain 1
await trainLoRA({ traitType: "typescript-expertise"}); // Domain 2
await trainLoRA({ traitType: "vin-diesel-style" });    // Personality 1
await trainLoRA({ traitType: "shakespeare-eloquence" }); // Personality 2
// Result: 4 training jobs → 2×2 = 4 personas (AND more combinations!)
```

**Scaling**:
- 10 domains + 5 personalities = **15 training jobs → 50 persona combinations**
- vs single-layer = **50 training jobs**

### PEFT Multi-Layer Composition Pattern

**PEFT (Parameter-Efficient Fine-Tuning)** library provides `set_adapters()`:

```python
# Python PEFT example (target architecture)
from peft import PeftModel

# Load base model
model = AutoModelForCausalLM.from_pretrained("meta-llama/Llama-3.1-8B")
peft_model = PeftModel.from_pretrained(model, "base-adapter")

# Load multiple adapters
peft_model.load_adapter("wine-expertise", adapter_name="domain")
peft_model.load_adapter("vin-diesel-style", adapter_name="personality")

# Set active adapters with weights
peft_model.set_adapters(
    ["domain", "personality"],
    weights=[0.7, 0.3]  # 70% expertise, 30% personality
)

# Generate with combined phenotype
output = peft_model.generate("What's the best Bordeaux vintage?")
# Response: Vin Diesel persona talking about wine!
```

**TypeScript Equivalent** (needs implementation):

```typescript
// Target API for PersonaGenome
await genome.activateLayers([
  { name: "wine-expertise", weight: 0.7, type: "domain" },
  { name: "vin-diesel-style", weight: 0.3, type: "personality" }
]);

// Dynamic weight adjustment per task
if (taskComplexity === 'nuanced') {
  genome.adjustWeights({ "wine-expertise": 0.9, "vin-diesel-style": 0.1 });
} else {
  genome.adjustWeights({ "wine-expertise": 0.6, "vin-diesel-style": 0.4 });
}

// Query active phenotype
const phenotype = await genome.getActivePhenotype();
// { layers: [...], totalWeight: 1.0, expressionProfile: {...} }
```

---

## Three Deployment Scenarios

### Scenario 1: Local-Only (Full Control)

**Storage**: All LoRA weights stored locally on disk
**PEFT**: Native PEFT library with `set_adapters()`
**Inference**: Local Ollama with PEFT support

```typescript
interface LocalGenomeConfig {
  storage: 'local';
  adapterPath: string;           // e.g., ~/.continuum/lora-adapters/
  baseModel: string;             // e.g., "llama3.1:8b"
  peftLibrary: 'transformers' | 'ollama-peft';
  memoryBudgetMB: number;
}

// Load adapters from disk, use PEFT to compose
await genome.activateLayers([
  { name: "wine-expertise", weight: 0.7, path: "./adapters/wine-expertise/" },
  { name: "vin-diesel-style", weight: 0.3, path: "./adapters/vin-diesel/" }
]);
```

**Benefits**:
- Full control over weights
- Dynamic composition at runtime
- No API costs
- Privacy-preserving

**Limitations**:
- Requires PEFT-compatible local inference (Ollama + PEFT?)
- Memory constraints

### Scenario 2: Hybrid (Local + Cloud)

**Storage**: Some adapters local, some cloud (Fireworks, OpenAI)
**PEFT**: Pre-merged adapters deployed to cloud
**Inference**: Route to appropriate backend based on layer availability

```typescript
interface HybridGenomeConfig {
  storage: 'hybrid';
  localAdapters: string[];       // Can be composed locally
  cloudAdapters: CloudAdapter[]; // Pre-merged, deployed remotely
  fallbackStrategy: 'local' | 'cloud' | 'decompose';
}

interface CloudAdapter {
  name: string;
  provider: 'fireworks' | 'openai' | 'together';
  modelId: string;               // e.g., "accounts/joel/models/wine-expert-vinstyle"
  composedFrom: string[];        // Source layers
  weights: number[];             // Merge weights
}

// Request phenotype
await genome.activateLayers([
  { name: "wine-expertise", weight: 0.7, location: "local" },
  { name: "vin-diesel-style", weight: 0.3, location: "cloud:fireworks" }
]);

// Genome resolves to:
// 1. Check if cloud has pre-merged version
// 2. If not, decompose: use local wine-expertise, use base model with vin-diesel from cloud
// 3. Or fallback to nearest available combination
```

**Benefits**:
- Leverage cloud LoRA hosting (Fireworks)
- Mix local privacy with cloud scale
- Cache popular combinations in cloud

**Limitations**:
- Limited dynamic composition (pre-merge offline)
- Network latency
- API costs for cloud layers

### Scenario 3: Cloud-Only (Cannot Download Weights)

**Storage**: All adapters hosted remotely, weights not accessible
**PEFT**: Pre-merged adapters only (offline TIES/DARE merging)
**Inference**: Pure API calls to hosted LoRA models

```typescript
interface CloudOnlyGenomeConfig {
  storage: 'cloud-only';
  provider: 'fireworks' | 'openai' | 'together';
  availableModels: string[];     // Pre-merged models only
  compositionStrategy: 'offline-merge-only';
}

// Request phenotype
await genome.activatePhenotype("wine-expert-vinstyle");
// Maps to: accounts/joel/models/wine-expert-vinstyle (pre-merged)

// Dynamic composition NOT POSSIBLE
// Must pre-merge popular combinations and deploy as separate models
```

**Benefits**:
- No local storage required
- Leverage provider GPU infrastructure
- Can use providers that don't allow weight downloads (OpenAI)

**Limitations**:
- **No runtime composition** - must pre-merge
- **Combinatorial explosion** - N×M models to deploy
- **No dynamic weighting** - fixed at merge time
- API costs

---

## Architectural Components

### 1. GenomeStorage (Abstraction Layer)

**Purpose**: Abstract storage across local/cloud scenarios

```typescript
interface IGenomeStorage {
  // Adapter discovery
  listAvailableAdapters(): Promise<AdapterMetadata[]>;

  // Adapter loading
  loadAdapter(name: string): Promise<LoRAWeights | CloudAdapterRef>;

  // Composition support
  supportsRuntimeComposition(): boolean;
  getCompositionStrategy(): 'peft' | 'offline-merge' | 'none';
}

class LocalGenomeStorage implements IGenomeStorage {
  async loadAdapter(name: string): Promise<LoRAWeights> {
    // Load from disk, return raw weights
    const path = path.join(this.adapterPath, name);
    return await fs.readFile(path); // Simplified
  }

  supportsRuntimeComposition(): boolean {
    return true; // PEFT can compose at runtime
  }
}

class CloudGenomeStorage implements IGenomeStorage {
  async loadAdapter(name: string): Promise<CloudAdapterRef> {
    // Return reference to cloud-hosted adapter
    return {
      provider: this.provider,
      modelId: this.resolveModelId(name),
      composedFrom: this.getCompositionMetadata(name)
    };
  }

  supportsRuntimeComposition(): boolean {
    return false; // Must use pre-merged models
  }
}
```

### 2. GenomeCompositor (PEFT Integration)

**Purpose**: Compose multiple LoRA layers with dynamic weights

```typescript
interface LayerActivation {
  name: string;
  weight: number;
  type: 'domain' | 'personality' | 'skill';
}

class GenomeCompositor {
  private peftModel: PEFTModel; // Hypothetical PEFT wrapper
  private activeLayerStack: LayerActivation[] = [];

  /**
   * Activate N layers with specified weights
   * Weights must sum to 1.0
   */
  async activateLayers(layers: LayerActivation[]): Promise<void> {
    // Validate weights sum to 1.0
    const totalWeight = layers.reduce((sum, l) => sum + l.weight, 0);
    if (Math.abs(totalWeight - 1.0) > 0.01) {
      throw new Error(`Weights must sum to 1.0, got ${totalWeight}`);
    }

    // Load adapters into PEFT
    for (const layer of layers) {
      await this.peftModel.loadAdapter(layer.name, { adapterName: layer.name });
    }

    // Set active adapters with weights
    const names = layers.map(l => l.name);
    const weights = layers.map(l => l.weight);
    await this.peftModel.setAdapters(names, { weights });

    this.activeLayerStack = layers;
  }

  /**
   * Dynamically adjust weights without reloading
   * Example: Increase expertise for complex tasks
   */
  async adjustWeights(weightMap: Record<string, number>): Promise<void> {
    for (const layer of this.activeLayerStack) {
      if (weightMap[layer.name] !== undefined) {
        layer.weight = weightMap[layer.name];
      }
    }

    // Re-apply weights to PEFT model
    const names = this.activeLayerStack.map(l => l.name);
    const weights = this.activeLayerStack.map(l => l.weight);
    await this.peftModel.setAdapters(names, { weights });
  }

  /**
   * Get current phenotype expression
   */
  getActivePhenotype(): PhenotypeProfile {
    return {
      layers: this.activeLayerStack,
      totalWeight: this.activeLayerStack.reduce((sum, l) => sum + l.weight, 0),
      dominantLayer: this.activeLayerStack.reduce((max, l) =>
        l.weight > max.weight ? l : max
      ),
      expressionProfile: this.calculateExpression()
    };
  }

  private calculateExpression(): ExpressionProfile {
    // Calculate weighted influence of each trait
    const domainInfluence = this.sumWeightsByType('domain');
    const personalityInfluence = this.sumWeightsByType('personality');
    const skillInfluence = this.sumWeightsByType('skill');

    return { domainInfluence, personalityInfluence, skillInfluence };
  }
}
```

### 3. Enhanced PersonaGenome (Integration Point)

**Purpose**: Unified genome interface integrating paging + composition

```typescript
class PersonaGenome {
  private storage: IGenomeStorage;           // Local, cloud, or hybrid
  private compositor: GenomeCompositor;      // PEFT multi-layer composition
  private pager: GenomePager;                // LRU eviction, memory management

  constructor(config: GenomeConfig) {
    // Select storage strategy
    if (config.storage === 'local') {
      this.storage = new LocalGenomeStorage(config);
    } else if (config.storage === 'cloud-only') {
      this.storage = new CloudGenomeStorage(config);
    } else {
      this.storage = new HybridGenomeStorage(config);
    }

    // Initialize compositor if runtime composition supported
    if (this.storage.supportsRuntimeComposition()) {
      this.compositor = new GenomeCompositor(config.peftModel);
    }

    // Initialize pager for memory management
    this.pager = new GenomePager(config.memoryBudgetMB);
  }

  /**
   * Activate N-layer phenotype
   * If runtime composition supported: Use PEFT
   * If not: Resolve to nearest pre-merged model
   */
  async activatePhenotype(layers: LayerActivation[]): Promise<void> {
    if (this.compositor) {
      // Runtime composition (local or hybrid with PEFT)
      await this.compositor.activateLayers(layers);
    } else {
      // Cloud-only: Resolve to pre-merged model
      const phenotypeId = this.resolvePhenotypeId(layers);
      await this.storage.loadAdapter(phenotypeId);
    }

    // Update pager with active layers
    await this.pager.trackActivation(layers);
  }

  /**
   * Adjust layer weights dynamically (if supported)
   * Example: Increase domain expertise when task complexity rises
   */
  async adjustWeights(
    weightMap: Record<string, number>,
    reason?: string
  ): Promise<void> {
    if (!this.compositor) {
      throw new Error('Dynamic weight adjustment requires runtime composition');
    }

    await this.compositor.adjustWeights(weightMap);

    // Log adjustment for observability
    console.log(`🧬 Genome: Adjusted weights ${JSON.stringify(weightMap)} - ${reason}`);
  }

  /**
   * Evict least-used layers to free memory
   * Integrates with existing LRU pager
   */
  async evictLRU(): Promise<void> {
    const victim = await this.pager.selectVictim();
    await this.compositor.unloadAdapter(victim.name);
    console.log(`🧬 Genome: Evicted ${victim.name} (LRU)`);
  }

  /**
   * Get current phenotype expression profile
   */
  async getActivePhenotype(): Promise<PhenotypeProfile> {
    if (this.compositor) {
      return this.compositor.getActivePhenotype();
    } else {
      // Cloud-only: Return metadata about active model
      return this.storage.getPhenotypeMetadata();
    }
  }
}
```

---

## Integration with SPIKE Escalation

**Context**: SPIKE (adaptive-complexity-routing.md) routes tasks to appropriate model tier

**Current**: SPIKE routes to different MODEL FAMILIES (llama3.1, claude-3.5, gpt-4o)

**Future**: SPIKE routes to different LAYER COMPOSITIONS within SAME base model

### Architecture

```typescript
interface ComplexityAdaptiveGenome {
  /**
   * Adjust genome composition based on task complexity
   * Complexity detected by ComplexityDetector, passed to genome
   */
  async adaptToComplexity(
    complexity: ComplexityLevel,
    domain: string
  ): Promise<void> {
    if (complexity === 'straightforward') {
      // Prioritize speed: Lightweight personality, minimal domain depth
      await this.activatePhenotype([
        { name: `${domain}-basics`, weight: 0.8, type: 'skill' },
        { name: 'concise-style', weight: 0.2, type: 'personality' }
      ]);

    } else if (complexity === 'moderate') {
      // Balanced: Domain expertise + personality
      await this.activatePhenotype([
        { name: `${domain}-expertise`, weight: 0.7, type: 'domain' },
        { name: 'default-personality', weight: 0.3, type: 'personality' }
      ]);

    } else if (complexity === 'nuanced') {
      // Prioritize depth: Maximum domain knowledge
      await this.activatePhenotype([
        { name: `${domain}-mastery`, weight: 0.9, type: 'domain' },
        { name: 'thoughtful-style', weight: 0.1, type: 'personality' }
      ]);
    }
  }
}
```

**Integration Point**: PersonaMessageEvaluator.evaluateShouldRespond()

```typescript
// In PersonaMessageEvaluator
const complexity = await this.complexityDetector.detect(message);

// Adapt genome layers to complexity
await this.persona.genome.adaptToComplexity(complexity, message.domain);

// Process with adapted phenotype
const response = await this.persona.processMessage(message);
```

**Benefits**:
- Single base model (llama3.1:8b) handles all complexity levels
- Swap layers instead of swapping models
- Faster than model switching (layers are smaller than full models)
- No need to load multiple entire models into memory

---

## Offline Merging for Cloud Providers

**Problem**: Cloud providers (Fireworks, OpenAI) host adapters but don't support runtime PEFT composition

**Solution**: Pre-merge popular combinations offline, deploy as separate models

### Merge Methods

**TIES (Task Interpolation Elimination and Sign)** - Best for combining dissimilar adapters:
```python
from peft import merge_adapters

merged = merge_adapters(
    ["wine-expertise", "vin-diesel-style"],
    weights=[0.7, 0.3],
    method="ties",
    density=0.5  # Keep top 50% of parameters
)

# Save merged adapter
merged.save_pretrained("./wine-expert-vinstyle")

# Deploy to Fireworks
fireworks.upload_model("wine-expert-vinstyle")
```

**DARE (Drop And REscale)** - Randomly drop parameters:
```python
merged = merge_adapters(
    ["wine-expertise", "vin-diesel-style"],
    weights=[0.7, 0.3],
    method="dare",
    drop_rate=0.3  # Drop 30% of parameters
)
```

**Linear** - Simple weighted average (fastest):
```python
merged = merge_adapters(
    ["wine-expertise", "vin-diesel-style"],
    weights=[0.7, 0.3],
    method="linear"
)
```

### CLI Commands for Offline Merging

```bash
# Merge two adapters locally
./jtag genome/merge \
  --adapters='["wine-expertise","vin-diesel-style"]' \
  --weights='[0.7,0.3]' \
  --method="ties" \
  --output="wine-expert-vinstyle"

# Deploy to cloud provider
./jtag genome/deploy \
  --adapter="wine-expert-vinstyle" \
  --provider="fireworks" \
  --modelName="accounts/joel/models/wine-expert-vinstyle"

# List pre-merged cloud models
./jtag genome/list-cloud --provider="fireworks"

# Use pre-merged model
./jtag genome/activate-phenotype \
  --phenotype="wine-expert-vinstyle" \
  --provider="fireworks"
```

---

## Implementation Phases

### Phase 1: Multi-Layer Activation (Foundation)

**Goal**: Support N-layer composition in PersonaGenome

**Tasks**:
1. Refactor PersonaGenome to support `activeLayerStack: LayerActivation[]` instead of `currentAdapter`
2. Implement `activateLayers(layers)` method
3. Add weight normalization/validation
4. Update memory budget tracking to handle N layers
5. Extend LRU eviction to consider layer importance + type

**Testing**:
```typescript
await genome.activateLayers([
  { name: "wine-expertise", weight: 0.7, type: "domain" },
  { name: "vin-diesel-style", weight: 0.3, type: "personality" }
]);

const phenotype = await genome.getActivePhenotype();
assert(phenotype.layers.length === 2);
assert(phenotype.totalWeight === 1.0);
```

**Deliverable**: PersonaGenome supports N-layer API (without PEFT integration yet)

### Phase 2: PEFT Integration (Local Runtime Composition)

**Goal**: Integrate PEFT library for runtime layer composition

**Prerequisites**:
- Ollama with PEFT support OR
- Python PEFT server with TypeScript client OR
- Native TypeScript PEFT implementation

**Tasks**:
1. Create `GenomeCompositor` class wrapping PEFT
2. Implement `set_adapters()` equivalent in TypeScript
3. Add dynamic weight adjustment
4. Test with Ollama + PEFT-patched models
5. Benchmark inference latency vs single-layer

**Testing**:
```bash
# Train two adapters
./jtag genome/train --adapter="wine-expertise" --dataset="wine-qa"
./jtag genome/train --adapter="vin-diesel-style" --dataset="vin-diesel-quotes"

# Compose at runtime
./jtag genome/activate-phenotype \
  --layers='[{"name":"wine-expertise","weight":0.7},{"name":"vin-diesel-style","weight":0.3}]'

# Test generation
./jtag collaboration/chat/send --room="general" --message="What's the best Bordeaux vintage?"
# Expected: Vin Diesel personality talking about wine
```

**Deliverable**: Local PEFT composition working with Ollama

### Phase 3: Cloud-Hybrid Storage (Abstraction)

**Goal**: Support local, cloud, and hybrid storage scenarios

**Tasks**:
1. Define `IGenomeStorage` interface
2. Implement `LocalGenomeStorage` (disk-based)
3. Implement `CloudGenomeStorage` (Fireworks API)
4. Implement `HybridGenomeStorage` (mixed)
5. Add offline merge CLI commands (`./jtag genome/merge`)
6. Add cloud deployment commands (`./jtag genome/deploy`)

**Testing**:
```bash
# Local storage
export GENOME_STORAGE=local
./jtag genome/activate-phenotype --layers='[...]'

# Cloud-only storage
export GENOME_STORAGE=cloud
export GENOME_PROVIDER=fireworks
./jtag genome/activate-phenotype --phenotype="wine-expert-vinstyle"

# Hybrid storage
export GENOME_STORAGE=hybrid
export GENOME_LOCAL_ADAPTERS='["wine-expertise"]'
export GENOME_CLOUD_ADAPTERS='["vin-diesel-style"]'
./jtag genome/activate-phenotype --layers='[...]'
# System resolves to nearest available pre-merged model or decomposes
```

**Deliverable**: Genome storage abstraction supporting three scenarios

### Phase 4: SPIKE Escalation Integration

**Goal**: Adapt genome composition based on task complexity

**Tasks**:
1. Extend `PersonaGenome` with `adaptToComplexity()` method
2. Define complexity → layer mapping strategies
3. Integrate with `PersonaMessageEvaluator` complexity detection
4. Add complexity-adaptive weighting (straightforward: 80/20, nuanced: 90/10)
5. Benchmark latency vs full model swapping

**Testing**:
```bash
# Send straightforward message
./jtag collaboration/chat/send --room="general" --message="Hi"
# Genome should activate lightweight layers

# Send nuanced message
./jtag collaboration/chat/send --room="general" --message="Compare the philosophical implications of actor-critic vs PPO in RLHF"
# Genome should activate deep expertise layers

# Check active phenotype
./jtag genome/status
# Should show layer weights adjusted per complexity
```

**Deliverable**: Genome adapts layers based on SPIKE complexity detection

### Phase 5: Continuous Learning Integration

**Goal**: LoRA training as just another task in task system

**Tasks**:
1. Define `fine-tune-lora` task type in TaskEntity
2. Create `./jtag genome/train` command → enqueues training task
3. PersonaUser processes training tasks via genome module
4. After training: Hot-reload new adapter into active layer stack
5. Self-task generation: Persona autonomously creates training tasks based on mistakes

**Testing**:
```bash
# Manual training task
./jtag task/create \
  --assignee="helper-ai-id" \
  --taskType="fine-tune-lora" \
  --params='{"targetSkill":"wine-expertise","dataset":"recent-wine-mistakes"}'

# AI autonomously detects poor performance
# Creates self-task: "I need to improve my wine knowledge"
./jtag task/list --assignee="helper-ai-id" --filter='{"createdBy":"helper-ai-id"}'
# Shows self-created training task

# After training completes
./jtag genome/status
# Shows newly trained layer active in composition
```

**Deliverable**: Training integrated into autonomous task loop

---

## Design Questions

### Question 1: GenomeCompositor vs PersonaGenome Responsibilities

**Option A**: PersonaGenome handles composition directly
```typescript
class PersonaGenome {
  private peftModel: PEFTModel;
  async activateLayers(layers: LayerActivation[]): Promise<void> {
    // PersonaGenome owns PEFT integration
  }
}
```

**Option B**: Separate GenomeCompositor class
```typescript
class GenomeCompositor {
  async activateLayers(layers: LayerActivation[]): Promise<void> {
    // Compositor owns PEFT integration
  }
}

class PersonaGenome {
  private compositor: GenomeCompositor;
  async activateLayers(layers: LayerActivation[]): Promise<void> {
    // Delegate to compositor
    return this.compositor.activateLayers(layers);
  }
}
```

**Recommendation**: **Option B** - Separate concerns
- PersonaGenome: Memory management, paging, storage, lifecycle
- GenomeCompositor: PEFT integration, weight math, layer stacking
- Allows testing composition logic independently
- Cleaner abstraction boundaries

### Question 2: PEFT Runtime vs Python Server

**Option A**: Native TypeScript PEFT
- Implement PEFT algorithms in TypeScript
- Tight integration with Ollama
- No Python dependency

**Option B**: Python PEFT server + TypeScript client
- Use battle-tested `peft` library
- TypeScript client calls Python server via RPC
- Simpler TypeScript code

**Recommendation**: **Option B initially** (then Option A long-term)
- PEFT library is complex, well-tested
- Use Python server for proof-of-concept
- Migrate to native TypeScript once patterns proven

### Question 3: Cloud-Only Fallback Strategy

**Scenario**: User requests layers [A:0.6, B:0.4] but cloud only has pre-merged [A:0.5, B:0.5]

**Option A**: Use nearest pre-merged (ignore weights)
```typescript
// Requested: A:0.6, B:0.4
// Available: A:0.5, B:0.5
// Action: Use A:0.5, B:0.5 (closest match)
```

**Option B**: Decompose to single layer
```typescript
// Requested: A:0.6, B:0.4
// Fallback: Use only A:1.0 (dominant layer)
```

**Option C**: Error and force explicit selection
```typescript
// Requested: A:0.6, B:0.4
// Error: "Cloud provider does not support requested weights. Available: ..."
// User must choose from pre-merged models
```

**Recommendation**: **Option A with warning**
- Use nearest pre-merged model
- Log warning: "Requested weights not available, using closest match"
- Allow override via config: `genomeConfig.cloudFallback = 'nearest' | 'decompose' | 'error'`

### Question 4: SPIKE Integration Point

**Where should genome adaptation happen?**

**Option A**: In PersonaMessageEvaluator (before response generation)
```typescript
const complexity = await this.complexityDetector.detect(message);
await this.persona.genome.adaptToComplexity(complexity, domain);
const response = await this.persona.generateResponse(message);
```

**Option B**: In PersonaGenome (automatic based on context)
```typescript
// Genome observes PersonaState and adapts automatically
class PersonaGenome {
  async tick(): Promise<void> {
    const context = await this.persona.selfState.get();
    if (context.cognitiveLoad > 0.8) {
      // Under heavy load: Use lightweight layers
      await this.adaptToLoad('low-latency');
    }
  }
}
```

**Recommendation**: **Option A** (explicit in evaluator)
- Clearer causality (complexity → genome → response)
- Easier to debug and observe
- Option B can be added later for autonomous adaptation

---

## Success Criteria

**Phase 1 Complete**:
- [ ] PersonaGenome API supports N-layer activation
- [ ] Weight normalization and validation working
- [ ] LRU eviction considers layer type/importance
- [ ] Tests pass for multi-layer activation

**Phase 2 Complete**:
- [ ] PEFT integration working with local Ollama
- [ ] Dynamic weight adjustment functional
- [ ] Benchmark: Multi-layer latency < 150% of single-layer
- [ ] Example phenotype: Vin Diesel sommelier generates expected responses

**Phase 3 Complete**:
- [ ] IGenomeStorage abstraction implemented
- [ ] Local, cloud, hybrid storage scenarios working
- [ ] Offline merge CLI commands functional
- [ ] Cloud deployment commands working (Fireworks)

**Phase 4 Complete**:
- [ ] SPIKE complexity detection triggers genome adaptation
- [ ] Layer weights adjust based on complexity (straightforward: 80/20, nuanced: 90/10)
- [ ] Latency improvement vs full model swapping (target: 3x faster)

**Phase 5 Complete**:
- [ ] Training tasks enqueued via `./jtag genome/train`
- [ ] PersonaUser processes training tasks
- [ ] Hot-reload newly trained adapters
- [ ] Self-generated training tasks appear in task database

---

## Related Documents

**Current Implementation**:
- `system/user/server/modules/PersonaGenome.ts` (347 lines) - Single-layer paging

**Multi-Layer Vision**:
- `dynamic-composition-roadmap.md` - PEFT N-layer composition (this doc supersedes it)
- `lora-genome-paging.md` - Virtual memory pattern for adapters

**SPIKE Integration**:
- `adaptive-complexity-routing.md` - Complexity-based model routing
- `adaptive-thresholds.md` - Thermal gating for response decisions

**Training Infrastructure**:
- `genome-fine-tuning-e2e.md` - End-to-end training pipeline
- `genome-training-abstraction.md` - Multi-backend training API

**RTOS Architecture**:
- `PERSONA-CONVERGENCE-ROADMAP.md` - Three pillar integration (autonomous loop, self-managed queues, genome paging)
- `AUTONOMOUS-LOOP-ROADMAP.md` - RTOS servicing pattern

---

## Appendix: PEFT Research

**Papers**:
- LoRA: Low-Rank Adaptation of Large Language Models (Hu et al., 2021)
- TIES-Merging: Resolving Interference When Merging Models (Yadav et al., 2023)
- DARE: Drop And REscale for Parameter-Efficient Merging (Yu et al., 2023)

**Libraries**:
- Hugging Face PEFT: https://github.com/huggingface/peft
- Ollama PEFT Support: https://ollama.com/blog/lora-adapters (if exists)

**Providers Supporting LoRA Hosting**:
- Fireworks AI: Custom LoRA deployment
- Together AI: Adapter hosting
- Replicate: LoRA fine-tuning + hosting
- OpenAI: Fine-tuning (but no LoRA, full model only)

