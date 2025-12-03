import { LogsSearchCommand } from '../shared/LogsSearchCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { LogsSearchParams, LogsSearchResult } from '../shared/LogsSearchTypes';
export class LogsSearchServerCommand extends LogsSearchCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) { super('logs/search', context, subpath, commander); }
  async execute(params: LogsSearchParams): Promise<LogsSearchResult> {
    return { context: params.context, sessionId: params.sessionId, success: true, matches: [], totalMatches: 0 };
  }
}
