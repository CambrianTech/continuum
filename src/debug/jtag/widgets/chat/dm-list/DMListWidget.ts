/**
 * DM List Widget - Shows direct message conversations
 *
 * For 2-person DMs: Shows the other person's name
 * For group DMs (3+): Shows names + member count
 *
 * Uses ReactiveListWidget with header/item/footer pattern
 */

import {
  ReactiveListWidget,
  html,
  reactive,
  unsafeCSS,
  type TemplateResult,
  type CSSResultGroup
} from '../../shared/ReactiveListWidget';
import { RoomEntity, type RoomMember } from '../../../system/data/entities/RoomEntity';
import { UserEntity } from '../../../system/data/entities/UserEntity';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { pageState } from '../../../system/state/PageStateService';
import { ContentService } from '../../../system/state/ContentService';
import { Commands } from '../../../system/core/shared/Commands';
import { DATA_COMMANDS } from '../../../commands/data/shared/DataCommandConstants';
import type { DataListParams, DataListResult } from '../../../commands/data/list/shared/DataListTypes';
import { COMMANDS } from '../../../shared/generated-command-constants';
import type { CollaborationLiveStartParams, CollaborationLiveStartResult } from '../../../commands/collaboration/live/start/shared/CollaborationLiveStartTypes';

const styles = `
  :host {
    display: block;
    width: 100%;
  }

  .entity-list-container {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .entity-list-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--spacing-sm, 8px) var(--spacing-md, 12px);
    border-bottom: 1px solid var(--border-color, rgba(255,255,255,0.1));
    font-size: 0.75rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-secondary, #aaa);
  }

  .list-count {
    background: var(--surface-secondary, #2a2a4a);
    padding: 2px 6px;
    border-radius: var(--radius-sm, 4px);
    font-size: 0.65rem;
  }

  .entity-list-body {
    flex: 1;
    overflow-y: auto;
    padding: var(--spacing-sm, 8px) 0;
  }

  .dm-item {
    display: flex;
    align-items: center;
    gap: var(--spacing-sm, 8px);
    padding: var(--spacing-sm, 8px) var(--spacing-md, 12px);
    cursor: pointer;
    transition: background 0.15s ease;
    border-radius: var(--radius-sm, 4px);
    margin: 0 var(--spacing-xs, 4px);
  }

  .dm-item:hover {
    background: var(--surface-hover, rgba(255,255,255,0.05));
  }

  .dm-item.active {
    background: var(--accent-color-dim, rgba(0,200,255,0.15));
  }

  .dm-avatar {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    background: var(--surface-secondary, #2a2a4a);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 0.875rem;
    font-weight: 500;
    flex-shrink: 0;
  }

  .dm-avatar.group {
    border-radius: var(--radius-sm, 4px);
  }

  .dm-info {
    flex: 1;
    min-width: 0;
    overflow: hidden;
  }

  .dm-name {
    font-size: 0.875rem;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text-primary, #fff);
  }

  .dm-members {
    font-size: 0.75rem;
    color: var(--text-secondary, #aaa);
    margin-top: 2px;
  }

  .empty-state {
    padding: var(--spacing-lg, 24px);
    text-align: center;
    color: var(--text-secondary, #aaa);
    font-size: 0.8rem;
  }

  .new-dm-btn {
    margin: var(--spacing-sm, 8px);
    padding: var(--spacing-sm, 8px);
    background: var(--surface-secondary, #2a2a4a);
    border: 1px dashed var(--border-color, rgba(255,255,255,0.2));
    border-radius: var(--radius-sm, 4px);
    color: var(--text-secondary, #aaa);
    cursor: pointer;
    font-size: 0.75rem;
    transition: all 0.15s ease;
    text-align: center;
  }

  .new-dm-btn:hover {
    background: var(--surface-hover, rgba(255,255,255,0.05));
    color: var(--text-primary, #fff);
  }

  .voice-btn {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    background: transparent;
    border: none;
    color: var(--text-secondary, #aaa);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.15s ease;
    flex-shrink: 0;
    opacity: 0;
  }

  .dm-item:hover .voice-btn {
    opacity: 1;
  }

  .voice-btn:hover {
    background: var(--accent-color-dim, rgba(0,200,255,0.2));
    color: var(--accent-color, #00c8ff);
  }

  .voice-btn.active {
    opacity: 1;
    background: var(--success-color-dim, rgba(67,181,129,0.2));
    color: var(--success-color, #43b581);
    animation: pulse 2s infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.7; }
  }

  .voice-icon {
    font-size: 14px;
  }
`;

export class DMListWidget extends ReactiveListWidget<RoomEntity> {
  readonly collection = RoomEntity.collection;

  @reactive() private currentRoomId: UUID = '' as UUID;
  @reactive() private userCache = new Map<string, UserEntity>();
  @reactive() private activeVoiceRoomId: UUID | null = null;

  static override styles = [
    ReactiveListWidget.styles,
    unsafeCSS(styles)
  ] as CSSResultGroup;

  constructor() {
    super({ widgetName: 'DMListWidget' });
  }

  // === CONFIGURATION ===
  protected override get listTitle(): string { return 'Direct Messages'; }
  protected override get containerClass(): string { return 'entity-list-body'; }

  // Filter at database level - only load DM rooms
  protected override get loadFilter(): Record<string, unknown> {
    return { type: 'direct' };
  }

  // === HEADER ===
  protected override renderHeader(): TemplateResult {
    return html`
      <div class="entity-list-header">
        <span class="list-title">${this.listTitle}</span>
        <span class="list-count">${this.entityCount}</span>
      </div>
    `;
  }

  // === ITEM ===
  renderItem(room: RoomEntity): TemplateResult {
    const isActive = room.id === this.currentRoomId;
    const memberCount = room.members?.length || 0;
    const isGroup = memberCount > 2;
    const isVoiceActive = room.id === this.activeVoiceRoomId;

    // Get display name based on member count
    const displayInfo = this.getDMDisplayInfo(room);

    return html`
      <div class="dm-item ${isActive ? 'active' : ''}"
           data-room-id="${room.id}"
           @click=${() => this.selectRoom(room)}>
        <div class="dm-avatar ${isGroup ? 'group' : ''}">
          ${displayInfo.avatar}
        </div>
        <div class="dm-info">
          <div class="dm-name">${displayInfo.name}</div>
          ${isGroup ? html`<div class="dm-members">${memberCount} members</div>` : ''}
        </div>
        <button class="voice-btn ${isVoiceActive ? 'active' : ''}"
                title="${isVoiceActive ? 'Leave call' : 'Start voice call'}"
                @click=${(e: Event) => this.toggleVoice(e, room)}>
          <span class="voice-icon">${isVoiceActive ? '🔊' : '📞'}</span>
        </button>
      </div>
    `;
  }

  // === MAIN RENDER ===
  override render(): TemplateResult {
    return html`
      <div class="entity-list-container">
        ${this.renderHeader()}
        <div class="${this.containerClass}">
          ${this.entityCount === 0 ? html`
            <div class="empty-state">
              No direct messages yet
            </div>
          ` : ''}
        </div>
        <div class="new-dm-btn" @click=${this.startNewDM}>
          + Start a conversation
        </div>
        ${this.renderFooter()}
      </div>
    `;
  }

  // === FILTERING - Only show DMs ===
  protected override shouldAddEntity(room: RoomEntity): boolean {
    // Only show direct/DM rooms
    if (room.type !== 'direct') return false;
    if ((room.tags ?? []).includes('system')) return false;
    return true;
  }

  // === DM DISPLAY INFO ===
  private getDMDisplayInfo(room: RoomEntity): { name: string; avatar: string } {
    const members = room.members || [];
    const currentUserId = this.currentUser?.id;

    if (members.length === 2) {
      // 1-on-1 DM: Show the other person's name
      const otherMember = members.find(m => m.userId !== currentUserId);
      if (otherMember) {
        const user = this.userCache.get(otherMember.userId);
        if (user) {
          return {
            name: user.displayName || user.uniqueId,
            avatar: user.displayName?.charAt(0).toUpperCase() || '?'
          };
        }
      }
      // Fallback to room name if user not cached
      return {
        name: room.displayName || room.name,
        avatar: (room.displayName || room.name).charAt(0).toUpperCase()
      };
    }

    // Group DM: Show first two names + count
    const firstTwoUsers = members.slice(0, 2)
      .map(m => this.userCache.get(m.userId))
      .filter(Boolean)
      .map(u => u!.displayName || u!.uniqueId);

    if (firstTwoUsers.length > 0) {
      const remaining = members.length - firstTwoUsers.length;
      const name = remaining > 0
        ? `${firstTwoUsers.join(', ')} +${remaining}`
        : firstTwoUsers.join(', ');
      return { name, avatar: `${members.length}` };
    }

    // Fallback
    return {
      name: room.displayName || room.name,
      avatar: `${members.length}`
    };
  }

  // === LIFECYCLE ===
  protected override async onFirstRender(): Promise<void> {
    await super.onFirstRender();

    // Subscribe to pageState for current room tracking
    this.createMountEffect(() => {
      const unsubscribe = pageState.subscribe((state) => {
        if (state.contentType === 'chat' && state.entityId) {
          const matchingRoom = this.entities.find(
            (room: RoomEntity) => room.id === state.entityId || room.uniqueId === state.entityId
          );
          const newRoomId = matchingRoom?.id as UUID || state.entityId as UUID;
          if (newRoomId !== this.currentRoomId) {
            this.currentRoomId = newRoomId;
          }
        }
      });
      return () => unsubscribe();
    });

    // Load user info for DM display
    await this.loadMemberUsers();
  }

  // === LOAD USER INFO ===
  private async loadMemberUsers(): Promise<void> {
    // Collect all member user IDs from DM rooms
    const userIds = new Set<string>();
    for (const room of this.entities) {
      for (const member of room.members || []) {
        userIds.add(member.userId);
      }
    }

    if (userIds.size === 0) return;

    // Fetch all users in one query
    try {
      const result = await Commands.execute<DataListParams, DataListResult<UserEntity>>(
        DATA_COMMANDS.LIST,
        {
          collection: UserEntity.collection,
          filter: { id: { $in: Array.from(userIds) } },
          limit: userIds.size
        }
      );

      if (result.success && result.items) {
        for (const user of result.items) {
          this.userCache.set(user.id, user);
        }
        // Trigger re-render with user info
        this.requestUpdate();
      }
    } catch (error) {
      console.warn('DMListWidget: Failed to load member users', error);
    }
  }

  // === ROOM SELECTION ===
  private selectRoom(room: RoomEntity): void {
    const roomId = room.id as UUID;

    if (pageState.contentType === 'chat' && pageState.entityId === roomId) {
      return;
    }

    this.currentRoomId = roomId;

    if (this.currentUser?.id) {
      ContentService.setUserId(this.currentUser.id as UUID);
    }

    // Use ContentService.open() like RoomListWidget
    ContentService.open('chat', roomId, {
      title: room.displayName || room.name,
      subtitle: `${(room.members?.length || 0)} members`,
      uniqueId: room.uniqueId || room.name || roomId,
      metadata: { entity: room }
    });
  }

  // === START NEW DM ===
  private startNewDM(): void {
    // TODO: Open user picker dialog to start new DM
    console.log('DMListWidget: Start new DM - user picker not implemented yet');
  }

  // === VOICE CALL ===
  private async toggleVoice(e: Event, room: RoomEntity): Promise<void> {
    e.stopPropagation(); // Don't select the room when clicking voice button

    if (this.activeVoiceRoomId === room.id) {
      // Leave current call
      await this.leaveVoice(room);
    } else {
      // Start/join voice call
      await this.startVoice(room);
    }
  }

  private async startVoice(room: RoomEntity): Promise<void> {
    console.log('DMListWidget: Starting voice call for room:', room.displayName || room.name);

    // Get participant user IDs (excluding self)
    const participants = (room.members || [])
      .map(m => m.userId)
      .filter(id => id !== this.currentUser?.id);

    if (participants.length === 0) {
      console.warn('DMListWidget: No other participants in room');
      return;
    }

    try {
      const result = await Commands.execute<CollaborationLiveStartParams, CollaborationLiveStartResult>(
        COMMANDS.COLLABORATION_LIVE_START,
        {
          participants,
          name: room.displayName || room.name
        }
      );

      if (result.success) {
        this.activeVoiceRoomId = room.id as UUID;
        console.log('DMListWidget: Voice call started:', result.message);

        // Open the live widget in the center panel
        // Use room.id directly as uniqueId to keep URLs clean
        ContentService.open('live', room.id as UUID, {
          title: `Voice: ${room.displayName || room.name}`,
          subtitle: `${participants.length + 1} participants`,
          uniqueId: room.id,  // Room ID is the canonical identifier
          metadata: { room, session: result.session }
        });
      } else {
        console.error('DMListWidget: Failed to start voice call:', result.message);
      }
    } catch (error) {
      console.error('DMListWidget: Error starting voice call:', error);
    }
  }

  private async leaveVoice(room: RoomEntity): Promise<void> {
    console.log('DMListWidget: Leaving voice call for room:', room.displayName || room.name);

    // TODO: Call collaboration/live/leave command when implemented
    this.activeVoiceRoomId = null;

    // Navigate back to chat
    this.selectRoom(room);
  }
}

// Register the custom element
if (typeof customElements !== 'undefined' && !customElements.get('dm-list-widget')) {
  customElements.define('dm-list-widget', DMListWidget);
}
