/**
 * Genome Academy Session List Command - Browser Implementation
 *
 * Delegates to server for Academy session listing.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GenomeAcademySessionListParams, GenomeAcademySessionListResult } from '../shared/GenomeAcademySessionListTypes';

export class GenomeAcademySessionListBrowserCommand extends CommandBase<GenomeAcademySessionListParams, GenomeAcademySessionListResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/academy-session-list', context, subpath, commander);
  }

  async execute(params: GenomeAcademySessionListParams): Promise<GenomeAcademySessionListResult> {
    return await this.remoteExecute(params);
  }
}
