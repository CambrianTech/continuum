/**
 * Code History Command - Server Implementation
 *
 * Get change history for a specific file or the entire workspace.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { CodeHistoryParams, CodeHistoryResult } from '../shared/CodeHistoryTypes';
import { createCodeHistoryResultFromParams } from '../shared/CodeHistoryTypes';
import { CodeDaemon } from '@daemons/code-daemon/shared/CodeDaemon';

export class CodeHistoryServerCommand extends CommandBase<CodeHistoryParams, CodeHistoryResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/history', context, subpath, commander);
  }

  async execute(params: CodeHistoryParams): Promise<CodeHistoryResult> {
    if (!params.userId) {
      throw new ValidationError('userId', 'Workspace operations require a userId (auto-injected for persona tool calls).');
    }
    const personaId = params.userId;

    const result = await CodeDaemon.workspaceHistory(
      personaId,
      params.filePath,
      params.limit
    );

    return createCodeHistoryResultFromParams(params, {
      success: result.success,
      nodes: result.nodes,
      totalCount: result.total_count,
    });
  }
}
