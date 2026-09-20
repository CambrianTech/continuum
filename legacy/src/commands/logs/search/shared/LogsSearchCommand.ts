import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { LogsSearchParams, LogsSearchResult } from './LogsSearchTypes';
export abstract class LogsSearchCommand extends CommandBase<LogsSearchParams, LogsSearchResult> {
  constructor(path: string, context: JTAGContext, subpath: string, commander: ICommandDaemon) { super(path, context, subpath, commander); }
  abstract execute(params: LogsSearchParams): Promise<LogsSearchResult>;
}
