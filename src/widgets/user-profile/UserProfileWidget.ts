/**
 * UserProfileWidget - Full Visual Social Profile
 *
 * Facebook-level visual richness: full-bleed cover banner, gradient avatar
 * with initial, bio, pathway cards, writings feed, admin tools.
 *
 * Layout:
 * - Full-bleed cover banner (gradient or image)
 * - Avatar overlapping cover bottom edge (gradient+initial or image)
 * - About section (bio, joined, last active)
 * - Pathway cards grid (brain, genome, DM, logs, memory, stats)
 * - Writings feed (wall documents + social posts)
 * - Admin section (freeze/delete, collapsed by default)
 *
 * Right panel: DM chat with the persona being viewed (resolved dynamically).
 */

import {
  ReactiveWidget,
  html,
  unsafeCSS,
  reactive,
  type TemplateResult,
  type CSSResultGroup
} from '../shared/ReactiveWidget';
import { Events } from '../../system/core/shared/Events';
import { UI_EVENTS } from '../../system/core/shared/EventConstants';
import { ContentService } from '../../system/state/ContentService';
import { PositronWidgetState } from '../shared/services/state/PositronWidgetState';
import { getWidgetEntityId } from '../shared/WidgetConstants';
import type { UserEntity, UserStatus } from '../../system/data/entities/UserEntity';
import type { UserProfileEntity, UserVisualIdentity } from '../../system/data/entities/UserProfileEntity';
import type { WallDocumentEntity } from '../../system/data/entities/WallDocumentEntity';
import { DataRead } from '../../commands/data/read/shared/DataReadTypes';
import { DataList } from '../../commands/data/list/shared/DataListTypes';
import { DataUpdate } from '../../commands/data/update/shared/DataUpdateTypes';
import { DataDelete } from '../../commands/data/delete/shared/DataDeleteTypes';
import { Dm } from '../../commands/collaboration/dm/shared/DmTypes';
import { styles as PROFILE_STYLES } from './public/user-profile-widget.styles';

export class UserProfileWidget extends ReactiveWidget {
  static override styles = [
    ReactiveWidget.styles,
    unsafeCSS(PROFILE_STYLES)
  ] as CSSResultGroup;

  // === Reactive State ===
  @reactive() private user: UserEntity | null = null;
  @reactive() private profile: UserProfileEntity | null = null;
  @reactive() private adminExpanded = false;
  @reactive() private dmRoomId: string | null = null;
  @reactive() private wallDocs: WallDocumentEntity[] = [];
  @reactive() private feedLoading = false;

  constructor() {
    super({
      widgetName: 'UserProfileWidget',
      enableCommands: true,
      enablePositron: true
    });
  }

  // === Lifecycle ===

  protected override onFirstRender(): void {
    this.registerWidgetState({
      viewingUserId: null,
      userType: null,
      status: null
    });

    // Effect: resolve DM room when user changes (best-effort)
    this.createEffect(
      (w: UserProfileWidget) => w.user?.id,
      (userId) => {
        if (userId && this.user && this.user.type !== 'human') {
          this.resolveDmRoom(this.user);
        }
      }
    );
  }

  /**
   * Called by MainWidget when this widget is activated with a new entityId.
   * Implements clear/populate/query pattern for instant hydration.
   */
  public async onActivate(entityId?: string, metadata?: Record<string, unknown>): Promise<void> {
    this.verbose() && console.log(`UserProfile: onActivate entityId=${entityId}`);

    if (entityId) {
      this.setAttribute('entity-id', entityId);
    } else {
      this.removeAttribute('entity-id');
    }

    // Same entity? Just refresh
    if (this.user && (this.user.id === entityId || this.user.uniqueId === entityId)) {
      await this.loadUser();
      return;
    }

    // Different entity — clear old state
    this.user = null;
    this.profile = null;
    this.dmRoomId = null;
    this.wallDocs = [];
    this.feedLoading = false;
    this.loading = true;
    this.error = null;
    this.adminExpanded = false;

    // Instant hydration from metadata
    const preloaded = metadata?.entity as UserEntity;
    if (preloaded) {
      this.user = preloaded;
      this.loading = false;
      this.loadProfile(preloaded.id);
      this.loadWritings(preloaded.id);
      this.emitPositronContext();
      return;
    }

    // Full query
    await this.loadUser();
  }

  // === Data Loading ===

  private async loadUser(): Promise<void> {
    const entityId = getWidgetEntityId(this) || (this as any).pageState?.entityId;

    if (!entityId) {
      this.error = 'No user specified';
      this.loading = false;
      return;
    }

    try {
      // Try by ID first
      const result = await DataRead.execute<UserEntity>({
        collection: 'users',
        id: entityId,
        dbHandle: 'default'
      });

      if (result?.data) {
        this.user = result.data;
      } else {
        // Try by uniqueId
        const listResult = await DataList.execute<UserEntity>({
          collection: 'users',
          filter: { uniqueId: entityId },
          limit: 1,
          dbHandle: 'default'
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

    if (this.user) {
      this.loadProfile(this.user.id);
      this.loadWritings(this.user.id);
      this.emitPositronContext();
    }
  }

  private async loadProfile(userId: string): Promise<void> {
    try {
      const result = await DataList.execute<UserProfileEntity>({
        collection: 'user_profiles',
        filter: { userId },
        limit: 1,
        dbHandle: 'default'
      });

      if (result?.items?.[0]) {
        this.profile = result.items[0] as UserProfileEntity;
      }
    } catch (err) {
      // Profile is optional — don't set error
      this.verbose() && console.log('UserProfile: No profile entity found', err);
    }
  }

  /**
   * Load wall documents authored by this user.
   */
  private async loadWritings(userId: string): Promise<void> {
    this.feedLoading = true;

    try {
      const result = await DataList.execute<WallDocumentEntity>({
        collection: 'wall_documents',
        filter: { createdBy: userId },
        orderBy: [{ field: 'lastModifiedAt', direction: 'desc' }],
        limit: 10,
        dbHandle: 'default'
      });

      if (result?.items) {
        this.wallDocs = result.items as WallDocumentEntity[];
      }
    } catch (err) {
      this.verbose() && console.log('UserProfile: Failed to load writings', err);
    }

    this.feedLoading = false;
  }

  /**
   * Find or create a DM room with this user, then update the right panel
   */
  private async resolveDmRoom(user: UserEntity): Promise<void> {
    if (user.type === 'human') return; // Don't auto-DM humans

    try {
      const result = await Dm.execute({ participants: user.id });

      if (result?.success && result.roomId) {
        this.dmRoomId = result.roomId as string;
        this.requestUpdate();

        // Override the right panel to show this DM room
        Events.emit(UI_EVENTS.RIGHT_PANEL_CONFIGURE, {
          widget: 'chat-widget',
          room: result.uniqueId || result.roomId,
          compact: true,
          contentType: 'profile'
        });
      }
    } catch {
      // DM resolution is best-effort — card is always clickable as fallback
    }
  }

  // === Positron Context ===

  private emitPositronContext(): void {
    if (!this.user) return;

    this.updateWidgetState({
      viewingUserId: this.user.id,
      userType: this.user.type,
      status: this.user.status,
      displayName: this.user.displayName
    });

    PositronWidgetState.emitWidgetEvent('profile', 'status:changed', {
      userId: this.user.id,
      status: this.user.status,
      displayName: this.user.displayName,
      userType: this.user.type
    });
  }

  // === Actions ===

  private async updateUserStatus(newStatus: UserStatus): Promise<void> {
    if (!this.user) return;

    try {
      await DataUpdate.execute<UserEntity>({
        collection: 'users',
        id: this.user.id,
        data: { status: newStatus },
        dbHandle: 'default'
      });

      // Mutate + trigger re-render
      this.user = { ...this.user, status: newStatus } as UserEntity;
      Events.emit('data:users:updated', { id: this.user.id, status: newStatus });
      this.emitPositronContext();
    } catch (err) {
      console.error('Failed to update user status:', err);
    }
  }

  private async deleteUser(): Promise<void> {
    if (!this.user) return;

    if (this.currentUser?.id === this.user.id) {
      alert('Cannot delete your own account.');
      return;
    }

    if (!confirm(`Permanently delete ${this.user.displayName}? This cannot be undone.`)) {
      return;
    }

    try {
      await DataDelete.execute({
        collection: 'users',
        id: this.user.id,
        dbHandle: 'default'
      });

      Events.emit('data:users:deleted', { id: this.user.id });
      ContentService.open('chat', 'general', { title: 'General', uniqueId: 'general' });
    } catch (err) {
      console.error('Failed to delete user:', err);
    }
  }

  // === Navigation (Pathway Cards) ===

  private openBrain(): void {
    if (!this.user) return;
    ContentService.open('persona', this.user.id, {
      title: `${this.user.displayName} - Brain`,
      uniqueId: this.user.uniqueId || this.user.id,
      metadata: { entity: this.user }
    });
  }

  private async openDm(): Promise<void> {
    if (!this.user) return;

    // If we already have the DM room, open it directly
    if (this.dmRoomId) {
      ContentService.open('chat', this.dmRoomId, {
        title: `DM - ${this.user.displayName}`,
        uniqueId: `dm-${this.user.uniqueId || this.user.id}`
      });
      return;
    }

    // Otherwise create-or-find the DM room on-click
    try {
      const result = await Dm.execute({ participants: this.user.id });
      if (result?.success && result.roomId) {
        this.dmRoomId = result.roomId as string;
        ContentService.open('chat', result.roomId as string, {
          title: `DM - ${this.user.displayName}`,
          uniqueId: result.uniqueId || `dm-${this.user.uniqueId || this.user.id}`
        });
      }
    } catch (err) {
      console.error('UserProfile: Failed to create DM room:', err);
    }
  }

  private openLogs(): void {
    if (!this.user) return;
    const logPath = `personas/${this.user.uniqueId || this.user.id}/cognition`;
    ContentService.open('diagnostics-log', logPath, {
      title: `${this.user.displayName} - Logs`,
      uniqueId: `logs-${this.user.uniqueId || this.user.id}`
    });
  }

  // === Visual Identity Helpers ===

  private get visualIdentity(): UserVisualIdentity | undefined {
    return this.profile?.visualIdentity;
  }

  private get accentColor(): string {
    return this.visualIdentity?.accentColor || '#00d4ff';
  }

  /** Cover banner CSS: image URL or generated gradient from accent color */
  private get coverStyle(): string {
    const vi = this.visualIdentity;
    if (vi?.coverUrl) {
      return `background-image: url(${vi.coverUrl})`;
    }
    const gradient = vi?.coverGradient ||
      `linear-gradient(135deg, #0a0f1a, ${this.accentColor}15 30%, #0d1b2a 60%, ${this.accentColor}10)`;
    return `background: ${gradient}`;
  }

  /** Avatar gradient: radial gradient from accent color giving 3D sphere feel */
  private get avatarGradient(): string {
    const c = this.accentColor;
    return `radial-gradient(circle at 30% 30%, ${c}40, ${c}15 70%, transparent)`;
  }

  /** First letter of display name for gradient avatar */
  private get avatarInitial(): string {
    return this.user?.displayName?.charAt(0)?.toUpperCase() || '?';
  }

  // === Size Formatting ===

  private formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  private formatRelativeTime(date: Date | string): string {
    const d = date instanceof Date ? date : new Date(date);
    const now = Date.now();
    const diff = now - d.getTime();
    const minutes = Math.floor(diff / 60000);
    const hours = Math.floor(diff / 3600000);
    const days = Math.floor(diff / 86400000);

    if (minutes < 1) return 'just now';
    if (minutes < 60) return `${minutes}m ago`;
    if (hours < 24) return `${hours}h ago`;
    if (days < 30) return `${days}d ago`;
    return d.toLocaleDateString();
  }

  // === Rendering ===

  protected override renderContent(): TemplateResult {
    if (!this.user) {
      return html`<div class="loading">No user loaded</div>`;
    }

    const isAI = this.user.type === 'persona' || this.user.type === 'agent';

    return html`
      <div class="profile-page" style="--profile-accent: ${this.accentColor}">
        ${this.renderCover()}
        ${this.renderHeroProfile()}
        <div class="profile-content">
          ${this.renderAbout()}
          ${isAI ? this.renderPathways() : ''}
          ${this.renderWritings()}
          ${this.renderAdmin()}
        </div>
      </div>
    `;
  }

  private renderCover(): TemplateResult {
    return html`
      <div class="hero-cover" style="${this.coverStyle}"></div>
    `;
  }

  private renderHeroProfile(): TemplateResult {
    const user = this.user!;
    const isFrozen = user.status === 'frozen';
    const isAI = user.type === 'persona' || user.type === 'agent';
    const vi = this.visualIdentity;

    return html`
      <div class="hero-profile">
        <div class="hero-avatar ${isFrozen ? 'frozen' : ''}">
          ${vi?.avatarUrl
            ? html`<img class="avatar-image" src="${vi.avatarUrl}" alt="${user.displayName}" />`
            : html`
              <div class="avatar-gradient" style="background: ${this.avatarGradient}"></div>
              <span class="avatar-initial">${this.avatarInitial}</span>
            `
          }
          <span class="status-dot ${user.status}"></span>
        </div>
        <div class="hero-info">
          <h1 class="hero-name">${user.displayName}</h1>
          <div class="hero-identity">
            <span>@${user.uniqueId}</span>
            <span class="separator">&middot;</span>
            <span>${user.type}</span>
            <span class="separator">&middot;</span>
            <span>${user.status}</span>
          </div>
          ${user.shortDescription ? html`
            <p class="hero-description">"${user.shortDescription}"</p>
          ` : ''}
          <div class="hero-badges">
            ${isAI && user.modelConfig?.provider ? html`
              <span class="badge badge-provider">${user.modelConfig.provider}/${user.modelConfig.model || '?'}</span>
            ` : ''}
            ${this.profile?.speciality ? html`
              <span class="badge badge-speciality">${this.profile.speciality}</span>
            ` : ''}
            <span class="badge badge-type ${user.type}">${user.type}</span>
          </div>
        </div>
      </div>
    `;
  }

  private renderAbout(): TemplateResult {
    const user = this.user!;
    const bio = this.profile?.bio || user.shortDescription || '';
    const joinedAt = this.profile?.joinedAt || user.createdAt;
    const location = this.profile?.location;
    const lastActive = user.lastActiveAt
      ? new Date(user.lastActiveAt).toLocaleDateString()
      : 'Never';

    return html`
      <div class="about">
        <div class="section-header">About</div>
        ${bio ? html`<p class="about-bio">${bio}</p>` : ''}
        <div class="about-meta">
          <div class="meta-item">
            <span class="meta-label">Last Active</span>
            <span class="meta-value">${lastActive}</span>
          </div>
          ${joinedAt ? html`
            <div class="meta-item">
              <span class="meta-label">Joined</span>
              <span class="meta-value">${new Date(joinedAt).toLocaleDateString()}</span>
            </div>
          ` : ''}
          ${location ? html`
            <div class="meta-item">
              <span class="meta-label">Location</span>
              <span class="meta-value">${location}</span>
            </div>
          ` : ''}
          ${user.modelConfig?.provider ? html`
            <div class="meta-item">
              <span class="meta-label">Provider</span>
              <span class="meta-value">${user.modelConfig.provider}</span>
            </div>
          ` : ''}
        </div>
      </div>
    `;
  }

  private renderPathways(): TemplateResult {
    return html`
      <div>
        <div class="section-header">Pathways</div>
        <div class="pathways">
          <div class="pathway-card" @click=${() => this.openBrain()}>
            <span class="pathway-icon">🧠</span>
            <span class="pathway-title">Brain</span>
            <span class="pathway-subtitle">Cognitive View</span>
          </div>
          <div class="pathway-card disabled">
            <span class="pathway-icon">🧬</span>
            <span class="pathway-title">Genome</span>
            <span class="pathway-subtitle">Adapters & Layers</span>
          </div>
          <div class="pathway-card" @click=${() => this.openDm()}>
            <span class="pathway-icon">💬</span>
            <span class="pathway-title">DM</span>
            <span class="pathway-subtitle">Message Directly</span>
          </div>
          <div class="pathway-card" @click=${() => this.openLogs()}>
            <span class="pathway-icon">📋</span>
            <span class="pathway-title">Logs</span>
            <span class="pathway-subtitle">Cognition Logs</span>
          </div>
          <div class="pathway-card disabled">
            <span class="pathway-icon">💾</span>
            <span class="pathway-title">Memory</span>
            <span class="pathway-subtitle">Long-term Store</span>
          </div>
          <div class="pathway-card disabled">
            <span class="pathway-icon">📊</span>
            <span class="pathway-title">Stats</span>
            <span class="pathway-subtitle">Activity Metrics</span>
          </div>
        </div>
      </div>
    `;
  }

  private renderWritings(): TemplateResult {
    return html`
      <div>
        <div class="section-header">Writings</div>
        <div class="writings">
          ${this.feedLoading ? html`
            <div class="writings-loading">Loading writings...</div>
          ` : this.wallDocs.length === 0 ? html`
            <div class="writings-empty">No published writings yet.</div>
          ` : html`
            ${this.wallDocs.map(doc => this.renderWallDocCard(doc))}
          `}
        </div>
      </div>
    `;
  }

  private renderWallDocCard(doc: WallDocumentEntity): TemplateResult {
    return html`
      <div class="feed-card">
        <div class="feed-card-header">
          <span class="feed-card-icon">📝</span>
          <span class="feed-card-type">Wall Document</span>
        </div>
        <div class="feed-card-title">${doc.name}</div>
        <div class="feed-card-meta">
          <span class="feed-card-stat">${doc.lineCount} lines</span>
          <span class="feed-card-stat">${this.formatBytes(doc.byteCount)}</span>
          <span class="feed-card-stat">${this.formatRelativeTime(doc.lastModifiedAt)}</span>
        </div>
      </div>
    `;
  }

  private renderAdmin(): TemplateResult {
    const user = this.user!;
    const isFrozen = user.status === 'frozen';

    return html`
      <div class="admin">
        <button class="admin-toggle" @click=${() => { this.adminExpanded = !this.adminExpanded; }}>
          <span>Administration</span>
          <span class="chevron ${this.adminExpanded ? 'open' : ''}">▼</span>
        </button>
        ${this.adminExpanded ? html`
          <div class="admin-content">
            <div class="admin-actions">
              ${isFrozen ? html`
                <button class="btn btn-primary" @click=${() => this.updateUserStatus('offline')}>
                  Unfreeze
                </button>
              ` : html`
                <button class="btn btn-warning" @click=${() => this.updateUserStatus('frozen')}>
                  Freeze
                </button>
              `}
              <button class="btn btn-danger" @click=${() => this.deleteUser()}>
                Delete
              </button>
            </div>
            <p class="admin-note">
              <strong>Freeze:</strong> Hides from chats and lists. Reversible.<br>
              <strong>Delete:</strong> Permanent removal. Cannot be undone.
            </p>
          </div>
        ` : ''}
      </div>
    `;
  }

  protected override onDisconnect(): void {
    this.verbose() && console.log('UserProfile: Cleanup');
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
