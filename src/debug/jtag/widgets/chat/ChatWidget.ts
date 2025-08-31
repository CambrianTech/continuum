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
      template: 'chat-widget.html',
      styles: 'chat-widget.css',
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
    // Use external template and styles loaded by BaseWidget
    const styles = this.templateCSS || '/* No styles loaded */';
    const template = this.templateHTML || '<div>No template loaded</div>';
    
    // Ensure template is a string before calling replace
    const templateString = typeof template === 'string' ? template : '<div>Template error</div>';
    
    // Replace dynamic content in template
    const dynamicContent = templateString.replace(
      '<!-- Dynamic messages rendered here -->', 
      this.renderMessages()
    );
    
    this.shadowRoot.innerHTML = `
      <style>${styles}</style>
      ${dynamicContent}
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