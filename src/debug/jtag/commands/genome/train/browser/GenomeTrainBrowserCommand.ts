/**
 * Genome Train Browser Command (Stub)
 *
 * Browser environments cannot execute fine-tuning (requires Node.js).
 * This command delegates to server or returns an error.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { GenomeTrainParams, GenomeTrainResult } from '../shared/GenomeTrainTypes';

/**
 * Genome Train Browser Command
 *
 * Browser stub - delegates to server for actual training execution
 */
export class GenomeTrainBrowserCommand extends CommandBase<GenomeTrainParams, GenomeTrainResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome-train', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<GenomeTrainResult> {
    // Browser cannot execute training (requires Node.js for Python subprocess)
    // Delegate to server command
    console.log('🧬 GENOME TRAIN (BROWSER): Delegating to server...');

    return transformPayload(params, {
      success: false,
      error: 'genome/train must be executed from server environment (requires Node.js)'
    });
  }
}
