/**
 * Load and optionally run saved sentinel definitions from database.
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { SentinelEntity, SentinelExecutionResult } from '../../../../system/sentinel';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';

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
