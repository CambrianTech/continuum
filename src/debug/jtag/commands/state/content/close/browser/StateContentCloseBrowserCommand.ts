/**
 * State Content Close Command - Browser Implementation
 *
 * Browser-side proxies to server for state operations.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { StateContentCloseParams, StateContentCloseResult } from '../shared/StateContentCloseTypes';

export class StateContentCloseBrowserCommand extends CommandBase<StateContentCloseParams, StateContentCloseResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('state/content/close', context, subpath, commander);
  }

  async execute(params: StateContentCloseParams): Promise<StateContentCloseResult> {
    console.log('🌐 BROWSER: state/content/close - proxying to server');
    return this.remoteExecute(params);
  }
}
