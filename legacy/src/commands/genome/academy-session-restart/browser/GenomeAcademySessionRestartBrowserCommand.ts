/**
 * Genome Academy Session Restart Command - Browser Implementation
 *
 * Delegates to server for Academy session restart orchestration.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GenomeAcademySessionRestartParams, GenomeAcademySessionRestartResult } from '../shared/GenomeAcademySessionRestartTypes';

export class GenomeAcademySessionRestartBrowserCommand extends CommandBase<GenomeAcademySessionRestartParams, GenomeAcademySessionRestartResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/academy-session-restart', context, subpath, commander);
  }

  async execute(params: GenomeAcademySessionRestartParams): Promise<GenomeAcademySessionRestartResult> {
    console.log('🌐 BROWSER: Delegating Genome Academy Session Restart to server');
    return await this.remoteExecute(params);
  }
}
