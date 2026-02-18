/**
 * Code Write Command - Server Implementation
 *
 * Writes or creates a file in the persona's workspace via Rust IPC.
 * Creates a ChangeNode in the change graph for undo support.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { CodeWriteParams, CodeWriteResult } from '../shared/CodeWriteTypes';
import { createCodeWriteResultFromParams } from '../shared/CodeWriteTypes';
import { CodeDaemon } from '@daemons/code-daemon/shared/CodeDaemon';
import { ToolResult } from '@system/core/shared/ToolResult';
import { v4 as uuid } from 'uuid';

export class CodeWriteServerCommand extends CommandBase<CodeWriteParams, CodeWriteResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/write', context, subpath, commander);
  }

  async execute(params: CodeWriteParams): Promise<CodeWriteResult> {
    if (!params.filePath || params.filePath.trim() === '') {
      throw new ValidationError(
        'filePath',
        `Missing required parameter 'filePath'. See the code/write README for usage.`
      );
    }
    if (params.content === undefined || params.content === null) {
      throw new ValidationError(
        'content',
        `Missing required parameter 'content'. See the code/write README for usage.`
      );
    }

    if (!params.userId) {
      throw new ValidationError('userId', 'Workspace operations require a userId (auto-injected for persona tool calls).');
    }
    const personaId = params.userId;

    const handle = uuid();
    const startTime = Date.now();

    const result = await CodeDaemon.workspaceWrite(
      personaId,
      params.filePath,
      params.content,
      params.description
    );

    // Emit tool result for memory capture
    ToolResult.emit({
      tool: 'code/write',
      handle,
      userId: personaId,
      success: result.success,
      summary: result.success
        ? `Wrote ${result.bytes_written} bytes to ${result.file_path}`
        : `Failed to write ${params.filePath}`,
      data: {
        path: result.file_path,
        bytes: result.bytes_written,
        changeId: result.change_id,
      },
      durationMs: Date.now() - startTime,
    });

    return createCodeWriteResultFromParams(params, {
      success: result.success,
      changeId: result.change_id || '',
      filePath: result.file_path,
      bytesWritten: result.bytes_written,
    });
  }
}
