/**
 * Code Task Command - Browser Implementation
 *
 * Execute a coding task end-to-end via the coding agent pipeline. Formulates a plan using LLM reasoning, enforces security tiers, and executes steps via code/* commands. Supports dry-run mode, governance approval for high-risk plans, and multi-agent delegation.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { CodeTaskParams, CodeTaskResult } from '../shared/CodeTaskTypes';

export class CodeTaskBrowserCommand extends CommandBase<CodeTaskParams, CodeTaskResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/task', context, subpath, commander);
  }

  async execute(params: CodeTaskParams): Promise<CodeTaskResult> {
    console.log('🌐 BROWSER: Delegating Code Task to server');
    return await this.remoteExecute(params);
  }
}
