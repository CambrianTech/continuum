/**
 * Logging Status Command - Browser Implementation
 *
 * Show current logging configuration for all personas or a specific persona
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { LoggingStatusParams, LoggingStatusResult } from '../shared/LoggingStatusTypes';

export class LoggingStatusBrowserCommand extends CommandBase<LoggingStatusParams, LoggingStatusResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('logging/status', context, subpath, commander);
  }

  async execute(params: LoggingStatusParams): Promise<LoggingStatusResult> {
    console.log('🌐 BROWSER: Delegating Logging Status to server');
    return await this.remoteExecute(params);
  }
}
