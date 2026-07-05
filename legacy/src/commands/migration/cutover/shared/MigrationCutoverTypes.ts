/**
 * Migration Cutover Command - Shared Types
 *
 * Switch all operations from current adapter to the migration target. Saves previous connection for rollback.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Migration Cutover Command Parameters
 */
export interface MigrationCutoverParams extends CommandParams {
  // Current (source) connection string to decommission
  current: string;
  // Target connection string to switch to
  target: string;
}

/**
 * Factory function for creating MigrationCutoverParams
 */
export const createMigrationCutoverParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Current (source) connection string to decommission
    current: string;
    // Target connection string to switch to
    target: string;
  }
): MigrationCutoverParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,

  ...data
});

/**
 * Migration Cutover Command Result
 */
export interface MigrationCutoverResult extends CommandResult {
  success: boolean;
  // Whether cutover was successful
  cutover: boolean;
  // The connection string that was replaced (for rollback)
  previousConnection: string;
  error?: string;
}

/**
 * Factory function for creating MigrationCutoverResult with defaults
 */
export const createMigrationCutoverResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Whether cutover was successful
    cutover?: boolean;
    // The connection string that was replaced (for rollback)
    previousConnection?: string;
    error?: string;
  }
): MigrationCutoverResult => createPayload(context, sessionId, {
  cutover: data.cutover ?? false,
  previousConnection: data.previousConnection ?? '',
  ...data
});

/**
 * Smart Migration Cutover-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createMigrationCutoverResultFromParams = (
  params: MigrationCutoverParams,
  differences: Omit<MigrationCutoverResult, 'context' | 'sessionId' | 'userId'>
): MigrationCutoverResult => transformPayload(params, differences);

/**
 * Migration Cutover — Type-safe command executor
 *
 * Usage:
 *   import { MigrationCutover } from '...shared/MigrationCutoverTypes';
 *   const result = await MigrationCutover.execute({ ... });
 */
export const MigrationCutover = {
  execute(params: CommandInput<MigrationCutoverParams>): Promise<MigrationCutoverResult> {
    return Commands.execute<MigrationCutoverParams, MigrationCutoverResult>('migration/cutover', params as Partial<MigrationCutoverParams>);
  },
  commandName: 'migration/cutover' as const,
} as const;
