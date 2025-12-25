/**
 * State Content Switch Command - Browser Implementation
 *
 * Browser-side proxies to server, then emits content:switched locally
 * for browser widgets to respond to.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { StateContentSwitchParams, StateContentSwitchResult } from '../shared/StateContentSwitchTypes';
import { Events } from '@system/core/shared/Events';

export class StateContentSwitchBrowserCommand extends CommandBase<StateContentSwitchParams, StateContentSwitchResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('state/content/switch', context, subpath, commander);
  }

  async execute(params: StateContentSwitchParams): Promise<StateContentSwitchResult> {
    console.log('🌐 BROWSER: state/content/switch - proxying to server');

    // Delegate to server
    const result = await this.remoteExecute(params) as StateContentSwitchResult;

    // If successful, emit content:switched locally for browser widgets
    // Server includes content item details in response
    if (result.success && result.contentType && result.entityId) {
      console.log('📋 StateContentSwitchBrowserCommand: Emitting content:switched locally');
      Events.emit('content:switched', {
        contentItemId: params.contentItemId,
        userId: params.userId,
        currentItemId: result.currentItemId,
        contentType: result.contentType,
        entityId: result.entityId,
        title: result.title
      });
    }

    return result;
  }
}
