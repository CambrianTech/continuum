/**
 * Room List Widget - Navigation for chat rooms with tab integration
 * 
 * Architecture:
 * - Uses named CSS variables from theme system
 * - Event-driven room switching and tab highlighting
 * - Shows unread counts and last messages
 * - Supports room creation and management
 * - Integrates with tab system for "open content" highlighting
 */

import { 
  ChatRoom, 
  ChatModuleEvents,
  ChatModuleEventType,
  DEFAULT_CHAT_CONFIG,
  createChatRoom
} from '../../shared/ChatModuleTypes';

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

interface RoomListState {
  currentRoomId: string | null;
  rooms: ChatRoom[];
  isLoading: boolean;
  error: string | null;
}

export class RoomListWidget extends HTMLElement {
  private jtagClient: any = null;
  private state: RoomListState = {
    currentRoomId: null,
    rooms: [],
    isLoading: false,
    error: null
  };
  
  private eventListeners: Map<string, Function> = new Map();
  
  // UI Elements
  private roomListContainer?: HTMLElement;
  private headerTitle?: HTMLElement;
  private roomCountBadge?: HTMLElement;
  private addRoomButton?: HTMLButtonElement;
  private loadingIndicator?: HTMLElement;

  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
  }

  async connectedCallback(): Promise<void> {
    console.log('🏠 RoomListWidget: Initializing...');
    
    try {
      // Load themed styles
      await this.loadThemedStyles();
      
      // Create UI structure
      this.createUI();
      this.setupEventListeners();
      this.cacheUIElements();
      
      // Connect to JTAG system
      await this.connectToJTAG();
      
      // Load initial rooms
      await this.loadRooms();
      
      console.log('✅ RoomListWidget: Initialized successfully');
      
    } catch (error) {
      console.error('❌ RoomListWidget: Initialization failed:', error);
      this.renderError(`Room list initialization failed: ${error}`);
    }
  }

  disconnectedCallback(): void {
    this.cleanup();
  }

  /**
   * Load themed CSS with named variables
   */
  private async loadThemedStyles(): Promise<void> {
    if (!this.shadowRoot) return;
    
    const style = document.createElement('style');
    style.textContent = `
      :host {
        display: block;
        font-family: var(--font-primary, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif);
        background: var(--widget-background, rgba(15, 20, 25, 0.95));
        border: 1px solid var(--widget-border, rgba(0, 212, 255, 0.3));
        border-radius: var(--radius-lg, 12px);
        overflow: hidden;
        backdrop-filter: blur(10px);
        min-width: 240px;
        max-width: 280px;
      }
      
      .room-list-container {
        display: flex;
        flex-direction: column;
        height: 100%;
        background: var(--surface-primary, rgba(20, 25, 35, 0.9));
      }
      
      .room-list-header {
        padding: var(--spacing-md, 12px);
        background: var(--surface-secondary, rgba(30, 35, 45, 0.8));
        border-bottom: 1px solid var(--border-primary, rgba(255, 255, 255, 0.1));
        display: flex;
        align-items: center;
        gap: var(--spacing-sm, 8px);
      }
      
      .header-icon {
        font-size: 16px;
        color: var(--content-accent, #00d4ff);
      }
      
      .header-title {
        flex: 1;
        color: var(--content-primary, #e0e6ed);
        font-weight: 600;
        font-size: 13px;
        text-transform: uppercase;
        letter-spacing: 0.5px;
      }
      
      .room-count-badge {
        background: var(--content-accent, #00d4ff);
        color: var(--surface-primary, #000);
        border-radius: 50%;
        width: 18px;
        height: 18px;
        display: flex;
        align-items: center;
        justify-content: center;
        font-size: 10px;
        font-weight: bold;
      }
      
      .add-room-button {
        background: none;
        border: none;
        color: var(--content-secondary, #8a92a5);
        cursor: pointer;
        font-size: 14px;
        padding: 4px;
        border-radius: var(--radius-sm, 4px);
        transition: all 0.2s ease;
        display: flex;
        align-items: center;
        justify-content: center;
      }
      
      .add-room-button:hover {
        color: var(--content-accent, #00d4ff);
        background: var(--surface-hover, rgba(255, 255, 255, 0.05));
        transform: scale(1.1);
      }
      
      .room-list {
        flex: 1;
        overflow-y: auto;
        padding: var(--spacing-sm, 8px);
      }
      
      .room-item {
        display: flex;
        align-items: center;
        gap: var(--spacing-md, 12px);
        padding: var(--spacing-sm, 8px) var(--spacing-md, 12px);
        border-radius: var(--radius-md, 6px);
        cursor: pointer;
        transition: all 0.2s ease;
        margin-bottom: var(--spacing-xs, 4px);
        position: relative;
        border-left: 3px solid transparent;
      }
      
      .room-item:hover {
        background: var(--surface-hover, rgba(255, 255, 255, 0.05));
        transform: translateX(2px);
      }
      
      .room-item.current {
        background: var(--surface-selected, rgba(0, 212, 255, 0.15));
        border-left-color: var(--content-accent, #00d4ff);
        box-shadow: inset 0 0 0 1px var(--content-accent, rgba(0, 212, 255, 0.3));
      }
      
      .room-item.open-tab {
        background: var(--surface-open-tab, rgba(0, 212, 255, 0.08));
        border-left-color: var(--content-accent, rgba(0, 212, 255, 0.6));
      }
      
      .room-item.has-unread {
        border-left-color: var(--warning-color, #ffaa00);
      }
      
      .room-item.has-unread:not(.current) {
        background: var(--surface-unread, rgba(255, 170, 0, 0.05));
      }
      
      .room-icon {
        width: 24px;
        height: 24px;
        border-radius: var(--radius-sm, 4px);
        background: var(--surface-secondary, rgba(30, 35, 45, 0.8));
        display: flex;
        align-items: center;
        justify-content: center;
        font-size: 14px;
        flex-shrink: 0;
        position: relative;
      }
      
      .room-icon.general { background: var(--room-general-color, rgba(0, 212, 255, 0.2)); }
      .room-icon.academy { background: var(--room-academy-color, rgba(138, 43, 226, 0.2)); }
      .room-icon.dev { background: var(--room-dev-color, rgba(255, 69, 0, 0.2)); }
      .room-icon.research { background: var(--room-research-color, rgba(50, 205, 50, 0.2)); }
      .room-icon.grid { background: var(--room-grid-color, rgba(255, 20, 147, 0.2)); }
      
      .room-info {
        flex: 1;
        min-width: 0; /* Allow text truncation */
      }
      
      .room-name {
        color: var(--content-primary, #e0e6ed);
        font-weight: 600;
        font-size: 13px;
        margin-bottom: 2px;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
      }
      
      .room-item.current .room-name {
        color: var(--content-accent, #00d4ff);
      }
      
      .room-last-message {
        color: var(--content-secondary, #8a92a5);
        font-size: 10px;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
        max-width: 140px;
      }
      
      .room-meta {
        display: flex;
        flex-direction: column;
        align-items: flex-end;
        gap: 2px;
        flex-shrink: 0;
      }
      
      .room-time {
        color: var(--content-tertiary, #666);
        font-size: 9px;
      }
      
      .room-unread-badge {
        background: var(--warning-color, #ffaa00);
        color: var(--surface-primary, #000);
        border-radius: 10px;
        min-width: 16px;
        height: 16px;
        display: flex;
        align-items: center;
        justify-content: center;
        font-size: 10px;
        font-weight: bold;
        padding: 0 4px;
      }
      
      .room-participants-count {
        color: var(--content-tertiary, #666);
        font-size: 9px;
      }
      
      .tab-indicator {
        position: absolute;
        top: 2px;
        right: 2px;
        width: 8px;
        height: 8px;
        background: var(--content-accent, #00d4ff);
        border-radius: 50%;
        opacity: 0.8;
        animation: pulse 2s infinite;
      }
      
      @keyframes pulse {
        0%, 100% { opacity: 0.8; transform: scale(1); }
        50% { opacity: 1; transform: scale(1.1); }
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
        padding: var(--spacing-xl, 24px);
        text-align: center;
        color: var(--content-secondary, #8a92a5);
        font-size: 12px;
      }
      
      .empty-icon {
        font-size: 48px;
        margin-bottom: var(--spacing-md, 12px);
        opacity: 0.5;
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
      
      .room-tooltip {
        position: absolute;
        bottom: 100%;
        left: 50%;
        transform: translateX(-50%);
        background: var(--tooltip-background, rgba(0, 0, 0, 0.9));
        color: var(--tooltip-text, #fff);
        padding: var(--spacing-xs, 4px) var(--spacing-sm, 8px);
        border-radius: var(--radius-sm, 4px);
        font-size: 10px;
        white-space: nowrap;
        z-index: 1000;
        opacity: 0;
        pointer-events: none;
        transition: opacity 0.2s ease;
      }
      
      .room-item:hover .room-tooltip {
        opacity: 1;
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
    container.className = 'room-list-container';
    container.innerHTML = `
      <div class="room-list-header">
        <span class="header-icon">🏠</span>
        <span class="header-title">Chat Rooms</span>
        <span class="room-count-badge">0</span>
        <button class="add-room-button" title="Add new room">➕</button>
      </div>
      
      <div class="room-list" role="listbox" aria-label="Chat Rooms">
        <div class="loading-indicator">
          <div class="loading-spinner"></div>
          Loading rooms...
        </div>
      </div>
    `;
    
    this.shadowRoot.appendChild(container);
  }

  /**
   * Cache UI elements
   */
  private cacheUIElements(): void {
    if (!this.shadowRoot) return;
    
    this.roomListContainer = this.shadowRoot.querySelector('.room-list') as HTMLElement;
    this.headerTitle = this.shadowRoot.querySelector('.header-title') as HTMLElement;
    this.roomCountBadge = this.shadowRoot.querySelector('.room-count-badge') as HTMLElement;
    this.addRoomButton = this.shadowRoot.querySelector('.add-room-button') as HTMLButtonElement;
    this.loadingIndicator = this.shadowRoot.querySelector('.loading-indicator') as HTMLElement;
  }

  /**
   * Setup event listeners
   */
  private setupEventListeners(): void {
    if (!this.shadowRoot) return;
    
    // Room item clicks
    this.shadowRoot.addEventListener('click', (e) => {
      const target = e.target as HTMLElement;
      
      // Handle room item selection
      const roomItem = target.closest('.room-item') as HTMLElement;
      if (roomItem) {
        const roomId = roomItem.dataset.roomId;
        if (roomId) {
          this.handleRoomSelection(roomId);
        }
        return;
      }
      
      // Handle add room button
      if (target === this.addRoomButton || target.closest('.add-room-button')) {
        this.handleAddRoom();
        return;
      }
    });
    
    // Double-click to open room in new tab
    this.shadowRoot.addEventListener('dblclick', (e) => {
      const target = e.target as HTMLElement;
      const roomItem = target.closest('.room-item') as HTMLElement;
      if (roomItem) {
        const roomId = roomItem.dataset.roomId;
        if (roomId) {
          this.handleRoomTabOpen(roomId);
        }
      }
    });
  }

  /**
   * Connect to JTAG system
   */
  private async connectToJTAG(): Promise<void> {
    try {
      console.log('🔌 RoomListWidget: Connecting to JTAG...');
      
      const jtagSystem = await window.jtag.connect();
      this.jtagClient = jtagSystem.client;
      
      // Subscribe to relevant events
      this.setupJTAGEventListeners();
      
      console.log('✅ RoomListWidget: Connected to JTAG');
      
    } catch (error) {
      console.error('❌ RoomListWidget: JTAG connection failed:', error);
      throw error;
    }
  }

  /**
   * Setup JTAG event subscriptions
   */
  private setupJTAGEventListeners(): void {
    if (!this.jtagClient?.events) return;
    
    // Listen for room events
    const roomUpdatedHandler = (data: any) => this.handleRoomUpdated(data);
    const messageReceivedHandler = (data: any) => this.handleMessageReceived(data);
    const tabRoomHighlightedHandler = (data: any) => this.handleTabRoomHighlighted(data);
    const tabRoomClosedHandler = (data: any) => this.handleTabRoomClosed(data);
    
    this.jtagClient.events.on('room:updated', roomUpdatedHandler);
    this.jtagClient.events.on('message:received', messageReceivedHandler);
    this.jtagClient.events.on('tab:room-highlighted', tabRoomHighlightedHandler);
    this.jtagClient.events.on('tab:room-closed', tabRoomClosedHandler);
    
    // Store for cleanup
    this.eventListeners.set('room:updated', roomUpdatedHandler);
    this.eventListeners.set('message:received', messageReceivedHandler);
    this.eventListeners.set('tab:room-highlighted', tabRoomHighlightedHandler);
    this.eventListeners.set('tab:room-closed', tabRoomClosedHandler);
  }

  /**
   * Load rooms from system or create defaults
   */
  private async loadRooms(): Promise<void> {
    this.state.isLoading = true;
    
    try {
      // Try to load existing rooms
      const result = await this.jtagClient?.commands['data/list']?.({
        collection: 'chat-rooms',
        sort: { createdAt: 1 }
      });
      
      if (result?.success && result.items?.length > 0) {
        this.state.rooms = result.items;
      } else {
        // Create default rooms
        this.state.rooms = await this.createDefaultRooms();
      }
      
      this.state.isLoading = false;
      this.renderRooms();
      this.updateRoomCount();
      
    } catch (error) {
      console.warn('⚠️ RoomListWidget: Failed to load rooms, using defaults:', error);
      
      // Fallback to local defaults
      this.state.rooms = DEFAULT_CHAT_CONFIG.defaultRooms.map(roomName =>
        createChatRoom({ name: roomName })
      );
      this.state.isLoading = false;
      this.renderRooms();
      this.updateRoomCount();
    }
  }

  /**
   * Create default rooms in the system
   */
  private async createDefaultRooms(): Promise<ChatRoom[]> {
    const defaultRooms = DEFAULT_CHAT_CONFIG.defaultRooms;
    const rooms: ChatRoom[] = [];
    
    for (const roomName of defaultRooms) {
      try {
        const room = createChatRoom({ name: roomName });
        
        // Create in data system
        await this.jtagClient?.commands['data/create']?.({
          collection: 'chat-rooms',
          data: room
        });
        
        rooms.push(room);
        
      } catch (error) {
        console.warn(`⚠️ Failed to create room ${roomName}:`, error);
        // Add locally even if persistence failed
        rooms.push(createChatRoom({ name: roomName }));
      }
    }
    
    return rooms;
  }

  /**
   * Handle room selection
   */
  private handleRoomSelection(roomId: string): void {
    const room = this.state.rooms.find(r => r.id === roomId);
    if (!room) return;
    
    // Update current room
    this.state.currentRoomId = roomId;
    
    // Clear unread count for selected room
    if (room.unreadCount > 0) {
      room.unreadCount = 0;
      this.persistRoomChange(room);
    }
    
    // Re-render to update visual state
    this.renderRooms();
    
    // Emit event for other widgets
    this.emitModuleEvent('room:selected', { roomId, room });
    
    console.log('🏠 RoomListWidget: Room selected:', room.displayName);
  }

  /**
   * Handle room tab opening
   */
  private handleRoomTabOpen(roomId: string): void {
    const room = this.state.rooms.find(r => r.id === roomId);
    if (!room) return;
    
    // Mark room as open tab
    room.isOpen = true;
    this.persistRoomChange(room);
    
    // Re-render to show tab indicator
    this.renderRooms();
    
    // Emit event for tab system
    this.emitModuleEvent('room:opened', { roomId, room });
    
    console.log('📑 RoomListWidget: Room opened in tab:', room.displayName);
  }

  /**
   * Handle add room button
   */
  private async handleAddRoom(): Promise<void> {
    const roomName = prompt('Enter room name:');
    if (!roomName?.trim()) return;
    
    try {
      const room = createChatRoom({ name: roomName.trim() });
      
      // Create in data system
      await this.jtagClient?.commands['data/create']?.({
        collection: 'chat-rooms',
        data: room
      });
      
      // Add to local state
      this.state.rooms.push(room);
      this.renderRooms();
      this.updateRoomCount();
      
      console.log('➕ RoomListWidget: Room created:', room.displayName);
      
    } catch (error) {
      console.error('❌ Failed to create room:', error);
      alert('Failed to create room. Please try again.');
    }
  }

  /**
   * Handle room updated event
   */
  private handleRoomUpdated(data: ChatModuleEvents['room:updated']): void {
    const roomIndex = this.state.rooms.findIndex(r => r.id === data.roomId);
    if (roomIndex === -1) return;
    
    this.state.rooms[roomIndex] = { ...this.state.rooms[roomIndex], ...data.room };
    this.renderRooms();
  }

  /**
   * Handle message received event
   */
  private handleMessageReceived(data: ChatModuleEvents['message:received']): void {
    const room = this.state.rooms.find(r => r.id === data.roomId);
    if (!room) return;
    
    // Update last message
    room.lastMessage = {
      content: data.message.content,
      timestamp: data.message.timestamp,
      sender: data.message.senderName
    };
    
    // Increment unread count if not current room
    if (this.state.currentRoomId !== data.roomId) {
      room.unreadCount = (room.unreadCount || 0) + 1;
    }
    
    // Update message count
    room.messageCount = (room.messageCount || 0) + 1;
    
    this.renderRooms();
  }

  /**
   * Handle tab room highlighted event
   */
  private handleTabRoomHighlighted(data: ChatModuleEvents['tab:room-highlighted']): void {
    // Update all rooms to clear previous highlights, then set new one
    this.state.rooms.forEach(room => {
      room.isOpen = room.id === data.roomId;
    });
    
    this.renderRooms();
  }

  /**
   * Handle tab room closed event
   */
  private handleTabRoomClosed(data: ChatModuleEvents['tab:room-closed']): void {
    const room = this.state.rooms.find(r => r.id === data.roomId);
    if (!room) return;
    
    room.isOpen = false;
    this.renderRooms();
  }

  /**
   * Render rooms list
   */
  private renderRooms(): void {
    if (!this.roomListContainer) return;
    
    if (this.state.isLoading) {
      return; // Keep loading indicator
    }
    
    if (this.state.rooms.length === 0) {
      this.renderEmptyState();
      return;
    }
    
    const roomIcons: Record<string, string> = {
      general: '💬',
      academy: '🎓', 
      dev: '⚡',
      research: '🔬',
      grid: '🌐'
    };
    
    this.roomListContainer.innerHTML = this.state.rooms.map(room => {
      const icon = roomIcons[room.id] || '📁';
      const isCurrentRoom = this.state.currentRoomId === room.id;
      const hasUnread = (room.unreadCount || 0) > 0;
      const lastMessageTime = room.lastMessage?.timestamp 
        ? new Date(room.lastMessage.timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
        : '';
        
      let classNames = ['room-item'];
      if (isCurrentRoom) classNames.push('current');
      if (room.isOpen) classNames.push('open-tab');
      if (hasUnread) classNames.push('has-unread');
      
      return `
        <div class="${classNames.join(' ')}" data-room-id="${room.id}" role="option" aria-selected="${isCurrentRoom}">
          <div class="room-icon ${room.id}">
            ${icon}
          </div>
          <div class="room-info">
            <div class="room-name">${this.escapeHtml(room.displayName)}</div>
            <div class="room-last-message">${room.lastMessage ? this.escapeHtml(room.lastMessage.content) : 'No messages yet'}</div>
          </div>
          <div class="room-meta">
            ${lastMessageTime ? `<div class="room-time">${lastMessageTime}</div>` : ''}
            ${hasUnread ? `<div class="room-unread-badge">${room.unreadCount}</div>` : ''}
            <div class="room-participants-count">${room.participants.length} users</div>
          </div>
          ${room.isOpen ? '<div class="tab-indicator"></div>' : ''}
          <div class="room-tooltip">${this.escapeHtml(room.description)}</div>
        </div>
      `;
    }).join('');
  }

  /**
   * Render empty state
   */
  private renderEmptyState(): void {
    if (!this.roomListContainer) return;
    
    this.roomListContainer.innerHTML = `
      <div class="empty-state">
        <div class="empty-icon">🏠</div>
        <div>No rooms available</div>
        <div style="margin-top: 8px; font-size: 10px;">Click ➕ to create a room</div>
      </div>
    `;
  }

  /**
   * Update room count badge
   */
  private updateRoomCount(): void {
    if (this.roomCountBadge) {
      this.roomCountBadge.textContent = String(this.state.rooms.length);
    }
  }

  /**
   * Persist room changes to data system
   */
  private async persistRoomChange(room: ChatRoom): Promise<void> {
    try {
      await this.jtagClient?.commands['data/update']?.({
        collection: 'chat-rooms',
        filter: { id: room.id },
        data: room
      });
    } catch (error) {
      console.warn('⚠️ RoomListWidget: Failed to persist room change:', error);
    }
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
    
    console.log('🧹 RoomListWidget: Cleaned up');
  }
}

// Registration handled by generator system in browser/generated.ts