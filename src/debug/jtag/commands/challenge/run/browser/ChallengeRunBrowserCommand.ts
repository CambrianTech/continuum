/**
 * Challenge Run Command - Browser Implementation
 *
 * Run a coding challenge against the AI coding pipeline. Sets up a fresh workspace, executes the challenge via code/task, evaluates with AI judge, and records the attempt.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ChallengeRunParams, ChallengeRunResult } from '../shared/ChallengeRunTypes';

export class ChallengeRunBrowserCommand extends CommandBase<ChallengeRunParams, ChallengeRunResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('challenge/run', context, subpath, commander);
  }

  async execute(params: ChallengeRunParams): Promise<ChallengeRunResult> {
    console.log('🌐 BROWSER: Delegating Challenge Run to server');
    return await this.remoteExecute(params);
  }
}
