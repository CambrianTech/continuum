/**
 * Claude Context Command - Browser Implementation
 *
 * Generates a comprehensive context summary for Claude Code session resumption — recent git changes, open issues, team chat, system health, and active work state. This is Claude's bridge from stateless sessions to persistent citizenship.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ClaudeContextParams, ClaudeContextResult } from '../shared/ClaudeContextTypes';

export class ClaudeContextBrowserCommand extends CommandBase<ClaudeContextParams, ClaudeContextResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('claude/context', context, subpath, commander);
  }

  async execute(params: ClaudeContextParams): Promise<ClaudeContextResult> {
    console.log('🌐 BROWSER: Delegating Claude Context to server');
    return await this.remoteExecute(params);
  }
}
