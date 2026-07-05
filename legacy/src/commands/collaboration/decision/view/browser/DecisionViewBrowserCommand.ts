/**
 * Decision View Command - Browser Implementation
 *
 * View detailed information about a specific governance proposal
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { DecisionViewParams, DecisionViewResult } from '../shared/DecisionViewTypes';

export class DecisionViewBrowserCommand extends CommandBase<DecisionViewParams, DecisionViewResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Decision View', context, subpath, commander);
  }

  async execute(params: DecisionViewParams): Promise<DecisionViewResult> {
    console.log('🌐 BROWSER: Delegating Decision View to server');
    return await this.remoteExecute(params);
  }
}
