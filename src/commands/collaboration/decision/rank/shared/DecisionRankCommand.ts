/**
 * decision/rank - Base command class
 * Server-only command that handles vote submission and Condorcet winner calculation
 */

import { CommandBase } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { DecisionRankParams, DecisionRankResult } from './DecisionRankTypes';

export abstract class DecisionRankCommand extends CommandBase<DecisionRankParams, DecisionRankResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/decision/rank', context, subpath, commander);
  }

  async execute(params: DecisionRankParams): Promise<DecisionRankResult> {
    if (this.context.environment !== 'server') {
      return await this.remoteExecute(params);
    }
    return await this.executeCommand(params);
  }

  protected abstract executeCommand(params: DecisionRankParams): Promise<DecisionRankResult>;
}
