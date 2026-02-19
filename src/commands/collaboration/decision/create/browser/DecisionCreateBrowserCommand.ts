/**
 * Decision Create Command - Browser Implementation
 *
 * Create a new governance proposal with voting options
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { DecisionCreateParams, DecisionCreateResult } from '../shared/DecisionCreateTypes';

export class DecisionCreateBrowserCommand extends CommandBase<DecisionCreateParams, DecisionCreateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Decision Create', context, subpath, commander);
  }

  async execute(params: DecisionCreateParams): Promise<DecisionCreateResult> {
    console.log('🌐 BROWSER: Delegating Decision Create to server');
    return await this.remoteExecute(params);
  }
}
