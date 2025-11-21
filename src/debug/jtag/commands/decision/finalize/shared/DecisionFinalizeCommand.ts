/**
 * decision/finalize - Base command class
 * Server-only command that manually finalizes voting and calculates winner
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DecisionFinalizeParams, DecisionFinalizeResult } from './DecisionFinalizeTypes';

export abstract class DecisionFinalizeCommand extends CommandBase<DecisionFinalizeParams, DecisionFinalizeResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('decision/finalize', context, subpath, commander);
  }

  async execute(params: DecisionFinalizeParams): Promise<DecisionFinalizeResult> {
    if (this.context.environment !== 'server') {
      return await this.remoteExecute(params);
    }
    return await this.executeCommand(params);
  }

  protected abstract executeCommand(params: DecisionFinalizeParams): Promise<DecisionFinalizeResult>;
}
