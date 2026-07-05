/**
 * Code Edit Command - Server Implementation
 *
 * Edits a file using search-replace, line-range, insert-at, or append.
 * Creates a ChangeNode for undo support.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { CodeEditParams, CodeEditResult } from '../shared/CodeEditTypes';
import { createCodeEditResultFromParams } from '../shared/CodeEditTypes';
import { CodeDaemon } from '@daemons/code-daemon/shared/CodeDaemon';
import type { WorkspaceEditMode } from '@daemons/code-daemon/shared/CodeDaemonTypes';
import { ToolResult } from '@system/core/shared/ToolResult';
import { v4 as uuid } from 'uuid';

export class CodeEditServerCommand extends CommandBase<CodeEditParams, CodeEditResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/edit', context, subpath, commander);
  }

  async execute(params: CodeEditParams): Promise<CodeEditResult> {
    if (!params.filePath || params.filePath.trim() === '') {
      throw new ValidationError(
        'filePath',
        `Missing required parameter 'filePath'. See the code/edit README for usage.`
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

    const handle = uuid();
    const startTime = Date.now();

    const editMode = this.buildEditMode(params);

    const result = await CodeDaemon.workspaceEdit(
      personaId,
      params.filePath,
      editMode,
      params.description
    );

    // Emit tool result for memory capture
    ToolResult.emit({
      tool: 'code/edit',
      handle,
      userId: personaId,
      success: result.success,
      summary: result.success
        ? `${params.editType} edit: wrote ${result.bytes_written} bytes to ${result.file_path}`
        : `Failed to edit ${params.filePath} (${params.editType})`,
      data: {
        editType: params.editType,
        path: result.file_path,
        bytes: result.bytes_written,
        changeId: result.change_id,
      },
      durationMs: Date.now() - startTime,
    });

    return createCodeEditResultFromParams(params, {
      success: result.success,
      changeId: result.change_id || '',
      filePath: result.file_path,
      bytesWritten: result.bytes_written,
    });
  }

  private buildEditMode(params: CodeEditParams): WorkspaceEditMode {
    switch (params.editType) {
      case 'search_replace':
        if (!params.search) throw new ValidationError('search', `'search' is required for search_replace edit mode.`);
        if (params.replace === undefined) throw new ValidationError('replace', `'replace' is required for search_replace edit mode.`);
        return { type: 'search_replace', search: params.search, replace: params.replace, all: params.replaceAll ?? false };

      case 'line_range':
        if (!params.startLine) throw new ValidationError('startLine', `'startLine' is required for line_range edit mode.`);
        if (!params.endLine) throw new ValidationError('endLine', `'endLine' is required for line_range edit mode.`);
        if (params.newContent === undefined) throw new ValidationError('newContent', `'newContent' is required for line_range edit mode.`);
        return { type: 'line_range', start_line: params.startLine, end_line: params.endLine, new_content: params.newContent };

      case 'insert_at':
        if (!params.line) throw new ValidationError('line', `'line' is required for insert_at edit mode.`);
        if (params.content === undefined) throw new ValidationError('content', `'content' is required for insert_at edit mode.`);
        return { type: 'insert_at', line: params.line, content: params.content };

      case 'append':
        if (params.content === undefined) throw new ValidationError('content', `'content' is required for append edit mode.`);
        return { type: 'append', content: params.content };

      default:
        throw new ValidationError('editType', `Invalid editType '${params.editType}'. Must be 'search_replace', 'line_range', 'insert_at', or 'append'.`);
    }
  }
}
