/**
 * TaskCreateBrowserCommand - Browser stub for task creation
 *
 * All task creation happens server-side.
 * Browser just forwards the request.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { TaskCreateParams, TaskCreateResult } from '../shared/TaskCreateTypes';

export class TaskCreateBrowserCommand extends CommandBase<TaskCreateParams, TaskCreateResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('task-create', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<TaskCreateResult> {
    // Browser doesn't create tasks locally - forward to server
    return transformPayload(params, {
      success: false,
      error: 'Task creation must happen on server'
    });
  }
}
