/**
 * User List Widget - Shows participants like the legacy user selector
 * 
 * Architecture:
 * - Uses named CSS variables from theme system
 * - Event-driven updates for user status changes
 * - Search/filter functionality
 * - Star/unstar users
 * - Real-time status updates
 */

import { 
  ChatUser, 
  ChatModuleEvents,
  ChatModuleEventType,
  DEFAULT_CHAT_CONFIG,
  createChatUser,
  formatLastActive,
  getUserStatusColor
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

export class UserListWidget extends HTMLElement {
  private jtagClient: any = null;
  private users: ChatUser[] = [];
  private filteredUsers: ChatUser[] = [];
  private searchQuery: string = '';
  private eventListeners: Map<string, Function> = new Map();
  
  // UI Elements
  private searchInput?: HTMLInputElement;
  private userListContainer?: HTMLElement;
  private headerTitle?: HTMLElement;
  private userCountBadge?: HTMLElement;

  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
  }

  async connectedCallback(): Promise<void> {
    console.log('👥 UserListWidget: Initializing...');
    
    try {
      // Load themed styles
      await this.loadThemedStyles();
      
      // Create UI structure
      this.createUI();
      this.setupEventListeners();
      this.cacheUIElements();
      
      // Connect to JTAG system
      await this.connectToJTAG();
      
      // Load initial users
      await this.loadUsers();
      
      console.log('✅ UserListWidget: Initialized successfully');
      
    } catch (error) {
      console.error('❌ UserListWidget: Initialization failed:', error);
      this.renderError(`User list initialization failed: ${error}`);
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
        min-width: 280px;
        max-width: 320px;
      }
      
      .user-list-container {
        display: flex;
        flex-direction: column;
        height: 100%;
        background: var(--surface-primary, rgba(20, 25, 35, 0.9));
      }
      
      .user-list-header {
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
      
      .user-count-badge {
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
      
      .search-container {
        padding: var(--spacing-sm, 8px) var(--spacing-md, 12px);
        border-bottom: 1px solid var(--border-primary, rgba(255, 255, 255, 0.1));
      }
      
      .search-input-wrapper {
        position: relative;
        display: flex;
        align-items: center;
      }
      
      .search-icon {
        position: absolute;
        left: var(--spacing-sm, 8px);
        color: var(--content-secondary, #8a92a5);
        font-size: 14px;
        pointer-events: none;
      }
      
      .search-input {
        width: 100%;
        background: var(--input-background, rgba(40, 45, 55, 0.8));
        border: 1px solid var(--input-border, rgba(255, 255, 255, 0.15));
        border-radius: var(--radius-md, 6px);
        color: var(--content-primary, #ffffff);
        padding: var(--spacing-sm, 8px) var(--spacing-sm, 8px) var(--spacing-sm, 8px) 32px;
        font-size: 12px;
        transition: all 0.2s ease;
      }
      
      .search-input:focus {
        border-color: var(--input-border-focus, rgba(0, 212, 255, 0.5));
        outline: none;
        box-shadow: 0 0 0 2px var(--input-focus-shadow, rgba(0, 212, 255, 0.2));
      }
      
      .search-input::placeholder {
        color: var(--content-secondary, #8a92a5);
      }
      
      .user-list {
        flex: 1;
        overflow-y: auto;
        padding: var(--spacing-sm, 8px);
      }
      
      .user-item {
        display: flex;
        align-items: center;
        gap: var(--spacing-md, 12px);
        padding: var(--spacing-sm, 8px);
        border-radius: var(--radius-md, 6px);
        cursor: pointer;
        transition: all 0.2s ease;
        margin-bottom: var(--spacing-xs, 4px);
        position: relative;
      }
      
      .user-item:hover {
        background: var(--surface-hover, rgba(255, 255, 255, 0.05));
        transform: translateX(2px);
      }
      
      .user-item.selected {
        background: var(--surface-selected, rgba(0, 212, 255, 0.15));
        border-left: 3px solid var(--content-accent, #00d4ff);
      }
      
      .user-avatar {
        width: 32px;
        height: 32px;
        border-radius: 50%;
        background: var(--surface-secondary, rgba(30, 35, 45, 0.8));
        display: flex;
        align-items: center;
        justify-content: center;
        font-size: 16px;
        position: relative;
        border: 2px solid transparent;
      }
      
      .user-avatar.online {
        border-color: var(--success-color, #00ff88);
      }
      
      .user-status-dot {
        position: absolute;
        bottom: -2px;
        right: -2px;
        width: 8px;
        height: 8px;
        border-radius: 50%;
        border: 2px solid var(--surface-primary, #000);
      }
      
      .status-online { background: var(--success-color, #00ff88); }
      .status-away { background: var(--warning-color, #ffaa00); }
      .status-busy { background: var(--error-color, #ff4444); }
      .status-offline { background: var(--content-secondary, #666666); }
      
      .user-info {
        flex: 1;
        min-width: 0; /* Allow text truncation */
      }
      
      .user-name {
        color: var(--content-primary, #e0e6ed);
        font-weight: 600;
        font-size: 13px;
        margin-bottom: 2px;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
      }
      
      .user-role {
        color: var(--content-secondary, #8a92a5);
        font-size: 10px;
        font-weight: 500;
        text-transform: uppercase;
        letter-spacing: 0.3px;
        margin-bottom: 2px;
      }
      
      .user-role.ai-assistant { color: var(--content-accent, #00d4ff); }
      .user-role.user { color: var(--success-color, #00ff88); }
      
      .user-last-active {
        color: var(--content-tertiary, #666);
        font-size: 9px;
      }
      
      .user-actions {
        display: flex;
        gap: var(--spacing-xs, 4px);
        opacity: 0;
        transition: opacity 0.2s ease;
      }
      
      .user-item:hover .user-actions {
        opacity: 1;
      }
      
      .star-button {
        background: none;
        border: none;
        color: var(--content-secondary, #8a92a5);
        cursor: pointer;
        font-size: 12px;
        padding: 2px;
        border-radius: 2px;
        transition: all 0.2s ease;
      }
      
      .star-button:hover {
        color: var(--warning-color, #ffaa00);
        background: var(--surface-hover, rgba(255, 255, 255, 0.05));
      }
      
      .star-button.starred {
        color: var(--warning-color, #ffaa00);
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
    container.className = 'user-list-container';
    container.innerHTML = `
      <div class="user-list-header">
        <span class="header-icon">👥</span>
        <span class="header-title">Users & Agents</span>
        <span class="user-count-badge">0</span>
      </div>
      
      <div class="search-container">
        <div class="search-input-wrapper">
          <span class="search-icon">🔍</span>
          <input type="text" class="search-input" placeholder="Search agents..." autocomplete="off">
        </div>
      </div>
      
      <div class="user-list" role="listbox" aria-label="Users and Agents">
        <!-- Users will be dynamically populated -->
      </div>
    `;
    
    this.shadowRoot.appendChild(container);
  }

  /**
   * Cache UI elements
   */
  private cacheUIElements(): void {
    if (!this.shadowRoot) return;
    
    this.searchInput = this.shadowRoot.querySelector('.search-input') as HTMLInputElement;
    this.userListContainer = this.shadowRoot.querySelector('.user-list') as HTMLElement;
    this.headerTitle = this.shadowRoot.querySelector('.header-title') as HTMLElement;
    this.userCountBadge = this.shadowRoot.querySelector('.user-count-badge') as HTMLElement;
  }

  /**
   * Setup event listeners
   */
  private setupEventListeners(): void {
    if (!this.shadowRoot) return;
    
    // Search input
    this.shadowRoot.addEventListener('input', (e) => {
      const target = e.target as HTMLInputElement;
      if (target === this.searchInput) {
        this.handleSearch(target.value);
      }
    });
    
    // User item clicks
    this.shadowRoot.addEventListener('click', (e) => {
      const target = e.target as HTMLElement;
      
      // Handle user item selection
      const userItem = target.closest('.user-item') as HTMLElement;
      if (userItem) {
        const userId = userItem.dataset.userId;
        if (userId) {
          this.handleUserSelection(userId);
        }
        return;
      }
      
      // Handle star button
      const starButton = target.closest('.star-button') as HTMLElement;
      if (starButton) {
        e.stopPropagation(); // Prevent user selection
        const userItem = starButton.closest('.user-item') as HTMLElement;
        const userId = userItem?.dataset.userId;
        if (userId) {
          this.handleUserStar(userId);
        }
        return;
      }
    });
  }

  /**
   * Connect to JTAG system
   */
  private async connectToJTAG(): Promise<void> {
    try {
      console.log('🔌 UserListWidget: Connecting to JTAG...');
      
      const jtagSystem = await window.jtag.connect();
      this.jtagClient = jtagSystem.client;
      
      // Subscribe to relevant events
      this.setupJTAGEventListeners();
      
      console.log('✅ UserListWidget: Connected to JTAG');
      
    } catch (error) {
      console.error('❌ UserListWidget: JTAG connection failed:', error);
      throw error;
    }
  }

  /**
   * Setup JTAG event subscriptions
   */
  private setupJTAGEventListeners(): void {
    if (!this.jtagClient?.events) return;
    
    // Listen for user events from other widgets
    const userJoinedHandler = (data: any) => this.handleUserJoined(data);
    const userLeftHandler = (data: any) => this.handleUserLeft(data);
    const userStatusChangedHandler = (data: any) => this.handleUserStatusChanged(data);
    
    this.jtagClient.events.on('user:joined', userJoinedHandler);
    this.jtagClient.events.on('user:left', userLeftHandler);
    this.jtagClient.events.on('user:status-changed', userStatusChangedHandler);
    
    // Store for cleanup
    this.eventListeners.set('user:joined', userJoinedHandler);
    this.eventListeners.set('user:left', userLeftHandler);
    this.eventListeners.set('user:status-changed', userStatusChangedHandler);
  }

  /**
   * Load users from system or use defaults
   */
  private async loadUsers(): Promise<void> {
    try {
      // Try to load users from data system
      const result = await this.jtagClient?.commands['data/list']?.({
        collection: 'chat-users',
        sort: { lastActive: -1 },
        limit: 100
      });
      
      if (result?.success && result.items?.length > 0) {
        this.users = result.items;
      } else {
        // Use default users from config
        this.users = DEFAULT_CHAT_CONFIG.defaultUsers.map(userData => 
          createChatUser(userData)
        );
      }
      
      this.filteredUsers = [...this.users];
      this.renderUsers();
      this.updateUserCount();
      
    } catch (error) {
      console.warn('⚠️ UserListWidget: Failed to load users, using defaults:', error);
      
      // Fallback to defaults
      this.users = DEFAULT_CHAT_CONFIG.defaultUsers.map(userData => 
        createChatUser(userData)
      );
      this.filteredUsers = [...this.users];
      this.renderUsers();
      this.updateUserCount();
    }
  }

  /**
   * Handle search input
   */
  private handleSearch(query: string): void {
    this.searchQuery = query.toLowerCase().trim();
    
    if (!this.searchQuery) {
      this.filteredUsers = [...this.users];
    } else {
      this.filteredUsers = this.users.filter(user => 
        user.name.toLowerCase().includes(this.searchQuery) ||
        user.role.toLowerCase().includes(this.searchQuery) ||
        user.metadata?.capabilities?.some(cap => 
          cap.toLowerCase().includes(this.searchQuery)
        )
      );
    }
    
    this.renderUsers();
    this.updateUserCount();
  }

  /**
   * Handle user selection
   */
  private handleUserSelection(userId: string): void {
    const user = this.users.find(u => u.id === userId);
    if (!user) return;
    
    // Update visual selection
    this.updateSelectedUser(userId);
    
    // Emit event for other widgets
    this.emitModuleEvent('user:selected', { userId, user });
    
    console.log('👤 UserListWidget: User selected:', user.name);
  }

  /**
   * Handle user star/unstar
   */
  private handleUserStar(userId: string): void {
    const user = this.users.find(u => u.id === userId);
    if (!user) return;
    
    user.isStarred = !user.isStarred;
    
    // Re-render to update star state
    this.renderUsers();
    
    // Persist change
    this.persistUserChange(user);
    
    console.log(`⭐ UserListWidget: User ${user.isStarred ? 'starred' : 'unstarred'}:`, user.name);
  }

  /**
   * Handle user joined event
   */
  private handleUserJoined(data: ChatModuleEvents['user:joined']): void {
    const existingUser = this.users.find(u => u.id === data.userId);
    if (existingUser) return; // Already exists
    
    this.users.push(data.user);
    this.applySearchFilter();
    this.renderUsers();
    this.updateUserCount();
  }

  /**
   * Handle user left event
   */
  private handleUserLeft(data: ChatModuleEvents['user:left']): void {
    this.users = this.users.filter(u => u.id !== data.userId);
    this.applySearchFilter();
    this.renderUsers();
    this.updateUserCount();
  }

  /**
   * Handle user status change event
   */
  private handleUserStatusChanged(data: ChatModuleEvents['user:status-changed']): void {
    const user = this.users.find(u => u.id === data.userId);
    if (!user) return;
    
    Object.assign(user, data.user);
    this.renderUsers();
  }

  /**
   * Render users list
   */
  private renderUsers(): void {
    if (!this.userListContainer) return;
    
    if (this.filteredUsers.length === 0) {
      this.renderEmptyState();
      return;
    }
    
    // Sort users: starred first, then by online status, then alphabetically
    const sortedUsers = [...this.filteredUsers].sort((a, b) => {
      if (a.isStarred !== b.isStarred) return a.isStarred ? -1 : 1;
      if (a.status !== b.status) {
        const statusOrder = { online: 0, away: 1, busy: 2, offline: 3 };
        return statusOrder[a.status] - statusOrder[b.status];
      }
      return a.name.localeCompare(b.name);
    });
    
    this.userListContainer.innerHTML = sortedUsers.map(user => `
      <div class="user-item" data-user-id="${user.id}" role="option" aria-selected="false">
        <div class="user-avatar ${user.status}">
          ${user.avatar}
          <div class="user-status-dot status-${user.status}"></div>
        </div>
        <div class="user-info">
          <div class="user-name">${this.escapeHtml(user.name)}</div>
          <div class="user-role ${user.metadata?.isAI ? 'ai-assistant' : 'user'}">${this.escapeHtml(user.role)}</div>
          <div class="user-last-active">Last active: ${this.escapeHtml(user.lastActive)}</div>
        </div>
        <div class="user-actions">
          <button class="star-button ${user.isStarred ? 'starred' : ''}" 
                  title="${user.isStarred ? 'Unstar' : 'Star'} ${user.name}">
            ${user.isStarred ? '⭐' : '☆'}
          </button>
        </div>
      </div>
    `).join('');
  }

  /**
   * Render empty state
   */
  private renderEmptyState(): void {
    if (!this.userListContainer) return;
    
    const message = this.searchQuery 
      ? `No users found matching "${this.searchQuery}"`
      : 'No users available';
      
    this.userListContainer.innerHTML = `
      <div class="empty-state">
        <div class="empty-icon">👥</div>
        <div>${this.escapeHtml(message)}</div>
      </div>
    `;
  }

  /**
   * Update selected user visual state
   */
  private updateSelectedUser(selectedUserId: string): void {
    if (!this.userListContainer) return;
    
    // Clear previous selection
    this.userListContainer.querySelectorAll('.user-item').forEach(item => {
      item.classList.remove('selected');
      item.setAttribute('aria-selected', 'false');
    });
    
    // Set new selection
    const selectedItem = this.userListContainer.querySelector(`[data-user-id="${selectedUserId}"]`);
    if (selectedItem) {
      selectedItem.classList.add('selected');
      selectedItem.setAttribute('aria-selected', 'true');
    }
  }

  /**
   * Update user count badge
   */
  private updateUserCount(): void {
    if (this.userCountBadge) {
      this.userCountBadge.textContent = String(this.filteredUsers.length);
    }
  }

  /**
   * Apply current search filter
   */
  private applySearchFilter(): void {
    this.handleSearch(this.searchQuery);
  }

  /**
   * Persist user changes to data system
   */
  private async persistUserChange(user: ChatUser): Promise<void> {
    try {
      await this.jtagClient?.commands['data/update']?.({
        collection: 'chat-users',
        filter: { id: user.id },
        data: user
      });
    } catch (error) {
      console.warn('⚠️ UserListWidget: Failed to persist user change:', error);
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
    
    console.log('🧹 UserListWidget: Cleaned up');
  }
}