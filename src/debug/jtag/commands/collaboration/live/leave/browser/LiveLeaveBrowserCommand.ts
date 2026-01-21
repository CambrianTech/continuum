/**
 * Live Leave Command - Browser Implementation
 *
 * Delegates to server for session management.
 */

import { LiveLeaveCommand } from '../shared/LiveLeaveCommand';
import type { LiveLeaveParams, LiveLeaveResult } from '../shared/LiveLeaveTypes';

export class LiveLeaveBrowserCommand extends LiveLeaveCommand {

  protected async executeLeave(_params: LiveLeaveParams): Promise<LiveLeaveResult> {
    // Browser delegates to server - command routing handles this automatically
    throw new Error('live/leave command must run on server');
  }
}
