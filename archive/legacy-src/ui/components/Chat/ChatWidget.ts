/**
 * ChatWidget - TypeScript Web Component
 * Handles message display and input for chat rooms
 */

import { BaseWidget } from '../shared/BaseWidget';
import { universalUserSystem } from '../shared/UniversalUserSystem';
import { roomDataManager, RoomData, RoomChangeEvent } from '../shared/RoomDataManager';
import { 
  ChatEventsConfig
} from '../shared/ChatEventTypes';

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
  private currentRoom: RoomData | null = null; // Now using complete room data instead of just ID
  private isLoadingHistory: boolean = false;
  private chatEventsConfig: ChatEventsConfig | null = null; // Loaded from JSON


  constructor() {
    console.log('🏗️ ChatWidget: Constructor called, about to call super()');
    super();
    console.log('🏗️ ChatWidget: super() completed, setting properties');
    this.widgetName = 'ChatWidget';
    this.widgetIcon = '💬';
    this.widgetTitle = 'Chat';
    console.log('🏗️ ChatWidget: Constructor complete');
    // CSS loaded via declarative asset system
  }


  protected async initializeWidget(): Promise<void> {
    await this.initializeRoomDataManager();
    await this.loadChatEventsConfig();
    await this.initializeChat();
    this.setupContinuumListeners();
    this.setupServerControlListeners();
    this.setupUniversalUserSystem();
    this.setupRoomDataListeners();
  }

  private async initializeRoomDataManager(): Promise<void> {
    console.log('💬 Chat: Initializing room data manager...');
    
    // Initialize room data manager if not already done
    await roomDataManager.initialize();
    
    // Set initial room (either current room or default)
    if (!this.currentRoom) {
      this.currentRoom = roomDataManager.getCurrentRoom() || roomDataManager.getRoom('general');
      if (!this.currentRoom) {
        console.warn('💬 Chat: No rooms available, will show placeholder');
      }
    }
  }

  private async loadChatEventsConfig(): Promise<void> {
    try {
      console.log('💬 Chat: Loading chat events configuration...');
      
      const response = await fetch('/src/ui/components/shared/chat-events-config.json');
      if (!response.ok) {
        throw new Error(`Failed to load chat events config: ${response.status}`);
      }
      
      this.chatEventsConfig = await response.json() as ChatEventsConfig;
      console.log('💬 Chat: Chat events configuration loaded');
      
    } catch (error) {
      console.warn('💬 Chat: Failed to load chat events config, using defaults:', error);
      // Minimal fallback config
      this.chatEventsConfig = {
        chatEvents: {
          message_received: { handler: 'handleIncomingMessage', description: 'New message' },
          agent_typing: { handler: 'setTypingIndicator', handlerArgs: [true], description: 'Typing' },
          agent_stop_typing: { handler: 'setTypingIndicator', handlerArgs: [false], description: 'Stop typing' }
        },
        globalEvents: {},
        messageTypes: {
          user: { icon: '👤', className: 'user-message', showAvatar: true, allowEdit: true },
          assistant: { icon: '🤖', className: 'assistant-message', showAvatar: true, allowEdit: false },
          system: { icon: 'ℹ️', className: 'system-message', showAvatar: false, allowEdit: false }
        },
        messageStatuses: {
          sending: { icon: '⏳', className: 'status-sending', description: 'Sending...' },
          sent: { icon: '✓', className: 'status-sent', description: 'Sent' },
          error: { icon: '❌', className: 'status-error', description: 'Failed' }
        }
      };
    }
  }

  private async initializeChat(): Promise<void> {
    const roomInfo = this.currentRoom ? `${this.currentRoom.name} (${this.currentRoom.type})` : 'unknown room';
    console.log(`💬 Chat: Initializing chat for room: ${roomInfo}`);
    
    // Load history in background - don't block initialization
    this.loadRoomHistory().catch(error => {
      console.warn(`💬 Chat: History loading failed (non-blocking):`, error);
    });
    
    if (this.messages.length === 0 && this.currentRoom) {
      // Use proper welcome message from room type config
      const welcomeMessage = roomDataManager.getWelcomeMessage(this.currentRoom.type);
      
      this.addMessage({
        id: this.generateMessageId(),
        type: 'system',
        content: welcomeMessage,
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
        roomId: this.currentRoom?.id,
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
      // Listen for API ready event instead of polling
      window.addEventListener('continuum:ready', () => {
        this.setupContinuumListeners();
      }, { once: true });
      return;
    }

    if (!this.chatEventsConfig) {
      console.warn('💬 Chat: No chat events config available, skipping event setup');
      return;
    }

    // Setup chat events from JSON configuration
    for (const [eventName, eventConfig] of Object.entries(this.chatEventsConfig.chatEvents)) {
      this.notifySystem(eventName, (data: any) => {
        // Check if event requires current room matching
        if (eventConfig.requiresCurrentRoom && eventConfig.matchField) {
          const fieldValue = data[eventConfig.matchField];
          if (fieldValue !== this.currentRoom?.id) {
            return; // Event not for current room
          }
        }

        // Call the handler method
        this.callEventHandler(eventConfig.handler, data, eventConfig.handlerArgs);
      });
    }

    // Setup global events from JSON configuration
    for (const [eventName, eventConfig] of Object.entries(this.chatEventsConfig.globalEvents)) {
      const listenerOptions = eventConfig.once ? { once: true } : {};
      
      document.addEventListener(eventName, (e: Event) => {
        let data = e;
        
        // Extract data from specified path if configured
        if (eventConfig.dataPath) {
          const pathParts = eventConfig.dataPath.split('.');
          let current: any = e;
          for (const part of pathParts) {
            current = current?.[part];
          }
          data = current;
        }
        
        this.callEventHandler(eventConfig.handler, data);
      }, listenerOptions);
    }
  }

  private callEventHandler(handlerName: string, data: any, handlerArgs?: any[]): void {
    try {
      const handler = (this as any)[handlerName];
      if (typeof handler === 'function') {
        if (handlerArgs) {
          handler.call(this, ...handlerArgs);
        } else {
          handler.call(this, data);
        }
      } else {
        console.warn(`💬 Chat: Handler '${handlerName}' not found`);
      }
    } catch (error) {
      console.error(`💬 Chat: Error calling handler '${handlerName}':`, error);
    }
  }

  private setupRoomDataListeners(): void {
    // Listen to centralized room data manager
    roomDataManager.addEventListener('room-changed', (e: Event) => {
      const customEvent = e as CustomEvent<RoomChangeEvent>;
      const { currentRoom } = customEvent.detail;
      this.handleRoomChange(currentRoom);
    });

    roomDataManager.addEventListener('current-room-updated', (e: Event) => {
      const customEvent = e as CustomEvent<RoomData>;
      const updatedRoom = customEvent.detail;
      
      // Update our current room if it's the same room
      if (this.currentRoom?.id === updatedRoom.id) {
        this.currentRoom = updatedRoom;
        this.updateRoomDisplay();
      }
    });
  }

  private handleRoomChange(newRoom: RoomData): void {
    console.log(`💬 Chat: Room changed to ${newRoom.name} (${newRoom.type})`);
    this.currentRoom = newRoom;
    this.messages = [];
    
    // Add welcome message for new room
    const welcomeMessage = roomDataManager.getWelcomeMessage(newRoom.type);
    this.addMessage({
      id: this.generateMessageId(),
      type: 'system',
      content: welcomeMessage,
      timestamp: new Date()
    });
    
    this.loadRoomHistory().catch(error => {
      console.warn(`💬 Chat: Failed to load history for room ${newRoom.name}:`, error);
    });
    
    this.render();
  }

  private updateRoomDisplay(): void {
    // Update any UI elements that show room info
    const roomTitle = this.shadowRoot?.querySelector('.room-title');
    if (roomTitle && this.currentRoom) {
      roomTitle.textContent = this.currentRoom.name;
    }
    
    // Update widget title to include room name
    if (this.currentRoom) {
      this.widgetTitle = `💬 ${this.currentRoom.name}`;
    }
  }

  public async switchRoom(roomId: string): Promise<void> {
    if (this.currentRoom?.id === roomId) return;

    console.log(`💬 Chat: Switching to room ${roomId}`);
    
    // Use room data manager to switch rooms
    const success = roomDataManager.setCurrentRoom(roomId);
    if (!success) {
      console.warn(`💬 Chat: Failed to switch to room ${roomId} - room not found`);
      return;
    }
    
    // Room change will be handled by handleRoomChange() via event listener
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
              <div class="chat-title">${this.getRoomDisplayName()}</div>
              <div class="chat-subtitle">${this.getRoomDescription()} • Connected</div>
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
            ${this.renderSimpleConnectedUsers()}
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

    // Simple connected users - no action buttons needed in chat header
    // Full user management with action buttons is handled by UserSelector in sidebar
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
        roomId: this.currentRoom?.id,
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

  public handleIncomingMessage(data: any): void {
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
    // TODO: Add AI model initialization when universalUserSystem supports it
    console.log(`💬 Chat: Universal user system ready`);
    
    // Listen for user updates to refresh UI
    universalUserSystem.on('user:updated', () => {
      this.render(); // Refresh connected users display
    });

    // Handle user clicks (personas, AI models, etc.)
    universalUserSystem.on('persona:interaction-requested', (data: any) => {
      this.startConversationWithUser(data.personaId, data.personaName, 'persona');
    });

    universalUserSystem.on('ai-model:conversation-requested', (data: any) => {
      this.startConversationWithUser(data.modelId, data.modelName, 'ai-model');
    });
  }

  /**
   * Start conversation with any user type - same interface for all
   */
  private async startConversationWithUser(userId: string, userName: string, _userType: string): Promise<void> {
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
    this.addEventListener('widget:screenshot-complete', (event: Event) => {
      const customEvent = event as CustomEvent;
      const { success, result, error } = customEvent.detail;
      
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
    this.addEventListener('widget:refresh-complete', (event: Event) => {
      const customEvent = event as CustomEvent;
      const { success, error } = customEvent.detail;
      
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
    // Use proper room name from room data manager
    if (this.currentRoom) {
      return this.currentRoom.name;
    }
    
    // Fallback for no room data
    return 'Chat';
  }

  private getRoomDescription(): string {
    // Use proper room description from room data manager
    if (this.currentRoom) {
      return this.currentRoom.description;
    }
    
    // Fallback for no room data
    return 'Chat room';
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

  private renderSimpleConnectedUsers(): string {
    // Simple connected user list for chat header - no action buttons
    const users = universalUserSystem.getAllUsers();
    const onlineUsers = users.filter(user => user.status === 'online');
    
    return onlineUsers.map(user => `
      <span class="simple-user-badge" title="${user.name}">
        <span class="user-avatar">${user.avatar}</span>
        <span class="user-name">${user.name}</span>
      </span>
    `).join('');
  }
}

// Register the custom element
// Prevent duplicate widget registration
if (!customElements.get('chat-widget')) {
    customElements.define('chat-widget', ChatWidget);
}