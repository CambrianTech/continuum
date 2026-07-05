/**
 * Live Leave Command - Base class for leaving live sessions
 *
 * Removes user from session participants.
 * Session ends automatically when last participant leaves.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { LiveLeaveParams, LiveLeaveResult } from './LiveLeaveTypes';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';

export abstract class LiveLeaveCommand extends CommandBase<LiveLeaveParams, LiveLeaveResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/live/leave', context, subpath, commander);
  }

  /**
   * Execute the leave command
   */
  async execute(params: JTAGPayload): Promise<LiveLeaveResult> {
    return this.executeLeave(params as LiveLeaveParams);
  }

  /**
   * Subclass must implement the actual leave logic
   */
  protected abstract executeLeave(params: LiveLeaveParams): Promise<LiveLeaveResult>;
}
