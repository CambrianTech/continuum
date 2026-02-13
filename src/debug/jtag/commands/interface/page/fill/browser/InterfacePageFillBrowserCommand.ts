/**
 * Interface Page Fill Command - Browser Implementation
 *
 * Fill form fields on a web page. Use interface/page/forms first to discover available forms and their fields. This command fills fields but does NOT submit - use interface/page/submit after filling.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { InterfacePageFillParams, InterfacePageFillResult } from '../shared/InterfacePageFillTypes';

export class InterfacePageFillBrowserCommand extends CommandBase<InterfacePageFillParams, InterfacePageFillResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('interface/page/fill', context, subpath, commander);
  }

  async execute(params: InterfacePageFillParams): Promise<InterfacePageFillResult> {
    console.log('🌐 BROWSER: Delegating Interface Page Fill to server');
    return await this.remoteExecute(params);
  }
}
