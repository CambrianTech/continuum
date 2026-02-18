/**
 * Chat Analyze Command - Browser Implementation
 * Delegates to server
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { ChatAnalyzeCommand } from '../shared/ChatAnalyzeCommand';
import type { ChatAnalyzeParams, ChatAnalyzeResult } from '../shared/ChatAnalyzeTypes';

export class ChatAnalyzeBrowserCommand extends ChatAnalyzeCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeChatAnalyze(params: ChatAnalyzeParams): Promise<ChatAnalyzeResult> {
    // Delegate to server
    return await this.remoteExecute(params);
  }
}
