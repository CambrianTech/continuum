/**
 * SQLite Vector Search Manager
 *
 * Extracted from SqliteStorageAdapter - handles all vector search operations.
 * Uses VectorSearchAdapterBase for generic logic, implements SQLite-specific storage.
 *
 * PATTERN:
 * - Public methods delegate to vectorSearchBase (composition)
 * - Private methods provide SQLite-specific vector storage operations
 * - Maximizes code reuse across SQL/JSON/MongoDB adapters
 *
 * PERFORMANCE:
 * - Embeddings stored as BLOB (Float32Array binary) not JSON TEXT
 * - 720ms → <5ms for 50K vectors (eliminates JSON.parse overhead)
 * - Backward compatible: reads both BLOB and legacy JSON TEXT
 */

import type { SqlExecutor } from '../SqlExecutor';
import { VectorSearchAdapterBase, type VectorStorageOperations, type StoredVector } from '../VectorSearchAdapterBase';
import type {
  VectorSearchAdapter,
  VectorSearchOptions,
  VectorSearchResponse,
  GenerateEmbeddingRequest,
  GenerateEmbeddingResponse,
  IndexVectorRequest,
  BackfillVectorsRequest,
  BackfillVectorsProgress,
  VectorIndexStats,
  VectorSearchCapabilities
} from '../../shared/VectorSearchTypes';
import type { StorageResult, DataStorageAdapter, RecordData } from '../../shared/DataStorageAdapter';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { SqlNamingConverter } from '../../shared/SqlNamingConverter';

/**
 * SQLite Vector Search Manager
 */
export class SqliteVectorSearchManager implements VectorSearchAdapter {
  private vectorSearchBase: VectorSearchAdapterBase;

  constructor(
    private executor: SqlExecutor,
    private storageAdapter: DataStorageAdapter
  ) {
    // Initialize VectorSearchAdapterBase with composition pattern
    const vectorOps: VectorStorageOperations = {
      ensureVectorStorage: (collection, dimensions) => this.ensureVectorTable(collection, dimensions),
      storeVector: (collection, vector) => this.storeVectorInSQLite(collection, vector),
      getAllVectors: (collection) => this.getVectorsFromSQLite(collection),
      getVectorCount: (collection) => this.countVectorsInSQLite(collection)
    };

    this.vectorSearchBase = new VectorSearchAdapterBase(storageAdapter, vectorOps);
  }

  // ============================================================================
  // PUBLIC METHODS - Delegate to VectorSearchAdapterBase
  // ============================================================================

  /**
   * Perform vector similarity search
   * Delegates to VectorSearchAdapterBase which uses the 4 SQLite-specific methods below
   */
  async vectorSearch<T extends RecordData>(
    options: VectorSearchOptions
  ): Promise<StorageResult<VectorSearchResponse<T>>> {
    return this.vectorSearchBase.vectorSearch<T>(options);
  }

  /**
   * Generate embedding for text
   */
  async generateEmbedding(
    request: GenerateEmbeddingRequest
  ): Promise<StorageResult<GenerateEmbeddingResponse>> {
    return this.vectorSearchBase.generateEmbedding(request);
  }

  /**
   * Index vector for a record
   */
  async indexVector(request: IndexVectorRequest): Promise<StorageResult<boolean>> {
    return this.vectorSearchBase.indexVector(request);
  }

  /**
   * Backfill embeddings for existing records
   */
  async backfillVectors(
    request: BackfillVectorsRequest,
    onProgress?: (progress: BackfillVectorsProgress) => void
  ): Promise<StorageResult<BackfillVectorsProgress>> {
    return this.vectorSearchBase.backfillVectors(request, onProgress);
  }

  /**
   * Get vector index statistics
   */
  async getVectorIndexStats(collection: string): Promise<StorageResult<VectorIndexStats>> {
    return this.vectorSearchBase.getVectorIndexStats(collection);
  }

  /**
   * Get vector search capabilities
   */
  async getVectorSearchCapabilities(): Promise<VectorSearchCapabilities> {
    return this.vectorSearchBase.getVectorSearchCapabilities();
  }

  // ============================================================================
  // PRIVATE SQLITE-SPECIFIC VECTOR STORAGE METHODS
  // ============================================================================

  /**
   * Ensure vector table exists for a collection
   * Creates {collection}_vectors table with proper schema
   * Uses BLOB for embeddings (binary Float32Array) for performance
   */
  private async ensureVectorTable(collection: string, dimensions: number): Promise<void> {
    const tableName = `${SqlNamingConverter.toTableName(collection)}_vectors`;
    const baseTableName = SqlNamingConverter.toTableName(collection);

    const sql = `
      CREATE TABLE IF NOT EXISTS \`${tableName}\` (
        record_id TEXT PRIMARY KEY,
        embedding BLOB NOT NULL,
        model TEXT,
        generated_at TEXT NOT NULL,
        FOREIGN KEY (record_id) REFERENCES \`${baseTableName}\`(id) ON DELETE CASCADE
      )
    `;

    await this.executor.runStatement(sql);

    // Create index on record_id for faster lookups
    await this.executor.runStatement(`
      CREATE INDEX IF NOT EXISTS \`${tableName}_record_id_idx\`
      ON \`${tableName}\`(record_id)
    `);

    console.log(`✅ SQLite: Vector table ${tableName} ready (${dimensions} dimensions, BLOB storage)`);
  }

  /**
   * Serialize embedding to BLOB (Float32Array → Buffer)
   * Uses Float32 for ~50% size reduction vs Float64
   * Accepts both number[] and Float32Array inputs
   */
  private embeddingToBlob(embedding: number[] | Float32Array): Buffer {
    // If already Float32Array, use directly; otherwise convert
    const float32 = embedding instanceof Float32Array
      ? embedding
      : new Float32Array(embedding);
    return Buffer.from(float32.buffer);
  }

  /**
   * Deserialize embedding from BLOB or legacy JSON
   *
   * PERFORMANCE: Returns Float32Array directly for BLOB data (zero-copy).
   * This is ~112x faster than JSON.parse and avoids the Array.from() copy.
   * SimilarityMetrics functions work with both number[] and Float32Array.
   *
   * Backward compatible: handles both Buffer (new) and string (legacy)
   */
  private blobToEmbedding(data: Buffer | string): Float32Array | number[] {
    // Legacy JSON string format - must return number[]
    if (typeof data === 'string') {
      return JSON.parse(data) as number[];
    }

    // New BLOB format - return Float32Array directly (zero-copy!)
    // No Array.from() needed - SimilarityMetrics works with Float32Array
    return new Float32Array(
      data.buffer,
      data.byteOffset,
      data.length / Float32Array.BYTES_PER_ELEMENT
    );
  }

  /**
   * Store vector for a record
   * Stores embedding as BLOB (binary Float32Array) for fast retrieval
   */
  private async storeVectorInSQLite(collection: string, vector: StoredVector): Promise<void> {
    const tableName = `${SqlNamingConverter.toTableName(collection)}_vectors`;

    await this.executor.runStatement(
      `INSERT OR REPLACE INTO \`${tableName}\` (record_id, embedding, model, generated_at)
       VALUES (?, ?, ?, ?)`,
      [
        vector.recordId,
        this.embeddingToBlob(vector.embedding),
        vector.model || null,
        vector.generatedAt
      ]
    );
  }

  /**
   * Retrieve all vectors from a collection
   * Decodes BLOB embeddings (or legacy JSON) back to number arrays
   *
   * IMPORTANT: Supports two storage patterns:
   * 1. Separate vector table ({collection}_vectors) - preferred for external indexing
   * 2. Inline embedding column in main table - used by memory consolidation
   */
  private async getVectorsFromSQLite(collection: string): Promise<StoredVector[]> {
    const tableName = `${SqlNamingConverter.toTableName(collection)}_vectors`;
    const baseTableName = SqlNamingConverter.toTableName(collection);

    // First try: Check if separate vector table exists (preferred pattern)
    const vectorTableExists = await this.executor.runSql(
      `SELECT name FROM sqlite_master WHERE type='table' AND name=?`,
      [tableName]
    );

    if (vectorTableExists.length > 0) {
      // Use separate vector table
      const rows = await this.executor.runSql(
        `SELECT record_id, embedding, model, generated_at FROM \`${tableName}\``
      );

      return rows.map(row => ({
        recordId: row.record_id as UUID,
        embedding: this.blobToEmbedding(row.embedding as Buffer | string),
        model: row.model as string | undefined,
        generatedAt: row.generated_at as string
      }));
    }

    // Second try: Check if main table has inline embedding column (memory consolidation pattern)
    const baseTableExists = await this.executor.runSql(
      `SELECT name FROM sqlite_master WHERE type='table' AND name=?`,
      [baseTableName]
    );

    if (baseTableExists.length === 0) {
      return [];  // Table doesn't exist
    }

    // Check if embedding column exists in main table
    const columns = await this.executor.runSql(`PRAGMA table_info(\`${baseTableName}\`)`);
    const hasEmbeddingColumn = columns.some((col: any) => col.name === 'embedding');

    if (!hasEmbeddingColumn) {
      return [];  // No embedding support
    }

    // Read inline embeddings from main table
    const rows = await this.executor.runSql(
      `SELECT id, embedding, created_at FROM \`${baseTableName}\`
       WHERE embedding IS NOT NULL AND embedding != '[]' AND embedding != 'null' AND length(embedding) > 10`
    );

    console.log(`🔍 SqliteVectorSearch: Found ${rows.length} records with inline embeddings in ${baseTableName}`);

    return rows.map(row => ({
      recordId: row.id as UUID,
      embedding: this.blobToEmbedding(row.embedding as Buffer | string),
      model: 'inline',  // Mark as inline embedding
      generatedAt: row.created_at as string
    }));
  }

  /**
   * Get count of vectors in a collection
   * Supports both separate vector table and inline embeddings
   */
  private async countVectorsInSQLite(collection: string): Promise<number> {
    const tableName = `${SqlNamingConverter.toTableName(collection)}_vectors`;
    const baseTableName = SqlNamingConverter.toTableName(collection);

    // First try: Check if separate vector table exists
    const vectorTableExists = await this.executor.runSql(
      `SELECT name FROM sqlite_master WHERE type='table' AND name=?`,
      [tableName]
    );

    if (vectorTableExists.length > 0) {
      const result = await this.executor.runSql(`SELECT COUNT(*) as count FROM \`${tableName}\``);
      return (result[0]?.count as number) || 0;
    }

    // Second try: Count inline embeddings in main table
    const baseTableExists = await this.executor.runSql(
      `SELECT name FROM sqlite_master WHERE type='table' AND name=?`,
      [baseTableName]
    );

    if (baseTableExists.length === 0) {
      return 0;
    }

    // Check if embedding column exists
    const columns = await this.executor.runSql(`PRAGMA table_info(\`${baseTableName}\`)`);
    const hasEmbeddingColumn = columns.some((col: any) => col.name === 'embedding');

    if (!hasEmbeddingColumn) {
      return 0;
    }

    const result = await this.executor.runSql(
      `SELECT COUNT(*) as count FROM \`${baseTableName}\`
       WHERE embedding IS NOT NULL AND embedding != '[]' AND embedding != 'null' AND length(embedding) > 10`
    );
    return (result[0]?.count as number) || 0;
  }
}
