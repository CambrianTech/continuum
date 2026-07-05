/**
 * List available log streams for a sentinel by handle.
 */

import type { CommandParams, CommandResult } from '../../../../../system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * List available log streams for a sentinel by handle.
 */
export interface SentinelLogsListParams extends CommandParams {
  /** Sentinel handle (short ID or full ID) */
  handle: string;
}

/**
 * Log stream info
 */
export interface LogStreamInfo {
  /** Stream name (e.g., "execution", "build-1", "stderr") */
  name: string;

  /** File path */
  path: string;

  /** Size in bytes */
  size: number;

  /** Last modified timestamp */
  modifiedAt: string;
}

/**
 * List result
 */
export interface SentinelLogsListResult extends CommandResult {
  success: boolean;
  handle: string;
  logsDir: string;
  streams: LogStreamInfo[];
  error?: string;
}

/**
 * SentinelLogsList — Type-safe command executor
 *
 * Usage:
 *   import { SentinelLogsList } from '...shared/SentinelLogsListTypes';
 *   const result = await SentinelLogsList.execute({ ... });
 */
export const SentinelLogsList = {
  execute(params: CommandInput<SentinelLogsListParams>): Promise<SentinelLogsListResult> {
    return Commands.execute<SentinelLogsListParams, SentinelLogsListResult>('sentinel/logs/list', params as Partial<SentinelLogsListParams>);
  },
  commandName: 'sentinel/logs/list' as const,
} as const;

/**
 * Factory function for creating SentinelLogsListParams
 */
export const createSentinelLogsListParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SentinelLogsListParams, 'context' | 'sessionId' | 'userId'>
): SentinelLogsListParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating SentinelLogsListResult with defaults
 */
export const createSentinelLogsListResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SentinelLogsListResult, 'context' | 'sessionId' | 'userId'>
): SentinelLogsListResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart sentinel/logs/list-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createSentinelLogsListResultFromParams = (
  params: SentinelLogsListParams,
  differences: Omit<SentinelLogsListResult, 'context' | 'sessionId' | 'userId'>
): SentinelLogsListResult => transformPayload(params, differences);

