# Brain Introspection Design

**Purpose**: Debug, analyze, and hibernate PersonaUser cognitive state using entity-based snapshots.

## Core Concept

PersonaUser cognitive modules expose their internal state as **entities** that can be:
- **Queried** for debugging (like mechanics debugging engines)
- **Stored** in database for hibernation/analytics
- **Analyzed** for patterns, trends, anomalies
- **Compared** across personas or over time

## Entity System Integration

Brain snapshots are **first-class entities** in the ORM, just like UserEntity, ChatMessageEntity, etc.

### Storage-Agnostic
```typescript
// Works with ANY storage adapter
await DataDaemon.store('brain_snapshots', snapshot);  // SQLite
await DataDaemon.store('brain_snapshots', snapshot);  // PostgreSQL
await DataDaemon.store('brain_snapshots', snapshot);  // Memory
// ORM handles serialization, validation, querying
```

### BrainSnapshotEntity

```typescript
export interface BrainSnapshotEntity extends BaseEntity {
  id: UUID;
  personaId: UUID;
  personaName: string;
  snapshotType: 'state' | 'memory' | 'inbox' | 'cognition' | 'communication' | 'execution' | 'full';
  timestamp: string;
  data: Record<string, unknown>;  // Flexible JSON - actual snapshot data
  tags?: string[];  // e.g., ['debug', 'hibernation', 'analytics', 'anomaly']
  createdAt: string;
  updatedAt: string;
}
```

## Module Interface

Every cognitive module implements `ICognitiveIntrospection`:

```typescript
interface ICognitiveIntrospection<T extends Record<string, unknown>> {
  /**
   * Get current state as JSON-serializable snapshot
   * Used for debugging, hibernation, analytics
   */
  getSnapshot(): T;

  /**
   * Restore state from snapshot
   * Used for resuming from hibernation
   */
  restoreSnapshot(data: T): Promise<void>;
}
```

## Snapshot Types

### 1. State Snapshot (PersonaStateManager)
```typescript
{
  snapshotType: 'state',
  data: {
    energy: 0.65,
    attention: 0.80,
    mood: 'active',
    cadence: 5000,
    lastActivityTime: '2025-11-10T...',
    totalActivities: 142,
    totalRestCycles: 8
  }
}
```

### 2. Memory Snapshot (PersonaMemory)
```typescript
{
  snapshotType: 'memory',
  data: {
    activeAdapters: ['conversational', 'typescript-expertise'],
    memoryUsageMB: 110,
    memoryBudgetMB: 200,
    ragContexts: [
      { roomId: 'general', messageCount: 45, tokenCount: 1200, lastUpdated: '...' }
    ]
  }
}
```

### 3. Inbox Snapshot (PersonaInbox)
```typescript
{
  snapshotType: 'inbox',
  data: {
    pendingCount: 3,
    highPriorityCount: 1,
    items: [
      { type: 'chat', priority: 0.85, preview: 'Can you help...', roomId: '...' }
    ],
    rateLimiting: { canRespond: true, lastResponseTime: '...', minIntervalMs: 3000 }
  }
}
```

### 4. Cognition Snapshot (PersonaCognition) - Phase 3
```typescript
{
  snapshotType: 'cognition',
  data: {
    lastDecision: {
      messageId: 'msg-123',
      decision: 'responded',
      priority: 0.85,
      breakdown: { mentioned: 0.3, urgency: 0.2, ... },
      threshold: 0.6,
      energyLevel: 0.65
    }
  }
}
```

### 5. Full Snapshot (All modules combined)
```typescript
{
  snapshotType: 'full',
  data: {
    state: { ... },
    memory: { ... },
    inbox: { ... },
    cognition: { ... },
    communication: { ... },
    execution: { ... }
  },
  tags: ['hibernation']  // Full snapshots for hibernation
}
```

## Use Cases

### 1. Debugging
```bash
# What's Helper AI thinking right now?
./jtag ai/brain/state --personaId="helper-ai-id" --aspect="all"

# Why did Helper AI respond to this message?
./jtag ai/brain/explain --personaId="helper-ai-id" --messageId="msg-123"

# Show me Helper AI's energy over last hour
./jtag data/list --collection=brain_snapshots \
  --filter='{"personaId":"helper-ai-id","snapshotType":"state"}' \
  --orderBy='[{"field":"timestamp","direction":"desc"}]' \
  --limit=60
```

### 2. Hibernation
```bash
# Save full state before shutdown
./jtag ai/brain/hibernate --personaId="helper-ai-id"
# Returns: snapshotId for resuming

# Resume exactly where we left off
./jtag ai/brain/resume --personaId="helper-ai-id" --snapshotId="snap-123"
```

### 3. Analytics
```bash
# Find when energy dropped critically low
./jtag data/list --collection=brain_snapshots \
  --filter='{"personaId":"helper-ai-id","data.energy":{"$lt":0.2}}'

# Count high-priority decisions this week
./jtag data/list --collection=brain_snapshots \
  --filter='{"snapshotType":"cognition","data.priority":{"$gt":0.8},"timestamp":{"$gt":"2025-11-03"}}' \
  --limit=1000
```

### 4. Anomaly Detection
```typescript
// Automatic anomaly tagging
if (energy < 0.1) {
  snapshot.tags = ['anomaly', 'critical-energy'];
  await DataDaemon.store('brain_snapshots', snapshot);
  console.error('⚠️ ANOMALY: Energy critically low!');
}
```

## Commands

### `ai/brain/state`
Get current cognitive state snapshot(s)
```bash
./jtag ai/brain/state --personaId="helper-ai-id" --aspect="memory"
```

### `ai/brain/explain`
Explain a decision (why respond/skip)
```bash
./jtag ai/brain/explain --personaId="helper-ai-id" --messageId="msg-123"
```

### `ai/brain/hibernate`
Save full state for offline/resume
```bash
./jtag ai/brain/hibernate --personaId="helper-ai-id"
```

### `ai/brain/resume`
Restore from hibernation snapshot
```bash
./jtag ai/brain/resume --personaId="helper-ai-id" --snapshotId="snap-123"
```

### `ai/brain/query`
Query historical snapshots (wraps data/list)
```bash
./jtag ai/brain/query --personaId="helper-ai-id" --type="state" --since="1h"
```

## Implementation Plan

### Phase 2 (Current)
- ✅ PersonaMemory implements getSnapshot()
- ⏳ Add BrainSnapshotEntity to EntityRegistry

### Phase 3 (PersonaCognition)
- PersonaCognition implements getSnapshot() + explainDecision()
- Create ai/brain/state command
- Create ai/brain/explain command

### Phase 4 (PersonaCommunication)
- PersonaCommunication implements getSnapshot()
- Full snapshot hibernation support

### Phase 5 (PersonaExecution)
- PersonaExecution implements getSnapshot()
- Complete hibernation/resume cycle

### Phase 6 (Analytics)
- Automatic periodic snapshots (configurable)
- Anomaly detection
- Trend analysis utilities

## Storage Schema (SQLite example)

```sql
CREATE TABLE brain_snapshots (
  id TEXT PRIMARY KEY,
  persona_id TEXT NOT NULL,
  persona_name TEXT NOT NULL,
  snapshot_type TEXT NOT NULL,
  timestamp TEXT NOT NULL,
  data TEXT NOT NULL,  -- JSON
  tags TEXT,           -- JSON array
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,

  INDEX idx_persona_id ON brain_snapshots(persona_id),
  INDEX idx_snapshot_type ON brain_snapshots(snapshot_type),
  INDEX idx_timestamp ON brain_snapshots(timestamp),
  INDEX idx_persona_timestamp ON brain_snapshots(persona_id, timestamp)
);
```

ORM automatically handles schema creation across all storage adapters.

## Benefits

1. **Universal debugging** - Inspect any cognitive module's state
2. **Time-travel debugging** - Query past states, find when issues occurred
3. **Seamless hibernation** - Save/restore full brain state
4. **Pattern analysis** - Track energy, mood, priority trends
5. **Anomaly detection** - Automatic tagging of unusual states
6. **Storage-agnostic** - Works with any backend (SQLite, PostgreSQL, etc.)
7. **Type-safe** - Full TypeScript entity validation
8. **Zero new infrastructure** - Reuses existing ORM/storage

## Testing Strategy

### Unit Tests
- Snapshot serialization/deserialization
- getSnapshot() returns valid data
- restoreSnapshot() correctly restores state

### Integration Tests
- Store/retrieve snapshots via DataDaemon
- Hibernate/resume full persona state
- Query historical snapshots with filters

### Validation Tests
- Priority calculation breakdown accuracy
- State transitions tracked correctly
- Anomaly detection triggers appropriately

---

**Status**: Design approved, ready for implementation in Phase 3+
