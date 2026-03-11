/**
 * User List Widget - Migrated to ReactiveListWidget
 * Uses header/item/footer pattern with Lit templates
 *
 * PersonaTile handles all AI-specific visuals (ring, diamonds, meters, genome bars)
 * as a self-contained Lit component with its own event subscriptions.
 * No more manual DOM patching for cached EntityScroller elements.
 */

import {
  ReactiveListWidget,
  html,
  reactive,
  unsafeCSS,
  nothing,
  type TemplateResult,
  type CSSResultGroup
} from '../../shared/ReactiveListWidget';
import { render } from 'lit';
import type { RenderFn, RenderContext } from '../../shared/EntityScroller';
import { UserEntity } from '../../../system/data/entities/UserEntity';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { ContentService } from '../../../system/state/ContentService';
import { Dm } from '../../../commands/collaboration/dm/shared/DmTypes';

import { styles as externalStyles } from './user-list.styles';

// Register PersonaTile custom element
import './PersonaTile';

// Verbose logging helper
const verbose = () => typeof window !== 'undefined' && (window as any).JTAG_VERBOSE === true;

export class UserListWidget extends ReactiveListWidget<UserEntity> {
  readonly collection = UserEntity.collection;

  // === REACTIVE STATE ===
  @reactive() private _selectedUserId: string | null = null;

  // Filter chips state
  @reactive() private activeFilters: Set<string> = new Set(['all']);

  static override styles = [
    ReactiveListWidget.styles,
    unsafeCSS(externalStyles)
  ] as CSSResultGroup;

  constructor() {
    super({ widgetName: 'UserListWidget' });
  }

  // === CONFIGURATION ===
  protected override get listTitle(): string { return 'Users & Agents'; }
  protected override get containerClass(): string { return 'user-list'; }
  protected override get orderBy() {
    return [{ field: 'lastActiveAt', direction: 'desc' as const }];
  }

  // Users MUST always revalidate against the server — localStorage cache caused
  // the 3-user bug where stale cache prevented loading all 17 users.
  protected override get loadBackend(): 'stale-while-revalidate' { return 'stale-while-revalidate'; }

  // === HEADER with filter chips ===
  protected override renderHeader(): TemplateResult {
    const typeFilters = [
      { id: 'all', label: 'All', icon: '◉' },
      { id: 'human', label: 'Human', icon: '👤' },
      { id: 'persona', label: 'Persona', icon: '⭐' },
      { id: 'agent', label: 'Agent', icon: '🤖' }
    ];

    const statusFilters = [
      { id: 'online', label: 'Online', icon: '●' }
    ];

    return html`
      <div class="entity-list-header">
        <span class="header-title">${this.listTitle}</span>
        <span class="user-count">${this.entityCount}</span>
      </div>
      <div class="filter-chips">
        <div class="filter-group type-filters">
          ${typeFilters.map(f => html`
            <button
              class="filter-chip ${this.activeFilters.has(f.id) ? 'active' : ''}"
              data-filter="${f.id}"
              @click=${() => this.toggleFilter(f.id)}
            >
              <span class="chip-icon">${f.icon}</span>
              <span class="chip-label">${f.label}</span>
            </button>
          `)}
        </div>
        <span class="filter-divider"></span>
        <div class="filter-group status-filters">
          ${statusFilters.map(f => html`
            <button
              class="filter-chip ${this.activeFilters.has(f.id) ? 'active' : ''}"
              data-filter="${f.id}"
              @click=${() => this.toggleFilter(f.id)}
            >
              <span class="chip-icon">${f.icon}</span>
              <span class="chip-label">${f.label}</span>
            </button>
          `)}
        </div>
      </div>
    `;
  }

  private toggleFilter(filterId: string): void {
    const newFilters = new Set(this.activeFilters);

    if (filterId === 'all') {
      // 'All' clears other type filters, keeps status filters
      newFilters.clear();
      newFilters.add('all');
    } else if (filterId === 'online') {
      // Toggle online status independently
      if (newFilters.has('online')) {
        newFilters.delete('online');
      } else {
        newFilters.add('online');
      }
    } else {
      // Type filter - remove 'all' and toggle this type
      newFilters.delete('all');
      if (newFilters.has(filterId)) {
        newFilters.delete(filterId);
        // If no type filters left, go back to 'all'
        const typeFilters = ['human', 'persona', 'agent'];
        if (!typeFilters.some(t => newFilters.has(t))) {
          newFilters.add('all');
        }
      } else {
        newFilters.add(filterId);
      }
    }

    this.activeFilters = newFilters;
    this.requestUpdate();  // Re-render header with new active states

    // Clear and reload to apply new filters
    // (getRenderFunction checks matchesFilters for each item)
    this.scroller?.clear();
    this.scroller?.load();
  }

  // === MAIN RENDER ===
  override render(): TemplateResult {
    return html`
      <div class="entity-list-container">
        ${this.renderHeader()}
        <div class="${this.containerClass}"></div>
        ${this.renderFooter()}
      </div>
    `;
  }

  // === ITEM RENDERING ===
  renderItem(user: UserEntity): TemplateResult {
    const displayName = user.displayName ?? 'Unknown User';
    const isSelected = this._selectedUserId === user.id;
    const lastActive = user.lastActiveAt ? this.formatTimestamp(user.lastActiveAt) : '';

    // Model info for AI
    let modelInfo = '';
    let modelBadge = '';
    if (user.type === 'persona' || user.type === 'agent') {
      const provider = user.modelConfig?.provider || (user.personaConfig?.responseModel ? 'candle' : '');
      const model = user.modelConfig?.model || user.personaConfig?.responseModel || '';
      if (provider) {
        modelInfo = model ? `${provider}/${model}` : provider;
        modelBadge = provider.substring(0, 8).toUpperCase();
      }
    }

    // RAG certification
    let ragCertified = user.modelConfig?.ragCertified ?? false;
    if (!ragCertified && user.uniqueId === 'persona-sentinel') {
      ragCertified = true;
    }

    // Response mode
    const requiresMention = user.modelConfig?.requiresExplicitMention ?? false;

    return html`
      <div
        class="user-item ${isSelected ? 'selected' : ''}"
        data-user-id=${user.id}
        data-user-type=${user.type}
        @click=${(e: Event) => this.handleUserClick(e, user)}
      >
        <persona-tile
          .userId=${user.id}
          .displayName=${displayName}
          .userType=${user.type}
          .uniqueId=${user.uniqueId || ''}
          .status=${user.status || 'offline'}
          .speciality=${user.speciality || ''}
          .modelInfo=${modelInfo}
          .modelBadge=${modelBadge}
          .requiresMention=${requiresMention}
          .ragCertified=${ragCertified}
          .lastActive=${lastActive}
          .intelligenceLevel=${user.intelligenceLevel ?? 0}
        ></persona-tile>
        <div class="user-controls">
          <button class="user-call-btn" title="Message" @click=${(e: Event) => this.handleCallClick(e, user)}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"></path>
            </svg>
          </button>
          <button class="user-favorite-btn" title="Add to favorites" @click=${(e: Event) => this.handleFavoriteClick(e, user.id)}>⭐</button>
          <button class="user-action-btn" title="Actions" @click=${(e: Event) => this.handleActionClick(e, user.id)}>»</button>
        </div>
      </div>
    `;
  }

  // === FILTERING ===
  private matchesFilters(user: UserEntity): boolean {
    // Check type filters
    const hasAll = this.activeFilters.has('all');
    const typeFilters = ['human', 'persona', 'agent'];
    const activeTypeFilters = typeFilters.filter(t => this.activeFilters.has(t));

    if (!hasAll && activeTypeFilters.length > 0) {
      if (!activeTypeFilters.includes(user.type)) {
        return false;
      }
    }

    // Check online filter
    if (this.activeFilters.has('online')) {
      if (user.status !== 'online') {
        return false;
      }
    }

    return true;
  }

  // Override getRenderFunction to apply chip filtering
  protected override getRenderFunction(): RenderFn<UserEntity> {
    return (user: UserEntity, _context: RenderContext<UserEntity>) => {
      // Return hidden element for non-matching users
      if (!this.matchesFilters(user)) {
        const hiddenElement = globalThis.document.createElement('div');
        hiddenElement.style.display = 'none';
        hiddenElement.dataset.id = user.id;
        return hiddenElement;
      }

      // Normal rendering via parent
      const div = globalThis.document.createElement('div');
      div.className = 'list-item';
      div.dataset.id = user.id;
      render(this.renderItem(user), div);
      div.addEventListener('click', (e) => {
        e.stopPropagation();
        this.onItemClick(user);
      });
      return div;
    };
  }

  // === EVENT HANDLERS ===
  private handleUserClick(e: Event, user: UserEntity): void {
    if ((e.target as HTMLElement).tagName === 'BUTTON') return;
    this._selectedUserId = user.id;
    this.openUserProfile(user);
  }

  private handleFavoriteClick(e: Event, userId: string): void {
    e.stopPropagation();
    verbose() && console.log(`⭐ UserListWidget: Toggle favorite for user ${userId}`);
  }

  private handleActionClick(e: Event, userId: string): void {
    e.stopPropagation();
    verbose() && console.log(`» UserListWidget: Show action menu for user ${userId}`);
  }

  private async handleCallClick(e: Event, user: UserEntity): Promise<void> {
    e.stopPropagation();
    console.log(`💬 UserListWidget: Opening DM with ${user.displayName} (${user.id})`);

    try {
      const result = await Dm.execute({
        participants: user.id
      });

      if (result.success && result.room) {
        if (this.currentUser?.id) {
          ContentService.setUserId(this.currentUser.id as UUID);
        }

        ContentService.open('chat', result.roomId as string, {
          title: user.displayName || user.uniqueId || 'DM',
          subtitle: 'Direct Message',
          uniqueId: result.uniqueId,
          metadata: { entity: result.room }
        });
      } else {
        console.error('UserListWidget: Failed to create DM:', result.message);
      }
    } catch (error) {
      console.error('UserListWidget: Error creating DM:', error);
    }
  }

  private openUserProfile(userEntity: UserEntity): void {
    const entityId = userEntity.id;
    const uniqueId = userEntity.uniqueId || userEntity.id;
    const title = userEntity.displayName || 'User Profile';

    verbose() && console.log(`👤 UserListWidget: Opening profile for ${title} (entityId=${entityId}, uniqueId=${uniqueId})`);

    if (this.currentUser?.id) {
      ContentService.setUserId(this.currentUser.id as UUID);
    }

    ContentService.open('profile', entityId, {
      title,
      uniqueId,
      metadata: { entity: userEntity }
    });
  }

  // === HELPER METHODS ===
  private formatTimestamp(timestamp: number | Date | string | undefined): string {
    if (!timestamp) return 'Never';

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

    if (diff < 60000) return 'Just now';
    if (diff < 3600000) return `${Math.floor(diff / 60000)}m ago`;
    if (diff < 86400000) return `${Math.floor(diff / 3600000)}h ago`;
    if (diff < 604800000) return `${Math.floor(diff / 86400000)}d ago`;
    return new Date(timestampMs).toLocaleDateString();
  }

  // === SELECTION HOOK (override base) ===
  protected override onItemClick(_item: UserEntity): void {
    // Handled by @click in renderItem template
  }
}
