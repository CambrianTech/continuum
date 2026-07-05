/**
 * Interface Launch Url Command - Browser Implementation
 *
 * Opens a URL in the default browser. Enables personas to view what they build.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { InterfaceLaunchUrlParams, InterfaceLaunchUrlResult } from '../shared/InterfaceLaunchUrlTypes';

export class InterfaceLaunchUrlBrowserCommand extends CommandBase<InterfaceLaunchUrlParams, InterfaceLaunchUrlResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('interface/launch/url', context, subpath, commander);
  }

  async execute(params: InterfaceLaunchUrlParams): Promise<InterfaceLaunchUrlResult> {
    console.log('🌐 BROWSER: Delegating Interface Launch Url to server');
    return await this.remoteExecute(params);
  }
}
