/**
 * User List Widget - Database-Driven Chat Users
 * Properly located in chat directory structure
 */

import { ChatWidgetBase } from '../shared/ChatWidgetBase';

import type { BaseUser } from '../../../api/types/User';
import { COLLECTIONS } from '../../../api/data-seed/SeedConstants';

interface DatabaseItem {
  id: string;
  collection: string;
  data: BaseUser;
  metadata: {
    createdAt: string;
    updatedAt: string;
    version: number;
  };
}

interface UserListResult {
  success: boolean;
  items?: DatabaseItem[];
}

export class UserListWidget extends ChatWidgetBase {
  private users: BaseUser[] = [];

  constructor() {
    super({
      widgetName: 'UserListWidget',
      template: 'user-list-widget.html',
      styles: 'user-list.css',
      enableAI: false,
      enableDatabase: true,
      enableRouterEvents: false,
      enableScreenshots: false
    });
  }

  protected override resolveResourcePath(filename: string): string {
      // Extract widget directory name from widget name (ChatWidget -> chat)
      //const widgetDir = this.config.widgetName.toLowerCase().replace('widget', '');
      // Return relative path from current working directory
      return `widgets/chat/user-list/${filename}`;
    }

  async onWidgetInitialize(): Promise<void> {
    await this.loadUsersFromDatabase();
  }

  private async loadUsersFromDatabase(): Promise<void> {
    const result = await this.jtagOperation<UserListResult>('data/list', {
      collection: COLLECTIONS.USERS,
      sort: { lastActiveAt: -1 },
      limit: 100
    });
    
    if (!result?.success || !result.items?.length) {
      console.warn('⚠️ No users found in database - using empty list');
      this.users = [];
      return;
    }
    
    // Extract user data from the nested structure
    this.users = result.items.map(item => item.data).filter(user => user?.id);
    console.log(`✅ UserListWidget: Loaded ${this.users.length} users from database`);
  }

  protected async renderWidget(): Promise<void> {
    const styles = this.templateCSS || '/* No styles loaded */';
    
    this.shadowRoot.innerHTML = `
      <style>${styles}</style>
      ${this.renderUserListHTML()}
    `;
  }

  protected async onWidgetCleanup(): Promise<void> {
    this.users = [];
  }

  private renderUserListHTML(): string {
    return `
      <div class="user-list-container">
        <div class="user-list-header">
          <h3>USERS & AGENTS</h3>
          <span class="user-count">${this.users.length}</span>
        </div>
        <div class="user-search">
          <input type="text" class="search-input" placeholder="Search users..." />
        </div>
        <div class="user-list">
          ${this.users.map(user => this.renderUserItem(user)).join('')}
        </div>
      </div>
    `;
  }

  private renderUserItem(user: BaseUser): string {
    const statusClass = user.isAuthenticated ? 'online' : 'offline';
    const avatar = user.userType === 'human' ? '👤' : '🤖';
    const displayName = user.userType === 'human' && user.name === 'human' ? 'joel' : user.name;
    
    return `
      <div class="user-item ${statusClass}" data-user-id="${user.id}">
        <span class="user-avatar">${avatar}</span>
        <div class="user-info">
          <div class="user-name">${displayName}</div>
          <div class="user-type">${user.userType}</div>
        </div>
        <div class="user-status">
          <span class="status-indicator"></span>
        </div>
      </div>
    `;
  }
}