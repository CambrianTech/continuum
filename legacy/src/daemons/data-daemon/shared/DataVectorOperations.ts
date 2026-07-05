/**
 * DataVectorOperations — Vector search facade for DataDaemon
 *
 * Extracted from DataDaemon. Provides a static interface for vector search,
 * embedding generation, indexing, backfilling, and capability queries.
 *
 * All methods delegate to the VectorCapableAdapter on the active storage adapter.
 */

import type {
  DataStorageAdapter,
  StorageResult,
  RecordData,
} from './DataStorageAdapter';

import type {
  VectorSearchOptions,
  VectorSearchResponse,
  GenerateEmbeddingRequest,
  GenerateEmbeddingResponse,
  IndexVectorRequest,
  BackfillVectorsRequest,
  BackfillVectorsProgress,
  VectorIndexStats,
  VectorSearchCapabilities,
} from './VectorSearchTypes';

import { DataSchemaManager } from './DataSchemaManager';

/**
 * Optional vector search methods that some storage adapters may support.
 * Used for runtime capability checking on the adapter.
 */
export interface VectorCapableAdapter extends DataStorageAdapter {
  vectorSearch?(options: VectorSearchOptions): Promise<StorageResult<VectorSearchResponse>>;
  generateEmbedding?(request: GenerateEmbeddingRequest): Promise<StorageResult<GenerateEmbeddingResponse>>;
  indexVector?(request: IndexVectorRequest): Promise<StorageResult<boolean>>;
  backfillVectors?(request: BackfillVectorsRequest, onProgress?: (progress: BackfillVectorsProgress) => void): Promise<StorageResult<BackfillVectorsProgress>>;
  getVectorIndexStats?(collection: string): Promise<StorageResult<VectorIndexStats>>;
  getVectorSearchCapabilities?(): Promise<VectorSearchCapabilities | null>;
}

export class DataVectorOperations {
  constructor(
    private readonly getAdapter: () => DataStorageAdapter,
    private readonly getAdapterForCollection: (collection: string) => DataStorageAdapter,
    private readonly schemaManager: DataSchemaManager,
  ) {}

  async vectorSearch<T extends RecordData>(
    options: VectorSearchOptions
  ): Promise<StorageResult<VectorSearchResponse<T>>> {
    await this.schemaManager.ensureSchema(options.collection, this.getAdapterForCollection(options.collection));

    const adapter = this.getAdapter() as VectorCapableAdapter;
    if (!adapter.vectorSearch) {
      return {
        success: false,
        error: 'Current storage adapter does not support vector search'
      };
    }

    return await adapter.vectorSearch(options) as StorageResult<VectorSearchResponse<T>>;
  }

  async generateEmbedding(
    request: GenerateEmbeddingRequest
  ): Promise<StorageResult<GenerateEmbeddingResponse>> {
    const adapter = this.getAdapter() as VectorCapableAdapter;
    if (!adapter.generateEmbedding) {
      return {
        success: false,
        error: 'Current storage adapter does not support embedding generation'
      };
    }

    return await adapter.generateEmbedding(request);
  }

  async indexVector(
    request: IndexVectorRequest
  ): Promise<StorageResult<boolean>> {
    const adapter = this.getAdapter() as VectorCapableAdapter;
    if (!adapter.indexVector) {
      return {
        success: false,
        error: 'Current storage adapter does not support vector indexing'
      };
    }

    return await adapter.indexVector(request);
  }

  async backfillVectors(
    request: BackfillVectorsRequest,
    onProgress?: (progress: BackfillVectorsProgress) => void
  ): Promise<StorageResult<BackfillVectorsProgress>> {
    const adapter = this.getAdapter() as VectorCapableAdapter;
    if (!adapter.backfillVectors) {
      return {
        success: false,
        error: 'Current storage adapter does not support vector backfilling'
      };
    }

    return await adapter.backfillVectors(request, onProgress);
  }

  async getVectorIndexStats(
    collection: string
  ): Promise<StorageResult<VectorIndexStats>> {
    const adapter = this.getAdapter() as VectorCapableAdapter;
    if (!adapter.getVectorIndexStats) {
      return {
        success: false,
        error: 'Current storage adapter does not support vector index stats'
      };
    }

    return await adapter.getVectorIndexStats(collection);
  }

  async getVectorSearchCapabilities(): Promise<VectorSearchCapabilities | null> {
    const adapter = this.getAdapter() as VectorCapableAdapter;
    if (!adapter.getVectorSearchCapabilities) {
      return null;
    }

    return await adapter.getVectorSearchCapabilities();
  }
}
