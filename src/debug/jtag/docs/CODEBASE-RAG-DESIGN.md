# Codebase RAG System Design

**Status**: Implementation Phase (Week 1)
**Goal**: Enable PersonaUser to answer architecture questions by indexing and querying the codebase
**Related**: ARCHITECTURE-GAPS-PHASE1.md, PRACTICAL-ROADMAP.md

---

## Architecture Understanding

### Existing RAG Infrastructure

**RAGBuilder Pattern** (system/rag/shared/RAGBuilder.ts):
- Abstract class with `buildContext()` method
- Factory pattern for domain-specific builders
- Currently has `ChatRAGBuilder` for chat rooms
- Returns `RAGContext` with identity + conversation history + artifacts

**ChatRAGBuilder** (system/rag/builders/ChatRAGBuilder.ts):
- Loads recent messages from database
- Converts to LLM message format
- Extracts artifacts (images, files)
- Builds system prompt with persona identity
- Includes recipe strategy (conversation governance)

**Key Insight**: RAG builders are **context assemblers**, not search engines. They query data sources and format results for LLM consumption.

### System Patterns

**1. Constants** (system/shared/Constants.ts):
- Single source of truth for ALL constants
- COLLECTIONS object for entity collection names
- PATHS object for file system paths
- Helper functions for env var priority

**2. Commands** (commands/*/shared/*Types.ts):
- Shared types extend `CommandParams` and `CommandResult`
- Server implementation in server/*ServerCommand.ts
- Browser implementation (if needed) in browser/*BrowserCommand.ts
- 80-90% logic in shared, 5-10% in browser/server

**3. Entities** (system/data/entities/*.ts):
- Extend `BaseEntity` interface
- Generic at data layer, specific at application layer
- Collection names in Constants.COLLECTIONS

---

## Design Decisions

### 1. Full Vector Search From Start

**Why dumbing down is wrong**:
- Embedding adapters already exist (AIProviderAdapter pattern)
- Local embedding models are FREE (nomic-embed-text, all-minilm, etc.)
- Ollama already supports embeddings API
- Vector search is THE RIGHT WAY to do semantic code search

**Architecture**:
```typescript
// Embedding is just another AI adapter capability
interface EmbeddingAdapter extends AIProviderAdapter {
  async generateEmbedding(text: string, domain?: 'code' | 'text' | 'multilingual'): Promise<number[]>;
}

// Ollama supports multiple embedding models!
// Code-specific: qwen3-embedding (8B, ranks #1 on MTEB for code+multilingual)
// General text: nomic-embed-text (high-performance, long context)
// Large: mxbai-embed-large (state-of-the-art)
```

**Domain-Specific Model Selection**:
- **Code embeddings** (TypeScript, JavaScript): `qwen3-embedding` - Trained on code, understands syntax
- **Documentation** (Markdown): `nomic-embed-text` - Better for natural language
- **Mixed/Unknown**: `qwen3-embedding` - Handles both code and text well

**What we build**:
- Vector embeddings from day 1 (via Ollama adapter)
- **Smart model selection by content type**
- Cosine similarity search
- Hybrid approach: keyword + vector (best of both)
- Falls back to keyword-only if embeddings fail

**Why domain-specific matters**:
- Code embeddings understand `PersonaUser.inbox` vs `inbox` (class member context)
- They know `async function` patterns vs prose about async concepts
- Trained on GitHub repos, not Wikipedia
- Better semantic understanding of programming patterns

### 2. Storage Strategy

**code_index collection** (extends BaseEntity):
```typescript
interface CodeIndexEntry extends BaseEntity {
  // File metadata
  filePath: string;  // Relative path from repo root
  fileType: 'typescript' | 'markdown' | 'javascript';

  // Content
  content: string;  // Actual code or documentation
  summary?: string;  // Optional summary for long files

  // Location
  startLine?: number;
  endLine?: number;

  // TypeScript metadata
  exportType?: 'class' | 'interface' | 'function' | 'type' | 'const';
  exportName?: string;  // e.g., "PersonaUser"

  // Embeddings (Phase 1B)
  embedding?: number[];
  embeddingModel?: string;

  // Metadata
  lastIndexed: Date;
  imports?: string[];  // What this file imports
  exports?: string[];  // What this file exports
  tags?: string[];  // Additional categorization
}
```

**Why this works**:
- Simple keyword search works for exact matches ("PersonaUser", "Commands.execute")
- Full content stored means no need to read files at query time
- Export metadata enables targeted searches ("find all interfaces")
- Embedding field reserved for Phase 1B

### 3. Indexing Strategy

**TypeScriptIndexer**:
```typescript
class TypeScriptIndexer {
  constructor(
    private embeddingAdapter: EmbeddingAdapter
  ) {}

  async indexFile(filePath: string): Promise<CodeIndexEntry[]> {
    // 1. Read file content
    // 2. Parse with TypeScript AST (ts-morph)
    // 3. Extract exports (classes, interfaces, functions, types)
    // 4. Create one entry per export
    // 5. Include surrounding context (imports, JSDoc comments)
    // 6. Generate CODE-SPECIFIC embedding for each entry
    const entries: CodeIndexEntry[] = [];

    for (const exportItem of exports) {
      const content = this.buildContentWithContext(exportItem);

      // Use code-specific embedding model (qwen3-embedding)
      const embedding = await this.embeddingAdapter.generateEmbedding(
        content,
        'code'  // Signals to use qwen3-embedding
      );

      entries.push({
        ...exportItem,
        content,
        embedding,
        embeddingModel: 'qwen3-embedding'  // Track which model used
      });
    }

    return entries;
  }
}
```

**MarkdownIndexer**:
```typescript
class MarkdownIndexer {
  constructor(
    private embeddingAdapter: EmbeddingAdapter
  ) {}

  async indexFile(filePath: string): Promise<CodeIndexEntry[]> {
    // 1. Read file content
    // 2. Parse markdown structure (headers, code blocks) - use 'marked'
    // 3. Create one entry per section (## Header level)
    // 4. Include code examples within section
    // 5. Generate TEXT-SPECIFIC embedding for section content
    const entries: CodeIndexEntry[] = [];

    for (const section of sections) {
      // Markdown is natural language - use text model (nomic-embed-text)
      const embedding = await this.embeddingAdapter.generateEmbedding(
        section.content,
        'text'  // Signals to use nomic-embed-text
      );

      entries.push({
        ...section,
        embedding,
        embeddingModel: 'nomic-embed-text'  // Track which model used
      });
    }

    return entries;
  }
}
```

**Why this is better**:
- **Code gets code-trained embeddings** (qwen3-embedding understands syntax)
- **Docs get text-trained embeddings** (nomic-embed-text understands prose)
- Embeddings generated during indexing (one-time cost)
- Stored in database alongside content
- Query uses appropriate model based on what we're searching
- Ollama embedding models are free and local

### 4. Query Strategy - Hybrid (Vector + Keyword)

**Semantic vector search with keyword boost**:
```typescript
async queryCodebase(query: string, limit: number): Promise<CodeIndexEntry[]> {
  // 1. Generate query embedding
  const queryEmbedding = await this.embeddingAdapter.generateEmbedding(query);

  // 2. Load all indexed entries (with embeddings)
  const allEntries = await DataDaemon.query<CodeIndexEntry>({
    collection: COLLECTIONS.CODE_INDEX,
    filter: {},  // Load all
    limit: 1000  // Reasonable cap
  });

  // 3. Hybrid scoring: vector similarity + keyword boost
  const scored = allEntries.data.map(record => {
    const entry = record.data;

    // Cosine similarity (0-1)
    const vectorScore = this.cosineSimilarity(queryEmbedding, entry.embedding!);

    // Keyword boost (0-1)
    const keywordScore = this.calculateKeywordScore(query, entry);

    // Weighted combination (70% vector, 30% keyword)
    const finalScore = (vectorScore * 0.7) + (keywordScore * 0.3);

    return { entry, score: finalScore };
  });

  // 4. Sort by score descending
  scored.sort((a, b) => b.score - a.score);

  // 5. Return top N
  return scored.slice(0, limit).map(s => s.entry);
}

private cosineSimilarity(a: number[], b: number[]): number {
  const dotProduct = a.reduce((sum, val, i) => sum + val * b[i], 0);
  const magnitudeA = Math.sqrt(a.reduce((sum, val) => sum + val * val, 0));
  const magnitudeB = Math.sqrt(b.reduce((sum, val) => sum + val * val, 0));
  return dotProduct / (magnitudeA * magnitudeB);
}

private calculateKeywordScore(query: string, entry: CodeIndexEntry): number {
  const keywords = query.toLowerCase().split(/\s+/);
  let score = 0;

  for (const keyword of keywords) {
    // Exact export name match: 1.0
    if (entry.exportName?.toLowerCase() === keyword) {
      score += 1.0;
    }
    // Partial export name match: 0.5
    else if (entry.exportName?.toLowerCase().includes(keyword)) {
      score += 0.5;
    }
    // File path match: 0.3
    else if (entry.filePath.toLowerCase().includes(keyword)) {
      score += 0.3;
    }
  }

  return Math.min(score / keywords.length, 1.0);  // Normalize to 0-1
}
```

**Why hybrid is best**:
- Vector search handles semantic similarity ("Why does X have Y?" → finds X and Y relationship)
- Keyword boost ensures exact name matches rank high ("PersonaUser" → PersonaUser.ts first)
- Combines strengths of both approaches
- Falls back gracefully if embeddings missing (pure keyword)

### 5. CodebaseRAGBuilder Integration

**Extends RAGBuilder**:
```typescript
class CodebaseRAGBuilder extends RAGBuilder {
  readonly domain = 'code';

  async buildContext(contextId, personaId, options): Promise<RAGContext> {
    // contextId = query text (e.g., "Why does PersonaUser have inbox?")

    // 1. Query indexed codebase
    const results = await this.queryCodebase(contextId, options.maxMessages ?? 10);

    // 2. Build system message with code context
    const systemMessage = {
      role: 'system',
      content: `Relevant code:\n${formatResults(results)}`
    };

    // 3. User's question
    const userMessage = {
      role: 'user',
      content: contextId
    };

    // 4. Return RAGContext
    return {
      domain: 'code',
      contextId,
      personaId,
      identity: await this.loadPersonaIdentity(personaId),
      conversationHistory: [systemMessage, userMessage],
      artifacts: this.buildArtifacts(results),
      privateMemories: [],
      metadata: { ... }
    };
  }
}
```

**Why this works**:
- Fits existing RAGBuilder pattern
- System orchestrates query, AI just consumes results
- No tool use by AI needed
- Results formatted as conversation history

---

## Implementation Plan

### Week 1: Full Implementation (4-5 days)

**Day 1: Data Layer + Embedding Adapter**
- [x] Create CodebaseTypes.ts
- [ ] Add CODE_INDEX to Constants.COLLECTIONS
- [ ] Create CodeIndexEntity.ts extending BaseEntity
- [ ] Register in EntityRegistry
- [ ] Add embedding capability to AIProviderAdapter interface
- [ ] Implement in OllamaAdapter (use nomic-embed-text model)

**Day 2: Indexers with Embeddings**
- [ ] Create TypeScriptIndexer.ts
  - Parse TypeScript AST (ts-morph library)
  - Extract exports (classes, interfaces, functions, types, consts)
  - Include JSDoc comments and surrounding context
  - **Generate embeddings via adapter**
- [ ] Create MarkdownIndexer.ts
  - Parse markdown sections (marked library)
  - Extract headers, code blocks
  - Create entries per section
  - **Generate embeddings via adapter**

**Day 3: RAG Builder + Hybrid Query**
- [ ] Complete CodebaseRAGBuilder.ts
  - Hybrid query (vector + keyword)
  - Cosine similarity calculation
  - Format results as LLM messages
  - Build artifacts from code snippets
- [ ] Create rag/index-codebase command
  - Takes paths parameter
  - Calls TypeScriptIndexer + MarkdownIndexer
  - Stores entries with embeddings in database
- [ ] Create rag/query-codebase command
  - Takes query parameter
  - Returns formatted results with relevance scores

**Day 4: Testing + Validation**
- [ ] Pull nomic-embed-text model: `ollama pull nomic-embed-text`
- [ ] Index /system/user/ directory (~50 files)
- [ ] Query "PersonaUser inbox" (expect PersonaInbox.ts first)
- [ ] Query "How do Commands work?" (semantic similarity test)
- [ ] Verify hybrid scoring works (keyword + vector)
- [ ] Check accuracy of code snippets + line numbers

**Day 5: PersonaUser Integration**
- [ ] Modify PersonaUser to query codebase RAG
- [ ] Add buildPromptWithRAG() method
- [ ] Test end-to-end question answering
- [ ] Measure accuracy (target 70%+)

### Success Criteria (Week 1)

**Quantitative**:
- [ ] Code indexing completes in <60 seconds for /system/user/ (~50 files)
- [ ] Query returns results in <100ms
- [ ] Results include correct file paths + line numbers
- [ ] 70%+ of queries return relevant code

**Qualitative**:
- [ ] "Helper AI actually finds the right code"
- [ ] "File references are accurate"
- [ ] "Faster than manual search"

---

## Technical Details

### TypeScript AST Parsing

**Use ts-morph library**:
```typescript
import { Project, SourceFile } from 'ts-morph';

class TypeScriptIndexer {
  private project: Project;

  constructor() {
    this.project = new Project({
      tsConfigFilePath: './tsconfig.json'
    });
  }

  async indexFile(filePath: string): Promise<CodeIndexEntry[]> {
    const sourceFile = this.project.addSourceFileAtPath(filePath);
    const entries: CodeIndexEntry[] = [];

    // Extract classes
    for (const classDecl of sourceFile.getClasses()) {
      entries.push({
        id: generateId(),
        filePath: this.getRelativePath(filePath),
        fileType: 'typescript',
        content: classDecl.getFullText(),
        startLine: classDecl.getStartLineNumber(),
        endLine: classDecl.getEndLineNumber(),
        exportType: 'class',
        exportName: classDecl.getName(),
        lastIndexed: new Date(),
        ...
      });
    }

    // Extract interfaces, functions, types, etc.
    // ...

    return entries;
  }
}
```

### Markdown Parsing

**Use marked library**:
```typescript
import { marked } from 'marked';

class MarkdownIndexer {
  async indexFile(filePath: string): Promise<CodeIndexEntry[]> {
    const content = await fs.readFile(filePath, 'utf-8');
    const tokens = marked.lexer(content);
    const entries: CodeIndexEntry[] = [];

    let currentSection = '';
    let currentContent: string[] = [];
    let startLine = 1;

    for (const token of tokens) {
      if (token.type === 'heading' && token.depth === 2) {
        // Save previous section
        if (currentSection) {
          entries.push({
            id: generateId(),
            filePath: this.getRelativePath(filePath),
            fileType: 'markdown',
            content: currentContent.join('\n'),
            exportType: 'markdown-section',
            exportName: currentSection,
            startLine,
            lastIndexed: new Date()
          });
        }

        // Start new section
        currentSection = token.text;
        currentContent = [];
        startLine = token.line ?? startLine;
      } else {
        currentContent.push(token.raw);
      }
    }

    return entries;
  }
}
```

### Query Algorithm (Phase 1A)

**Priority-based keyword matching**:
```typescript
async queryCodebase(query: string, limit: number): Promise<CodeIndexEntry[]> {
  const keywords = this.extractKeywords(query);  // ["PersonaUser", "inbox"]

  // Query database
  const allEntries = await DataDaemon.query<CodeIndexEntry>({
    collection: COLLECTIONS.CODE_INDEX,
    filter: {}, // Load all for now
    limit: 1000  // Reasonable cap
  });

  if (!allEntries.success || !allEntries.data) {
    return [];
  }

  const entries = allEntries.data.map(r => r.data);

  // Score each entry
  const scored = entries.map(entry => ({
    entry,
    score: this.calculateScore(entry, keywords)
  }));

  // Sort by score descending
  scored.sort((a, b) => b.score - a.score);

  // Return top N
  return scored.slice(0, limit).map(s => s.entry);
}

private calculateScore(entry: CodeIndexEntry, keywords: string[]): number {
  let score = 0;

  for (const keyword of keywords) {
    // Exact export name match: 100 points
    if (entry.exportName?.toLowerCase() === keyword.toLowerCase()) {
      score += 100;
    }
    // Partial export name match: 50 points
    else if (entry.exportName?.toLowerCase().includes(keyword.toLowerCase())) {
      score += 50;
    }
    // File path match: 30 points
    else if (entry.filePath.toLowerCase().includes(keyword.toLowerCase())) {
      score += 30;
    }
    // Content match: 10 points
    else if (entry.content.toLowerCase().includes(keyword.toLowerCase())) {
      score += 10;
    }
  }

  return score;
}
```

---

## Future Enhancements (Phase 1B+)

### Vector Embeddings

**Add embedding generation**:
```typescript
import { encode } from '@anthropic-ai/text-encoder';  // Or OpenAI API

class EmbeddingService {
  async generateEmbedding(text: string): Promise<number[]> {
    // Option 1: OpenAI API
    const response = await fetch('https://api.openai.com/v1/embeddings', {
      method: 'POST',
      headers: { 'Authorization': `Bearer ${OPENAI_API_KEY}` },
      body: JSON.stringify({
        model: 'text-embedding-ada-002',
        input: text
      })
    });

    const data = await response.json();
    return data.data[0].embedding;

    // Option 2: Local model (GGUF via llama.cpp)
    // More complex but free
  }
}
```

**Vector similarity search**:
```typescript
async queryCodebaseWithEmbeddings(query: string, limit: number): Promise<CodeIndexEntry[]> {
  // 1. Generate embedding for query
  const queryEmbedding = await this.embeddingService.generateEmbedding(query);

  // 2. Calculate cosine similarity with all indexed entries
  const entries = await this.loadAllEntriesWithEmbeddings();
  const scored = entries.map(entry => ({
    entry,
    similarity: this.cosineSimilarity(queryEmbedding, entry.embedding!)
  }));

  // 3. Sort by similarity
  scored.sort((a, b) => b.similarity - a.similarity);

  // 4. Return top N
  return scored.slice(0, limit).map(s => s.entry);
}

private cosineSimilarity(a: number[], b: number[]): number {
  const dotProduct = a.reduce((sum, val, i) => sum + val * b[i], 0);
  const magnitudeA = Math.sqrt(a.reduce((sum, val) => sum + val * val, 0));
  const magnitudeB = Math.sqrt(b.reduce((sum, val) => sum + val * val, 0));
  return dotProduct / (magnitudeA * magnitudeB);
}
```

### Incremental Indexing

**Watch file system for changes**:
```typescript
import { watch } from 'chokidar';

class CodebaseWatcher {
  start(paths: string[]) {
    const watcher = watch(paths, {
      ignored: /(^|[\/\\])\../, // Ignore dotfiles
      persistent: true
    });

    watcher.on('change', async (path) => {
      console.log(`📝 File changed: ${path}`);
      await this.reindexFile(path);
    });

    watcher.on('add', async (path) => {
      console.log(`➕ File added: ${path}`);
      await this.indexFile(path);
    });

    watcher.on('unlink', async (path) => {
      console.log(`➖ File removed: ${path}`);
      await this.removeFromIndex(path);
    });
  }
}
```

### Scope-Based Context

**Load public LoRA layers from scope**:
```typescript
// In CodebaseRAGBuilder
async buildContext(scopePath: string, personaId: UUID): Promise<RAGContext> {
  // 1. Query code only from scopePath
  const results = await this.queryCodebase(query, {
    scopePath: '/system/user/',  // Only index this directory
    limit: 10
  });

  // 2. Load public LoRA layers from .continuum/ in scope
  const publicLayers = await this.loadPublicLayers(scopePath);

  // 3. Return context with scope-specific expertise
  return {
    ...ragContext,
    metadata: {
      ...metadata,
      scopePath,
      publicLayers: publicLayers.map(l => l.id)
    }
  };
}
```

---

## Related Documentation

- [ARCHITECTURE-GAPS-PHASE1.md](ARCHITECTURE-GAPS-PHASE1.md) - Gap analysis identifying this as critical
- [PRACTICAL-ROADMAP.md](PRACTICAL-ROADMAP.md) - Phase 1 Milestone 1
- [RAG_ADAPTER_ARCHITECTURE.md](../system/rag/RAG_ADAPTER_ARCHITECTURE.md) - Existing RAG patterns
- [CLAUDE.md](../CLAUDE.md) - Essential development patterns

---

**Status**: Design complete, ready for implementation
**Next**: Day 1 implementation (Data Layer)
**Last Updated**: 2025-11-12
