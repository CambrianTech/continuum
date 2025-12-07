/**
 * logs/list Browser Command
 */

import { LogsListCommand } from '../shared/LogsListCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { LogsListParams, LogsListResult } from '../shared/LogsListTypes';

export class LogsListBrowserCommand extends LogsListCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('logs/list', context, subpath, commander);
  }

  async execute(params: LogsListParams): Promise<LogsListResult> {
    return await this.remoteExecute({
      ...params,
      context: params.context ?? this.context,
      sessionId: params.sessionId ?? ''
    });
  }
}
