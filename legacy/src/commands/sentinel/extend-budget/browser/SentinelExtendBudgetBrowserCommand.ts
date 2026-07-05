/**
 * Sentinel Extend Budget Command - Browser Implementation
 *
 * Extend budget limits for a running or paused pipeline. Merges new limits into existing checkpoint budget.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { SentinelExtendBudgetParams, SentinelExtendBudgetResult } from '../shared/SentinelExtendBudgetTypes';

export class SentinelExtendBudgetBrowserCommand extends CommandBase<SentinelExtendBudgetParams, SentinelExtendBudgetResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('sentinel/extend-budget', context, subpath, commander);
  }

  async execute(params: SentinelExtendBudgetParams): Promise<SentinelExtendBudgetResult> {
    console.log('🌐 BROWSER: Delegating Sentinel Extend Budget to server');
    return await this.remoteExecute(params);
  }
}
