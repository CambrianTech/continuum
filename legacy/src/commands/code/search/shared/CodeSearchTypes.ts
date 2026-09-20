/**
 * Code Search Command - Shared Types
 *
 * Search for a regex pattern across workspace files. Respects .gitignore, supports glob-based file filtering. Returns matching lines with context.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { SearchMatch } from '@shared/generated/code/SearchMatch';

/**
 * Code Search Command Parameters
 */
export interface CodeSearchParams extends CommandParams {
  // Regex pattern to search for
  pattern: string;
  // Glob pattern to filter files (e.g., '*.ts', 'src/**/*.rs')
  fileGlob?: string;
  // Maximum number of matches to return (default: 100)
  maxResults?: number;
}

/**
 * Factory function for creating CodeSearchParams
 */
export const createCodeSearchParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Regex pattern to search for
    pattern: string;
    // Glob pattern to filter files (e.g., '*.ts', 'src/**/*.rs')
    fileGlob?: string;
    // Maximum number of matches to return (default: 100)
    maxResults?: number;
  }
): CodeSearchParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  fileGlob: data.fileGlob ?? '',
  maxResults: data.maxResults ?? 0,
  ...data
});

/**
 * Code Search Command Result
 */
export interface CodeSearchResult extends CommandResult {
  success: boolean;
  // Search matches from Rust (generated type via ts-rs)
  matches: SearchMatch[];
  // Total number of matches found
  totalMatches: number;
  // Number of files searched
  filesSearched: number;
  error?: JTAGError;
}

/**
 * Factory function for creating CodeSearchResult with defaults
 */
export const createCodeSearchResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Search matches from Rust (generated type via ts-rs)
    matches?: SearchMatch[];
    // Total number of matches found
    totalMatches?: number;
    // Number of files searched
    filesSearched?: number;
    error?: JTAGError;
  }
): CodeSearchResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  matches: data.matches ?? [],
  totalMatches: data.totalMatches ?? 0,
  filesSearched: data.filesSearched ?? 0,
  ...data
});

/**
 * Smart Code Search-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createCodeSearchResultFromParams = (
  params: CodeSearchParams,
  differences: Omit<CodeSearchResult, 'context' | 'sessionId'>
): CodeSearchResult => transformPayload(params, differences);

/**
 * Code Search — Type-safe command executor
 *
 * Usage:
 *   import { CodeSearch } from '...shared/CodeSearchTypes';
 *   const result = await CodeSearch.execute({ ... });
 */
export const CodeSearch = {
  execute(params: CommandInput<CodeSearchParams>): Promise<CodeSearchResult> {
    return Commands.execute<CodeSearchParams, CodeSearchResult>('code/search', params as Partial<CodeSearchParams>);
  },
  commandName: 'code/search' as const,
} as const;
