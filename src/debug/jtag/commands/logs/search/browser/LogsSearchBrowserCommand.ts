import { LogsSearchCommand } from '../shared/LogsSearchCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { LogsSearchParams, LogsSearchResult } from '../shared/LogsSearchTypes';
export class LogsSearchBrowserCommand extends LogsSearchCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) { super('logs/search', context, subpath, commander); }
  async execute(params: LogsSearchParams): Promise<LogsSearchResult> {
    return await this.remoteExecute({...params, context: params.context ?? this.context, sessionId: params.sessionId ?? ''});
  }
}
