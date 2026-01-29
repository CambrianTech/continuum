import type { CommandParams, JTAGContext, CommandInput} from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';
export interface LogsSearchParams extends CommandParams { pattern: string; logs?: string[]; category?: string; personaId?: string; }
export interface LogsSearchResult { context: JTAGContext; sessionId: UUID; success: boolean; error?: string; matches: any[]; totalMatches: number; }

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
