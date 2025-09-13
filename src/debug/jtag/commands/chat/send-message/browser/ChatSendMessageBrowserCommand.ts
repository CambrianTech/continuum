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
   * Browser-specific event emission - Skip since server already handles events
   * FIXED: No double remoteExecute call - server handles event emission after storing message
   */
  protected async emitMessageEvent(message: any): Promise<void> {
    console.log(`🌐 BROWSER: Skipping event emission - server will handle events for message ${message.messageId}`);

    // CRITICAL: Don't call remoteExecute here - that would trigger the command twice!
    // The server version of this command will handle event emission after storing the message
    // Browser commands should only handle browser-specific logic, not duplicate server work
  }

  /**
   * Store original params for delegation
   * FIXED: Strict typing like Rust
   */
  private originalParams?: ChatSendMessageParams;

  async execute(params: JTAGPayload): Promise<ChatSendMessageResult> {
    this.originalParams = params as ChatSendMessageParams;

    console.log(`🔥 CLAUDE-BROWSER-COMMAND-${Date.now()}: ChatSendMessageBrowserCommand.execute() called!`);
    console.log(`🌐 BROWSER: Delegating chat/send-message to server for real-time events`);

    // FIXED: Delegate to server-side command to ensure events are emitted
    // This ensures the server-side ChatSendMessageServerCommand.emitMessageEvent() is called
    const result = await this.remoteExecute<ChatSendMessageParams, ChatSendMessageResult>(
      this.originalParams,
      'chat/send-message'
    );

    if (!result.success) {
      throw new Error(`Server delegation failed: ${result.error}`);
    }

    console.log(`✅ BROWSER: Server delegation successful for message ${result.messageId}`);
    return result;
  }
}