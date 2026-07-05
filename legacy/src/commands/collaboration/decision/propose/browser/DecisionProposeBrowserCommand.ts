/**
 * decision/propose - Browser-side stub
 * All logic is on the server side
 */

import { DecisionProposeCommand } from '../shared/DecisionProposeCommand';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { DecisionProposeParams, DecisionProposeResult } from '../shared/DecisionProposeTypes';

export class DecisionProposeBrowserCommand extends DecisionProposeCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeCommand(params: DecisionProposeParams): Promise<DecisionProposeResult> {
    // Browser just forwards to server (handled by base class execute())
    return transformPayload(params, {
      success: false,
      error: 'decision/propose must be executed on server'
    });
  }
}
