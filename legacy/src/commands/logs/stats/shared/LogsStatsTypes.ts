import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { Commands } from '../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
/** Return aggregate statistics about all log files, including total file count and combined size in megabytes. */
export interface LogsStatsParams extends CommandParams {}
export interface LogsStatsResult extends CommandResult { success: boolean; error?: string; totalFiles: number; totalSizeMB: number; }

/**
 * LogsStats — Type-safe command executor
 *
 * Usage:
 *   import { LogsStats } from '...shared/LogsStatsTypes';
 *   const result = await LogsStats.execute({ ... });
 */
export const LogsStats = {
  execute(params: CommandInput<LogsStatsParams>): Promise<LogsStatsResult> {
    return Commands.execute<LogsStatsParams, LogsStatsResult>('logs/stats', params as Partial<LogsStatsParams>);
  },
  commandName: 'logs/stats' as const,
} as const;

/**
 * Factory function for creating LogsStatsParams
 */
export const createLogsStatsParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<LogsStatsParams, 'context' | 'sessionId' | 'userId'>
): LogsStatsParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating LogsStatsResult with defaults
 */
export const createLogsStatsResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<LogsStatsResult, 'context' | 'sessionId' | 'userId'>
): LogsStatsResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart logs/stats-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createLogsStatsResultFromParams = (
  params: LogsStatsParams,
  differences: Omit<LogsStatsResult, 'context' | 'sessionId' | 'userId'>
): LogsStatsResult => transformPayload(params, differences);

