/**
 * User List Widget - Database-Driven Chat Users
 * Properly located in chat directory structure
 */

import { ChatWidgetBase } from '../shared/ChatWidgetBase';
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

  // Path resolution now handled automatically by ChatWidgetBase
  // Generates: widgets/chat/user-list/{filename} from "UserListWidget"

  async onWidgetInitialize(): Promise<void> {
    await this.loadUsersFromDatabase();
  }

  private async loadUsersFromDatabase(): Promise<void> {
    // More elegant: automatic client context injection
    const result = await this.executeCommand<DataListParams, DataListResult<UserData>>('data/list', {
      collection: COLLECTIONS.USERS,
      orderBy: [{ field: 'lastActiveAt', direction: 'desc' }],
      limit: 100
    });

    // FAIL FAST: Don't allow silent failures
    if (!result) {
      throw new Error('UserListWidget: Database command returned null - system failure');
    }
    if (!result.success) {
      throw new Error(`UserListWidget: Database command failed: ${result.error || 'Unknown error'}`);
    }
    if (!result.items) {
      throw new Error('UserListWidget: Database returned no items array - data structure error');
    }
    // Empty results are OK - UI will show "no users" message
    if (result.items.length === 0) {
      console.log('ℹ️ UserListWidget: No users found - showing empty state');
      this.users = [];
      return;
    }

    // Validate required fields - check both 'id' and 'userId'
    const validUsers = result.items.filter((user: UserData) => {
      const userId = user?.id || user?.userId;
      if (!userId) {
        console.error('❌ UserListWidget: User missing required id/userId:', user);
        return false;
      }
      const displayName = user.profile?.displayName;
      if (!displayName) {
        console.error('❌ UserListWidget: User missing required displayName/name:', user);
        return false;
      }
      return true;
    });

    // Empty valid users is OK - show empty state, don't crash
    if (validUsers.length === 0) {
      console.log('ℹ️ UserListWidget: All users invalid - showing empty state');
      this.users = [];
      return;
    }

    this.users = validUsers;
    console.log(`✅ UserListWidget: Loaded ${this.users.length} valid users`);
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

  private renderUserItem(user: UserData): string {
    const statusClass = user.status === 'online' ? 'online' : 'offline';
    const avatar = user.profile?.avatar || (user.type === 'human' ? '👤' : '🤖');
    const displayName = user.profile?.displayName;

    return `
      <div class="user-item ${statusClass}" data-user-id="${user.id || user.userId}">
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