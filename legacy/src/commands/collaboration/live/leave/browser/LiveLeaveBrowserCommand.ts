/**
 * Live Leave Command - Browser Implementation
 *
 * Delegates to server for session management.
 */

import { LiveLeaveCommand } from '../shared/LiveLeaveCommand';
import type { LiveLeaveParams, LiveLeaveResult } from '../shared/LiveLeaveTypes';

export class LiveLeaveBrowserCommand extends LiveLeaveCommand {

  protected async executeLeave(params: LiveLeaveParams): Promise<LiveLeaveResult> {
    // Delegate to server
    return await this.remoteExecute(params);
  }
}
