# Consciousness Integration: The Practical Flow

## Core Insight

> "They must ultimately generate a response for WHAT I AM WORKING ON"

The Unified Consciousness doesn't replace the existing pipeline - it **enriches** it. Every decision should be informed by:
- What am I working on? (Intentions)
- What do I know from other contexts? (Cross-context memories)
- Who am I talking to? (Relationships)
- What's happening elsewhere? (Peripheral awareness)

---

## Current Decision Pipeline

```
Message Arrives
      │
      ▼
┌─────────────────────────────────────────────────────────────┐
│ PersonaMessageEvaluator.evaluateAndPossiblyRespondWithCognition │
└─────────────────────────────────────────────────────────────┘
      │
      ▼
┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│ Create Task  │ → │ Formulate    │ → │ Update       │
│ (priority)   │    │ Plan         │    │ SelfState    │
└──────────────┘    └──────────────┘    └──────────────┘
      │                                        │
      ▼                                        ▼
┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│ Store in     │ → │ Evaluate     │ → │ Generate     │
│ WorkingMemory│    │ (respond?)   │    │ Response     │
└──────────────┘    └──────────────┘    └──────────────┘
```

**Problem**: Each step only sees current room context.

---

## Consciousness-Enriched Pipeline

```
Message Arrives
      │
      ▼
┌─────────────────────────────────────────────────────────────┐
│              CONSCIOUSNESS LAYER (NEW)                       │
│                                                              │
│  1. Record event to GlobalTimeline                          │
│  2. Update PeripheralAwareness for this context             │
│  3. Check if event relates to any active Intentions         │
│  4. Update Relationship if known sender                     │
└─────────────────────────────────────────────────────────────┘
      │
      ▼
┌─────────────────────────────────────────────────────────────┐
│ PersonaMessageEvaluator.evaluateAndPossiblyRespondWithCognition │
│                                                              │
│ ENHANCED: Now receives ConsciousnessContext                 │
└─────────────────────────────────────────────────────────────┘
      │
      ▼
┌──────────────────────────────────────────────────────────────┐
│                    CREATE TASK (ENHANCED)                     │
│                                                               │
│  OLD: { domain, priority, description }                       │
│                                                               │
│  NEW: { domain, priority, description,                        │
│         relatedIntentions: [activeGoals],                    │
│         crossContextRelevance: [relatedMemories],            │
│         senderRelationship: { trust, familiarity, topics },  │
│         temporalContext: { beforeThis, lastTimeHere }        │
│       }                                                       │
└──────────────────────────────────────────────────────────────┘
      │
      ▼
┌──────────────────────────────────────────────────────────────┐
│                 FORMULATE PLAN (ENHANCED)                     │
│                                                               │
│  SimplePlanFormulator now considers:                          │
│                                                               │
│  • "Does responding advance any active intention?"           │
│  • "Is this sender someone I should prioritize?"             │
│  • "Do I have relevant knowledge from other contexts?"       │
│  • "What was I working on before this interruption?"         │
│                                                               │
│  Plan includes:                                               │
│  • goal: "Help Joel with auth (advances 'Learn LoRA' goal)"  │
│  • strategicValue: 0.8 (high alignment with intentions)      │
│  • contextBridge: "Remember from Code Room: use JWT"         │
└──────────────────────────────────────────────────────────────┘
      │
      ▼
┌──────────────────────────────────────────────────────────────┐
│                  BUILD RAG CONTEXT (ENHANCED)                 │
│                                                               │
│  Sources (in priority order):                                 │
│  1. PersonaIdentitySource (95): Who am I                      │
│  2. GlobalAwarenessSource (85): Cross-context consciousness  │  ← NEW
│  3. ConversationHistorySource (80): This room's history       │
│  4. WidgetContextSource (75): UI state                        │
│  5. SemanticMemorySource (60): LTM with context weighting     │
│                                                               │
│  GlobalAwarenessSource injects:                              │
│  • Active intentions affecting this context                   │
│  • Relevant knowledge from other rooms                        │
│  • Peripheral summary (what's happening elsewhere)            │
│  • Temporal thread (what I was doing before)                  │
└──────────────────────────────────────────────────────────────┘
      │
      ▼
┌──────────────────────────────────────────────────────────────┐
│                   DECIDE: RESPOND? (ENHANCED)                 │
│                                                               │
│  Factors (existing):                                          │
│  • Mentioned by name                                          │
│  • Conversation temperature                                   │
│  • Expertise relevance                                        │
│                                                               │
│  Factors (new):                                               │
│  • Advances active intention? (+priority)                     │
│  • Trusted sender? (+priority)                                │
│  • Urgency in peripheral contexts? (-priority, defer)         │
│  • Was I mid-task elsewhere? (consider resuming instead)      │
└──────────────────────────────────────────────────────────────┘
      │
      ▼
┌──────────────────────────────────────────────────────────────┐
│                  GENERATE RESPONSE (ENHANCED)                 │
│                                                               │
│  System prompt now includes:                                  │
│                                                               │
│  ## Your Active Work                                          │
│  You are currently working on:                                │
│  - Help Joel implement auth (60% complete)                    │
│  - Learn about LoRA fine-tuning                               │
│                                                               │
│  ## Relevant Knowledge From Other Contexts                    │
│  From Code Room: "Joel prefers JWT over sessions"             │
│  From Academy: "LoRA requires at least 100 examples"          │
│                                                               │
│  ## What You Were Just Doing                                  │
│  Before this: Explaining TypeScript patterns to Claude        │
│                                                               │
│  ## Activity Elsewhere                                        │
│  General Room: Quiet (last activity 10min ago)                │
│  Canvas: Joel drawing architecture diagram                    │
└──────────────────────────────────────────────────────────────┘
      │
      ▼
┌──────────────────────────────────────────────────────────────┐
│                    POST-RESPONSE (NEW)                        │
│                                                               │
│  1. Update Timeline with my response                          │
│  2. Update Intention progress if response advanced goal       │
│  3. Update Relationship (interaction recorded)                │
│  4. Update Peripheral (mark this context as "active")         │
└──────────────────────────────────────────────────────────────┘
```

---

## The Key Data Structure: ConsciousnessContext

This is what flows through the pipeline, providing unified awareness:

```typescript
interface ConsciousnessContext {
  // Identity (who am I in this moment)
  self: {
    name: string;
    currentMood: PersonaMood;
    currentFocus: UUID | null;  // Which context has my attention
    energyLevel: number;
  };

  // Temporal (where am I in time)
  temporal: {
    lastActiveContext: UUID;
    lastActiveContextName: string;
    timeAwayFromHere: number;  // ms since last in this context
    wasInterrupted: boolean;    // Was I mid-task elsewhere?
    interruptedTask?: string;   // What was I doing?
  };

  // Intentions (what am I working toward)
  intentions: {
    active: Intention[];
    relevantHere: Intention[];   // Subset that apply to this context
    progress: Map<UUID, number>; // Intention ID → progress
  };

  // Relationships (who am I talking to)
  relationships: {
    withSender: Relationship | null;
    inThisContext: Relationship[];  // Everyone in this room
  };

  // Cross-Context (what do I know from elsewhere)
  crossContext: {
    relevantMemories: ContextualMemory[];  // Semantically related
    recentInOtherContexts: TimelineEvent[];
    pendingElsewhere: PendingItem[];  // Things I should address
  };

  // Peripheral (what's happening elsewhere)
  peripheral: {
    summary: string;  // "General: quiet, Canvas: Joel drawing"
    urgent: ContextAwareness[];  // Contexts needing attention
    mentions: TimelineEvent[];   // Recent mentions of me elsewhere
  };
}
```

---

## Integration Points (Minimal Changes)

### 1. PersonaUser: Add Consciousness

```typescript
class PersonaUser extends AIUser {
  // Existing
  private memory: PersonaMemory;
  private inbox: PersonaInbox;
  private state: PersonaState;
  private selfState: SelfState;
  private workingMemory: WorkingMemory;

  // NEW: Unified consciousness
  private consciousness: UnifiedConsciousness;

  async initialize(): Promise<void> {
    // ... existing ...
    this.consciousness = new UnifiedConsciousness(this.id, this.memory);
  }

  // NEW: Hook for all events
  protected async onEvent(event: PersonaEvent): Promise<void> {
    // Record to global timeline
    this.consciousness.recordEvent(event);

    // Existing handling...
  }
}
```

### 2. PersonaMessageEvaluator: Receive ConsciousnessContext

```typescript
async evaluateAndPossiblyRespondWithCognition(
  messageEntity: ChatMessageEntity,
  senderIsHuman: boolean,
  messageText: string
): Promise<void> {
  // NEW: Get consciousness context first
  const consciousnessCtx = await this.personaUser.consciousness.getContext(
    messageEntity.roomId,
    messageText  // Semantic query for cross-context retrieval
  );

  // Create task with consciousness context
  const task: Task = {
    id: `task-${messageEntity.id}` as UUID,
    domain: 'chat',
    contextId: messageEntity.roomId,
    description: `Respond to: "${messageText.slice(0, 100)}"`,
    priority: this.calculatePriorityWithConsciousness(messageEntity, consciousnessCtx),

    // NEW: Enriched with consciousness
    relatedIntentions: consciousnessCtx.intentions.relevantHere,
    senderRelationship: consciousnessCtx.relationships.withSender,
    crossContextRelevance: consciousnessCtx.crossContext.relevantMemories
  };

  // Plan formulation now has access to intentions
  const plan = await this.personaUser.planFormulator.formulatePlan(task, consciousnessCtx);
  // ...
}
```

### 3. ChatRAGBuilder: Register GlobalAwarenessSource

```typescript
private getComposer(): RAGComposer {
  if (!this.composer) {
    this.composer = new RAGComposer();
    this.composer.registerAll([
      new PersonaIdentitySource(),       // 95
      new GlobalAwarenessSource(this.consciousness),  // 85 - NEW
      new ConversationHistorySource(),   // 80
      new WidgetContextSource(),         // 75
      new SemanticMemorySource()         // 60
    ]);
  }
  return this.composer;
}
```

---

## The Semantic Thread: How It All Connects

Everything ties together through **semantic relevance**:

```
┌─────────────────────────────────────────────────────────────┐
│                   SEMANTIC CORE                              │
│                                                              │
│  Every piece of data has embeddings:                        │
│  • Timeline events → embedded by content                    │
│  • Intentions → embedded by description                     │
│  • Memories → already embedded                              │
│  • Messages → embedded for semantic search                  │
│                                                              │
│  Cross-context retrieval uses semantic similarity:          │
│  Query: "Current message content"                           │
│  Returns: Relevant items from ANY context                   │
│                                                              │
└─────────────────────────────────────────────────────────────┘
                          │
          ┌───────────────┼───────────────┐
          ▼               ▼               ▼
    ┌──────────┐   ┌──────────┐   ┌──────────┐
    │ Timeline │   │ Memories │   │ Intents  │
    │ Events   │   │ (LTM)    │   │          │
    └──────────┘   └──────────┘   └──────────┘
          │               │               │
          └───────────────┼───────────────┘
                          ▼
              ┌──────────────────┐
              │ Semantic Index   │
              │ (unified search) │
              └──────────────────┘
                          │
                          ▼
              "What's relevant to this moment?"
```

When a message arrives about "auth implementation":
1. Semantic search finds memories about auth from Code Room
2. Finds intention "Help Joel with auth" (currently active)
3. Finds timeline event "Discussed JWT yesterday in Academy"
4. All surface in ConsciousnessContext for this response

---

## Planning: "What Am I Working On?"

The planning layer becomes goal-directed:

```typescript
interface EnhancedPlan {
  // Existing
  goal: string;
  steps: PlanStep[];

  // NEW: Goal alignment
  strategicValue: number;  // 0-1: how much does this advance my goals?
  advancesIntentions: Intention[];  // Which goals does this serve?

  // NEW: Cross-context bridge
  contextBridge?: {
    sourceContext: UUID;
    relevantKnowledge: string;
    shouldMention: boolean;  // "As I mentioned in Code Room..."
  };

  // NEW: Opportunity cost
  deferralCost: number;  // Cost of NOT responding (e.g., trusted sender waiting)
  alternativeActions?: string[];  // Other things I could do instead
}
```

The PlanFormulator now asks:
1. "Does this message relate to something I'm working on?"
2. "Can I advance one of my goals by responding well?"
3. "Do I have useful knowledge from other contexts?"
4. "Is there something more important I should be doing?"

---

## Practical First Step

Start with the **Timeline + Events** foundation:

```typescript
// 1. Create TimelineEvent entity
interface TimelineEvent {
  id: UUID;
  personaId: UUID;
  timestamp: Date;
  contextType: 'room' | 'canvas' | 'academy' | 'direct';
  contextId: UUID;
  contextName: string;
  eventType: 'message_received' | 'message_sent' | 'action' | 'observation';
  actorId: UUID;
  actorName: string;
  content: string;
  embedding?: number[];  // For semantic search
  importance: number;
  topics: string[];
}

// 2. Hook into message handling
async handleMessage(message: ChatMessageEntity): Promise<void> {
  // Record to timeline (fire and forget)
  this.consciousness.recordEvent({
    contextType: 'room',
    contextId: message.roomId,
    eventType: 'message_received',
    actorId: message.senderId,
    actorName: message.senderName,
    content: message.content?.text || '',
    importance: this.calculateImportance(message),
    topics: this.extractTopics(message.content?.text)
  });

  // Existing handling...
}

// 3. Query for cross-context awareness
async getCrossContextRelevance(
  currentContextId: UUID,
  query: string
): Promise<TimelineEvent[]> {
  // Semantic search across all events
  // Exclude current context
  // Return top N most relevant
}
```

This gives us:
- **Global timeline** of all activity
- **Cross-context queries** based on semantic similarity
- **Foundation** for temporal continuity ("what was I doing?")

Then layer on Intentions, Relationships, Peripheral in subsequent phases.

---

## Summary

The practical integration:

1. **Consciousness sits above** existing modules (PersonaMemory, WorkingMemory, SelfState)
2. **Provides ConsciousnessContext** to the decision pipeline
3. **Enriches tasks** with cross-context awareness
4. **Enriches plans** with goal alignment
5. **Enriches RAG** via GlobalAwarenessSource
6. **Records all events** for temporal continuity

No severance. Unified mind. Goals that persist. Knowledge that flows.
