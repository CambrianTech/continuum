/**
 * User List Widget - Database-Driven Chat Users
 * Properly located in chat directory structure
 */

import { ChatWidgetBase } from '../shared/ChatWidgetBase';
import { UserEntity } from '../../../system/data/entities/UserEntity';
import type { DataListParams, DataListResult } from '../../../commands/data/list/shared/DataListTypes';
import { Commands } from '../../../system/core/client/shared/Commands';
import { Events } from '../../../system/core/client/shared/Events';
import { createEntityCrudHandler } from '../../../commands/data/shared/DataEventUtils';
import { createScroller, SCROLLER_PRESETS, type RenderFn, type LoadFn, type EntityScroller } from '../../shared/EntityScroller';

export class UserListWidget extends ChatWidgetBase {
  private userScroller?: EntityScroller<UserEntity>;
  private unsubscribeUserEvents?: () => void;

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
    console.log('🔧 CLAUDE-DEBUG-' + Date.now() + ': UserListWidget onWidgetInitialize() called');

    // Setup event subscriptions first - EntityScroller will be set up after template renders
    await this.setupUserEventSubscriptions();

    console.log('✅ CLAUDE-DEBUG-' + Date.now() + ': UserListWidget onWidgetInitialize() completed');
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

      // ✨ BEAUTIFUL: Intelligent avatar defaults based on user type
      // Note: Plain data from DB, not entity instances, so compute avatar directly
      const avatar = user.profile?.visualIdentity?.avatar ||
        (user.type === 'human' ? '👤' :
         user.type === 'agent' ? '🤖' :
         user.type === 'persona' ? '⭐' :
         user.type === 'system' ? '⚙️' : '❓');

      const speciality = user.profile?.speciality || null;
      const displayName = user.displayName || 'Unknown User';

      const userElement = document.createElement('div');
      userElement.className = `user-item ${statusClass}`;
      userElement.innerHTML = `
        <span class="user-avatar">${avatar}</span>
        <div class="user-info">
          <div class="user-name">${displayName}</div>
          <div class="user-meta">
            <span class="user-type">${user.type}</span>
            ${speciality ? `<span class="user-speciality">${speciality}</span>` : ''}
          </div>
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
   * Set up comprehensive CRUD event subscriptions for real-time updates
   * Now handles CREATE, UPDATE, DELETE operations with a single subscription
   */
  private async setupUserEventSubscriptions(): Promise<void> {
    console.log('🔧 CLAUDE-DEBUG-' + Date.now() + ': UserListWidget setupUserEventSubscriptions() called');

    try {
      console.log(`🎧 UserListWidget: Setting up unified CRUD subscriptions for ${UserEntity.collection}`);

      // Single subscription for ALL User CRUD operations (create, update, delete)
      this.unsubscribeUserEvents = createEntityCrudHandler<UserEntity>(
        UserEntity.collection,
        {
          add: (user: UserEntity) => {
            console.log(`🔥 CRUD-CREATE: Adding user ${user.id}`, user);
            this.userScroller?.add(user);
            this.updateUserCount();
          },
          update: (id: string, user: UserEntity) => {
            console.log(`🔥 CRUD-UPDATE: Updating user ${id}`, user);
            this.userScroller?.update(id, user);
            this.updateUserCount(); // In case display name affects count display
          },
          remove: (id: string) => {
            console.log(`🔥 CRUD-DELETE: Removing user ${id}`);
            this.userScroller?.remove(id);
            this.updateUserCount();
          }
        }
      );

      console.log(`✅ UserListWidget: Unified CRUD subscriptions active for ${UserEntity.collection}`);
      console.log('✅ CLAUDE-DEBUG-' + Date.now() + ': UserListWidget setupUserEventSubscriptions() completed successfully');
    } catch (error) {
      console.error('❌ CLAUDE-DEBUG-' + Date.now() + ': UserListWidget setupUserEventSubscriptions() FAILED:', error);
      console.error('❌ UserListWidget: Failed to set up user event subscriptions:', error);
    }
  }


  protected async onWidgetCleanup(): Promise<void> {
    // Clean up event subscriptions
    this.unsubscribeUserEvents?.();
    this.unsubscribeUserEvents = undefined;

    // Clean up scroller
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