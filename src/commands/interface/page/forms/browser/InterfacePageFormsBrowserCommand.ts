/**
 * Interface Page Forms Command - Browser Implementation
 *
 * Discover all forms on a web page. Returns structured form definitions with field names, types, labels, and submit buttons. Works on ANY page with HTML forms - no WebMCP required. Use this first to understand what you can interact with, then use interface/page/fill and interface/page/submit.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { InterfacePageFormsParams, InterfacePageFormsResult } from '../shared/InterfacePageFormsTypes';

export class InterfacePageFormsBrowserCommand extends CommandBase<InterfacePageFormsParams, InterfacePageFormsResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('interface/page/forms', context, subpath, commander);
  }

  async execute(params: InterfacePageFormsParams): Promise<InterfacePageFormsResult> {
    console.log('🌐 BROWSER: Delegating Interface Page Forms to server');
    return await this.remoteExecute(params);
  }
}
