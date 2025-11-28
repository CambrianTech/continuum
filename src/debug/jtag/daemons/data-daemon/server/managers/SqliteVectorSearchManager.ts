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
 */

import type { SqliteRawExecutor } from '../SqliteRawExecutor';
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
    private executor: SqliteRawExecutor,
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
   */
  private async ensureVectorTable(collection: string, dimensions: number): Promise<void> {
    const tableName = `${SqlNamingConverter.toTableName(collection)}_vectors`;
    const baseTableName = SqlNamingConverter.toTableName(collection);

    const sql = `
      CREATE TABLE IF NOT EXISTS \`${tableName}\` (
        record_id TEXT PRIMARY KEY,
        embedding TEXT NOT NULL,
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

    console.log(`✅ SQLite: Vector table ${tableName} ready (${dimensions} dimensions)`);
  }

  /**
   * Store vector for a record
   * Stores embedding as JSON text (SQLite doesn't have native array type)
   */
  private async storeVectorInSQLite(collection: string, vector: StoredVector): Promise<void> {
    const tableName = `${SqlNamingConverter.toTableName(collection)}_vectors`;

    await this.executor.runStatement(
      `INSERT OR REPLACE INTO \`${tableName}\` (record_id, embedding, model, generated_at)
       VALUES (?, ?, ?, ?)`,
      [
        vector.recordId,
        JSON.stringify(vector.embedding),
        vector.model || null,
        vector.generatedAt
      ]
    );
  }

  /**
   * Retrieve all vectors from a collection
   * Parses JSON embeddings back to number arrays
   */
  private async getVectorsFromSQLite(collection: string): Promise<StoredVector[]> {
    const tableName = `${SqlNamingConverter.toTableName(collection)}_vectors`;

    // Check if table exists
    const tableExists = await this.executor.runSql(
      `SELECT name FROM sqlite_master WHERE type='table' AND name=?`,
      [tableName]
    );

    if (tableExists.length === 0) {
      return [];  // No vectors yet
    }

    const rows = await this.executor.runSql(`SELECT record_id, embedding, model, generated_at FROM \`${tableName}\``);

    return rows.map(row => ({
      recordId: row.record_id as UUID,
      embedding: JSON.parse(row.embedding as string) as number[],
      model: row.model as string | undefined,
      generatedAt: row.generated_at as string
    }));
  }

  /**
   * Get count of vectors in a collection
   */
  private async countVectorsInSQLite(collection: string): Promise<number> {
    const tableName = `${SqlNamingConverter.toTableName(collection)}_vectors`;

    // Check if table exists
    const tableExists = await this.executor.runSql(
      `SELECT name FROM sqlite_master WHERE type='table' AND name=?`,
      [tableName]
    );

    if (tableExists.length === 0) {
      return 0;
    }

    const result = await this.executor.runSql(`SELECT COUNT(*) as count FROM \`${tableName}\``);
    return (result[0]?.count as number) || 0;
  }
}
