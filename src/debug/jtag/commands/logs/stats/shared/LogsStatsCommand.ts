import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { LogsStatsParams, LogsStatsResult } from './LogsStatsTypes';
export abstract class LogsStatsCommand extends CommandBase<LogsStatsParams, LogsStatsResult> {
  constructor(path: string, context: JTAGContext, subpath: string, commander: ICommandDaemon) { super(path, context, subpath, commander); }
  abstract execute(params: LogsStatsParams): Promise<LogsStatsResult>;
}
