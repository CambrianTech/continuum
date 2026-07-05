/**
 * Collaboration Live Start Command - Browser Implementation
 *
 * Start a live session with selected participants. Creates or finds the DM room for the participant set, then joins the live session. Like Discord's group call - select users, click call.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { CollaborationLiveStartParams, CollaborationLiveStartResult } from '../shared/CollaborationLiveStartTypes';

export class CollaborationLiveStartBrowserCommand extends CommandBase<CollaborationLiveStartParams, CollaborationLiveStartResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/live/start', context, subpath, commander);
  }

  async execute(params: CollaborationLiveStartParams): Promise<CollaborationLiveStartResult> {
    console.log('🌐 BROWSER: Delegating Collaboration Live Start to server');
    return await this.remoteExecute(params);
  }
}
