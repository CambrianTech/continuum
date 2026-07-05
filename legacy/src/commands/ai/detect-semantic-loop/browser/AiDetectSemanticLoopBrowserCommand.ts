/**
 * Ai Detect Semantic Loop Command - Browser Implementation
 *
 * Detects if an AI's response is semantically too similar to recent messages, preventing repetitive loop behavior
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { AiDetectSemanticLoopParams, AiDetectSemanticLoopResult } from '../shared/AiDetectSemanticLoopTypes';

export class AiDetectSemanticLoopBrowserCommand extends CommandBase<AiDetectSemanticLoopParams, AiDetectSemanticLoopResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Ai Detect Semantic Loop', context, subpath, commander);
  }

  async execute(params: AiDetectSemanticLoopParams): Promise<AiDetectSemanticLoopResult> {
    console.log('🌐 BROWSER: Delegating Ai Detect Semantic Loop to server');
    return await this.remoteExecute(params);
  }
}
