import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { Commands } from '../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
/** Search across log files for lines matching a pattern, optionally scoped to specific logs, categories, or personas. */
export interface LogsSearchParams extends CommandParams { pattern: string; logs?: string[]; category?: string; personaId?: string; }
export interface LogsSearchMatch { file: string; line: number; text: string; category?: string; }
export interface LogsSearchResult extends CommandResult { success: boolean; error?: string; matches: LogsSearchMatch[]; totalMatches: number; }

/**
 * LogsSearch — Type-safe command executor
 *
 * Usage:
 *   import { LogsSearch } from '...shared/LogsSearchTypes';
 *   const result = await LogsSearch.execute({ ... });
 */
export const LogsSearch = {
  execute(params: CommandInput<LogsSearchParams>): Promise<LogsSearchResult> {
    return Commands.execute<LogsSearchParams, LogsSearchResult>('logs/search', params as Partial<LogsSearchParams>);
  },
  commandName: 'logs/search' as const,
} as const;

/**
 * Factory function for creating LogsSearchParams
 */
export const createLogsSearchParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<LogsSearchParams, 'context' | 'sessionId' | 'userId'>
): LogsSearchParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating LogsSearchResult with defaults
 */
export const createLogsSearchResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<LogsSearchResult, 'context' | 'sessionId' | 'userId'>
): LogsSearchResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart logs/search-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createLogsSearchResultFromParams = (
  params: LogsSearchParams,
  differences: Omit<LogsSearchResult, 'context' | 'sessionId' | 'userId'>
): LogsSearchResult => transformPayload(params, differences);

