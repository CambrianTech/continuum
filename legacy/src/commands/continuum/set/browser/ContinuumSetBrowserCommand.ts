/**
 * Continuum Set Command - Browser Implementation
 *
 * Browser stub - base class delegates to server, then we emit event locally
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { ContinuumSetCommand } from '../shared/ContinuumSetCommand';
import type { ContinuumSetParams, ContinuumSetResult } from '../shared/ContinuumSetTypes';
import { Events } from '../../../../system/core/shared/Events';

export class ContinuumSetBrowserCommand extends ContinuumSetCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeContinuumSet(params: ContinuumSetParams): Promise<ContinuumSetResult> {
    // Browser stub - delegate to server using remoteExecute()
    // Server will emit event with await, which ensures it crosses WebSocket to browser
    return await this.remoteExecute<ContinuumSetParams, ContinuumSetResult>(params);
  }
}
