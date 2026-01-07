/**
 * State Content Switch Command - Browser Implementation
 *
 * Browser-side proxies to server. Server emits content:switched
 * which routes via WebSocket to browser widgets.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { StateContentSwitchParams, StateContentSwitchResult } from '../shared/StateContentSwitchTypes';

export class StateContentSwitchBrowserCommand extends CommandBase<StateContentSwitchParams, StateContentSwitchResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('state/content/switch', context, subpath, commander);
  }

  async execute(params: StateContentSwitchParams): Promise<StateContentSwitchResult> {
    console.log('🌐 BROWSER: state/content/switch - proxying to server');

    // Delegate to server - server emits content:switched which routes via WebSocket
    // NOTE: Removed duplicate local emit - server's event routes through Events system
    return await this.remoteExecute(params) as StateContentSwitchResult;
  }
}
