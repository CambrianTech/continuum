import { LogsReadCommand } from '../shared/LogsReadCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { LogsReadParams, LogsReadResult } from '../shared/LogsReadTypes';

export class LogsReadBrowserCommand extends LogsReadCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('logs/read', context, subpath, commander);
  }

  async execute(params: LogsReadParams): Promise<LogsReadResult> {
    return await this.remoteExecute({...params, context: params.context ?? this.context, sessionId: params.sessionId ?? ''});
  }
}
