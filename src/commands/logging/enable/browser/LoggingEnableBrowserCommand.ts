/**
 * Logging Enable Command - Browser Implementation
 *
 * Enable logging for a persona. Persists to .continuum/logging.json
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { LoggingEnableParams, LoggingEnableResult } from '../shared/LoggingEnableTypes';

export class LoggingEnableBrowserCommand extends CommandBase<LoggingEnableParams, LoggingEnableResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('logging/enable', context, subpath, commander);
  }

  async execute(params: LoggingEnableParams): Promise<LoggingEnableResult> {
    console.log('🌐 BROWSER: Delegating Logging Enable to server');
    return await this.remoteExecute(params);
  }
}
