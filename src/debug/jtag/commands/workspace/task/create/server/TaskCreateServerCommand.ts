/**
 * TaskCreateServerCommand - Server-side task creation
 *
 * Creates new tasks and stores them in the database.
 * Tasks can be assigned to any PersonaUser for autonomous work.
 */

import { CommandBase } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { TaskCreateParams, TaskCreateResult } from '../shared/TaskCreateTypes';
import { ORM } from '@daemons/data-daemon/server/ORM';
import { COLLECTIONS } from '@system/data/config/DatabaseConfig';
import type { TaskEntity } from '@system/data/entities/TaskEntity';
import { v4 as uuidv4 } from 'uuid';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export class TaskCreateServerCommand extends CommandBase<TaskCreateParams, TaskCreateResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('workspace/task/create', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<TaskCreateResult> {
    const createParams = params as TaskCreateParams;

    console.log('📋 TASK CREATE: Creating new task');
    console.log(`   Assignee: ${createParams.assigneeId.slice(0, 8)}...`);
    console.log(`   Domain: ${createParams.domain}`);
    console.log(`   Type: ${createParams.taskType}`);

    try {
      // Validate assignee exists
      const assigneeResult = await ORM.query({
        collection: COLLECTIONS.USERS,
        filter: { id: createParams.assigneeId },
        limit: 1
      });

      if (!assigneeResult.success || !assigneeResult.data || assigneeResult.data.length === 0) {
        return transformPayload(params, {
          success: false,
          error: `Assignee not found: ${createParams.assigneeId}`
        });
      }

      // Create task entity
      const taskId = uuidv4() as UUID;
      const now = new Date();

      const taskData: Partial<TaskEntity> = {
        id: taskId,
        assigneeId: createParams.assigneeId,
        createdBy: createParams.assigneeId, // Created by the assignee for now
        domain: createParams.domain,
        taskType: createParams.taskType,
        contextId: createParams.contextId,
        description: createParams.description,
        priority: createParams.priority ?? 0.5,
        dueDate: createParams.dueDate ? new Date(createParams.dueDate) : undefined,
        estimatedDuration: createParams.estimatedDuration,
        status: 'pending',
        dependsOn: createParams.dependsOn,
        metadata: createParams.metadata,
        createdAt: now,
        updatedAt: now
      };

      // Store task in database - ORM.store(collection, data)
      const storedTask = await ORM.store(COLLECTIONS.TASKS, taskData as TaskEntity);

      if (!storedTask) {
        return transformPayload(params, {
          success: false,
          error: 'Failed to store task'
        });
      }

      console.log(`✅ TASK CREATE: Task created successfully (${taskId.slice(0, 8)}...)`);

      return transformPayload(params, {
        success: true,
        taskId,
        task: {
          id: taskId,
          assigneeId: createParams.assigneeId,
          domain: createParams.domain,
          taskType: createParams.taskType,
          description: createParams.description,
          priority: taskData.priority ?? 0.5,
          status: 'pending',
          createdAt: now.toISOString()
        }
      });

    } catch (error) {
      console.error('❌ TASK CREATE: Error:', error);
      return transformPayload(params, {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }
}
