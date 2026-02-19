# Semantic Search Architecture

**Status**: Design Phase (Phase 3 Implementation)
**Created**: 2025-11-23
**Pattern**: Adapter Strategy + DataDaemon Abstraction

## Overview

Semantic search is implemented as a **first-class data operation** in the DataDaemon layer, using the adapter strategy pattern to support multiple backend implementations (SQLite, PostgreSQL, Elasticsearch) without changing application code.

**Key Principle**: Vector search is just another data operation, like `list` or `get`. Keep it in the data layer where it belongs.

---

## Architecture Layers

```
Application Layer (PersonaGenome, Hippocampus)
    ↓
SemanticSearchService (High-level API)
    ↓
Commands.execute('data/vector-search', ...)
    ↓
VectorSearchCommand
    ↓
DataDaemon Interface
    ↓
DataDaemonAdapter (Strategy Pattern)
    ├─ SQLiteAdapter (sqlite-vss)
    ├─ PostgreSQLAdapter (pgvector)
    └─ ElasticsearchAdapter (native knn)
```

---

## DataDaemon Interface Extension

### New Vector Operations

```typescript
// daemons/data-daemon/shared/DataDaemon.ts

export interface DataDaemon {
  // Existing operations
  create<T extends BaseEntity>(collection: string, data: T): Promise<T>;
  list<T extends BaseEntity>(collection: string, options: ListOptions): Promise<ListResult<T>>;
  get<T extends BaseEntity>(collection: string, id: UUID): Promise<T | null>;
  update<T extends BaseEntity>(collection: string, id: UUID, updates: Partial<T>): Promise<T>;
  delete(collection: string, id: UUID): Promise<void>;

  // NEW: Vector operations (Phase 3)
  vectorSearch<T extends BaseEntity>(
    collection: string,
    options: VectorSearchOptions
  ): Promise<VectorSearchResult<T>>;

  generateEmbedding(
    text: string,
    model?: string
  ): Promise<number[]>;

  indexVector(
    collection: string,
    id: UUID,
    embedding: number[]
  ): Promise<void>;

  backfillVectors(
    collection: string,
    batchSize?: number
  ): Promise<BackfillResult>;
}
```

### Type Definitions

```typescript
// daemons/data-daemon/shared/VectorTypes.ts

export interface VectorSearchOptions {
  // Query specification (provide one)
  queryVector?: number[];       // Direct vector
  queryText?: string;           // Generate embedding from text

  // Search parameters
  k?: number;                   // Top K results (default: 10)
  similarityThreshold?: number; // Minimum similarity 0-1 (default: 0)

  // Hybrid search
  hybridMode?: 'semantic' | 'keyword' | 'hybrid';
  hybridRatio?: number;         // Semantic weight 0-1 (default: 0.7)
  keywordQuery?: string;        // For hybrid/keyword search

  // Standard filters (importance, domain, etc.)
  filter?: Record<string, any>;

  // Pagination
  offset?: number;
  limit?: number;
}

export interface VectorSearchResult<T> {
  results: Array<{
    data: T;
    score: number;        // Similarity score 0-1 (1=identical)
    distance: number;     // Vector distance
    metadata?: {
      semanticScore?: number;
      keywordScore?: number;
    };
  }>;
  total: number;
  queryVector: number[];  // The vector that was searched
}

export interface BackfillResult {
  success: boolean;
  processed: number;
  failed: number;
  errors?: Array<{ id: UUID; error: string }>;
}
```

---

## Adapter Strategy Pattern

### Base Adapter Interface

```typescript
// daemons/data-daemon/server/adapters/DataDaemonAdapter.ts

export interface DataDaemonAdapter {
  // Standard CRUD operations
  create<T>(collection: string, data: T): Promise<T>;
  list<T>(collection: string, options: ListOptions): Promise<ListResult<T>>;
  get<T>(collection: string, id: string): Promise<T | null>;
  update<T>(collection: string, id: string, updates: Partial<T>): Promise<T>;
  delete(collection: string, id: string): Promise<void>;

  // Vector operations (NEW)
  vectorSearch<T>(collection: string, options: VectorSearchOptions): Promise<VectorSearchResult<T>>;
  generateEmbedding(text: string, model?: string): Promise<number[]>;
  indexVector(collection: string, id: string, embedding: number[]): Promise<void>;
  backfillVectors(collection: string, batchSize?: number): Promise<BackfillResult>;

  // Lifecycle
  initialize(): Promise<void>;
  shutdown(): Promise<void>;
}
```

### SQLite Implementation (Phase 3.0)

```typescript
// daemons/data-daemon/server/adapters/SQLiteAdapter.ts

export class SQLiteDataAdapter implements DataDaemonAdapter {
  private db: Database;
  private embeddingService: EmbeddingService;

  constructor(config: SQLiteConfig) {
    this.db = new Database(config.path);
    this.embeddingService = new OllamaEmbeddingService({
      url: config.ollamaUrl || 'http://localhost:11434',
      model: config.embeddingModel || 'all-minilm'
    });
  }

  async initialize(): Promise<void> {
    // Load vector extension
    this.db.loadExtension('sqlite-vss');

    // Ensure vector tables exist
    await this.ensureVectorTables();
  }

  async vectorSearch<T>(
    collection: string,
    options: VectorSearchOptions
  ): Promise<VectorSearchResult<T>> {

    // 1. Get or generate query vector
    const queryVector = await this.resolveQueryVector(options);

    // 2. Route to appropriate search method
    switch (options.hybridMode || 'semantic') {
      case 'semantic':
        return this.semanticSearch(collection, queryVector, options);
      case 'keyword':
        return this.keywordSearch(collection, options);
      case 'hybrid':
        return this.hybridSearch(collection, queryVector, options);
    }
  }

  private async semanticSearch<T>(
    collection: string,
    queryVector: number[],
    options: VectorSearchOptions
  ): Promise<VectorSearchResult<T>> {

    const vectorTable = `${collection}_vectors`;
    const filterClause = this.buildFilterClause(options.filter);

    const stmt = this.db.prepare(`
      SELECT
        d.data,
        (1 - vss_distance(v.embedding, ?)) as score,
        vss_distance(v.embedding, ?) as distance
      FROM ${vectorTable} v
      JOIN ${collection} d ON d.id = v.row_id
      WHERE vss_search(v.embedding, ?)
        ${filterClause.sql}
        AND (1 - vss_distance(v.embedding, ?)) >= ?
      ORDER BY distance ASC
      LIMIT ? OFFSET ?
    `);

    const results = stmt.all(
      JSON.stringify(queryVector),
      JSON.stringify(queryVector),
      JSON.stringify(queryVector),
      JSON.stringify(queryVector),
      options.similarityThreshold || 0,
      options.limit || 10,
      options.offset || 0,
      ...filterClause.params
    );

    return {
      results: results.map(r => ({
        data: JSON.parse(r.data) as T,
        score: r.score,
        distance: r.distance
      })),
      total: results.length,
      queryVector: queryVector
    };
  }

  async generateEmbedding(text: string, model?: string): Promise<number[]> {
    return this.embeddingService.embed(text, model);
  }

  async indexVector(
    collection: string,
    id: string,
    embedding: number[]
  ): Promise<void> {
    const vectorTable = `${collection}_vectors`;

    this.db.prepare(`
      INSERT INTO ${vectorTable} (row_id, embedding)
      VALUES (?, ?)
      ON CONFLICT(row_id) DO UPDATE SET embedding = excluded.embedding
    `).run(id, JSON.stringify(embedding));
  }

  async backfillVectors(
    collection: string,
    batchSize: number = 100
  ): Promise<BackfillResult> {
    // Get all rows without vectors
    const rows = this.db.prepare(`
      SELECT d.id, d.data
      FROM ${collection} d
      LEFT JOIN ${collection}_vectors v ON v.row_id = d.id
      WHERE v.row_id IS NULL
      LIMIT ?
    `).all(batchSize);

    let processed = 0;
    let failed = 0;
    const errors: Array<{ id: UUID; error: string }> = [];

    for (const row of rows) {
      try {
        const data = JSON.parse(row.data);
        const content = this.extractSearchableContent(data);
        const embedding = await this.generateEmbedding(content);
        await this.indexVector(collection, row.id, embedding);
        processed++;
      } catch (error) {
        failed++;
        errors.push({
          id: row.id,
          error: error instanceof Error ? error.message : String(error)
        });
      }
    }

    return { success: failed === 0, processed, failed, errors };
  }

  private async ensureVectorTables(): Promise<void> {
    // Create vector table for each collection
    const collections = this.getCollections();

    for (const collection of collections) {
      this.db.exec(`
        CREATE VIRTUAL TABLE IF NOT EXISTS ${collection}_vectors
        USING vss0(
          row_id TEXT PRIMARY KEY,
          embedding(384)
        )
      `);
    }
  }
}
```

### PostgreSQL Implementation (Phase 3.5 - Future)

```typescript
// daemons/data-daemon/server/adapters/PostgreSQLAdapter.ts

export class PostgreSQLDataAdapter implements DataDaemonAdapter {

  async vectorSearch<T>(
    collection: string,
    options: VectorSearchOptions
  ): Promise<VectorSearchResult<T>> {

    const queryVector = await this.resolveQueryVector(options);

    // PostgreSQL uses pgvector extension with <=> operator
    const result = await this.pool.query(`
      SELECT
        d.data,
        1 - (v.embedding <=> $1::vector) as score,
        v.embedding <=> $1::vector as distance
      FROM ${collection}_vectors v
      JOIN ${collection} d ON d.id = v.row_id
      WHERE v.embedding <=> $1::vector < $2
      ORDER BY v.embedding <=> $1::vector
      LIMIT $3 OFFSET $4
    `, [
      `[${queryVector.join(',')}]`,
      1 - (options.similarityThreshold || 0),
      options.limit || 10,
      options.offset || 0
    ]);

    return {
      results: result.rows.map(r => ({
        data: r.data as T,
        score: r.score,
        distance: r.distance
      })),
      total: result.rowCount || 0,
      queryVector: queryVector
    };
  }
}
```

### Elasticsearch Implementation (Phase 4.0 - Future)

```typescript
// daemons/data-daemon/server/adapters/ElasticsearchAdapter.ts

export class ElasticsearchDataAdapter implements DataDaemonAdapter {

  async vectorSearch<T>(
    collection: string,
    options: VectorSearchOptions
  ): Promise<VectorSearchResult<T>> {

    const queryVector = await this.resolveQueryVector(options);

    // Elasticsearch native knn search
    const result = await this.client.search({
      index: collection,
      body: {
        knn: {
          field: 'embedding',
          query_vector: queryVector,
          k: options.k || 10,
          num_candidates: 100
        },
        query: options.filter ? this.buildESQuery(options.filter) : undefined,
        size: options.limit || 10,
        from: options.offset || 0
      }
    });

    return {
      results: result.hits.hits.map(hit => ({
        data: hit._source as T,
        score: hit._score! / 2,  // Normalize to 0-1
        distance: 1 - (hit._score! / 2)
      })),
      total: result.hits.total.value,
      queryVector: queryVector
    };
  }
}
```

---

## Embedding Service Abstraction

```typescript
// system/search/shared/EmbeddingService.ts

export interface EmbeddingService {
  embed(text: string, model?: string): Promise<number[]>;
  batchEmbed(texts: string[], model?: string): Promise<number[][]>;
}

// system/search/server/OllamaEmbeddingService.ts

export class OllamaEmbeddingService implements EmbeddingService {
  constructor(
    private config: {
      url: string;
      model: string;
    }
  ) {}

  async embed(text: string, model?: string): Promise<number[]> {
    const response = await fetch(`${this.config.url}/api/embeddings`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        model: model || this.config.model,
        prompt: text
      })
    });

    const data = await response.json();
    return data.embedding;
  }

  async batchEmbed(texts: string[], model?: string): Promise<number[][]> {
    return Promise.all(texts.map(text => this.embed(text, model)));
  }
}
```

---

## Configuration

```typescript
// config/database.ts

export interface DatabaseConfig {
  adapter: 'sqlite' | 'postgresql' | 'elasticsearch';

  // SQLite config
  sqlite?: {
    path: string;
    extensions: string[];
  };

  // PostgreSQL config
  postgresql?: {
    host: string;
    port: number;
    database: string;
    user: string;
    password: string;
  };

  // Elasticsearch config
  elasticsearch?: {
    nodes: string[];
    auth?: {
      username: string;
      password: string;
    };
  };

  // Vector search config
  vectorSearch: {
    embeddingModel: string;
    embeddingDimensions: number;
    ollamaUrl?: string;
  };
}

// Example: SQLite with vector search
export const DATABASE_CONFIG: DatabaseConfig = {
  adapter: 'sqlite',
  sqlite: {
    path: './data/continuum.db',
    extensions: ['sqlite-vss']
  },
  vectorSearch: {
    embeddingModel: 'all-minilm',
    embeddingDimensions: 384,
    ollamaUrl: 'http://localhost:11434'
  }
};
```

---

## Command Interface

```typescript
// commands/data/vector-search/shared/VectorSearchTypes.ts

export interface VectorSearchParams extends CommandParams {
  collection: string;
  queryText?: string;
  queryVector?: number[];
  k?: number;
  similarityThreshold?: number;
  hybridMode?: 'semantic' | 'keyword' | 'hybrid';
  hybridRatio?: number;
  filter?: Record<string, any>;
  offset?: number;
  limit?: number;
}

export interface VectorSearchResult extends CommandResult {
  results: Array<{
    data: any;
    score: number;
    distance: number;
    metadata?: any;
  }>;
  total: number;
  queryVector: number[];
}
```

---

## Usage Examples

### Through Commands (Recommended)

```typescript
// Search semantically
const results = await Commands.execute('data/vector-search', {
  collection: 'memories',
  queryText: "system coordination failures",
  k: 10,
  similarityThreshold: 0.7,
  hybridMode: 'hybrid',
  filter: {
    domain: 'chat',
    importance: { $gte: 0.5 }
  }
});

// Results work regardless of backend (SQLite, PostgreSQL, Elasticsearch)
for (const result of results.results) {
  console.log(`${result.score.toFixed(2)}: ${result.data.content}`);
}
```

### Direct DataDaemon Access

```typescript
// For low-level usage
const dataDaemon = DataDaemon.getInstance();

const results = await dataDaemon.vectorSearch('memories', {
  queryText: "async debugging patterns",
  k: 5,
  hybridMode: 'semantic'
});
```

---

## Migration Path

### Phase 3.0: SQLite + Vector Search (Week 1-2)

**Tasks:**
1. Extend DataDaemon interface with vector operations
2. Implement SQLiteAdapter vector methods
3. Create VectorSearchCommand
4. Install sqlite-vss extension
5. Create OllamaEmbeddingService

**Deliverables:**
- ✅ Vector search working with SQLite
- ✅ Embeddings generated via Ollama
- ✅ Hybrid search (semantic + keyword)

### Phase 3.5: PostgreSQL Support (Week 3-4)

**Tasks:**
1. Implement PostgreSQLAdapter
2. Use pgvector extension
3. Test with larger datasets

**Benefits:**
- Better concurrency
- ACID transactions
- Better tooling

### Phase 4.0: Elasticsearch Support (Month 2)

**Tasks:**
1. Implement ElasticsearchAdapter
2. Use native knn search
3. Benchmark at scale

**Benefits:**
- Native vector search
- Massive scale (millions of vectors)
- Advanced features (reranking, etc.)

---

## Testing Strategy

### Unit Tests

```typescript
// tests/unit/vector-search.test.ts

describe('VectorSearch', () => {
  let adapter: MockDataAdapter;

  beforeEach(() => {
    adapter = new MockDataAdapter();
  });

  it('should find similar documents', async () => {
    // Insert test documents with known embeddings
    await adapter.create('test', {
      id: 'doc1',
      content: 'async debugging patterns',
      embedding: [0.1, 0.2, 0.3, ...]
    });

    // Search
    const results = await adapter.vectorSearch('test', {
      queryVector: [0.11, 0.21, 0.31, ...],  // Similar
      k: 1
    });

    expect(results.results[0].data.id).toBe('doc1');
    expect(results.results[0].score).toBeGreaterThan(0.9);
  });
});
```

### Integration Tests

```typescript
// tests/integration/vector-search-backends.test.ts

describe('VectorSearch across backends', () => {
  const backends = ['sqlite', 'postgresql', 'elasticsearch'];

  backends.forEach(backend => {
    it(`should work with ${backend}`, async () => {
      const adapter = createAdapter(backend);
      await adapter.initialize();

      // Same test for all backends
      const results = await adapter.vectorSearch('memories', {
        queryText: "test query",
        k: 5
      });

      expect(results.results).toHaveLength(5);
      expect(results.results[0].score).toBeGreaterThan(0);
    });
  });
});
```

---

## Performance Considerations

### Embedding Generation

- **Ollama local**: ~50-100ms per text (CPU)
- **Batch embeddings**: 5-10x faster than sequential
- **Cache frequently used embeddings**

### Vector Search

- **SQLite (sqlite-vss)**: ~1-5ms for 10K vectors
- **PostgreSQL (pgvector)**: ~1-10ms for 100K vectors
- **Elasticsearch**: ~10-50ms for millions of vectors

### Optimization Tips

1. **Pre-filter with metadata** before vector search
2. **Use hybrid search** (semantic + keyword) for best accuracy
3. **Batch backfill** embeddings (100-1000 at a time)
4. **Index frequently searched collections**
5. **Cache query embeddings** for common queries

---

## Security & Privacy

### Embedding Security

- Embeddings can leak information about original text
- Store embeddings separately if needed
- Consider encryption for sensitive data

### Network Security

- Ollama runs locally (no data leaves machine)
- Vector search is local (SQLite) or controlled (PostgreSQL)
- Only share embeddings, not raw text, across P2P mesh

---

## References

- **sqlite-vss**: https://github.com/asg017/sqlite-vss
- **pgvector**: https://github.com/pgvector/pgvector
- **Elasticsearch Vector Search**: https://www.elastic.co/what-is/vector-search
- **Ollama Embeddings API**: https://github.com/ollama/ollama/blob/main/docs/api.md#generate-embeddings

---

## Next Steps

1. **Read companion docs:**
   - `PersonaGenome-VECTOR-SEARCH.md` - Genomic layer discovery
   - `HIPPOCAMPUS-VECTOR-RETRIEVAL.md` - Memory retrieval

2. **Start implementation:**
   ```bash
   npm install better-sqlite3 sqlite-vss
   ollama pull all-minilm
   ./jtag data/vector-search --help
   ```

3. **Test with real data:**
   ```bash
   ./jtag data/backfill-vectors --collection=memories
   ./jtag data/vector-search --collection=memories --query="test"
   ```
