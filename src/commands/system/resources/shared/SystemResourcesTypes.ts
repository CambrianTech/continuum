/**
 * System Resources Command - Shared Types
 *
 * Query system resource usage: CPU load, memory pressure, swap, and optionally top processes by CPU and memory. Uses sysinfo for cross-platform monitoring (macOS/Linux/Windows). On Apple Silicon, memory pressure directly impacts GPU headroom since VRAM is unified.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type {
	CpuStatsInfo,
	MemoryStatsInfo,
	ProcessStatsInfo,
	TopProcessInfo,
} from '../../../../workers/continuum-core/bindings/modules/system_resources';

// Re-export for consumers
export type { CpuStatsInfo, MemoryStatsInfo, ProcessStatsInfo, TopProcessInfo };

/**
 * System Resources Command Parameters
 */
export interface SystemResourcesParams extends CommandParams {
  // Include top processes by CPU and memory usage. More expensive — iterates all OS processes.
  includeProcesses?: boolean;
  // Number of top processes to return (default: 10). Only used when includeProcesses=true.
  topN?: number;
}

/**
 * Factory function for creating SystemResourcesParams
 */
export const createSystemResourcesParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Include top processes by CPU and memory usage. More expensive — iterates all OS processes.
    includeProcesses?: boolean;
    // Number of top processes to return (default: 10). Only used when includeProcesses=true.
    topN?: number;
  }
): SystemResourcesParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  includeProcesses: data.includeProcesses ?? false,
  topN: data.topN ?? 0,
  ...data
});

/**
 * System Resources Command Result
 */
export interface SystemResourcesResult extends CommandResult {
  success: boolean;
  // CPU stats: brand, core counts, global usage (0.0-1.0), per-core usage
  cpu: CpuStatsInfo;
  // Memory stats: total, used, available bytes, pressure (0.0-1.0), swap
  memory: MemoryStatsInfo;
  // Top processes by CPU and memory (only when includeProcesses=true)
  processes?: ProcessStatsInfo;
  // Snapshot timestamp in milliseconds since epoch
  timestampMs: number;
  // System uptime in seconds
  uptimeSeconds: number;
  error?: JTAGError;
}

/**
 * Factory function for creating SystemResourcesResult with defaults
 */
export const createSystemResourcesResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    cpu: CpuStatsInfo;
    memory: MemoryStatsInfo;
    processes?: ProcessStatsInfo;
    timestampMs: number;
    uptimeSeconds: number;
    error?: JTAGError;
  }
): SystemResourcesResult => createPayload(context, sessionId, {
  ...data,
});

/**
 * Smart System Resources-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createSystemResourcesResultFromParams = (
  params: SystemResourcesParams,
  differences: Omit<SystemResourcesResult, 'context' | 'sessionId' | 'userId'>
): SystemResourcesResult => transformPayload(params, differences);

/**
 * System Resources — Type-safe command executor
 *
 * Usage:
 *   import { SystemResources } from '...shared/SystemResourcesTypes';
 *   const result = await SystemResources.execute({ ... });
 */
export const SystemResources = {
  execute(params: CommandInput<SystemResourcesParams>): Promise<SystemResourcesResult> {
    return Commands.execute<SystemResourcesParams, SystemResourcesResult>('system/resources', params as Partial<SystemResourcesParams>);
  },
  commandName: 'system/resources' as const,
} as const;
