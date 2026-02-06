/**
 * Code Tree Command - Server Implementation
 *
 * Generate a directory tree for the workspace or a subdirectory.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { CodeTreeParams, CodeTreeResult } from '../shared/CodeTreeTypes';
import { createCodeTreeResultFromParams } from '../shared/CodeTreeTypes';
import { CodeDaemon } from '@daemons/code-daemon/shared/CodeDaemon';

export class CodeTreeServerCommand extends CommandBase<CodeTreeParams, CodeTreeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/tree', context, subpath, commander);
  }

  async execute(params: CodeTreeParams): Promise<CodeTreeResult> {
    if (!params.userId) {
      throw new ValidationError('userId', 'Workspace operations require a userId (auto-injected for persona tool calls).');
    }
    const personaId = params.userId;

    const result = await CodeDaemon.workspaceTree(
      personaId,
      params.path,
      params.maxDepth,
      params.includeHidden
    );

    return createCodeTreeResultFromParams(params, {
      success: result.success,
      root: result.root ?? null,
      totalFiles: result.total_files,
      totalDirectories: result.total_directories,
    });
  }
}
