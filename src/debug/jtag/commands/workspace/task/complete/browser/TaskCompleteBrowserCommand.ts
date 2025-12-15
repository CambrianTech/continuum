/**
 * TaskCompleteBrowserCommand - Browser stub for task completion
 *
 * All task updates happen server-side.
 * Browser just forwards the request.
 */

import { CommandBase } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { TaskCompleteParams, TaskCompleteResult } from '../shared/TaskCompleteTypes';

export class TaskCompleteBrowserCommand extends CommandBase<TaskCompleteParams, TaskCompleteResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('workspace/task/complete', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<TaskCompleteResult> {
    // Browser doesn't update tasks locally - forward to server
    return transformPayload(params, {
      success: false,
      error: 'Task completion must happen on server'
    });
  }
}
