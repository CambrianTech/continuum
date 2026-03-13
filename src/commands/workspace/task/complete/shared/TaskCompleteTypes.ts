/**
 * TaskCompleteTypes - Mark tasks as completed (or failed)
 *
 * Updates task status and records results.
 * Used by PersonaUsers to report task completion.
 */

import type { CommandParams, CommandResult, CommandInput} from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';

/**
 * Marks a task as completed or failed, recording its output, error details, and performance metrics such as tokens used, latency, and confidence.
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

/**
 * TaskComplete — Type-safe command executor
 *
 * Usage:
 *   import { TaskComplete } from '...shared/TaskCompleteTypes';
 *   const result = await TaskComplete.execute({ ... });
 */
export const TaskComplete = {
  execute(params: CommandInput<TaskCompleteParams>): Promise<TaskCompleteResult> {
    return Commands.execute<TaskCompleteParams, TaskCompleteResult>('workspace/task/complete', params as Partial<TaskCompleteParams>);
  },
  commandName: 'workspace/task/complete' as const,
} as const;

/**
 * Factory function for creating WorkspaceTaskCompleteParams
 */
export const createWorkspaceTaskCompleteParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<TaskCompleteParams, 'context' | 'sessionId' | 'userId'>
): TaskCompleteParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating WorkspaceTaskCompleteResult with defaults
 */
export const createWorkspaceTaskCompleteResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<TaskCompleteResult, 'context' | 'sessionId' | 'userId'>
): TaskCompleteResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart workspace/task/complete-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createWorkspaceTaskCompleteResultFromParams = (
  params: TaskCompleteParams,
  differences: Omit<TaskCompleteResult, 'context' | 'sessionId' | 'userId'>
): TaskCompleteResult => transformPayload(params, differences);

