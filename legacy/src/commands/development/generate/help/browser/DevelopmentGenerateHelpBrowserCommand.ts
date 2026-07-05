/**
 * Development Generate Help Command - Browser Implementation
 *
 * Display comprehensive generator documentation including spec reference, example specs, type reference, access levels, workflow guide, and audit information. This is the primary documentation entry point for AI agents learning to use the generator.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { DevelopmentGenerateHelpParams, DevelopmentGenerateHelpResult } from '../shared/DevelopmentGenerateHelpTypes';

export class DevelopmentGenerateHelpBrowserCommand extends CommandBase<DevelopmentGenerateHelpParams, DevelopmentGenerateHelpResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('development/generate/help', context, subpath, commander);
  }

  async execute(params: DevelopmentGenerateHelpParams): Promise<DevelopmentGenerateHelpResult> {
    console.log('🌐 BROWSER: Delegating Development Generate Help to server');
    return await this.remoteExecute(params);
  }
}
