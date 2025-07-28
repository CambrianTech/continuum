import { SendMessageCommand } from '../shared/SendMessageCommand';
import { SendMessageParams, SendMessageResult } from '../shared/SendMessageTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../shared/JTAGTypes';

export class SendMessageBrowserCommand extends SendMessageCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  async execute(params: SendMessageParams): Promise<SendMessageResult> {
    try {
      // Validate parameters
      if (!params.roomId || !params.content) {
        return new SendMessageResult({
          roomId: params.roomId || '',
          success: false,
          deliveryStatus: 'failed',
          error: 'roomId and content are required',
          contentLength: params.content?.length || 0
        });
      }

      // Process message content for browser display
      const processedContent = this.processContentForBrowser(params.content, params.deliveryOptions);

      // Send through browser-to-server communication
      const messageId = await this.sendThroughWebSocket(params, processedContent);

      // Update local chat UI if applicable
      if (this.isChatWidgetActive()) {
        await this.updateChatWidget(params, messageId, processedContent);
      }

      return new SendMessageResult({
        roomId: params.roomId,
        success: true,
        messageId,
        senderId: params.senderId,
        deliveryStatus: 'delivered',
        contentLength: params.content.length,
        messageType: params.messageType,
        deliveryTime: Date.now() - this.startTime
      });

    } catch (error) {
      return new SendMessageResult({
        roomId: params.roomId,
        success: false,
        deliveryStatus: 'failed',
        error: error instanceof Error ? error.message : 'Unknown error',
        contentLength: params.content?.length || 0
      });
    }
  }

  private processContentForBrowser(content: string, options?: any): string {
    let processed = content;

    // Process markdown if enabled
    if (options?.markdown) {
      processed = this.processMarkdown(processed);
    }

    // Process emojis if enabled
    if (options?.emojis) {
      processed = this.processEmojis(processed);
    }

    // Process mentions if enabled
    if (options?.mentions) {
      processed = this.processMentions(processed);
    }

    return processed;
  }

  private processMarkdown(content: string): string {
    // Simple markdown processing for browser
    return content
      .replace(/\*\*(.*?)\*\*/g, '<strong>$1</strong>')
      .replace(/\*(.*?)\*/g, '<em>$1</em>')
      .replace(/`(.*?)`/g, '<code>$1</code>');
  }

  private processEmojis(content: string): string {
    // Convert emoji shortcodes to unicode
    return content
      .replace(/:smile:/g, '😊')
      .replace(/:thumbsup:/g, '👍')
      .replace(/:heart:/g, '❤️')
      .replace(/:fire:/g, '🔥');
  }

  private processMentions(content: string): string {
    // Process @mentions for browser display
    return content.replace(/@(\w+)/g, '<span class="mention">@$1</span>');
  }

  private async sendThroughWebSocket(params: SendMessageParams, processedContent: string): Promise<string> {
    // Send message through WebSocket connection to server
    const messageId = `msg_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
    
    // Get WebSocket connection (placeholder)
    const websocket = this.getWebSocketConnection();
    
    if (websocket && websocket.readyState === WebSocket.OPEN) {
      const message = {
        type: 'chat-message',
        messageId,
        roomId: params.roomId,
        content: processedContent,
        originalContent: params.content,
        senderId: params.senderId,
        messageType: params.messageType,
        timestamp: Date.now(),
        academyContext: params.academyContext,
        deliveryOptions: params.deliveryOptions
      };

      websocket.send(JSON.stringify(message));
    } else {
      // Fallback to HTTP if WebSocket not available
      await this.sendThroughHTTP(params, processedContent, messageId);
    }

    return messageId;
  }

  private async sendThroughHTTP(params: SendMessageParams, processedContent: string, messageId: string): Promise<void> {
    // Fallback HTTP sending
    const response = await fetch('/api/chat/send-message', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        messageId,
        roomId: params.roomId,
        content: processedContent,
        originalContent: params.content,
        senderId: params.senderId,
        messageType: params.messageType,
        academyContext: params.academyContext,
        deliveryOptions: params.deliveryOptions
      })
    });

    if (!response.ok) {
      throw new Error(`HTTP send failed: ${response.status} ${response.statusText}`);
    }
  }

  private getWebSocketConnection(): WebSocket | null {
    // Get existing WebSocket connection
    // This would integrate with the browser WebSocket system
    if (typeof window !== 'undefined') {
      return (window as any).continuumWebSocket || null;
    }
    return null;
  }

  private isChatWidgetActive(): boolean {
    // Check if chat widget is active in browser
    if (typeof document !== 'undefined') {
      const chatWidget = document.querySelector('chat-widget');
      return chatWidget !== null;
    }
    return false;
  }

  private async updateChatWidget(params: SendMessageParams, messageId: string, processedContent: string): Promise<void> {
    // Update chat widget with new message
    if (typeof document !== 'undefined') {
      const chatWidget = document.querySelector('chat-widget') as any;
      if (chatWidget && chatWidget.addMessage) {
        chatWidget.addMessage({
          id: messageId,
          roomId: params.roomId,
          content: processedContent,
          senderId: params.senderId,
          messageType: params.messageType,
          timestamp: Date.now(),
          status: 'sending'
        });
      }
    }
  }

  private startTime = Date.now();
}