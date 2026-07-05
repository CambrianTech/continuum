/**
 * Decision Vote Command - Browser Implementation
 *
 * Cast ranked-choice vote on a proposal
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { DecisionVoteParams, DecisionVoteResult } from '../shared/DecisionVoteTypes';

export class DecisionVoteBrowserCommand extends CommandBase<DecisionVoteParams, DecisionVoteResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Decision Vote', context, subpath, commander);
  }

  async execute(params: DecisionVoteParams): Promise<DecisionVoteResult> {
    console.log('🌐 BROWSER: Delegating Decision Vote to server');
    return await this.remoteExecute(params);
  }
}
