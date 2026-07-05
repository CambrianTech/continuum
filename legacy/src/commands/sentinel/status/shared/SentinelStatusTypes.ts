/**
 * Check the status of a running or completed sentinel by handle ID.
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { SentinelType } from '../../run/shared/SentinelRunTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Check the status of a running or completed sentinel by handle ID.
 */
export interface SentinelStatusParams extends CommandParams {
  /** Handle ID to check */
  handle: string;
}

export interface SentinelStatusResult extends CommandResult {
  handle: string;
  status: 'running' | 'completed' | 'failed' | 'cancelled' | 'not_found';
  progress?: number;
  exitCode?: number;
  workingDir?: string;
  logsDir?: string;
  error?: string;
}

/**
 * SentinelStatus — Type-safe command executor
 *
 * Usage:
 *   import { SentinelStatus } from '...shared/SentinelStatusTypes';
 *   const result = await SentinelStatus.execute({ ... });
 */
export const SentinelStatus = {
  execute(params: CommandInput<SentinelStatusParams>): Promise<SentinelStatusResult> {
    return Commands.execute<SentinelStatusParams, SentinelStatusResult>('sentinel/status', params as Partial<SentinelStatusParams>);
  },
  commandName: 'sentinel/status' as const,
} as const;

/**
 * Factory function for creating SentinelStatusParams
 */
export const createSentinelStatusParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SentinelStatusParams, 'context' | 'sessionId' | 'userId'>
): SentinelStatusParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating SentinelStatusResult with defaults
 */
export const createSentinelStatusResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SentinelStatusResult, 'context' | 'sessionId' | 'userId'>
): SentinelStatusResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart sentinel/status-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createSentinelStatusResultFromParams = (
  params: SentinelStatusParams,
  differences: Omit<SentinelStatusResult, 'context' | 'sessionId' | 'userId'>
): SentinelStatusResult => transformPayload(params, differences);

