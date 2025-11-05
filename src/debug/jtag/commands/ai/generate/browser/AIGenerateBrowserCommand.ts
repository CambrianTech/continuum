/**
 * AI Generate Command - Browser Implementation
 * ============================================
 *
 * Browser delegates to server for AI generation
 * All database access and LLM calls happen server-side
 */

import { AIGenerateCommand } from '../shared/AIGenerateCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { AIGenerateParams, AIGenerateResult } from '../shared/AIGenerateTypes';

export class AIGenerateBrowserCommand extends AIGenerateCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  async execute(params: AIGenerateParams): Promise<AIGenerateResult> {
    // Delegate to server - it has database access and LLM
    return this.remoteExecute<AIGenerateParams, AIGenerateResult>(params, 'ai/generate', 'server');
  }
}
