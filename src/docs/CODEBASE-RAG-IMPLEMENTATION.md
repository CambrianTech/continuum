# Codebase RAG Implementation Plan

## Architecture Vision

**Goal**: Enable personas to learn from code through embeddings and self-directed study.

**Key Principle**: Break down into small, composable commands that can be used independently or combined for emergent behaviors.

## Command Hierarchy

### Low-Level Commands (Building Blocks)

```bash
# 1. Generate embeddings for any text
./jtag ai/embedding/generate --text="code snippet" --model="qwen3-embedding"
# Returns: { embedding: number[], model: string, dimensions: number }

# 2. Store code index entry
./jtag ai/rag/index/create --filePath="..." --content="..." --embedding=[...]
# Returns: { entryId: UUID, indexed: boolean }

# 3. Query code index with embeddings
./jtag ai/rag/index/query --query="PersonaUser" --limit=5
# Returns: { entries: CodeIndexEntry[], relevanceScores: number[] }
```

### Mid-Level Commands (Composed Operations)

```bash
# Index a single file (uses: generate + create)
./jtag ai/rag/index/file --filePath="system/user/PersonaUser.ts"
# Internally: Parse → Generate embeddings → Store entries

# Index a directory (uses: index/file repeatedly)
./jtag ai/rag/index/directory --path="system/user" --recursive
# Internally: Find files → Index each file

# Search codebase (uses: generate + query)
./jtag ai/rag/search --query="how does coordination work?" --limit=10
# Internally: Generate query embedding → Search index → Return ranked results
```

### High-Level Commands (User-Facing)

```bash
# Index entire codebase
./jtag ai/rag/index-codebase --paths="system,commands" --fileTypes="typescript,markdown"
# Uses: index/directory for each path

# Ask question about codebase
./jtag ai/rag/ask --question="How does PersonaUser handle learning?"
# Uses: search + AI generation with context
```

## Implementation Order

### Phase 1: Foundation (Low-Level)
1. ✅ `CodeIndexEntity` - Data structure
2. ✅ `OllamaAdapter.createEmbedding()` - Embedding generation
3. ✅ `ai/embedding/generate` - Command wrapper for embedding generation
4. ✅ `ai/rag/index/create` - Store single code entry
5. ⬜ `ai/rag/query-open` - Open similarity search, return handle + first page
6. ⬜ `ai/rag/query-fetch` - Fetch results at any offset (bidirectional)
7. ⬜ `ai/rag/query-close` - Close query handle and cleanup

### Phase 2: Composition (Mid-Level)
6. ⬜ `ai/rag/index/file` - Parse and index single file (TypeScript/Markdown)
7. ⬜ `ai/rag/index/directory` - Index multiple files
8. ⬜ `ai/rag/search` - Semantic code search

### Phase 3: User Experience (High-Level)
9. ⬜ `ai/rag/index-codebase` - Full codebase indexing
10. ⬜ `ai/rag/ask` - Natural language queries

### Phase 4: Learning Integration
11. ⬜ PersonaUser captures conversation embeddings
12. ⬜ PersonaUser schedules self-training tasks
13. ⬜ Learning session orchestration

## Command Design Pattern

Each command follows this structure:

```
commands/ai/[category]/[action]/
├── shared/
│   ├── [Action]Types.ts      # Params & Result interfaces
│   └── [Action]Command.ts    # Abstract base class
├── server/
│   └── [Action]ServerCommand.ts  # Implementation
└── browser/
    └── [Action]BrowserCommand.ts # (if needed)
```

## Why This Works

**Composability**:
- Low-level commands = Lego blocks
- Mid-level commands = Pre-built structures
- High-level commands = Complete solutions
- PersonaUser can use ANY level for its needs

**Emergent Behaviors**:
```typescript
// Simple: Human indexes code manually
./jtag ai/rag/index-codebase --paths="system/user"

// Complex: Persona self-directs learning
PersonaUser.serviceInbox()
  → Detects knowledge gap in "coordination"
  → Creates task: { type: 'study', topic: 'coordination' }
  → Executes: ai/rag/search --query="coordination patterns"
  → Generates quiz from results
  → Answers quiz
  → Captures training data
  → Fine-tunes adapter
  → Returns to normal operation
```

**Reusability**:
- Teacher AI uses `ai/rag/search` to generate quizzes
- Helper AI uses `ai/rag/search` to improve answers
- CodeReview AI uses `ai/rag/search` to find patterns
- Human uses `ai/rag/search` to explore codebase

## Next Steps

1. Implement `ai/embedding/generate` command (wrapper for OllamaAdapter)
2. Implement `ai/rag/index/create` command (store single entry)
3. Implement `ai/rag/index/query` command (search with embeddings)
4. Test composition by building `ai/rag/index/file` from primitives
5. Continue building upward

**Start simple, compose elegantly, unlock emergence.**

## Universal Iterator/Cursor Pattern

The RAG query commands implement a **universal iterator pattern** that works for all time-ordered and ranked data.

### The Three-Command Pattern

Every iterator follows this structure:
1. **`{domain}/query-open`** - Opens query, returns handle + first page
2. **`{domain}/query-fetch`** - Fetches at any position (bidirectional + random access)
3. **`{domain}/query-close`** - Closes handle and cleanup

### Iterator Features

**Bidirectional Navigation:**
```bash
# Move forward through results
./jtag ai/rag/query-fetch --queryHandle="abc" --direction="forward"

# Move backward
./jtag ai/rag/query-fetch --queryHandle="abc" --direction="backward"
```

**Random Access:**
```bash
# Jump to specific offset
./jtag ai/rag/query-fetch --queryHandle="abc" --offset=30 --limit=10
```

**Position Tracking:**
- Handle maintains current offset
- Results include `hasMore` and `hasPrevious` flags
- Frontend can implement infinite scroll in both directions

### Universal Applications

This pattern applies to:
- **RAG similarity search** (relevance-ranked)
- **Chat message history** (time-ordered)
- **Game replays** (frame-by-frame navigation)
- **Training logs** (epoch progression)
- **Database queries** (already implemented: `data/query-*`)
- **Time series data** (any chronological sequence)
- **Event streams** (real-time or historical)

### Future: Formalized Iterator Type

Eventually this will become a generic abstraction:
```typescript
interface Iterator<T> {
  handle: UUID;
  offset: number;
  limit: number;
  totalCount: number;
  hasMore: boolean;
  hasPrevious: boolean;
}
```

**One mental model, infinite applications.**
