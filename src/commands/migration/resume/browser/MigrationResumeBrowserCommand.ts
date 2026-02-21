/**
 * Migration Resume Command - Browser Implementation
 *
 * Resume a paused migration from its last checkpoint
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { MigrationResumeParams, MigrationResumeResult } from '../shared/MigrationResumeTypes';

export class MigrationResumeBrowserCommand extends CommandBase<MigrationResumeParams, MigrationResumeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('migration/resume', context, subpath, commander);
  }

  async execute(params: MigrationResumeParams): Promise<MigrationResumeResult> {
    console.log('🌐 BROWSER: Delegating Migration Resume to server');
    return await this.remoteExecute(params);
  }
}
