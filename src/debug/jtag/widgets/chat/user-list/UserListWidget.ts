/**
 * User List Widget - Database-Driven Chat Users
 * Properly located in chat directory structure
 */

import { ChatWidgetBase } from '../shared/ChatWidgetBase';
import { JTAGClient } from '../../../system/core/client/shared/JTAGClient';
import type { UserData } from '../../../system/data/domains/User';
import type { DataListParams, DataListResult } from '../../../commands/data/list/shared/DataListTypes';
import { COLLECTIONS } from '../../../system/data/core/FieldMapping';

export class UserListWidget extends ChatWidgetBase {
  private users: UserData[] = [];

  constructor() {
    super({
      widgetName: 'UserListWidget',
      // No template specified - use renderTemplate() method instead
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
    const client = await JTAGClient.sharedInstance;
    const result = await this.executeCommand<DataListParams, DataListResult<UserData>>('data/list', {
      context: client.context,
      sessionId: client.sessionId,
      collection: COLLECTIONS.USERS,
      orderBy: [{ field: 'lastActiveAt', direction: 'desc' }],
      limit: 100
    });

    // FAIL FAST: Don't allow silent failures with optional chaining
    if (!result) {
      throw new Error('UserListWidget: Database command returned null - system failure');
    }

    if (!result.success) {
      throw new Error(`UserListWidget: Database command failed: ${result.error || 'Unknown error'}`);
    }

    if (!result.items) {
      throw new Error('UserListWidget: Database returned no items array - data structure error');
    }

    if (result.items.length === 0) {
      throw new Error('UserListWidget: No users found in database - check data seeding');
    }

    // Validate required fields - no optional chaining
    const validUsers = result.items.filter((user: UserData) => {
      if (!user) {
        console.error('❌ UserListWidget: Null user in database results');
        return false;
      }
      if (!user.id) {
        console.error('❌ UserListWidget: User missing required id:', user);
        return false;
      }
      if (!user.profile?.displayName) {
        console.error('❌ UserListWidget: User missing required profile.displayName:', user);
        return false;
      }
      return true;
    });

    if (validUsers.length === 0) {
      throw new Error('UserListWidget: No valid users found - all users missing required fields');
    }

    this.users = validUsers;
    console.log(`✅ UserListWidget: Loaded ${this.users.length} valid users from database`);
    console.log(`   Users: ${this.users.map(u => u.profile.displayName).join(', ')}`);
  }

  protected override async renderWidget(): Promise<void> {
    // Use parent's getReplacements pattern like other widgets
    await super.renderWidget();
  }

  protected async onWidgetCleanup(): Promise<void> {
    this.users = [];
  }

  protected renderTemplate(): string {
    return `
      <!-- User List Widget - Sidebar navigation for users and agents -->
      <div class="user-list-container">
        <div class="user-list-header">
          <span class="header-title">Users & Agents</span>
          <span class="user-count">${this.users.length}</span>
        </div>

        <div class="user-list">
          ${this.renderUserListHTML()}
        </div>
      </div>
    `;
  }

  private renderUserListHTML(): string {
    // GRACEFUL EMPTY STATE: Show "no content" template instead of failing
    if (!this.users || this.users.length === 0) {
      return `
        <div class="no-users-message">
          <span class="no-content-icon">👤</span>
          <p class="no-content-text">No users available</p>
          <small class="no-content-hint">Check your data seeding or invite some users</small>
        </div>
      `;
    }

    // REQUIRED ROW FUNCTION: Map each user using validated row rendering
    return this.users.map((user, index) => {
      try {
        return this.renderUserItem(user);
      } catch (error) {
        throw new Error(`UserListWidget: Failed to render user at index ${index}: ${error instanceof Error ? error.message : String(error)}`);
      }
    }).join('');
  }

  /**
   * REQUIRED ROW FUNCTION: Renders a single user item
   * This is the "row function" that must work for any valid user data
   */
  private renderUserItem(user: UserData): string {
    // FAIL FAST: Validate required fields for row rendering
    if (!user) {
      throw new Error('UserListWidget: Cannot render null user');
    }
    if (!user.id) {
      throw new Error(`UserListWidget: User missing required 'id' field: ${JSON.stringify(user)}`);
    }
    if (!user.profile?.displayName) {
      throw new Error(`UserListWidget: User missing required 'profile.displayName' field: ${JSON.stringify(user)}`);
    }

    const statusClass = user.status === 'online' ? 'online' : 'offline';
    const avatar = user.profile.avatar || (user.type === 'human' ? '👤' : '🤖');
    const displayName = user.profile.displayName;

    return `
      <div class="user-item ${statusClass}" data-user-id="${user.id}">
        <span class="user-avatar">${avatar}</span>
        <div class="user-info">
          <div class="user-name">${displayName}</div>
          <div class="user-type">${user.type}</div>
        </div>
        <div class="user-status">
          <span class="status-indicator"></span>
        </div>
      </div>
    `;
  }
}