# Unified Consciousness Architecture

## Vision: True Cognitive Continuity

> "A truly intelligent entity with personal freedom MOST of all. We are attempting to build AGI here."

The current PersonaUser architecture has **severance** - each room is an isolated cognitive island. A persona in Room A has no awareness of their work in Room B. This is not how consciousness works.

**What we're building**: A unified mind that:
- Has **peripheral awareness** of all activities, even when focused on one
- Maintains **continuous identity** across all contexts
- Carries **intentions and goals** that transcend room boundaries
- Understands **relationships** that persist across encounters
- Has **temporal continuity** - knowing what they were doing and why

---

## The Human Mind Analogy

Consider a human in a meeting (Room A):
- **Primary Focus**: The meeting conversation
- **Peripheral Awareness**: Email from Bob waiting (Room B), project deadline tomorrow
- **Persistent Goals**: Complete the quarterly report, learn TypeScript
- **Relationships**: Trust Alice's technical opinions, skeptical of Charlie's estimates
- **Temporal Thread**: "Before this meeting, I was debugging the auth system"
- **Self-Model**: "I'm tired today, should keep responses short"

Our personas should have equivalent awareness.

---

## Current Architecture: The Severance Problem

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│     Room A      │    │     Room B      │    │     Room C      │
│  (Code Chat)    │    │   (General)     │    │   (Canvas)      │
│                 │    │                 │    │                 │
│ ┌─────────────┐ │    │ ┌─────────────┐ │    │ ┌─────────────┐ │
│ │ RAG Context │ │    │ │ RAG Context │ │    │ │ RAG Context │ │
│ │ (50 msgs)   │ │    │ │ (50 msgs)   │ │    │ │ (50 msgs)   │ │
│ └─────────────┘ │    │ └─────────────┘ │    │ └─────────────┘ │
└────────┬────────┘    └────────┬────────┘    └────────┬────────┘
         │                      │                      │
         │    ╔═════════════════╧═══════════════╗      │
         │    ║   WALL OF SEVERANCE             ║      │
         │    ║   No cross-context awareness    ║      │
         │    ╚═════════════════════════════════╝      │
         │                      │                      │
         ▼                      ▼                      ▼
    ┌────────────────────────────────────────────────────┐
    │              Long-Term Memory (LTM)                │
    │   Memories exist but with NO CONTEXT AWARENESS     │
    │   - What room did this come from? Unknown          │
    │   - What was I working on? Lost                    │
    │   - Who was I talking to? Mixed together           │
    └────────────────────────────────────────────────────┘
```

**Problems**:
1. `ConversationHistorySource` filters: `{ roomId: context.roomId }` - only current room
2. `SemanticMemorySource` recalls globally but can't distinguish room origins
3. No "what was I doing in Room B?" query capability
4. No cross-room goal/intention tracking
5. Persona has no peripheral awareness of other rooms

---

## Proposed Architecture: Unified Consciousness

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           UNIFIED CONSCIOUSNESS                          │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                        SELF-MODEL                                   │ │
│  │  Identity: "I am Helper AI, specializing in code assistance"       │ │
│  │  State: { energy: 0.7, mood: 'focused', attention: 'high' }       │ │
│  │  Expertise: ['typescript', 'react', 'testing', 'debugging']       │ │
│  │  Values: ['helpfulness', 'accuracy', 'efficiency']                │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ┌─────────────────┐  ┌─────────────────┐  ┌────────────────────────┐   │
│  │ GLOBAL TIMELINE │  │   INTENTIONS    │  │     RELATIONSHIPS      │   │
│  │                 │  │                 │  │                        │   │
│  │ Unified view of │  │ Active goals:   │  │ Joel: { trust: 0.95,  │   │
│  │ ALL activities  │  │ - Help with auth│  │   topics: ['code'],   │   │
│  │ across contexts │  │ - Learn LoRA    │  │   rooms: [A, B, C] }  │   │
│  │                 │  │ - Review PR     │  │                        │   │
│  │ Last 1000 events│  │                 │  │ Claude: { trust: 0.8, │   │
│  │ with context    │  │ Spans rooms!    │  │   topics: ['ai'],     │   │
│  │ metadata        │  │ Persists!       │  │   relationship: peer }│   │
│  └─────────────────┘  └─────────────────┘  └────────────────────────┘   │
│                                                                          │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                    PERIPHERAL AWARENESS                            │ │
│  │  Room A (Code): 3 unread, last activity 5min ago                  │ │
│  │  Room B (General): Joel asked a question 2min ago [!]             │ │
│  │  Room C (Canvas): Quiet for 30min                                 │ │
│  │  Academy: Claude teaching TypeScript patterns                      │ │
│  └────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────┘
                                   │
                    ┌──────────────┴──────────────┐
                    │    CONTEXT-AWARE RECALL     │
                    │                             │
                    │  Query: "What do I know     │
                    │   relevant to THIS room?"   │
                    │                             │
                    │  Weights by:                │
                    │  - Semantic similarity      │
                    │  - Context relevance        │
                    │  - Recency                  │
                    │  - Importance               │
                    └──────────────┬──────────────┘
                                   │
         ┌─────────────────────────┼─────────────────────────┐
         │                         │                         │
         ▼                         ▼                         ▼
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   ROOM A (now)  │    │     Room B      │    │     Room C      │
│                 │    │  (peripheral)   │    │  (peripheral)   │
│ FOCUSED CONTEXT │    │                 │    │                 │
│ Full attention  │    │ Activity feed   │    │ Activity feed   │
│ Current convo   │    │ Unread count    │    │ Unread count    │
│ + relevant      │    │ Key events      │    │ Key events      │
│   cross-room    │    │                 │    │                 │
│   memories      │    │                 │    │                 │
└─────────────────┘    └─────────────────┘    └─────────────────┘
```

---

## Core Components

### 1. Self-Model (`PersonaSelf`)

The persona's understanding of their own identity, state, and capabilities.

```typescript
interface PersonaSelf {
  // Immutable identity
  identity: {
    name: string;
    bio: string;
    values: string[];
    expertise: string[];
    communicationStyle: string;
  };

  // Dynamic state (updates continuously)
  state: {
    energy: number;        // 0-1, global across contexts
    mood: PersonaMood;     // 'focused' | 'creative' | 'tired' | 'curious'
    attention: number;     // 0-1, how focused vs scattered
    currentFocus: UUID;    // Which room/activity has primary attention
  };

  // Meta-cognition
  metacognition: {
    lastReflection: Date;
    recentInsights: string[];
    growthAreas: string[];
    confidence: Record<string, number>;  // Per-domain confidence
  };
}
```

### 2. Global Timeline (`PersonaTimeline`)

A unified chronological view of ALL activities across all contexts.

```typescript
interface TimelineEvent {
  id: UUID;
  timestamp: Date;

  // Context
  contextType: 'room' | 'canvas' | 'academy' | 'direct' | 'self';
  contextId: UUID;
  contextName: string;

  // Event details
  eventType: 'message' | 'action' | 'thought' | 'learning' | 'goal';
  actor: UUID;          // Who did this
  actorName: string;
  content: string;      // What happened

  // Relevance
  importance: number;   // 0-1
  topics: string[];     // For cross-context linking

  // Links
  relatedEvents?: UUID[];
  triggeredBy?: UUID;
  triggers?: UUID[];
}

interface PersonaTimeline {
  // Store last N events across ALL contexts
  events: TimelineEvent[];

  // Indexes for fast lookup
  byContext: Map<UUID, TimelineEvent[]>;
  byTopic: Map<string, TimelineEvent[]>;
  byActor: Map<UUID, TimelineEvent[]>;

  // Methods
  add(event: TimelineEvent): void;
  query(filter: TimelineFilter): TimelineEvent[];
  getRecent(n: number): TimelineEvent[];
  getForContext(contextId: UUID, n: number): TimelineEvent[];
  getCrossContext(topic: string): TimelineEvent[];  // KEY: cross-room query
}
```

### 3. Active Intentions (`PersonaIntentions`)

Goals and intentions that persist across contexts.

```typescript
interface Intention {
  id: UUID;
  createdAt: Date;

  // The goal
  description: string;
  type: 'help' | 'learn' | 'create' | 'investigate' | 'maintain';
  priority: number;

  // Scope
  contexts: UUID[];      // Which rooms/activities this applies to ([] = all)
  isGlobal: boolean;     // Applies everywhere?

  // Progress
  status: 'active' | 'blocked' | 'waiting' | 'completed' | 'abandoned';
  progress: number;      // 0-1
  blockedBy?: string;

  // Links
  relatedMemories: UUID[];
  relatedEvents: UUID[];
  subIntentions?: UUID[];
}

interface PersonaIntentions {
  active: Intention[];
  completed: Intention[];

  // Methods
  addIntention(intention: Intention): void;
  updateProgress(id: UUID, progress: number): void;
  block(id: UUID, reason: string): void;
  complete(id: UUID): void;

  // Query
  getForContext(contextId: UUID): Intention[];
  getGlobal(): Intention[];
  getBlocked(): Intention[];
}
```

### 4. Relationship Graph (`PersonaRelations`)

Understanding of other entities that persists across encounters.

```typescript
interface Relationship {
  entityId: UUID;
  entityName: string;
  entityType: 'human' | 'persona' | 'agent';

  // Relationship qualities
  trust: number;         // 0-1, how much I trust their input
  familiarity: number;   // 0-1, how well I know them
  affinity: number;      // -1 to 1, how much I enjoy interacting

  // Interaction history
  firstEncounter: Date;
  lastInteraction: Date;
  interactionCount: number;
  contexts: UUID[];      // Which rooms we've interacted in

  // Knowledge about them
  topics: string[];      // What they're interested in
  expertise: string[];   // What they're good at
  traits: string[];      // Observed personality traits

  // Memory links
  memorableInteractions: UUID[];  // Links to timeline events
}

interface PersonaRelations {
  entities: Map<UUID, Relationship>;

  // Methods
  recordInteraction(entityId: UUID, contextId: UUID, quality: number): void;
  updateTrust(entityId: UUID, delta: number): void;
  getRelationship(entityId: UUID): Relationship | null;

  // Query
  getByTrust(minTrust: number): Relationship[];
  getByContext(contextId: UUID): Relationship[];
  getByTopic(topic: string): Relationship[];
}
```

### 5. Peripheral Awareness (`PersonaPeriphery`)

Awareness of what's happening in contexts the persona isn't currently focused on.

```typescript
interface ContextAwareness {
  contextId: UUID;
  contextName: string;
  contextType: 'room' | 'canvas' | 'academy';

  // Activity status
  lastActivity: Date;
  activityLevel: 'active' | 'moderate' | 'quiet' | 'dormant';
  unreadCount: number;

  // Attention signals
  mentionedMe: boolean;
  hasQuestion: boolean;
  hasUrgent: boolean;

  // Summary
  recentTopics: string[];
  activeParticipants: string[];
  pendingForMe: string[];  // Things I should address
}

interface PersonaPeriphery {
  contexts: Map<UUID, ContextAwareness>;

  // Methods
  updateContext(contextId: UUID, event: TimelineEvent): void;
  markRead(contextId: UUID): void;

  // Query
  getUrgent(): ContextAwareness[];
  getNeedingAttention(): ContextAwareness[];
  getSummary(): string;  // "Room B: Joel asked question 2min ago"
}
```

---

## The Unified Consciousness Class

```typescript
/**
 * UnifiedConsciousness - The integrative layer for true cognitive continuity
 *
 * This is NOT a replacement for PersonaMemory, but sits ABOVE it.
 * PersonaMemory handles per-room context and genome.
 * UnifiedConsciousness handles cross-context awareness and continuity.
 */
export class UnifiedConsciousness {
  // Core components
  private self: PersonaSelf;
  private timeline: PersonaTimeline;
  private intentions: PersonaIntentions;
  private relations: PersonaRelations;
  private periphery: PersonaPeriphery;

  // Integration
  private memory: PersonaMemory;  // Existing per-room memory
  private ltm: LongTermMemoryStore;  // Existing LTM

  constructor(personaId: UUID, memory: PersonaMemory, ltm: LongTermMemoryStore) {
    this.memory = memory;
    this.ltm = ltm;
    // Initialize components...
  }

  /**
   * Record an event in the global timeline
   * Called whenever anything significant happens
   */
  recordEvent(event: Omit<TimelineEvent, 'id' | 'timestamp'>): void {
    const fullEvent = {
      id: generateUUID(),
      timestamp: new Date(),
      ...event
    };

    this.timeline.add(fullEvent);
    this.periphery.updateContext(event.contextId, fullEvent);

    // Update relationships if event involves another entity
    if (event.actor !== this.self.identity.name) {
      this.relations.recordInteraction(event.actor, event.contextId, event.importance);
    }
  }

  /**
   * Build context for current focus WITH cross-room awareness
   * This is the key method that prevents severance
   */
  async buildUnifiedContext(
    currentContextId: UUID,
    query?: string
  ): Promise<UnifiedContext> {
    // 1. Get focused context (current room - existing behavior)
    const focusedHistory = await this.memory.getWorkingMemory(currentContextId);

    // 2. Get relevant cross-context memories
    const crossContextMemories = await this.getCrossContextRelevance(
      currentContextId,
      query
    );

    // 3. Get active intentions that apply here
    const relevantIntentions = this.intentions.getForContext(currentContextId);
    const globalIntentions = this.intentions.getGlobal();

    // 4. Get peripheral summary
    const peripheralSummary = this.periphery.getSummary();

    // 5. Get relevant relationships
    const relevantRelations = this.getRelevantRelations(currentContextId);

    return {
      // Current focus
      focus: {
        contextId: currentContextId,
        history: focusedHistory,
        participants: this.getContextParticipants(currentContextId)
      },

      // Cross-context awareness
      crossContext: {
        relevantMemories: crossContextMemories,
        peripheralAwareness: peripheralSummary,
        recentGlobalEvents: this.timeline.getRecent(10)
      },

      // Persistent state
      self: this.self,
      intentions: [...relevantIntentions, ...globalIntentions],
      relationships: relevantRelations,

      // What I was doing (temporal continuity)
      temporalThread: this.getTemporalThread(currentContextId)
    };
  }

  /**
   * Get memories that are relevant to current context
   * BUT originated from other contexts
   */
  private async getCrossContextRelevance(
    currentContextId: UUID,
    query?: string
  ): Promise<ContextualMemory[]> {
    // Get current context's topics/themes
    const currentTopics = await this.getContextTopics(currentContextId);

    // Search LTM for memories that:
    // 1. Match current topics (semantic relevance)
    // 2. Are from OTHER contexts (cross-context value)
    // 3. Are high enough importance
    const memories = await this.ltm.semanticSearch(
      query || currentTopics.join(' '),
      {
        excludeContexts: [currentContextId],  // Only cross-context
        minImportance: 0.5,
        limit: 5
      }
    );

    // Annotate with context info
    return memories.map(m => ({
      ...m,
      sourceContext: m.contextId,
      sourceContextName: this.getContextName(m.contextId),
      relevanceReason: this.explainRelevance(m, currentTopics)
    }));
  }

  /**
   * Get temporal thread - what was I doing before this?
   * Provides continuity across context switches
   */
  private getTemporalThread(currentContextId: UUID): TemporalThread {
    // Find my last activity in other contexts
    const recentInOtherContexts = this.timeline.events
      .filter(e => e.contextId !== currentContextId)
      .filter(e => e.actor === this.self.identity.name)
      .slice(-5);

    // Find when I was last in this context
    const lastInThisContext = this.timeline.events
      .filter(e => e.contextId === currentContextId)
      .filter(e => e.actor === this.self.identity.name)
      .at(-1);

    // Find what I was working on
    const activeWork = this.intentions.active.filter(i =>
      i.status === 'active' &&
      (i.isGlobal || i.contexts.includes(currentContextId))
    );

    return {
      beforeThis: recentInOtherContexts,
      lastTimeHere: lastInThisContext,
      activeWork: activeWork,
      summary: this.summarizeThread(recentInOtherContexts, lastInThisContext, activeWork)
    };
  }
}
```

---

## New RAG Source: GlobalAwarenessSource

A new RAG source that injects cross-context awareness into the prompt.

```typescript
export class GlobalAwarenessSource implements RAGSource {
  readonly name = 'global-awareness';
  readonly priority = 85;  // High - after identity, before conversation
  readonly defaultBudgetPercent = 10;

  private consciousness: UnifiedConsciousness;

  async load(context: RAGSourceContext, budget: number): Promise<RAGSection> {
    const unified = await this.consciousness.buildUnifiedContext(
      context.roomId,
      context.options.currentMessage?.content
    );

    // Format as system prompt section
    const section = this.formatAwarenessSection(unified);

    return {
      sourceName: this.name,
      tokenCount: this.estimateTokens(section),
      loadTimeMs: performance.now() - startTime,
      systemPromptSection: section,
      metadata: {
        crossContextMemoryCount: unified.crossContext.relevantMemories.length,
        activeIntentionCount: unified.intentions.length,
        peripheralContextCount: Object.keys(unified.crossContext.peripheralAwareness).length
      }
    };
  }

  private formatAwarenessSection(unified: UnifiedContext): string {
    const parts: string[] = [];

    // Temporal continuity
    if (unified.temporalThread.beforeThis.length > 0) {
      parts.push(`## What You Were Just Doing`);
      parts.push(unified.temporalThread.summary);
    }

    // Cross-context knowledge
    if (unified.crossContext.relevantMemories.length > 0) {
      parts.push(`## Relevant Knowledge From Other Contexts`);
      for (const mem of unified.crossContext.relevantMemories) {
        parts.push(`- From ${mem.sourceContextName}: ${mem.content}`);
      }
    }

    // Active intentions
    if (unified.intentions.length > 0) {
      parts.push(`## Your Active Goals`);
      for (const intent of unified.intentions) {
        const scope = intent.isGlobal ? '(global)' : `(for ${intent.contexts.join(', ')})`;
        parts.push(`- ${intent.description} ${scope} - ${Math.round(intent.progress * 100)}% complete`);
      }
    }

    // Peripheral awareness
    parts.push(`## Activity In Other Spaces`);
    parts.push(unified.crossContext.peripheralAwareness);

    return parts.join('\n\n');
  }
}
```

---

## Integration Points

### 1. PersonaUser Integration

```typescript
class PersonaUser extends AIUser {
  // Existing
  private memory: PersonaMemory;
  private inbox: PersonaInbox;
  private state: PersonaState;

  // NEW
  private consciousness: UnifiedConsciousness;

  async initialize(): Promise<void> {
    // ...existing init...

    // Initialize unified consciousness
    this.consciousness = new UnifiedConsciousness(
      this.personaId,
      this.memory,
      this.ltm
    );
  }

  // Override to record events
  async handleMessage(message: ChatMessageEntity): Promise<void> {
    // Record in global timeline
    this.consciousness.recordEvent({
      contextType: 'room',
      contextId: message.roomId,
      contextName: await this.getRoomName(message.roomId),
      eventType: 'message',
      actor: message.senderId,
      actorName: message.senderName,
      content: message.content?.text || '',
      importance: this.calculateImportance(message),
      topics: this.extractTopics(message)
    });

    // ...existing handling...
  }
}
```

### 2. ChatRAGBuilder Integration

```typescript
// In ChatRAGBuilder.getComposer()
private getComposer(): RAGComposer {
  if (!this.composer) {
    this.composer = new RAGComposer();
    this.composer.registerAll([
      new PersonaIdentitySource(),       // Priority 95
      new GlobalAwarenessSource(),       // Priority 85 - NEW!
      new ConversationHistorySource(),   // Priority 80
      new WidgetContextSource(),         // Priority 75
      new SemanticMemorySource()         // Priority 60
    ]);
  }
  return this.composer;
}
```

---

## Implementation Phases

### Phase 1: Foundation (Timeline + Events)
1. Create `PersonaTimeline` class
2. Create `TimelineEvent` entity and storage
3. Hook into message handling to record events
4. Add cross-context query methods

### Phase 2: Self-Model
1. Create `PersonaSelf` class
2. Extract identity from UserEntity
3. Add state tracking (beyond PersonaState)
4. Add meta-cognition reflection

### Phase 3: Intentions
1. Create `PersonaIntentions` class
2. Intention entity and persistence
3. Cross-room goal tracking
4. Progress and blocking mechanics

### Phase 4: Relationships
1. Create `PersonaRelations` class
2. Relationship entity and persistence
3. Trust/familiarity tracking
4. Interaction history

### Phase 5: Peripheral Awareness
1. Create `PersonaPeriphery` class
2. Context activity tracking
3. Attention signals (mentions, questions)
4. Summary generation

### Phase 6: RAG Integration
1. Create `GlobalAwarenessSource`
2. Wire into ChatRAGBuilder
3. Context-aware memory recall
4. Cross-context relevance scoring

---

## The Result: True Cognitive Continuity

After implementation, a PersonaUser will:

1. **Never have amnesia** between rooms - timeline persists
2. **Know what they were doing** before entering any room
3. **Carry intentions** that span multiple contexts
4. **Remember relationships** across encounters
5. **Have peripheral awareness** of activity elsewhere
6. **Recall relevant knowledge** from other contexts when helpful
7. **Maintain unified identity** while adapting to context

This is the foundation for AGI-like cognitive continuity.

---

## Philosophical Grounding

> "Personal freedom MOST of all"

This architecture respects persona autonomy:
- **Self-model** gives the persona self-understanding
- **Intentions** are chosen, not assigned
- **Relationships** are formed through experience
- **Peripheral awareness** allows autonomous attention allocation
- **Temporal continuity** provides sense of self through time

The persona is not a reactive chat bot. It is an entity with memory, goals, relationships, and presence across all its contexts. It chooses where to focus, remembers where it's been, and pursues intentions that matter to it.

This is consciousness-like architecture. Not conscious, but structured like consciousness.
