/**
 * Code Diff Command - Browser Implementation
 *
 * Preview an edit as a unified diff without applying it. Useful for reviewing changes before committing them. Uses the same edit modes as code/edit.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { CodeDiffParams, CodeDiffResult } from '../shared/CodeDiffTypes';

export class CodeDiffBrowserCommand extends CommandBase<CodeDiffParams, CodeDiffResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/diff', context, subpath, commander);
  }

  async execute(params: CodeDiffParams): Promise<CodeDiffResult> {
    console.log('🌐 BROWSER: Delegating Code Diff to server');
    return await this.remoteExecute(params);
  }
}
