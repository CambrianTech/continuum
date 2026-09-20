/**
 * Interface Page Submit Command - Browser Implementation
 *
 * Submit a form on a web page. Use interface/page/forms to discover forms, interface/page/fill to populate fields, then this command to submit. Returns the resulting page state after submission.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { InterfacePageSubmitParams, InterfacePageSubmitResult } from '../shared/InterfacePageSubmitTypes';

export class InterfacePageSubmitBrowserCommand extends CommandBase<InterfacePageSubmitParams, InterfacePageSubmitResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('interface/page/submit', context, subpath, commander);
  }

  async execute(params: InterfacePageSubmitParams): Promise<InterfacePageSubmitResult> {
    console.log('🌐 BROWSER: Delegating Interface Page Submit to server');
    return await this.remoteExecute(params);
  }
}
