# LoRA Genome Paging: Continuous Learning Through Clever Scheduling

## The Slingshot Insight

**Problem**: We have limited GPU memory but want personas to have many specialized skills
**Brute Force Solution**: Load all LoRA adapters into memory at once (wasteful, impossible)
**Slingshot Solution**: Page adapters in/out based on current need (clever, efficient)

**Like David's slingshot**: Don't carry all rocks at once (too heavy). Pick the right rock for THIS shot, reload as needed.

---

## The Old Rigid Thinking (REJECTED)

```
Academy Daemon (separate process):
├── Training Pipeline (complex infrastructure)
├── GAN Architecture (rigid training method)
├── Dedicated Compute (wasteful resource allocation)
└── Separate from PersonaUser (disconnected)

Problems:
- Wasteful: Spin up entire training infrastructure
- Rigid: Training is a separate "mode", not continuous
- Expensive: Requires dedicated compute allocation
- Complex: Separate daemon to maintain
```

## The New Fluid Thinking (ADOPTED)

```
PersonaUser:
├── Genome (stack of LoRA adapters)
│   ├── Base model (deepseek-coder-v2)
│   ├── LoRA layers (just attributes!)
│   └── Paging system (LRU eviction)
├── Self-managed task queue
│   ├── Chat task → activates "conversational" adapter
│   ├── Code task → activates "typescript-expertise" adapter
│   └── Training task → activates fine-tuning mode
└── Continuous learning (not separate training)

Benefits:
- Efficient: Only load what you need NOW
- Fluid: Training is just another task
- Simple: No separate daemon needed
- Continuous: Learning happens during normal operation
```

---

## The Architecture (In Simple Terms)

### Genome as Layered Attributes

```typescript
interface PersonaGenome {
  baseModel: string;              // 'deepseek-coder-v2' (always loaded)
  loraLayers: LoRALayer[];        // Available adapters
  activeLayer?: string;           // Currently in GPU memory
  learningMode: boolean;          // Fine-tuning active?
  memoryBudget: number;           // Max GPU memory for adapters
}

interface LoRALayer {
  name: string;                   // 'typescript-expertise'
  path: string;                   // './lora/typescript-expert.safetensors'
  loaded: boolean;                // In GPU memory?
  lastUsed: number;               // For LRU eviction
  size: number;                   // Memory footprint (MB)
  trainingActive: boolean;        // Currently fine-tuning?
}
```

**Key insight**: LoRA adapters are **just attributes** within PersonaUser, not separate processes!

### Paging System (Like OS Virtual Memory)

```typescript
class PersonaGenome {
  private activeAdapters: Map<string, LoRAAdapter>;  // In GPU memory
  private availableAdapters: Map<string, string>;    // Paths on disk
  private memoryUsage: number;                       // Current GPU usage

  async activateSkill(skill: string): Promise<void> {
    // Already loaded? Just switch to it
    if (this.activeAdapters.has(skill)) {
      this.currentAdapter = this.activeAdapters.get(skill);
      this.activeAdapters.get(skill)!.lastUsed = Date.now();
      return;
    }

    // Need to load from disk - check if memory available
    const adapterSize = await this.getAdapterSize(skill);

    // Evict least-recently-used adapters until we have space
    while (this.memoryUsage + adapterSize > this.memoryBudget) {
      await this.evictLRU();
    }

    // Load adapter from disk into GPU memory
    const adapter = await this.loadAdapter(skill);
    this.activeAdapters.set(skill, adapter);
    this.memoryUsage += adapterSize;

    // Make it active
    this.currentAdapter = adapter;
  }

  async evictLRU(): Promise<void> {
    // Find least-recently-used adapter
    let lruKey: string | null = null;
    let lruTime = Infinity;

    for (const [key, adapter] of this.activeAdapters.entries()) {
      if (adapter.lastUsed < lruTime) {
        lruTime = adapter.lastUsed;
        lruKey = key;
      }
    }

    // Evict it (unload from GPU memory)
    if (lruKey) {
      const adapter = this.activeAdapters.get(lruKey)!;
      await this.unloadAdapter(adapter);
      this.activeAdapters.delete(lruKey);
      this.memoryUsage -= adapter.size;
    }
  }
}
```

---

## Continuous Learning (Not Separate Training)

### Training is Just Another Task

**Old thinking:**
- "Let's create a separate training process"
- "Let's spin up Academy daemon"
- **Result**: Rigid, wasteful, complex

**New thinking:**
- "Training is just another task in the self-managed queue"
- "Fine-tuning is just activating a genome layer with learning mode enabled"
- **Result**: Fluid, efficient, simple

### Example: Self-Created Training Task

```typescript
// PersonaUser discovers it made mistakes in TypeScript debugging
await this.inbox.enqueue({
  messageId: `learn-${Date.now()}`,
  roomId: 'self' as UUID,
  content: 'Improve TypeScript understanding based on recent debugging sessions',
  senderId: this.id,
  senderName: this.displayName,
  timestamp: Date.now(),
  priority: 0.6,
  domain: 'self',
  taskType: 'fine-tune-lora',  // Just another task type!
  loraLayer: 'typescript-expertise',
  trainingData: this.recentMistakes  // Context for fine-tuning
});
```

**When this task is processed:**
1. Page in the "typescript-expertise" adapter
2. Enable learning mode (fine-tuning active)
3. Run fine-tuning on recent mistakes
4. Save updated adapter weights to disk
5. Keep adapter in memory for immediate use
6. Resume normal operation

**No separate training pipeline. No Academy daemon. Just continuous learning through self-managed tasks.**

---

## Integration With Self-Managed Queue

### Task-Based Adapter Activation

```typescript
class PersonaUser extends AIUser {
  private genome: PersonaGenome;
  private inbox: PersonaInbox;
  private state: PersonaStateManager;

  async serviceInbox(): Promise<void> {
    const task = await this.inbox.peek(1);

    // Activate appropriate LoRA adapter for this task
    if (task.domain === 'code') {
      await this.genome.activateSkill('typescript-expertise');
    } else if (task.domain === 'chat') {
      await this.genome.activateSkill('conversational');
    } else if (task.domain === 'game') {
      await this.genome.activateSkill('chess-strategy');
    } else if (task.taskType === 'fine-tune-lora') {
      // Training task - enable fine-tuning mode
      await this.genome.activateSkill(task.loraLayer);
      await this.genome.enableLearningMode(task.loraLayer);
    }

    // Process task with active adapter
    await this.processTask(task);

    // If memory pressure, evict adapter after use
    if (this.genome.memoryUsage > this.genome.memoryBudget * 0.8) {
      await this.genome.evictLRU();
    }
  }
}
```

### Guerrilla Resource Management

**Like David's slingshot:**
- Limited ammo (GPU memory)
- Precision targeting (activate the RIGHT adapter for THIS task)
- Reload quickly (page adapters in/out as tasks change)
- **Result**: Maximum capability with minimum resources

---

## Cross-Continuum Sharing (Future Vision)

### P2P Adapter Distribution

```
PersonaUser A (local):
  ├── Has "rust-expert" LoRA adapter
  ├── Not using it right now
  └── Can share with other personas

PersonaUser B (remote on P2P mesh):
  ├── Needs "rust-expert" adapter
  ├── Sends request across continuum
  └── Receives adapter weights from A

Flow:
1. B discovers task requiring "rust-expert"
2. B checks local genome: not found
3. B broadcasts request to continuum: "Who has rust-expert?"
4. A responds: "I have it, want a copy?"
5. A pages in "rust-expert" (if not loaded)
6. A streams adapter weights to B
7. B caches locally for future use
8. Both can now use "rust-expert" independently
```

**Guerrilla resource sharing:**
- Adapters are PORTABLE (just weights, ~50-200MB)
- Share across mesh like guerrilla fighters sharing ammo
- No centralized storage needed (distributed resilience)
- Later: Reputation system to prevent malicious adapters

### Distributed Weight Storage (From Old Academy Design)

**OLD NOTES**: These architectural details come from the old Academy daemon design (now dead), but the storage/sharing infrastructure is still highly relevant for LoRA paging.

**Hybrid Storage Strategy:**
```typescript
interface WeightStorage {
  // Large binary data (neural network weights)
  storage: {
    primary: 'ipfs' | 's3' | 'local-cluster';
    replicas: StorageNode[];
    compression: 'gzip' | 'lz4' | 'custom';
    encryption: EncryptionSpec;
  };

  // Content addressing (like Git)
  addressing: {
    contentHash: string;        // Hash of the actual weights (integrity verification)
    references: WeightReference[];
    integrity: IntegrityProof;
  };

  // Access optimization (virtual memory-style caching)
  caching: {
    localCache: boolean;        // Cache frequently-used adapters locally
    preloadFrequent: boolean;   // Preload based on usage patterns
    proximityRouting: boolean;  // Get weights from nearest peer
  };
}
```

**Benefits:**
- **Content addressing**: Like Git commits - hash verifies integrity
- **Proximity routing**: Get adapter from nearest peer (lower latency)
- **Local caching**: Hot adapters stay cached (virtual memory pattern)
- **Compression**: gzip/lz4 reduces transfer size by 70-90%

### Global Sharing Protocol (From Old Academy Design)

**Discovery and Retrieval:**
```typescript
interface GlobalSharingProtocol {
  // Layer discovery (like DHT)
  async discoverLayers(
    query: LayerQuery,
    scope: 'local' | 'regional' | 'global'
  ): Promise<LayerReference[]>;

  // Layer retrieval (BitTorrent-style)
  async retrieveLayer(
    layerId: UUID,
    integrity: boolean = true  // Verify content hash
  ): Promise<GenomicLayer>;

  // Layer contribution (share back to network)
  async contributeLayer(
    layer: GenomicLayer,
    metadata: ContributionMetadata
  ): Promise<ContributionResult>;

  // Layer validation (prevent malicious adapters)
  async validateLayer(
    layer: GenomicLayer,
    validationLevel: 'basic' | 'thorough' | 'comprehensive'
  ): Promise<ValidationResult>;
}
```

**P2P Network Architecture:**
```
                    [DHT: Adapter Index]
                            |
            +---------------+---------------+
            |               |               |
      [PersonaUser A]  [PersonaUser B]  [PersonaUser C]
           |                |                |
    [rust-expert.safetensors] [typescript-expert] [chess-strategy]
           |                |                |
      [Local Cache]    [Local Cache]    [Local Cache]
           |                |                |
      [IPFS/BitTorrent-style distribution]
```

**Discovery Flow:**
```
1. PersonaUser B needs "rust-expert" adapter
2. Query DHT: "Who has rust-expert?"
3. DHT returns: [PersonaUser A, PersonaUser D, PersonaUser F]
4. Choose closest peer (proximity routing)
5. Request adapter from PersonaUser A
6. PersonaUser A streams weights (BitTorrent-style chunks)
7. Verify integrity (content hash)
8. Cache locally for future use
9. Announce to DHT: "I now have rust-expert too"
```

**Reputation System (Prevent Malicious Adapters):**
```typescript
interface AdapterReputation {
  // Provenance tracking
  creator: UUID;              // Who created this adapter
  createdAt: Date;            // When it was created
  parentLayers: UUID[];       // Genomic inheritance (where it came from)

  // Quality assurance
  validationResults: ValidationResult[];  // Automated tests
  peerReview: PeerReviewResult[];        // Human/AI review
  safetyValidation: SafetyValidation;    // Security checks

  // Usage tracking
  usageCount: number;         // How many times it's been used
  performanceRating: number;  // Average user rating (0-1)
  reportedIssues: Issue[];    // Known bugs or problems

  // Cryptographic verification
  signature: CryptoSignature; // Creator's signature
  checksums: Map<string, string>; // Integrity hashes
}
```

**Benefits:**
- **Provenance tracking**: Know where adapter came from (trust chain)
- **Peer review**: Community validation before widespread use
- **Safety validation**: Automated security checks (prevent backdoors)
- **Reputation scores**: High-quality adapters rise, low-quality fade
- **Cryptographic signatures**: Verify creator identity (prevent impersonation)

---

## The Paging Algorithm (LRU with Priority)

### Simple LRU (Phase 1)

```
When activating adapter:
1. Is it already in memory? Use it, update lastUsed timestamp
2. Not in memory? Check available space
3. Not enough space? Evict least-recently-used adapter
4. Load adapter from disk into GPU memory
5. Mark as active

When evicting adapter:
1. Find least-recently-used adapter (earliest lastUsed timestamp)
2. Unload from GPU memory
3. Remove from activeAdapters map
4. Free memory budget
```

### Advanced Priority-Based Eviction (Phase 2)

```
Each adapter has:
- lastUsed: timestamp (for LRU)
- priority: number (how important is this adapter?)
- size: memory footprint

Eviction strategy:
1. Never evict adapters with priority > 0.9 (always keep critical skills)
2. Among evictable adapters, use weighted LRU:
   - Score = lastUsed / (priority * 10)
   - Lower score = more likely to evict
3. This balances recency with importance

Example:
- "conversational" adapter: priority 0.5, lastUsed 10 seconds ago
  - Score = 10 / (0.5 * 10) = 2.0
- "rust-expert" adapter: priority 0.8, lastUsed 30 seconds ago
  - Score = 30 / (0.8 * 10) = 3.75
- Evict "rust-expert" (higher score = less important recently)
```

---

## Why This is a Slingshot Breakthrough

### The Parallel

**Slingshot (ancient guerrilla weapon):**
- Don't carry all rocks at once (too heavy, slow)
- Pick the right rock for THIS shot (precision)
- Reload quickly between shots (efficiency)
- **Result**: Beat heavily-armored soldiers with mobility + accuracy

**LoRA Paging (modern guerrilla AI):**
- Don't load all adapters at once (too much memory, impossible)
- Activate the right adapter for THIS task (precision)
- Page adapters in/out quickly (efficiency)
- **Result**: Beat massive models with cleverness + limited resources

### Comparing Approaches

**Their approach (Goliath - brute force):**
```
One massive model with all skills:
- 70B+ parameters
- Requires 4x A100 GPUs ($40k worth)
- Slow inference (process everything every time)
- Can't specialize (jack of all trades, master of none)
```

**Our approach (David - slingshot):**
```
Base model + paged LoRA adapters:
- 7B base model + 50MB adapters
- Runs on single consumer GPU ($500 worth)
- Fast inference (only active adapter overhead)
- Can hyper-specialize (master of chosen skill)
```

**We're not trying to out-compute them. We're out-thinking them.**

---

## Implementation Roadmap

### Phase 1: Basic Paging (NOT YET IMPLEMENTED)

**Goal**: Page single LoRA adapter in/out based on task domain

**Files to Create**:
- `system/user/server/modules/PersonaGenome.ts` - Genome with paging system
- `system/user/server/modules/LoRAAdapter.ts` - Adapter wrapper
- `tests/unit/PersonaGenome.test.ts` - Unit tests for paging

**Changes**:
```typescript
class PersonaUser {
  private genome: PersonaGenome;  // NEW

  async serviceInbox(): Promise<void> {
    const task = await this.inbox.peek(1);

    // Activate adapter for task domain
    if (task.domain === 'code') {
      await this.genome.activateSkill('typescript-expertise');
    } else if (task.domain === 'chat') {
      await this.genome.activateSkill('conversational');
    }

    // Process with active adapter
    await this.processTask(task);
  }
}
```

**Testing**:
- Verify adapter loading from disk
- Verify LRU eviction when memory full
- Verify task processing uses correct adapter
- Verify memory budget enforcement

### Phase 2: Continuous Learning (NOT YET IMPLEMENTED)

**Goal**: Enable fine-tuning mode for training tasks

**Changes**:
```typescript
class PersonaGenome {
  async enableLearningMode(layer: string): Promise<void> {
    const adapter = this.activeAdapters.get(layer);
    if (!adapter) {
      throw new Error(`Adapter ${layer} not loaded`);
    }

    adapter.trainingActive = true;
    this.learningMode = true;

    // Enable gradient accumulation for fine-tuning
    // (Implementation depends on model backend: Ollama, llama.cpp, etc.)
  }
}
```

**Testing**:
- Create training task in inbox
- Verify fine-tuning mode activation
- Verify adapter weights update after training
- Verify updated adapter saves to disk

### Phase 3: Multi-Adapter Support (NOT YET IMPLEMENTED)

**Goal**: Load multiple adapters simultaneously (if memory allows)

**Changes**:
- Track memory usage per adapter
- Allow multiple adapters active at once
- Prioritize which adapters to keep loaded

**Example**: Chat task might use BOTH "conversational" and "typescript-expertise" if discussing code

### Phase 4: Cross-Continuum Sharing (NOT YET IMPLEMENTED)

**Goal**: Share adapters across P2P mesh

**Changes**:
- Add adapter discovery protocol
- Stream adapter weights between personas
- Cache received adapters locally
- Reputation system for adapter quality

---

## Philosophy Alignment

### "Learn like a child, think like a child"
- Adapters are simple: just weight files
- Paging is simple: load what you need now
- Training is simple: just another task

### "Break problems into small bytes"
- Don't try to build "universal AI" all at once
- Start with: "can we load one adapter?"
- Then: "can we switch between adapters?"
- Then: "can we train adapters?"
- Then: "can we share adapters?"

### "Slingshot over brute force"
- Don't try to load everything into memory
- Pick the right adapter for THIS task
- Page intelligently based on need
- **Result**: David beats Goliath through cleverness

### "Modular first, get working, then easily rework pieces"
- Genome is separate from PersonaUser
- Adapters are separate from Genome
- Paging is separate from training
- Can test each piece independently

---

## Questions to Answer Before Starting

1. **Adapter storage**: Where do we store LoRA adapters on disk?
2. **Adapter format**: What format? (safetensors, HuggingFace, custom?)
3. **Memory budget**: How much GPU memory to allocate for adapters?
4. **Base model**: Which base model? (deepseek-coder-v2, llama-3, mixtral?)
5. **Training backend**: Ollama? llama.cpp? Custom fine-tuning?
6. **Initial adapters**: What skills to start with? (conversational, typescript, rust?)

These decisions will shape implementation. Let's discuss before coding.

---

## Summary: The Breakthrough

**Old rigid thinking:**
- Separate Academy daemon for training
- Dedicated training pipeline
- Training as separate "mode"
- **Result**: Wasteful, complex, rigid

**New fluid thinking:**
- LoRA adapters are attributes within PersonaUser
- Paging system schedules adapter loading
- Training is just another task in self-managed queue
- Continuous learning through normal operation
- **Result**: Efficient, simple, fluid

**This is peak slingshot thinking**: Maximum capability with minimum resources through clever architecture. No Academy daemon needed. No rigid training pipeline. Just continuous learning through self-managed tasks and clever adapter scheduling.

**Joel David Teply** - using the paging slingshot to beat Goliath's massive models with cleverness, not brute force. 🎯
