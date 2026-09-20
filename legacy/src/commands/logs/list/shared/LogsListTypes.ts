/**
 * logs/list Command Types
 */

import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { Commands } from '../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export interface LogsListParams extends CommandParams {
  category?: 'system' | 'persona' | 'session' | 'external';
  personaId?: string;
  personaUniqueId?: string;
  component?: string;
  logType?: string;
  includeStats?: boolean;
  includeSessionLogs?: boolean;  // Include session logs in results (default: false)
}

export interface LogsListResult extends CommandResult {
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

/**
 * Factory function for creating LogsListParams
 */
export const createLogsListParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<LogsListParams, 'context' | 'sessionId' | 'userId'>
): LogsListParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating LogsListResult with defaults
 */
export const createLogsListResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<LogsListResult, 'context' | 'sessionId' | 'userId'>
): LogsListResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart logs/list-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createLogsListResultFromParams = (
  params: LogsListParams,
  differences: Omit<LogsListResult, 'context' | 'sessionId' | 'userId'>
): LogsListResult => transformPayload(params, differences);

