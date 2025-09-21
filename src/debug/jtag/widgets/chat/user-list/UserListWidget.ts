/**
 * User List Widget - Database-Driven Chat Users
 * Properly located in chat directory structure
 */

import { ChatWidgetBase } from '../shared/ChatWidgetBase';
import { UserEntity } from '../../../system/data/entities/UserEntity';
import type { DataListParams, DataListResult } from '../../../commands/data/list/shared/DataListTypes';
import { Commands } from '../../../system/core/client/shared/Commands';
import { Events } from '../../../system/core/client/shared/Events';
import { getDataEventName } from '../../../commands/data/shared/DataEventConstants';

export class UserListWidget extends ChatWidgetBase {
  private users: UserEntity[] = [];

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
    await this.setupUserEventSubscriptions();
  }

  /**
   * Set up user event subscriptions for real-time updates
   * Uses static Events.subscribe() for data:User:created events
   */
  private async setupUserEventSubscriptions(): Promise<void> {
    try {
      // Subscribe to data:User:created events using static Events interface
      const eventName = getDataEventName(UserEntity.collection, 'created');
      Events.subscribe<UserEntity>(eventName, (userEntity: UserEntity) => {
        console.log(`🔥 SERVER-EVENT-RECEIVED: ${eventName}`, userEntity);
        this.onDataUserCreated(userEntity);
      });

      console.log(`🎧 UserListWidget: Subscribed to data:${UserEntity.collection}:created events via Events.subscribe()`);

      // TODO: Add other data event subscriptions
      // Events.subscribe(`data:${UserEntity.collection}:updated`, this.onDataUserUpdated.bind(this));
      // Events.subscribe(`data:${UserEntity.collection}:deleted`, this.onDataUserDeleted.bind(this));
    } catch (error) {
      console.error('❌ UserListWidget: Failed to set up user event subscriptions:', error);
    }
  }

  /**
   * Handle data:User:created event - add new user to list with proper sorting
   * Uses generic data events from DataCreateServerCommand
   */
  private onDataUserCreated(userEntity: UserEntity): void {
    // The userEntity is the exact same structure that data/list returns
    const displayName = userEntity.profile?.displayName || userEntity.displayName || 'Unknown User';
    console.log(`👤 UserListWidget: Adding new user ${displayName} from data:${UserEntity.collection}:created event`);
    console.log(`🔧 CLAUDE-DEBUG: UserEntity data:`, userEntity);

    this.users.push(userEntity);

    // Re-sort by lastActiveAt (newest first) - same as database query
    this.users.sort((a, b) => {
      const aTime = new Date(a.lastActiveAt).getTime();
      const bTime = new Date(b.lastActiveAt).getTime();
      return bTime - aTime; // Descending order
    });

    // Re-render with updated user list
    this.renderWidget();
  }

  private async loadUsersFromDatabase(): Promise<void> {
    // Sacred type-driven data access - UserData drives everything
    // Collection name "UserData" -> ORM maps to appropriate table
    const result = await Commands.execute<DataListParams<UserEntity>, DataListResult<UserEntity>>('data/list', {
      collection: UserEntity.collection,
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

    // Results are pure UserEntity - the sacred entity with decorators
    // No casting needed, ORM returns proper UserEntity objects
    const validUsers = result.items.filter((user: UserEntity) => {
      return user && user.type; // Minimal validation, let decorators handle the rest
    });

    // Empty valid users is OK - show empty state, don't crash
    if (validUsers.length === 0) {
      console.log('ℹ️ UserListWidget: All users invalid - showing empty state');
      this.users = [];
      return;
    }

    this.users = validUsers;
    console.log(`✅ UserListWidget: Loaded ${this.users.length} valid users via entity-type-driven system`);
    console.log(`   Users: ${this.users.map(u => u.profile?.displayName || 'Unknown').join(', ')}`);
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
          <p class="no-content-text">No users online</p>
          <small class="no-content-hint">Waiting for others to join</small>
        </div>
      `;
    }

    // REQUIRED ROW FUNCTION: Map each user using validated row rendering
    return this.users
      .filter((user, index) => {
        // CRITICAL: Filter out invalid users to prevent crashes
        if (!user || typeof user !== 'object') {
          console.warn(`⚠️ UserListWidget: Skipping invalid user at index ${index}:`, user);
          return false;
        }
        if (!user.displayName) {
          console.warn(`⚠️ UserListWidget: Skipping user without displayName at index ${index}:`, user);
          return false;
        }
        return true;
      })
      .map((user, index) => {
        try {
          return this.renderUserItem(user);
        } catch (error) {
          console.error(`❌ UserListWidget: Failed to render user at index ${index}:`, error);
          return `<div class="user-item error">⚠️ Invalid User</div>`;
        }
      }).join('');
  }

  private renderUserItem(user: UserEntity): string {
    const statusClass = user.status === 'online' ? 'online' : 'offline';
    const avatar = user.profile?.avatar || (user.type === 'human' ? '👤' : '🤖');
    const displayName = user.profile.displayName;

    return `
      <div class="user-item ${statusClass}" data-user-id="${user.userId}">
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