/**
 * Vector Search Command - Server Implementation
 *
 * Performs semantic search over collections using vector similarity.
 * Delegates to DataDaemon which uses VectorSearchAdapterBase for backend-agnostic implementation.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { VectorSearchParams, VectorSearchResult_CLI } from '../shared/VectorSearchCommandTypes';
import { createVectorSearchResultFromParams } from '../shared/VectorSearchCommandTypes';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import type { RecordData } from '../../../../daemons/data-daemon/shared/DataStorageAdapter';
import { DEFAULT_EMBEDDING_MODELS } from '../../../../daemons/data-daemon/shared/VectorSearchTypes';

const DEFAULT_CONFIG = {
  vectorSearch: {
    defaultK: 10,
    maxK: 100,
    defaultSimilarityThreshold: 0.0,
    defaultEmbeddingModel: 'all-minilm',
    defaultProvider: 'ollama'
  }
} as const;

export class VectorSearchServerCommand extends CommandBase<VectorSearchParams, VectorSearchResult_CLI> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-vector-search', context, subpath, commander);
  }

  async execute(params: VectorSearchParams): Promise<VectorSearchResult_CLI> {
    const startTime = Date.now();
    console.debug(`🔍 VECTOR-SEARCH: Searching collection "${params.collection}"`);

    try {
      // Validate required parameters
      if (!params.queryText && !params.queryVector) {
        return createVectorSearchResultFromParams(params, {
          success: false,
          error: 'Must provide either queryText or queryVector'
        });
      }

      // Prepare embedding model
      const modelName = params.embeddingModel || DEFAULT_CONFIG.vectorSearch.defaultEmbeddingModel;
      const providerName = params.embeddingProvider || DEFAULT_CONFIG.vectorSearch.defaultProvider;

      const embeddingModel = DEFAULT_EMBEDDING_MODELS[modelName] || {
        ...DEFAULT_EMBEDDING_MODELS['all-minilm'],
        name: modelName,
        provider: providerName as any
      };

      // Prepare search options
      const k = Math.min(params.k || DEFAULT_CONFIG.vectorSearch.defaultK, DEFAULT_CONFIG.vectorSearch.maxK);
      const similarityThreshold = params.similarityThreshold ?? DEFAULT_CONFIG.vectorSearch.defaultSimilarityThreshold;

      console.debug(`🔍 VECTOR-SEARCH: k=${k}, threshold=${similarityThreshold}, model=${embeddingModel.name}`);

      // Execute vector search via DataDaemon
      const searchResult = await DataDaemon.vectorSearch<RecordData>({
        collection: params.collection,
        queryText: params.queryText,
        queryVector: params.queryVector,
        k,
        similarityThreshold,
        hybridMode: params.hybridMode || 'semantic',
        filter: params.filter,
        embeddingModel
      });

      if (!searchResult.success || !searchResult.data) {
        console.error(`❌ VECTOR-SEARCH: Failed for collection "${params.collection}":`, searchResult.error);
        return createVectorSearchResultFromParams(params, {
          success: false,
          error: searchResult.error || 'Vector search failed'
        });
      }

      const queryTime = Date.now() - startTime;
      console.debug(
        `✅ VECTOR-SEARCH: Found ${searchResult.data.totalResults} results in ${queryTime}ms`,
        `(top score: ${searchResult.data.results[0]?.score.toFixed(3) || 'N/A'})`
      );

      return createVectorSearchResultFromParams(params, {
        success: true,
        results: searchResult.data.results,
        totalResults: searchResult.data.totalResults,
        queryVector: searchResult.data.queryVector,
        metadata: {
          ...searchResult.data.metadata,
          queryTime
        }
      });

    } catch (error: unknown) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ VECTOR-SEARCH: Execution failed for "${params.collection}":`, errorMessage);
      return createVectorSearchResultFromParams(params, {
        success: false,
        error: `Vector search failed: ${errorMessage}`
      });
    }
  }
}
