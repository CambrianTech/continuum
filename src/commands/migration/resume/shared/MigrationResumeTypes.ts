/**
 * Migration Resume Command - Shared Types
 *
 * Resume a paused migration from its last checkpoint
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Migration Resume Command Parameters
 */
export interface MigrationResumeParams extends CommandParams {
  _noParams?: never; // Marker to avoid empty interface
}

/**
 * Factory function for creating MigrationResumeParams
 */
export const createMigrationResumeParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Record<string, never>
): MigrationResumeParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,

  ...data
});

/**
 * Migration Resume Command Result
 */
export interface MigrationResumeResult extends CommandResult {
  success: boolean;
  // Total records across all collections
  total: number;
  // Records successfully migrated after resume
  migrated: number;
  // Records that failed to migrate
  failed: number;
  // Whether migration is still paused
  paused: boolean;
  // Per-collection migration status
  collections: object[];
  error?: string;
}

/**
 * Factory function for creating MigrationResumeResult with defaults
 */
export const createMigrationResumeResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Total records across all collections
    total?: number;
    // Records successfully migrated after resume
    migrated?: number;
    // Records that failed to migrate
    failed?: number;
    // Whether migration is still paused
    paused?: boolean;
    // Per-collection migration status
    collections?: object[];
    error?: string;
  }
): MigrationResumeResult => createPayload(context, sessionId, {
  total: data.total ?? 0,
  migrated: data.migrated ?? 0,
  failed: data.failed ?? 0,
  paused: data.paused ?? false,
  collections: data.collections ?? [],
  ...data
});

/**
 * Smart Migration Resume-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createMigrationResumeResultFromParams = (
  params: MigrationResumeParams,
  differences: Omit<MigrationResumeResult, 'context' | 'sessionId' | 'userId'>
): MigrationResumeResult => transformPayload(params, differences);

/**
 * Migration Resume — Type-safe command executor
 *
 * Usage:
 *   import { MigrationResume } from '...shared/MigrationResumeTypes';
 *   const result = await MigrationResume.execute({ ... });
 */
export const MigrationResume = {
  execute(params: CommandInput<MigrationResumeParams>): Promise<MigrationResumeResult> {
    return Commands.execute<MigrationResumeParams, MigrationResumeResult>('migration/resume', params as Partial<MigrationResumeParams>);
  },
  commandName: 'migration/resume' as const,
} as const;
