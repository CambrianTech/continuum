import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { LogsReadParams, LogsReadResult } from './LogsReadTypes';

export abstract class LogsReadCommand extends CommandBase<LogsReadParams, LogsReadResult> {
  constructor(path: string, context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(path, context, subpath, commander);
  }
  abstract execute(params: LogsReadParams): Promise<LogsReadResult>;
}
