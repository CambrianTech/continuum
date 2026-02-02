/**
 * Challenge List Command - Browser Implementation
 *
 * List available coding challenges with their difficulty, status, and best scores. Shows progressive challenge sequence for AI training.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ChallengeListParams, ChallengeListResult } from '../shared/ChallengeListTypes';

export class ChallengeListBrowserCommand extends CommandBase<ChallengeListParams, ChallengeListResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('challenge/list', context, subpath, commander);
  }

  async execute(params: ChallengeListParams): Promise<ChallengeListResult> {
    console.log('🌐 BROWSER: Delegating Challenge List to server');
    return await this.remoteExecute(params);
  }
}
