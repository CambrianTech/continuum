/**
 * Code Undo Command - Browser Implementation
 *
 * Undo a specific change or the last N changes. Applies reverse diffs from the change graph to restore previous file state.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { CodeUndoParams, CodeUndoResult } from '../shared/CodeUndoTypes';

export class CodeUndoBrowserCommand extends CommandBase<CodeUndoParams, CodeUndoResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/undo', context, subpath, commander);
  }

  async execute(params: CodeUndoParams): Promise<CodeUndoResult> {
    console.log('🌐 BROWSER: Delegating Code Undo to server');
    return await this.remoteExecute(params);
  }
}
