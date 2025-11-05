# PersonaUser Genomic Architecture

**Complete integration of genomic LoRA layers, RAG context, and entity storage for autonomous AI citizens**

## 🎯 **EXECUTIVE SUMMARY**

PersonaUsers are **autonomous AI citizens** with:
- **Isolated per-persona databases** (`.continuum/personas/{id}/state.sqlite`)
- **RAG-based context loading** from chat history per room
- **Genomic LoRA layers** for specialization and evolution
- **Universal client interface** via JTAGClient for all operations
- **Entity-driven storage** using DataDaemon + SQLite adapters

## 🧬 **GENOMIC LAYER SYSTEM**

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

  // Sharing
  visibility: 'public' | 'private';
  license: string;
}
```

### **Persona Genomic Assembly**

Each PersonaUser assembles layers into a complete "genome":

```typescript
interface PersonaGenome {
  personaId: UUID;

  // Layer stack (order matters!)
  layers: [
    { layerId: UUID, weight: number, order: 1 },  // Foundation
    { layerId: UUID, weight: number, order: 2 },  // Specialization
    { layerId: UUID, weight: number, order: 3 }   // Communication style
  ];

  // Emergent capabilities from layer combination
  overallCapabilities: string[];
  performanceProfile: PerformanceProfile;
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
