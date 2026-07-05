/**
 * Genome Academy Competition Command - Browser Implementation
 *
 * Delegates to server for competition orchestration.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GenomeAcademyCompetitionParams, GenomeAcademyCompetitionResult } from '../shared/GenomeAcademyCompetitionTypes';

export class GenomeAcademyCompetitionBrowserCommand extends CommandBase<GenomeAcademyCompetitionParams, GenomeAcademyCompetitionResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/academy-competition', context, subpath, commander);
  }

  async execute(params: GenomeAcademyCompetitionParams): Promise<GenomeAcademyCompetitionResult> {
    console.log('🌐 BROWSER: Delegating Genome Academy Competition to server');
    return await this.remoteExecute(params);
  }
}
