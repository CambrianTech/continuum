/**
 * TaskCompleteServerCommand - Server-side task completion
 *
 * Updates task status and records results.
 * Calculates task duration and updates database.
 */

import { CommandBase } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { TaskCompleteParams, TaskCompleteResult } from '../shared/TaskCompleteTypes';
import { ORM } from '@daemons/data-daemon/shared/ORM';
import { COLLECTIONS } from '@system/data/config/DatabaseConfig';
import type { TaskEntity } from '@system/data/entities/TaskEntity';

export class TaskCompleteServerCommand extends CommandBase<TaskCompleteParams, TaskCompleteResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('workspace/task/complete', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<TaskCompleteResult> {
    const completeParams = params as TaskCompleteParams;

    console.log('📋 TASK COMPLETE: Completing task');
    console.log(`   Task ID: ${completeParams.taskId.slice(0, 8)}...`);
    console.log(`   Success: ${completeParams.success}`);

    try {
      // Fetch existing task
      const queryResult = await ORM.query<TaskEntity>({
        collection: COLLECTIONS.TASKS,
        filter: { id: completeParams.taskId },
        limit: 1
      });

      if (!queryResult.success || !queryResult.data || queryResult.data.length === 0) {
        return transformPayload(params, {
          success: false,
          error: `Task not found: ${completeParams.taskId}`
        });
      }

      const taskRecord = queryResult.data[0];
      const task = taskRecord.data;

      // Check if task is already completed
      if (task.status === 'completed' || task.status === 'failed' || task.status === 'cancelled') {
        return transformPayload(params, {
          success: false,
          error: `Task already ${task.status}: ${completeParams.taskId}`
        });
      }

      // Calculate duration if task was started
      const completedAt = new Date();
      const duration = task.startedAt
        ? completedAt.getTime() - task.startedAt.getTime()
        : undefined;

      // Build result object
      const result: TaskEntity['result'] = {
        success: completeParams.success,
        output: completeParams.output,
        error: completeParams.error,
        metrics: completeParams.metrics
      };

      // Update task
      const updatedTask: Partial<TaskEntity> = {
        status: completeParams.success ? 'completed' : 'failed',
        completedAt,
        updatedAt: completedAt,
        result
      };

      // ORM.update(collection, id, data, incrementVersion?)
      const updatedTaskEntity = await ORM.update(
        COLLECTIONS.TASKS,
        completeParams.taskId,
        updatedTask,
        true
      );

      if (!updatedTaskEntity) {
        return transformPayload(params, {
          success: false,
          error: 'Failed to update task'
        });
      }

      console.log(`✅ TASK COMPLETE: Task ${completeParams.success ? 'completed' : 'failed'} successfully`);
      if (duration) {
        console.log(`   Duration: ${(duration / 1000).toFixed(2)}s`);
      }

      return transformPayload(params, {
        success: true,
        task: {
          id: completeParams.taskId,
          status: updatedTask.status ?? 'completed',
          completedAt: completedAt.toISOString(),
          duration
        }
      });

    } catch (error) {
      console.error('❌ TASK COMPLETE: Error:', error);
      return transformPayload(params, {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }
}
