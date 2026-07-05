/**
 * Migration Pause Command - Browser Implementation
 *
 * Pause an in-flight migration. Can be resumed later from the last checkpoint.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { MigrationPauseParams, MigrationPauseResult } from '../shared/MigrationPauseTypes';

export class MigrationPauseBrowserCommand extends CommandBase<MigrationPauseParams, MigrationPauseResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('migration/pause', context, subpath, commander);
  }

  async execute(params: MigrationPauseParams): Promise<MigrationPauseResult> {
    console.log('🌐 BROWSER: Delegating Migration Pause to server');
    return await this.remoteExecute(params);
  }
}
