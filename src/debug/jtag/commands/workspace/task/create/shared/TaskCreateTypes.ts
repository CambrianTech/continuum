/**
 * TaskCreateTypes - Create new tasks for PersonaUsers
 *
 * Allows users to assign tasks to AI personas (or themselves).
 * Tasks can be chat responses, code reviews, analysis, or self-improvement.
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { TaskDomain, TaskType, TaskPriority } from '@system/data/entities/TaskEntity';

/**
 * Parameters for task/create command
 */
export interface TaskCreateParams extends CommandParams {
  /**
   * Who should do this task (PersonaUser ID)
   */
  assigneeId: UUID;

  /**
   * Task domain (chat, code, game, academy, analysis, self)
   */
  domain: TaskDomain;

  /**
   * Specific task type within domain
   */
  taskType: TaskType;

  /**
   * Domain-specific context ID (roomId, fileId, gameId, etc.)
   */
  contextId: UUID;

  /**
   * Human-readable task description
   */
  description: string;

  /**
   * Task priority (0.0-1.0, default: 0.5)
   */
  priority?: TaskPriority;

  /**
   * Optional deadline
   */
  dueDate?: string; // ISO date string

  /**
   * Estimated time to complete (milliseconds)
   */
  estimatedDuration?: number;

  /**
   * Tasks this task depends on (must complete first)
   */
  dependsOn?: UUID[];

  /**
   * Domain-specific metadata
   */
  metadata?: Record<string, unknown>;
}

/**
 * Result from task/create command
 */
export interface TaskCreateResult extends CommandResult {
  success: boolean;
  taskId?: UUID;
  error?: string;

  /**
   * Created task summary
   */
  task?: {
    id: UUID;
    assigneeId: UUID;
    domain: TaskDomain;
    taskType: TaskType;
    description: string;
    priority: TaskPriority;
    status: string;
    createdAt: string;
  };
}
