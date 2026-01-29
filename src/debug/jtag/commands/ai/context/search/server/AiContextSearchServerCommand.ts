/**
 * Ai Context Search Command - Server Implementation
 *
 * Semantic context navigation - search ANY BaseEntity collection
 * using cosine similarity via Rust embedding worker (ONNX, ~5ms per embedding).
 *
 * This is the primary tool for LLMs to navigate large contexts programmatically
 * instead of having everything stuffed into the prompt.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { AiContextSearchParams, AiContextSearchResult, ContextSearchItem, CollectionName } from '../shared/AiContextSearchTypes';
import { createAiContextSearchResultFromParams } from '../shared/AiContextSearchTypes';
import { Commands } from '@system/core/shared/Commands';
import type { VectorSearchParams, VectorSearchResult_CLI } from '@commands/data/vector-search/shared/VectorSearchCommandTypes';

// Default collections that typically have semantic content
const DEFAULT_COLLECTIONS: CollectionName[] = [
  'chat_messages',
  'memories',
  'timeline_events'
];

export class AiContextSearchServerCommand extends CommandBase<AiContextSearchParams, AiContextSearchResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/context/search', context, subpath, commander);
  }

  async execute(params: AiContextSearchParams): Promise<AiContextSearchResult> {
    const startTime = Date.now();

    // Validate required parameters
    if (!params.query || params.query.trim() === '') {
      throw new ValidationError(
        'query',
        `Missing required parameter 'query'. Provide a natural language description of what you're looking for.`
      );
    }

    const collections = params.collections || DEFAULT_COLLECTIONS;
    const limit = Math.min(params.limit || 10, 50);
    const minSimilarity = params.minSimilarity ?? 0.5;
    const mode = (params.mode as 'semantic' | 'keyword' | 'hybrid') || 'semantic';

    console.debug(`🔍 CONTEXT-SEARCH: "${params.query.slice(0, 50)}..." collections=${collections.join(',')}`);

    try {
      // Search across requested collections in parallel
      const searchPromises = collections.map(collection => this.searchCollection(
        collection,
        params.query,
        {
          personaId: params.personaId,
          excludeContextId: params.excludeContextId,
          limit: Math.ceil(limit / collections.length) + 5, // Over-fetch then trim
          minSimilarity,
          since: params.since,
          mode
        }
      ));

      const searchResults = await Promise.all(searchPromises);

      // Merge and sort by score
      const allItems: ContextSearchItem[] = [];
      for (const result of searchResults) {
        allItems.push(...result);
      }

      // Sort by score descending, take top limit
      allItems.sort((a, b) => b.score - a.score);
      const items = allItems.slice(0, limit);

      const durationMs = Date.now() - startTime;
      console.debug(`✅ CONTEXT-SEARCH: Found ${items.length} results in ${durationMs}ms`);

      return createAiContextSearchResultFromParams(params, {
        success: true,
        items,
        totalMatches: allItems.length,
        durationMs
      });

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ CONTEXT-SEARCH: Failed:`, errorMessage);
      throw new Error(`Context search failed: ${errorMessage}`);
    }
  }

  private async searchCollection(
    collection: CollectionName,
    query: string,
    options: {
      personaId?: string;
      excludeContextId?: string;
      limit: number;
      minSimilarity: number;
      since?: string;
      mode: 'semantic' | 'keyword' | 'hybrid';
    }
  ): Promise<ContextSearchItem[]> {
    try {
      // Build filter
      const filter: Record<string, any> = {};

      if (options.personaId) {
        filter.personaId = options.personaId;
      }

      if (options.excludeContextId) {
        filter.contextId = { $ne: options.excludeContextId };
      }

      if (options.since) {
        // Use createdAt as it's on BaseEntity
        filter.createdAt = { $gte: options.since };
      }

      // Use data/vector-search command (delegates to Rust embedding worker)
      const result = await Commands.execute<VectorSearchParams, VectorSearchResult_CLI>('data/vector-search', {
        collection,
        queryText: query,
        k: options.limit,
        similarityThreshold: options.minSimilarity,
        filter,
        hybridMode: options.mode
      });

      if (!result.success || !result.results) {
        return [];
      }

      // Map to ContextSearchItem
      return result.results.map((r: any) => this.mapToContextItem(r, collection));

    } catch (error) {
      console.warn(`Context search failed for ${collection}: ${error}`);
      return [];
    }
  }

  /**
   * Generic content extraction from any entity
   * Uses common patterns: content, text, content.text, description, name
   */
  private mapToContextItem(
    result: { data: any; score: number },
    collection: CollectionName
  ): ContextSearchItem {
    const data = result.data;

    // Generic content extraction - try common fields
    const content = this.extractContent(data);
    const source = this.extractSource(data, collection);

    // Truncate content for display
    const maxContentLength = 300;
    const truncatedContent = content.length > maxContentLength
      ? content.slice(0, maxContentLength) + '...'
      : content;

    return {
      id: data.id,
      collection,
      content: truncatedContent,
      score: result.score,
      timestamp: data.timestamp || data.createdAt || new Date().toISOString(),
      source,
      metadata: data.metadata || data
    };
  }

  /**
   * Extract text content from entity - tries common field patterns
   */
  private extractContent(data: any): string {
    // Try common content field patterns
    if (data.content?.text) return data.content.text;
    if (typeof data.content === 'string') return data.content;
    if (data.text) return data.text;
    if (data.description) return data.description;
    if (data.message) return data.message;
    if (data.body) return data.body;
    if (data.name) return data.name;
    if (data.title) return data.title;

    // Fallback: stringify first few fields
    const keys = Object.keys(data).filter(k => !['id', 'createdAt', 'updatedAt', 'version', 'metadata'].includes(k));
    const preview = keys.slice(0, 3).map(k => `${k}: ${String(data[k]).slice(0, 50)}`).join(', ');
    return preview || '[No content]';
  }

  /**
   * Extract source/context info from entity
   */
  private extractSource(data: any, collection: CollectionName): string {
    // Try common source field patterns
    if (data.roomId) return `Room: ${data.roomId.slice(0, 8)}`;
    if (data.contextId) return `Context: ${data.contextId.slice(0, 8)}`;
    if (data.contextName) return data.contextName;
    if (data.type) return `${collection}: ${data.type}`;
    if (data.category) return `${collection}: ${data.category}`;

    // Fallback: just use collection name
    return collection;
  }
}
