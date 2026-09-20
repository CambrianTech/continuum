/**
 * Genome Academy Team Command - Browser Implementation
 *
 * Start a collaborative team training project. Decomposes a project description into roles, trains each student for their role, then orchestrates collaborative building. Teacher grades both the overall project and individual role performance. Students communicate via the academy chat room.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GenomeAcademyTeamParams, GenomeAcademyTeamResult } from '../shared/GenomeAcademy-teamTypes';

export class GenomeAcademyTeamBrowserCommand extends CommandBase<GenomeAcademyTeamParams, GenomeAcademyTeamResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/academy-team', context, subpath, commander);
  }

  async execute(params: GenomeAcademyTeamParams): Promise<GenomeAcademyTeamResult> {
    console.log('🌐 BROWSER: Delegating Genome Academy Team to server');
    return await this.remoteExecute(params);
  }
}
