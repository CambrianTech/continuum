/**
 * DecisionProposeCommand - Shared base class
 *
 * Server-only command for creating decision proposals with ranked-choice voting
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DecisionProposeParams, DecisionProposeResult } from './DecisionProposeTypes';

export abstract class DecisionProposeCommand extends CommandBase<DecisionProposeParams, DecisionProposeResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('decision/propose', context, subpath, commander);
  }

  /**
   * Execute with environment routing
   * Server-only command - always routes to server
   */
  async execute(params: DecisionProposeParams): Promise<DecisionProposeResult> {
    // Server-only command - always route to server
    if (this.context.environment !== 'server') {
      return await this.remoteExecute(params);
    }
    return await this.executeCommand(params);
  }

  /**
   * Subclasses implement the actual command logic
   */
  protected abstract executeCommand(params: DecisionProposeParams): Promise<DecisionProposeResult>;
}
