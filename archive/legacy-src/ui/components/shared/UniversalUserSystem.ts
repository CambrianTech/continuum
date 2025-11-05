/**
 * Universal User System - ELEGANT & SIMPLE
 * Real users from database/shared storage via server API
 */

// ELEGANT: Simple inline types - no external dependencies
export interface UniversalUser {
  id: string;
  name: string;
  type: 'human' | 'persona' | 'ai-model' | 'ai-system';
  avatar: string;
  status: 'online' | 'offline' | 'thinking' | 'working' | 'idle';
  capabilities: string[];
  isClickable: boolean;
  currentTask?: string;
}

export class UniversalUserSystem {
  private users = new Map<string, UniversalUser>();
  private eventListeners = new Map<string, Function[]>();

  constructor() {
    // ELEGANT: Don't block constructor - load users async when ready
    this.loadUsersWhenReady();
  }

  private async loadUsersWhenReady(): Promise<void> {
    // ELEGANT: Wait for continuum API to be available, then load real users
    try {
      // Wait for continuum API
      while (!(globalThis as any).continuum) {
        await new Promise(resolve => setTimeout(resolve, 100));
      }
      
      const continuum = (globalThis as any).continuum;
      const response = await continuum.execute('users', {});
      
      if (response?.users && Array.isArray(response.users)) {
        console.log(`✅ Loaded ${response.users.length} real users from server`);
        response.users.forEach((user: UniversalUser) => {
          this.addUser(user);
        });
        return;
      }
    } catch (error) {
      console.warn('⚠️ Failed to load real users, using fallback:', error);
    }
    
    // ELEGANT: Minimal fallback - just current user
    this.addUser({
      id: 'current-user',
      name: 'User',
      type: 'human',
      avatar: '👤',
      status: 'online',
      capabilities: ['all'],
      isClickable: false
    });
  }

  // ELEGANT: Simple user management
  addUser(user: UniversalUser): void {
    this.users.set(user.id, user);
    this.notifyUserAdded(user);
  }

  updateUser(userId: string, updates: Partial<UniversalUser>): void {
    const user = this.users.get(userId);
    if (user) {
      Object.assign(user, updates);
      this.notifyUserUpdated(user);
    }
  }

  getUser(userId: string): UniversalUser | null {
    return this.users.get(userId) || null;
  }

  getAllUsers(): UniversalUser[] {
    return Array.from(this.users.values());
  }

  getClickableUsers(): UniversalUser[] {
    return Array.from(this.users.values()).filter(u => u.isClickable);
  }

  // ELEGANT: Simple event system
  on(event: string, callback: Function): void {
    if (!this.eventListeners.has(event)) {
      this.eventListeners.set(event, []);
    }
    this.eventListeners.get(event)!.push(callback);
  }

  private emit(event: string, data: any): void {
    const listeners = this.eventListeners.get(event) || [];
    listeners.forEach(callback => callback(data));
  }

  private notifyUserAdded(user: UniversalUser): void {
    this.emit('user:added', user);
  }

  private notifyUserUpdated(user: UniversalUser): void {
    this.emit('user:updated', user);
  }

  // ELEGANT: Handle user clicks for widget interactions
  handleUserClick(userId: string, options: any = {}): void {
    const user = this.getUser(userId);
    if (user) {
      this.emit('user:clicked', { user, options });
      console.log(`🎛️ User clicked: ${user.name}`, options);
      
      // If user is clickable, perform default action
      if (user.isClickable) {
        // Could trigger chat, collaboration, or context switching
        this.emit('user:interact', { user, action: 'select', options });
      }
    } else {
      console.warn(`⚠️ User not found: ${userId}`);
    }
  }

  // ELEGANT: Simple HTML generation for UI
  generateConnectedUsersHTML(): string {
    const users = this.getAllUsers();
    return users.map(user => `
      <div class="user-badge ${user.isClickable ? 'clickable' : ''} ${user.type}" data-user-id="${user.id}">
        <span class="user-avatar">${user.avatar}</span>
        <span class="user-name">${user.name}</span>
        <span class="user-status ${user.status}"></span>
        ${user.currentTask ? `<span class="user-task">${user.currentTask}</span>` : ''}
      </div>
    `).join('');
  }
}

// Global singleton instance
export const universalUserSystem = new UniversalUserSystem();