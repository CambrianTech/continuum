# Vector Search Architecture

**Created**: 2025-11-23
**Status**: Implementation in progress

## Overview

Extensible vector search architecture using composition pattern.
**Goal**: Minimal backend-specific code, maximum code reuse.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                   VectorSearchAdapterBase                    │
│               (ALL generic vector search logic)              │
├─────────────────────────────────────────────────────────────┤
│ Dependencies (composition):                                  │
│   • DataStorageAdapter (existing CRUD operations)           │
│   • VectorStorageOperations (4 backend-specific methods)    │
├─────────────────────────────────────────────────────────────┤
│ Implements:                                                  │
│   • vectorSearch() - cosine similarity + top-k              │
│   • generateEmbedding() - delegates to AIProviderDaemon     │
│   • indexVector() - delegates to vectorOps                  │
│   • backfillVectors() - batch embedding generation          │
│   • getVectorIndexStats() - counts + dimensions             │
│   • getVectorSearchCapabilities() - feature detection       │
└─────────────────────────────────────────────────────────────┘
                               ▲
                               │ uses
           ┌───────────────────┴────────────────────┐
           │                                        │
┌──────────────────────┐              ┌─────────────────────────┐
│ SqliteStorageAdapter │              │ JsonFileStorageAdapter  │
│                      │              │                         │
│ Implements 4 methods:│              │ Implements 4 methods:   │
│ • ensureVectorTable  │              │ • ensureVectorFile      │
│ • storeVectorInSQL   │              │ • storeVectorInJSON     │
│ • getAllVectorsSQL   │              │ • getAllVectorsJSON     │
│ • countVectorsSQL    │              │ • countVectorsJSON      │
└──────────────────────┘              └─────────────────────────┘
```

## Backend-Specific Interface

```typescript
export interface VectorStorageOperations {
  /**
   * Ensure vector table/collection exists
   * SQLite: CREATE TABLE
   * PostgreSQL: CREATE TABLE
   * MongoDB: create collection/index
   * JSON: ensure directory exists
   */
  ensureVectorStorage(collection: string, dimensions: number): Promise<void>;

  /**
   * Store vector for a record
   */
  storeVector(collection: string, vector: StoredVector): Promise<void>;

  /**
   * Retrieve all vectors from storage
   */
  getAllVectors(collection: string): Promise<StoredVector[]>;

  /**
   * Get vector count
   */
  getVectorCount(collection: string): Promise<number>;
}
```

**Only 4 methods** - backends implement NOTHING else.

## Generic Vector Search Logic (Shared)

All of this is in `VectorSearchAdapterBase` and works for every backend:

### 1. Vector Similarity Search
```typescript
async vectorSearch<T extends RecordData>(
  options: VectorSearchOptions
): Promise<StorageResult<VectorSearchResponse<T>>>
```

**Logic**:
- Generate query embedding (if text provided)
- Fetch all vectors from storage (delegates to `vectorOps.getAllVectors()`)
- Compute cosine similarity for each vector
- Sort by similarity (descending)
- Take top-k results
- Fetch actual records (uses `storageAdapter.read()`)
- Return typed results

**Backend work**: ZERO - just implement `getAllVectors()`

### 2. Embedding Generation
```typescript
async generateEmbedding(
  request: GenerateEmbeddingRequest
): Promise<StorageResult<GenerateEmbeddingResponse>>
```

**Logic**:
- Delegates to `AIProviderDaemon.generateEmbedding()`
- Uses Ollama locally (all-minilm, nomic-embed-text, etc.)
- Returns vector + metadata

**Backend work**: ZERO - AIProviderDaemon handles it

### 3. Vector Indexing
```typescript
async indexVector(
  request: IndexVectorRequest
): Promise<StorageResult<boolean>>
```

**Logic**:
- Ensure vector storage exists (delegates to `vectorOps.ensureVectorStorage()`)
- Store vector (delegates to `vectorOps.storeVector()`)

**Backend work**: Implement `ensureVectorStorage()` and `storeVector()`

### 4. Backfill Vectors
```typescript
async backfillVectors(
  request: BackfillVectorsRequest,
  onProgress?: (progress: BackfillVectorsProgress) => void
): Promise<StorageResult<BackfillVectorsProgress>>
```

**Logic**:
- Query existing records (uses `storageAdapter.query()`)
- For each record:
  - Extract text from specified field
  - Generate embedding
  - Index vector
- Report progress
- Handle batch processing

**Backend work**: ZERO - uses existing query() and delegates to indexVector()

### 5. Index Statistics
```typescript
async getVectorIndexStats(
  collection: string
): Promise<StorageResult<VectorIndexStats>>
```

**Logic**:
- Get total records (uses `storageAdapter.getCollectionStats()`)
- Get vector count (delegates to `vectorOps.getVectorCount()`)
- Get dimensions from first vector

**Backend work**: Implement `getVectorCount()`

## Type Safety

**Strong typing throughout**:
```typescript
// Generic over record data type
interface VectorSearchResult<T extends RecordData> {
  readonly id: UUID;
  readonly data: T;  // ← Fully typed!
  readonly score: number;
  readonly distance: number;
}

// Example usage
const results = await vectorSearch<MemoryEntity>({
  collection: 'memories',
  queryText: 'user prefers examples'
});
// results.results[0].data is typed as MemoryEntity!
```

**No `any` types** - everything is properly typed with generics.

## Adding a New Backend

To add PostgreSQL vector search:

```typescript
// In PostgresStorageAdapter.ts
import { VectorSearchAdapterBase, type VectorStorageOperations, type StoredVector } from './VectorSearchAdapterBase';

export class PostgresStorageAdapter extends SqlStorageAdapterBase {
  private vectorSearch: VectorSearchAdapterBase;

  constructor(/* ... */) {
    super(/* ... */);

    // Initialize vector search with composition
    this.vectorSearch = new VectorSearchAdapterBase(
      this,  // DataStorageAdapter for CRUD
      {      // VectorStorageOperations
        ensureVectorStorage: (coll, dims) => this.ensureVectorTable(coll, dims),
        storeVector: (coll, vec) => this.storeVectorInPostgres(coll, vec),
        getAllVectors: (coll) => this.getVectorsFromPostgres(coll),
        getVectorCount: (coll) => this.countVectorsInPostgres(coll)
      }
    );
  }

  // Implement VectorSearchAdapter interface by delegating
  async vectorSearch<T extends RecordData>(options: VectorSearchOptions) {
    return this.vectorSearch.vectorSearch<T>(options);
  }

  async generateEmbedding(request: GenerateEmbeddingRequest) {
    return this.vectorSearch.generateEmbedding(request);
  }

  // ... delegate other methods ...

  // Only implement 4 backend-specific methods:

  private async ensureVectorTable(collection: string, dimensions: number): Promise<void> {
    // CREATE TABLE with pgvector extension
    await this.runStatement(`
      CREATE TABLE IF NOT EXISTS ${collection}_vectors (
        record_id UUID PRIMARY KEY REFERENCES ${collection}(id) ON DELETE CASCADE,
        embedding vector(${dimensions}) NOT NULL,
        model TEXT,
        generated_at TIMESTAMP NOT NULL
      )
    `);

    // Create vector index for fast similarity search
    await this.runStatement(`
      CREATE INDEX IF NOT EXISTS ${collection}_vectors_embedding_idx
      ON ${collection}_vectors USING ivfflat (embedding vector_cosine_ops)
      WITH (lists = 100)
    `);
  }

  private async storeVectorInPostgres(collection: string, vector: StoredVector): Promise<void> {
    await this.runStatement(
      `INSERT INTO ${collection}_vectors (record_id, embedding, model, generated_at)
       VALUES ($1, $2, $3, $4)
       ON CONFLICT (record_id) DO UPDATE SET embedding = $2, model = $3, generated_at = $4`,
      [vector.recordId, `[${vector.embedding.join(',')}]`, vector.model, vector.generatedAt]
    );
  }

  private async getVectorsFromPostgres(collection: string): Promise<StoredVector[]> {
    const rows = await this.runSql(`SELECT record_id, embedding, model, generated_at FROM ${collection}_vectors`);
    return rows.map(row => ({
      recordId: row.record_id as UUID,
      embedding: JSON.parse(row.embedding as string),
      model: row.model as string | undefined,
      generatedAt: row.generated_at as string
    }));
  }

  private async countVectorsInPostgres(collection: string): Promise<number> {
    const result = await this.runSql(`SELECT COUNT(*) as count FROM ${collection}_vectors`);
    return result[0]?.count || 0;
  }
}
```

**Total code**: ~60 lines for complete vector search support.
**Shared code**: ~400 lines in VectorSearchAdapterBase.

## Benefits

1. **Minimal duplication**: 4 methods per backend vs 400+ lines duplicated
2. **Consistency**: All backends behave identically
3. **Easy testing**: Test generic logic once, not per backend
4. **Backend flexibility**: Easy to swap SQLite → PostgreSQL → Elasticsearch
5. **Type safety**: Full TypeScript inference, no `any` types

## Implementation Status

- [x] VectorSearchTypes (shared interface definitions)
- [x] VectorSearchAdapterBase (generic logic with composition)
- [ ] SqliteStorageAdapter (4 backend-specific methods)
- [ ] Data commands (data/vector-search, data/generate-embedding)
- [ ] PersonaMemory integration (semantic recall)
- [ ] End-to-end test

## Next Steps

1. Implement SqliteStorageAdapter vector methods (4 methods, ~50 lines)
2. Add data commands for vector operations
3. Integrate into PersonaMemory for semantic memory recall
4. Test end-to-end: store memory → embed → recall

---

**Key Principle**: "Architect it, don't hack it. Think what if PostgreSQL was next."

This architecture makes adding PostgreSQL, MongoDB, or Elasticsearch vector search trivial - just 4 methods.
