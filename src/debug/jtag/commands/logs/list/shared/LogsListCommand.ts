/**
 * logs/list Base Command
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { LogsListParams, LogsListResult } from './LogsListTypes';

export abstract class LogsListCommand extends CommandBase<LogsListParams, LogsListResult> {
  constructor(path: string, context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(path, context, subpath, commander);
  }
  abstract execute(params: LogsListParams): Promise<LogsListResult>;
}
