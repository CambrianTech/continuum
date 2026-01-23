/**
 * Live Join Command - Browser Implementation
 *
 * Delegates to server for session management.
 */

import { LiveJoinCommand } from '../shared/LiveJoinCommand';
import type { LiveJoinParams, LiveJoinResult } from '../shared/LiveJoinTypes';

export class LiveJoinBrowserCommand extends LiveJoinCommand {

  protected async executeJoin(params: LiveJoinParams): Promise<LiveJoinResult> {
    // Delegate to server
    return await this.remoteExecute(params);
  }
}
