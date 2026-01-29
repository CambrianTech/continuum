import type { CommandParams, JTAGContext, CommandInput} from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';
export interface LogsStatsParams extends CommandParams {}
export interface LogsStatsResult { context: JTAGContext; sessionId: UUID; success: boolean; error?: string; totalFiles: number; totalSizeMB: number; }

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
