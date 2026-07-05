/**
 * Activity User Presence - Browser Implementation
 *
 * Browser-side command that delegates to server for temperature tracking.
 * Called by MainWidget when tab visibility changes.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';
import type { ActivityUserPresentParams, ActivityUserPresentResult } from '../shared/ActivityUserPresentTypes';

export class ActivityUserPresentCommand extends CommandBase<ActivityUserPresentParams, ActivityUserPresentResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/activity/user-present', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<ActivityUserPresentResult> {
    const userPresentParams = params as ActivityUserPresentParams;

    // Delegate to server to update temperature
    return await this.remoteExecute(userPresentParams);
  }
}
