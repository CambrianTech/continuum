/**
 * Web Search Command Types
 *
 * Allows AIs to search the web and get results.
 * Uses search APIs to find relevant information.
 */

import type { JTAGContext, CommandParams, JTAGPayload, CommandResult, CommandInput} from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../../system/core/shared/Commands';

/**
 * Web search parameters
 */
export interface WebSearchParams extends CommandParams {
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

/**
 * WebSearch — Type-safe command executor
 *
 * Usage:
 *   import { WebSearch } from '...shared/WebSearchTypes';
 *   const result = await WebSearch.execute({ ... });
 */
export const WebSearch = {
  execute(params: CommandInput<WebSearchParams>): Promise<WebSearchResult> {
    return Commands.execute<WebSearchParams, WebSearchResult>('interface/web/search', params as Partial<WebSearchParams>);
  },
  commandName: 'interface/web/search' as const,
} as const;
