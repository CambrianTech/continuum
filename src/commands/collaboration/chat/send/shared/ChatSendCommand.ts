/**
 * Chat Send Command - Shared Implementation
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { ChatSendParams, ChatSendResult } from './ChatSendTypes';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';

export abstract class ChatSendCommand extends CommandBase<ChatSendParams, ChatSendResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/chat/send', context, subpath, commander);
  }

  /**
   * Server handles the actual message creation
   * Browser delegates to server
   */
  protected abstract executeChatSend(params: ChatSendParams): Promise<ChatSendResult>;

  async execute(params: JTAGPayload): Promise<ChatSendResult> {
    return this.executeChatSend(params as ChatSendParams);
  }
}
