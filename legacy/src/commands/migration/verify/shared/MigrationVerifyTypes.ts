/**
 * Migration Verify Command - Shared Types
 *
 * Verify migration integrity by comparing record counts between source and target
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Migration Verify Command Parameters — no command-specific params;
 * CommandParams (context + sessionId + userId) is the full payload.
 * Type alias (not `extends CommandParams {}` with `_noParams: never`)
 * so the type is genuinely empty + structurally identical to
 * CommandParams.
 */
export type MigrationVerifyParams = CommandParams;

/**
 * Factory function for creating MigrationVerifyParams. System-scoped:
 * issued by the migration system, not a user — userId is always
 * SYSTEM_SCOPES.SYSTEM.
 */
export const createMigrationVerifyParams = (
  context: JTAGContext,
  sessionId: UUID,
): MigrationVerifyParams => createPayload(context, sessionId, { userId: SYSTEM_SCOPES.SYSTEM });

/**
 * Migration Verify Command Result
 */
export interface MigrationVerifyResult extends CommandResult {
  success: boolean;
  // Whether all collections match between source and target
  verified: boolean;
  // Per-collection verification (collection, sourceCount, targetCount, migrated, failed, matches)
  collections: object[];
  error?: string;
}

/**
 * Factory function for creating MigrationVerifyResult with defaults
 */
export const createMigrationVerifyResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Whether all collections match between source and target
    verified?: boolean;
    // Per-collection verification (collection, sourceCount, targetCount, migrated, failed, matches)
    collections?: object[];
    error?: string;
  }
): MigrationVerifyResult => createPayload(context, sessionId, {
  verified: data.verified ?? false,
  collections: data.collections ?? [],
  ...data
});

/**
 * Smart Migration Verify-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createMigrationVerifyResultFromParams = (
  params: MigrationVerifyParams,
  differences: Omit<MigrationVerifyResult, 'context' | 'sessionId' | 'userId'>
): MigrationVerifyResult => transformPayload(params, differences);

/**
 * Migration Verify — Type-safe command executor
 *
 * Usage:
 *   import { MigrationVerify } from '...shared/MigrationVerifyTypes';
 *   const result = await MigrationVerify.execute({ ... });
 */
export const MigrationVerify = {
  execute(params: CommandInput<MigrationVerifyParams>): Promise<MigrationVerifyResult> {
    return Commands.execute<MigrationVerifyParams, MigrationVerifyResult>('migration/verify', params as Partial<MigrationVerifyParams>);
  },
  commandName: 'migration/verify' as const,
} as const;
