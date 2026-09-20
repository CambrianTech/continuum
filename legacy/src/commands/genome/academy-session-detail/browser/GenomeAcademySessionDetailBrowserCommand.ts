/**
 * Genome Academy Session Detail Command - Browser Implementation
 *
 * Delegates to server for session detail retrieval.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type {
  GenomeAcademySessionDetailParams,
  GenomeAcademySessionDetailResult,
} from '../shared/GenomeAcademySessionDetailTypes';

export class GenomeAcademySessionDetailBrowserCommand extends CommandBase<
  GenomeAcademySessionDetailParams,
  GenomeAcademySessionDetailResult
> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/academy-session-detail', context, subpath, commander);
  }

  async execute(params: GenomeAcademySessionDetailParams): Promise<GenomeAcademySessionDetailResult> {
    return await this.remoteExecute(params);
  }
}
