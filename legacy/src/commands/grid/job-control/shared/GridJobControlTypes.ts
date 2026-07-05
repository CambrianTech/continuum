/**
 * Grid Job Control Command - Shared Types
 *
 * Control a running or queued forge job: pause (checkpoint + stop), resume (reload checkpoint + continue), cancel (kill + clean up). Uses the grid transport layer.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Grid Job Control Command Parameters
 */
export interface GridJobControlParams extends CommandParams {
  // Job ID from grid/job-submit
  jobId: string;
  // Control action: 'pause' | 'resume' | 'cancel'
  action: string;
  // Node the job is on (auto-resolved from jobId if omitted)
  nodeId?: string;
}

/**
 * Factory function for creating GridJobControlParams
 */
export const createGridJobControlParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Job ID from grid/job-submit
    jobId: string;
    // Control action: 'pause' | 'resume' | 'cancel'
    action: string;
    // Node the job is on (auto-resolved from jobId if omitted)
    nodeId?: string;
  }
): GridJobControlParams => createPayload(context, sessionId, {
  // userId is auto-injected by infrastructure at runtime
  nodeId: data.nodeId ?? '',
  ...data
}) as GridJobControlParams;

/**
 * Grid Job Control Command Result
 */
export interface GridJobControlResult extends CommandResult {
  success: boolean;
  // The job that was controlled
  jobId: string;
  // State before the action: 'queued' | 'running' | 'paused'
  previousState: string;
  // State after the action: 'paused' | 'running' | 'cancelled'
  newState: string;
  // Checkpoint info if paused: { cycle, step, path }
  checkpoint: object;
  error?: JTAGError;
}

/**
 * Factory function for creating GridJobControlResult with defaults
 */
export const createGridJobControlResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // The job that was controlled
    jobId?: string;
    // State before the action: 'queued' | 'running' | 'paused'
    previousState?: string;
    // State after the action: 'paused' | 'running' | 'cancelled'
    newState?: string;
    // Checkpoint info if paused: { cycle, step, path }
    checkpoint?: object;
    error?: JTAGError;
  }
): GridJobControlResult => createPayload(context, sessionId, {
  jobId: data.jobId ?? '',
  previousState: data.previousState ?? '',
  newState: data.newState ?? '',
  checkpoint: data.checkpoint ?? {},
  ...data
});

/**
 * Smart Grid Job Control-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createGridJobControlResultFromParams = (
  params: GridJobControlParams,
  differences: Omit<GridJobControlResult, 'context' | 'sessionId' | 'userId'>
): GridJobControlResult => transformPayload(params, differences);

/**
 * Grid Job Control — Type-safe command executor
 *
 * Usage:
 *   import { GridJobControl } from '...shared/GridJobControlTypes';
 *   const result = await GridJobControl.execute({ ... });
 */
export const GridJobControl = {
  execute(params: CommandInput<GridJobControlParams>): Promise<GridJobControlResult> {
    return Commands.execute<GridJobControlParams, GridJobControlResult>('grid/job-control', params as Partial<GridJobControlParams>);
  },
  commandName: 'grid/job-control' as const,
} as const;
