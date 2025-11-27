/**
 * Continuum Set Command - Browser Implementation
 *
 * Browser stub - all logic happens on server via event emission
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { ContinuumSetCommand } from '../shared/ContinuumSetCommand';
import type { ContinuumSetParams, ContinuumSetResult } from '../shared/ContinuumSetTypes';

export class ContinuumSetBrowserCommand extends ContinuumSetCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeContinuumSet(params: ContinuumSetParams): Promise<ContinuumSetResult> {
    // Browser stub - delegates to server which emits event
    throw new Error('ContinuumSetBrowserCommand should delegate to server');
  }
}
