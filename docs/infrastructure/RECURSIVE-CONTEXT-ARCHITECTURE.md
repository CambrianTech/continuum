# Recursive Context Architecture

## Overview

This document describes how Continuum implements recursive context navigation - a pattern where LLMs programmatically access large contexts via tools rather than having everything stuffed into their prompt.

This pattern was formalized in academic literature as "Recursive Language Models" (RLM, Zhang et al. 2024), but Continuum's entity-based architecture already implements the core concepts more elegantly.

## The Key Insight

**Academic approach (RLM):**
- Load context as Python variable in REPL
- LLM writes code to search/slice
- Recursive sub-calls to LLM on snippets

**Continuum approach:**
- Store everything as entities (unified data layer)
- LLM uses tools to query entities
- Tool results become memories (can recall, repeat, undo)
- Recursive calls via `ai/generate`

The advantage: No separate "context environment" - the entity system IS the environment.

## Architecture

### Everything is an Entity

```
┌─────────────────────────────────────────────────────────────┐
│                    Entity Layer (SQLite)                     │
├─────────────────────────────────────────────────────────────┤
│  chat_messages    │  memories        │  tool_results        │
│  decisions        │  code_edits      │  timeline_events     │
│  users            │  rooms           │  training_signals    │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    Unified Query Layer                       │
├─────────────────────────────────────────────────────────────┤
│  data/list         → Filter, sort, paginate any entity      │
│  data/vector-search → Semantic search with embeddings       │
│  ai/rag/query-*    → Cursor-based context streaming         │
└─────────────────────────────────────────────────────────────┘
```

### Tool Results as Memories

When a persona executes a tool, the result becomes an entity:

```typescript
// Tool execution
const result = await PersonaToolExecutor.execute('development/code/read', {
  path: 'src/main.ts'
});

// Result stored as entity
await Commands.execute('data/create', {
  collection: 'tool_results',
  data: {
    personaId,
    toolName: 'development/code/read',
    params: { path: 'src/main.ts' },
    result: result.data,
    timestamp: new Date(),
    contextId  // Links to conversation
  }
});
```

This enables:
- **Recall**: "What did I find when I read main.ts?"
- **Repeat**: Re-execute with same params
- **Undo**: For code edits, store before/after states
- **Learn**: "This tool call succeeded in similar situations"

### Context Navigation vs Context Stuffing

**Current (stuffed):**
```
RAGBuilder.buildContext()
  → Retrieves relevant entities
  → Compresses/summarizes
  → Stuffs into system prompt
  → LLM sees static snapshot
```

**Recursive (navigated):**
```
Minimal system prompt + tools
  → LLM decides what context it needs
  → Calls context/search, context/slice
  → Retrieves just-in-time
  → Can recurse: ai/generate on snippet
```

## Implementation

### Phase 1: Context Navigation Commands (IMPLEMENTED)

Two commands provide semantic context navigation:

#### `ai/context/search` - Semantic Search
```bash
# Find memories about TypeScript patterns
./jtag ai/context/search --query="TypeScript error handling patterns"

# Cross-context search (exclude current room)
./jtag ai/context/search --query="database optimization" \
  --excludeContextId="abc123" \
  --types="memories,timeline_events"
```

**Features:**
- Searches across: `chat_messages`, `memories`, `timeline_events`, `tool_results`
- Uses Rust embedding worker (ONNX, ~5ms per embedding)
- Cosine similarity with configurable threshold
- Hybrid mode: semantic, keyword, or combined

#### `ai/context/slice` - Retrieve Full Content
```bash
# Get full content of a memory found via search
./jtag ai/context/slice --id="abc123" --type="memories"

# Get chat message with thread context
./jtag ai/context/slice --id="def456" --type="chat_messages" --includeRelated=true
```

**Features:**
- Full content retrieval (search returns truncated summaries)
- Related item fetching (parent messages, linked memories)
- Works with all entity types

### Phase 2: Adapter Selection

In `PersonaMessageEvaluator` or `ChatRAGBuilder`:

```typescript
interface RAGStrategyConfig {
  strategy: 'stuffed' | 'navigated';

  // For 'navigated':
  maxPromptTokens: number;  // Keep prompt small
  contextToolsEnabled: boolean;
}
```

### Phase 3: Recursive Calls

The `context/recurse` tool:

```typescript
// Persona can spawn sub-generation on a snippet
await Commands.execute('context/recurse', {
  snippet: contextChunk,
  task: "Summarize the key points",
  model: 'llama3.2:1b'  // Can use smaller model for sub-tasks
});
```

## Undo/Revert Pattern

Because tool results are entities, undo is natural:

```typescript
// Code edit stored with before/after
interface CodeEditEntity extends BaseEntity {
  filePath: string;
  beforeContent: string;
  afterContent: string;
  editType: 'insert' | 'replace' | 'delete';
  personaId: string;
  timestamp: Date;
}

// Revert = apply inverse
async function revertEdit(editId: string) {
  const edit = await Commands.execute('data/read', {
    collection: 'code_edits',
    id: editId
  });

  await fs.writeFile(edit.filePath, edit.beforeContent);

  // Store revert as new entity (audit trail)
  await Commands.execute('data/create', {
    collection: 'code_edits',
    data: {
      ...edit,
      editType: 'revert',
      beforeContent: edit.afterContent,
      afterContent: edit.beforeContent,
      revertedEditId: editId
    }
  });
}
```

## Why This is Better Than Papers

1. **Unified data layer**: No separate "context store" - everything is entities
2. **Tool results as memories**: Automatic learning from actions
3. **Undo is natural**: Entity versioning enables time travel
4. **Already integrated**: Not bolted on - it's the foundation
5. **Type-safe tools**: Commands system with schema validation
6. **Rust performance**: Heavy lifting in workers, not Python

## Comparison with RLM Paper

| RLM Paper | Continuum |
|-----------|-----------|
| Python REPL sandbox | Commands system (already sandboxed) |
| Context as variable | Context as entities (queryable) |
| exec(code) | Commands.execute() |
| Sub-LLM calls | ai/generate |
| Custom Python libs | 170+ typed commands |
| Memory separate | Memory IS entities |

## Future Work

1. ✅ **Explicit context tools**: `ai/context/search`, `ai/context/slice` (DONE)
2. **Strategy adapter**: Let personas choose stuffed vs navigated RAG mode
3. **Edit history**: Full undo/redo for code changes via stored entities
4. **Learning from results**: Train on successful tool sequences

---

*This architecture emerged organically from the entity-first design principle. The academic formalization (RLM) validates the approach but describes a subset of what's implemented here.*
