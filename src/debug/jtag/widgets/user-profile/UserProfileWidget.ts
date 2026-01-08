/**
 * UserProfileWidget - Universal user profile view
 *
 * Works for ALL user types: humans, personas, agents
 * Shows: name, type, status, last active, stats
 * Actions: Edit, Freeze/Unfreeze, Delete
 * For PersonaUsers: Links to cognitive views (brain, genome, memory)
 *
 * Structure:
 * - public/user-profile-widget.html - Template container
 * - public/user-profile-widget.scss - Styles (compiled to .css)
 * - UserProfileWidget.ts - Logic (this file)
 */

import { BaseWidget } from '../shared/BaseWidget';
import { Commands } from '../../system/core/shared/Commands';
import { Events } from '../../system/core/shared/Events';
import { DATA_COMMANDS } from '../../commands/data/shared/DataCommandConstants';
import type { UserEntity, UserStatus } from '../../system/data/entities/UserEntity';
import type { DataReadParams, DataReadResult } from '../../commands/data/read/shared/DataReadTypes';
import type { DataListParams, DataListResult } from '../../commands/data/list/shared/DataListTypes';
import type { DataDeleteParams, DataDeleteResult } from '../../commands/data/delete/shared/DataDeleteTypes';
import type { DataUpdateParams, DataUpdateResult } from '../../commands/data/update/shared/DataUpdateTypes';
import type { ContentOpenParams, ContentOpenResult } from '../../commands/collaboration/content/open/shared/ContentOpenTypes';
import { PositronWidgetState } from '../shared/services/state/PositronWidgetState';
import { getWidgetEntityId } from '../shared/WidgetConstants';

export class UserProfileWidget extends BaseWidget {
  private user: UserEntity | null = null;
  private loading = true;
  private error: string | null = null;

  constructor() {
    super({
      widgetName: 'UserProfileWidget',
      template: 'user-profile-widget.html',
      styles: 'user-profile-widget.css',
      enableAI: false,
      enableDatabase: true,
      enableRouterEvents: false,
      enableScreenshots: false
    });
  }

  /**
   * Override path resolution - directory is 'user-profile' (kebab-case)
   */
  protected resolveResourcePath(filename: string): string {
    return `widgets/user-profile/public/${filename}`;
  }

  protected async onWidgetInitialize(): Promise<void> {
    this.verbose() && console.log('UserProfile: Initializing...');
    await this.loadUser();
  }

  /**
   * Called by MainWidget when this widget is activated with a new entityId.
   * This allows cached widgets to reload with different entities.
   */
  public async onActivate(entityId?: string): Promise<void> {
    this.verbose() && console.log(`UserProfile: onActivate called with entityId=${entityId}`);

    // Store entityId as attribute for loadUser to find
    if (entityId) {
      this.setAttribute('entity-id', entityId);
    } else {
      this.removeAttribute('entity-id');
    }

    // Reset state and reload
    this.user = null;
    this.loading = true;
    this.error = null;
    await this.loadUser();
  }

  private async loadUser(): Promise<void> {
    // Use helper function for consistent attribute handling
    const entityId = getWidgetEntityId(this) || this.pageState?.entityId;

    if (!entityId) {
      this.error = 'No user specified';
      this.loading = false;
      this.renderWidget();
      return;
    }

    try {
      // Try to find by uniqueId first, then by id
      const result = await Commands.execute<DataReadParams, DataReadResult<UserEntity>>(DATA_COMMANDS.READ, {
        collection: 'users',
        id: entityId
      });

      if (result?.data) {
        this.user = result.data;
      } else {
        // Try finding by uniqueId
        const listResult = await Commands.execute<DataListParams, DataListResult<UserEntity>>(DATA_COMMANDS.LIST, {
          collection: 'users',
          filter: { uniqueId: entityId },
          limit: 1
        });

        if (listResult?.items?.[0]) {
          this.user = listResult.items[0] as UserEntity;
        } else {
          this.error = `User not found: ${entityId}`;
        }
      }
    } catch (err) {
      this.error = `Failed to load user: ${err}`;
    }

    this.loading = false;
    this.emitPositronContext();
    this.renderWidget();
  }

  /**
   * Emit Positron context for AI awareness
   * NOTE: Removed PositronWidgetState.emit() - MainWidget handles context.
   * KEPT: emitWidgetEvent for widget-to-widget communication (ChatWidget listens)
   */
  private emitPositronContext(): void {
    if (!this.user) return;

    // Emit widget event for reactive subscriptions (ChatWidget listens to this)
    PositronWidgetState.emitWidgetEvent('profile', 'status:changed', {
      userId: this.user.id,
      status: this.user.status,
      displayName: this.user.displayName,
      userType: this.user.type
    });
  }

  private async updateUserStatus(newStatus: UserStatus): Promise<void> {
    if (!this.user) return;

    try {
      await Commands.execute<DataUpdateParams, DataUpdateResult<UserEntity>>(DATA_COMMANDS.UPDATE, {
        collection: 'users',
        id: this.user.id,
        data: { status: newStatus }
      });

      this.user.status = newStatus;
      this.renderWidget();

      // Emit event so user list can refresh
      Events.emit('data:users:updated', { id: this.user.id, status: newStatus });

      // Emit widget event for reactive subscriptions
      PositronWidgetState.emitWidgetEvent('profile', 'status:changed', {
        userId: this.user.id,
        status: newStatus,
        displayName: this.user.displayName,
        userType: this.user.type
      });
    } catch (err) {
      console.error('Failed to update user status:', err);
    }
  }

  private async deleteUser(): Promise<void> {
    if (!this.user) return;

    if (!confirm(`Are you sure you want to permanently delete ${this.user.displayName}? This cannot be undone.`)) {
      return;
    }

    try {
      await Commands.execute<DataDeleteParams, DataDeleteResult>(DATA_COMMANDS.DELETE, {
        collection: 'users',
        id: this.user.id
      });

      // Emit event so user list can refresh
      Events.emit('data:users:deleted', { id: this.user.id });

      // Navigate back to chat via command (command emits content:opened)
      const userId = this.userState?.userId;
      if (userId) {
        Commands.execute<ContentOpenParams, ContentOpenResult>('collaboration/content/open', {
          userId,
          contentType: 'chat',
          entityId: 'general',
          title: 'General',
          setAsCurrent: true
        }).catch(console.error);
      }
    } catch (err) {
      console.error('Failed to delete user:', err);
    }
  }

  private async openCognition(): Promise<void> {
    if (!this.user) return;

    // NOTE: Removed direct Events.emit('content:opened') - the command emits it
    // Having both causes duplicate events and state cascade
    const userId = this.userState?.userId;
    if (userId) {
      Commands.execute<ContentOpenParams, ContentOpenResult>('collaboration/content/open', {
        userId,
        contentType: 'persona',
        entityId: this.user.uniqueId || this.user.id,
        title: `${this.user.displayName} - Brain`,
        setAsCurrent: true
      }).catch(console.error);
    }
  }

  protected async renderWidget(): Promise<void> {
    // Inject loaded template and styles into shadow DOM
    if (this.shadowRoot && (this.templateHTML || this.templateCSS)) {
      const styleTag = this.templateCSS ? `<style>${this.templateCSS}</style>` : '';
      this.shadowRoot.innerHTML = styleTag + (this.templateHTML || '');
    }

    // Render dynamic content
    this.renderContent();
    this.setupEventListeners();
  }

  private renderContent(): void {
    const container = this.shadowRoot?.querySelector('.profile-container');
    if (!container) return;

    if (this.loading) {
      container.innerHTML = '<div class="loading">Loading user...</div>';
      return;
    }

    if (this.error) {
      container.innerHTML = `<div class="error">${this.error}</div>`;
      return;
    }

    if (!this.user) return;

    const avatar = this.user.type === 'human' ? '👤' :
                  this.user.type === 'agent' ? '🤖' :
                  this.user.type === 'persona' ? '⭐' : '⚙️';

    const isFrozen = this.user.status === 'frozen';
    const isAI = this.user.type === 'persona' || this.user.type === 'agent';

    const lastActive = this.user.lastActiveAt
      ? new Date(this.user.lastActiveAt).toLocaleString()
      : 'Never';

    container.innerHTML = `
      <div class="profile-header">
        <div class="avatar ${isFrozen ? 'frozen' : ''}">${avatar}</div>
        <div class="user-details">
          <h1 class="user-name">${this.user.displayName}</h1>
          <div class="user-meta">
            <span class="badge type-${this.user.type}">${this.user.type}</span>
            <span class="badge status-${this.user.status}">${this.user.status}</span>
            ${this.user.shortDescription ? `<span class="description-text">${this.user.shortDescription}</span>` : ''}
          </div>
        </div>
      </div>

      <div class="section">
        <div class="section-header">Information</div>
        <div class="section-content">
          <div class="info-grid">
            <div class="info-item">
              <span class="info-label">Unique ID</span>
              <span class="info-value">${this.user.uniqueId}</span>
            </div>
            <div class="info-item">
              <span class="info-label">Last Active</span>
              <span class="info-value">${lastActive}</span>
            </div>
            ${isAI && this.user.modelConfig?.provider ? `
            <div class="info-item">
              <span class="info-label">Provider</span>
              <span class="info-value">${this.user.modelConfig.provider}</span>
            </div>
            ` : ''}
            ${isAI && this.user.modelConfig?.model ? `
            <div class="info-item">
              <span class="info-label">Model</span>
              <span class="info-value">${this.user.modelConfig.model}</span>
            </div>
            ` : ''}
          </div>
        </div>
      </div>

      ${isAI ? `
      <div class="section">
        <div class="section-header">Cognitive Modules</div>
        <div class="section-content">
          <p class="cognitive-description">
            Access this AI's cognitive systems and memory
          </p>
          <div class="cognitive-links">
            <div class="cognitive-link" data-action="cognition">
              <span class="icon">🧠</span>
              <span class="label">Brain View</span>
            </div>
            <div class="cognitive-link" style="opacity: 0.5; cursor: not-allowed;">
              <span class="icon">🧬</span>
              <span class="label">Genome</span>
            </div>
            <div class="cognitive-link" style="opacity: 0.5; cursor: not-allowed;">
              <span class="icon">💾</span>
              <span class="label">Memory</span>
            </div>
          </div>
        </div>
      </div>
      ` : ''}

      <div class="section">
        <div class="section-header">Actions</div>
        <div class="section-content">
          <div class="actions">
            ${isFrozen ? `
              <button class="btn btn-primary" data-action="unfreeze">
                <span>❄️</span> Unfreeze User
              </button>
            ` : `
              <button class="btn btn-warning" data-action="freeze">
                <span>🥶</span> Freeze User
              </button>
            `}
            <button class="btn btn-danger" data-action="delete">
              <span>🗑️</span> Delete Permanently
            </button>
          </div>
          <p class="actions-note">
            <strong>Freeze:</strong> Hides user from chats and lists. Can be undone.<br>
            <strong>Delete:</strong> Permanently removes user. Cannot be undone.
          </p>
        </div>
      </div>
    `;
  }

  private setupEventListeners(): void {
    this.shadowRoot?.querySelectorAll('[data-action]').forEach(btn => {
      btn.addEventListener('click', (e) => {
        const action = (e.currentTarget as HTMLElement).dataset.action;
        switch (action) {
          case 'freeze':
            this.updateUserStatus('frozen');
            break;
          case 'unfreeze':
            this.updateUserStatus('offline');
            break;
          case 'delete':
            this.deleteUser();
            break;
          case 'cognition':
            this.openCognition();
            break;
        }
      });
    });
  }

  protected async onWidgetCleanup(): Promise<void> {
    this.verbose() && console.log('UserProfile: Cleanup complete');
  }
}

// Register the custom element
// Registration handled by centralized BROWSER_WIDGETS registry
