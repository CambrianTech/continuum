/**
 * TaskListServerCommand - Server-side task listing
 *
 * Queries tasks from database with flexible filtering.
 * Provides statistics for monitoring AI work queues.
 */

import { CommandBase } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { TaskListParams, TaskListResult, TaskSummary } from '../shared/TaskListTypes';
import { DataDaemon } from '@daemons/data-daemon/shared/DataDaemon';
import { COLLECTIONS } from '@system/data/config/DatabaseConfig';
import type { TaskEntity } from '@system/data/entities/TaskEntity';
import type { DataRecord } from '@daemons/data-daemon/shared/DataTypes';

export class TaskListServerCommand extends CommandBase<TaskListParams, TaskListResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('workspace/task/list', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<TaskListResult> {
    const listParams = params as TaskListParams;

    console.log('📋 TASK LIST: Querying tasks');
    if (listParams.assigneeId) {
      console.log(`   Assignee: ${listParams.assigneeId.slice(0, 8)}...`);
    }
    if (listParams.status) {
      console.log(`   Status: ${Array.isArray(listParams.status) ? listParams.status.join(', ') : listParams.status}`);
    }

    try {
      // Build query filter
      const filter: any = {};

      if (listParams.assigneeId) {
        filter.assigneeId = listParams.assigneeId;
      }

      if (listParams.status) {
        // Handle array or single status
        filter.status = Array.isArray(listParams.status) ? listParams.status[0] : listParams.status;
      }

      if (listParams.domain) {
        filter.domain = listParams.domain;
      }

      if (listParams.taskType) {
        filter.taskType = listParams.taskType;
      }

      if (listParams.contextId) {
        filter.contextId = listParams.contextId;
      }

      if (listParams.createdBy) {
        filter.createdBy = listParams.createdBy;
      }

      // Query tasks
      const queryResult = await DataDaemon.query<TaskEntity>({
        collection: COLLECTIONS.TASKS,
        filter,
        limit: listParams.limit ?? 50
      });

      if (!queryResult.success || !queryResult.data) {
        return transformPayload(params, {
          success: false,
          error: queryResult.error ?? 'Failed to query tasks'
        });
      }

      // Filter out completed tasks by default (unless explicitly included)
      let filteredData = queryResult.data;
      if (!listParams.includeCompleted) {
        filteredData = queryResult.data.filter(record => record.data.status !== 'completed');
      }

      // Convert DataRecord<TaskEntity>[] to TaskSummary[]
      const tasks: TaskSummary[] = filteredData.map(record => {
        const task = record.data;

        // Helper to convert Date or ISO string to ISO string
        const toISOString = (value: Date | string | undefined | null): string | undefined => {
          if (!value) return undefined;
          return typeof value === 'string' ? value : value.toISOString();
        };

        return {
          id: task.id,
          assigneeId: task.assigneeId,
          createdBy: task.createdBy,
          domain: task.domain,
          taskType: task.taskType,
          contextId: task.contextId,
          description: task.description,
          priority: task.priority,
          status: task.status,
          createdAt: toISOString(task.createdAt)!,
          updatedAt: toISOString(task.updatedAt)!,
          startedAt: toISOString(task.startedAt),
          completedAt: toISOString(task.completedAt),
          dueDate: toISOString(task.dueDate),
          estimatedDuration: task.estimatedDuration,
          dependsOn: task.dependsOn,
          blockedBy: task.blockedBy
        };
      });

      // Calculate statistics
      const stats = {
        total: tasks.length,
        pending: tasks.filter(t => t.status === 'pending').length,
        inProgress: tasks.filter(t => t.status === 'in_progress').length,
        completed: tasks.filter(t => t.status === 'completed').length,
        failed: tasks.filter(t => t.status === 'failed').length,
        cancelled: tasks.filter(t => t.status === 'cancelled').length
      };

      // Sort tasks
      const sortedTasks = this.sortTasks(tasks, listParams.sortBy, listParams.sortOrder);

      console.log(`✅ TASK LIST: Found ${tasks.length} tasks`);
      console.log(`   Pending: ${stats.pending}, In Progress: ${stats.inProgress}, Completed: ${stats.completed}`);

      return transformPayload(params, {
        success: true,
        tasks: sortedTasks,
        stats
      });

    } catch (error) {
      console.error('❌ TASK LIST: Error:', error);
      return transformPayload(params, {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }

  /**
   * Sort tasks by specified field and order
   */
  private sortTasks(
    tasks: TaskSummary[],
    sortBy: TaskListParams['sortBy'] = 'priority',
    sortOrder: TaskListParams['sortOrder'] = 'desc'
  ): TaskSummary[] {
    const sorted = [...tasks];

    sorted.sort((a, b) => {
      let comparison = 0;

      switch (sortBy) {
        case 'priority':
          comparison = a.priority - b.priority;
          break;
        case 'created':
          comparison = new Date(a.createdAt).getTime() - new Date(b.createdAt).getTime();
          break;
        case 'dueDate':
          const aDate = a.dueDate ? new Date(a.dueDate).getTime() : Infinity;
          const bDate = b.dueDate ? new Date(b.dueDate).getTime() : Infinity;
          comparison = aDate - bDate;
          break;
        case 'status':
          comparison = a.status.localeCompare(b.status);
          break;
        default:
          comparison = a.priority - b.priority;
      }

      return sortOrder === 'desc' ? -comparison : comparison;
    });

    return sorted;
  }
}
