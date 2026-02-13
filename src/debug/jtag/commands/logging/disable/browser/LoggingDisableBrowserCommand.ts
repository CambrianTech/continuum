/**
 * Logging Disable Command - Browser Implementation
 *
 * Disable logging for a persona. Persists to .continuum/logging.json
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { LoggingDisableParams, LoggingDisableResult } from '../shared/LoggingDisableTypes';

export class LoggingDisableBrowserCommand extends CommandBase<LoggingDisableParams, LoggingDisableResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('logging/disable', context, subpath, commander);
  }

  async execute(params: LoggingDisableParams): Promise<LoggingDisableResult> {
    console.log('🌐 BROWSER: Delegating Logging Disable to server');
    return await this.remoteExecute(params);
  }
}
