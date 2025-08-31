/**
 * Chat Widget - JTAG-based Web Component using BaseWidget Architecture
 * 
 * Updated to use the new BaseWidget abstraction with one-line operations.
 * Demonstrates dramatic code simplification through proper abstraction.
 */

import { BaseWidget } from '../shared/BaseWidget';
import type { ChatMessage } from '../chat-widget/shared/ChatTypes';

export class ChatWidget extends BaseWidget {
  private messages: ChatMessage[] = [];
  private currentRoom = 'general';
  private messageInput?: HTMLInputElement;
  
  constructor() {
    super({
      widgetName: 'ChatWidget',
      enableAI: true,
      enableDatabase: true,
      enableRouterEvents: true,
      enableScreenshots: true,
      theme: 'cyberpunk'
    });
  }

  protected async onWidgetInitialize(): Promise<void> {
    console.log('🎯 ChatWidget: Initializing with BaseWidget architecture...');
    
    // Load chat history using BaseWidget's data methods
    const savedMessages = await this.getData('chat_messages', []);
    this.messages = savedMessages;
    
    console.log('✅ ChatWidget: BaseWidget features initialized');
  }

  protected async renderWidget(): Promise<void> {
    // Simple, clean UI using BaseWidget pattern
    this.shadowRoot.innerHTML = `
      <style>
        :host {
          display: flex;
          flex-direction: column;
          height: 100%;
          background: #1a1f2e;
          color: #e0e6ed;
          font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
        }
        
        .chat-header {
          padding: 12px 16px;
          background: #2d3748;
          border-bottom: 1px solid #4a5568;
          font-weight: 600;
        }
        
        .messages-container {
          flex: 1;
          overflow-y: auto;
          padding: 16px;
          display: flex;
          flex-direction: column;
          gap: 12px;
        }
        
        .message {
          padding: 8px 12px;
          border-radius: 8px;
          max-width: 80%;
        }
        
        .message.user {
          align-self: flex-end;
          background: #3182ce;
        }
        
        .message.assistant {
          align-self: flex-start;
          background: #4a5568;
        }
        
        .input-container {
          padding: 16px;
          border-top: 1px solid #4a5568;
          display: flex;
          gap: 8px;
        }
        
        .message-input {
          flex: 1;
          padding: 8px 12px;
          background: #4a5568;
          border: 1px solid #718096;
          border-radius: 6px;
          color: #e0e6ed;
          font-size: 14px;
        }
        
        .send-button {
          padding: 8px 16px;
          background: #00d4ff;
          border: none;
          border-radius: 6px;
          color: #000;
          cursor: pointer;
          font-weight: 600;
        }
        
        .send-button:hover {
          background: #00b8e6;
        }
      </style>
      
      <div class="chat-header">
        💬 Chat Widget (BaseWidget Architecture)
      </div>
      
      <div class="messages-container" id="messages">
        ${this.renderMessages()}
      </div>
      
      <div class="input-container">
        <input type="text" class="message-input" id="messageInput" placeholder="Type a message...">
        <button class="send-button" id="sendButton">Send</button>
      </div>
    `;
    
    // Cache input element
    this.messageInput = this.shadowRoot.getElementById('messageInput') as HTMLInputElement;
    
    // Setup event listeners
    this.setupEventListeners();
  }

  private renderMessages(): string {
    if (this.messages.length === 0) {
      return '<div class="message assistant">Hello! This is a BaseWidget-powered chat. Try sending a message!</div>';
    }
    
    return this.messages.map(msg => `
      <div class="message ${msg.type}">
        ${msg.content}
      </div>
    `).join('');
  }

  private setupEventListeners(): void {
    // Send message on Enter or button click
    this.messageInput?.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') {
        this.sendMessage();
      }
    });
    
    this.shadowRoot.getElementById('sendButton')?.addEventListener('click', () => {
      this.sendMessage();
    });
  }

  private async sendMessage(): Promise<void> {
    if (!this.messageInput) return;
    
    const content = this.messageInput.value.trim();
    if (!content) return;
    
    // Create user message
    const userMessage: ChatMessage = {
      id: `msg_${Date.now()}`,
      content,
      roomId: this.currentRoom,
      userId: 'current_user',
      type: 'user',
      timestamp: new Date().toISOString()
    };
    
    // Add to messages and clear input
    this.messages.push(userMessage);
    this.messageInput.value = '';
    
    try {
      // ONE LINE: Store message with BaseWidget abstraction
      await this.storeData(`message_${userMessage.id}`, userMessage, { 
        persistent: true, 
        broadcast: true 
      });
      
      // ONE LINE: Get AI response with BaseWidget abstraction
      const aiResponse = await this.queryAI(content, {
        persona: 'chat_assistant',
        context: this.getChatContext()
      });
      
      if (aiResponse && aiResponse.reply) {
        const aiMessage: ChatMessage = {
          id: `ai_${Date.now()}`,
          content: aiResponse.reply,
          roomId: this.currentRoom,
          userId: 'ai_assistant',
          type: 'assistant',
          timestamp: new Date().toISOString()
        };
        
        // Add AI response
        this.messages.push(aiMessage);
        
        // ONE LINE: Store AI response
        await this.storeData(`message_${aiMessage.id}`, aiMessage, { 
          persistent: true, 
          broadcast: true 
        });
      }
      
      // Re-render messages
      await this.renderWidget();
      
    } catch (error) {
      console.error('❌ ChatWidget: Failed to send message:', error);
      this.handleError(error, 'sendMessage');
    }
  }

  private getChatContext(): any {
    return {
      recentMessages: this.messages.slice(-5),
      roomId: this.currentRoom,
      messageCount: this.messages.length
    };
  }

  protected async onWidgetCleanup(): Promise<void> {
    // Save messages using BaseWidget abstraction
    await this.storeData('chat_messages', this.messages, { persistent: true });
    console.log('✅ ChatWidget: BaseWidget cleanup complete');
  }

  // Static property required by widget registration system
  static get widgetName(): string {
    return 'chat';
  }
}