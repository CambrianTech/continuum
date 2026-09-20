/**
 * Migration Status Command - Browser Implementation
 *
 * Get current migration progress with per-collection breakdown
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { MigrationStatusParams, MigrationStatusResult } from '../shared/MigrationStatusTypes';

export class MigrationStatusBrowserCommand extends CommandBase<MigrationStatusParams, MigrationStatusResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('migration/status', context, subpath, commander);
  }

  async execute(params: MigrationStatusParams): Promise<MigrationStatusResult> {
    console.log('🌐 BROWSER: Delegating Migration Status to server');
    return await this.remoteExecute(params);
  }
}
