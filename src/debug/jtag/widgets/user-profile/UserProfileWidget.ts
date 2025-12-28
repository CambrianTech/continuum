/**
 * UserProfileWidget - Universal user profile view
 *
 * Works for ALL user types: humans, personas, agents
 * Shows: name, type, status, last active, stats
 * Actions: Edit, Freeze/Unfreeze, Delete
 * For PersonaUsers: Links to cognitive views (brain, genome, memory)
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

export class UserProfileWidget extends BaseWidget {
  private user: UserEntity | null = null;
  private loading = true;
  private error: string | null = null;

  constructor() {
    super({
      widgetName: 'UserProfileWidget',
      template: undefined,
      styles: undefined,
      enableAI: false,
      enableDatabase: true,
      enableRouterEvents: false,
      enableScreenshots: false
    });
  }

  protected async onWidgetInitialize(): Promise<void> {
    console.log('UserProfile: Initializing...');
    await this.loadUser();
  }

  private async loadUser(): Promise<void> {
    const entityId = this.getAttribute('data-entity-id');
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
        const listResult = await Commands.execute<DataListParams<UserEntity>, DataListResult<UserEntity>>(DATA_COMMANDS.LIST, {
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
   */
  private emitPositronContext(): void {
    if (!this.user) return;

    PositronWidgetState.emit(
      {
        widgetType: 'profile',
        section: this.user.type,
        title: `Profile - ${this.user.displayName}`,
        entityId: this.user.id,
        metadata: {
          userType: this.user.type,
          userName: this.user.displayName,
          userStatus: this.user.status,
          isAI: this.user.type === 'persona' || this.user.type === 'agent',
          hasModelConfig: !!this.user.modelConfig
        }
      },
      { action: 'viewing', target: `${this.user.type} profile` }
    );

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

      // Navigate back to chat
      Events.emit('content:opened', {
        contentType: 'chat',
        entityId: 'general',
        title: 'General',
        setAsCurrent: true
      });
    } catch (err) {
      console.error('Failed to delete user:', err);
    }
  }

  private async openCognition(): Promise<void> {
    if (!this.user) return;

    Events.emit('content:opened', {
      contentType: 'persona',
      entityId: this.user.uniqueId || this.user.id,
      title: `${this.user.displayName} - Brain`,
      setAsCurrent: true
    });

    // Persist to server
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
    const styles = `
      :host {
        display: block;
        height: 100%;
        overflow: auto;
      }

      .profile-container {
        max-width: 800px;
        margin: 0 auto;
        padding: 32px;
      }

      .loading, .error {
        display: flex;
        align-items: center;
        justify-content: center;
        height: 200px;
        color: rgba(255, 255, 255, 0.6);
        font-size: 16px;
      }

      .error {
        color: #ff6b6b;
      }

      .profile-header {
        display: flex;
        align-items: center;
        gap: 24px;
        padding: 24px;
        background: rgba(10, 15, 20, 0.95);
        border: 1px solid rgba(0, 212, 255, 0.2);
        border-radius: 12px;
        margin-bottom: 24px;
      }

      .avatar {
        width: 80px;
        height: 80px;
        border-radius: 50%;
        background: rgba(0, 212, 255, 0.2);
        display: flex;
        align-items: center;
        justify-content: center;
        font-size: 40px;
        border: 2px solid rgba(0, 212, 255, 0.4);
      }

      .avatar.frozen {
        opacity: 0.5;
        filter: grayscale(1);
      }

      .user-details {
        flex: 1;
      }

      .user-name {
        font-size: 28px;
        font-weight: 600;
        color: white;
        margin: 0 0 8px 0;
      }

      .user-meta {
        display: flex;
        gap: 12px;
        flex-wrap: wrap;
      }

      .badge {
        padding: 4px 10px;
        border-radius: 12px;
        font-size: 12px;
        text-transform: uppercase;
        font-weight: 600;
        letter-spacing: 0.5px;
      }

      .badge.type-human { background: #4CAF50; color: white; }
      .badge.type-persona { background: #9C27B0; color: white; }
      .badge.type-agent { background: #2196F3; color: white; }
      .badge.type-system { background: #607D8B; color: white; }

      .badge.status-online { background: #4CAF50; color: white; }
      .badge.status-offline { background: rgba(100, 100, 100, 0.5); color: rgba(255,255,255,0.7); }
      .badge.status-frozen { background: #00bcd4; color: white; }
      .badge.status-deleted { background: #f44336; color: white; }

      .section {
        background: rgba(10, 15, 20, 0.95);
        border: 1px solid rgba(0, 212, 255, 0.2);
        border-radius: 12px;
        margin-bottom: 24px;
        overflow: hidden;
      }

      .section-header {
        padding: 16px 20px;
        background: rgba(0, 212, 255, 0.1);
        border-bottom: 1px solid var(--border-accent, rgba(0, 212, 255, 0.2));
        font-size: 14px;
        font-weight: 600;
        color: var(--content-accent, #00d4ff);
        text-transform: uppercase;
        letter-spacing: 1px;
      }

      .section-content {
        padding: 20px;
      }

      .info-grid {
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
        gap: 16px;
      }

      .info-item {
        display: flex;
        flex-direction: column;
        gap: 4px;
      }

      .info-label {
        font-size: 12px;
        color: rgba(255, 255, 255, 0.5);
        text-transform: uppercase;
        letter-spacing: 0.5px;
      }

      .info-value {
        font-size: 16px;
        color: white;
      }

      .actions {
        display: flex;
        gap: 12px;
        flex-wrap: wrap;
      }

      .btn {
        padding: 12px 24px;
        border-radius: 8px;
        font-size: 14px;
        font-weight: 600;
        cursor: pointer;
        border: none;
        transition: all 0.15s ease;
        display: flex;
        align-items: center;
        gap: 8px;
      }

      .btn-primary {
        background: var(--button-primary-background, #00d4ff);
        color: var(--button-primary-text, #000);
      }
      .btn-primary:hover {
        background: var(--button-primary-hover, #00e5ff);
        transform: translateY(-1px);
      }

      .btn-secondary {
        background: rgba(0, 212, 255, 0.2);
        color: var(--content-accent, #00d4ff);
        border: 1px solid var(--border-accent, rgba(0, 212, 255, 0.3));
      }
      .btn-secondary:hover {
        background: rgba(0, 212, 255, 0.3);
      }

      .btn-warning {
        background: rgba(255, 152, 0, 0.2);
        color: #ff9800;
        border: 1px solid rgba(255, 152, 0, 0.3);
      }
      .btn-warning:hover {
        background: rgba(255, 152, 0, 0.3);
      }

      .btn-danger {
        background: rgba(244, 67, 54, 0.2);
        color: #f44336;
        border: 1px solid rgba(244, 67, 54, 0.3);
      }
      .btn-danger:hover {
        background: rgba(244, 67, 54, 0.3);
      }

      .cognitive-links {
        display: flex;
        gap: 12px;
        margin-top: 12px;
      }

      .cognitive-link {
        padding: 16px 24px;
        background: rgba(156, 39, 176, 0.15);
        border: 1px solid rgba(156, 39, 176, 0.3);
        border-radius: 8px;
        color: #ce93d8;
        cursor: pointer;
        transition: all 0.15s ease;
        text-align: center;
      }
      .cognitive-link:hover {
        background: rgba(156, 39, 176, 0.25);
        transform: translateY(-2px);
      }
      .cognitive-link .icon {
        font-size: 24px;
        display: block;
        margin-bottom: 8px;
      }
      .cognitive-link .label {
        font-size: 14px;
        font-weight: 600;
      }
    `;

    let content = '';

    if (this.loading) {
      content = '<div class="loading">Loading user...</div>';
    } else if (this.error) {
      content = `<div class="error">${this.error}</div>`;
    } else if (this.user) {
      const avatar = this.user.type === 'human' ? '👤' :
                    this.user.type === 'agent' ? '🤖' :
                    this.user.type === 'persona' ? '⭐' : '⚙️';

      const isFrozen = this.user.status === 'frozen';
      const isDeleted = this.user.status === 'deleted';
      const isAI = this.user.type === 'persona' || this.user.type === 'agent';

      const lastActive = this.user.lastActiveAt
        ? new Date(this.user.lastActiveAt).toLocaleString()
        : 'Never';

      content = `
        <div class="profile-container">
          <div class="profile-header">
            <div class="avatar ${isFrozen ? 'frozen' : ''}">${avatar}</div>
            <div class="user-details">
              <h1 class="user-name">${this.user.displayName}</h1>
              <div class="user-meta">
                <span class="badge type-${this.user.type}">${this.user.type}</span>
                <span class="badge status-${this.user.status}">${this.user.status}</span>
                ${this.user.shortDescription ? `<span style="color: rgba(255,255,255,0.6)">${this.user.shortDescription}</span>` : ''}
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
              <p style="color: rgba(255,255,255,0.6); margin: 0 0 16px 0;">
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
              <p style="color: rgba(255,255,255,0.5); font-size: 13px; margin: 16px 0 0 0;">
                <strong>Freeze:</strong> Hides user from chats and lists. Can be undone.<br>
                <strong>Delete:</strong> Permanently removes user. Cannot be undone.
              </p>
            </div>
          </div>
        </div>
      `;
    }

    this.shadowRoot!.innerHTML = `<style>${styles}</style>${content}`;
    this.setupEventListeners();
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
    console.log('UserProfile: Cleanup complete');
  }
}

// Register the custom element
customElements.define('user-profile-widget', UserProfileWidget);
