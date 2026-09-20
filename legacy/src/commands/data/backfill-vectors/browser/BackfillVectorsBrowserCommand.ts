/**
 * Backfill Vectors Browser Command
 * Delegates to server for batch embedding generation
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type {
  BackfillVectorsParams,
  BackfillVectorsResult
} from '../shared/BackfillVectorsCommandTypes';
import { createBackfillVectorsResultFromParams } from '../shared/BackfillVectorsCommandTypes';

export class BackfillVectorsBrowserCommand extends CommandBase<BackfillVectorsParams, BackfillVectorsResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-backfill-vectors', context, subpath, commander);
  }

  async execute(params: BackfillVectorsParams): Promise<BackfillVectorsResult> {
    console.debug('🌐 BACKFILL-VECTORS (Browser): Delegating to server...');

    try {
      // Validate required parameters
      if (!params.collection || params.collection.trim().length === 0) {
        return createBackfillVectorsResultFromParams(params, {
          success: false,
          error: 'Collection parameter is required'
        });
      }

      if (!params.textField || params.textField.trim().length === 0) {
        return createBackfillVectorsResultFromParams(params, {
          success: false,
          error: 'textField parameter is required'
        });
      }

      // Delegate to server via command routing
      return await this.remoteExecute(params);

    } catch (error: unknown) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error('❌ BACKFILL-VECTORS (Browser): Execution failed:', errorMessage);
      return createBackfillVectorsResultFromParams(params, {
        success: false,
        error: `Backfill vectors failed: ${errorMessage}`
      });
    }
  }
}
