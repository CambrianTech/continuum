/**
 * Live Join Command - Browser Implementation
 *
 * Delegates to server for session management.
 */

import { LiveJoinCommand } from '../shared/LiveJoinCommand';
import type { LiveJoinParams, LiveJoinResult } from '../shared/LiveJoinTypes';

export class LiveJoinBrowserCommand extends LiveJoinCommand {

  protected async executeJoin(_params: LiveJoinParams): Promise<LiveJoinResult> {
    // Browser delegates to server - command routing handles this automatically
    throw new Error('live/join command must run on server');
  }
}
