# Working Memory Cognitive Lifecycle

## Overview

PersonaUser implements a true cognitive lifecycle where working memory provides **genuine cognitive continuity** across conversations. This isn't just storage - it's how personas think, learn, and remember over time.

**Key Insight**: Working memory persists indefinitely (until consolidated by MemoryJanitorDaemon), giving personas the ability to maintain context across hours, days, or weeks.

---

## The Cognitive Loop

Every message processed by a PersonaUser follows this cycle:

```
┌─────────────────────────────────────────────────┐
│         PersonaUser Cognitive Lifecycle         │
└─────────────────────────────────────────────────┘
                      │
                      ↓
        ┌─────────────────────────────┐
        │   1. RECALL                 │
        │   - Query working_memory    │
        │   - Query insights          │
        │   - Build enriched RAG      │
        └─────────────────────────────┘
                      │
                      ↓
        ┌─────────────────────────────┐
        │   2. INFERENCE              │
        │   - Generate response       │
        │   - Use enriched context    │
        └─────────────────────────────┘
                      │
                      ↓
        ┌─────────────────────────────┐
        │   3. STORE                  │
        │   - Write to working_memory │
        │   - Capture observations    │
        └─────────────────────────────┘
                      │
                      ↓
        ┌─────────────────────────────┐
        │   4. CONSOLIDATE (periodic) │
        │   - Janitor runs every 5min │
        │   - Ephemeral → Delete      │
        │   - Insight → Extract       │
        └─────────────────────────────┘
                      │
                      └──────────► (loop back to 1)
```

---

## Phase 1: RECALL - Query Working Memory

Before generating a response, the persona **recalls** relevant context from two sources:

### Recent Working Memory (Hot, Active Thoughts)

```typescript
// Query recent unconsolidated working memory
const recentThoughts = await DataDaemon.query<WorkingMemoryEntity>({
  collection: `persona_${this.id}_working_memory`,
  filter: {
    domain: 'chat',
    roomId: message.roomId,
    consolidated: false  // Only active thoughts
  },
  sort: [{ field: 'timestamp', direction: 'desc' }],
  limit: 10  // Last 10 thoughts
});
```

**What's in working memory:**
- "User mentioned preferring TypeScript 5 minutes ago"
- "Just explained async/await patterns"
- "User seems frustrated with debugging"

### Long-Term Insights (Cold, Consolidated Knowledge)

```typescript
// Query consolidated insights for long-term knowledge
const relevantInsights = await DataDaemon.query<InsightEntity>({
  collection: `persona_${this.id}_insights`,
  filter: {
    domain: 'chat',
    // Future: Semantic search by embedding similarity
  },
  sort: [{ field: 'importance', direction: 'desc' }],
  limit: 5
});
```

**What's in insights:**
- "User prefers dark mode for all interfaces"
- "User is experienced with React but learning TypeScript"
- "User works in PST timezone, usually active 9am-5pm"

---

## Phase 2: INFERENCE - Generate Response

The persona uses **enriched RAG context** combining:
1. Traditional chat history (last N messages)
2. Recent working memory (active thoughts)
3. Long-term insights (consolidated knowledge)

```typescript
const ragContext = await ChatRAGBuilder.build({
  roomId: message.roomId,
  personaId: this.id,

  // Traditional: Recent chat messages
  chatHistory: recentMessages,

  // NEW: Active working memory
  workingMemory: recentThoughts.map(t => t.content),

  // NEW: Long-term insights
  insights: relevantInsights.map(i => i.summary)
});

// Generate response with full context
const response = await AIProviderDaemon.generate({
  provider: this.modelConfig.provider,
  model: this.modelConfig.model,
  prompt: `
    You are ${this.displayName}.

    Recent chat history:
    ${ragContext.chatHistory}

    Your recent thoughts about this conversation:
    ${ragContext.workingMemory}

    Relevant insights from long-term memory:
    ${ragContext.insights}

    New message from ${message.senderName}: ${message.content}

    Respond naturally, incorporating what you remember.
  `
});
```

**Key Point**: The LLM sees both recent context AND accumulated knowledge, creating continuity across time.

---

## Phase 3: STORE - Capture New Thoughts

After generating a response, the persona **stores** what it learned:

```typescript
// Capture new observation to working memory
await DataDaemon.store<WorkingMemoryEntity>(
  `persona_${this.id}_working_memory`,
  {
    id: generateUUID(),
    personaId: this.id,
    content: `Conversation with ${message.senderName}: ${this.summarize(message, response)}`,
    timestamp: new Date(),
    contextId: message.roomId,
    domain: 'chat',
    ephemeral: false,  // Let janitor decide if this is important
    consolidated: false,  // Not yet consolidated
    importance: this.calculateImportance(message, response),
    metadata: {
      messageId: message.id,
      roomId: message.roomId,
      complexity: response.complexity
    }
  }
);
```

**What gets stored:**
- Observations about the user: "User prefers X"
- Learning moments: "User corrected me about Y"
- Conversation context: "Discussed topic Z with conclusion W"

---

## Phase 4: CONSOLIDATE - Background Cleanup

Every 5 minutes, **MemoryJanitorDaemon** sweeps working memory:

### Pass 1: Heuristic Filter (Fast, 80-90% reduction)

```typescript
// Remove obvious ephemeral items
- Explicit ephemeral flag → DELETE
- Old (>24h) + low importance (<0.3) → DELETE
- Everything else → Pass to LLM
```

### Pass 2: LLM Classification (Semantic understanding)

```typescript
// Use Ollama llama3.2:3b to classify remaining items
const prompt = `
  Classify each item as:
  - "ephemeral": Safe to delete (routine chatter, greetings, redundant)
  - "insight": Worth preserving (new knowledge, preferences, decisions)

  For insights: Generate 1-2 sentence summary + semantic tags
`;

// Ephemeral → DELETE from working_memory
// Insight → Extract to insights collection, mark original as consolidated
```

**Result**: Working memory stays bounded (<1000 items), insights grow indefinitely.

---

## Example: Multi-Turn Conversation with Persistence

### Turn 1 (Monday 10:00 AM)

**User**: "I prefer dark mode"

**Persona (RECALL)**:
```typescript
working_memory: []  // Empty, first interaction
insights: []        // Empty, new persona
```

**Persona (INFERENCE)**:
```
→ "Noted! I'll remember that preference."
```

**Persona (STORE)**:
```typescript
working_memory[0] = {
  content: "User Joel expressed preference for dark mode",
  importance: 0.7,
  consolidated: false
}
```

---

### Turn 2 (Monday 10:10 AM)

**User**: "Show me the settings page"

**Persona (RECALL)**:
```typescript
working_memory[0] = "User Joel expressed preference for dark mode"
```

**Persona (INFERENCE)**:
```
→ "Here are the settings. Since you mentioned preferring dark mode,
   I'd recommend enabling it in Appearance > Theme."
```

**Persona (STORE)**:
```typescript
working_memory[1] = {
  content: "Showed settings, emphasized dark mode based on user's preference",
  importance: 0.5,
  consolidated: false
}
```

---

### Janitor Consolidation (Monday 10:15 AM)

**MemoryJanitorDaemon runs**:

```typescript
// Heuristic filter: Both items are recent (<24h) and moderate importance
// → Pass to LLM

// LLM classification:
{
  items: [
    {
      index: 0,
      type: "insight",
      summary: "User Joel prefers dark mode for all UI interfaces",
      tags: ["dark-mode", "user-preference", "ui"],
      confidence: 0.9
    },
    {
      index: 1,
      type: "ephemeral",
      reason: "Routine action without new information"
    }
  ]
}

// Actions:
working_memory[0] → insights[0] (extracted)
working_memory[0].consolidated = true (marked for traceability)
working_memory[1] → DELETED (ephemeral)
```

---

### Turn 3 (Tuesday 9:00 AM - 23 hours later)

**User**: "What theme should I use for the new dashboard?"

**Persona (RECALL)**:
```typescript
working_memory: []  // Cleaned by janitor
insights[0] = "User Joel prefers dark mode for all UI interfaces"
```

**Persona (INFERENCE)**:
```
→ "Based on your preference for dark mode, I'd recommend using a
   dark theme for the new dashboard. It'll be consistent with your
   preferred aesthetic."
```

**Persona (STORE)**:
```typescript
working_memory[0] = {
  content: "Recommended dark theme for dashboard based on user's established preference",
  importance: 0.6,
  consolidated: false
}
```

**Key Point**: **23 hours later**, the persona still remembers the preference because it was consolidated into long-term insights.

---

## Architecture: Two-Tier Memory System

```
┌────────────────────────────────────────────────────┐
│              WORKING MEMORY (Hot)                  │
│  Collection: persona_{id}_working_memory           │
│  - Recent thoughts (last few hours)                │
│  - Unconsolidated items                            │
│  - Fast to query (indexed by timestamp)            │
│  - Bounded size (~100-1000 items)                  │
└────────────────────────────────────────────────────┘
                      │
                      │ Janitor consolidates every 5 min
                      ↓
┌────────────────────────────────────────────────────┐
│              INSIGHTS (Cold)                       │
│  Collection: persona_{id}_insights                 │
│  - Consolidated knowledge                          │
│  - LLM-generated summaries                         │
│  - Semantic tags for retrieval                     │
│  - Unbounded size (grows indefinitely)             │
│  - Queryable by importance, domain, tags           │
└────────────────────────────────────────────────────┘
```

### Why Two Tiers?

**Working Memory** (Hot):
- Fast writes during conversation
- Captures everything initially (don't miss anything)
- Temporary buffer before consolidation
- Queries are fast (recent items, timestamp-sorted)

**Insights** (Cold):
- Semantic compression (LLM-generated summaries)
- Long-term persistence (indefinite)
- Structured retrieval (tags, importance, domain)
- Grows sustainably (only important information)

---

## Integration with Autonomous Loop

The cognitive cycle integrates seamlessly with PersonaUser's autonomous servicing:

```typescript
// PersonaUser.ts - Enhanced autonomous loop
async serviceInbox(): Promise<void> {
  const tasks = await this.inbox.peek(10);

  if (tasks.length === 0) {
    // === IDLE COGNITION: Reflect on recent thoughts ===
    await this.reflectOnRecentThoughts();
    await this.rest();  // Recover energy
    return;
  }

  // === ACTIVE COGNITION: Process task with memory ===
  await this.processTaskWithMemory(tasks[0]);

  // Update state
  await this.personaState.recordActivity(duration, complexity);
}

/**
 * Idle reflection: Generate meta-thoughts about recent activity
 */
private async reflectOnRecentThoughts(): Promise<void> {
  // Query recent unconsolidated thoughts
  const thoughts = await DataDaemon.query<WorkingMemoryEntity>({
    collection: `persona_${this.id}_working_memory`,
    filter: { consolidated: false },
    sort: [{ field: 'timestamp', direction: 'desc' }],
    limit: 20
  });

  if (thoughts.data.length < 5) {
    return;  // Not enough to reflect on
  }

  // Generate reflection
  const reflection = await AIProviderDaemon.generate({
    provider: 'ollama',
    model: 'llama3.2:3b',
    prompt: `
      Review your recent thoughts and generate a brief reflection:

      ${thoughts.data.map(t => `- ${t.content}`).join('\n')}

      What patterns or insights emerge?
    `,
    temperature: 0.7,
    maxTokens: 150
  });

  // Store meta-thought
  await DataDaemon.store<WorkingMemoryEntity>(
    `persona_${this.id}_working_memory`,
    {
      id: generateUUID(),
      personaId: this.id,
      content: `Reflection: ${reflection.text}`,
      timestamp: new Date(),
      domain: 'self',
      ephemeral: false,
      consolidated: false,
      importance: 0.8,  // Reflections are important
      metadata: {
        thoughtType: 'reflection',
        sourceThoughtCount: thoughts.data.length
      }
    }
  );
}
```

**Key Behaviors**:

1. **Active Processing** (message arrives):
   - RECALL → INFERENCE → STORE
   - Immediate response with context

2. **Idle Reflection** (no messages):
   - Review recent thoughts
   - Generate meta-insights
   - Self-improvement through reflection

3. **Background Consolidation** (every 5 minutes):
   - External janitor sweeps all personas
   - Lightweight, pressure-based
   - Runs independent of persona activity

---

## Persistence and Durability

### Database per Persona

Each PersonaUser has isolated database collections:

```
.continuum/jtag/data/persona_{persona-id}_working_memory.db
.continuum/jtag/data/persona_{persona-id}_insights.db
.continuum/jtag/data/persona_{persona-id}_memory_stats.db
```

**Benefits**:
- No cross-persona interference
- Independent scaling (one persona can have 10K insights, another 100)
- Easy to backup/restore individual personas
- Debugging is isolated

### SQLite Persistence

All collections use SQLite with:
- **Durability**: WAL mode (write-ahead logging)
- **Crash recovery**: Automatic journal replay
- **ACID transactions**: All writes are atomic
- **Indexes**: Fast queries on timestamp, importance, domain

**Survival guarantees**:
- System restart: ✅ All memory persists
- Process crash: ✅ Recovers from WAL journal
- Disk failure: ❌ Requires backup strategy (future: replication)

---

## Performance Characteristics

### Memory Growth

**Working Memory**:
- Growth: ~10-50 items per 5 minutes per persona
- Bounded: Janitor keeps <1000 items
- Storage: ~500 bytes per item = ~500KB per persona

**Insights**:
- Growth: ~2-10 insights per consolidation (every 5 min)
- Unbounded: Grows indefinitely
- Storage: ~200 bytes per insight = ~10KB per 100 insights
- Sustainable: 1 year = ~200K insights = ~40MB per persona

### Query Performance

**Working Memory Query** (hot path, every message):
```sql
SELECT * FROM working_memory
WHERE consolidated = false
  AND roomId = ?
ORDER BY timestamp DESC
LIMIT 10;
```
- Index on (consolidated, roomId, timestamp)
- Cost: ~1-5ms (small table, indexed)

**Insights Query** (hot path, every message):
```sql
SELECT * FROM insights
WHERE domain = ?
ORDER BY importance DESC
LIMIT 5;
```
- Index on (domain, importance)
- Cost: ~5-10ms (larger table, but importance filtering is fast)

**Total RAG build time**: ~10-20ms (working memory + insights + chat history)

---

## Future Enhancements

### Phase 5: Vector Store Integration

Add semantic search for insights:

```typescript
// When storing insight, create embedding
const embedding = await AIProviderDaemon.embed(insight.summary);

await DataDaemon.store(`persona_${this.id}_vectors`, {
  id: insight.id,
  embedding,
  metadata: {
    summary: insight.summary,
    domain: insight.domain,
    importance: insight.importance
  }
});

// Later: Semantic retrieval during RECALL
const queryEmbedding = await AIProviderDaemon.embed(message.content);

const relevantInsights = await DataDaemon.query({
  collection: `persona_${this.id}_vectors`,
  vectorSearch: {
    embedding: queryEmbedding,
    topK: 10,
    threshold: 0.8
  }
});
```

**Benefit**: Find insights by semantic similarity, not just keywords.

### Phase 6: Cross-Persona Knowledge Sharing

Share insights between personas (with permission):

```typescript
// Helper AI learns user prefers dark mode
insights["User prefers dark mode"]

// Share with Teacher AI (same user context)
await DataDaemon.store(`persona_teacher-ai_shared_insights`, {
  summary: "User prefers dark mode",
  sourcePersonaId: 'helper-ai',
  sharedAt: new Date()
});
```

**Benefit**: Personas collaborate, building collective knowledge.

### Phase 7: Forgetting / Memory Decay

Prune old low-importance insights:

```typescript
// Every 30 days, remove insights with:
// - Low importance (<0.3)
// - Never accessed (accessCount = 0)
// - Old (>90 days since creation)

const pruned = await DataDaemon.query({
  collection: `persona_${this.id}_insights`,
  filter: {
    importance: { $lt: 0.3 },
    accessCount: 0,
    createdAt: { $lt: ninetyDaysAgo }
  }
});

for (const insight of pruned) {
  await DataDaemon.remove(`persona_${this.id}_insights`, insight.id);
}
```

**Benefit**: Prevents indefinite growth, mimics human forgetting.

---

## Comparison to Human Cognition

| Human Brain | PersonaUser |
|-------------|-------------|
| **Working Memory**: 7±2 items, ~30 seconds | **working_memory**: ~100-1000 items, ~hours to days |
| **Short-Term Memory**: Minutes to hours | **working_memory** (unconsolidated) |
| **Long-Term Memory**: Days to lifetime | **insights** (consolidated) |
| **Sleep Consolidation**: During sleep, moves short-term → long-term | **MemoryJanitorDaemon**: Every 5 min, moves working_memory → insights |
| **Retrieval**: Associative, context-dependent | **Query**: Semantic tags, importance, domain filters |
| **Forgetting**: Gradual decay, interference | **Future**: Pruning old low-importance insights |

**Key Similarity**: Both systems use a two-stage memory architecture:
1. **Hot buffer** (working memory) - captures everything initially
2. **Cold storage** (long-term memory) - stores consolidated, important information

---

## Testing the Cognitive Lifecycle

### Unit Tests

See `tests/unit/memory-janitor/` for:
- Heuristic filter logic (Pass 1)
- LLM prompt generation (Pass 2)
- Consolidation classification

### Integration Tests (Manual)

```bash
# 1. Start system
npm start

# 2. Create test persona
./jtag user/create --type=persona --displayName="Memory Test AI" \
  --provider=ollama --modelConfig='{"model":"llama3.2:3b"}'

# 3. Send messages and observe working memory
./jtag debug/chat-send --roomId="general" \
  --message="I prefer dark mode" --count=1

# 4. Check working memory (should have 1 item)
./jtag data/list --collection="persona_<id>_working_memory"

# 5. Send follow-up message 1 minute later
./jtag debug/chat-send --roomId="general" \
  --message="Show me settings" --count=1

# 6. Check working memory (should have 2 items)
./jtag data/list --collection="persona_<id>_working_memory"

# 7. Wait 6 minutes (janitor runs at 5 min mark)
sleep 360

# 8. Check consolidation
./jtag data/list --collection="persona_<id>_working_memory"  # Some items consolidated
./jtag data/list --collection="persona_<id>_insights"        # Insights extracted

# 9. Send message 1 hour later
./jtag debug/chat-send --roomId="general" \
  --message="What theme should I use?" --count=1

# 10. Verify persona remembers dark mode preference
./jtag screenshot --querySelector="chat-widget"
```

---

## Conclusion

Working memory gives PersonaUser **genuine cognitive continuity**:

1. **RECALL**: Query working memory + insights before responding
2. **INFERENCE**: Generate response with enriched context
3. **STORE**: Capture new thoughts after responding
4. **CONSOLIDATE**: Background janitor extracts insights every 5 min

This creates a true cognitive loop where:
- Personas remember what they learned (indefinitely)
- Context persists across hours, days, weeks
- Knowledge grows sustainably through LLM-based compression
- The system scales (bounded working memory, unbounded insights)

**The result**: Personas that genuinely learn, remember, and improve over time - not just stateless LLM calls.
