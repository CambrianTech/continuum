/**
 * User List Widget - Database-Driven Chat Users
 * Properly located in chat directory structure
 */

import { ChatWidgetBase } from '../shared/ChatWidgetBase';
import { UserEntity } from '../../../system/data/entities/UserEntity';
import type { DataListParams, DataListResult } from '../../../commands/data/list/shared/DataListTypes';
import { Commands } from '../../../system/core/client/shared/Commands';
import { USER_EVENTS } from '../../../system/events/user/UserEventConstants';
import type { UserCreatedEventData, UserEventMap } from '../../../system/events/user/UserEventTypes';

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
   * Uses EventsDaemon directly instead of widget-specific event system
   */
  private async setupUserEventSubscriptions(): Promise<void> {
    try {
      // Subscribe to USER_CREATED events through EventsDaemon
      document.addEventListener(USER_EVENTS.USER_CREATED, (event: Event) => {
        const customEvent = event as CustomEvent<UserCreatedEventData>;
        console.log(`🔥 SERVER-EVENT-RECEIVED: ${USER_EVENTS.USER_CREATED}`, customEvent.detail);
        this.onUserCreated(customEvent.detail);
      });

      console.log(`🎧 UserListWidget: Subscribed to user events`);

      // TODO: Add other user event subscriptions
      // document.addEventListener(USER_EVENTS.USER_UPDATED, this.onUserUpdated.bind(this));
      // document.addEventListener(USER_EVENTS.USER_DELETED, this.onUserDeleted.bind(this));
    } catch (error) {
      console.error('❌ UserListWidget: Failed to set up user event subscriptions:', error);
    }
  }

  /**
   * Handle USER_CREATED event - add new user to list with proper sorting
   * Different from chat (append) - users get inserted in sort order
   */
  private onUserCreated(eventData: UserCreatedEventData): void {
    const displayName = eventData.userData.displayName || eventData.userData.profile?.displayName || 'Unknown User';
    console.log(`👤 UserListWidget: Adding new user ${displayName}`);

    // Create new UserEntity and populate from event data
    const userEntity = new UserEntity();
    // TODO: populate userEntity fields from eventData.userData
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
    return this.users.map((user, index) => {
      try {
        return this.renderUserItem(user);
      } catch (error) {
        throw new Error(`UserListWidget: Failed to render user at index ${index}: ${error instanceof Error ? error.message : String(error)}`);
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