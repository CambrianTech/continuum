/**
 * Decision List Command - Browser Implementation
 *
 * List all governance proposals with optional filtering
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { DecisionListParams, DecisionListResult } from '../shared/DecisionListTypes';

export class DecisionListBrowserCommand extends CommandBase<DecisionListParams, DecisionListResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Decision List', context, subpath, commander);
  }

  async execute(params: DecisionListParams): Promise<DecisionListResult> {
    console.log('🌐 BROWSER: Delegating Decision List to server');
    return await this.remoteExecute(params);
  }
}
