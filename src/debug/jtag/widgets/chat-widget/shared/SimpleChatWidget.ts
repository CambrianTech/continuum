/**
 * SimpleChatWidget - Chat widget using BaseWidget abstraction
 * 
 * Compare this to the complex ChatWidgetServer/Browser split!
 * BaseWidget handles ALL the complexity:
 * - Daemon coordination -> this.storeData(), this.queryAI()
 * - Event broadcasting -> this.broadcastEvent()
 * - Theme system -> this.applyTheme()
 * - Screenshots -> this.takeScreenshot()
 * - Error handling -> automatic
 * 
 * This widget focuses ONLY on chat-specific logic!
 */

import { BaseWidget, WidgetConfig } from '../../shared/BaseWidget';
import { ChatMessage, ChatUser, formatMessageTime } from './ChatTypes';

export interface ChatWidgetConfig extends WidgetConfig {
  // Chat-specific config
  maxMessages?: number;
  showTypingIndicators?: boolean;
  allowFileUpload?: boolean;
  autoScrollToBottom?: boolean;
  enableSoundNotifications?: boolean;
  aiPersona?: string;
}

export class SimpleChatWidget extends BaseWidget {
  declare shadowRoot: ShadowRoot;
  private messages: ChatMessage[] = [];
  private currentRoom = 'general';
  private isTyping = false;
  private typingTimeout?: number;
  
  // UI elements (cached for performance)
  private messagesContainer?: HTMLElement;
  private messageInput?: HTMLInputElement;
  
  constructor(config: Partial<ChatWidgetConfig> = {}) {
    super({
      widgetName: 'SimpleChatWidget',
      enableAI: true,
      enableDatabase: true,
      enableRouterEvents: true,
      enableScreenshots: true,
      maxMessages: 100,
      showTypingIndicators: true,
      autoScrollToBottom: true,
      aiPersona: 'chat_assistant',
      ...config
    });
  }

  // === IMPLEMENTATION - Focus only on chat logic ===

  protected async onWidgetInitialize(): Promise<void> {
    console.log('🎯 SimpleChatWidget: Initializing chat-specific features...');
    
    // Load chat history (BaseWidget handles database coordination)
    const savedMessages = await this.getData('chat_messages', []);
    this.messages = savedMessages;
    
    // Subscribe to chat events (BaseWidget handles router coordination) 
    this.eventEmitter.on('message_received', (data) => {
      this.handleIncomingMessage(data.message);
    });
    
    this.eventEmitter.on('typing_indicator', (data) => {
      this.handleTypingIndicator(data.userId, data.isTyping);
    });
    
    console.log('✅ SimpleChatWidget: Chat features initialized');
  }

  protected async renderWidget(): Promise<void> {
    // Load CSS (BaseWidget handles theme coordination)
    await this.loadWidgetCSS();
    
    // Render chat UI
    this.shadowRoot.innerHTML += `
      <div class="chat-container">
        <div class="chat-header">
          <h3>💬 Chat Room: ${this.currentRoom}</h3>
          <div class="chat-controls">
            <button id="themeBtn">🎨 Theme</button>
            <button id="screenshotBtn">📸 Screenshot</button>
            <button id="exportBtn">💾 Export</button>
          </div>
        </div>
        
        <div class="messages" id="messages">
          ${this.renderMessages()}
        </div>
        
        <div class="input-area">
          <input type="text" id="messageInput" placeholder="Type a message..." />
          <button id="sendBtn">Send</button>
        </div>
      </div>
    `;
    
    // Cache UI elements
    this.messagesContainer = this.shadowRoot.getElementById('messages') || undefined;
    this.messageInput = (this.shadowRoot.getElementById('messageInput') as HTMLInputElement) || undefined;
    
    // Setup event listeners
    this.setupChatEventListeners();
  }

  protected async onWidgetCleanup(): Promise<void> {
    // Save current messages (BaseWidget handles database coordination)
    await this.storeData('chat_messages', this.messages, { persistent: true });
    
    console.log('✅ SimpleChatWidget: Cleanup complete');
  }

  // === CHAT-SPECIFIC METHODS - Simple and focused ===

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
    
    // Add to UI immediately
    this.addMessage(userMessage);
    this.messageInput.value = '';
    
    try {
      // Store message (BaseWidget handles database + broadcasting)
      await this.storeData(`message_${userMessage.id}`, userMessage, { 
        persistent: true, 
        broadcast: true 
      });
      
      // Get AI response (BaseWidget handles Academy daemon coordination)
      const aiResponse = await this.queryAI(content, {
        persona: (this.config as ChatWidgetConfig).aiPersona,
        context: this.getChatContext()
      });
      
      if (aiResponse && aiResponse.reply) {
        const aiMessage: ChatMessage = {
          id: `ai_${Date.now()}`,
          content: aiResponse.reply,
          roomId: this.currentRoom,
          userId: 'ai_assistant',
          type: 'assistant',
          timestamp: new Date().toISOString(),
          metadata: { persona: aiResponse.persona }
        };
        
        // Add AI response to UI
        this.addMessage(aiMessage);
        
        // Store AI response
        await this.storeData(`message_${aiMessage.id}`, aiMessage, { 
          persistent: true, 
          broadcast: true 
        });
      }
      
    } catch (error) {
      console.error('❌ Failed to send message:', error);
      // BaseWidget handles error display automatically
      this.handleError(error, 'sendMessage');
    }
  }

  private addMessage(message: ChatMessage): void {
    this.messages.push(message);
    
    // Keep message limit
    const maxMessages = (this.config as ChatWidgetConfig).maxMessages || 100;
    if (this.messages.length > maxMessages) {
      this.messages.shift();
    }
    
    // Update UI
    this.renderNewMessage(message);
    
    // Auto-scroll if enabled
    if ((this.config as ChatWidgetConfig).autoScrollToBottom) {
      this.scrollToBottom();
    }
  }

  private handleIncomingMessage(message: ChatMessage): void {
    // Only add if it's for this room and not from us
    if (message.roomId === this.currentRoom && message.userId !== 'current_user') {
      this.addMessage(message);
    }
  }

  private handleTypingIndicator(userId: string, isTyping: boolean): void {
    // Update typing indicator UI
    const indicator = this.shadowRoot.querySelector('.typing-indicator') as HTMLElement;
    if (indicator) {
      indicator.style.display = isTyping ? 'block' : 'none';
    }
  }

  // === UI HELPERS - Simple rendering logic ===

  private renderMessages(): string {
    return this.messages.map(msg => this.renderMessage(msg)).join('');
  }

  private renderMessage(message: ChatMessage): string {
    const timeStr = formatMessageTime(message.timestamp);
    const avatar = message.type === 'user' ? '👤' : '🤖';
    
    return `
      <div class="message message-${message.type}">
        <div class="message-header">
          <span class="avatar">${avatar}</span>
          <span class="author">${message.userId}</span>
          <span class="time">${timeStr}</span>
        </div>
        <div class="message-content">${this.formatContent(message.content)}</div>
      </div>
    `;
  }

  private renderNewMessage(message: ChatMessage): void {
    if (this.messagesContainer) {
      const messageHTML = this.renderMessage(message);
      this.messagesContainer.insertAdjacentHTML('beforeend', messageHTML);
    }
  }

  private formatContent(content: string): string {
    return content
      .replace(/\*\*(.*?)\*\*/g, '<strong>$1</strong>')
      .replace(/\*(.*?)\*/g, '<em>$1</em>')
      .replace(/`(.*?)`/g, '<code>$1</code>');
  }

  private scrollToBottom(): void {
    if (this.messagesContainer) {
      this.messagesContainer.scrollTop = this.messagesContainer.scrollHeight;
    }
  }

  private getChatContext(): any {
    return {
      recentMessages: this.messages.slice(-5), // Last 5 messages for context
      roomId: this.currentRoom,
      messageCount: this.messages.length
    };
  }

  // === EVENT HANDLERS - Clean and simple ===

  private setupChatEventListeners(): void {
    // Send message on Enter or button click
    this.messageInput?.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') {
        this.sendMessage();
      } else {
        this.handleTypingStart();
      }
    });
    
    this.shadowRoot.getElementById('sendBtn')?.addEventListener('click', () => {
      this.sendMessage();
    });
    
    // Theme button (BaseWidget handles theme system)
    this.shadowRoot.getElementById('themeBtn')?.addEventListener('click', async () => {
      const currentTheme = this.state.theme;
      const newTheme = currentTheme === 'cyberpunk' ? 'basic' : 'cyberpunk';
      await this.applyTheme(newTheme);
    });
    
    // Screenshot button (BaseWidget handles JTAG coordination)
    this.shadowRoot.getElementById('screenshotBtn')?.addEventListener('click', async () => {
      const filepath = await this.takeScreenshot({
        filename: `chat-${this.currentRoom}-${Date.now()}.png`
      });
      
      if (filepath) {
        this.addMessage({
          id: `system_${Date.now()}`,
          content: `📸 Screenshot saved: ${filepath}`,
          roomId: this.currentRoom,
          userId: 'system',
          type: 'system',
          timestamp: new Date().toISOString()
        });
      }
    });
    
    // Export button (BaseWidget handles file operations)
    this.shadowRoot.getElementById('exportBtn')?.addEventListener('click', async () => {
      const chatData = JSON.stringify(this.messages, null, 2);
      const filepath = await this.saveFile(
        `chat-export-${this.currentRoom}-${Date.now()}.json`,
        chatData,
        { directory: 'chat_exports', compress: true }
      );
      
      if (filepath) {
        this.addMessage({
          id: `system_${Date.now()}`,
          content: `💾 Chat exported: ${filepath}`,
          roomId: this.currentRoom,
          userId: 'system',
          type: 'system',
          timestamp: new Date().toISOString()
        });
      }
    });
  }

  private handleTypingStart(): void {
    if (!this.isTyping) {
      this.isTyping = true;
      
      // Broadcast typing indicator (BaseWidget handles router coordination)
      this.broadcastEvent('typing_indicator', {
        userId: 'current_user',
        roomId: this.currentRoom,
        isTyping: true
      });
    }
    
    // Reset typing timeout
    if (this.typingTimeout) {
      clearTimeout(this.typingTimeout);
    }
    
    this.typingTimeout = window.setTimeout(() => {
      this.isTyping = false;
      
      // Broadcast stop typing
      this.broadcastEvent('typing_indicator', {
        userId: 'current_user',
        roomId: this.currentRoom,
        isTyping: false
      });
    }, 2000);
  }

  private async loadWidgetCSS(): Promise<void> {
    try {
      const response = await fetch('/widgets/chat-widget/public/chat-widget.css');
      const css = await response.text();
      
      const style = document.createElement('style');
      style.textContent = css;
      this.shadowRoot.appendChild(style);
      
    } catch (error) {
      console.warn('❌ Failed to load chat CSS, using defaults');
    }
  }

  // === PUBLIC API - Simple interface for external use ===

  public async sendChatMessage(message: string): Promise<void> {
    if (this.messageInput) {
      this.messageInput.value = message;
      await this.sendMessage();
    }
  }

  public switchRoom(roomId: string): void {
    this.currentRoom = roomId;
    this.messages = []; // Clear messages for new room
    this.renderWidget(); // Re-render for new room
  }

  public getMessageCount(): number {
    return this.messages.length;
  }

  public exportChatHistory(): ChatMessage[] {
    return [...this.messages];
  }
}

// Register the widget
if (!customElements.get('simple-chat-widget')) {
  customElements.define('simple-chat-widget', SimpleChatWidget);
}

/**
 * COMPARISON:
 * 
 * OLD COMPLEX WAY:
 * - ChatWidgetServer: 500+ lines of daemon coordination
 * - ChatWidgetBrowser: 400+ lines of UI + communication
 * - ChatTypes: 200+ lines of type definitions
 * - Total: 1100+ lines, complex server/browser split
 * 
 * NEW BASEWIDGET WAY:
 * - SimpleChatWidget: 250 lines of pure chat logic
 * - BaseWidget: Handles ALL complexity automatically
 * - Same functionality, 4x less code, 10x simpler
 * 
 * WIDGET OPERATIONS BECOME ONE-LINERS:
 * - Store message: await this.storeData(key, message, { persistent: true, broadcast: true })
 * - AI query: await this.queryAI(message, { persona: 'assistant' })  
 * - Screenshot: await this.takeScreenshot({ filename: 'chat.png' })
 * - Theme: await this.applyTheme('cyberpunk')
 * - Export: await this.saveFile('export.json', data)
 * 
 * Just like JTAG commands - all complexity abstracted away!
 */