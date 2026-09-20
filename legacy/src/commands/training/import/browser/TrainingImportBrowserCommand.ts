/**
 * Training Import Command - Browser Implementation
 *
 * Delegates to server since file system access required
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import type { TrainingImportParams, TrainingImportResult } from '../shared/TrainingImportTypes';
import { Commands } from '../../../../system/core/shared/Commands';

import { TrainingImport } from '../shared/TrainingImportTypes';
export class TrainingImportBrowserCommand extends CommandBase<TrainingImportParams, TrainingImportResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('training/import', context, subpath, commander);
  }

  /**
   * Browser delegates to server for file system access
   */
  async execute(params: JTAGPayload): Promise<TrainingImportResult> {
    return await TrainingImport.execute(params as TrainingImportParams) as TrainingImportResult;
  }
}
