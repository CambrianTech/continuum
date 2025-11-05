/**
 * Chat Send Debug Command - Server
 *
 * Delegates to browser to trigger chat widget
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ChatSendDebugParams, ChatSendDebugResult } from '../shared/ChatSendDebugTypes';

export class ChatSendDebugServerCommand extends CommandBase<ChatSendDebugParams, ChatSendDebugResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('debug/chat-send', context, subpath, commander);
  }

  async execute(params: ChatSendDebugParams): Promise<ChatSendDebugResult> {
    // Server always delegates to browser for widget interaction
    return await this.remoteExecute(params);
  }
}