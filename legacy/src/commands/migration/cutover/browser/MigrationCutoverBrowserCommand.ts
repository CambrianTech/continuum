/**
 * Migration Cutover Command - Browser Implementation
 *
 * Switch all operations from current adapter to the migration target. Saves previous connection for rollback.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { MigrationCutoverParams, MigrationCutoverResult } from '../shared/MigrationCutoverTypes';

export class MigrationCutoverBrowserCommand extends CommandBase<MigrationCutoverParams, MigrationCutoverResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('migration/cutover', context, subpath, commander);
  }

  async execute(params: MigrationCutoverParams): Promise<MigrationCutoverResult> {
    console.log('🌐 BROWSER: Delegating Migration Cutover to server');
    return await this.remoteExecute(params);
  }
}
