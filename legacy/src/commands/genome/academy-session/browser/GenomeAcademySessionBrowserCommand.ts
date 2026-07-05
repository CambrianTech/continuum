/**
 * Genome Academy Session Command - Browser Implementation
 *
 * Delegates to server for Academy session orchestration.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GenomeAcademySessionParams, GenomeAcademySessionResult } from '../shared/GenomeAcademySessionTypes';

export class GenomeAcademySessionBrowserCommand extends CommandBase<GenomeAcademySessionParams, GenomeAcademySessionResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/academy-session', context, subpath, commander);
  }

  async execute(params: GenomeAcademySessionParams): Promise<GenomeAcademySessionResult> {
    console.log('🌐 BROWSER: Delegating Genome Academy Session to server');
    return await this.remoteExecute(params);
  }
}
