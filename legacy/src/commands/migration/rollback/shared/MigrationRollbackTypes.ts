/**
 * Migration Rollback Command - Shared Types
 *
 * Revert to the previous connection string after a cutover. Source data is never deleted.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Migration Rollback Command Parameters
 */
export interface MigrationRollbackParams extends CommandParams {
  // Current (target) connection string to roll back from
  current: string;
}

/**
 * Factory function for creating MigrationRollbackParams
 */
export const createMigrationRollbackParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Current (target) connection string to roll back from
    current: string;
  }
): MigrationRollbackParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,

  ...data
});

/**
 * Migration Rollback Command Result
 */
export interface MigrationRollbackResult extends CommandResult {
  success: boolean;
  // Whether rollback was successful
  rolledBack: boolean;
  // The connection string that was restored
  restoredConnection: string;
  error?: string;
}

/**
 * Factory function for creating MigrationRollbackResult with defaults
 */
export const createMigrationRollbackResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Whether rollback was successful
    rolledBack?: boolean;
    // The connection string that was restored
    restoredConnection?: string;
    error?: string;
  }
): MigrationRollbackResult => createPayload(context, sessionId, {
  rolledBack: data.rolledBack ?? false,
  restoredConnection: data.restoredConnection ?? '',
  ...data
});

/**
 * Smart Migration Rollback-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createMigrationRollbackResultFromParams = (
  params: MigrationRollbackParams,
  differences: Omit<MigrationRollbackResult, 'context' | 'sessionId' | 'userId'>
): MigrationRollbackResult => transformPayload(params, differences);

/**
 * Migration Rollback — Type-safe command executor
 *
 * Usage:
 *   import { MigrationRollback } from '...shared/MigrationRollbackTypes';
 *   const result = await MigrationRollback.execute({ ... });
 */
export const MigrationRollback = {
  execute(params: CommandInput<MigrationRollbackParams>): Promise<MigrationRollbackResult> {
    return Commands.execute<MigrationRollbackParams, MigrationRollbackResult>('migration/rollback', params as Partial<MigrationRollbackParams>);
  },
  commandName: 'migration/rollback' as const,
} as const;
