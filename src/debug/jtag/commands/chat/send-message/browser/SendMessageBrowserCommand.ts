// ISSUES: 0 open, last updated 2025-07-25 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Send Message Command - Browser Implementation (Simplified)
 * 
 * Simplified version for modular architecture - handles basic message sending.
 * Follows screenshot/navigate pattern - minimal, focused, ~50 lines.
 * Uses shared chat patterns: roomId, senderId, standard result format.
 */

import { SendMessageCommand } from '@chatSendMessage/shared/SendMessageCommand';
import { type SendMessageParams, type SendMessageResult, createSendMessageResult } from '@chatSendMessage/shared/SendMessageTypes';
import type { ICommandDaemon } from '@commandBase';
import type { JTAGContext } from '@shared/JTAGTypes';

export class SendMessageBrowserCommand extends SendMessageCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  async execute(params: SendMessageParams): Promise<SendMessageResult> {
    console.log(`💬 BROWSER: Sending message to room ${params.roomId}`);

    try {
      // Basic validation
      if (!params.roomId || !params.content) {
        throw new Error('Missing required parameters: roomId or content');
      }

      // Route to server for actual message sending
      const result = await this.remoteExecute(params);

      // Update local chat widget if successful
      if (result.success) {
        console.log(`✅ BROWSER: Message sent successfully: ${result.messageId}`);
        await this.updateLocalChatWidget(result);
      }

      return result;

    } catch (error: any) {
      console.error(`❌ BROWSER: Send message error:`, error.message);
      
      return createSendMessageResult(params.context, params.sessionId, {
        messageId: '',
        roomId: params.roomId,
        success: false,
        error: error.message
      });
    }
  }

  private async updateLocalChatWidget(result: SendMessageResult): Promise<void> {
    // Notify chat widget of successful message send
    const chatWidget = document.querySelector('chat-widget');
    if (chatWidget) {
      const messageEvent = new CustomEvent('message-sent', {
        detail: { messageId: result.messageId, roomId: result.roomId }
      });
      chatWidget.dispatchEvent(messageEvent);
    }
  }
}