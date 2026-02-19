/**
 * TaskListBrowserCommand - Browser stub for task listing
 *
 * All task queries happen server-side.
 * Browser just forwards the request.
 */

import { CommandBase } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { TaskListParams, TaskListResult } from '../shared/TaskListTypes';

export class TaskListBrowserCommand extends CommandBase<TaskListParams, TaskListResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('workspace/task/list', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<TaskListResult> {
    // Browser doesn't query tasks locally - forward to server
    return transformPayload(params, {
      success: false,
      error: 'Task listing must happen on server'
    });
  }
}
