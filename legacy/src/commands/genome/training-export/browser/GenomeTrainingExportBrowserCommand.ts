/**
 * Genome Training Export Command - Browser Implementation
 *
 * Delegates to server — training data accumulator is server-side only.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GenomeTrainingExportParams, GenomeTrainingExportResult } from '../shared/GenomeTrainingExportTypes';

export class GenomeTrainingExportBrowserCommand extends CommandBase<GenomeTrainingExportParams, GenomeTrainingExportResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/training-export', context, subpath, commander);
  }

  async execute(params: GenomeTrainingExportParams): Promise<GenomeTrainingExportResult> {
    console.log('🌐 BROWSER: Delegating Genome Training Export to server');
    return await this.remoteExecute(params);
  }
}
