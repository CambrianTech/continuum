/**
 * Hello Command - Browser Implementation
 *
 * Simple hello world command for testing
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { HelloParams, HelloResult } from '../shared/HelloTypes';

export class HelloBrowserCommand extends CommandBase<HelloParams, HelloResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('utilities/hello', context, subpath, commander);
  }

  async execute(params: HelloParams): Promise<HelloResult> {
    console.log('🌐 BROWSER: Delegating Hello to server');
    return await this.remoteExecute(params);
  }
}
