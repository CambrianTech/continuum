/**
 * Migration Rollback Command - Browser Implementation
 *
 * Revert to the previous connection string after a cutover. Source data is never deleted.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { MigrationRollbackParams, MigrationRollbackResult } from '../shared/MigrationRollbackTypes';

export class MigrationRollbackBrowserCommand extends CommandBase<MigrationRollbackParams, MigrationRollbackResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('migration/rollback', context, subpath, commander);
  }

  async execute(params: MigrationRollbackParams): Promise<MigrationRollbackResult> {
    console.log('🌐 BROWSER: Delegating Migration Rollback to server');
    return await this.remoteExecute(params);
  }
}
