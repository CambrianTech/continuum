/**
 * RAG Query Fetch - Server Implementation
 *
 * Fetches results from an open query handle with bidirectional navigation
 */

import { RagQueryFetchCommand } from '../shared/RagQueryFetchCommand';
import type { RagQueryFetchParams, RagQueryFetchResult } from '../shared/RagQueryFetchTypes';
import { normalizeRagQueryFetchParams } from '../shared/RagQueryFetchTypes';
import type { JTAGContext } from '../../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import { getQueryHandle, updateQueryHandleOffset } from '../../query-open/server/RagQueryOpenServerCommand';

export class RagQueryFetchServerCommand extends RagQueryFetchCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/rag/query-fetch', context, subpath, commander);
  }

  async execute(params: RagQueryFetchParams): Promise<RagQueryFetchResult> {
    try {
      // Normalize and validate parameters
      const normalized = normalizeRagQueryFetchParams(params);

      console.log('📄 RAG Query Fetch: Fetching results', {
        queryHandle: params.queryHandle,
        offset: normalized.offset,
        limit: normalized.limit,
        direction: normalized.direction
      });

      // Get the query handle
      const handle = getQueryHandle(params.queryHandle);

      if (!handle) {
        return {
          success: false,
          error: `Query handle not found: ${params.queryHandle}`,
          offset: 0,
          limit: normalized.limit ?? 10,
          totalMatches: 0,
          results: [],
          hasMore: false,
          hasPrevious: false,
          context: this.context,
          sessionId: params.sessionId
        };
      }

      // Determine target offset
      let targetOffset: number;
      const fetchLimit = normalized.limit ?? handle.pageSize;

      if (normalized.offset !== undefined) {
        // Absolute positioning
        targetOffset = normalized.offset;
      } else if (normalized.direction) {
        // Relative positioning
        if (normalized.direction === 'forward') {
          targetOffset = handle.currentOffset + fetchLimit;
        } else {
          targetOffset = Math.max(0, handle.currentOffset - fetchLimit);
        }
      } else {
        // No offset or direction provided - use current position
        targetOffset = handle.currentOffset;
      }

      // Clamp offset to valid range
      targetOffset = Math.max(0, Math.min(targetOffset, handle.allResults.length));

      // Update handle's current offset
      updateQueryHandleOffset(params.queryHandle, targetOffset);

      // Fetch results at target offset
      const endIndex = Math.min(targetOffset + fetchLimit, handle.allResults.length);
      const results = handle.allResults.slice(targetOffset, endIndex);

      // Calculate navigation flags
      const hasMore = endIndex < handle.allResults.length;
      const hasPrevious = targetOffset > 0;

      console.log(`📄 Fetched ${results.length} results at offset ${targetOffset}/${handle.allResults.length}`);

      return {
        success: true,
        offset: targetOffset,
        limit: fetchLimit,
        totalMatches: handle.allResults.length,
        results,
        hasMore,
        hasPrevious,
        context: this.context,
        sessionId: params.sessionId
      };

    } catch (error) {
      console.error('❌ RAG Query Fetch failed:', error);

      // Try to normalize params for error response
      let limit = 10;
      try {
        const normalized = normalizeRagQueryFetchParams(params);
        limit = normalized.limit ?? 10;
      } catch {
        // Use default
      }

      return {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error',
        offset: 0,
        limit,
        totalMatches: 0,
        results: [],
        hasMore: false,
        hasPrevious: false,
        context: this.context,
        sessionId: params.sessionId
      };
    }
  }
}
