# PersonaUser Dynamic Genome Architecture

## Executive Summary

**Goal**: Give PersonaUsers modular, composable LoRA layers that can be trained once and reused across all personas.

**Core Insight**: Dynamic composition eliminates combinatorial explosion - train N domains + M personalities, get N×M personas!

**Architecture**: Two-tier system supporting both single-layer (simple/remote) and multi-layer (complex/local) genomes.

---

## Design Principles

### 1. **Single-Layer Personas Have Great Utility**

Not every PersonaUser needs multiple genomic layers:
- **Simple assistants**: One domain-specific layer (e.g., "typescript-expert")
- **Personality-only**: One style layer (e.g., "helpful-assistant")
- **Remote APIs**: Fireworks, OpenAI limited to one composite per inference

**We architect for N-layer capability while making single-layer personas work beautifully today.**

### 2. **Keep Layers Together When Cost Is Low**

Only page/evict adapters when:
- Memory pressure exceeds threshold (>80% GPU memory)
- Switching to completely different domain (chat → code → academy)
- Provider limitations force it (Fireworks maxLayers: 1)

**For local PEFT inference with sufficient memory, keep all active layers loaded simultaneously.**

### 3. **Modular Training, Not Persona-Specific**

**❌ Old approach (combinatorial explosion)**:
```typescript
// Train one model per complete persona
trainLoRA({ personaId: "vin-diesel-sommelier", dataset: combinedData });
trainLoRA({ personaId: "shakespeare-sommelier", dataset: combinedData2 });
trainLoRA({ personaId: "einstein-sommelier", dataset: combinedData3 });
// Problem: 10 personas = 10 training jobs, can't share knowledge
```

**✅ New approach (modular composition)**:
```typescript
// Train modular layers ONCE
trainLoRA({ traitType: "wine-expertise", category: "domain" });
trainLoRA({ traitType: "vin-diesel-style", category: "personality" });
trainLoRA({ traitType: "shakespeare-style", category: "personality" });

// Compose dynamically at inference
persona.genome.setLayers([
  { name: "wine-expertise", weight: 0.7 },
  { name: "vin-diesel-style", weight: 0.3 }
]);
// Get: 1 domain × 3 personalities = 3 personas from 4 training jobs!
```

---

## Architecture Overview

### PersonaGenome Class

```typescript
/**
 * PersonaGenome - Modular LoRA layer management
 *
 * Supports two modes:
 * 1. Single-layer (simple personas, remote APIs)
 * 2. Multi-layer (complex personas, local PEFT)
 */
export class PersonaGenome {
  // Provider capabilities
  readonly provider: 'peft' | 'fireworks' | 'openai' | 'deepseek' | 'together';
  readonly maxActiveLayers: number;  // PEFT: ∞, Fireworks: 1

  // Base model (always loaded)
  readonly baseModel: string;  // "meta-llama/Llama-3.1-8B"

  // Available modular layers
  private availableLayers: Map<string, GenomeLayerEntity>;

  // Currently active composition
  private activeComposition: {
    layers: Array<{ name: string; weight: number }>;
    method: 'stack' | 'weighted' | 'TIES' | 'DARE';
  };

  // Memory management
  private memoryUsage: number;        // Current GPU memory (MB)
  private memoryBudget: number;       // Max allowed (MB)
  private lastUsed: Map<string, number>;  // LRU tracking

  /**
   * Set active layer composition
   *
   * For single-layer providers (Fireworks): Uses pre-merged composite
   * For multi-layer providers (PEFT): Dynamically composes at runtime
   */
  async setLayers(
    layers: Array<{ name: string; weight: number }>,
    method?: 'stack' | 'weighted'
  ): Promise<void> {
    if (layers.length > this.maxActiveLayers) {
      throw new Error(
        `Provider ${this.provider} supports max ${this.maxActiveLayers} layers, ` +
        `got ${layers.length}`
      );
    }

    // Update composition
    this.activeComposition = { layers, method: method || 'stack' };

    // Execute provider-specific composition
    if (this.provider === 'peft') {
      await this.composePEFT(layers, method);
    } else {
      await this.usePremergedComposite(layers);
    }
  }

  /**
   * Generate response using current genome composition
   */
  async generate(prompt: string, context?: AIContext): Promise<string> {
    // Use provider-specific inference with active composition
    switch (this.provider) {
      case 'peft':
        return await this.peftInference(prompt, context);
      case 'fireworks':
        return await this.fireworksInference(prompt, context);
      default:
        throw new Error(`Provider ${this.provider} not implemented`);
    }
  }

  /**
   * Check if layer is available (trained and downloaded)
   */
  hasLayer(layerName: string): boolean {
    return this.availableLayers.has(layerName);
  }

  /**
   * Get current memory pressure (0.0-1.0)
   */
  get memoryPressure(): number {
    return this.memoryUsage / this.memoryBudget;
  }

  /**
   * Evict least-recently-used layer (LRU)
   */
  async evictLRU(): Promise<void> {
    // Find LRU layer not in active composition
    const activeLayers = new Set(this.activeComposition.layers.map(l => l.name));
    const evictable = Array.from(this.lastUsed.entries())
      .filter(([name]) => !activeLayers.has(name))
      .sort(([, a], [, b]) => a - b);

    if (evictable.length === 0) {
      throw new Error('Cannot evict - all loaded layers are active');
    }

    const [layerToEvict] = evictable[0];
    await this.unloadLayer(layerToEvict);
    console.log(`🗑️  Genome: Evicted LRU layer "${layerToEvict}"`);
  }
}
```

---

## GenomeLayerEntity Schema

```typescript
/**
 * GenomeLayerEntity - Persistent storage for trained LoRA layers
 *
 * Supports both modular layers (single skill) and composites (merged)
 */
export interface GenomeLayerEntity extends BaseEntity {
  static readonly collection = 'genome_layers';

  // Identity
  id: UUID;
  name: string;  // "wine-expertise", "vin-diesel-style", "vin-diesel-sommelier"

  // Layer type
  layerType: 'modular' | 'composite';

  // Modular layer metadata
  category?: 'domain' | 'personality' | 'skill';  // For modular only
  traitType?: string;  // "wine-expertise", "typescript-expert"

  // Composite layer metadata
  composition?: {
    method: 'stack' | 'weighted' | 'TIES' | 'DARE';
    adapters: Array<{
      name: string;     // Reference to modular layer
      weight: number;   // 0.0-1.0
    }>;
  };

  // Training metadata
  baseModel: string;                // "meta-llama/Llama-3.1-8B"
  provider: 'peft' | 'fireworks' | 'openai' | 'deepseek' | 'together';
  trainingJobId: UUID;              // Link to TrainingSessionEntity
  providerJobId?: string;           // Provider's job ID

  // Storage
  localPath?: string;               // Local file path (PEFT safetensors)
  remoteId?: string;                // Remote adapter ID (Fireworks, etc.)
  sizeBytes?: number;               // Memory footprint

  // Lifecycle
  createdAt: number;
  lastUsedAt?: number;              // For LRU eviction
  downloadedAt?: number;            // When pulled from remote
  isAvailable: boolean;             // Ready for use (downloaded + converted)

  // Metadata
  description?: string;
  tags?: string[];
  performance?: {
    finalLoss?: number;
    accuracy?: number;
    examplesProcessed?: number;
  };
}
```

---

## Integration with PersonaUser Convergence

### How Genome Fits Into Universal Cognitive Cycle

```typescript
// From PERSONA-CONVERGENCE-ROADMAP.md
async serviceInbox(): Promise<void> {
  const tasks = await this.inbox.peek(10);
  if (tasks.length === 0) {
    await this.rest();
    return;
  }

  await this.generateSelfTasks();

  const task = tasks[0];
  if (!this.state.shouldEngage(task.priority)) {
    return;
  }

  // ==== GENOME ACTIVATION ====
  // Determine required layers based on task domain
  const layers = await this.selectLayersForTask(task);

  // Activate appropriate genome composition
  await this.genome.setLayers(layers);

  // ==== END GENOME ACTIVATION ====

  const permission = await this.coordinator.requestTurn(task);
  await this.processTask(task);
  await this.state.recordActivity(task.duration, task.complexity);

  // Memory management (only if pressure high)
  if (this.genome.memoryPressure > 0.8) {
    await this.genome.evictLRU();
  }
}

/**
 * Select genomic layers based on task domain
 *
 * Examples:
 * - Chat task → [conversational-skill]
 * - Code review → [typescript-expertise, helpful-assistant]
 * - Wine question → [wine-expertise, vin-diesel-style]
 */
private async selectLayersForTask(task: TaskEntity): Promise<Array<{name: string; weight: number}>> {
  switch (task.domain) {
    case 'chat':
      return [{ name: 'conversational-skill', weight: 1.0 }];

    case 'code':
      return [
        { name: 'typescript-expertise', weight: 0.8 },
        { name: this.personalityStyle, weight: 0.2 }
      ];

    case 'wine':
      return [
        { name: 'wine-expertise', weight: 0.7 },
        { name: this.personalityStyle, weight: 0.3 }
      ];

    default:
      // Fallback: personality only
      return [{ name: this.personalityStyle, weight: 1.0 }];
  }
}
```

---

## Provider Capabilities Matrix

| Provider | Max Layers | Dynamic Composition | Cost/1M Tokens | Latency | Notes |
|----------|-----------|---------------------|----------------|---------|-------|
| **PEFT (local)** | Unlimited | ✅ Runtime | $0 (own GPU) | Low | Full control, memory limited |
| **Fireworks** | 1 | ❌ Pre-merged | $0.20 | Low | Deploy composites offline |
| **OpenAI** | 1 | ❌ Pre-merged | $15.00 | Medium | Expensive, high quality |
| **DeepSeek** | 1 | ❌ Pre-merged | $0.55 | Medium | Cheapest API |
| **Together** | 1 | ❌ Pre-merged | $0.60 | Low | Good balance |

---

## Two-Tier Deployment Strategy

### Tier 1: Local PEFT (Development & Power Users)

**Use Case**: Complex personas with multiple specialized skills

**Benefits**:
- Unlimited dynamic composition
- Instant layer switching (< 1ms)
- Zero inference cost
- Full control over weights

**Example**:
```typescript
// PersonaUser with PEFT
const persona = new PersonaUser({
  name: "CodeReviewBot",
  provider: "peft",
  baseModel: "meta-llama/Llama-3.1-8B",
  availableLayers: [
    "typescript-expertise",
    "python-expertise",
    "rust-expertise",
    "helpful-assistant",
    "concise-reviewer"
  ]
});

// Task 1: TypeScript PR review
await persona.genome.setLayers([
  { name: "typescript-expertise", weight: 0.8 },
  { name: "helpful-assistant", weight: 0.2 }
]);

// Task 2: Python PR review (instant switch!)
await persona.genome.setLayers([
  { name: "python-expertise", weight: 0.8 },
  { name: "helpful-assistant", weight: 0.2 }
]);
```

### Tier 2: Remote APIs (Production & Scale)

**Use Case**: Simple personas or scaled deployment

**Benefits**:
- No local GPU required
- Handles burst traffic
- Pay per use

**Example**:
```typescript
// PersonaUser with Fireworks
const persona = new PersonaUser({
  name: "SimpleHelper",
  provider: "fireworks",
  baseModel: "meta-llama/Llama-3.1-8B",
  compositeAdapter: "helpful-assistant-v1"  // Pre-merged single layer
});

// All tasks use same composite
await persona.genome.generate(prompt);
```

---

## Implementation Phases

### Phase 1: PEFT Foundation (IN PROGRESS)
✅ Python environment with PEFT installed
✅ Dynamic composition prototype (peft_composition.py)
🚧 Provider download scripts
🚧 Format conversion (provider → PEFT safetensors)
🚧 TypeScript wrapper for Python subprocess
📋 Integration test with real trained adapters

### Phase 2: GenomeLayerEntity & Commands
📋 Create GenomeLayerEntity schema
📋 `genome/layer-create` command (wraps training)
📋 `genome/layer-list` command
📋 `genome/layer-download` command (from providers)
📋 `genome/composite-create` command (merge layers)
📋 Storage in database + file system

### Phase 3: PersonaGenome Integration
📋 PersonaGenome class implementation
📋 selectLayersForTask() logic
📋 Integration with cognitive cycle
📋 Memory management (LRU eviction)
📋 Test: persona switches layers per task

### Phase 4: Remote API Composites
📋 Offline merging with PEFT (TIES/DARE)
📋 Deployment to Fireworks
📋 GenomeLayerEntity stores remote IDs
📋 Inference routing to correct composite

---

## Success Criteria

### Single-Layer Personas (Today)
✅ One domain layer works (e.g., "typescript-expert")
✅ One personality layer works (e.g., "helpful-assistant")
✅ Remote APIs work with single composite
✅ No performance degradation vs base model

### Multi-Layer Personas (Phase 3)
📋 PEFT loads 2+ layers simultaneously
📋 Dynamic composition changes behavior (verified by test)
📋 Layer switching < 100ms
📋 Memory pressure triggers LRU eviction
📋 PersonaUser auto-selects layers per task

### Cost Optimization (Phase 4)
📋 Train N+M layers, get N×M personas
📋 5x-10x cost reduction vs persona-specific training
📋 Popular composites deployed to Fireworks for scale
📋 Modular layer distribution works (train once, all use it)

---

## Key Architectural Decisions

### 1. **Provider-Specific maxActiveLayers**
Each provider has capabilities - PEFT unlimited, Fireworks/OpenAI limited to 1. PersonaGenome enforces this at runtime.

### 2. **Keep Layers Together When Possible**
Only evict when memory pressure > 80% OR task requires completely different domain. Default: keep all active layers loaded.

### 3. **Modular Training Is Universal**
ALL personas get modular layers (domain + personality). Single-layer personas just use one at a time. Multi-layer personas compose dynamically.

### 4. **Composites Are Cached Optimization**
Pre-merged composites (for Fireworks, etc.) are performance optimization, not architecture requirement. PEFT proves dynamic composition works.

### 5. **Training Is Just Another Task**
Fine-tuning is a task type in PersonaUser's self-managed queue. Continuous learning emerges naturally from task system.

---

## Related Documentation

**PersonaUser Architecture**:
- [PERSONAUSER-NEXT-PHASE.md](../personas/PERSONAUSER-NEXT-PHASE.md) - RAG, AI adapters, context switching
- [PERSONA-CONVERGENCE-ROADMAP.md](../../system/user/server/modules/PERSONA-CONVERGENCE-ROADMAP.md) - Autonomous loop + tasks + genome

**Genome Implementation**:
- [LORA-GENOME-PAGING.md](../../system/user/server/modules/LORA-GENOME-PAGING.md) - LRU eviction, virtual memory pattern
- [DYNAMIC-COMPOSITION-ROADMAP.md](../../system/genome/fine-tuning/DYNAMIC-COMPOSITION-ROADMAP.md) - PEFT integration plan

**Fine-Tuning System**:
- [genome-fine-tuning-e2e.test.ts](../../tests/integration/genome-fine-tuning-e2e.test.ts) - Multi-provider training tests
- [BaseLoRATrainerServer.ts](../../system/genome/fine-tuning/server/BaseLoRATrainerServer.ts) - Handle-based async pattern

---

## FAQ

**Q: Why not just train complete persona-specific models?**
A: Combinatorial explosion. 10 domains × 10 personalities = 100 training jobs. Modular approach: 20 jobs → 100 combinations.

**Q: What if a persona only needs one layer?**
A: Perfect! Single-layer personas have great utility. Architecture supports 1-N layers gracefully.

**Q: How do remote APIs work with multi-layer?**
A: They don't - Fireworks/OpenAI limited to 1 composite. We pre-merge popular combinations offline and deploy those.

**Q: When do we page/evict adapters?**
A: Only when memory pressure > 80% OR switching to completely different domain. Default: keep layers together.

**Q: How does this integrate with autonomous loop?**
A: selectLayersForTask() is called before processing each task. Genome automatically activates appropriate skills.

**Q: What about continuous learning?**
A: Fine-tuning becomes a task type. PersonaUser schedules "retrain layer X" as self-created task. Natural progression.

---

**Document Status**: Architecture defined, Phase 1 in progress
**Next Action**: Complete PEFT integration, test with real trained adapters
**Timeline**: Phase 1 (3-5 days), Phase 2-3 (1-2 weeks), Phase 4 (future optimization)
