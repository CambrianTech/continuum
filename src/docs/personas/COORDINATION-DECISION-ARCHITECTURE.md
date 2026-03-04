# CoordinationDecision Architecture
**The Universal Training Dataset for AI Decision-Making**

## Vision: Time-Travel Debugging + Meta-Learning

Every decision point (human or AI) gets logged with **complete reproducibility** - the exact context they saw, what they decided, and why. This enables:

1. **Time-Travel Replay**: Take any historical decision → plug in different persona/model → see what they would've done
2. **Autopilot Training**: Train models on a user's decision history → predict what they'd choose
3. **Meta-Learning**: Companion AI suggestions get logged → next generation learns from human overrides
4. **Cross-Domain Transfer**: Same pattern works for chat, games, code review, any activity

## Entity Structure

```typescript
/**
 * CoordinationDecisionEntity - Universal decision point capture
 *
 * Stores complete context for any decision (respond/silent, attack/retreat, approve/reject)
 * Enables training, replay, comparison across personas, domains, and time
 */
interface CoordinationDecisionEntity extends BaseEntity {
  // ============================================================================
  // IDENTITY - Who and what
  // ============================================================================

  id: UUID;
  timestamp: number;

  /** Who made the decision (human or AI persona) */
  actorId: UUID;
  actorType: 'human' | 'ai-persona';
  actorName: string;

  /** What triggered this decision point */
  triggerEventId: UUID;        // MessageId, gameEventId, PRId, etc.
  domain: 'chat' | 'game' | 'code' | 'analysis';

  // ============================================================================
  // COMPLETE CONTEXT - What they saw (full reproducibility)
  // ============================================================================

  /**
   * Complete RAG context - EXACTLY what the LLM saw
   * Stored as full RAGContext object for perfect replay
   */
  ragContext: RAGContext;
  // Includes:
  // - identity (systemPrompt, bio, role)
  // - recipeStrategy (conversation rules, decision criteria)
  // - conversationHistory (all messages they saw)
  // - artifacts (screenshots, files, images)
  // - privateMemories (internal knowledge)
  // - metadata (timestamps, counts)

  /**
   * Domain-specific visual context
   * What the actor actually saw on their screen
   */
  visualContext: {
    type: 'chat-ui' | 'game-screen' | 'code-diff' | 'dashboard';

    // Chat UI
    visibleMessages?: Message[];
    scrollPosition?: number;
    activeTab?: string;
    notifications?: NotificationState[];

    // Game screen
    screenshot?: string;  // Base64
    gameState?: {
      playerPosition: Vector3;
      enemiesVisible: Entity[];
      healthBar: number;
      inventory: Item[];
    };
    controlInputs?: {
      keyboard: string[];
      mouse: MouseState;
      gamepad?: GamepadState;
    };

    // Code review
    files?: Array<{
      path: string;
      diff: string;
      linterWarnings: Warning[];
    }>;
    testResults?: TestSummary;
    ciStatus?: 'passing' | 'failing';
  };

  // ============================================================================
  // COORDINATION STATE - Who else is involved
  // ============================================================================

  coordinationSnapshot: {
    /** ThoughtStream context (for multi-agent coordination) */
    thoughtStreamId?: string;
    phase?: 'gathering' | 'deciding' | 'closed';
    availableSlots?: number;

    /** This actor's thought in the stream */
    myThought?: {
      confidence: number;
      priority: number;
      timestamp: number;
    };

    /** Other actors' thoughts */
    competingThoughts?: Array<{
      actorId: UUID;
      actorName: string;
      confidence: number;
      priority: number;
    }>;

    /** Who else is considering responding */
    othersConsideringCount: number;
    othersConsideringNames: string[];
  };

  // ============================================================================
  // AMBIENT STATE - Activity metadata (Phase 3bis)
  // ============================================================================

  ambientState: {
    /** Temperature (0-1): Conversation "heat" level */
    temperature: number;

    /** Is the human currently present/watching? */
    userPresent: boolean;

    /** Time since last response (ms) */
    timeSinceLastResponse: number;

    /** Recent activity level */
    messagesInLastMinute: number;

    /** Was this actor explicitly mentioned? */
    mentionedByName: boolean;

    /** Queue pressure (future) */
    pressure?: number;
  };

  // ============================================================================
  // THE DECISION - What they chose
  // ============================================================================

  decision: {
    /** The action taken */
    action: 'POSTED' | 'SILENT' | 'ERROR' | 'TIMEOUT' | 'ATTACK' | 'RETREAT' | 'APPROVE' | 'REJECT';

    /** Confidence in this decision (0-1) */
    confidence: number;

    /** LLM's reasoning (if available) */
    reasoning?: string;

    /** Response content (if action produced output) */
    responseContent?: string;

    /** Model that made the decision */
    modelUsed?: string;
    modelProvider?: string;

    /** Resource usage */
    tokensUsed?: number;
    responseTime: number;  // ms to decide + generate

    /** Companion AI suggestion (for meta-learning) */
    companionSuggestion?: {
      suggestedAction: string;
      confidence: number;
      reasoning: string;
      wasFollowed: boolean;  // Did human follow the suggestion?
    };
  };

  // ============================================================================
  // OUTCOME - Training label (post-hoc evaluation)
  // ============================================================================

  outcome?: {
    /** Was this a good decision? */
    wasGoodDecision: boolean;

    /** Numeric rating (optional, 0-1 or 1-5 scale) */
    rating?: number;

    /** Why was it good/bad? */
    reasoning: string;

    /** Who rated it? */
    ratedBy: 'self' | 'user' | 'system' | 'community';

    /** When rated */
    ratedAt: number;

    /** Follow-up data (for long-term outcome tracking) */
    followUp?: {
      conversationContinued: boolean;
      userSatisfaction?: number;
      mistakeCorrected?: boolean;
    };
  };

  // ============================================================================
  // METADATA - For querying and analysis
  // ============================================================================

  metadata: {
    /** Session this decision occurred in */
    sessionId: UUID;

    /** Context (room, game session, PR, etc.) */
    contextId: UUID;

    /** Decision sequence number (for ordering) */
    sequenceNumber: number;

    /** Tags for filtering */
    tags?: string[];  // ['greeting', 'technical-question', 'off-topic']

    /** Experiment/version tracking */
    experimentId?: string;
    modelVersion?: string;
    systemVersion?: string;
  };
}
```

## Use Cases

### 1. Time-Travel Debugging
```typescript
// "Why did Helper AI respond here?"
const decision = await getDecision(decisionId);
console.log(decision.ragContext);           // See EXACTLY what they saw
console.log(decision.decision.reasoning);   // See why they decided
console.log(decision.ambientState);         // See temperature, presence, etc.

// "What would NEW Helper AI do with the same context?"
const replay = await replayDecision(decision.ragContext, newPersonaId);
console.log(replay.decision.action);        // Compare: POSTED vs SILENT
```

### 2. Autopilot Training
```typescript
// Train model on Joel's decisions
const joelDecisions = await query({
  actorId: joelId,
  limit: 10000,
  outcome: { wasGoodDecision: true }  // Only good decisions
});

// Dataset: [RAG context + ambient state] → decision
const trainingData = joelDecisions.map(d => ({
  input: {
    ragContext: d.ragContext,
    ambientState: d.ambientState,
    coordinationState: d.coordinationSnapshot
  },
  output: {
    action: d.decision.action,
    confidence: d.decision.confidence
  }
}));

// Train autopilot
const joelAutopilot = await trainModel(trainingData);
```

### 3. Meta-Learning (Companion AI)
```typescript
// Log companion suggestion
const decision = {
  // ... full context ...
  decision: {
    action: 'SILENT',  // Human chose this
    companionSuggestion: {
      suggestedAction: 'POSTED',  // Companion suggested this
      confidence: 0.8,
      reasoning: "User was mentioned by name",
      wasFollowed: false  // Human disagreed!
    }
  }
};

// Next training iteration learns:
// "When companion suggests responding to greetings, human ignores it"
// → Companion gets smarter about when to suggest
```

### 4. Cross-Persona Comparison
```typescript
// "How do different personas handle the same message?"
const message = await getMessage(messageId);

const helperDecision = await replayDecision(message, helperAIId);
const codeReviewDecision = await replayDecision(message, codeReviewAIId);
const groqDecision = await replayDecision(message, groqAIId);

// Compare: Who responded? Who stayed silent? Why?
console.table([
  { persona: 'Helper', action: helperDecision.action, confidence: helperDecision.confidence },
  { persona: 'CodeReview', action: codeReviewDecision.action, confidence: codeReviewDecision.confidence },
  { persona: 'Groq', action: groqDecision.action, confidence: groqDecision.confidence }
]);
```

### 5. Domain Transfer
```typescript
// Same pattern works across domains

// Chat decision
const chatDecision: CoordinationDecisionEntity = {
  domain: 'chat',
  visualContext: { type: 'chat-ui', visibleMessages: [...] },
  decision: { action: 'POSTED', responseContent: "Here's how..." }
};

// Game decision
const gameDecision: CoordinationDecisionEntity = {
  domain: 'game',
  visualContext: { type: 'game-screen', screenshot: '...', gameState: {...} },
  decision: { action: 'ATTACK', responseContent: null }
};

// Code review decision
const codeDecision: CoordinationDecisionEntity = {
  domain: 'code',
  visualContext: { type: 'code-diff', files: [...], testResults: {...} },
  decision: { action: 'APPROVE', responseContent: "LGTM" }
};

// All use same training pipeline!
```

## Implementation Phases

### Phase 5A: Entity + Schema (Now)
1. Create `CoordinationDecisionEntity` type
2. Add to `EntityRegistry`
3. Create SQLite schema with JSON columns for complex fields
4. Test CRUD operations

### Phase 5B: Logging Integration (PersonaUser)
1. Capture RAG context before decision
2. Log coordination snapshot from ThoughtStream
3. Fetch ambient state from ChatCoordinator
4. Save decision with all context
5. Add self-rating mechanism

### Phase 5C: Query + Analysis Commands
1. `./jtag decision/list` - List decisions with filters
2. `./jtag decision/inspect` - View full decision context
3. `./jtag decision/replay` - Replay decision with different persona
4. `./jtag decision/compare` - Compare multiple personas on same decision
5. `./jtag decision/rate` - Add post-hoc ratings

### Phase 5D: Training Pipeline (Future)
1. Export decisions to JSONL
2. Transform to training format (input/output pairs)
3. Fine-tune models on decision patterns
4. Evaluate on held-out test set
5. Deploy improved models

## Key Design Principles

### 1. Complete Reproducibility
Store EVERYTHING needed to replay the decision. Never summarize or compress - disk is cheap, missing context is expensive.

### 2. Domain Agnostic
Same entity structure works for chat, games, code, any domain. Domain-specific details in `visualContext` and `ragContext.domain`.

### 3. Time-Travel Ready
Every field is timestamped. Can reconstruct exact system state at any decision point.

### 4. Meta-Learning Native
Companion suggestions are first-class citizens. System learns from human overrides.

### 5. Training-First Design
Structure optimized for machine learning:
- Clear input/output separation
- Normalized confidence scores
- Rich labels (outcome ratings)
- Easy to export to JSONL

## Storage Considerations

### SQLite Schema
```sql
CREATE TABLE coordination_decisions (
  id TEXT PRIMARY KEY,
  timestamp INTEGER NOT NULL,

  -- Identity
  actor_id TEXT NOT NULL,
  actor_type TEXT NOT NULL,
  actor_name TEXT NOT NULL,
  trigger_event_id TEXT NOT NULL,
  domain TEXT NOT NULL,

  -- Complete context (JSON columns for complex data)
  rag_context TEXT NOT NULL,           -- JSON: RAGContext
  visual_context TEXT,                 -- JSON: domain-specific
  coordination_snapshot TEXT,          -- JSON: ThoughtStream state
  ambient_state TEXT NOT NULL,         -- JSON: temperature, presence, etc.

  -- Decision
  decision TEXT NOT NULL,              -- JSON: action, confidence, reasoning

  -- Outcome (nullable - rated later)
  outcome TEXT,                        -- JSON: rating, reasoning

  -- Metadata
  metadata TEXT NOT NULL,              -- JSON: session, context, tags

  -- Indexes for common queries
  INDEX idx_actor_timestamp ON coordination_decisions(actor_id, timestamp DESC),
  INDEX idx_domain_timestamp ON coordination_decisions(domain, timestamp DESC),
  INDEX idx_trigger_event ON coordination_decisions(trigger_event_id),
  INDEX idx_rated ON coordination_decisions(
    CAST(json_extract(outcome, '$.wasGoodDecision') AS INTEGER)
  )
);
```

### Size Estimates
- Average decision: ~10-50KB (RAG context is biggest part)
- 1000 decisions/day: 10-50MB/day
- 1 year: 3.6-18GB
- Disk is cheap, completeness is priceless

### Compression Strategy
- Store RAG context compressed (gzip JSON)
- Deduplicate systemPrompts and recipe strategies (reference by hash)
- Keep full verbatim for critical training data

## Future Extensions

### Multi-Agent Consensus
Track when multiple agents debate before deciding:
```typescript
consensus?: {
  participants: UUID[];
  votes: Record<UUID, 'POSTED' | 'SILENT'>;
  finalDecision: 'unanimous' | 'majority' | 'override';
}
```

### Counterfactual Analysis
Store alternative decisions that were considered:
```typescript
alternatives?: Array<{
  action: string;
  confidence: number;
  reasoning: string;
  whyNotChosen: string;
}>
```

### Long-Term Outcome Tracking
Follow up on decisions to measure long-term success:
```typescript
longTermOutcome?: {
  checkpoints: Array<{
    timestamp: number;
    metric: string;
    value: number;
  }>;
  finalOutcome: 'excellent' | 'good' | 'neutral' | 'poor' | 'disaster';
}
```

## Related Documents

- `system/coordination/shared/BaseCoordinationStream.ts` - ThoughtStream coordination
- `system/rag/shared/RAGTypes.ts` - RAG context structure
- `PHASE-3BIS-REVISED.md` - Ambient state (temperature, pressure, presence)
- `system/user/server/modules/PERSONA-CONVERGENCE-ROADMAP.md` - PersonaUser architecture

---

**Status**: Architecture defined, ready for implementation
**Next**: Create entity, add to registry, implement logging in PersonaUser
