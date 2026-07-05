/**
 * Code Search Command - Server Implementation
 *
 * Search for a regex pattern across workspace files.
 * Respects .gitignore, supports glob-based file filtering.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { CodeSearchParams, CodeSearchResult } from '../shared/CodeSearchTypes';
import { createCodeSearchResultFromParams } from '../shared/CodeSearchTypes';
import { CodeDaemon } from '@daemons/code-daemon/shared/CodeDaemon';

export class CodeSearchServerCommand extends CommandBase<CodeSearchParams, CodeSearchResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/search', context, subpath, commander);
  }

  async execute(params: CodeSearchParams): Promise<CodeSearchResult> {
    if (!params.pattern || params.pattern.trim() === '') {
      throw new ValidationError(
        'pattern',
        `Missing required parameter 'pattern'. See the code/search README for usage.`
      );
    }

    if (!params.userId) {
      throw new ValidationError('userId', 'Workspace operations require a userId (auto-injected for persona tool calls).');
    }
    const personaId = params.userId;

    const result = await CodeDaemon.workspaceSearch(
      personaId,
      params.pattern,
      params.fileGlob,
      params.maxResults
    );

    return createCodeSearchResultFromParams(params, {
      success: result.success,
      matches: result.matches,
      totalMatches: result.total_matches,
      filesSearched: result.files_searched,
    });
  }
}
