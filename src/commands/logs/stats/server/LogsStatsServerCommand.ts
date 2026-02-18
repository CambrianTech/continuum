import { LogsStatsCommand } from '../shared/LogsStatsCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { LogsStatsParams, LogsStatsResult } from '../shared/LogsStatsTypes';
export class LogsStatsServerCommand extends LogsStatsCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) { super('logs/stats', context, subpath, commander); }
  async execute(params: LogsStatsParams): Promise<LogsStatsResult> {
    return { context: params.context, sessionId: params.sessionId, success: true, totalFiles: 0, totalSizeMB: 0 };
  }
}
