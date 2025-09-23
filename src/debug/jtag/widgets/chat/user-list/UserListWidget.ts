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
import { createScroller, SCROLLER_PRESETS, type RenderFn, type LoadFn, type EntityScroller } from '../../shared/EntityScroller';

export class UserListWidget extends ChatWidgetBase {
  private userScroller?: EntityScroller<UserEntity>;

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
    // Setup event subscriptions first - EntityScroller will be set up after template renders
    await this.setupUserEventSubscriptions();
  }

  protected override async renderWidget(): Promise<void> {
    // Render template first
    await super.renderWidget();

    // Now set up EntityScroller after DOM exists
    await this.setupUserScroller();
  }

  /**
   * Setup EntityScroller with proper deduplication and real-time updates
   */
  private async setupUserScroller(): Promise<void> {
    const container = this.shadowRoot.querySelector('.user-list') as HTMLElement;
    if (!container) {
      console.error('❌ UserListWidget: Could not find .user-list container');
      return;
    }

    // Render function for individual user items
    const renderUser: RenderFn<UserEntity> = (user: UserEntity, context) => {
      const statusClass = user.status === 'online' ? 'online' : 'offline';
      const avatar = user.profile?.avatar || (user.type === 'human' ? '👤' : '🤖');
      const displayName = user.profile?.displayName || user.displayName || 'Unknown User';

      const userElement = document.createElement('div');
      userElement.className = `user-item ${statusClass}`;
      userElement.innerHTML = `
        <span class="user-avatar">${avatar}</span>
        <div class="user-info">
          <div class="user-name">${displayName}</div>
          <div class="user-type">${user.type}</div>
        </div>
        <div class="user-status">
          <span class="status-indicator"></span>
        </div>
      `;

      return userElement;
    };

    // Load function using existing data/list command
    const loadUsers: LoadFn<UserEntity> = async (cursor, limit) => {
      const result = await Commands.execute<DataListParams<UserEntity>, DataListResult<UserEntity>>('data/list', {
        collection: UserEntity.collection,
        orderBy: [{ field: 'lastActiveAt', direction: 'desc' }],
        limit: limit || 100
      });

      if (!result?.success || !result.items) {
        throw new Error(`Failed to load users: ${result?.error || 'Unknown error'}`);
      }

      return {
        items: result.items,
        hasMore: false, // User lists are typically small, no pagination needed
        nextCursor: undefined
      };
    };

    // Create scroller with LIST preset (no auto-scroll, larger page size)
    this.userScroller = createScroller(
      container,
      renderUser,
      loadUsers,
      SCROLLER_PRESETS.LIST
    );

    // Load initial data
    await this.userScroller.load();
    console.log(`✅ UserListWidget: Initialized EntityScroller with automatic deduplication`);

    // Update count after initial load
    this.updateUserCount();
  }

  /**
   * Set up user event subscriptions for real-time updates
   * Uses EntityScroller's built-in deduplication via add() method
   */
  private async setupUserEventSubscriptions(): Promise<void> {
    try {
      // Subscribe to data:User:created events using static Events interface
      const eventName = getDataEventName(UserEntity.collection, 'created');
      Events.subscribe<UserEntity>(eventName, (userEntity: UserEntity) => {
        console.log(`🔥 SERVER-EVENT-RECEIVED: ${eventName}`, userEntity);
        console.log(`🔧 CLAUDE-FIX-${Date.now()}: Using EntityScroller.add() for automatic deduplication`);

        // EntityScroller automatically handles deduplication using entity.id
        this.userScroller?.add(userEntity);

        // Update count after adding user
        this.updateUserCount();
      });

      console.log(`🎧 UserListWidget: Subscribed to data:${UserEntity.collection}:created events via Events.subscribe()`);

      // Subscribe to data:User:updated events using static Events interface
      const updateEventName = getDataEventName(UserEntity.collection, 'updated');
      Events.subscribe<UserEntity>(updateEventName, (userEntity: UserEntity) => {
        console.log(`🔥 SERVER-EVENT-RECEIVED: ${updateEventName}`, userEntity);
        console.log(`🔧 CLAUDE-FIX-${Date.now()}: Using EntityScroller.updateEntity() for user updates`);

        // EntityScroller updates the user in place
        this.userScroller?.update(userEntity.id, userEntity);

        // Update count if needed (though should remain same for updates)
        this.updateUserCount();
      });

      console.log(`🎧 UserListWidget: Subscribed to data:${UserEntity.collection}:updated events via Events.subscribe()`);

      // TODO: Add delete events
      // Events.subscribe(getDataEventName(UserEntity.collection, 'deleted'), (user) => this.userScroller?.remove(user.id));
    } catch (error) {
      console.error('❌ UserListWidget: Failed to set up user event subscriptions:', error);
    }
  }


  protected async onWidgetCleanup(): Promise<void> {
    this.userScroller?.destroy();
    this.userScroller = undefined;
  }

  protected renderTemplate(): string {
    return `
      <!-- User List Widget - Sidebar navigation for users and agents -->
      <div class="user-list-container">
        <div class="user-list-header">
          <span class="header-title">Users & Agents</span>
          <span class="user-count">0</span>
        </div>

        <!-- EntityScroller will manage this container -->
        <div class="user-list">
          <!-- Users will be added here by EntityScroller -->
        </div>
      </div>
    `;
  }

  /**
   * Update the user count display in the header
   */
  private updateUserCount(): void {
    const userCountElement = this.shadowRoot.querySelector('.user-count') as HTMLElement;
    if (userCountElement && this.userScroller) {
      const count = this.userScroller.entities().length;
      userCountElement.textContent = count.toString();
      console.log(`🔧 CLAUDE-FIX-${Date.now()}: Updated user count to ${count}`);
    }
  }
}