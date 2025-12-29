/**
 * Web Fetch Command Types
 *
 * Allows AIs to fetch and read web pages.
 * Returns clean text content from HTML pages.
 */

import type { JTAGContext, CommandParams, JTAGPayload, CommandResult } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Web fetch parameters
 */
export interface WebFetchParams extends CommandParams {
  readonly context: JTAGContext;
  readonly sessionId: UUID;

  /**
   * URL to fetch
   */
  url: string;

  /**
   * Return format (default: 'text')
   * - 'text': Clean text content only
   * - 'html': Full HTML
   * - 'markdown': Convert to markdown
   */
  format?: 'text' | 'html' | 'markdown';

  /**
   * Maximum content length in characters (default: 50000)
   * Prevents fetching huge pages
   */
  maxLength?: number;

  /**
   * Headers to forward (browser passes its headers to server)
   * Server uses these to look like the user's actual browser
   */
  headers?: Record<string, string>;
}

/**
 * Web fetch result
 */
export interface WebFetchResult extends CommandResult {
  readonly context: JTAGContext;
  readonly sessionId: UUID;
  readonly success: boolean;

  /** URL that was fetched */
  readonly url: string;

  /** Page title */
  readonly title?: string;

  /** Page content in requested format */
  readonly content: string;

  /** Content length */
  readonly contentLength: number;

  /** Content type from response */
  readonly contentType?: string;

  /** Final URL (after redirects) */
  readonly finalUrl?: string;

  readonly error?: string;
}

/**
 * Create WebFetchResult from WebFetchParams (type-safe factory)
 */
export function createWebFetchResultFromParams(
  params: WebFetchParams,
  data: Partial<Omit<WebFetchResult, 'context' | 'sessionId'>>
): WebFetchResult {
  return {
    context: params.context,
    sessionId: params.sessionId,
    success: data.success ?? false,
    url: data.url ?? params.url,
    title: data.title,
    content: data.content ?? '',
    contentLength: data.contentLength ?? 0,
    contentType: data.contentType,
    finalUrl: data.finalUrl,
    error: data.error
  };
}
