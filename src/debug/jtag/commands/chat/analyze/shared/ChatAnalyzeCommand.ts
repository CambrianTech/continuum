/**
 * Chat Analyze Command - Shared Implementation
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { ChatAnalyzeParams, ChatAnalyzeResult } from './ChatAnalyzeTypes';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';

export abstract class ChatAnalyzeCommand extends CommandBase<ChatAnalyzeParams, ChatAnalyzeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('chat/analyze', context, subpath, commander);
  }

  /**
   * Server handles the actual analysis logic
   * Browser delegates to server
   */
  protected abstract executeChatAnalyze(params: ChatAnalyzeParams): Promise<ChatAnalyzeResult>;

  async execute(params: JTAGPayload): Promise<ChatAnalyzeResult> {
    return this.executeChatAnalyze(params as ChatAnalyzeParams);
  }
}
