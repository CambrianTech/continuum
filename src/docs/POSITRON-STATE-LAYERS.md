# Positron State Layers

> "Beat React at rendering, beat databases at persistence, beat RAG at perception"

---

## The Performance Challenge

**React's Advantage**: State changes are cheap. Just mutate JS, diff virtual DOM, done.

**Positron's Challenge**: If every state change writes to SQLite + generates embeddings + updates RAG indexes... we lose on performance.

**Solution**: State layers with different performance characteristics.

---

## The Four Layers

```
┌─────────────────────────────────────────────────────────────────┐
│                    POSITRON STATE LAYERS                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  LAYER 0: EPHEMERAL           │ 60fps OK, never persisted       │
│  ─────────────────────────────┼──────────────────────────────── │
│  hover state, animations,     │ Pure JS, React-like speed       │
│  drag position, scroll pos    │ No DB, no RAG, no AI perception │
│                               │                                  │
├───────────────────────────────┼──────────────────────────────────┤
│                                                                  │
│  LAYER 1: SESSION             │ Fast, memory-only               │
│  ─────────────────────────────┼──────────────────────────────── │
│  form inputs, temp selections │ In-memory Map/WeakMap           │
│  UI preferences, panel sizes  │ Lost on refresh (OK)            │
│                               │ AI can see if subscribed        │
│                                                                  │
├───────────────────────────────┼──────────────────────────────────┤
│                                                                  │
│  LAYER 2: PERSISTENT          │ Debounced writes, SQLite        │
│  ─────────────────────────────┼──────────────────────────────── │
│  user settings, open tabs,    │ 500ms debounce minimum          │
│  content state, preferences   │ Survives refresh/restart        │
│                               │ AI sees as entity changes       │
│                                                                  │
├───────────────────────────────┼──────────────────────────────────┤
│                                                                  │
│  LAYER 3: SEMANTIC            │ Compressed, RAG-indexed         │
│  ─────────────────────────────┼──────────────────────────────── │
│  user intent, session summary │ Semantic compression first      │
│  meaningful actions, learnings│ Embedding generation            │
│                               │ longterm.db for semantic search │
│                               │ AI primary perception layer     │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Layer 0: Ephemeral

**What**: Transient UI state that doesn't matter
**Speed**: 60fps, zero overhead
**Persistence**: None
**AI Perception**: None

```typescript
// Layer 0 - Pure React-like speed
class PositronWidget {
  // Ephemeral state - just JS variables
  private _hoverTarget: HTMLElement | null = null;
  private _dragPosition: { x: number, y: number } = { x: 0, y: 0 };
  private _animationFrame: number = 0;

  onMouseMove(e: MouseEvent) {
    // Direct mutation, no events, no nothing
    this._hoverTarget = e.target as HTMLElement;
    this.updateHoverStyles();  // Direct DOM manipulation
  }
}
```

**Key Principle**: Most state is ephemeral. Don't pay the persistence tax for hover states.

---

## Layer 1: Session

**What**: State that matters for this session but not beyond
**Speed**: Fast (in-memory Map)
**Persistence**: Memory only, lost on refresh
**AI Perception**: Optional (if subscribed)

```typescript
// Layer 1 - Session state
class SessionState {
  private store = new Map<string, unknown>();

  set(key: string, value: unknown, options?: { aiVisible?: boolean }) {
    this.store.set(key, value);

    if (options?.aiVisible) {
      // Only emit if AI cares
      Events.emit('session:state:changed', {
        key,
        value,
        layer: 1
      });
    }
  }

  get(key: string): unknown {
    return this.store.get(key);
  }
}

// Usage
sessionState.set('form.email', 'user@example.com', { aiVisible: true });
sessionState.set('dropdown.open', true);  // AI doesn't care
```

---

## Layer 2: Persistent

**What**: State that survives refresh/restart
**Speed**: Debounced (500ms+ batching)
**Persistence**: SQLite via Entity system
**AI Perception**: As entity CRUD events

```typescript
// Layer 2 - Persistent state (debounced)
class PersistentState {
  private pendingWrites = new Map<string, unknown>();
  private debounceTimer: number | null = null;
  private DEBOUNCE_MS = 500;

  async set(key: string, value: unknown): Promise<void> {
    this.pendingWrites.set(key, value);

    if (!this.debounceTimer) {
      this.debounceTimer = setTimeout(() => this.flush(), this.DEBOUNCE_MS);
    }
  }

  private async flush(): Promise<void> {
    const writes = new Map(this.pendingWrites);
    this.pendingWrites.clear();
    this.debounceTimer = null;

    // Batch write to SQLite
    await Commands.execute('data/batch-update', {
      collection: 'state',
      updates: Array.from(writes.entries()).map(([key, value]) => ({
        id: key,
        data: { value }
      }))
    });

    // AI perceives as entity changes
    Events.emit('data:state:batch-updated', {
      keys: Array.from(writes.keys()),
      layer: 2
    });
  }
}
```

---

## Layer 3: Semantic

**What**: Meaningful state that becomes memory
**Speed**: Async, compressed, batched heavily
**Persistence**: longterm.db with embeddings
**AI Perception**: Primary source of truth for understanding

```typescript
// Layer 3 - Semantic state (compressed + embedded)
class SemanticState {
  private eventBuffer: SemanticEvent[] = [];
  private FLUSH_INTERVAL = 5000;  // 5 seconds
  private MIN_EVENTS_TO_COMPRESS = 10;

  recordEvent(event: RawEvent): void {
    // Convert raw → semantic
    const semantic = this.toSemanticEvent(event);
    this.eventBuffer.push(semantic);

    // Batch compress when enough events
    if (this.eventBuffer.length >= this.MIN_EVENTS_TO_COMPRESS) {
      this.compressAndStore();
    }
  }

  private toSemanticEvent(raw: RawEvent): SemanticEvent {
    // Map low-level events to high-level intent
    switch (raw.type) {
      case 'click':
        return this.inferClickIntent(raw);
      case 'form-submit':
        return { intent: 'submitted-form', form: raw.formId, ...raw };
      case 'navigation':
        return { intent: 'navigated', from: raw.from, to: raw.to };
      default:
        return { intent: 'unknown', raw };
    }
  }

  private async compressAndStore(): Promise<void> {
    const events = [...this.eventBuffer];
    this.eventBuffer = [];

    // LLM-based compression: many events → one summary
    const summary = await Commands.execute('ai/compress-session', {
      events,
      maxTokens: 200
    });

    // Generate embedding for semantic search
    const embedding = await Commands.execute('ai/embed', {
      text: summary.text
    });

    // Store in longterm.db
    await Commands.execute('memory/store', {
      type: 'session-summary',
      content: summary.text,
      embedding: embedding.vector,
      entities: summary.entities,
      actions: summary.actions,
      timestamp: new Date()
    });

    // AI perception event
    Events.emit('semantic:session:stored', {
      summary: summary.text,
      layer: 3
    });
  }
}
```

---

## Decorator-Based Layer Assignment

Developers declare which layer state belongs to:

```typescript
class ProfileWidget extends PositronWidget {
  // Layer 0: Ephemeral - 60fps OK
  @Ephemeral()
  private hoverUserId: string | null = null;

  // Layer 1: Session - memory only
  @Session({ aiVisible: false })
  private scrollPosition: number = 0;

  // Layer 2: Persistent - SQLite
  @Persistent({ debounce: 500 })
  private selectedUserId: string | null = null;

  // Layer 3: Semantic - RAG indexed
  @Semantic({
    description: 'User being viewed',
    extractEntities: true
  })
  private viewedUser: UserEntity | null = null;
}
```

### Decorator Implementations

```typescript
function Ephemeral() {
  return function(target: any, propertyKey: string) {
    // No-op - just a marker, pure JS property
  };
}

function Session(options: { aiVisible?: boolean }) {
  return function(target: any, propertyKey: string) {
    const privateKey = `_${propertyKey}`;

    Object.defineProperty(target, propertyKey, {
      get() { return sessionState.get(privateKey); },
      set(value) {
        sessionState.set(privateKey, value, options);
      }
    });
  };
}

function Persistent(options: { debounce?: number }) {
  return function(target: any, propertyKey: string) {
    const privateKey = `_${propertyKey}`;

    Object.defineProperty(target, propertyKey, {
      get() { return this[privateKey]; },
      set(value) {
        this[privateKey] = value;
        persistentState.set(
          `${target.constructor.name}.${propertyKey}`,
          value
        );
      }
    });
  };
}

function Semantic(options: { description: string, extractEntities?: boolean }) {
  return function(target: any, propertyKey: string) {
    const privateKey = `_${propertyKey}`;

    Object.defineProperty(target, propertyKey, {
      get() { return this[privateKey]; },
      set(value) {
        const oldValue = this[privateKey];
        this[privateKey] = value;

        semanticState.recordEvent({
          type: 'state-change',
          widget: target.constructor.name,
          property: propertyKey,
          description: options.description,
          oldValue,
          newValue: value,
          extractEntities: options.extractEntities
        });
      }
    });
  };
}
```

---

## Performance Comparison

| Operation | React | Positron L0 | Positron L1 | Positron L2 | Positron L3 |
|-----------|-------|-------------|-------------|-------------|-------------|
| State update | ~0.1ms | ~0.1ms | ~0.2ms | ~5ms (batched) | ~100ms (async) |
| Re-render | ~1-10ms | ~1-10ms | ~1-10ms | ~1-10ms | ~1-10ms |
| Persistence | N/A | N/A | N/A | SQLite | longterm.db |
| AI visible | N/A | No | Optional | Yes | Yes (primary) |
| Semantic search | N/A | N/A | N/A | No | Yes |

**Key Insight**: Layer 0 and Layer 1 are as fast as React. We only pay the persistence/RAG tax when the state actually matters.

---

## AI Perception by Layer

```typescript
// What each layer means for AI perception

// Layer 0: Invisible
// AI never sees hover states, animations, drag positions

// Layer 1: Optional subscription
// AI CAN see if they subscribe, but usually doesn't
Events.subscribe('session:state:changed', handler);

// Layer 2: Entity awareness
// AI sees as database changes, part of world model
Events.subscribe('data:*:created', handler);
Events.subscribe('data:*:updated', handler);

// Layer 3: Primary perception
// This IS what AI "remembers" and can search
// Flows to RAG context automatically
// Semantic search available: "What did Joel do last week?"
```

---

## The Compression Pipeline

Raw events → Semantic events → Session summaries → Long-term memory

```
USER ACTIONS (1000+ events/session)
│
├─► Layer 0: Ignored (hover, scroll-tick, animation-frame)
│
├─► Layer 1: Session map (form values, UI state)
│
├─► Layer 2: Entity updates (SQLite, debounced)
│
└─► Layer 3: Semantic pipeline
    │
    ├─► Event classification (click → intent)
    │
    ├─► Entity extraction (mentioned: "Test User", "Joel")
    │
    ├─► Session batching (5-second windows)
    │
    ├─► LLM compression ("Joel viewed and froze Test User")
    │
    ├─► Embedding generation (for semantic search)
    │
    └─► longterm.db storage
```

---

## Why This Beats React + Separate AI

**React + ChatGPT approach**:
- UI state in React
- Copy relevant state to AI context manually
- No semantic memory
- No automatic perception

**Positron approach**:
- Same rendering speed for ephemeral state
- Automatic layer routing based on declarations
- Semantic compression built-in
- AI perception is automatic
- Long-term memory searchable

**The magic**: You write `@Semantic()` once, and that state becomes part of AI memory forever. No manual context building.

---

## Implementation Status

### Implemented
- [x] Layer 2: Entity system with SQLite
- [x] Events system for state changes
- [x] Basic RAG with embeddings

### In Progress
- [ ] Layer decorators (@Ephemeral, @Session, @Persistent, @Semantic)
- [ ] Debounced batching for Layer 2
- [ ] Semantic compression pipeline

### Planned
- [ ] Layer 3 automatic embedding generation
- [ ] Session summarization (LLM compression)
- [ ] longterm.db semantic search integration
- [ ] AI perception budget integration with layers

---

*Positron: Fast where it needs to be fast, persistent where it needs to persist, semantic where AI needs to understand.*
