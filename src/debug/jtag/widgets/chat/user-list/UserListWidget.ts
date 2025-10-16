/**
 * User List Widget - Database-Driven Chat Users
 * Now uses EntityScrollerWidget base class for automatic EntityScroller management
 */

import { EntityScrollerWidget } from '../../shared/EntityScrollerWidget';
import { UserEntity } from '../../../system/data/entities/UserEntity';
import type { DataListParams, DataListResult } from '../../../commands/data/list/shared/DataListTypes';
import { Commands } from '../../../system/core/shared/Commands';
import { DATA_COMMANDS } from '../../../commands/data/shared/DataCommandConstants';
import { SCROLLER_PRESETS, type RenderFn, type LoadFn, type ScrollerConfig } from '../../shared/EntityScroller';
import { Events } from '../../../system/core/shared/Events';
import { AI_DECISION_EVENTS } from '../../../system/events/shared/AIDecisionEvents';

/**
 * AI Status tracking for user list display
 */
interface AIStatusState {
  personaId: string;
  currentPhase: 'evaluating' | 'responding' | 'generating' | 'checking' | 'passed' | 'error' | null;
  timestamp: number;
  errorMessage?: string;
}

export class UserListWidget extends EntityScrollerWidget<UserEntity> {
  private searchFilter: string = '';
  private selectedUserId: string | null = null;
  private aiStatuses: Map<string, AIStatusState> = new Map();
  private allUsers: UserEntity[] = [];

  constructor() {
    super({
      widgetId: 'user-list-widget',
      widgetName: 'UserListWidget',
      styles: 'user-list.css',
      enableAI: false,
      enableDatabase: true,
      enableRouterEvents: false,
      enableScreenshots: false
    });
  }

  // Path resolution now handled automatically by ChatWidgetBase
  // EntityScroller setup now handled automatically by EntityScrollerWidget base class

  /**
   * Override onWidgetInitialize to setup AI event subscriptions
   */
  protected override async onWidgetInitialize(): Promise<void> {
    await super.onWidgetInitialize();
    this.setupAIEventSubscriptions();
  }

  /**
   * Subscribe to AI decision events for status emoji indicators
   */
  private setupAIEventSubscriptions(): void {
    console.log('🔧 UserListWidget: Setting up AI event subscriptions...');

    Events.subscribe(AI_DECISION_EVENTS.EVALUATING, (data: { personaId: string }) => {
      this.updateAIStatus(data.personaId, 'evaluating');
    });

    Events.subscribe(AI_DECISION_EVENTS.DECIDED_RESPOND, (data: { personaId: string }) => {
      this.updateAIStatus(data.personaId, 'responding');
    });

    Events.subscribe(AI_DECISION_EVENTS.DECIDED_SILENT, (data: { personaId: string }) => {
      this.updateAIStatus(data.personaId, 'passed');
    });

    Events.subscribe(AI_DECISION_EVENTS.GENERATING, (data: { personaId: string }) => {
      this.updateAIStatus(data.personaId, 'generating');
    });

    Events.subscribe(AI_DECISION_EVENTS.CHECKING_REDUNDANCY, (data: { personaId: string }) => {
      this.updateAIStatus(data.personaId, 'checking');
    });

    Events.subscribe(AI_DECISION_EVENTS.POSTED, (data: { personaId: string }) => {
      this.updateAIStatus(data.personaId, null); // Clear status after successful post
    });

    Events.subscribe(AI_DECISION_EVENTS.ERROR, (data: { personaId: string; error: string }) => {
      this.updateAIStatus(data.personaId, 'error', data.error);
    });
  }

  /**
   * Update AI status and refresh the user item in the list
   */
  private updateAIStatus(personaId: string, phase: AIStatusState['currentPhase'], errorMessage?: string): void {
    if (phase === null) {
      this.aiStatuses.delete(personaId);
    } else {
      this.aiStatuses.set(personaId, {
        personaId,
        currentPhase: phase,
        timestamp: Date.now(),
        errorMessage
      });
    }

    // Refresh the specific user item in the scroller
    this.scroller?.refresh();
  }

  /**
   * Get AI status emoji for display
   */
  private getStatusEmoji(userId: string): string {
    const status = this.aiStatuses.get(userId);
    if (!status || !status.currentPhase) return '';

    switch (status.currentPhase) {
      case 'evaluating':
        return '🤔';
      case 'responding':
        return '💭';
      case 'generating':
        return '✍️';
      case 'checking':
        return '🔍';
      case 'error':
        return '❌';
      case 'passed':
        return '⏭️';
      default:
        return '';
    }
  }

  /**
   * Override renderTemplate to add search input
   */
  protected override renderTemplate(): string {
    const cssClass = 'user-list';
    return `
      <div class="entity-list-container">
        ${this.renderHeader()}

        <div class="user-search">
          <input
            type="text"
            class="search-input"
            placeholder="Search users and agents..."
            value="${this.searchFilter}"
          />
        </div>

        <div class="entity-list-body ${cssClass}">
          <!-- EntityScroller will populate this container -->
        </div>

        ${this.renderFooter()}
      </div>
    `;
  }

  /**
   * Setup event listeners for search input and user interactions
   */
  protected override setupEventListeners(): void {
    const searchInput = this.shadowRoot.querySelector('.search-input') as HTMLInputElement;
    if (searchInput) {
      searchInput.addEventListener('input', (e) => {
        this.searchFilter = (e.target as HTMLInputElement).value.toLowerCase();
        this.scroller?.refresh();
      });
    }
  }

  // Required by EntityScrollerWidget - render function for individual user items
  protected getRenderFunction(): RenderFn<UserEntity> {
    return (user: UserEntity, _context) => {
      // Apply search filter - return hidden element for non-matching users
      const displayName = user.displayName ?? 'Unknown User';
      const searchableText = `${displayName} ${user.type} ${user.profile?.speciality ?? ''}`.toLowerCase();
      if (this.searchFilter && !searchableText.includes(this.searchFilter)) {
        const hiddenElement = globalThis.document.createElement('div');
        hiddenElement.style.display = 'none';
        return hiddenElement;
      }

      const statusClass = user.status === 'online' ? 'online' : 'offline';
      const isSelected = this.selectedUserId === user.id;

      // Avatar fallback by type (profile not loaded in list view)
      const avatar = user.type === 'human' ? '👤' :
                    user.type === 'agent' ? '🤖' :
                    user.type === 'persona' ? '⭐' :
                    user.type === 'system' ? '⚙️' : '❓';

      const speciality = user.speciality; // UserEntity getter
      const aiStatusEmoji = this.getStatusEmoji(user.id);

      // Format last active timestamp
      const lastActive = user.lastActiveAt ? this.formatTimestamp(user.lastActiveAt) : null;

      // User type badge - uses UserType from UserEntity (CSS handles uppercase)
      const typeBadge = user.type;

      const userElement = globalThis.document.createElement('div');
      userElement.className = `user-item ${statusClass} ${isSelected ? 'selected' : ''}`;
      userElement.dataset.userId = user.id;

      userElement.innerHTML = `
        <span class="user-avatar">${avatar}</span>
        <div class="user-info">
          <div class="user-name-row">
            <span class="user-name">${displayName}</span>
            ${aiStatusEmoji ? `<span class="user-ai-status" title="AI Status">${aiStatusEmoji}</span>` : ''}
          </div>
          <div class="user-meta">
            <span class="user-type-badge">${typeBadge}</span>
            ${speciality ? `<span class="user-speciality">${speciality}</span>` : ''}
            ${lastActive ? `<span class="user-last-active">${lastActive}</span>` : ''}
          </div>
        </div>
        <div class="user-controls">
          <button class="user-favorite-btn" title="Add to favorites">⭐</button>
          <button class="user-action-btn" title="Actions">»</button>
        </div>
        <div class="user-status">
          <span class="status-indicator"></span>
        </div>
      `;

      // Add click handler for selection
      userElement.addEventListener('click', (e) => {
        // Don't select if clicking on a button
        if ((e.target as HTMLElement).tagName === 'BUTTON') return;
        this.selectUser(user.id);
      });

      // Add button handlers
      const favoriteBtn = userElement.querySelector('.user-favorite-btn');
      favoriteBtn?.addEventListener('click', (e) => {
        e.stopPropagation();
        this.toggleFavorite(user.id);
      });

      const actionBtn = userElement.querySelector('.user-action-btn');
      actionBtn?.addEventListener('click', (e) => {
        e.stopPropagation();
        this.showActionMenu(user.id);
      });

      return userElement;
    };
  }

  /**
   * Format timestamp for last active display
   */
  private formatTimestamp(timestamp: number | Date | string | undefined): string {
    if (!timestamp) return 'Never';

    // Convert to timestamp number
    let timestampMs: number;
    if (typeof timestamp === 'number') {
      timestampMs = timestamp;
    } else if (timestamp instanceof Date) {
      timestampMs = timestamp.getTime();
    } else if (typeof timestamp === 'string') {
      timestampMs = new Date(timestamp).getTime();
    } else {
      return 'Unknown';
    }

    const now = Date.now();
    const diff = now - timestampMs;

    // Less than 1 minute
    if (diff < 60000) return 'Just now';

    // Less than 1 hour
    if (diff < 3600000) {
      const mins = Math.floor(diff / 60000);
      return `${mins}m ago`;
    }

    // Less than 24 hours
    if (diff < 86400000) {
      const hours = Math.floor(diff / 3600000);
      return `${hours}h ago`;
    }

    // Less than 7 days
    if (diff < 604800000) {
      const days = Math.floor(diff / 86400000);
      return `${days}d ago`;
    }

    // Format as date
    return new Date(timestampMs).toLocaleDateString();
  }

  /**
   * Select a user (highlight in list)
   */
  private selectUser(userId: string): void {
    this.selectedUserId = userId;
    this.scroller?.refresh();
    console.log(`✅ UserListWidget: Selected user ${userId}`);
  }

  /**
   * Toggle favorite status
   */
  private toggleFavorite(userId: string): void {
    console.log(`⭐ UserListWidget: Toggle favorite for user ${userId}`);
    // TODO: Implement favorite persistence
  }

  /**
   * Show action menu for user
   */
  private showActionMenu(userId: string): void {
    console.log(`» UserListWidget: Show action menu for user ${userId}`);
    // TODO: Implement action menu (DM, view profile, etc.)
  }

  // Required by EntityScrollerWidget - load function using data/list command
  protected getLoadFunction(): LoadFn<UserEntity> {
    return async (cursor, limit) => {
      const result = await Commands.execute<DataListParams<UserEntity>, DataListResult<UserEntity>>(DATA_COMMANDS.LIST, {
        collection: UserEntity.collection,
        orderBy: [{ field: 'lastActiveAt', direction: 'desc' }],
        limit: limit ?? 100
      });

      if (!result?.success || !result.items) {
        throw new Error(`Failed to load users: ${result?.error ?? 'Unknown error'}`);
      }

      return {
        items: result.items,
        hasMore: false, // User lists are typically small, no pagination needed
        nextCursor: undefined
      };
    };
  }

  // Required by EntityScrollerWidget
  protected getScrollerPreset(): ScrollerConfig {
    return SCROLLER_PRESETS.LIST; // No auto-scroll, larger page size
  }

  // Required by EntityScrollerWidget
  protected getContainerSelector(): string {
    return '.user-list';
  }

  // Required by EntityScrollerWidget
  protected getEntityCollection(): string {
    return UserEntity.collection;
  }

  protected resolveResourcePath(filename: string): string {
    return `widgets/chat/user-list/${filename}`;
  }

  // Event subscriptions now handled automatically by EntityScrollerWidget base class


  // Entity count now handled automatically by EntityScrollerWidget base class

  protected getEntityTitle(_entity?: UserEntity): string {
    return 'Users & Agents';
  }

  // Cleanup now handled automatically by EntityScrollerWidget base class

  // Using default template from EntityScrollerWidget (generates "user-list" CSS class automatically)

}