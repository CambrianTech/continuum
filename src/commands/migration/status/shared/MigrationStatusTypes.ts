/**
 * Migration Status Command - Shared Types
 *
 * Get current migration progress with per-collection breakdown
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Migration Status Command Parameters
 */
export interface MigrationStatusParams extends CommandParams {
  _noParams?: never; // Marker to avoid empty interface
}

/**
 * Factory function for creating MigrationStatusParams
 */
export const createMigrationStatusParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Record<string, never>
): MigrationStatusParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,

  ...data
});

/**
 * Migration Status Command Result
 */
export interface MigrationStatusResult extends CommandResult {
  success: boolean;
  // Total records across all collections
  total: number;
  // Records successfully migrated so far
  migrated: number;
  // Records that failed to migrate
  failed: number;
  // Whether migration is currently paused
  paused: boolean;
  // Per-collection status (collection, status, total, migrated, failed, error)
  collections: object[];
  error?: string;
}

/**
 * Factory function for creating MigrationStatusResult with defaults
 */
export const createMigrationStatusResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Total records across all collections
    total?: number;
    // Records successfully migrated so far
    migrated?: number;
    // Records that failed to migrate
    failed?: number;
    // Whether migration is currently paused
    paused?: boolean;
    // Per-collection status (collection, status, total, migrated, failed, error)
    collections?: object[];
    error?: string;
  }
): MigrationStatusResult => createPayload(context, sessionId, {
  total: data.total ?? 0,
  migrated: data.migrated ?? 0,
  failed: data.failed ?? 0,
  paused: data.paused ?? false,
  collections: data.collections ?? [],
  ...data
});

/**
 * Smart Migration Status-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createMigrationStatusResultFromParams = (
  params: MigrationStatusParams,
  differences: Omit<MigrationStatusResult, 'context' | 'sessionId' | 'userId'>
): MigrationStatusResult => transformPayload(params, differences);

/**
 * Migration Status — Type-safe command executor
 *
 * Usage:
 *   import { MigrationStatus } from '...shared/MigrationStatusTypes';
 *   const result = await MigrationStatus.execute({ ... });
 */
export const MigrationStatus = {
  execute(params: CommandInput<MigrationStatusParams>): Promise<MigrationStatusResult> {
    return Commands.execute<MigrationStatusParams, MigrationStatusResult>('migration/status', params as Partial<MigrationStatusParams>);
  },
  commandName: 'migration/status' as const,
} as const;
