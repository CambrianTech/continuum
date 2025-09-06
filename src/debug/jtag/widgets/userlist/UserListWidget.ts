/**
 * User List Widget - Simple BaseWidget for sidebar user display
 * 
 * Uses BaseWidget architecture with template/styles system.
 * Shows list of users in current room.
 */

import { BaseWidget } from '../shared/BaseWidget';

export class UserListWidget extends BaseWidget {
  private currentRoomId: string = 'general';
  private users: Array<{id: string, name: string, status: string, role: string}> = [];
  
  constructor() {
    super({
      widgetName: 'UserListWidget',
      template: 'user-list-widget.html',
      styles: 'user-list-widget.css',
      enableAI: false,
      enableDatabase: true,
      enableRouterEvents: true,
      enableScreenshots: false
    });
  }

  protected async onWidgetInitialize(): Promise<void> {
    console.log('👥 UserListWidget: Initializing...');
    
    // Load users from data system or use defaults
    await this.loadUsers();
    
    // Listen for room changes
    this.setupRoomListener();
    
    console.log('✅ UserListWidget: Initialized');
  }

  protected async renderWidget(): Promise<void> {
    const styles = this.templateCSS || '/* No styles loaded */';
    const template = this.templateHTML || '<div>No template loaded</div>';
    
    const templateString = typeof template === 'string' ? template : '<div>Template error</div>';
    
    // Replace dynamic content
    const dynamicContent = templateString.replace(
      '<!-- USER_LIST_CONTENT -->', 
      this.renderUserList()
    );

    this.shadowRoot!.innerHTML = `
      <style>${styles}</style>
      ${dynamicContent}
    `;
    
    // Setup event listeners
    this.setupEventListeners();
  }

  private async loadUsers(): Promise<void> {
    try {
      // Use BaseWidget data methods to load users
      const savedUsers = await this.getData(`room_users_${this.currentRoomId}`, []);
      
      if (savedUsers.length === 0) {
        // Create default users
        this.users = [
          { id: 'user1', name: 'You', status: 'online', role: 'USER' },
          { id: 'ai1', name: 'AI Assistant', status: 'online', role: 'AI Assistant' }
        ];
        await this.storeData(`room_users_${this.currentRoomId}`, this.users, { persistent: true });
      } else {
        this.users = savedUsers;
      }
    } catch (error) {
      console.error('❌ UserListWidget: Failed to load users:', error);
      // Fallback to defaults
      this.users = [
        { id: 'user1', name: 'You', status: 'online', role: 'USER' },
        { id: 'ai1', name: 'AI Assistant', status: 'online', role: 'AI Assistant' }
      ];
    }
  }

  private setupRoomListener(): void {
    // Listen for room selection events from other widgets
    this.addEventListener('room:selected', ((event: CustomEvent) => {
      const { roomId } = event.detail;
      this.handleRoomChange(roomId);
    }) as EventListener);
  }

  private async handleRoomChange(roomId: string): Promise<void> {
    if (this.currentRoomId !== roomId) {
      this.currentRoomId = roomId;
      await this.loadUsers();
      await this.renderWidget();
    }
  }

  private renderUserList(): string {
    if (this.users.length === 0) {
      return '<div class="empty-state">No users in this room</div>';
    }

    return this.users.map(user => {
      const statusIcon = user.status === 'online' ? '🟢' : '🔴';
      const roleIcon = user.role === 'AI Assistant' ? '🤖' : '👤';
      
      return `
        <div class="user-item" data-user-id="${user.id}">
          <span class="user-icon">${roleIcon}</span>
          <div class="user-info">
            <span class="user-name">${user.name}</span>
            <span class="user-role">${user.role}</span>
          </div>
          <span class="user-status">${statusIcon}</span>
        </div>
      `;
    }).join('');
  }

  private setupEventListeners(): void {
    this.shadowRoot?.addEventListener('click', (e) => {
      const target = e.target as HTMLElement;
      const userItem = target.closest('.user-item') as HTMLElement;
      
      if (userItem) {
        const userId = userItem.dataset.userId;
        if (userId) {
          this.selectUser(userId);
        }
      }
    });
  }

  private async selectUser(userId: string): Promise<void> {
    // Broadcast event using BaseWidget methods
    await this.broadcastEvent('user:selected', { userId });
    
    console.log('👥 UserListWidget: Selected user:', userId);
  }

  protected async onWidgetCleanup(): Promise<void> {
    console.log('🧹 UserListWidget: Cleanup complete');
  }
}

// Register the custom element
customElements.define('user-list-widget', UserListWidget);