/**
 * Web Search Command Types
 *
 * Allows AIs to search the web and get results.
 * Uses search APIs to find relevant information.
 */

import type { JTAGContext, JTAGPayload, CommandResult } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Web search parameters
 */
export interface WebSearchParams extends JTAGPayload {
  readonly context: JTAGContext;
  readonly sessionId: UUID;

  /**
   * Search query
   */
  query: string;

  /**
   * Maximum number of results to return (default: 10)
   */
  maxResults?: number;

  /**
   * Filter results to specific domains (optional)
   */
  domains?: string[];
}

/**
 * Single search result
 */
export interface SearchResult {
  /** Page title */
  title: string;

  /** Page URL */
  url: string;

  /** Snippet/description */
  snippet: string;

  /** Domain */
  domain: string;
}

/**
 * Web search result
 */
export interface WebSearchResult extends CommandResult {
  readonly context: JTAGContext;
  readonly sessionId: UUID;
  readonly success: boolean;

  /** Search query that was executed */
  readonly query: string;

  /** Array of search results */
  readonly results: SearchResult[];

  /** Total results found */
  readonly totalResults: number;

  readonly error?: string;
}

/**
 * Create WebSearchResult from WebSearchParams (type-safe factory)
 */
export function createWebSearchResultFromParams(
  params: WebSearchParams,
  data: Partial<Omit<WebSearchResult, 'context' | 'sessionId'>>
): WebSearchResult {
  return {
    context: params.context,
    sessionId: params.sessionId,
    success: data.success ?? false,
    query: data.query ?? params.query,
    results: data.results ?? [],
    totalResults: data.totalResults ?? 0,
    error: data.error
  };
}
