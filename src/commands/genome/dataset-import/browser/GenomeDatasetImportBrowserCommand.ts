/**
 * Genome Dataset Import Command - Browser Implementation
 *
 * Delegates to server since dataset import requires file system access.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GenomeDatasetImportParams, GenomeDatasetImportResult } from '../shared/GenomeDatasetImportTypes';

export class GenomeDatasetImportBrowserCommand extends CommandBase<GenomeDatasetImportParams, GenomeDatasetImportResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/dataset-import', context, subpath, commander);
  }

  async execute(params: GenomeDatasetImportParams): Promise<GenomeDatasetImportResult> {
    console.log('🌐 BROWSER: Delegating Genome Dataset Import to server');
    return await this.remoteExecute(params);
  }
}
