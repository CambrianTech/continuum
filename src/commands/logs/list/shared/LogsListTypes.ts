/**
 * logs/list Command Types
 */

import type { CommandParams, JTAGContext, CommandInput} from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

export interface LogsListParams extends CommandParams {
  category?: 'system' | 'persona' | 'session' | 'external';
  personaId?: string;
  personaUniqueId?: string;
  component?: string;
  logType?: string;
  includeStats?: boolean;
  includeSessionLogs?: boolean;  // Include session logs in results (default: false)
}

export interface LogsListResult {
  context: JTAGContext;
  sessionId: UUID;
  success: boolean;
  error?: string;
  logs: LogInfo[];
  summary: {
    totalFiles: number;
    totalSizeMB: number;
    categories: Record<string, number>;
  };
}

export interface LogInfo {
  name: string;
  category: 'system' | 'persona' | 'session' | 'external';
  component?: string;
  personaName?: string;
  logType?: string;
  sizeMB: number;
  lineCount: number;
  lastModified: string;
  isActive: boolean;
}

/**
 * LogsList — Type-safe command executor
 *
 * Usage:
 *   import { LogsList } from '...shared/LogsListTypes';
 *   const result = await LogsList.execute({ ... });
 */
export const LogsList = {
  execute(params: CommandInput<LogsListParams>): Promise<LogsListResult> {
    return Commands.execute<LogsListParams, LogsListResult>('logs/list', params as Partial<LogsListParams>);
  },
  commandName: 'logs/list' as const,
} as const;
