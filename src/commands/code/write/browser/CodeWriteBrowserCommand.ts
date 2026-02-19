/**
 * Code Write Command - Browser Implementation
 *
 * Write or create a file in the persona's workspace. Creates a ChangeNode in the change graph for undo support. File extension must be in the allowlist.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { CodeWriteParams, CodeWriteResult } from '../shared/CodeWriteTypes';

export class CodeWriteBrowserCommand extends CommandBase<CodeWriteParams, CodeWriteResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/write', context, subpath, commander);
  }

  async execute(params: CodeWriteParams): Promise<CodeWriteResult> {
    console.log('🌐 BROWSER: Delegating Code Write to server');
    return await this.remoteExecute(params);
  }
}
