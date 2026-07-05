/**
 * AI Sleep Command - Browser Implementation
 *
 * Routes to server for persona state management.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { AiSleepParams, AiSleepResult } from '../shared/AiSleepTypes';

export class AiSleepBrowserCommand extends CommandBase<AiSleepParams, AiSleepResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/sleep', context, subpath, commander);
  }

  async execute(params: AiSleepParams): Promise<AiSleepResult> {
    console.log('🌐 BROWSER: Delegating ai/sleep to server');
    return await this.remoteExecute(params);
  }
}

export default AiSleepBrowserCommand;
