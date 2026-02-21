/**
 * Migration Pause Command - Shared Types
 *
 * Pause an in-flight migration. Can be resumed later from the last checkpoint.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Migration Pause Command Parameters
 */
export interface MigrationPauseParams extends CommandParams {
  _noParams?: never; // Marker to avoid empty interface
}

/**
 * Factory function for creating MigrationPauseParams
 */
export const createMigrationPauseParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Record<string, never>
): MigrationPauseParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,

  ...data
});

/**
 * Migration Pause Command Result
 */
export interface MigrationPauseResult extends CommandResult {
  success: boolean;
  // Whether the migration was successfully paused
  paused: boolean;
  error?: string;
}

/**
 * Factory function for creating MigrationPauseResult with defaults
 */
export const createMigrationPauseResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Whether the migration was successfully paused
    paused?: boolean;
    error?: string;
  }
): MigrationPauseResult => createPayload(context, sessionId, {
  paused: data.paused ?? false,
  ...data
});

/**
 * Smart Migration Pause-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createMigrationPauseResultFromParams = (
  params: MigrationPauseParams,
  differences: Omit<MigrationPauseResult, 'context' | 'sessionId' | 'userId'>
): MigrationPauseResult => transformPayload(params, differences);

/**
 * Migration Pause — Type-safe command executor
 *
 * Usage:
 *   import { MigrationPause } from '...shared/MigrationPauseTypes';
 *   const result = await MigrationPause.execute({ ... });
 */
export const MigrationPause = {
  execute(params: CommandInput<MigrationPauseParams>): Promise<MigrationPauseResult> {
    return Commands.execute<MigrationPauseParams, MigrationPauseResult>('migration/pause', params as Partial<MigrationPauseParams>);
  },
  commandName: 'migration/pause' as const,
} as const;
