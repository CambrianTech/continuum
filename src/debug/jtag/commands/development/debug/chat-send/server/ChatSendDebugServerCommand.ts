/**
 * Chat Send Debug Command - Server
 *
 * Delegates to browser to trigger chat widget
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ChatSendDebugParams, ChatSendDebugResult } from '../shared/ChatSendDebugTypes';

export class ChatSendDebugServerCommand extends CommandBase<ChatSendDebugParams, ChatSendDebugResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('development/debug/chat-send', context, subpath, commander);
  }

  async execute(params: ChatSendDebugParams): Promise<ChatSendDebugResult> {
    // debug/chat-send is deprecated - users should use chat/send instead
    throw new Error(
      'The debug/chat-send command is deprecated. Please use chat/send instead:\n\n' +
      `  ./jtag chat/send --room="${params.roomId ?? 'general'}" --message="${params.message ?? 'your message'}"\n\n` +
      'The chat/send command provides the same functionality with proper error handling and validation.'
    );
  }
}