/**
 * Generate Command - Browser Implementation
 *
 * Generate a new command from a CommandSpec JSON definition
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { GenerateParams, GenerateResult } from '../shared/GenerateTypes';

export class GenerateBrowserCommand extends CommandBase<GenerateParams, GenerateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Generate', context, subpath, commander);
  }

  async execute(params: GenerateParams): Promise<GenerateResult> {
    console.log('🌐 BROWSER: Delegating Generate to server');
    return await this.remoteExecute(params);
  }
}
