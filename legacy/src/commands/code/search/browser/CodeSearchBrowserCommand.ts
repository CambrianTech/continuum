/**
 * Code Search Command - Browser Implementation
 *
 * Search for a regex pattern across workspace files. Respects .gitignore, supports glob-based file filtering. Returns matching lines with context.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { CodeSearchParams, CodeSearchResult } from '../shared/CodeSearchTypes';

export class CodeSearchBrowserCommand extends CommandBase<CodeSearchParams, CodeSearchResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/search', context, subpath, commander);
  }

  async execute(params: CodeSearchParams): Promise<CodeSearchResult> {
    console.log('🌐 BROWSER: Delegating Code Search to server');
    return await this.remoteExecute(params);
  }
}
