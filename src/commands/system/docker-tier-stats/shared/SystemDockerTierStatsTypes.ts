/**
 * System Docker Tier Stats Command - Shared Types
 *
 * Snapshot of the Docker storage tier (capacity, used bytes, pressure ratio, detection state). Phase 1 of #1239 — exposes the data the existing `DockerTierPool` (`modules/docker_tier_pool.rs`) already computes, without depending on the not-yet-instantiated `PressureBroker` singleton. Wired so `bin/continuum status` can surface a `Docker disk: ...` row + warn at >90%, and so future scheduler hot paths can refuse before ENOSPC. Returns `detected: false` + zeros on hosts where Docker isn't installed.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { DockerTierStats } from '@shared/generated/resources';


/**
 * System Docker Tier Stats Command Parameters
 */
export type SystemDockerTierStatsParams = CommandParams;

/**
 * Factory function for creating SystemDockerTierStatsParams
 */
export const createSystemDockerTierStatsParams = (
  context: JTAGContext,
  sessionId: UUID,
  userId: UUID,
): SystemDockerTierStatsParams => createPayload(context, sessionId, { userId });

/**
 * System Docker Tier Stats Command Result
 */
export interface SystemDockerTierStatsResult extends CommandResult {
  success: boolean;
  // { capacityBytes, usedBytes, pressure (0.0-1.0+), detected }. See shared/generated/resources/DockerTierStats.ts.
  stats: DockerTierStats;
  error?: JTAGError;
}

/**
 * Factory function for creating SystemDockerTierStatsResult with defaults
 */
export const createSystemDockerTierStatsResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // { capacityBytes, usedBytes, pressure (0.0-1.0+), detected }. See shared/generated/resources/DockerTierStats.ts.
    stats: DockerTierStats;
    error?: JTAGError;
  }
): SystemDockerTierStatsResult => createPayload(context, sessionId, {

  ...data
});

/**
 * Smart System Docker Tier Stats-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createSystemDockerTierStatsResultFromParams = (
  params: SystemDockerTierStatsParams,
  differences: Omit<SystemDockerTierStatsResult, 'context' | 'sessionId' | 'userId'>
): SystemDockerTierStatsResult => transformPayload(params, differences);

/**
 * System Docker Tier Stats — Type-safe command executor
 *
 * Usage:
 *   import { SystemDockerTierStats } from '...shared/SystemDockerTierStatsTypes';
 *   const result = await SystemDockerTierStats.execute({ ... });
 */
export const SystemDockerTierStats = {
  execute(params: CommandInput<SystemDockerTierStatsParams>): Promise<SystemDockerTierStatsResult> {
    return Commands.execute<SystemDockerTierStatsParams, SystemDockerTierStatsResult>('system/docker-tier-stats', params as Partial<SystemDockerTierStatsParams>);
  },
  commandName: 'system/docker-tier-stats' as const,
} as const;
