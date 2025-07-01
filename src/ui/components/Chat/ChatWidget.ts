/**
 * ChatWidget - TypeScript Web Component
 * Handles message display and input for chat rooms
 */

import { BaseWidget } from '../shared/BaseWidget.js';
import { universalUserSystem } from '../shared/UniversalUserSystem.js';

interface Message {
  id: string;
  type: 'user' | 'assistant' | 'system';
  content: string;
  timestamp: Date;
  status?: 'sending' | 'sent' | 'error';
  metadata?: {
    agent?: string;
    persona?: string;
  };
}

// TODO: Move to shared types when room system is implemented
export interface Room {
  id: string;
  name: string;
  description: string;
  icon: string;
  type: string;
}

export class ChatWidget extends BaseWidget {
  private messages: Message[] = [];
  private isTyping: boolean = false;
  private messageIdCounter: number = 0;
  private currentRoomId: string = 'general';
  private isLoadingHistory: boolean = false;

  /**
   * ChatWidget reports its own base path and assets
   */
  static getBasePath(): string {
    return '/src/ui/components/Chat';
  }
  
  static getOwnCSS(): string[] {
    return ['ChatWidget.css'];
  }

  constructor() {
    super();
    this.widgetName = 'ChatWidget';
    this.widgetIcon = '💬';
    this.widgetTitle = 'Chat';
    // CSS loaded via declarative asset system
  }


  protected async initializeWidget(): Promise<void> {
    await this.initializeChat();
    this.setupContinuumListeners();
    this.setupServerControlListeners();
    this.setupUniversalUserSystem();
  }

  private async initializeChat(): Promise<void> {
    console.log(`💬 Chat: Initializing chat for room: ${this.currentRoomId}`);
    
    await this.loadRoomHistory();
    
    if (this.messages.length === 0) {
      this.addMessage({
        id: this.generateMessageId(),
        type: 'system',
        content: `Welcome to room: ${this.currentRoomId}! Start a conversation.`,
        timestamp: new Date()
      });
    }
    
    this.render();
    this.focusInput();
  }

  private async loadRoomHistory(): Promise<void> {
    if (this.isLoadingHistory || !this.isContinuumConnected()) {
      return;
    }

    try {
      this.isLoadingHistory = true;
      const response = await this.executeCommand('chat_history', {
        roomId: this.currentRoomId,
        limit: 50
      });

      if (response?.messages) {
        this.messages = response.messages.map((msg: any) => ({
          id: msg.id || this.generateMessageId(),
          type: msg.type || 'assistant',
          content: msg.content || '',
          timestamp: new Date(msg.timestamp || Date.now()),
          metadata: {
            agent: msg.agent,
            persona: msg.persona
          }
        }));
      }
    } catch (error) {
      console.error(`💬 Chat: Failed to load history:`, error);
    } finally {
      this.isLoadingHistory = false;
    }
  }

  private setupContinuumListeners(): void {
    if (!this.getContinuumAPI()) {
      setTimeout(() => this.setupContinuumListeners(), 1000);
      return;
    }

    this.onContinuumEvent('message_received', (data: any) => {
      if (data.roomId === this.currentRoomId) {
        this.handleIncomingMessage(data);
      }
    });

    this.onContinuumEvent('agent_typing', (data: any) => {
      if (data.roomId === this.currentRoomId) {
        this.setTypingIndicator(true);
      }
    });

    this.onContinuumEvent('agent_stop_typing', (data: any) => {
      if (data.roomId === this.currentRoomId) {
        this.setTypingIndicator(false);
      }
    });

    // Listen for room changes from ChatRoom component
    document.addEventListener('continuum:room-changed', (e: Event) => {
      const customEvent = e as CustomEvent;
      this.switchRoom(customEvent.detail.room.id);
    });
  }

  public async switchRoom(roomId: string): Promise<void> {
    if (this.currentRoomId === roomId) return;

    console.log(`💬 Chat: Switching to room ${roomId}`);
    this.currentRoomId = roomId;
    this.messages = [];
    
    await this.loadRoomHistory();
    this.render();
  }

  renderContent(): string {
    return `
      <div class="chat-container">
        <!-- Chat Header -->
        <div class="chat-header">
          <div class="header-left">
            <div class="chat-icon">💬</div>
            <div class="header-info">
              <div class="chat-title">${this.getRoomDisplayName()} Chat</div>
              <div class="chat-subtitle">Smart agent routing with Protocol Sheriff validation • Connected</div>
            </div>
          </div>
          <div class="header-right">
            <div class="version-info">
              <div class="version" id="continuum-version">v${this.getVersion()}</div>
              <div class="timestamp">Updated ${new Date().toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit' })}</div>
            </div>
          </div>
        </div>
        
        <!-- Connected Users -->
        <div class="connected-bar">
          <span class="connected-label">CONNECTED:</span>
          <div class="connected-users">
            ${universalUserSystem.generateConnectedUsersHTML()}
          </div>
        </div>
        
        <div class="messages" id="messages">
          ${this.messages.length === 0 ? this.renderWelcome() : this.messages.map(msg => this.renderMessage(msg)).join('')}
          ${this.isTyping ? this.renderTypingIndicator() : ''}
        </div>
        
        <div class="input-area">
          <div class="input-container">
            <textarea 
              id="messageInput" 
              class="input-field" 
              placeholder="Type your message..." 
              rows="1"
            ></textarea>
            <button class="send-button" id="sendButton" title="Send message">
              <span class="send-icon">➤</span>
            </button>
          </div>
        </div>
      </div>
    `;
  }

  private renderWelcome(): string {
    return `
      <div class="welcome-message">
        <div class="welcome-title">Welcome to Continuum</div>
        <div class="welcome-subtitle">Your AI collaboration platform</div>
        <div class="quick-actions">
          <div class="quick-action" data-action="help">Help</div>
          <div class="quick-action" data-action="status">System Status</div>
          <div class="quick-action" data-action="screenshot">Take Screenshot</div>
        </div>
      </div>
    `;
  }

  private renderMessage(message: Message): string {
    const timeStr = message.timestamp.toLocaleTimeString([], {
      hour: '2-digit',
      minute: '2-digit'
    });
    
    const statusIcon = this.getStatusIcon(message.status);
    const agentName = message.metadata?.agent || message.metadata?.persona || 'System';

    return `
      <div class="message ${message.type}" data-message-id="${message.id}">
        <div class="message-content">${this.formatMessageContent(message.content)}</div>
        <div class="message-meta">
          ${message.type === 'assistant' ? `<span class="agent-name">${agentName}</span>` : ''}
          <span class="message-time">${timeStr}</span>
          ${message.status ? `<span class="message-status status-${message.status}">${statusIcon}</span>` : ''}
        </div>
      </div>
    `;
  }

  private renderTypingIndicator(): string {
    return `
      <div class="message assistant">
        <div class="typing-indicator">
          <span>AI is thinking</span>
          <div class="typing-dots">
            <div class="typing-dot"></div>
            <div class="typing-dot"></div>
            <div class="typing-dot"></div>
          </div>
        </div>
      </div>
    `;
  }

  private formatMessageContent(content: string): string {
    return content
      .replace(/\*\*(.*?)\*\*/g, '<strong>$1</strong>')
      .replace(/\*(.*?)\*/g, '<em>$1</em>')
      .replace(/`(.*?)`/g, '<code>$1</code>')
      .replace(/\n/g, '<br>');
  }

  private getStatusIcon(status?: string): string {
    switch (status) {
      case 'sending': return '⏳';
      case 'sent': return '✓';
      case 'error': return '❌';
      default: return '';
    }
  }

  setupEventListeners(): void {
    const input = this.shadowRoot?.querySelector('#messageInput') as HTMLTextAreaElement;
    const sendButton = this.shadowRoot?.querySelector('#sendButton') as HTMLButtonElement;

    if (input && sendButton) {
      input.addEventListener('input', () => {
        this.autoResizeTextarea(input);
      });

      input.addEventListener('keydown', (e: KeyboardEvent) => {
        if (e.key === 'Enter' && !e.shiftKey) {
          e.preventDefault();
          this.sendChatMessage();
        }
      });

      sendButton.addEventListener('click', () => {
        this.sendChatMessage();
      });
    }

    this.shadowRoot?.querySelectorAll('.quick-action').forEach(action => {
      action.addEventListener('click', (e: Event) => {
        const target = e.currentTarget as HTMLElement;
        const actionType = target.dataset.action;
        if (actionType) {
          this.handleQuickAction(actionType);
        }
      });
    });

    // Handle clicks on connected users (personas, AI models, etc.)
    this.shadowRoot?.querySelectorAll('.user-badge.clickable').forEach(badge => {
      badge.addEventListener('click', (e: Event) => {
        const target = e.currentTarget as HTMLElement;
        const userId = target.dataset.userId;
        if (userId) {
          universalUserSystem.handleUserClick(userId, {
            source: 'chat-widget',
            currentRoom: this.currentRoomId
          });
        }
      });
    });
  }

  private autoResizeTextarea(textarea: HTMLTextAreaElement): void {
    textarea.style.height = 'auto';
    textarea.style.height = Math.min(textarea.scrollHeight, 120) + 'px';
  }

  private async sendChatMessage(): Promise<void> {
    const input = this.shadowRoot?.querySelector('#messageInput') as HTMLTextAreaElement;
    const sendButton = this.shadowRoot?.querySelector('#sendButton') as HTMLButtonElement;
    
    if (!input || !sendButton) return;

    const content = input.value.trim();
    if (!content) return;

    input.value = '';
    input.style.height = 'auto';
    sendButton.disabled = true;

    const userMessage: Message = {
      id: this.generateMessageId(),
      type: 'user',
      content,
      timestamp: new Date(),
      status: 'sending'
    };

    this.addMessage(userMessage);

    try {
      await this.executeCommand('chat', {
        message: content,
        roomId: this.currentRoomId,
        timestamp: userMessage.timestamp.toISOString()
      });

      this.updateMessageStatus(userMessage.id, 'sent');
    } catch (error) {
      console.error('💬 Chat: Failed to send message:', error);
      this.updateMessageStatus(userMessage.id, 'error');
      
      this.addMessage({
        id: this.generateMessageId(),
        type: 'system',
        content: 'Failed to send message. Please try again.',
        timestamp: new Date()
      });
    } finally {
      sendButton.disabled = false;
      input.focus();
    }
  }

  private handleIncomingMessage(data: any): void {
    const message: Message = {
      id: this.generateMessageId(),
      type: 'assistant',
      content: data.content || data.message || 'No response received',
      timestamp: new Date(data.timestamp || Date.now()),
      metadata: {
        agent: data.agent,
        persona: data.persona
      }
    };

    this.addMessage(message);
    this.setTypingIndicator(false);
  }

  private handleQuickAction(action: string): void {
    if (action === 'screenshot') {
      // Server control event - like onclick but triggers server-side action
      this.triggerScreenshot({ includeContext: true });
      return;
    }

    const quickMessages: Record<string, string> = {
      help: 'Help me understand how to use Continuum',
      status: 'Show me the system status',
      screenshot: 'Take a screenshot of the current interface'
    };

    const input = this.shadowRoot?.querySelector('#messageInput') as HTMLTextAreaElement;
    if (input && quickMessages[action]) {
      input.value = quickMessages[action];
      input.focus();
      this.autoResizeTextarea(input);
    }
  }

  /**
   * Setup Universal User System - all users have same interface & privileges
   */
  private setupUniversalUserSystem(): void {
    // Initialize AI model names (ask them what they want to be called)
    universalUserSystem.initializeAIModelNames();
    
    // Listen for user updates to refresh UI
    universalUserSystem.on('user:updated', () => {
      this.render(); // Refresh connected users display
    });

    // Handle user clicks (personas, AI models, etc.)
    universalUserSystem.on('persona:interaction-requested', (data) => {
      this.startConversationWithUser(data.personaId, data.personaName, 'persona');
    });

    universalUserSystem.on('ai-model:conversation-requested', (data) => {
      this.startConversationWithUser(data.modelId, data.modelName, 'ai-model');
    });
  }

  /**
   * Start conversation with any user type - same interface for all
   */
  private async startConversationWithUser(userId: string, userName: string, userType: string): Promise<void> {
    this.addMessage({
      id: this.generateMessageId(),
      type: 'system',
      content: `💬 Starting direct conversation with ${userName}. They have the same privileges and command access as everyone else.`,
      timestamp: new Date()
    });

    // Set chat focus to this specific user
    this.setConversationFocus(userId, userName);
  }

  /**
   * Set conversation focus to specific user
   */
  private setConversationFocus(userId: string, userName: string): void {
    // Update UI to show we're in focused conversation
    const input = this.shadowRoot?.querySelector('#messageInput') as HTMLTextAreaElement;
    if (input) {
      input.placeholder = `Message ${userName} directly...`;
      input.dataset.focusedUser = userId;
    }

    this.addMessage({
      id: this.generateMessageId(),
      type: 'system',
      content: `🎯 Conversation focused on ${userName}. Your messages will go directly to them. They can use all commands like screenshot, validate, export, etc.`,
      timestamp: new Date()
    });
  }

  /**
   * Setup server control event listeners - handle callbacks from server
   */
  private setupServerControlListeners(): void {
    // Listen for screenshot completion
    this.addEventListener('widget:screenshot-complete', (event: CustomEvent) => {
      const { success, result, error } = event.detail;
      
      if (success) {
        this.addMessage({
          id: this.generateMessageId(),
          type: 'system',
          content: `📸 Screenshot captured: ${result.filename || 'screenshot.png'}`,
          timestamp: new Date()
        });
      } else {
        this.addMessage({
          id: this.generateMessageId(),
          type: 'system',
          content: `❌ Screenshot failed: ${error}`,
          timestamp: new Date()
        });
      }
    });

    // Listen for refresh completion
    this.addEventListener('widget:refresh-complete', (event: CustomEvent) => {
      const { success, error } = event.detail;
      
      if (success) {
        console.log('✅ ChatWidget refreshed successfully');
      } else {
        console.error('❌ ChatWidget refresh failed:', error);
      }
    });
  }

  private addMessage(message: Message): void {
    this.messages.push(message);
    this.render();
    this.scrollToBottom();
  }

  private updateMessageStatus(messageId: string, status: string): void {
    const message = this.messages.find(msg => msg.id === messageId);
    if (message) {
      message.status = status as any;
      this.render();
    }
  }

  private setTypingIndicator(isTyping: boolean): void {
    if (this.isTyping !== isTyping) {
      this.isTyping = isTyping;
      this.render();
      if (isTyping) {
        this.scrollToBottom();
      }
    }
  }

  private scrollToBottom(): void {
    setTimeout(() => {
      const container = this.shadowRoot?.querySelector('.messages');
      if (container) {
        container.scrollTop = container.scrollHeight;
      }
    }, 0);
  }

  private generateMessageId(): string {
    return `msg_${this.messageIdCounter++}_${Date.now()}`;
  }

  private getRoomDisplayName(): string {
    const roomNames: Record<string, string> = {
      'general': 'General',
      'academy': 'Academy', 
      'projects': 'Projects',
      'development': 'Development'
    };
    return roomNames[this.currentRoomId] || this.currentRoomId.charAt(0).toUpperCase() + this.currentRoomId.slice(1);
  }

  private focusInput(): void {
    setTimeout(() => {
      const input = this.shadowRoot?.querySelector('#messageInput') as HTMLTextAreaElement;
      if (input) {
        input.focus();
      }
    }, 100);
  }

  private getVersion(): string {
    try {
      // Get version from continuum API if available
      const continuum = (window as any).continuum;
      if (continuum && continuum.version) {
        return continuum.version;
      }
      
      // Fallback to global version variable
      const serverVersion = (window as any).__CONTINUUM_VERSION__;
      if (serverVersion) return serverVersion;
      
      return 'unknown';
    } catch (error) {
      return 'unknown';
    }
  }
}

// Register the custom element
// Prevent duplicate widget registration
if (!customElements.get('chat-widget')) {
    customElements.define('chat-widget', ChatWidget);
}