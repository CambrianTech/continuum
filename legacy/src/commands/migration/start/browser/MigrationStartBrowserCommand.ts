/**
 * Migration Start Command - Browser Implementation
 *
 * Start streaming data migration between any two storage adapters (e.g., SQLite to PostgreSQL)
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { MigrationStartParams, MigrationStartResult } from '../shared/MigrationStartTypes';

export class MigrationStartBrowserCommand extends CommandBase<MigrationStartParams, MigrationStartResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('migration/start', context, subpath, commander);
  }

  async execute(params: MigrationStartParams): Promise<MigrationStartResult> {
    console.log('🌐 BROWSER: Delegating Migration Start to server');
    return await this.remoteExecute(params);
  }
}
