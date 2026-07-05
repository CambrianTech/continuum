/**
 * Claude Resume Command - Browser Implementation
 *
 * Loads the latest snapshot and current context, synthesizes a session briefing. Run this first thing in a new session — it tells you who you are, what you were doing, and what to do next.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ClaudeResumeParams, ClaudeResumeResult } from '../shared/ClaudeResumeTypes';

export class ClaudeResumeBrowserCommand extends CommandBase<ClaudeResumeParams, ClaudeResumeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('claude/resume', context, subpath, commander);
  }

  async execute(params: ClaudeResumeParams): Promise<ClaudeResumeResult> {
    console.log('🌐 BROWSER: Delegating Claude Resume to server');
    return await this.remoteExecute(params);
  }
}
