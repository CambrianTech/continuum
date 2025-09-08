/**
 * Chat Send Message Browser Command
 */

import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { ChatSendMessageCommand } from '../shared/ChatSendMessageCommand';
import type { ChatSendMessageParams, ChatSendMessageResult } from '../shared/ChatSendMessageTypes';

export class ChatSendMessageBrowserCommand extends ChatSendMessageCommand {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected getEnvironmentLabel(): string {
    return 'BROWSER';
  }

  /**
   * Browser-specific event emission - delegate to server for proper event broadcasting
   */
  protected async emitMessageEvent(message: any): Promise<void> {
    console.log(`🌐 BROWSER: Delegating event emission to server for message ${message.messageId}`);
    
    try {
      // Create params for server-side event emission only
      const serverParams = {
        ...this.originalParams, // Preserve original params
        messageId: message.messageId,
        message: message,
        eventOnly: true, // Flag to indicate this is just for event emission
        context: this.context,
        sessionId: message.senderId
      };

      // Delegate to server for event emission
      const result = await this.remoteExecute(serverParams);
      console.log(`📨 BROWSER: Server event emission ${(result as any).success ? 'succeeded' : 'failed'}`);
      
    } catch (error) {
      console.error(`❌ BROWSER: Failed to delegate event emission:`, error);
      // Don't fail the entire operation if event delegation fails
    }
  }

  /**
   * Store original params for delegation
   * FIXED: Strict typing like Rust
   */
  private originalParams?: ChatSendMessageParams;

  async execute(params: JTAGPayload): Promise<ChatSendMessageResult> {
    this.originalParams = params as ChatSendMessageParams;
    return await super.execute(params);
  }
}