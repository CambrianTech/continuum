/**
 * Chat Send Debug Command - Browser
 *
 * Triggers chat widget to send a message programmatically
 * Used for testing event flow through the chat system
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ChatSendDebugParams, ChatSendDebugResult } from '../shared/ChatSendDebugTypes';
import { createChatSendResult } from '../shared/ChatSendDebugTypes';

export class ChatSendDebugBrowserCommand extends CommandBase<ChatSendDebugParams, ChatSendDebugResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('development/debug/chat-send', context, subpath, commander);
  }

  async execute(params: ChatSendDebugParams): Promise<ChatSendDebugResult> {
    try {
      // Find chat widget
      const continuumWidget = document.querySelector('continuum-widget');
      const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
      const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget') as any;

      if (!chatWidget) {
        return createChatSendResult(this.context, params.sessionId, {
          success: false,
          sent: false,
          error: 'Chat widget not found'
        });
      }

      // Find message input
      const messageInput = chatWidget.shadowRoot?.querySelector('.message-input') as HTMLInputElement;
      if (!messageInput) {
        return createChatSendResult(this.context, params.sessionId, {
          success: false,
          sent: false,
          error: 'Message input not found'
        });
      }

      // Set message text
      messageInput.value = params.message;

      // Trigger Enter key event to send message
      const enterEvent = new KeyboardEvent('keydown', {
        key: 'Enter',
        code: 'Enter',
        keyCode: 13,
        bubbles: true,
        cancelable: true
      });

      messageInput.dispatchEvent(enterEvent);

      console.log(`✅ Chat widget triggered to send: "${params.message}"`);

      // Wait a bit for message to be sent
      await new Promise(resolve => setTimeout(resolve, 500));

      return {
        context: params.context,
        sessionId: params.sessionId,
        success: true,
        sent: true
      };

    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      console.error('❌ Chat send debug failed:', error);

      return {
        context: params.context,
        sessionId: params.sessionId,
        success: false,
        sent: false,
        error: errorMsg
      };
    }
  }
}