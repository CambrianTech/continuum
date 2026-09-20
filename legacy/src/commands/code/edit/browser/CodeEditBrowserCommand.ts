/**
 * Code Edit Command - Browser Implementation
 *
 * Edit a file using search-replace, line-range replacement, insert-at, or append. Creates a ChangeNode for undo. Safer than full file write for targeted modifications.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { CodeEditParams, CodeEditResult } from '../shared/CodeEditTypes';

export class CodeEditBrowserCommand extends CommandBase<CodeEditParams, CodeEditResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/edit', context, subpath, commander);
  }

  async execute(params: CodeEditParams): Promise<CodeEditResult> {
    console.log('🌐 BROWSER: Delegating Code Edit to server');
    return await this.remoteExecute(params);
  }
}
