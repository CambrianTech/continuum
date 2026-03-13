/**
 * Room List Widget - Unified rooms + DMs with filter chips
 *
 * Uses ReactiveListWidget with header/item/footer pattern.
 * Absorbs former DMListWidget functionality:
 * - Filter chips: All | Rooms | DMs
 * - DM display (other user's name, avatar, voice button)
 * - Member user cache for display names
 */

import {
  ReactiveListWidget,
  html,
  reactive,
  unsafeCSS,
  type TemplateResult,
  type CSSResultGroup
} from '../../shared/ReactiveListWidget';
import { RoomEntity } from '../../../system/data/entities/RoomEntity';
import { UserEntity } from '../../../system/data/entities/UserEntity';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { DEFAULT_ROOMS } from '../../../system/data/domains/DefaultEntities';
import { pageState } from '../../../system/state/PageStateService';
import { ContentService } from '../../../system/state/ContentService';
import { Commands } from '../../../system/core/shared/Commands';
import { DataList } from '../../../commands/data/list/shared/DataListTypes';
import { COMMANDS } from '../../../shared/generated-command-constants';
import type { CollaborationLiveStartParams, CollaborationLiveStartResult } from '../../../commands/collaboration/live/start/shared/CollaborationLiveStartTypes';

import { styles as externalStyles } from './room-list-widget.styles';

type RoomFilter = 'all' | 'rooms' | 'dms';

export class RoomListWidget extends ReactiveListWidget<RoomEntity> {
  readonly collection = RoomEntity.collection;

  // Always fetch rooms from server — localStorage cache goes stale after reseed
  // and 'auto' backend returns cached data without ever hitting the server.
  protected override get loadBackend(): 'server' { return 'server'; }

  @reactive() private currentRoomId: UUID = DEFAULT_ROOMS.GENERAL as UUID;
  @reactive() private activeFilter: RoomFilter = 'all';
  @reactive() private userCache = new Map<string, UserEntity>();
  @reactive() private activeVoiceRoomId: UUID | null = null;

  static override styles = [
    ReactiveListWidget.styles,
    unsafeCSS(externalStyles)
  ] as CSSResultGroup;

  constructor() {
    super({ widgetName: 'RoomListWidget' });
  }

  // === CONFIGURATION ===
  protected override get listTitle(): string { return 'Rooms'; }
  protected override get containerClass(): string { return 'entity-list-body'; }

  // === HEADER with filter chips ===
  protected override renderHeader(): TemplateResult {
    const filters: { id: RoomFilter; label: string }[] = [
      { id: 'all', label: 'All' },
      { id: 'rooms', label: 'Rooms' },
      { id: 'dms', label: 'DMs' }
    ];

    return html`
      <div class="entity-list-header">
        <span class="list-title">${this.listTitle}</span>
        <div class="filter-chips">
          ${filters.map(f => html`
            <button
              class="filter-chip ${this.activeFilter === f.id ? 'active' : ''}"
              @click=${() => this.setFilter(f.id)}
            >${f.label}</button>
          `)}
        </div>
        <span class="list-count">${this.entityCount}</span>
      </div>
    `;
  }

  private setFilter(filter: RoomFilter): void {
    if (this.activeFilter === filter) return;
    this.activeFilter = filter;
    this.scroller?.clear();
    this.scroller?.load();
  }

  // === ITEM ===
  renderItem(room: RoomEntity): TemplateResult {
    if (this.isDM(room)) {
      return this.renderDMItem(room);
    }
    return this.renderRoomItem(room);
  }

  private renderRoomItem(room: RoomEntity): TemplateResult {
    const isActive = room.id === this.currentRoomId;
    return html`
      <div class="room-item ${isActive ? 'active' : ''}"
           data-room-id="${room.id}"
           @click=${() => this.selectRoom(room)}>
        <div class="room-info">
          <div class="room-name">${room.displayName ?? room.name}</div>
          <div class="room-topic">${room.topic ?? ''}</div>
        </div>
      </div>
    `;
  }

  private renderDMItem(room: RoomEntity): TemplateResult {
    const isActive = room.id === this.currentRoomId;
    const memberCount = room.members?.length || 0;
    const isGroup = memberCount > 2;
    const isVoiceActive = room.id === this.activeVoiceRoomId;
    const displayInfo = this.getDMDisplayInfo(room);

    return html`
      <div class="dm-item ${isActive ? 'active' : ''}"
           data-room-id="${room.id}"
           @click=${() => this.selectRoom(room)}>
        <div class="dm-avatar ${isGroup ? 'group' : ''}">${displayInfo.avatar}</div>
        <div class="dm-info">
          <div class="dm-name">${displayInfo.name}</div>
          ${isGroup ? html`<div class="dm-members">${memberCount} members</div>` : ''}
        </div>
        <button class="voice-btn ${isVoiceActive ? 'active' : ''}"
                title="${isVoiceActive ? 'Leave call' : 'Start voice call'}"
                @click=${(e: Event) => this.toggleVoice(e, room)}>
          <span class="voice-icon">
            ${isVoiceActive
              ? html`<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon>
                  <path d="M15.54 8.46a5 5 0 0 1 0 7.07"></path>
                  <path d="M19.07 4.93a10 10 0 0 1 0 14.14"></path>
                </svg>`
              : html`<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.5 19.5 0 0 1-6-6 19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 4.11 2h3a2 2 0 0 1 2 1.72 12.84 12.84 0 0 0 .7 2.81 2 2 0 0 1-.45 2.11L8.09 9.91a16 16 0 0 0 6 6l1.27-1.27a2 2 0 0 1 2.11-.45 12.84 12.84 0 0 0 2.81.7A2 2 0 0 1 22 16.92z"></path>
                </svg>`
            }
          </span>
        </button>
      </div>
    `;
  }

  // === MAIN RENDER ===
  override render(): TemplateResult {
    const showNewDM = this.activeFilter === 'dms' || this.activeFilter === 'all';
    const hasDMs = this.entities.some(r => this.isDM(r));

    return html`
      <div class="entity-list-container">
        ${this.renderHeader()}
        <div class="${this.containerClass}"></div>
        ${showNewDM && hasDMs ? html`
          <div class="new-dm-btn" @click=${this.startNewDM}>+ Start a conversation</div>
        ` : ''}
        ${this.renderFooter()}
      </div>
    `;
  }

  // === FILTERING ===
  private isDM(room: RoomEntity): boolean {
    return room.type === 'direct' || (room.tags ?? []).includes('dm');
  }

  protected override shouldAddEntity(room: RoomEntity): boolean {
    // Always hide system rooms
    if ((room.tags ?? []).includes('system')) return false;

    // Apply filter
    switch (this.activeFilter) {
      case 'rooms':
        return !this.isDM(room);
      case 'dms':
        return this.isDM(room);
      case 'all':
      default:
        return true;
    }
  }

  // === DM DISPLAY INFO ===
  private getDMDisplayInfo(room: RoomEntity): { name: string; avatar: string } {
    const members = room.members || [];
    const currentUserId = this.currentUser?.id;

    if (members.length === 2) {
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
      return {
        name: room.displayName || room.name,
        avatar: (room.displayName || room.name).charAt(0).toUpperCase()
      };
    }

    // Group DM: first two names + count
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

    return {
      name: room.displayName || room.name,
      avatar: `${members.length}`
    };
  }

  // === LIFECYCLE ===
  protected override async onFirstRender(): Promise<void> {
    await super.onFirstRender();

    // Subscribe to pageState - single source of truth for current room
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

    // Load user info for DM display names
    await this.loadMemberUsers();
  }

  // === LOAD USER INFO FOR DMs ===
  private async loadMemberUsers(): Promise<void> {
    const userIds = new Set<string>();
    for (const room of this.entities) {
      if (this.isDM(room)) {
        for (const member of room.members || []) {
          userIds.add(member.userId);
        }
      }
    }

    if (userIds.size === 0) return;

    try {
      const result = await DataList.execute<UserEntity>({
        collection: UserEntity.collection,
        filter: { id: { $in: Array.from(userIds) } },
        limit: userIds.size,
        dbHandle: 'default'
      });

      if (result.success && result.items) {
        for (const user of result.items) {
          this.userCache.set(user.id, user);
        }
        this.requestUpdate();
      }
    } catch (error) {
      console.warn('RoomListWidget: Failed to load member users', error);
    }
  }

  // === REACTIVE SCROLL ===
  protected override updated(changedProperties: Map<string, unknown>): void {
    super.updated(changedProperties);

    if (changedProperties.has('currentRoomId')) {
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          const container = this.shadowRoot?.querySelector('.entity-list-body') as HTMLElement;
          const activeItem = this.shadowRoot?.querySelector('.room-item.active, .dm-item.active') as HTMLElement;
          if (container && activeItem) {
            const containerRect = container.getBoundingClientRect();
            const itemRect = activeItem.getBoundingClientRect();
            if (itemRect.top < containerRect.top || itemRect.bottom > containerRect.bottom) {
              activeItem.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
            }
          }
        });
      });
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

    ContentService.open('chat', roomId, {
      title: room.displayName || room.name,
      subtitle: this.isDM(room) ? `${(room.members?.length || 0)} members` : room.topic,
      uniqueId: room.uniqueId || room.name || roomId,
      metadata: { entity: room }
    });
  }

  // === DM ACTIONS ===
  private startNewDM(): void {
    console.log('RoomListWidget: Start new DM - user picker not implemented yet');
  }

  private async toggleVoice(e: Event, room: RoomEntity): Promise<void> {
    e.stopPropagation();

    if (this.activeVoiceRoomId === room.id) {
      await this.leaveVoice(room);
    } else {
      await this.startVoice(room);
    }
  }

  private async startVoice(room: RoomEntity): Promise<void> {
    const participants = (room.members || [])
      .map(m => m.userId)
      .filter(id => id !== this.currentUser?.id);

    if (participants.length === 0) {
      console.warn('RoomListWidget: No other participants in room');
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

        ContentService.open('live', room.id as UUID, {
          title: `Voice: ${room.displayName || room.name}`,
          subtitle: `${participants.length + 1} participants`,
          uniqueId: room.id,
          metadata: { room, session: result.session }
        });
      } else {
        console.error('RoomListWidget: Failed to start voice call:', result.message);
      }
    } catch (error) {
      console.error('RoomListWidget: Error starting voice call:', error);
    }
  }

  private async leaveVoice(room: RoomEntity): Promise<void> {
    this.activeVoiceRoomId = null;
    this.selectRoom(room);
  }

  protected override onItemClick(_item: RoomEntity): void {
    // Handled by @click in renderItem template
  }
}
