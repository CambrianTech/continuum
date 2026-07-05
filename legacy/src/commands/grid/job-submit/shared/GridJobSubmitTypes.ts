/**
 * Grid Job Submit Command - Shared Types
 *
 * Submit a forge job to a grid node's queue. The node executes when ready (GPU free). Returns a job ID for tracking. Replaces direct SSH forge execution.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Grid Job Submit Command Parameters
 */
export interface GridJobSubmitParams extends CommandParams {
  // Target node to queue the job on
  nodeId: string;
  // Complete alloy JSON (recipe) to execute
  alloy: object;
  // Queue priority 0-10 (higher = sooner). Default: 5
  priority?: number;
}

/**
 * Factory function for creating GridJobSubmitParams
 */
export const createGridJobSubmitParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Target node to queue the job on
    nodeId: string;
    // Complete alloy JSON (recipe) to execute
    alloy: object;
    // Queue priority 0-10 (higher = sooner). Default: 5
    priority?: number;
  }
): GridJobSubmitParams => createPayload(context, sessionId, {
  // userId is auto-injected by infrastructure at runtime
  priority: data.priority ?? 0,
  ...data
}) as GridJobSubmitParams;

/**
 * Grid Job Submit Command Result
 */
export interface GridJobSubmitResult extends CommandResult {
  success: boolean;
  // Unique job identifier for tracking and control
  jobId: string;
  // Position in the queue (0 = running now)
  position: number;
  // Node the job was queued on
  nodeId: string;
  // Estimated start time (ISO 8601) based on queue depth
  estimatedStart: string;
  error?: JTAGError;
}

/**
 * Factory function for creating GridJobSubmitResult with defaults
 */
export const createGridJobSubmitResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Unique job identifier for tracking and control
    jobId?: string;
    // Position in the queue (0 = running now)
    position?: number;
    // Node the job was queued on
    nodeId?: string;
    // Estimated start time (ISO 8601) based on queue depth
    estimatedStart?: string;
    error?: JTAGError;
  }
): GridJobSubmitResult => createPayload(context, sessionId, {
  jobId: data.jobId ?? '',
  position: data.position ?? 0,
  nodeId: data.nodeId ?? '',
  estimatedStart: data.estimatedStart ?? '',
  ...data
});

/**
 * Smart Grid Job Submit-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createGridJobSubmitResultFromParams = (
  params: GridJobSubmitParams,
  differences: Omit<GridJobSubmitResult, 'context' | 'sessionId' | 'userId'>
): GridJobSubmitResult => transformPayload(params, differences);

/**
 * Grid Job Submit — Type-safe command executor
 *
 * Usage:
 *   import { GridJobSubmit } from '...shared/GridJobSubmitTypes';
 *   const result = await GridJobSubmit.execute({ ... });
 */
export const GridJobSubmit = {
  execute(params: CommandInput<GridJobSubmitParams>): Promise<GridJobSubmitResult> {
    return Commands.execute<GridJobSubmitParams, GridJobSubmitResult>('grid/job-submit', params as Partial<GridJobSubmitParams>);
  },
  commandName: 'grid/job-submit' as const,
} as const;
