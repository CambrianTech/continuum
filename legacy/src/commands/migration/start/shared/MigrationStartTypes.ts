/**
 * Migration Start Command - Shared Types
 *
 * Start streaming data migration between any two storage adapters (e.g., SQLite to PostgreSQL)
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Migration Start Command Parameters
 */
export interface MigrationStartParams extends CommandParams {
  // Source connection string (file path for SQLite, postgres:// URL for Postgres)
  source: string;
  // Target connection string (file path for SQLite, postgres:// URL for Postgres)
  target: string;
  // Records per batch (default: 500)
  batchSize?: number;
  // Milliseconds to pause between batches (default: 10)
  throttleMs?: number;
  // Specific collections to migrate (default: all)
  collections?: string[];
}

/**
 * Factory function for creating MigrationStartParams
 */
export const createMigrationStartParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Source connection string (file path for SQLite, postgres:// URL for Postgres)
    source: string;
    // Target connection string (file path for SQLite, postgres:// URL for Postgres)
    target: string;
    // Records per batch (default: 500)
    batchSize?: number;
    // Milliseconds to pause between batches (default: 10)
    throttleMs?: number;
    // Specific collections to migrate (default: all)
    collections?: string[];
  }
): MigrationStartParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  batchSize: data.batchSize ?? 0,
  throttleMs: data.throttleMs ?? 0,
  collections: data.collections ?? [],
  ...data
});

/**
 * Migration Start Command Result
 */
export interface MigrationStartResult extends CommandResult {
  success: boolean;
  // Total records across all collections
  total: number;
  // Records successfully migrated
  migrated: number;
  // Records that failed to migrate
  failed: number;
  // Whether migration was paused
  paused: boolean;
  // Per-collection migration status
  collections: object[];
  error?: string;
}

/**
 * Factory function for creating MigrationStartResult with defaults
 */
export const createMigrationStartResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Total records across all collections
    total?: number;
    // Records successfully migrated
    migrated?: number;
    // Records that failed to migrate
    failed?: number;
    // Whether migration was paused
    paused?: boolean;
    // Per-collection migration status
    collections?: object[];
    error?: string;
  }
): MigrationStartResult => createPayload(context, sessionId, {
  total: data.total ?? 0,
  migrated: data.migrated ?? 0,
  failed: data.failed ?? 0,
  paused: data.paused ?? false,
  collections: data.collections ?? [],
  ...data
});

/**
 * Smart Migration Start-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createMigrationStartResultFromParams = (
  params: MigrationStartParams,
  differences: Omit<MigrationStartResult, 'context' | 'sessionId' | 'userId'>
): MigrationStartResult => transformPayload(params, differences);

/**
 * Migration Start — Type-safe command executor
 *
 * Usage:
 *   import { MigrationStart } from '...shared/MigrationStartTypes';
 *   const result = await MigrationStart.execute({ ... });
 */
export const MigrationStart = {
  execute(params: CommandInput<MigrationStartParams>): Promise<MigrationStartResult> {
    return Commands.execute<MigrationStartParams, MigrationStartResult>('migration/start', params as Partial<MigrationStartParams>);
  },
  commandName: 'migration/start' as const,
} as const;
