/**
 * Migration Verify Command - Browser Implementation
 *
 * Verify migration integrity by comparing record counts between source and target
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { MigrationVerifyParams, MigrationVerifyResult } from '../shared/MigrationVerifyTypes';

export class MigrationVerifyBrowserCommand extends CommandBase<MigrationVerifyParams, MigrationVerifyResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('migration/verify', context, subpath, commander);
  }

  async execute(params: MigrationVerifyParams): Promise<MigrationVerifyResult> {
    console.log('🌐 BROWSER: Delegating Migration Verify to server');
    return await this.remoteExecute(params);
  }
}
