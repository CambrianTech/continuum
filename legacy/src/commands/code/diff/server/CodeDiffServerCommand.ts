/**
 * Code Diff Command - Server Implementation
 *
 * Preview an edit as a unified diff without applying it.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { CodeDiffParams, CodeDiffResult } from '../shared/CodeDiffTypes';
import { createCodeDiffResultFromParams } from '../shared/CodeDiffTypes';
import { CodeDaemon } from '@daemons/code-daemon/shared/CodeDaemon';
import type { WorkspaceEditMode } from '@daemons/code-daemon/shared/CodeDaemonTypes';

export class CodeDiffServerCommand extends CommandBase<CodeDiffParams, CodeDiffResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/diff', context, subpath, commander);
  }

  async execute(params: CodeDiffParams): Promise<CodeDiffResult> {
    if (!params.filePath || params.filePath.trim() === '') {
      throw new ValidationError(
        'filePath',
        `Missing required parameter 'filePath'. See the code/diff README for usage.`
      );
    }
    if (!params.editType) {
      throw new ValidationError(
        'editType',
        `Missing required parameter 'editType'. Must be 'search_replace', 'line_range', 'insert_at', or 'append'.`
      );
    }

    if (!params.userId) {
      throw new ValidationError('userId', 'Workspace operations require a userId (auto-injected for persona tool calls).');
    }
    const personaId = params.userId;

    const editMode = this.buildEditMode(params);

    const result = await CodeDaemon.workspaceDiff(
      personaId,
      params.filePath,
      editMode
    );

    return createCodeDiffResultFromParams(params, {
      success: result.success,
      unified: result.unified,
    });
  }

  private buildEditMode(params: CodeDiffParams): WorkspaceEditMode {
    switch (params.editType) {
      case 'search_replace':
        if (!params.search) throw new ValidationError('search', `'search' is required for search_replace mode.`);
        if (params.replace === undefined) throw new ValidationError('replace', `'replace' is required for search_replace mode.`);
        return { type: 'search_replace', search: params.search, replace: params.replace, all: params.replaceAll ?? false };

      case 'line_range':
        if (!params.startLine) throw new ValidationError('startLine', `'startLine' is required for line_range mode.`);
        if (!params.endLine) throw new ValidationError('endLine', `'endLine' is required for line_range mode.`);
        if (params.newContent === undefined) throw new ValidationError('newContent', `'newContent' is required for line_range mode.`);
        return { type: 'line_range', start_line: params.startLine, end_line: params.endLine, new_content: params.newContent };

      case 'insert_at':
        if (!params.line) throw new ValidationError('line', `'line' is required for insert_at mode.`);
        if (params.content === undefined) throw new ValidationError('content', `'content' is required for insert_at mode.`);
        return { type: 'insert_at', line: params.line, content: params.content };

      case 'append':
        if (params.content === undefined) throw new ValidationError('content', `'content' is required for append mode.`);
        return { type: 'append', content: params.content };

      default:
        throw new ValidationError('editType', `Invalid editType '${params.editType}'.`);
    }
  }
}
