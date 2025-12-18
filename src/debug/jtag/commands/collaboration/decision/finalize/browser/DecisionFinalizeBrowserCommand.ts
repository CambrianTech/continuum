/**
 * Decision Finalize Command - Browser Implementation
 *
 * Close voting and calculate winner using ranked-choice voting
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { DecisionFinalizeParams, DecisionFinalizeResult } from '../shared/DecisionFinalizeTypes';

export class DecisionFinalizeBrowserCommand extends CommandBase<DecisionFinalizeParams, DecisionFinalizeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Decision Finalize', context, subpath, commander);
  }

  async execute(params: DecisionFinalizeParams): Promise<DecisionFinalizeResult> {
    console.log('🌐 BROWSER: Delegating Decision Finalize to server');
    return await this.remoteExecute(params);
  }
}
