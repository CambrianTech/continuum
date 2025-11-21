/**
 * Chat Poll Command - Browser Implementation
 * Delegates to server
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { ChatPollCommand } from '../shared/ChatPollCommand';
import type { ChatPollParams, ChatPollResult } from '../shared/ChatPollTypes';

export class ChatPollBrowserCommand extends ChatPollCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeChatPoll(params: ChatPollParams): Promise<ChatPollResult> {
    // Delegate to server
    return await this.remoteExecute(params);
  }
}
