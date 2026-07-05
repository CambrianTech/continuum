import type { CommandParams, CommandResult, CommandInput} from '@system/core/types/JTAGTypes';
import { Commands } from '../../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Searches across all project documentation files for lines matching a text pattern, returning matching lines with their document name, line number, and content.
 */
export interface DocsSearchParams extends CommandParams {
  pattern: string;
  caseSensitive?: boolean;
  maxMatches?: number;
}

export interface DocsSearchResult extends CommandResult {
  success: boolean;
  error?: string;
  pattern: string;
  matches: Array<{ doc: string; lineNumber: number; content: string }>;
  totalMatches: number;
}

/**
 * DocsSearch — Type-safe command executor
 *
 * Usage:
 *   import { DocsSearch } from '...shared/DocsSearchTypes';
 *   const result = await DocsSearch.execute({ ... });
 */
export const DocsSearch = {
  execute(params: CommandInput<DocsSearchParams>): Promise<DocsSearchResult> {
    return Commands.execute<DocsSearchParams, DocsSearchResult>('utilities/docs/search', params as Partial<DocsSearchParams>);
  },
  commandName: 'utilities/docs/search' as const,
} as const;

/**
 * Factory function for creating UtilitiesDocsSearchParams
 */
export const createUtilitiesDocsSearchParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DocsSearchParams, 'context' | 'sessionId' | 'userId'>
): DocsSearchParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating UtilitiesDocsSearchResult with defaults
 */
export const createUtilitiesDocsSearchResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DocsSearchResult, 'context' | 'sessionId' | 'userId'>
): DocsSearchResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart utilities/docs/search-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createUtilitiesDocsSearchResultFromParams = (
  params: DocsSearchParams,
  differences: Omit<DocsSearchResult, 'context' | 'sessionId' | 'userId'>
): DocsSearchResult => transformPayload(params, differences);

