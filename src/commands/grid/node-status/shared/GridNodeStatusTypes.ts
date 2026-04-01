/**
 * Grid Node Status Command - Shared Types
 *
 * Query a grid node's current state: GPU utilization, running jobs, queue depth, temperature. Uses the grid transport layer (Tailscale now, Reticulum later).
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Grid Node Status Command Parameters
 */
export interface GridNodeStatusParams extends CommandParams {
  // Target node name or ID. Default: all known nodes.
  nodeId?: string;
}

/**
 * Factory function for creating GridNodeStatusParams
 */
export const createGridNodeStatusParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Target node name or ID. Default: all known nodes.
    nodeId?: string;
  }
): GridNodeStatusParams => createPayload(context, sessionId, {
  // userId is auto-injected by infrastructure at runtime
  nodeId: data.nodeId ?? '',
  ...data
}) as GridNodeStatusParams;

/**
 * Grid Node Status Command Result
 */
export interface GridNodeStatusResult extends CommandResult {
  success: boolean;
  // Node state: 'ready' | 'busy' | 'offline' | 'error'
  state: string;
  // GPU info: { name, utilization, memoryUsedMb, memoryTotalMb, temperatureC }
  gpu: object;
  // Array of running jobs: { pid, type, detail, cpu, mem }
  jobs: object;
  // Array of queued alloys: { name, path }
  queue: object;
  // The node that responded
  nodeId: string;
  // ISO 8601 timestamp of the status report
  timestamp: string;
  error?: JTAGError;
}

/**
 * Factory function for creating GridNodeStatusResult with defaults
 */
export const createGridNodeStatusResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Node state: 'ready' | 'busy' | 'offline' | 'error'
    state?: string;
    // GPU info: { name, utilization, memoryUsedMb, memoryTotalMb, temperatureC }
    gpu?: object;
    // Array of running jobs: { pid, type, detail, cpu, mem }
    jobs?: object;
    // Array of queued alloys: { name, path }
    queue?: object;
    // The node that responded
    nodeId?: string;
    // ISO 8601 timestamp of the status report
    timestamp?: string;
    error?: JTAGError;
  }
): GridNodeStatusResult => createPayload(context, sessionId, {
  state: data.state ?? '',
  gpu: data.gpu ?? {},
  jobs: data.jobs ?? {},
  queue: data.queue ?? {},
  nodeId: data.nodeId ?? '',
  timestamp: data.timestamp ?? '',
  ...data
});

/**
 * Smart Grid Node Status-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createGridNodeStatusResultFromParams = (
  params: GridNodeStatusParams,
  differences: Omit<GridNodeStatusResult, 'context' | 'sessionId' | 'userId'>
): GridNodeStatusResult => transformPayload(params, differences);

/**
 * Grid Node Status — Type-safe command executor
 *
 * Usage:
 *   import { GridNodeStatus } from '...shared/GridNodeStatusTypes';
 *   const result = await GridNodeStatus.execute({ ... });
 */
export const GridNodeStatus = {
  execute(params: CommandInput<GridNodeStatusParams>): Promise<GridNodeStatusResult> {
    return Commands.execute<GridNodeStatusParams, GridNodeStatusResult>('grid/node-status', params as Partial<GridNodeStatusParams>);
  },
  commandName: 'grid/node-status' as const,
} as const;
