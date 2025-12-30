/**
 * Help Command - Browser Implementation
 *
 * Discover and display help documentation from command READMEs, auto-generating templates for gaps
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { HelpParams, HelpResult } from '../shared/HelpTypes';

export class HelpBrowserCommand extends CommandBase<HelpParams, HelpResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Help', context, subpath, commander);
  }

  async execute(params: HelpParams): Promise<HelpResult> {
    console.log('🌐 BROWSER: Delegating Help to server');
    return await this.remoteExecute(params);
  }
}
