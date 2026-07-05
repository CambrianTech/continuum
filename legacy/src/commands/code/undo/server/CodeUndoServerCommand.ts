/**
 * Code Undo Command - Server Implementation
 *
 * Undo a specific change or the last N changes.
 * Applies reverse diffs from the change graph to restore previous state.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { CodeUndoParams, CodeUndoResult } from '../shared/CodeUndoTypes';
import { createCodeUndoResultFromParams } from '../shared/CodeUndoTypes';
import { CodeDaemon } from '@daemons/code-daemon/shared/CodeDaemon';

export class CodeUndoServerCommand extends CommandBase<CodeUndoParams, CodeUndoResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/undo', context, subpath, commander);
  }

  async execute(params: CodeUndoParams): Promise<CodeUndoResult> {
    if (!params.userId) {
      throw new ValidationError('userId', 'Workspace operations require a userId (auto-injected for persona tool calls).');
    }
    const personaId = params.userId;

    const result = await CodeDaemon.workspaceUndo(
      personaId,
      params.changeId,
      params.count
    );

    return createCodeUndoResultFromParams(params, {
      success: result.success,
      changesUndone: result.changes_undone.map(c => ({
        success: c.success,
        change_id: c.change_id,
        file_path: c.file_path,
        bytes_written: c.bytes_written,
      })),
    });
  }
}
