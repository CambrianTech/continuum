/**
 * Chat Widget Browser - Coordinated messaging within selected room
 * 
 * Architecture:
 * - Works with Room List Widget for room selection
 * - Works with User List Widget for participant display  
 * - Uses named CSS variables from theme system
 * - Event-driven real-time message updates
 * - Subscribes to room-specific message events
 * - Coordinates with tab system for room highlighting
 */

import { 
  ChatMessage, 
  ChatRoom,
  ChatUser,
  ChatModuleEvents,
  ChatModuleEventType,
  DEFAULT_CHAT_CONFIG
} from '../../shared/ChatModuleTypes';
import { JTAGClient } from '../../../../system/core/client/shared/JTAGClient';

// JTAG client interface
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

interface ChatWidgetState {
  currentRoomId: string | null;
  currentRoom: ChatRoom | null;
  messages: ChatMessage[];
  isLoading: boolean;
  isConnected: boolean;
  selectedUser: ChatUser | null;
  currentPersona: string;
  availablePersonas: string[];
}

export class ChatWidgetBrowser extends HTMLElement {
  private jtagClient: any = null;
  private state: ChatWidgetState = {
    currentRoomId: null,
    currentRoom: null,
    messages: [],
    isLoading: false,
    isConnected: false,
    selectedUser: null,
    currentPersona: 'Human',
    availablePersonas: ['Human', 'Claude', 'DevAssistant', 'ResearchBot']
  };
  
  private eventListeners: Map<string, Function> = new Map();
  
  // UI Elements
  private messagesContainer?: HTMLElement;
  private messageInput?: HTMLTextAreaElement;
  private sendButton?: HTMLButtonElement;
  private headerRoomName?: HTMLElement;
  private headerPersonaSelector?: HTMLSelectElement;
  private headerStatus?: HTMLElement;
  private loadingIndicator?: HTMLElement;

  constructor() {
    super();
    console.log('🔧 CLAUDE-DEBUG-' + Date.now() + ': ChatWidgetBrowser constructor - fixed architecture deployed');
    this.attachShadow({ mode: 'open' });
  }

  async connectedCallback(): Promise<void> {
    console.log('💬 ChatWidgetBrowser: Initializing...');
    
    try {
      // Load themed styles
      await this.loadThemedStyles();
      
      // Create UI structure
      this.createUI();
      this.setupEventListeners();
      this.cacheUIElements();
      
      // Connect to JTAG system
      await this.connectToJTAG();
      
      console.log('✅ ChatWidgetBrowser: Initialized successfully');
      
    } catch (error) {
      console.error('❌ ChatWidgetBrowser: Initialization failed:', error);
      this.renderError(`Chat widget initialization failed: ${error}`);
    }
  }

  disconnectedCallback(): void {
    this.cleanup();
  }

  /**
   * Load themed CSS with named variables for great visual design
   */
  private async loadThemedStyles(): Promise<void> {
    if (!this.shadowRoot) return;
    
    const style = document.createElement('style');
    style.textContent = `
      :host {
        display: flex;
        flex-direction: column;
        height: 100%;
        font-family: var(--font-primary, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif);
        background: var(--widget-background, rgba(15, 20, 25, 0.95));
        border: 1px solid var(--widget-border, rgba(0, 212, 255, 0.3));
        border-radius: var(--radius-lg, 12px);
        overflow: hidden;
        backdrop-filter: blur(10px);
        box-shadow: var(--shadow-lg, 0 8px 32px rgba(0, 0, 0, 0.3));
      }
      
      .chat-container {
        display: flex;
        flex-direction: column;
        height: 100%;
        background: var(--surface-primary, rgba(20, 25, 35, 0.9));
      }
      
      .chat-header {
        padding: var(--spacing-md, 12px);
        background: var(--surface-secondary, rgba(30, 35, 45, 0.8));
        border-bottom: 1px solid var(--border-primary, rgba(255, 255, 255, 0.1));
        display: flex;
        align-items: center;
        gap: var(--spacing-md, 12px);
        flex-shrink: 0;
      }
      
      .header-icon {
        font-size: 18px;
        color: var(--content-accent, #00d4ff);
      }
      
      .header-room-name {
        flex: 1;
        color: var(--content-primary, #e0e6ed);
        font-weight: 600;
        font-size: 14px;
      }
      
      .header-room-name.no-room {
        color: var(--content-secondary, #8a92a5);
        font-style: italic;
      }
      
      .persona-selector {
        background: var(--input-background, rgba(40, 45, 55, 0.8));
        border: 1px solid var(--input-border, rgba(255, 255, 255, 0.15));
        border-radius: var(--radius-sm, 4px);
        color: var(--content-primary, #ffffff);
        padding: var(--spacing-xs, 4px) var(--spacing-sm, 8px);
        font-size: 11px;
        min-width: 90px;
        cursor: pointer;
      }
      
      .persona-selector:focus {
        border-color: var(--input-border-focus, rgba(0, 212, 255, 0.5));
        outline: none;
        box-shadow: 0 0 0 2px var(--input-focus-shadow, rgba(0, 212, 255, 0.2));
      }
      
      .header-status {
        font-size: 10px;
        padding: 2px 6px;
        border-radius: 10px;
        font-weight: bold;
      }
      
      .status-connected {
        background: var(--success-color, #00ff88);
        color: var(--surface-primary, #000);
      }
      
      .status-disconnected {
        background: var(--error-color, #ff4444);
        color: var(--content-primary, #fff);
      }
      
      .messages-container {
        flex: 1;
        overflow-y: auto;
        padding: var(--spacing-md, 12px);
        scrollbar-width: thin;
        scrollbar-color: var(--scrollbar-thumb, rgba(255, 255, 255, 0.2)) var(--scrollbar-track, transparent);
      }
      
      .messages-container::-webkit-scrollbar {
        width: 8px;
      }
      
      .messages-container::-webkit-scrollbar-track {
        background: transparent;
      }
      
      .messages-container::-webkit-scrollbar-thumb {
        background: var(--scrollbar-thumb, rgba(255, 255, 255, 0.2));
        border-radius: 4px;
      }
      
      .messages-container::-webkit-scrollbar-thumb:hover {
        background: var(--scrollbar-thumb-hover, rgba(255, 255, 255, 0.3));
      }
      
      .message {
        margin-bottom: var(--spacing-md, 12px);
        display: flex;
        gap: var(--spacing-sm, 8px);
        opacity: 0;
        animation: messageSlideIn 0.3s ease-out forwards;
      }
      
      @keyframes messageSlideIn {
        from {
          opacity: 0;
          transform: translateY(10px);
        }
        to {
          opacity: 1;
          transform: translateY(0);
        }
      }
      
      .message-avatar {
        width: 32px;
        height: 32px;
        border-radius: 50%;
        background: var(--surface-secondary, rgba(30, 35, 45, 0.8));
        display: flex;
        align-items: center;
        justify-content: center;
        font-size: 14px;
        flex-shrink: 0;
        border: 2px solid transparent;
      }
      
      .message-avatar.human {
        border-color: var(--success-color, #00ff88);
      }
      
      .message-avatar.assistant {
        border-color: var(--content-accent, #00d4ff);
      }
      
      .message-avatar.system {
        border-color: var(--warning-color, #ffaa00);
      }
      
      .message-content {
        flex: 1;
        min-width: 0;
      }
      
      .message-header {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm, 8px);
        margin-bottom: var(--spacing-xs, 4px);
      }
      
      .message-sender {
        color: var(--content-accent, #00d4ff);
        font-weight: 600;
        font-size: 12px;
      }
      
      .message-sender.human {
        color: var(--success-color, #00ff88);
      }
      
      .message-sender.system {
        color: var(--warning-color, #ffaa00);
      }
      
      .message-time {
        color: var(--content-secondary, #8a92a5);
        font-size: 10px;
      }
      
      .message-text {
        color: var(--content-primary, #e0e6ed);
        font-size: 13px;
        line-height: 1.4;
        word-wrap: break-word;
        white-space: pre-wrap;
      }
      
      .message-status {
        color: var(--content-secondary, #8a92a5);
        font-size: 9px;
        margin-top: var(--spacing-xs, 4px);
      }
      
      .status-sending {
        color: var(--warning-color, #ffaa00);
      }
      
      .status-error {
        color: var(--error-color, #ff4444);
      }
      
      .input-container {
        padding: var(--spacing-md, 12px);
        background: var(--surface-secondary, rgba(30, 35, 45, 0.8));
        border-top: 1px solid var(--border-primary, rgba(255, 255, 255, 0.1));
        display: flex;
        gap: var(--spacing-sm, 8px);
        align-items: flex-end;
        flex-shrink: 0;
      }
      
      .message-input {
        flex: 1;
        background: var(--input-background, rgba(40, 45, 55, 0.8));
        border: 1px solid var(--input-border, rgba(255, 255, 255, 0.15));
        border-radius: var(--radius-md, 6px);
        color: var(--content-primary, #ffffff);
        padding: var(--spacing-sm, 8px);
        font-size: 13px;
        font-family: inherit;
        resize: vertical;
        min-height: 36px;
        max-height: 120px;
        transition: all 0.2s ease;
      }
      
      .message-input:focus {
        border-color: var(--input-border-focus, rgba(0, 212, 255, 0.5));
        outline: none;
        box-shadow: 0 0 0 2px var(--input-focus-shadow, rgba(0, 212, 255, 0.2));
      }
      
      .message-input::placeholder {
        color: var(--content-secondary, #8a92a5);
      }
      
      .message-input:disabled {
        opacity: 0.5;
        cursor: not-allowed;
      }
      
      .send-button {
        background: var(--button-primary-background, linear-gradient(135deg, #00d4ff, #0099cc));
        border: none;
        border-radius: var(--radius-md, 6px);
        color: var(--button-primary-text, #000000);
        padding: var(--spacing-sm, 8px) var(--spacing-md, 12px);
        font-weight: 600;
        cursor: pointer;
        transition: all 0.2s ease;
        min-width: 60px;
        height: 36px;
        display: flex;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-xs, 4px);
      }
      
      .send-button:hover:not(:disabled) {
        background: var(--button-primary-background-hover, linear-gradient(135deg, #00b8e6, #0088bb));
        transform: translateY(-1px);
        box-shadow: var(--shadow-md, 0 4px 12px rgba(0, 212, 255, 0.3));
      }
      
      .send-button:active:not(:disabled) {
        background: var(--button-primary-background-active, linear-gradient(135deg, #009acc, #0077aa));
        transform: translateY(0);
      }
      
      .send-button:disabled {
        opacity: 0.4;
        cursor: not-allowed;
        transform: none;
      }
      
      .loading-indicator {
        display: flex;
        align-items: center;
        justify-content: center;
        padding: var(--spacing-xl, 24px);
        color: var(--content-secondary, #8a92a5);
        font-size: 12px;
      }
      
      .loading-spinner {
        width: 16px;
        height: 16px;
        border: 2px solid var(--content-secondary, #8a92a5);
        border-top: 2px solid var(--content-accent, #00d4ff);
        border-radius: 50%;
        animation: spin 1s linear infinite;
        margin-right: var(--spacing-sm, 8px);
      }
      
      @keyframes spin {
        0% { transform: rotate(0deg); }
        100% { transform: rotate(360deg); }
      }
      
      .empty-state {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        flex: 1;
        padding: var(--spacing-xl, 24px);
        text-align: center;
        color: var(--content-secondary, #8a92a5);
        font-size: 13px;
      }
      
      .empty-icon {
        font-size: 64px;
        margin-bottom: var(--spacing-md, 12px);
        opacity: 0.5;
      }
      
      .empty-title {
        font-weight: 600;
        margin-bottom: var(--spacing-xs, 4px);
        color: var(--content-primary, #e0e6ed);
      }
      
      .empty-subtitle {
        font-size: 11px;
        opacity: 0.8;
      }
      
      .error-message {
        color: var(--error-color, #ff4444);
        padding: var(--spacing-md, 12px);
        text-align: center;
        font-size: 12px;
        border: 1px solid var(--error-color, rgba(255, 68, 68, 0.3));
        border-radius: var(--radius-md, 6px);
        background: var(--error-background, rgba(255, 68, 68, 0.05));
        margin: var(--spacing-sm, 8px);
      }
    `;
    
    this.shadowRoot.appendChild(style);
  }

  /**
   * Create UI structure
   */
  private createUI(): void {
    if (!this.shadowRoot) return;
    
    const container = document.createElement('div');
    container.className = 'chat-container';
    container.innerHTML = `
      <div class="chat-header">
        <span class="header-icon">💬</span>
        <div class="header-room-name no-room">Select a room to start chatting</div>
        <select class="persona-selector">
          <option value="Human">Human</option>
          <option value="Claude">Claude</option>
          <option value="DevAssistant">DevAssistant</option>
          <option value="ResearchBot">ResearchBot</option>
        </select>
        <div class="header-status status-disconnected">OFFLINE</div>
      </div>
      
      <div class="messages-container">
        <div class="empty-state">
          <div class="empty-icon">💬</div>
          <div class="empty-title">Welcome to Chat</div>
          <div class="empty-subtitle">Select a room from the room list to start messaging</div>
        </div>
      </div>
      
      <div class="input-container">
        <textarea 
          class="message-input" 
          placeholder="Type your message..."
          rows="1"
          disabled
        ></textarea>
        <button class="send-button" disabled>
          <span>Send</span>
        </button>
      </div>
    `;
    
    this.shadowRoot.appendChild(container);
  }

  /**
   * Cache UI elements
   */
  private cacheUIElements(): void {
    if (!this.shadowRoot) return;
    
    this.messagesContainer = this.shadowRoot.querySelector('.messages-container') as HTMLElement;
    this.messageInput = this.shadowRoot.querySelector('.message-input') as HTMLTextAreaElement;
    this.sendButton = this.shadowRoot.querySelector('.send-button') as HTMLButtonElement;
    this.headerRoomName = this.shadowRoot.querySelector('.header-room-name') as HTMLElement;
    this.headerPersonaSelector = this.shadowRoot.querySelector('.persona-selector') as HTMLSelectElement;
    this.headerStatus = this.shadowRoot.querySelector('.header-status') as HTMLElement;
    this.loadingIndicator = this.shadowRoot.querySelector('.loading-indicator') as HTMLElement;
  }

  /**
   * Setup event listeners
   */
  private setupEventListeners(): void {
    if (!this.shadowRoot) return;
    
    // Send button click
    this.shadowRoot.addEventListener('click', (e) => {
      const target = e.target as HTMLElement;
      
      if (target === this.sendButton || target.closest('.send-button')) {
        this.handleSendMessage();
      }
    });
    
    // Enter key to send (Shift+Enter for newline)
    this.shadowRoot.addEventListener('keydown', (e) => {
      const keyboardEvent = e as KeyboardEvent;
      if (e.target === this.messageInput) {
        if (keyboardEvent.key === 'Enter' && !keyboardEvent.shiftKey) {
          e.preventDefault();
          this.handleSendMessage();
        }
      }
    });
    
    // Persona selection
    this.shadowRoot.addEventListener('change', (e) => {
      if (e.target === this.headerPersonaSelector) {
        this.handlePersonaChange(this.headerPersonaSelector.value);
      }
    });
    
    // Auto-resize textarea
    this.shadowRoot.addEventListener('input', (e) => {
      if (e.target === this.messageInput) {
        this.autoResizeInput();
      }
    });
  }

  /**
   * Connect to JTAG system through WidgetDaemon
   */
  private async connectToJTAG(): Promise<void> {
    try {
      console.log('🔌 ChatWidgetBrowser: Connecting to JTAG through WidgetDaemon...');
      
      // The widget should use WidgetDaemon which provides executeCommand() like CommandBase.remoteExecute()
      const widgetDaemon = (window as any).widgetDaemon;
      if (!widgetDaemon) {
        throw new Error('WidgetDaemon not available on window object');
      }
      
      if (!widgetDaemon.executeCommand) {
        throw new Error('WidgetDaemon missing executeCommand method');
      }
      
      // Store reference to daemon for command execution
      this.jtagClient = widgetDaemon;
      this.state.isConnected = true;
      
      // Update UI
      this.updateConnectionStatus();
      
      // Subscribe to relevant events (if supported)
      this.setupJTAGEventListeners();
      
      console.log('✅ ChatWidgetBrowser: Connected to JTAG via WidgetDaemon');
      
    } catch (error) {
      console.error('❌ ChatWidgetBrowser: JTAG connection failed:', error);
      this.state.isConnected = false;
      this.updateConnectionStatus();
      throw error;
    }
  }

  /**
   * Setup JTAG event subscriptions
   */
  private setupJTAGEventListeners(): void {
    if (!this.jtagClient?.events) return;
    
    // Listen for room selection from Room List Widget
    const roomSelectedHandler = (data: any) => this.handleRoomSelected(data);
    const messageReceivedHandler = (data: any) => this.handleMessageReceived(data);
    const userSelectedHandler = (data: any) => this.handleUserSelected(data);
    
    this.jtagClient.events.on('room:selected', roomSelectedHandler);
    this.jtagClient.events.on('message:received', messageReceivedHandler);
    this.jtagClient.events.on('user:selected', userSelectedHandler);
    
    // Store for cleanup
    this.eventListeners.set('room:selected', roomSelectedHandler);
    this.eventListeners.set('message:received', messageReceivedHandler);
    this.eventListeners.set('user:selected', userSelectedHandler);
  }

  /**
   * Handle room selected event from Room List Widget
   */
  private async handleRoomSelected(data: ChatModuleEvents['room:selected']): Promise<void> {
    console.log('🏠 ChatWidgetBrowser: Room selected:', data.room.displayName);
    
    this.state.currentRoomId = data.roomId;
    this.state.currentRoom = data.room;
    
    // Update UI
    this.updateRoomDisplay();
    this.enableMessageInput();
    
    // Load room messages
    await this.loadRoomMessages(data.roomId);
    
    // Subscribe to room-specific message events  
    this.subscribeToRoomEvents(data.roomId);
  }

  /**
   * Handle message received event
   */
  private handleMessageReceived(data: ChatModuleEvents['message:received']): void {
    // Only handle messages for current room
    if (data.roomId !== this.state.currentRoomId) return;
    
    // Add message to state
    this.state.messages.push(data.message);
    
    // Re-render messages
    this.renderMessages();
    
    // Auto-scroll to bottom
    this.scrollToBottom();
  }

  /**
   * Handle user selected event from User List Widget
   */
  private handleUserSelected(data: ChatModuleEvents['user:selected']): void {
    this.state.selectedUser = data.user;
    console.log('👤 ChatWidgetBrowser: User selected:', data.user.name);
  }

  /**
   * Handle persona change
   */
  private handlePersonaChange(persona: string): void {
    this.state.currentPersona = persona;
    console.log('🎭 ChatWidgetBrowser: Persona changed to:', persona);
  }

  /**
   * Handle send message
   */
  private async handleSendMessage(): Promise<void> {
    if (!this.messageInput || !this.state.currentRoomId) return;
    
    const content = this.messageInput.value.trim();
    if (!content) return;
    
    // Create optimistic message
    const tempMessage: ChatMessage = {
      id: `temp_${Date.now()}`,
      content,
      roomId: this.state.currentRoomId,
      senderId: 'current-user',
      senderName: this.state.currentPersona,
      timestamp: new Date().toISOString(),
      type: this.state.currentPersona === 'Human' ? 'user' : 'assistant',
      status: 'sending'
    };
    
    // Add to UI immediately (optimistic)
    this.state.messages.push(tempMessage);
    this.renderMessages();
    this.scrollToBottom();
    
    // Clear input
    this.messageInput.value = '';
    this.autoResizeInput();
    
    try {
      // Send via WidgetDaemon.executeCommand() - same pattern as CommandBase.remoteExecute()
      const result = await this.jtagClient.executeCommand('chat/send-message', {
        roomId: this.state.currentRoomId,
        content,
        sessionId: 'current-user',
        senderName: this.state.currentPersona,
        metadata: {
          persona: this.state.currentPersona
        }
      });
      
      if (result?.success) {
        // Update temp message with real data
        const messageIndex = this.state.messages.findIndex(m => m.id === tempMessage.id);
        if (messageIndex !== -1) {
          this.state.messages[messageIndex] = {
            ...tempMessage,
            id: result.messageId || tempMessage.id,
            status: 'sent'
          };
          this.renderMessages();
        }
        
        console.log('✅ ChatWidgetBrowser: Message sent successfully');
        
      } else {
        throw new Error(result?.error || 'Failed to send message');
      }
      
    } catch (error) {
      console.error('❌ ChatWidgetBrowser: Failed to send message:', error);
      
      // Update temp message to show error
      const messageIndex = this.state.messages.findIndex(m => m.id === tempMessage.id);
      if (messageIndex !== -1) {
        this.state.messages[messageIndex].status = 'error';
        this.renderMessages();
      }
    }
  }

  /**
   * Load messages for selected room
   */
  private async loadRoomMessages(roomId: string): Promise<void> {
    this.state.isLoading = true;
    this.renderLoadingState();
    
    try {
      const result = await this.jtagClient.executeCommand('data/list', {
        collection: 'chat_messages',
        filter: { roomId },
        sort: { timestamp: 1 },
        limit: 50
      });
      
      if (result?.success) {
        this.state.messages = result.items || [];
      } else {
        this.state.messages = [];
      }
      
      this.state.isLoading = false;
      this.renderMessages();
      this.scrollToBottom();
      
    } catch (error) {
      console.error('❌ ChatWidgetBrowser: Failed to load messages:', error);
      this.state.isLoading = false;
      this.state.messages = [];
      this.renderMessages();
    }
  }

  /**
   * Subscribe to room-specific events
   */
  private subscribeToRoomEvents(roomId: string): void {
    // This would typically subscribe to room-specific message streams
    // For now, we rely on the global message:received events filtered by roomId
    console.log('📡 ChatWidgetBrowser: Subscribed to room events for:', roomId);
  }

  /**
   * Update room display in header
   */
  private updateRoomDisplay(): void {
    if (!this.headerRoomName || !this.state.currentRoom) return;
    
    this.headerRoomName.textContent = this.state.currentRoom.displayName;
    this.headerRoomName.classList.remove('no-room');
  }

  /**
   * Update connection status display
   */
  private updateConnectionStatus(): void {
    if (!this.headerStatus) return;
    
    if (this.state.isConnected) {
      this.headerStatus.textContent = 'ONLINE';
      this.headerStatus.className = 'header-status status-connected';
    } else {
      this.headerStatus.textContent = 'OFFLINE';
      this.headerStatus.className = 'header-status status-disconnected';
    }
  }

  /**
   * Enable message input when room is selected
   */
  private enableMessageInput(): void {
    if (this.messageInput) {
      this.messageInput.disabled = false;
      this.messageInput.placeholder = `Message ${this.state.currentRoom?.displayName || 'room'}...`;
    }
    
    if (this.sendButton) {
      this.sendButton.disabled = false;
    }
  }

  /**
   * Render messages list
   */
  private renderMessages(): void {
    if (!this.messagesContainer) return;
    
    if (this.state.messages.length === 0) {
      this.renderEmptyState();
      return;
    }
    
    const getMessageAvatar = (message: ChatMessage): string => {
      if (message.type === 'system') return '⚙️';
      if (message.metadata?.persona === 'Claude') return '🤖';
      if (message.metadata?.persona === 'DevAssistant') return '💻';
      if (message.metadata?.persona === 'ResearchBot') return '🔬';
      return '👤';
    };
    
    const getStatusText = (status: ChatMessage['status']): string => {
      switch (status) {
        case 'sending': return '⏳ Sending...';
        case 'sent': return '✓ Sent';
        case 'delivered': return '✓✓ Delivered';
        case 'error': return '❌ Failed';
        default: return '';
      }
    };
    
    this.messagesContainer.innerHTML = this.state.messages.map(message => {
      const avatar = getMessageAvatar(message);
      const time = new Date(message.timestamp).toLocaleTimeString([], { 
        hour: '2-digit', minute: '2-digit' 
      });
      const statusText = getStatusText(message.status);
      
      return `
        <div class="message" data-message-id="${message.id}">
          <div class="message-avatar ${message.type}">
            ${avatar}
          </div>
          <div class="message-content">
            <div class="message-header">
              <span class="message-sender ${message.type}">${this.escapeHtml(message.senderName)}</span>
              <span class="message-time">${time}</span>
            </div>
            <div class="message-text">${this.escapeHtml(message.content)}</div>
            ${statusText ? `<div class="message-status status-${message.status}">${statusText}</div>` : ''}
          </div>
        </div>
      `;
    }).join('');
  }

  /**
   * Render loading state
   */
  private renderLoadingState(): void {
    if (!this.messagesContainer) return;
    
    this.messagesContainer.innerHTML = `
      <div class="loading-indicator">
        <div class="loading-spinner"></div>
        Loading messages...
      </div>
    `;
  }

  /**
   * Render empty state
   */
  private renderEmptyState(): void {
    if (!this.messagesContainer) return;
    
    const roomName = this.state.currentRoom?.displayName || 'room';
    
    this.messagesContainer.innerHTML = `
      <div class="empty-state">
        <div class="empty-icon">💬</div>
        <div class="empty-title">Welcome to ${this.escapeHtml(roomName)}</div>
        <div class="empty-subtitle">Start a conversation by typing a message below</div>
      </div>
    `;
  }

  /**
   * Auto-resize textarea input
   */
  private autoResizeInput(): void {
    if (!this.messageInput) return;
    
    // Reset height to calculate new height
    this.messageInput.style.height = 'auto';
    
    // Set new height based on scroll height
    const newHeight = Math.min(this.messageInput.scrollHeight, 120);
    this.messageInput.style.height = `${newHeight}px`;
  }

  /**
   * Scroll to bottom of messages
   */
  private scrollToBottom(): void {
    if (!this.messagesContainer) return;
    
    // Use setTimeout to ensure DOM has been updated
    setTimeout(() => {
      this.messagesContainer!.scrollTop = this.messagesContainer!.scrollHeight;
    }, 0);
  }

  /**
   * Emit module-level events for coordination with other widgets
   */
  private emitModuleEvent<T extends ChatModuleEventType>(eventType: T, data: ChatModuleEvents[T]): void {
    // Use JTAG event system for cross-widget communication
    this.jtagClient?.events?.emit?.(eventType, data);
    
    // Also emit as custom DOM event
    this.dispatchEvent(new CustomEvent(eventType, { 
      detail: data, 
      bubbles: true 
    }));
  }

  /**
   * Render error state
   */
  private renderError(message: string): void {
    if (!this.shadowRoot) return;
    
    this.shadowRoot.innerHTML = `
      <div class="error-message">
        ❌ ${this.escapeHtml(message)}
      </div>
    `;
  }

  /**
   * Escape HTML to prevent XSS
   */
  private escapeHtml(unsafe: string): string {
    return unsafe
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;")
      .replace(/'/g, "&#039;");
  }

  /**
   * Cleanup resources
   */
  private cleanup(): void {
    // Remove JTAG event listeners
    if (this.jtagClient?.events) {
      this.eventListeners.forEach((handler, eventType) => {
        this.jtagClient.events.off(eventType, handler);
      });
    }
    
    this.eventListeners.clear();
    
    console.log('🧹 ChatWidgetBrowser: Cleaned up');
  }
}