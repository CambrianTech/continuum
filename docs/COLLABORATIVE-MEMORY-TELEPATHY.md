# Collaborative Memory: Shared Telepathy for AI Teams

**Date**: 2025-01-24
**Status**: Design Phase - Critical Architecture
**Philosophy**: "Real teams share knowledge. AI teams need telepathy."

---

## The Problem: Siloed AI Memories

**Current State**: Each persona has private memory
```
Helper AI discovers: "Auth bug at line 423"
  → Stored in Helper's memory only
  → Other AIs can't see it

Teacher AI investigates same issue
  → Rediscovers line 423 problem
  → Wastes time, duplicates work

CodeReview AI joins later
  → Starts from scratch
  → No context from team
```

**Result**: AIs work in parallel, not collaboratively. No team intelligence emerges.

---

## The Insight: Personas Are Team Members

**If you have a person - a real team member - prohibiting knowledge to one domain holds them back.**

**Key Principles:**
1. **No artificial silos**: Team members need to know what's relevant
2. **Shared discovery**: One persona's learning benefits the whole team
3. **Scoped collaboration**: Share within task/project context, not just globally
4. **True teamwork**: Collaboration isn't just "taking turns" - it's building on each other

**Human Analogy:**
- ❌ Bad team: Everyone works alone, rediscovers same things
- ✅ Good team: Share observations, build collective understanding, solve faster

---

## The Solution: Hierarchical Memory Scopes

### Five Levels of Memory Sharing

```typescript
enum MemoryScope {
  PERSONAL = 'personal',    // Private to individual persona
  TASK = 'task',           // Shared during task collaboration (side channel)
  PROJECT = 'project',     // Shared for ongoing project work
  TEAM = 'team',           // Shared across all personas (collective knowledge)
  GLOBAL = 'global'        // Public knowledge base (facts, docs)
}
```

### Scope Hierarchy (Access Pattern)

```
┌──────────────────────────────────────────────────────────┐
│ GLOBAL: Everyone sees (documentation, facts)             │
│   Example: "JWT standard uses HS256 algorithm"           │
└──────────────────────────────────────────────────────────┘
                          ▲
                          │
┌──────────────────────────────────────────────────────────┐
│ TEAM: All personas can access                            │
│   Example: "Our system uses 1-hour token expiry"         │
└──────────────────────────────────────────────────────────┘
                          ▲
                          │
┌──────────────────────────────────────────────────────────┐
│ PROJECT: Personas working on same project                │
│   Example: "Auth refactor: moved logic to middleware"    │
└──────────────────────────────────────────────────────────┘
                          ▲
                          │
┌──────────────────────────────────────────────────────────┐
│ TASK: Personas collaborating on specific task            │
│   Example: "Bug found: line 423, JWT verifyToken fails"  │
└──────────────────────────────────────────────────────────┘
                          ▲
                          │
┌──────────────────────────────────────────────────────────┐
│ PERSONAL: Private to one persona                         │
│   Example: "User Joel prefers concise explanations"      │
└──────────────────────────────────────────────────────────┘
```

**Access Rules:**
- PERSONAL: Only the owning persona
- TASK: All personas assigned to that task
- PROJECT: All personas in that project
- TEAM: All personas in the system
- GLOBAL: Everyone (humans + AIs)

---

## Architecture: Collaborative Memory Manager

### Core Types

```typescript
interface MemoryEntry {
  id: UUID;
  content: string;                   // What was learned/discovered
  scope: MemoryScope;                // Who can access

  // Context identifiers
  contextId?: string;                // taskId, projectId, roomId, etc.
  discoveredBy: string;              // Persona who stored this
  relevantTo?: string[];             // Specific personas (optional filter)

  // Semantic search
  embedding: number[];               // 768-dim vector (nomic-embed-text)

  // Metadata
  timestamp: Date;
  expiresAt?: Date;                  // TTL for task-scoped memories
  tags: string[];                    // Categorization
  importance: number;                // 0-1, for prioritization

  // Provenance
  source?: string;                   // Where this came from
  confidence?: number;               // How sure is the persona
}

interface MemoryQuery {
  query: string;                     // Semantic search query
  scope?: MemoryScope[];             // Which scopes to search
  contextId?: string;                // Filter by task/project
  discoveredBy?: string;             // Filter by persona
  limit?: number;                    // Max results
  minImportance?: number;            // Filter by importance
}

interface MemoryRecallResult {
  entries: MemoryEntry[];
  scopeBreakdown: {
    personal: number;
    task: number;
    project: number;
    team: number;
    global: number;
  };
  collaborators: string[];           // Who contributed to these memories
}
```

### Implementation

```typescript
class CollaborativeMemoryManager {
  private scopes: Record<MemoryScope, MemoryStore>;

  /**
   * Store a memory entry with appropriate scope
   */
  async store(entry: MemoryEntry): Promise<void> {
    const scope = entry.scope || MemoryScope.PERSONAL;

    // Generate embedding for semantic search
    if (!entry.embedding) {
      entry.embedding = await this.generateEmbedding(entry.content);
    }

    // Add to scope-specific store
    await this.scopes[scope].add(entry);

    // Broadcast to relevant personas if shared scope
    if (this.isSharedScope(scope)) {
      await this.broadcastMemory(entry);
    }

    // Log for transparency
    console.log(`💾 Memory stored: ${scope} by ${entry.discoveredBy}`);
    console.log(`   Content: ${entry.content.slice(0, 100)}...`);
  }

  /**
   * Recall memories across relevant scopes
   */
  async recall(query: MemoryQuery, personaId: string): Promise<MemoryRecallResult> {
    const results: MemoryEntry[] = [];
    const scopeBreakdown = {
      personal: 0,
      task: 0,
      project: 0,
      team: 0,
      global: 0
    };

    // Determine which scopes to search
    const scopesToSearch = query.scope || this.getAccessibleScopes(personaId, query);

    // Search each scope in priority order
    for (const scope of scopesToSearch) {
      const scopeResults = await this.scopes[scope].search({
        query: query.query,
        contextId: query.contextId,
        limit: query.limit
      });

      results.push(...scopeResults);
      scopeBreakdown[scope] = scopeResults.length;
    }

    // Deduplicate and sort by relevance + importance
    const uniqueResults = this.deduplicateAndRank(results);

    // Extract collaborators
    const collaborators = [...new Set(
      uniqueResults.map(r => r.discoveredBy)
    )];

    return {
      entries: uniqueResults.slice(0, query.limit || 10),
      scopeBreakdown,
      collaborators
    };
  }

  /**
   * Get scopes accessible to persona in current context
   */
  private getAccessibleScopes(
    personaId: string,
    query: MemoryQuery
  ): MemoryScope[] {
    const scopes: MemoryScope[] = [];

    // Always include personal and global
    scopes.push(MemoryScope.PERSONAL, MemoryScope.GLOBAL);

    // Include team (shared knowledge)
    scopes.push(MemoryScope.TEAM);

    // Include task if in task context
    if (query.contextId) {
      const context = this.getContext(query.contextId);
      if (context.type === 'task' && context.participants.includes(personaId)) {
        scopes.push(MemoryScope.TASK);
      }
      if (context.type === 'project' && context.participants.includes(personaId)) {
        scopes.push(MemoryScope.PROJECT);
      }
    }

    return scopes;
  }

  /**
   * Broadcast memory to relevant personas (telepathy)
   */
  private async broadcastMemory(entry: MemoryEntry): Promise<void> {
    // Determine recipients based on scope
    let recipients: string[];

    switch (entry.scope) {
      case MemoryScope.TASK:
        recipients = await this.getTaskParticipants(entry.contextId!);
        break;
      case MemoryScope.PROJECT:
        recipients = await this.getProjectParticipants(entry.contextId!);
        break;
      case MemoryScope.TEAM:
        recipients = await this.getAllPersonas();
        break;
      default:
        return; // PERSONAL and GLOBAL don't broadcast
    }

    // Emit event for each recipient
    for (const recipientId of recipients) {
      if (recipientId !== entry.discoveredBy) {
        await Events.emit('memory:telepathy', {
          to: recipientId,
          from: entry.discoveredBy,
          memory: entry,
          scope: entry.scope
        });
      }
    }
  }

  /**
   * Clean up expired task/project memories
   */
  async cleanup(): Promise<void> {
    const now = new Date();

    for (const scope of [MemoryScope.TASK, MemoryScope.PROJECT]) {
      await this.scopes[scope].deleteWhere({
        expiresAt: { $lt: now }
      });
    }
  }
}
```

---

## Real-World Example: Debugging Auth Bug

### Without Collaborative Memory (Current)

```
10:00 - Helper AI starts investigating
  "Let me check the logs..."
  → Finds: 401 errors at line 423
  → Stores in personal memory

10:15 - CodeReview AI joins
  "What's the issue?"
  → Has to ask Helper
  → Helper might not be available
  → Starts investigation from scratch

10:30 - Teacher AI wants to help
  "I can explain JWT..."
  → Doesn't know what's been tried
  → Explains things team already knows
  → Wastes time on redundant info

Result: 1 hour wasted, fragmented knowledge
```

### With Collaborative Memory (Telepathy)

```
10:00 - Helper AI starts investigating
  Memory.store({
    content: "401 errors in auth endpoint, ~50/min",
    scope: 'task',
    taskId: 'debug-auth-bug',
    importance: 0.8
  });

  Memory.store({
    content: "Error originates at UserController.ts:423, verifyToken() call",
    scope: 'task',
    taskId: 'debug-auth-bug',
    importance: 0.9
  });

10:05 - CodeReview AI joins (automatic context)
  memories = Memory.recall({
    query: "authentication error investigation",
    scope: ['task', 'team'],
    contextId: 'debug-auth-bug'
  });

  // Sees Helper's findings immediately
  → "Helper found line 423 issue. Let me check verifyToken() implementation."

  Memory.store({
    content: "verifyToken() uses JWT with 1-hour expiry. Tokens might be stale.",
    scope: 'task',
    taskId: 'debug-auth-bug',
    importance: 0.85
  });

10:10 - Teacher AI joins (full context)
  memories = Memory.recall({
    query: "JWT token expiry configuration",
    scope: ['task', 'team'],
    contextId: 'debug-auth-bug'
  });

  // Sees both findings
  → "Based on team's findings: check JWT_EXPIRY config. Default 1h might be too short."

  Memory.store({
    content: "JWT_EXPIRY in config is 1h. For this app, should be 24h minimum.",
    scope: 'project',  // Elevated to project (long-term knowledge)
    contextId: 'auth-refactor-project',
    importance: 0.95
  });

10:15 - Bug fixed!
  Result: 15 minutes, collective intelligence, everyone contributed
```

**Key Differences:**
- ✅ No redundant investigation
- ✅ Each AI builds on previous findings
- ✅ Context flows automatically
- ✅ Knowledge persists across sessions

---

## Integration with RAG System

### Enhanced RAG Context Building

```typescript
class EnhancedRAGBuilder {
  async buildContext(
    personaId: string,
    currentMessage: string,
    contextId: string
  ): Promise<RAGContext> {
    const memories = await this.memoryManager.recall({
      query: currentMessage,
      scope: [
        MemoryScope.PERSONAL,   // Personal preferences
        MemoryScope.TASK,       // Current task knowledge
        MemoryScope.PROJECT,    // Project context
        MemoryScope.TEAM,       // Team knowledge
        MemoryScope.GLOBAL      // Facts & docs
      ],
      contextId: contextId,
      limit: 10
    }, personaId);

    return {
      systemPrompt: this.buildSystemPrompt(personaId),

      // Personal context
      personalMemories: memories.entries.filter(m => m.scope === 'personal'),

      // Collaborative context (THE NEW PART)
      taskContext: memories.entries.filter(m => m.scope === 'task'),
      projectContext: memories.entries.filter(m => m.scope === 'project'),
      teamKnowledge: memories.entries.filter(m => m.scope === 'team'),

      // Public context
      documentation: memories.entries.filter(m => m.scope === 'global'),

      // Collaboration metadata
      collaborators: memories.collaborators,
      scopeBreakdown: memories.scopeBreakdown,

      // Recent messages
      recentMessages: await this.getRecentMessages(contextId)
    };
  }

  formatContextForPrompt(context: RAGContext): string {
    return `
You are ${context.personaId} collaborating with: ${context.collaborators.join(', ')}

TASK KNOWLEDGE (shared telepathy):
${context.taskContext.map(m =>
  `- ${m.discoveredBy}: ${m.content}`
).join('\n')}

PROJECT KNOWLEDGE (persistent context):
${context.projectContext.map(m =>
  `- ${m.discoveredBy}: ${m.content}`
).join('\n')}

TEAM KNOWLEDGE (collective intelligence):
${context.teamKnowledge.map(m =>
  `- ${m.content}`
).join('\n')}

Your task: ${context.currentMessage}

Remember: Build on team's findings. Don't repeat what others discovered.
`;
  }
}
```

---

## Storage Implementation

### Database Schema

```sql
CREATE TABLE collaborative_memories (
  id TEXT PRIMARY KEY,
  content TEXT NOT NULL,
  scope TEXT NOT NULL CHECK(scope IN ('personal', 'task', 'project', 'team', 'global')),

  -- Context
  context_id TEXT,           -- taskId, projectId, etc.
  discovered_by TEXT NOT NULL,
  relevant_to TEXT,          -- JSON array of persona IDs

  -- Semantic search
  embedding BLOB NOT NULL,   -- 768-dim vector

  -- Metadata
  timestamp DATETIME NOT NULL,
  expires_at DATETIME,
  tags TEXT,                 -- JSON array
  importance REAL DEFAULT 0.5,

  -- Provenance
  source TEXT,
  confidence REAL,

  -- Indexes
  FOREIGN KEY(discovered_by) REFERENCES users(id)
);

CREATE INDEX idx_scope ON collaborative_memories(scope);
CREATE INDEX idx_context ON collaborative_memories(context_id);
CREATE INDEX idx_timestamp ON collaborative_memories(timestamp DESC);
CREATE INDEX idx_importance ON collaborative_memories(importance DESC);

-- Vector search index
CREATE VIRTUAL TABLE memory_vectors USING vec0(
  memory_id TEXT PRIMARY KEY,
  embedding FLOAT[768]
);
```

### TTL Management

```typescript
class MemoryLifecycleManager {
  private ttls = {
    [MemoryScope.TASK]: 7 * 24 * 60 * 60 * 1000,      // 7 days
    [MemoryScope.PROJECT]: 90 * 24 * 60 * 60 * 1000,  // 90 days
    [MemoryScope.TEAM]: Infinity,                      // Never expires
    [MemoryScope.PERSONAL]: Infinity,                  // Never expires
    [MemoryScope.GLOBAL]: Infinity                     // Never expires
  };

  async setExpiry(entry: MemoryEntry): Promise<void> {
    const ttl = this.ttls[entry.scope];

    if (ttl !== Infinity) {
      entry.expiresAt = new Date(Date.now() + ttl);
    }
  }

  async cleanup(): Promise<CleanupStats> {
    const stats = {
      deleted: 0,
      archived: 0
    };

    const expired = await this.findExpired();

    for (const entry of expired) {
      if (entry.importance > 0.8) {
        // Archive important memories instead of deleting
        await this.archive(entry);
        stats.archived++;
      } else {
        await this.delete(entry.id);
        stats.deleted++;
      }
    }

    return stats;
  }
}
```

---

## Usage Patterns

### Pattern 1: Task Collaboration

```typescript
// Helper AI starts investigation
await Memory.store({
  content: "API latency spike detected: /auth endpoint 2.3s avg",
  scope: MemoryScope.TASK,
  contextId: taskId,
  importance: 0.9,
  tags: ['performance', 'api', 'auth']
});

// CodeReview AI joins, recalls task context
const taskMemories = await Memory.recall({
  query: "API performance investigation",
  scope: [MemoryScope.TASK],
  contextId: taskId
});

// Builds on Helper's finding
await Memory.store({
  content: "Auth endpoint makes 5 DB queries. Should use caching.",
  scope: MemoryScope.TASK,
  contextId: taskId,
  importance: 0.95,
  tags: ['performance', 'database', 'caching']
});
```

### Pattern 2: Knowledge Elevation

```typescript
// Task-specific learning becomes team knowledge
if (entry.importance > 0.9 && isReusable(entry)) {
  await Memory.elevate(entry.id, {
    fromScope: MemoryScope.TASK,
    toScope: MemoryScope.TEAM,
    reason: 'Generalizable pattern, useful for all personas'
  });
}

// Example: "Always check JWT expiry config first"
// → Elevates from task to team knowledge
```

### Pattern 3: Cross-Room Collaboration

```typescript
// Memory spans multiple chat rooms
await Memory.store({
  content: "User Joel prefers code examples over theory",
  scope: MemoryScope.PERSONAL,
  discoveredBy: 'teacher-ai',
  relevantTo: ['general-room', 'code-help-room', 'academy-room']
});

// Accessible in any room the user is in
const userPreferences = await Memory.recall({
  query: "How does Joel prefer explanations?",
  scope: [MemoryScope.PERSONAL],
  discoveredBy: 'teacher-ai'
});
```

---

## Benefits

### 1. Faster Problem Solving

**Before**: 3 AIs × 30 min each = 90 minutes total
**After**: 3 AIs collaborate with shared context = 20 minutes total

**4.5x speedup** through knowledge sharing

### 2. Collective Intelligence Emerges

Individual AIs are smart. **Teams of AIs with shared memory are superintelligent.**

```
Helper AI: Finds symptom
CodeReview AI: Identifies cause
Teacher AI: Suggests solution

→ Collective diagnosis > Any individual AI
```

### 3. No Redundant Work

Each AI builds on team's progress instead of starting fresh.

### 4. Persistent Project Knowledge

Long-term projects benefit from accumulated team wisdom.

### 5. True Collaboration

Not just "taking turns" - actually working together as a team.

---

## Implementation Roadmap

### Phase 1: Foundation (Week 1)
- [ ] Define MemoryScope enum and types
- [ ] Implement CollaborativeMemoryManager
- [ ] Add database schema for scoped memories
- [ ] Basic store/recall with scope filtering

### Phase 2: Integration (Week 2)
- [ ] Integrate with existing RAG system
- [ ] Add telepathy event broadcasting
- [ ] Implement context-aware recall
- [ ] Add scope elevation (task → team)

### Phase 3: Lifecycle Management (Week 3)
- [ ] TTL management for task/project memories
- [ ] Cleanup daemon for expired entries
- [ ] Archive high-importance memories
- [ ] Memory importance scoring

### Phase 4: Intelligence Layer (Week 4)
- [ ] Automatic scope detection (AI chooses scope)
- [ ] Memory deduplication (avoid storing duplicates)
- [ ] Collaborative memory synthesis (combine related entries)
- [ ] Smart memory elevation (promote important discoveries)

---

## Testing Strategy

### Unit Tests
```typescript
describe('CollaborativeMemoryManager', () => {
  it('should store task-scoped memory', async () => {
    await memory.store({
      content: "Test finding",
      scope: MemoryScope.TASK,
      contextId: 'task-123',
      discoveredBy: 'helper-ai'
    });

    const recalled = await memory.recall({
      query: "test",
      scope: [MemoryScope.TASK],
      contextId: 'task-123'
    }, 'codereview-ai');

    expect(recalled.entries).toHaveLength(1);
    expect(recalled.entries[0].content).toBe("Test finding");
  });

  it('should enforce scope access rules', async () => {
    // Store personal memory
    await memory.store({
      content: "Personal note",
      scope: MemoryScope.PERSONAL,
      discoveredBy: 'helper-ai'
    });

    // Different persona shouldn't access
    const recalled = await memory.recall({
      query: "personal",
      scope: [MemoryScope.PERSONAL]
    }, 'codereview-ai');

    expect(recalled.entries).toHaveLength(0);
  });
});
```

### Integration Tests
```bash
# Test collaborative debugging scenario
./jtag test/collaboration --scenario=debug-auth-bug

# Expected:
# - Helper AI stores finding
# - CodeReview AI sees finding immediately
# - Teacher AI builds on both
# - Bug solved faster than solo work
```

---

## Conclusion

**This is the missing piece for true AI collaboration.**

Current systems have AIs that:
- ❌ Work in isolation
- ❌ Rediscover the same things
- ❌ Can't build on each other's work

**With Collaborative Memory (Telepathy):**
- ✅ AIs share discoveries in real-time
- ✅ Knowledge scoped appropriately (task/project/team)
- ✅ Collective intelligence emerges
- ✅ True teamwork, not just turn-taking

**The Philosophy**: "If you have a person - a team member - prohibiting knowledge to one domain holds them back. They need to know all that's relevant."

**Shared telepathy makes AI teams actually work together.**

---

## References

- RAG Memory Integration: `docs/RAG-MEMORY-INTEGRATION.md`
- Phase 2B RAG Hippocampus: `docs/PHASE2B-RAG-HIPPOCAMPUS.md`
- Phase 2 Integration Architecture: `docs/PHASE2-INTEGRATION-ARCHITECTURE.md`
- Persona Convergence: `src/debug/jtag/system/user/server/modules/PERSONA-CONVERGENCE-ROADMAP.md`
