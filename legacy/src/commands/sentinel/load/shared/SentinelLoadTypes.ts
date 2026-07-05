/**
 * Load and optionally run saved sentinel definitions from database.
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { SentinelEntity, SentinelExecutionResult } from '../../../../system/sentinel';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Load and optionally run a saved sentinel definition by ID.
 */
export interface SentinelLoadParams extends CommandParams {
  /** Sentinel entity ID or shortId */
  id: string;

  /** Run immediately after loading (default: false) */
  run?: boolean;

  /** Run asynchronously (default: true when run=true) */
  async?: boolean;

  /** Override working directory for this run */
  workingDir?: string;
}

/**
 * Load result
 */
export interface SentinelLoadResult extends CommandResult {
  /** Whether load succeeded */
  success: boolean;

  /** The loaded entity */
  entity?: SentinelEntity;

  /** If run=true, the execution handle */
  handle?: string;

  /** If run=true and async=false, the execution result */
  result?: SentinelExecutionResult;

  /** Error message if failed */
  error?: string;
}

/**
 * SentinelLoad — Type-safe command executor
 *
 * Usage:
 *   import { SentinelLoad } from '...shared/SentinelLoadTypes';
 *   const result = await SentinelLoad.execute({ ... });
 */
export const SentinelLoad = {
  execute(params: CommandInput<SentinelLoadParams>): Promise<SentinelLoadResult> {
    return Commands.execute<SentinelLoadParams, SentinelLoadResult>('sentinel/load', params as Partial<SentinelLoadParams>);
  },
  commandName: 'sentinel/load' as const,
} as const;

/**
 * Factory function for creating SentinelLoadParams
 */
export const createSentinelLoadParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SentinelLoadParams, 'context' | 'sessionId' | 'userId'>
): SentinelLoadParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating SentinelLoadResult with defaults
 */
export const createSentinelLoadResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SentinelLoadResult, 'context' | 'sessionId' | 'userId'>
): SentinelLoadResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart sentinel/load-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createSentinelLoadResultFromParams = (
  params: SentinelLoadParams,
  differences: Omit<SentinelLoadResult, 'context' | 'sessionId' | 'userId'>
): SentinelLoadResult => transformPayload(params, differences);

