/**
 * Sentinel Approve Command - Browser Implementation
 *
 * Approve or reject a pending pipeline approval step. Resolves the blocking approval gate in the Rust executor.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { SentinelApproveParams, SentinelApproveResult } from '../shared/SentinelApproveTypes';

export class SentinelApproveBrowserCommand extends CommandBase<SentinelApproveParams, SentinelApproveResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('sentinel/approve', context, subpath, commander);
  }

  async execute(params: SentinelApproveParams): Promise<SentinelApproveResult> {
    console.log('🌐 BROWSER: Delegating Sentinel Approve to server');
    return await this.remoteExecute(params);
  }
}
