/**
 * Code Read Command - Browser Implementation
 *
 * Read a file or line range from the persona's workspace. Returns content with line numbers and metadata. Supports partial reads via start/end line parameters.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { CodeReadParams, CodeReadResult } from '../shared/CodeReadTypes';

export class CodeReadBrowserCommand extends CommandBase<CodeReadParams, CodeReadResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/read', context, subpath, commander);
  }

  async execute(params: CodeReadParams): Promise<CodeReadResult> {
    console.log('🌐 BROWSER: Delegating Code Read to server');
    return await this.remoteExecute(params);
  }
}
