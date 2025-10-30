/**
 * TaskCompleteTypes - Mark tasks as completed (or failed)
 *
 * Updates task status and records results.
 * Used by PersonaUsers to report task completion.
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

/**
 * Parameters for task/complete command
 */
export interface TaskCompleteParams extends CommandParams {
  /**
   * Task ID to complete
   */
  taskId: UUID;

  /**
   * Whether task succeeded or failed
   */
  success: boolean;

  /**
   * Task output/result (domain-specific)
   */
  output?: unknown;

  /**
   * Error message if task failed
   */
  error?: string;

  /**
   * Performance metrics
   */
  metrics?: {
    tokensUsed?: number;
    latencyMs?: number;
    confidence?: number;
  };
}

/**
 * Result from task/complete command
 */
export interface TaskCompleteResult extends CommandResult {
  success: boolean;
  error?: string;

  /**
   * Updated task summary
   */
  task?: {
    id: UUID;
    status: string;
    completedAt: string;
    duration?: number; // Time from start to completion (ms)
  };
}
