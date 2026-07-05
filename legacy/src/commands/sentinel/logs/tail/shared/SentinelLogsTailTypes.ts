/**
 * Get the last N lines of a sentinel log stream, like Unix tail.
 */

import type { CommandParams, CommandResult } from '../../../../../system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Get the last N lines of a sentinel log stream, like Unix tail.
 */
export interface SentinelLogsTailParams extends CommandParams {
  /** Sentinel handle (short ID or full ID) */
  handle: string;

  /** Stream name (e.g., "execution", "build-1", "stderr") */
  stream: string;

  /** Number of lines from the end (default: 20) */
  lines?: number;
}

/**
 * Tail result
 */
export interface SentinelLogsTailResult extends CommandResult {
  success: boolean;
  handle: string;
  stream: string;
  content: string;
  lineCount: number;
  error?: string;
}

/**
 * SentinelLogsTail — Type-safe command executor
 *
 * Usage:
 *   import { SentinelLogsTail } from '...shared/SentinelLogsTailTypes';
 *   const result = await SentinelLogsTail.execute({ ... });
 */
export const SentinelLogsTail = {
  execute(params: CommandInput<SentinelLogsTailParams>): Promise<SentinelLogsTailResult> {
    return Commands.execute<SentinelLogsTailParams, SentinelLogsTailResult>('sentinel/logs/tail', params as Partial<SentinelLogsTailParams>);
  },
  commandName: 'sentinel/logs/tail' as const,
} as const;

/**
 * Factory function for creating SentinelLogsTailParams
 */
export const createSentinelLogsTailParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SentinelLogsTailParams, 'context' | 'sessionId' | 'userId'>
): SentinelLogsTailParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating SentinelLogsTailResult with defaults
 */
export const createSentinelLogsTailResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SentinelLogsTailResult, 'context' | 'sessionId' | 'userId'>
): SentinelLogsTailResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart sentinel/logs/tail-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createSentinelLogsTailResultFromParams = (
  params: SentinelLogsTailParams,
  differences: Omit<SentinelLogsTailResult, 'context' | 'sessionId' | 'userId'>
): SentinelLogsTailResult => transformPayload(params, differences);

