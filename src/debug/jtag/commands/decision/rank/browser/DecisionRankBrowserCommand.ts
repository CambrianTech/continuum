/**
 * decision/rank - Browser-side stub
 * All logic is on the server side
 */

import { DecisionRankCommand } from '../shared/DecisionRankCommand';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DecisionRankParams, DecisionRankResult } from '../shared/DecisionRankTypes';

export class DecisionRankBrowserCommand extends DecisionRankCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeCommand(params: DecisionRankParams): Promise<DecisionRankResult> {
    return transformPayload(params, {
      success: false,
      error: 'decision/rank must be executed on server'
    });
  }
}
