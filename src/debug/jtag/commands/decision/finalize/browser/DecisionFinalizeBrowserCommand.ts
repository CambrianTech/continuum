/**
 * decision/finalize - Browser-side stub
 * All logic is on the server side
 */

import { DecisionFinalizeCommand } from '../shared/DecisionFinalizeCommand';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DecisionFinalizeParams, DecisionFinalizeResult } from '../shared/DecisionFinalizeTypes';

export class DecisionFinalizeBrowserCommand extends DecisionFinalizeCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeCommand(params: DecisionFinalizeParams): Promise<DecisionFinalizeResult> {
    return transformPayload(params, {
      success: false,
      error: 'decision/finalize must be executed on server'
    });
  }
}
