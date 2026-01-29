/**
 * TaskListTypes - List tasks for PersonaUsers
 *
 * Query tasks by assignee, status, domain, etc.
 * Used for monitoring AI work queues and debugging.
 */

import type { CommandParams, CommandResult, CommandInput} from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { TaskDomain, TaskType, TaskStatus, TaskPriority } from '@system/data/entities/TaskEntity';
import { Commands } from '../../../../../system/core/shared/Commands';

/**
 * Parameters for task/list command
 */
export interface TaskListParams extends CommandParams {
  /**
   * Filter by assignee (PersonaUser ID)
   */
  assigneeId?: UUID;

  /**
   * Filter by task status
   */
  status?: TaskStatus | TaskStatus[];

  /**
   * Filter by domain
   */
  domain?: TaskDomain;

  /**
   * Filter by task type
   */
  taskType?: TaskType;

  /**
   * Filter by context ID (roomId, fileId, etc.)
   */
  contextId?: UUID;

  /**
   * Filter by creator
   */
  createdBy?: UUID;

  /**
   * Maximum number of results (default: 50)
   */
  limit?: number;

  /**
   * Sort order (default: 'priority-desc')
   */
  sortBy?: 'priority' | 'created' | 'dueDate' | 'status';
  sortOrder?: 'asc' | 'desc';

  /**
   * Include completed tasks (default: false)
   * Use --includeCompleted=true to see completed tasks
   */
  includeCompleted?: boolean;
}

/**
 * Task summary for list results
 */
export interface TaskSummary {
  id: UUID;
  assigneeId: UUID;
  createdBy: UUID;
  domain: TaskDomain;
  taskType: TaskType;
  contextId: UUID;
  description: string;
  priority: TaskPriority;
  status: TaskStatus;
  createdAt: string;
  updatedAt: string;
  startedAt?: string;
  completedAt?: string;
  dueDate?: string;
  estimatedDuration?: number;
  dependsOn?: UUID[];
  blockedBy?: UUID[];
}

/**
 * Result from task/list command
 */
export interface TaskListResult extends CommandResult {
  success: boolean;
  error?: string;

  /**
   * List of matching tasks
   */
  tasks?: TaskSummary[];

  /**
   * Query statistics
   */
  stats?: {
    total: number;
    pending: number;
    inProgress: number;
    completed: number;
    failed: number;
    cancelled: number;
  };
}

/**
 * TaskList — Type-safe command executor
 *
 * Usage:
 *   import { TaskList } from '...shared/TaskListTypes';
 *   const result = await TaskList.execute({ ... });
 */
export const TaskList = {
  execute(params: CommandInput<TaskListParams>): Promise<TaskListResult> {
    return Commands.execute<TaskListParams, TaskListResult>('workspace/task/list', params as Partial<TaskListParams>);
  },
  commandName: 'workspace/task/list' as const,
} as const;
