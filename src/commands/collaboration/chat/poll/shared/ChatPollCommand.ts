/**
 * Chat Poll Command - Shared Implementation
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { ChatPollParams, ChatPollResult } from './ChatPollTypes';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';

export abstract class ChatPollCommand extends CommandBase<ChatPollParams, ChatPollResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/chat/poll', context, subpath, commander);
  }

  /**
   * Server handles the actual polling
   * Browser delegates to server
   */
  protected abstract executeChatPoll(params: ChatPollParams): Promise<ChatPollResult>;

  async execute(params: JTAGPayload): Promise<ChatPollResult> {
    return this.executeChatPoll(params as ChatPollParams);
  }
}
