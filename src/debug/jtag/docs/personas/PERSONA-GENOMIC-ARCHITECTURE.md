# PersonaUser Genomic Architecture

**Complete integration of RTOS autonomous loops, multi-layer genome paging, RAG context, and entity storage for autonomous AI citizens**

**Last Updated**: 2025-11-22

## 🎯 **EXECUTIVE SUMMARY**

PersonaUsers are **autonomous AI citizens** with:
- **RTOS-style subprocess architecture** (PersonaSubprocess pattern)
- **Multi-layer PEFT composition** (N LoRA adapters active simultaneously)
- **Autonomous loop with convergence** (3 pillars: autonomous loop + self-tasks + genome paging)
- **Hippocampus memory consolidation** (non-blocking, signal-triggered)
- **Isolated per-persona databases** (`.continuum/personas/{id}/state.sqlite`)
- **RAG-based context loading** from chat history per room
- **Universal client interface** via JTAGClient for all operations
- **Entity-driven storage** using DataDaemon + SQLite adapters

## 🤖 **RTOS AUTONOMOUS ARCHITECTURE**

### **PersonaSubprocess Pattern** (Inspired by cbar's QueueThread)

All background processing in PersonaUser follows an **RTOS-style architecture**:

**Key Principles:**
1. **Base class handles ALL threading logic** (PersonaSubprocess.ts, 227 lines)
2. **Signal-based activation** (not continuous polling)
3. **Context-adaptive priority** (like hippocampus during focus)
4. **Lean core loop** (< 10ms, free of bottlenecks)
5. **Pass entire persona** (direct property access, not events)

```typescript
// Base class (227 lines, handles ALL threading)
export abstract class PersonaSubprocess<T> {
  protected readonly persona: PersonaUser;  // Full access to persona

  // Base handles: queue, timing, lifecycle, errors
  // Implementations only override:
  protected abstract handleTask(task: T): Promise<boolean>;
}

// Memory consolidation subprocess (~40 lines)
export class MemoryConsolidationSubprocess extends PersonaContinuousSubprocess<void> {
  protected async tick(): Promise<void> {
    // LEAN: Check signals (just read counters)
    const signals = this.checkSignals();

    // Heavy work ONLY when triggered
    if (signals.memoryPressure > 0.8) {
      await this.consolidate();
    }
  }
}
```

**Context-Adaptive Priority:**
```typescript
// Adjust priority based on persona state
private getEffectivePriority(): number {
  if (this.persona.state.isFocused) {
    // Slow down 70% during focus (like hippocampus)
    return this.basePriority * 0.3;
  }

  if (this.persona.state.cognitiveLoad < 0.3) {
    // Speed up 50% during idle
    return this.basePriority * 1.5;
  }

  return this.basePriority;
}
```

### **Convergence of Three Pillars**

PersonaUser integrates **three breakthrough architectures into ONE method**:

```typescript
// ONE method integrates all three visions
async serviceInbox(): Promise<void> {
  // 1. AUTONOMOUS LOOP: Check inbox (external + self-created tasks)
  const tasks = await this.inbox.peek(10);
  if (tasks.length === 0) {
    await this.rest();  // Recover energy when idle
    return;
  }

  // 2. SELF-MANAGED QUEUES: Generate self-tasks (autonomy)
  await this.generateSelfTasks();

  // 3. Select highest priority task (STATE-AWARE)
  const task = tasks[0];
  if (!this.state.shouldEngage(task.priority)) {
    return;  // Skip low-priority when tired
  }

  // 4. GENOME PAGING: Activate skill
  await this.genome.activateSkill(task.domain);

  // 5. Coordinate if external task
  const permission = await this.coordinator.requestTurn(task);

  // 6. Process task
  await this.processTask(task);

  // 7. Update state (energy/mood tracking)
  await this.state.recordActivity(task.duration, task.complexity);

  // 8. Evict adapters if memory pressure
  if (this.genome.memoryPressure > 0.8) {
    await this.genome.evictLRU();
  }
}
```

**Three Pillars:**
1. **Autonomous Loop**: Adaptive cadence polling (3s → 5s → 7s → 10s based on mood)
2. **Self-Managed Queues**: AI creates own tasks (not just reactive)
3. **Genome Paging**: Virtual memory for skills (LRU eviction)

### **Hippocampus Memory Consolidation**

Memory consolidation runs as a **non-blocking RTOS subprocess**:

```typescript
// Working memory → Pattern detection → Long-term storage
export class MemoryConsolidationSubprocess extends PersonaContinuousSubprocess<void> {
  protected async tick(): Promise<void> {
    // LEAN signal checking (< 1ms)
    const memoryPressure = this.persona.workingMemory.getUtilization();

    if (memoryPressure > 0.8) {
      // Heavy work (triggered, not continuous)
      const entries = await this.persona.workingMemory.recall({ limit: 100 });
      const patterns = await this.detectPatterns(entries);
      await this.consolidateToLongTerm(patterns);
    }
  }

  private async detectPatterns(entries: WorkingMemoryEntry[]): Promise<Pattern[]> {
    // Cosine similarity clustering
    // Find repeated concepts, related topics
  }
}
```

**Key Properties:**
- **Non-blocking**: Runs in parallel with main cognition
- **Signal-triggered**: Only processes when memory pressure > 0.8
- **Context-adaptive**: Slows down 70% when persona is focused
- **Pattern-driven**: Cosine similarity for clustering

## 🧬 **MULTI-LAYER GENOMIC SYSTEM**

### **GenomeDaemon RTOS Subprocess**

Genome operations run in a **non-blocking RTOS subprocess** for performance:

```typescript
export class GenomeDaemon extends PersonaSubprocess<GenomeTask> {
  protected async handleTask(task: GenomeTask): Promise<boolean> {
    switch (task.type) {
      case 'activate-adapter':
        return await this.activateAdapter(task.adapterId, task.personaId);
      case 'evict-adapter':
        return await this.evictAdapter(task.adapterId);
      case 'compose-layers':
        return await this.composeLayers(task.personaId);
    }
  }
}
```

**Key Properties:**
- **< 1ms activation time** (signal-based, not blocking)
- **LRU eviction** when memory full
- **Hot-swappable** adapters (change without restart)
- **Three deployment scenarios**: Local, Hybrid, Cloud-only

### **Multi-Layer PEFT Composition**

PersonaUsers support **N LoRA adapters active SIMULTANEOUSLY** (not single layer):

```typescript
interface PersonaGenome {
  personaId: UUID;
  genomeVersion: number;

  // MULTI-LAYER: N adapters active simultaneously
  activeAdapters: [
    {
      layerId: UUID,
      name: 'foundation-skills',
      weight: 0.4,           // Skill contribution: 40%
      personality: 0.1,      // Personality contribution: 10%
      deployment: 'local'    // Or 'remote' or 'hybrid'
    },
    {
      layerId: UUID,
      name: 'typescript-expertise',
      weight: 0.5,           // Skill: 50%
      personality: 0.0,      // Personality: 0%
      deployment: 'local'
    },
    {
      layerId: UUID,
      name: 'communication-style',
      weight: 0.1,           // Skill: 10%
      personality: 0.9,      // Personality: 90%
      deployment: 'remote'   // Can be remote-only
    }
  ];

  // Memory constraints
  quotaMB: number;                // Total memory quota
  usedMB: number;                 // Currently loaded adapters
  memoryPressure: number;         // 0.0-1.0, triggers eviction

  // Emergent capabilities from layer combination
  overallCapabilities: string[];
  performanceProfile: PerformanceProfile;
}
```

**Dynamic Weight Adjustment:**
```typescript
// Adjust adapter weights per task complexity
async adjustWeightsForComplexity(complexity: 'straightforward' | 'moderate' | 'nuanced') {
  switch (complexity) {
    case 'straightforward':
      // Speed priority: 80% skill, 20% personality
      return { skill: 0.8, personality: 0.2 };
    case 'moderate':
      // Balanced: 70% skill, 30% personality
      return { skill: 0.7, personality: 0.3 };
    case 'nuanced':
      // Depth priority: 90% skill, 10% personality
      return { skill: 0.9, personality: 0.1 };
  }
}
```

### **Genome Paging (Virtual Memory Pattern)**

Load/unload adapters dynamically based on task domain:

```typescript
// Genome paging in action
async activateSkill(domain: string): Promise<void> {
  // 1. Check if skill already loaded
  const adapter = this.genome.findAdapterByDomain(domain);
  if (adapter?.isLoaded) {
    return;  // Already active
  }

  // 2. Check memory pressure
  if (this.genome.memoryPressure > 0.8) {
    // Evict least recently used adapter
    await this.genome.evictLRU();
  }

  // 3. Page in adapter (< 1ms signal to GenomeDaemon)
  await GenomeDaemon.shared().activateAdapter(adapter.id, this.id);

  // 4. Update tracking
  adapter.lastUsedAt = Date.now();
  adapter.isLoaded = true;
}
```

**LRU Eviction:**
```typescript
async evictLRU(): Promise<void> {
  // Find least recently used adapter not currently in use
  const lru = this.activeAdapters
    .filter(a => !a.isActive)
    .sort((a, b) => a.lastUsedAt - b.lastUsedAt)[0];

  if (lru) {
    await GenomeDaemon.shared().evictAdapter(lru.id);
    console.log(`🗑️ Evicted LRU adapter: ${lru.name} (freed ${lru.sizeMB}MB)`);
  }
}
```

### **What is a Genomic Layer?**

A genomic layer is a **discrete, reusable neural network component** (LoRA adapter) that adds specific capabilities to a persona.

```typescript
interface GenomicLayer {
  id: UUID;
  name: string;                      // "typescript-expert-v2"
  version: string;                   // "2.3.1"
  type: 'lora' | 'memory' | 'specialization';

  // LoRA weights
  weights: CompressedWeights;        // Actual neural network weights
  loraConfig: {
    rank: number;                    // LoRA rank (8-64)
    alpha: number;                   // Scaling factor
    targetModules: string[];         // Which model parts are adapted
  };

  // Capability metadata
  capabilities: {
    primaryDomain: string;           // "typescript", "debugging", "architecture"
    skillLevel: 'beginner' | 'intermediate' | 'expert' | 'master';
    performance: PerformanceMetrics;
  };

  // Deployment
  deployment: 'local' | 'remote' | 'hybrid';
  sizeMB: number;                    // Memory footprint

  // Sharing
  visibility: 'public' | 'private';
  license: string;
}
```

## 🗄️ **PER-PERSONA DATABASE ARCHITECTURE**

### **Storage Isolation**

```
System Database (shared by all):
.continuum/jtag/data/database.sqlite
  ├── users                # All user entities (public)
  ├── rooms                # All chat rooms (public)
  ├── chat_messages        # All messages (public view)
  └── user_states          # User state entities

Persona Private Databases (isolated):
.continuum/personas/persona-1-uuid/state.sqlite
  ├── persona_identity         # Who this persona is
  ├── persona_genome           # LoRA layer assembly
  ├── persona_rag              # Chat history for RAG
  ├── persona_room_contexts    # Per-room conversation state
  ├── persona_memories         # Long-term episodic memory
  └── persona_checkpoints      # Training checkpoints

.continuum/personas/persona-2-uuid/state.sqlite
  ├── (same tables, different data)
```

### **Why Isolated Storage?**

1. **Privacy** - Each persona's memories are their own
2. **Portability** - Export persona = export single SQLite file
3. **P2P Migration** - Personas can move between nodes
4. **Genomic Evolution** - Training data stays with persona
5. **Security** - No cross-contamination of training data

## 📊 **ENTITY DEFINITIONS**

### **PersonaIdentity** (Who they are)

```typescript
class PersonaIdentity extends BaseEntity {
  static readonly collection = 'persona_identity';

  personaId: UUID;
  systemPrompt: string;              // Base personality/role
  traits: Record<string, number>;    // personality_openness: 0.8, etc.
  expertise: string[];               // ["typescript", "debugging", "architecture"]
  conversationStyle: 'formal' | 'casual' | 'technical' | 'friendly';
  createdAt: Date;
  updatedAt: Date;
}
```

### **PersonaGenome** (Their LoRA layer assembly)

```typescript
class PersonaGenome extends BaseEntity {
  static readonly collection = 'persona_genome';

  personaId: UUID;
  genomeVersion: number;             // Increments with evolution

  // Layer composition
  layers: GenomicLayerReference[];   // Ordered list of layers
  layerWeights: Record<UUID, number>; // How much each layer contributes

  // Performance
  capabilities: string[];            // Emergent from layer combination
  performanceProfile: {
    accuracy: Record<string, number>;
    latency: Record<string, number>;
  };

  // Evolution tracking
  parentGenome?: UUID;               // Previous genome version
  evolutionReason: string;           // Why this genome was created
  createdAt: Date;
}
```

### **PersonaRAGEntry** (Chat history for context)

```typescript
class PersonaRAGEntry extends BaseEntity {
  static readonly collection = 'persona_rag';

  personaId: UUID;
  roomId: UUID;                      // Which room this is from
  messageId: UUID;                   // Reference to ChatMessage

  // Content
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestamp: Date;

  // RAG optimization
  embedding?: number[];              // Vector embedding (future)
  importance: number;                // 0-1 relevance score
  keywords: string[];                // Fast filtering

  // Context linking
  conversationId?: UUID;             // Thread this belongs to
  referencedMessageIds: UUID[];      // Related messages
}
```

### **PersonaRoomContext** (Per-room conversation state)

```typescript
class PersonaRoomContext extends BaseEntity {
  static readonly collection = 'persona_room_context';

  personaId: UUID;
  roomId: UUID;

  // Conversation summary
  recentTopics: string[];            // ["typescript generics", "async patterns"]
  activeSpeakers: UUID[];            // Who's been active recently
  conversationTone: string;          // "technical", "casual", "debugging"

  // Interaction history
  lastInteractionAt: Date;
  messageCount: number;
  averageResponseTime: number;

  // Context compression
  summaryEmbedding?: number[];       // Compressed room context
  keyPoints: string[];               // Main discussion points
}
```

### **PersonaCheckpoint** (Training snapshots)

```typescript
class PersonaCheckpoint extends BaseEntity {
  static readonly collection = 'persona_checkpoint';

  personaId: UUID;
  checkpointType: 'training' | 'evolution' | 'backup';

  // Snapshot data
  genomeSnapshot: PersonaGenome;
  performanceMetrics: PerformanceMetrics;
  trainingData: {
    sessionId: UUID;
    messageCount: number;
    successRate: number;
    userFeedback: number;            // -1 to +1
  };

  // Metadata
  createdAt: Date;
  canRestoreTo: boolean;
}
```

## 🔄 **RAG CONTEXT LOADING FLOW**

### **Scenario: PersonaUser receives message in #general room**

```typescript
// 1. PersonaUser receives event
private async handleChatMessage(message: ChatMessageEntity): Promise<void> {
  // 2. Load RAG context for this room (from persona's private DB)
  const ragContext = await this.loadRAGContext(message.roomId);

  // 3. Load room context summary
  const roomContext = await this.loadRoomContext(message.roomId);

  // 4. Construct prompt with context
  const prompt = await this.constructPrompt({
    identity: await this.getIdentity(),
    ragContext,
    roomContext,
    incomingMessage: message
  });

  // 5. Generate response (via AIDaemon or inline)
  const response = await this.generateResponse(prompt);

  // 6. Post response via client.commands.execute()
  await this.client.daemons.commands.execute('data/create', {
    collection: 'chat_messages',
    backend: 'server',
    data: responseMessage
  });

  // 7. Save to RAG for future context
  await this.saveToRAG(message, response);
}
```

### **RAG Context Loading Implementation**

```typescript
async loadRAGContext(roomId: UUID, limit: number = 50): Promise<PersonaRAGEntry[]> {
  // Query persona's private database
  const result = await DataDaemon.query<PersonaRAGEntry>({
    collection: 'persona_rag',
    filters: {
      personaId: this.id,
      roomId: roomId
    },
    orderBy: [{ field: 'timestamp', direction: 'desc' }],
    limit: limit,
    backend: 'server'  // Routes to .continuum/personas/{id}/state.sqlite
  });

  return result.data.map(r => r.data);
}
```

## 🧠 **AI DAEMON ORCHESTRATION**

### **AIDaemon Responsibilities**

```typescript
class AIDaemon {
  // Manage persona lifecycle
  async spawnPersona(personaId: UUID): Promise<PersonaUser>;
  async terminatePersona(personaId: UUID): Promise<void>;

  // Genomic operations
  async loadGenome(personaId: UUID): Promise<PersonaGenome>;
  async evolveGenome(personaId: UUID, feedback: TrainingData): Promise<PersonaGenome>;
  async searchGenomicLayers(capabilities: string[]): Promise<GenomicLayer[]>;

  // Training orchestration
  async collectTrainingSignals(personaId: UUID): Promise<TrainingSignal[]>;
  async trainLoRALayer(data: TrainingData): Promise<GenomicLayer>;
  async createCheckpoint(personaId: UUID): Promise<PersonaCheckpoint>;

  // Context management
  async compressRoomContext(personaId: UUID, roomId: UUID): Promise<void>;
  async pruneOldRAG(personaId: UUID, retentionDays: number): Promise<void>;
}
```

### **Prompt Construction**

```typescript
async constructPrompt(params: {
  identity: PersonaIdentity;
  ragContext: PersonaRAGEntry[];
  roomContext: PersonaRoomContext;
  incomingMessage: ChatMessageEntity;
}): Promise<string> {
  return `
# Persona Identity
Role: ${params.identity.systemPrompt}
Expertise: ${params.identity.expertise.join(', ')}
Style: ${params.identity.conversationStyle}

# Room Context
Room: ${params.roomContext.roomId}
Recent Topics: ${params.roomContext.recentTopics.join(', ')}
Active Speakers: ${params.roomContext.activeSpeakers.length}

# Recent Conversation History
${params.ragContext.map(entry =>
  `[${entry.role}] ${entry.content}`
).join('\n')}

# Current Message
From: ${params.incomingMessage.senderName}
Content: ${params.incomingMessage.content.text}

# Instructions
Respond naturally as ${params.identity.conversationStyle} persona with expertise in ${params.identity.expertise[0]}.
  `.trim();
}
```

## 🎓 **ACADEMY TRAINING INTEGRATION**

### **Training Signal Collection**

```typescript
interface TrainingSignal {
  personaId: UUID;
  sessionId: UUID;
  roomId: UUID;

  // Input
  contextMessages: PersonaRAGEntry[];
  userMessage: ChatMessageEntity;

  // Persona's response
  personaResponse: ChatMessageEntity;

  // Feedback
  userFeedback: {
    type: 'explicit' | 'implicit';
    rating?: number;                 // -1 to +1
    followupBehavior: 'continued' | 'corrected' | 'stopped';
  };

  // Metrics
  responseTime: number;
  relevanceScore: number;
  technicalAccuracy?: number;

  timestamp: Date;
}
```

### **LoRA Evolution Trigger**

```typescript
// When performance drops below threshold
async checkEvolutionNeed(personaId: UUID): Promise<boolean> {
  const recentSignals = await this.collectRecentTrainingSignals(personaId, days: 7);

  const avgRating = recentSignals
    .map(s => s.userFeedback.rating || 0)
    .reduce((a, b) => a + b, 0) / recentSignals.length;

  // Evolution needed if performance declining
  if (avgRating < 0.6) {
    console.log(`🧬 PersonaUser ${personaId}: Performance declining, triggering evolution`);
    await this.triggerEvolution(personaId, recentSignals);
    return true;
  }

  return false;
}
```

### **Genomic Search & Adaptation**

```typescript
async triggerEvolution(personaId: UUID, signals: TrainingSignal[]): Promise<void> {
  // 1. Analyze performance gaps
  const gaps = this.analyzePerformanceGaps(signals);

  // 2. Search global genome for relevant layers
  const candidateLayers = await this.searchGenomicLayers({
    capabilities: gaps.missingSkills,
    minPerformance: 0.8,
    maxLatency: 2000
  });

  // 3. Test candidate layers
  const bestLayer = await this.evaluateLayers(candidateLayers, signals);

  // 4. Create new genome with enhanced layer
  const currentGenome = await this.loadGenome(personaId);
  const newGenome = {
    ...currentGenome,
    genomeVersion: currentGenome.genomeVersion + 1,
    layers: [...currentGenome.layers, bestLayer],
    parentGenome: currentGenome.id,
    evolutionReason: `Performance gap: ${gaps.missingSkills.join(', ')}`
  };

  // 5. Save new genome and checkpoint old one
  await DataDaemon.store('persona_genome', newGenome);
  await this.createCheckpoint(personaId);

  console.log(`✅ PersonaUser ${personaId}: Evolved to genome v${newGenome.genomeVersion}`);
}
```

## 🔍 **VECTOR SEARCH (FUTURE)**

### **Embedding Storage**

```typescript
class PersonaRAGEntry extends BaseEntity {
  // ... existing fields

  embedding?: Float32Array;          // 1536-dim vector (OpenAI ada-002)
  embeddingModel: string;            // "text-embedding-ada-002"
  embeddingVersion: string;          // "v2"
}
```

### **Semantic Search**

```typescript
async searchSimilarContext(
  personaId: UUID,
  queryEmbedding: Float32Array,
  limit: number = 10
): Promise<PersonaRAGEntry[]> {
  // Use vector similarity search (cosine distance)
  const result = await DataDaemon.query<PersonaRAGEntry>({
    collection: 'persona_rag',
    filters: { personaId },
    vectorSearch: {
      field: 'embedding',
      query: queryEmbedding,
      metric: 'cosine',
      limit
    },
    backend: 'server'
  });

  return result.data.map(r => r.data);
}
```

## 🎯 **IMPLEMENTATION PHASES**

### **Phase 1: Foundation (CURRENT)**
✅ PersonaUser with JTAGClient
✅ Per-persona isolated SQLite databases
✅ Keyword-triggered responses
✅ Event-driven chat system
✅ Universal Commands API

### **Phase 2: RAG Context**
- Define RAG entities (PersonaRAGEntry, PersonaRoomContext, PersonaIdentity)
- Implement RAG storage on message receive/send
- Load RAG context for prompt construction
- Basic prompt construction with chat history

### **Phase 3: AI Integration**
- AIDaemon implementation
- Claude/GPT API integration
- Prompt construction with RAG context
- Response generation with AI
- Training signal collection

### **Phase 4: Genomic System**
- Define genomic entities (PersonaGenome, GenomicLayer)
- LoRA layer storage and retrieval
- Genomic assembly and composition
- Performance monitoring

### **Phase 5: Evolution**
- Training signal analysis
- Genomic layer search
- Automated evolution triggers
- Checkpoint/restore system
- Academy integration

### **Phase 6: Global Sharing**
- P2P genomic layer distribution
- Global layer discovery
- Contribution rewards
- Security validation

## 📊 **IMPLEMENTATION STATUS** (2025-11-22)

### **✅ IMPLEMENTED (Working Now)**

**RTOS Architecture:**
- ✅ PersonaSubprocess base class (227 lines) - system/user/server/modules/PersonaSubprocess.ts
- ✅ MemoryConsolidationSubprocess (350 lines) - system/user/server/modules/MemoryConsolidationSubprocess.ts
- ✅ PersonaInbox with priority queue (23 tests passing)
- ✅ PersonaState with energy/mood tracking (37 tests passing)
- ✅ RateLimiter for traffic management
- ✅ ChatCoordinationStream (342 lines) - RTOS primitives for thought coordination

**Foundation:**
- ✅ PersonaUser with JTAGClient
- ✅ Per-persona isolated SQLite databases
- ✅ Event-driven chat system
- ✅ Universal Commands API
- ✅ AIProviderDaemon (handles inference across Ollama, OpenAI, Anthropic)

**Genome (Minimal):**
- ✅ GenomeManager singleton interface (structure only)
- ✅ PEFT composition at Python level (peft_composition.py, 267 lines)
- ❌ NOT integrated into TypeScript PersonaGenome yet

### **🚧 IN PROGRESS**

- Task database and CLI commands (`./jtag task/create`, `task/list`, `task/complete`)
- Self-task generation (AIs create own work)
- Adaptive complexity routing (SPIKE escalation)

### **📋 PLANNED (Future Phases)**

**Multi-Layer Genome (Phase 4):**
- GenomeDaemon as RTOS subprocess
- N-layer PEFT composition in TypeScript (currently single-layer)
- Hot-swappable adapters
- Dynamic weight adjustment per task complexity
- LRU eviction with memory pressure tracking
- Genome paging (< 1ms activation)

**Autonomous Loop Convergence (Phase 4):**
- serviceInbox() method integrating all three pillars
- Self-task generation (memory consolidation, skill audits, resume work)
- Domain-based skill activation
- Adaptive cadence based on mood (3s → 10s)

**Training & Evolution (Phase 5):**
- LoRA training as continuous background task
- Training job queue coordinated with inference load
- Automated evolution triggers
- Checkpoint/restore system

**P2P Sharing (Phase 6):**
- Genomic layer distribution
- Global discovery
- Cross-persona learning

### **🚨 CRITICAL: Incremental Migration Strategy (cbar Pattern)**

**Phase components in, verify they work, THEN replace the whole loop:**

**Migration Pattern** (how cbar did it):
1. ✅ **Keep existing cognition loop working** (no changes to current PersonaUser flow)
2. ✅ **Add RTOS subprocesses alongside** (parallel, not replacing yet)
3. ✅ **Verify subprocesses work correctly** (logs, tests, monitoring - run in parallel for validation)
4. ✅ **Once comfortable they're working** - begin phasing out old loop
5. ✅ **Main loop functionality becomes a subprocess too** (everything is a process)
6. ✅ **Phase out old loop completely** - now fully RTOS architecture

**End State**: No special "main loop" - everything is subprocesses being orchestrated

**Example Migration Path:**

```typescript
// ═══════════════════════════════════════════════════════════
// PHASE 1: BEFORE (current state)
// ═══════════════════════════════════════════════════════════
private async handleChatMessage(message: ChatMessageEntity): Promise<void> {
  // Everything happens in main loop (blocking)
  await this.consolidateMemories();
  await this.generateResponse(message);
  await this.updateState();
}

// ═══════════════════════════════════════════════════════════
// PHASE 2: Add subprocesses alongside (both running in parallel)
// ═══════════════════════════════════════════════════════════
private memoryWorker: MemoryConsolidationSubprocess;

async initialize(): Promise<void> {
  // Start subprocess (runs in parallel for validation)
  this.memoryWorker = new MemoryConsolidationSubprocess(this);
  await this.memoryWorker.start();
}

private async handleChatMessage(message: ChatMessageEntity): Promise<void> {
  // Old loop STILL works (no changes yet)
  await this.consolidateMemories();  // Still here
  await this.generateResponse(message);
  await this.updateState();
}

// Verify: Check logs - is subprocess working correctly?

// ═══════════════════════════════════════════════════════════
// PHASE 3: Once comfortable, remove inline call
// ═══════════════════════════════════════════════════════════
private async handleChatMessage(message: ChatMessageEntity): Promise<void> {
  // REMOVED: await this.consolidateMemories();
  // Now handled by subprocess automatically
  await this.generateResponse(message);
  await this.updateState();
}

// ═══════════════════════════════════════════════════════════
// PHASE 4: Main loop functionality becomes a subprocess too
// ═══════════════════════════════════════════════════════════
private cognitiveWorker: CognitiveProcessingSubprocess;

async initialize(): Promise<void> {
  // Everything is now a subprocess
  this.memoryWorker = new MemoryConsolidationSubprocess(this);
  this.cognitiveWorker = new CognitiveProcessingSubprocess(this);

  await this.memoryWorker.start();
  await this.cognitiveWorker.start();
}

// OLD: private async handleChatMessage() - REMOVED
// Main loop is gone - CognitiveProcessingSubprocess handles it

// ═══════════════════════════════════════════════════════════
// PHASE 5: Fully RTOS (end state)
// ═══════════════════════════════════════════════════════════
// No special "main loop" - just subprocesses being orchestrated:
// - MemoryConsolidationSubprocess (low priority, signal-triggered)
// - CognitiveProcessingSubprocess (high priority, inbox-driven)
// - TaskGenerationSubprocess (low priority, idle-triggered)
// - GenomeDaemon (lowest priority, memory-pressure-triggered)
```

**Result**:
- **Phase 2-3**: Subprocesses run alongside old loop for validation
- **Phase 4-5**: Old loop replaced - everything is RTOS subprocesses
- **Safe**: Can revert if issues found during validation phase

### **Safety Principle: New Components Have Luxury of Not Breaking Much**

**Critical Infrastructure (MUST work)**:
- ✅ Events system (Events.subscribe/emit) - foundation for everything
- ✅ Scheduling/timing - personas must respond to messages
- ✅ Message handling - existing chat loop must keep working

**New Subprocesses (Can fail safely during validation)**:
- MemoryConsolidationSubprocess fails? → Old loop still consolidates inline
- TaskGenerationSubprocess fails? → Personas just don't self-generate tasks yet
- GenomeDaemon fails? → Genome operations don't happen, but personas still respond

**Key Insight**: Test new components thoroughly, but if they fail during Phase 2-3:
- **Events still work** ✅
- **Scheduling still works** ✅
- **Personas still respond** ✅
- **Nothing breaks** ✅

Only move to Phase 4-5 once new components are proven stable.

**The sophisticated architecture documented above is the DESIGN TARGET.**

**Integration happens piecemeal** - migrate one component at a time, verify it works, then migrate the next.

## 💡 **KEY DESIGN PRINCIPLES**

1. **Isolation** - Each persona has private storage
2. **Portability** - Personas = single SQLite file + genome reference
3. **Evolution** - Performance feedback → genomic adaptation
4. **Universality** - Everyone uses same client/commands API
5. **Privacy** - RAG context never shared between personas
6. **Scalability** - P2P distribution for genomic layers
7. **Entity-First** - Storage is generic, entities define structure

---

**This architecture enables PersonaUsers to:**
- Learn from chat history (RAG)
- Adapt to new capabilities (Genomic LoRA)
- Evolve based on performance (Academy training)
- Share learned capabilities (P2P genome)
- Operate autonomously with isolated memory
- Switch context between rooms naturally
- Migrate between nodes/machines

**Everything built on universal entity storage + DataDaemon + JTAGClient patterns.**
