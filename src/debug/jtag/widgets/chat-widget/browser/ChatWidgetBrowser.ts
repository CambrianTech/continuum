/**
 * Modern Chat Widget Browser - JTAG Event-Driven Architecture
 * 
 * Connects as proper JTAG client, subscribes to room events,
 * supports multiple rooms and personas as full citizens.
 * 
 * Key Architecture:
 * - Real JTAG client connection (not DOM manipulation)
 * - Event-driven updates via chat-message-sent events
 * - Command-based messaging via chat/send-message
 * - Data persistence via data/list and data/create commands
 * - Multi-room support with room switching
 * - Persona management as full participants
 */

import { 
  ChatMessage, 
  ChatUser, 
  ChatRoom, 
  ChatWidgetConfig, 
  ChatWidgetState,
  ChatEventEmitter,
  formatMessageTime,
  getMessageAuthor
} from '../shared/ChatTypes';

// JTAG client interface - consistent with other widgets
declare global {
  interface Window {
    jtag: {
      connect(): Promise<{
        client: {
          commands: {
            [key: string]: (params: any) => Promise<any>;
          };
          events: {
            on(eventType: string, handler: (data: any) => void): void;
            off(eventType: string, handler: (data: any) => void): void;
          };
        };
      }>;
    };
  }
}

export class ChatWidgetBrowser extends HTMLElement {
  private eventEmitter = new ChatEventEmitter();
  private jtagClient: any = null;
  
  // State management
  private state: ChatWidgetState = {
    isConnected: false,
    connectionStatus: 'disconnected',
    messages: [],
    messageCache: new Map(),
    isTyping: false,
    typingUsers: [],
    unreadCount: 0,
    isLoading: false,
    hasMoreHistory: true
  };
  
  private config: ChatWidgetConfig;
  private currentRoomId: string = 'general';
  private currentUserId: string = 'user'; // Will be populated from session
  private currentPersona: string = 'Human'; // Default persona
  
  // Available rooms and personas
  private availableRooms: ChatRoom[] = [];
  private availablePersonas: string[] = ['Human', 'Claude', 'DevAssistant', 'ResearchBot'];
  
  // UI Elements
  private container?: HTMLElement;
  private roomSelector?: HTMLSelectElement;
  private personaSelector?: HTMLSelectElement;
  private messagesContainer?: HTMLElement;
  private messageInput?: HTMLInputElement;
  private sendButton?: HTMLButtonElement;

  constructor(config: Partial<ChatWidgetConfig> = {}) {
    super();
    this.attachShadow({ mode: 'open' });
    
    this.config = {
      autoConnect: true,
      defaultRoom: 'general',
      showUserList: true,
      messageLimit: 50,
      autoScroll: true,
      allowMarkdown: true,
      enableAI: true,
      theme: 'cyberpunk',
      showTimestamps: true,
      ...config
    };
  }

  async connectedCallback(): Promise<void> {
    console.log('💬 ChatWidget: Initializing with JTAG integration...');
    
    try {
      await this.createUI();
      
      if (this.config.autoConnect) {
        await this.connectToJTAG();
      }
      
      console.log('✅ ChatWidget: Initialization complete');
    } catch (error) {
      console.error('❌ ChatWidget: Initialization failed:', error);
      this.showError('Failed to initialize chat widget');
    }
  }

  /**
   * Create the complete UI with room selection, persona selection, and messaging
   */
  private async createUI(): Promise<void> {
    if (!this.shadowRoot) return;
    this.shadowRoot.innerHTML = `
      <style>
        :host {
          display: block;
          font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', roboto, sans-serif;
        }
        
        .chat-widget {
          background: linear-gradient(135deg, rgba(15, 20, 25, 0.95), rgba(20, 25, 35, 0.9));
          border: 1px solid rgba(0, 212, 255, 0.3);
          border-radius: 12px;
          padding: 20px;
          height: 500px;
          display: flex;
          flex-direction: column;
          backdrop-filter: blur(20px);
          box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
        }
        
        .chat-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          margin-bottom: 15px;
          padding-bottom: 10px;
          border-bottom: 1px solid rgba(255, 255, 255, 0.1);
        }
        
        .chat-title {
          color: #00d4ff;
          font-weight: 600;
          font-size: 1.1rem;
        }
        
        .chat-controls {
          display: flex;
          gap: 10px;
          align-items: center;
        }
        
        .room-selector, .persona-selector {
          background: rgba(0, 0, 0, 0.3);
          border: 1px solid rgba(0, 212, 255, 0.2);
          border-radius: 6px;
          color: #e0e6ed;
          padding: 5px 8px;
          font-size: 0.85rem;
        }
        
        .connection-status {
          width: 8px;
          height: 8px;
          border-radius: 50%;
          margin-left: 8px;
        }
        
        .connection-status.connected {
          background: #00ff88;
          box-shadow: 0 0 6px #00ff88;
        }
        
        .connection-status.disconnected {
          background: #ff4444;
        }
        
        .messages-container {
          flex: 1;
          overflow-y: auto;
          padding: 10px 0;
          border-bottom: 1px solid rgba(255, 255, 255, 0.1);
          margin-bottom: 15px;
        }
        
        .message {
          margin-bottom: 12px;
          padding: 8px 12px;
          border-radius: 8px;
          background: rgba(255, 255, 255, 0.05);
          border-left: 3px solid transparent;
        }
        
        .message.user {
          border-left-color: #00d4ff;
        }
        
        .message.assistant {
          border-left-color: #00ff88;
          background: rgba(0, 255, 136, 0.05);
        }
        
        .message.system {
          border-left-color: #ffaa00;
          background: rgba(255, 170, 0, 0.05);
          font-style: italic;
        }
        
        .message-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          margin-bottom: 4px;
        }
        
        .message-author {
          font-weight: 600;
          font-size: 0.85rem;
        }
        
        .message-author.user { color: #00d4ff; }
        .message-author.assistant { color: #00ff88; }
        .message-author.system { color: #ffaa00; }
        
        .message-time {
          font-size: 0.75rem;
          color: #8a92a5;
        }
        
        .message-content {
          color: #e0e6ed;
          line-height: 1.4;
        }
        
        .input-container {
          display: flex;
          gap: 10px;
          align-items: center;
        }
        
        .message-input {
          flex: 1;
          background: rgba(0, 0, 0, 0.3);
          border: 1px solid rgba(0, 212, 255, 0.2);
          border-radius: 6px;
          padding: 8px 12px;
          color: #e0e6ed;
          font-size: 0.9rem;
        }
        
        .message-input::placeholder {
          color: #8a92a5;
        }
        
        .message-input:focus {
          outline: none;
          border-color: rgba(0, 212, 255, 0.5);
          box-shadow: 0 0 0 2px rgba(0, 212, 255, 0.1);
        }
        
        .send-button {
          background: linear-gradient(135deg, #00d4ff, #0099cc);
          border: none;
          border-radius: 6px;
          padding: 8px 16px;
          color: white;
          font-weight: 600;
          cursor: pointer;
          transition: all 0.2s ease;
        }
        
        .send-button:hover {
          background: linear-gradient(135deg, #00b8e6, #007799);
          transform: translateY(-1px);
        }
        
        .send-button:disabled {
          opacity: 0.5;
          cursor: not-allowed;
          transform: none;
        }
        
        .error-message {
          background: rgba(255, 68, 68, 0.1);
          border: 1px solid rgba(255, 68, 68, 0.3);
          border-radius: 6px;
          padding: 8px 12px;
          color: #ff6b6b;
          margin-bottom: 10px;
          font-size: 0.85rem;
        }
        
        .loading {
          text-align: center;
          color: #8a92a5;
          font-style: italic;
          padding: 20px;
        }
      </style>
      
      <div class="chat-widget">
        <div class="chat-header">
          <div class="chat-title">💬 Multi-Room Chat</div>
          <div class="chat-controls">
            <select class="room-selector">
              <option value="general">🏠 General</option>
              <option value="academy">🎓 Academy</option>
              <option value="dev">💻 Development</option>
              <option value="research">🔬 Research</option>
            </select>
            <select class="persona-selector">
              <option value="Human">👤 Human</option>
              <option value="Claude">🤖 Claude</option>
              <option value="DevAssistant">💻 DevAssistant</option>
              <option value="ResearchBot">🔬 ResearchBot</option>
            </select>
            <div class="connection-status disconnected"></div>
          </div>
        </div>
        
        <div class="messages-container">
          <div class="loading">Connecting to chat system...</div>
        </div>
        
        <div class="input-container">
          <input type="text" class="message-input" placeholder="Type your message..." disabled>
          <button class="send-button" disabled>Send</button>
        </div>
      </div>
    `;
    
    // Cache UI elements
    if (!this.shadowRoot) return;
    this.container = this.shadowRoot.querySelector('.chat-widget')!;
    this.roomSelector = this.shadowRoot.querySelector('.room-selector') as HTMLSelectElement;
    this.personaSelector = this.shadowRoot.querySelector('.persona-selector') as HTMLSelectElement;
    this.messagesContainer = this.shadowRoot.querySelector('.messages-container')!;
    this.messageInput = this.shadowRoot.querySelector('.message-input') as HTMLInputElement;
    this.sendButton = this.shadowRoot.querySelector('.send-button') as HTMLButtonElement;
    
    // Add event listeners
    this.setupEventListeners();
  }
  
  /**
   * Set up all UI event listeners
   */
  private setupEventListeners(): void {
    // Room selection
    this.roomSelector?.addEventListener('change', (e) => {
      const target = e.target as HTMLSelectElement;
      this.switchRoom(target.value);
    });
    
    // Persona selection
    this.personaSelector?.addEventListener('change', (e) => {
      const target = e.target as HTMLSelectElement;
      this.currentPersona = target.value;
      console.log(`💭 Switched to persona: ${this.currentPersona}`);
    });
    
    // Send message on Enter or button click
    this.messageInput?.addEventListener('keypress', (e) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        this.sendMessage();
      }
    });
    
    this.sendButton?.addEventListener('click', () => {
      this.sendMessage();
    });
  }
  
  /**
   * Connect to JTAG system and set up event subscriptions
   */
  private async connectToJTAG(): Promise<void> {
    try {
      console.log('🔌 ChatWidget: Connecting to JTAG system...');
      this.updateConnectionStatus('connecting');
      
      // Connect to JTAG system
      if (!window.jtag) {
        throw new Error('JTAG system not available');
      }
      
      const jtagSystem = await window.jtag.connect();
      this.jtagClient = jtagSystem.client;
      
      console.log('✅ ChatWidget: Connected to JTAG system');
      this.updateConnectionStatus('connected');
      
      // Initialize chat system
      await this.initializeChatSystem();
      
    } catch (error) {
      console.error('❌ ChatWidget: JTAG connection failed:', error);
      this.updateConnectionStatus('error');
      this.showError('Failed to connect to chat system');
    }
  }
  
  /**
   * Initialize the chat system - create default rooms, load history
   */
  private async initializeChatSystem(): Promise<void> {
    try {
      console.log('🏗️ ChatWidget: Initializing chat system...');
      
      // Ensure default rooms exist
      await this.ensureDefaultRoomsExist();
      
      // Load history for current room
      await this.loadRoomHistory(this.currentRoomId);
      
      // Set up event subscriptions for real-time updates
      await this.subscribeToRoomEvents();
      
      // Enable UI
      this.enableUI();
      
      console.log('✅ ChatWidget: Chat system initialized');
      
    } catch (error) {
      console.error('❌ ChatWidget: Chat system initialization failed:', error);
      this.showError('Failed to initialize chat system');
    }
  }
  
  /**
   * Ensure default rooms exist (General, Academy, etc.)
   */
  private async ensureDefaultRoomsExist(): Promise<void> {
    const defaultRooms = [
      { id: 'general', name: 'General', description: 'General discussion' },
      { id: 'academy', name: 'Academy', description: 'Learning and education' },
      { id: 'dev', name: 'Development', description: 'Software development topics' },
      { id: 'research', name: 'Research', description: 'Research and analysis' }
    ];
    
    for (const room of defaultRooms) {
      try {
        // Check if room exists via data/list
        const existingRooms = await this.jtagClient.commands['data/list']({
          collection: 'chat-rooms',
          filter: { id: room.id },
          limit: 1
        });
        
        if (!existingRooms.data || existingRooms.data.length === 0) {
          // Create room
          await this.jtagClient.commands['data/create']({
            collection: 'chat-rooms',
            data: {
              ...room,
              createdAt: new Date().toISOString(),
              participants: [],
              messageCount: 0
            }
          });
          console.log(`🏠 Created room: ${room.name}`);
        }
      } catch (error) {
        console.warn(`⚠️ Failed to ensure room exists: ${room.name}`, error);
      }
    }
  }
  
  /**
   * Load message history for a room
   */
  private async loadRoomHistory(roomId: string, limit: number = 50): Promise<void> {
    try {
      console.log(`📜 Loading history for room: ${roomId}`);
      this.state.isLoading = true;
      
      const historyResult = await this.jtagClient.commands['data/list']({
        collection: 'chat-messages',
        filter: { roomId },
        sort: { timestamp: -1 },
        limit
      });
      
      if (historyResult.success && historyResult.data) {
        const messages = historyResult.data.reverse(); // Show oldest first
        this.state.messages = messages.map(this.convertToUIMessage);
        this.renderMessages();
        console.log(`✅ Loaded ${messages.length} messages for room ${roomId}`);
      }
      
    } catch (error) {
      console.error(`❌ Failed to load room history for ${roomId}:`, error);
    } finally {
      this.state.isLoading = false;
    }
  }
  
  /**
   * Subscribe to room events for real-time updates
   * NOTE: This is simplified - real implementation would use proper JTAG event subscription
   */
  private async subscribeToRoomEvents(): Promise<void> {
    // TODO: Implement proper JTAG event subscription
    // For now, we'll rely on polling or manual refresh
    console.log('🔔 Event subscription setup (to be implemented with JTAG events)');
  }
  
  /**
   * Send a message using the proper chat/send-message command
   */
  private async sendMessage(): Promise<void> {
    const content = this.messageInput?.value.trim();
    if (!content || !this.jtagClient) return;
    
    try {
      console.log(`💬 Sending message to room ${this.currentRoomId} as ${this.currentPersona}`);
      
      // Clear input immediately for better UX
      this.messageInput!.value = '';
      this.sendButton!.disabled = true;
      
      // Send via proper JTAG command
      const result = await this.jtagClient.commands['chat/send-message']({
        roomId: this.currentRoomId,
        content,
        senderId: this.currentUserId,
        senderName: this.currentPersona,
        category: 'chat',
        timestamp: new Date().toISOString()
      });
      
      if (result.success) {
        console.log('✅ Message sent successfully');
        // Message will appear via event system or manual refresh
        await this.loadRoomHistory(this.currentRoomId, 10); // Refresh recent messages
      } else {
        throw new Error(result.error || 'Failed to send message');
      }
      
    } catch (error) {
      console.error('❌ Failed to send message:', error);
      this.showError('Failed to send message');
      // Restore message content on error
      if (content) {
        this.messageInput!.value = content;
      }
    } finally {
      this.sendButton!.disabled = false;
    }
  }
  
  /**
   * Switch to a different room
   */
  private async switchRoom(roomId: string): Promise<void> {
    if (roomId === this.currentRoomId) return;
    
    console.log(`🔄 Switching from ${this.currentRoomId} to ${roomId}`);
    this.currentRoomId = roomId;
    
    // Clear current messages
    this.state.messages = [];
    this.renderMessages();
    
    // Load new room history
    await this.loadRoomHistory(roomId);
  }
  
  /**
   * Convert database message to UI message format
   */
  private convertToUIMessage(dbMessage: any): ChatMessage {
    return {
      id: dbMessage.messageId || dbMessage.id,
      content: dbMessage.content,
      roomId: dbMessage.roomId,
      userId: dbMessage.senderId || dbMessage.userId,
      type: dbMessage.senderName?.includes('Assistant') || dbMessage.senderName?.includes('Claude') ? 'assistant' : 'user',
      timestamp: dbMessage.timestamp,
      metadata: {
        persona: dbMessage.senderName
      }
    };
  }
  
  /**
   * Render messages to UI
   */
  private renderMessages(): void {
    if (!this.messagesContainer) return;
    
    if (this.state.isLoading) {
      this.messagesContainer.innerHTML = '<div class="loading">Loading messages...</div>';
      return;
    }
    
    if (this.state.messages.length === 0) {
      this.messagesContainer.innerHTML = '<div class="loading">No messages yet. Start the conversation!</div>';
      return;
    }
    
    const messagesHTML = this.state.messages.map(message => {
      const time = formatMessageTime(message.timestamp);
      const authorName = message.metadata?.persona || message.userId;
      
      return `
        <div class="message ${message.type}">
          <div class="message-header">
            <span class="message-author ${message.type}">${authorName}</span>
            <span class="message-time">${time}</span>
          </div>
          <div class="message-content">${this.escapeHtml(message.content)}</div>
        </div>
      `;
    }).join('');
    
    this.messagesContainer.innerHTML = messagesHTML;
    
    // Auto-scroll to bottom
    if (this.config.autoScroll) {
      this.messagesContainer.scrollTop = this.messagesContainer.scrollHeight;
    }
  }
  
  /**
   * Update connection status indicator
   */
  private updateConnectionStatus(status: 'connecting' | 'connected' | 'disconnected' | 'error'): void {
    this.state.connectionStatus = status;
    const indicator = this.shadowRoot?.querySelector('.connection-status');
    if (indicator) {
      indicator.className = `connection-status ${status === 'connected' ? 'connected' : 'disconnected'}`;
    }
  }
  
  /**
   * Enable UI controls after successful connection
   */
  private enableUI(): void {
    if (this.messageInput) {
      this.messageInput.disabled = false;
      this.messageInput.placeholder = `Message ${this.currentRoomId} as ${this.currentPersona}...`;
    }
    if (this.sendButton) {
      this.sendButton.disabled = false;
    }
  }
  
  /**
   * Show error message to user
   */
  private showError(message: string): void {
    const existingError = this.shadowRoot?.querySelector('.error-message');
    if (existingError) {
      existingError.remove();
    }
    
    const errorDiv = document.createElement('div');
    errorDiv.className = 'error-message';
    errorDiv.textContent = message;
    
    const chatWidget = this.shadowRoot?.querySelector('.chat-widget');
    if (chatWidget) {
      chatWidget.insertBefore(errorDiv, chatWidget.firstChild);
      
      // Auto-remove after 5 seconds
      setTimeout(() => {
        if (errorDiv.parentNode) {
          errorDiv.remove();
        }
      }, 5000);
    }
  }
  
  /**
   * Escape HTML to prevent XSS
   */
  private escapeHtml(text: string): string {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
  }
}

// Register the custom element
customElements.define('chat-widget', ChatWidgetBrowser);