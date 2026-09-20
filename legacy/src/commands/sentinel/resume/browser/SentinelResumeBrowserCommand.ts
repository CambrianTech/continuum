/**
 * Sentinel Resume Command - Browser Implementation
 *
 * Resume a pipeline from a durable checkpoint. Only works for pipelines in Interrupted, Paused, or BudgetExhausted status.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { SentinelResumeParams, SentinelResumeResult } from '../shared/SentinelResumeTypes';

export class SentinelResumeBrowserCommand extends CommandBase<SentinelResumeParams, SentinelResumeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('sentinel/resume', context, subpath, commander);
  }

  async execute(params: SentinelResumeParams): Promise<SentinelResumeResult> {
    console.log('🌐 BROWSER: Delegating Sentinel Resume to server');
    return await this.remoteExecute(params);
  }
}
