/**
 * Code Read Command - Server Implementation
 *
 * Reads a file or line range from the persona's workspace via Rust IPC.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { CodeReadParams, CodeReadResult } from '../shared/CodeReadTypes';
import { createCodeReadResultFromParams } from '../shared/CodeReadTypes';
import { CodeDaemon } from '@daemons/code-daemon/shared/CodeDaemon';

export class CodeReadServerCommand extends CommandBase<CodeReadParams, CodeReadResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/read', context, subpath, commander);
  }

  async execute(params: CodeReadParams): Promise<CodeReadResult> {
    if (!params.filePath || params.filePath.trim() === '') {
      throw new ValidationError(
        'filePath',
        `Missing required parameter 'filePath'. See the code/read README for usage.`
      );
    }

    if (!params.userId) {
      throw new ValidationError('userId', 'Workspace operations require a userId (auto-injected for persona tool calls).');
    }
    const personaId = params.userId;

    const result = await CodeDaemon.workspaceRead(
      personaId,
      params.filePath,
      params.startLine,
      params.endLine
    );

    return createCodeReadResultFromParams(params, {
      success: result.success,
      content: result.content || '',
      filePath: result.file_path,
      totalLines: result.total_lines,
      linesReturned: result.lines_returned,
      startLine: result.start_line,
      endLine: result.end_line,
      sizeBytes: result.size_bytes,
    });
  }
}
