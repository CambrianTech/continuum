/**
 * Users & Agents Widget - Display and manage AI agents and users
 * =============================================================
 * Shows search, user list with avatars, and agent management
 */

import { BaseWidget } from '../shared/BaseWidget';

interface User {
  id: string;
  name: string;
  type: 'USER' | 'AI';
  specialization?: string;
  status: 'active' | 'training' | 'graduated' | 'offline';
  lastActive: string;
  avatar: string;
  accuracy?: number;
}

export class UsersAgentsWidget extends BaseWidget {
  private users: User[] = [];
  private searchQuery: string = '';
  private selectedUser: string | null = null;

  constructor() {
    super();
    this.loadUsers();
  }

  static get widgetName(): string {
    return 'user-selector';
  }

  protected getOwnCSS(): string[] {
    return ['UsersAgents.css'];
  }

  protected renderOwnContent(): string {
    const filteredUsers = this.getFilteredUsers();
    
    return `
      <div class="users-agents-container">
        <div class="section-header">
          <span class="section-icon">👥</span>
          <span class="section-title">USERS & AGENTS</span>
          <button class="collapse-button" data-action="toggle">▼</button>
        </div>
        
        <div class="widget-content">
          <div class="search-container">
            <input 
              type="text" 
              class="search-input" 
              placeholder="Search agents..."
              value="${this.searchQuery}"
              data-action="search"
            />
            <span class="search-icon">🔍</span>
          </div>
          
          <div class="users-list">
            ${filteredUsers.map(user => this.renderUser(user)).join('')}
          </div>
          
          <div class="actions-footer">
            <button class="action-button" data-action="add-agent">+ Add Agent</button>
            <button class="action-button" data-action="manage">⚙️ Manage</button>
          </div>
        </div>
      </div>
    `;
  }

  private renderUser(user: User): string {
    const isSelected = user.id === this.selectedUser;
    const statusIcon = this.getStatusIcon(user.status);
    const userBadge = user.type === 'USER' ? 'USER' : 'AI';
    
    return `
      <div class="user-item ${isSelected ? 'selected' : ''}" data-user-id="${user.id}">
        <div class="user-avatar">
          <span class="avatar-icon">${user.avatar}</span>
          <span class="status-indicator status-${user.status}">${statusIcon}</span>
        </div>
        
        <div class="user-info">
          <div class="user-header">
            <span class="user-name">${user.name}</span>
            <span class="user-badge badge-${user.type.toLowerCase()}">${userBadge}</span>
          </div>
          
          <div class="user-details">
            <span class="user-specialization">${user.specialization || 'General'}</span>
            ${user.accuracy ? `<span class="user-accuracy">${user.accuracy}%</span>` : ''}
          </div>
          
          <div class="user-activity">
            <span class="last-active">Last active: ${this.formatTime(user.lastActive)}</span>
          </div>
        </div>
        
        <div class="user-actions">
          <button class="star-button" data-action="star" data-user-id="${user.id}">⭐</button>
          <button class="more-button" data-action="more" data-user-id="${user.id}">⋯</button>
        </div>
      </div>
    `;
  }

  private async loadUsers(): Promise<void> {
    try {
      const response = await this.executeCommand('personas', { action: 'list' });
      
      if (response?.personas) {
        this.users = response.personas.map((persona: any) => ({
          id: persona.id,
          name: persona.name,
          type: persona.id === 'joel' ? 'USER' : 'AI',
          specialization: persona.specialization,
          status: persona.status === 'active' ? 'active' : 'offline',
          lastActive: persona.lastUsed,
          avatar: persona.avatar || (persona.id === 'joel' ? '👤' : '🤖'),
          accuracy: persona.accuracy
        }));
      } else {
        this.loadMockUsers();
      }
      
      this.updateContent();
      
    } catch (error) {
      console.warn('👥 UsersAgents: Failed to load users, using mock data:', error);
      this.loadMockUsers();
      this.updateContent();
    }
  }

  private loadMockUsers(): void {
    this.users = [
      {
        id: 'claude-code',
        name: 'Claude Code',
        type: 'AI',
        specialization: 'AI Assistant',
        status: 'active',
        lastActive: '8:19:21 AM',
        avatar: '🤖',
        accuracy: 96.5
      },
      {
        id: 'joel',
        name: 'joel',
        type: 'USER',
        specialization: 'Project Owner',
        status: 'active',
        lastActive: '8:19:21 AM',
        avatar: '👤'
      },
      {
        id: 'auto-route',
        name: 'Auto Route',
        type: 'AI',
        specialization: 'Smart agent selection',
        status: 'active',
        lastActive: '8:18:45 AM',
        avatar: '🎯',
        accuracy: 94.2
      },
      {
        id: 'codeai',
        name: 'CodeAI',
        type: 'AI',
        specialization: 'Code analysis & debugging',
        status: 'training',
        lastActive: '8:17:30 AM',
        avatar: '🔧',
        accuracy: 97.8
      },
      {
        id: 'generalai',
        name: 'GeneralAI',
        type: 'AI',
        specialization: 'General assistance',
        status: 'active',
        lastActive: '8:16:15 AM',
        avatar: '🌟',
        accuracy: 95.1
      },
      {
        id: 'plannerai',
        name: 'PlannerAI',
        type: 'AI',
        specialization: 'Strategy & web commands',
        status: 'graduated',
        lastActive: '8:15:00 AM',
        avatar: '📋',
        accuracy: 93.6
      }
    ];
  }

  private getFilteredUsers(): User[] {
    if (!this.searchQuery) return this.users;
    
    const query = this.searchQuery.toLowerCase();
    return this.users.filter(user => 
      user.name.toLowerCase().includes(query) ||
      user.specialization?.toLowerCase().includes(query)
    );
  }

  private getStatusIcon(status: string): string {
    switch (status) {
      case 'active': return '🟢';
      case 'training': return '🟡';
      case 'graduated': return '🎓';
      case 'offline': return '⚪';
      default: return '❓';
    }
  }

  private formatTime(timeStr: string): string {
    // Simple time formatting - could be enhanced
    return timeStr;
  }

  protected setupEventListeners(): void {
    this.addEventListener('click', this.handleClick.bind(this));
    this.addEventListener('input', this.handleInput.bind(this));
    
    // Listen for persona updates
    this.notifySystem('personas_updated', () => {
      this.loadUsers();
    });
  }

  private handleClick(event: Event): void {
    const target = event.target as HTMLElement;
    const action = target.dataset.action;
    const userId = target.dataset.userId;
    
    switch (action) {
      case 'toggle':
        this.toggleCollapse();
        break;
      case 'star':
        this.starUser(userId!);
        break;
      case 'more':
        this.showUserMenu(userId!);
        break;
      case 'add-agent':
        this.addAgent();
        break;
      case 'manage':
        this.openManagement();
        break;
      default:
        // Check if clicking on user item
        const userItem = target.closest('.user-item') as HTMLElement;
        if (userItem) {
          this.selectUser(userItem.dataset.userId!);
        }
    }
  }

  private handleInput(event: Event): void {
    const target = event.target as HTMLInputElement;
    if (target.dataset.action === 'search') {
      this.searchQuery = target.value;
      this.updateUsersList();
    }
  }

  public toggleCollapse(): void {
    const content = this.shadowRoot?.querySelector('.widget-content') as HTMLElement;
    const button = this.shadowRoot?.querySelector('.collapse-button') as HTMLElement;
    
    if (content && button) {
      const isCollapsed = content.style.display === 'none';
      content.style.display = isCollapsed ? 'block' : 'none';
      button.textContent = isCollapsed ? '▼' : '▲';
    }
  }

  private selectUser(userId: string): void {
    this.selectedUser = userId;
    this.updateUsersList();
    
    // Emit user selection event
    this.dispatchEvent(new CustomEvent('user-selected', {
      detail: { userId },
      bubbles: true
    }));
  }

  private starUser(userId: string): void {
    console.log(`👥 Starring user: ${userId}`);
    // TODO: Implement starring functionality
  }

  private showUserMenu(userId: string): void {
    console.log(`👥 Showing menu for user: ${userId}`);
    // TODO: Implement user context menu
  }

  private addAgent(): void {
    console.log('👥 Adding new agent');
    // TODO: Implement add agent functionality
  }

  private openManagement(): void {
    console.log('👥 Opening user management');
    // TODO: Implement management interface
  }

  private updateUsersList(): void {
    const usersList = this.shadowRoot?.querySelector('.users-list');
    if (usersList) {
      const filteredUsers = this.getFilteredUsers();
      usersList.innerHTML = filteredUsers.map(user => this.renderUser(user)).join('');
    }
  }

  private updateContent(): void {
    const container = this.shadowRoot?.querySelector('.users-agents-container');
    if (container) {
      container.innerHTML = this.renderOwnContent();
      this.setupEventListeners();
    }
  }
}

// Register the custom element
customElements.define('user-selector', UsersAgentsWidget);