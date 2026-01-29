/**
 * RAG Query Open - Server Implementation
 *
 * Opens a semantic similarity search and returns a handle for iteration
 */

import { RagQueryOpenCommand } from '../shared/RagQueryOpenCommand';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { RagQueryOpenParams, RagQueryOpenResult, CodeSearchResult } from '../shared/RagQueryOpenTypes';
import { normalizeRagQueryOpenParams } from '../shared/RagQueryOpenTypes';
import type { JTAGContext } from '../../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import { Commands } from '../../../../../system/core/shared/Commands';
import type { EmbeddingGenerateResult } from '../../../embedding/generate/shared/EmbeddingGenerateTypes';
import type { UUID } from '../../../../../system/core/types/CrossPlatformUUID';
import { v4 as uuidv4 } from 'uuid';
import type { CodeIndexEntity } from '../../../../../system/data/entities/CodeIndexEntity';
import type { DataListParams, DataListResult } from '../../../../data/list/shared/DataListTypes';
import type { BaseEntity } from '../../../../../system/data/entities/BaseEntity';

import { EmbeddingGenerate } from '../../../embedding/generate/shared/EmbeddingGenerateTypes';
import { DataList } from '../../../../data/list/shared/DataListTypes';
/**
 * Query handle state stored in memory
 * In production this would be persisted to database with TTL
 */
interface QueryHandle {
  queryHandle: UUID;
  query: string;
  queryEmbedding: number[];
  embeddingModel: string;
  fileType?: string;
  exportType?: string;
  minRelevance: number;
  pageSize: number;
  currentOffset: number;
  allResults: CodeSearchResult[];  // Cached sorted results
  createdAt: Date;
}

// In-memory handle storage (future: move to database with TTL)
const activeHandles = new Map<UUID, QueryHandle>();

export class RagQueryOpenServerCommand extends RagQueryOpenCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/rag/query-open', context, subpath, commander);
  }

  async execute(params: RagQueryOpenParams): Promise<RagQueryOpenResult> {
    try {
      // Normalize and validate parameters (handles CLI string params)
      const { pageSize, minRelevance, embeddingModel } = normalizeRagQueryOpenParams(params);

      console.log('🔎 RAG Query Open: Starting semantic search', {
        query: params.query,
        embeddingModel,
        pageSize,
        minRelevance
      });

      // Step 1: Get or generate query embedding
      let queryEmbedding: number[];

      if (params.queryEmbedding) {
        queryEmbedding = params.queryEmbedding;
        console.log('🔎 Using provided embedding');
      } else {
        // Generate embedding for query
        const embeddingResult = await EmbeddingGenerate.execute({
          input: params.query,
          model: embeddingModel,
          context: this.context,
          sessionId: params.sessionId
        }) as EmbeddingGenerateResult;

        if (!embeddingResult.success || !embeddingResult.embeddings || embeddingResult.embeddings.length === 0) {
          return {
            success: false,
            error: embeddingResult.error || 'Failed to generate query embedding',
            queryHandle: '' as UUID,
            query: params.query,
            embeddingModel,
            totalMatches: 0,
            offset: 0,
            limit: pageSize,
            results: [],
            hasMore: false,
            context: this.context,
            sessionId: params.sessionId
          };
        }

        queryEmbedding = embeddingResult.embeddings[0];
        console.log('🔎 Generated embedding:', {
          model: embeddingModel,
          dimensions: queryEmbedding.length
        });
      }

      // Step 2: Fetch all indexed entries from database
      // TODO: Add filters for fileType, exportType when fetching
      const listResult = await DataList.execute({
        collection: 'code_index',
        orderBy: [{ field: 'lastIndexed', direction: 'desc' }],
        context: this.context,
        sessionId: params.sessionId
      });

      if (!listResult.success || !listResult.items) {
        return {
          success: false,
          error: 'Failed to fetch code index entries',
          queryHandle: '' as UUID,
          query: params.query,
          embeddingModel,
          totalMatches: 0,
          offset: 0,
          limit: pageSize,
          results: [],
          hasMore: false,
          context: this.context,
          sessionId: params.sessionId
        };
      }

      console.log(`🔎 Fetched ${listResult.items.length} indexed entries`);

      // Step 3: Calculate cosine similarity for each entry
      const scoredResults: CodeSearchResult[] = [];

      for (const entry of listResult.items as readonly CodeIndexEntity[]) {
        // Skip entries without embeddings
        if (!entry.embedding || entry.embedding.length === 0) {
          continue;
        }

        // Apply filters
        if (params.fileType && entry.fileType !== params.fileType) {
          continue;
        }
        if (params.exportType && entry.exportType !== params.exportType) {
          continue;
        }

        // Calculate cosine similarity
        const similarity = this.cosineSimilarity(queryEmbedding, entry.embedding);

        // Only include results above relevance threshold
        if (similarity >= minRelevance) {
          scoredResults.push({
            entry,
            relevanceScore: similarity
          });
        }
      }

      // Step 4: Sort by relevance (highest first)
      scoredResults.sort((a, b) => b.relevanceScore - a.relevanceScore);

      console.log(`🔎 Found ${scoredResults.length} matches above threshold ${minRelevance}`);

      // Step 5: Create query handle
      const queryHandle = uuidv4() as UUID;

      const handle: QueryHandle = {
        queryHandle,
        query: params.query,
        queryEmbedding,
        embeddingModel,
        fileType: params.fileType,
        exportType: params.exportType,
        minRelevance,
        pageSize,
        currentOffset: 0,
        allResults: scoredResults,
        createdAt: new Date()
      };

      activeHandles.set(queryHandle, handle);
      console.log(`🔎 Created query handle: ${queryHandle}`);

      // Step 6: Return first page
      const firstPage = scoredResults.slice(0, pageSize);
      const hasMore = scoredResults.length > pageSize;

      return {
        success: true,
        queryHandle,
        query: params.query,
        embeddingModel,
        totalMatches: scoredResults.length,
        offset: 0,
        limit: pageSize,
        results: firstPage,
        hasMore,
        context: this.context,
        sessionId: params.sessionId
      };

    } catch (error) {
      console.error('❌ RAG Query Open failed:', error);

      // Try to get normalized params for error response, fallback to defaults
      let pageSize = 10;
      let embeddingModel = 'unknown';
      try {
        const normalized = normalizeRagQueryOpenParams(params);
        pageSize = normalized.pageSize;
        embeddingModel = normalized.embeddingModel;
      } catch {
        // Use defaults if normalization fails
      }

      return {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error',
        queryHandle: '' as UUID,
        query: params.query,
        embeddingModel,
        totalMatches: 0,
        offset: 0,
        limit: pageSize,
        results: [],
        hasMore: false,
        context: this.context,
        sessionId: params.sessionId
      };
    }
  }

  /**
   * Calculate cosine similarity between two vectors
   * Returns value between -1 and 1 (higher = more similar)
   */
  private cosineSimilarity(a: number[], b: number[]): number {
    if (a.length !== b.length) {
      throw new Error(`Vector dimension mismatch: ${a.length} vs ${b.length}`);
    }

    let dotProduct = 0;
    let normA = 0;
    let normB = 0;

    for (let i = 0; i < a.length; i++) {
      dotProduct += a[i] * b[i];
      normA += a[i] * a[i];
      normB += b[i] * b[i];
    }

    const denominator = Math.sqrt(normA) * Math.sqrt(normB);

    if (denominator === 0) {
      return 0;
    }

    return dotProduct / denominator;
  }
}

/**
 * Get active query handle (used by query-fetch command)
 */
export function getQueryHandle(handleId: UUID): QueryHandle | undefined {
  return activeHandles.get(handleId);
}

/**
 * Update query handle offset (used by query-fetch command)
 */
export function updateQueryHandleOffset(handleId: UUID, newOffset: number): void {
  const handle = activeHandles.get(handleId);
  if (handle) {
    handle.currentOffset = newOffset;
  }
}

/**
 * Close and cleanup query handle (used by query-close command)
 */
export function closeQueryHandle(handleId: UUID): boolean {
  return activeHandles.delete(handleId);
}
