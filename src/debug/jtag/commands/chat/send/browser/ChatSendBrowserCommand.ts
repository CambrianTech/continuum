/**
 * Chat Send Command - Browser Implementation
 * Delegates to server
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { ChatSendCommand } from '../shared/ChatSendCommand';
import type { ChatSendParams, ChatSendResult } from '../shared/ChatSendTypes';

export class ChatSendBrowserCommand extends ChatSendCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeChatSend(params: ChatSendParams): Promise<ChatSendResult> {
    // Delegate to server
    return await this.remoteExecute(params);
  }
}
