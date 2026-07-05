/**
 * Logs Config Command - Browser Implementation
 *
 * Get or set logging configuration per persona and category
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { LogsConfigParams, LogsConfigResult } from '../shared/LogsConfigTypes';

export class LogsConfigBrowserCommand extends CommandBase<LogsConfigParams, LogsConfigResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Logs Config', context, subpath, commander);
  }

  async execute(params: LogsConfigParams): Promise<LogsConfigResult> {
    console.log('🌐 BROWSER: Delegating Logs Config to server');
    return await this.remoteExecute(params);
  }
}
