/**
 * State Content Switch Command - Browser Implementation
 *
 * Browser-side proxies to server for state operations.
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
    return this.remoteExecute(params);
  }
}
