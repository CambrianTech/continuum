/**
 * Room List Widget - Now uses EntityScrollerWidget for automatic EntityScroller management
 *
 * Domain-agnostic room list widget - not chat-specific
 */

import { EntityScrollerWidget } from '../../shared/EntityScrollerWidget';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { DataListParams, DataListResult } from '../../../commands/data/list/shared/DataListTypes';
import type { ContentOpenParams, ContentOpenResult } from '../../../commands/collaboration/content/open/shared/ContentOpenTypes';
import { ChatMessageEntity } from '../../../system/data/entities/ChatMessageEntity';
import { RoomEntity } from '../../../system/data/entities/RoomEntity';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../../system/core/shared/Commands';
import { Events } from '../../../system/core/shared/Events';
import { UI_EVENTS } from '../../../system/core/shared/EventConstants';
import { DEFAULT_ROOMS } from '../../../system/data/domains/DefaultEntities';
import { SCROLLER_PRESETS, type RenderFn, type LoadFn, type ScrollerConfig } from '../../shared/EntityScroller';

export class RoomListWidget extends EntityScrollerWidget<RoomEntity> {
  private currentRoomId: UUID = DEFAULT_ROOMS.GENERAL as UUID; // Sync with ChatWidget's default
  private unreadCounts: Map<string, number> = new Map();
  private clickHandlerAdded: boolean = false;
  
  constructor() {
    super({
      widgetId: 'room-list-widget',
      widgetName: 'RoomListWidget',
      styles: 'room-list-widget.css',
      enableAI: false,
      enableDatabase: true,
      enableRouterEvents: true,
      enableScreenshots: false
    });
  }

  // Initialization now handled automatically by EntityScrollerWidget base class

  // Path resolution now handled automatically by ChatWidgetBase
  // Generates: widgets/chat/room-list/{filename} from "RoomListWidget"

  // Using default template from EntityScrollerWidget (generates "room-list" CSS class automatically)

  // Required by EntityScrollerWidget - render function for individual room items
  protected getRenderFunction(): RenderFn<RoomEntity> {
    return (room: RoomEntity, _context) => {
      const unreadCount = this.unreadCounts.get(room.id) ?? 0;
      // Compare with room.id (actual field from database)
      const isActive = room.id === this.currentRoomId;
      const activeClass = isActive ? 'active' : '';

      const roomElement = globalThis.document.createElement('div');
      roomElement.className = `room-item ${activeClass}`;
      // Use room.id (actual field from database) for click handling
      roomElement.setAttribute('data-room-id', room.id);
      roomElement.innerHTML = `
        <div class="room-info">
          <div class="room-name">${room.displayName ?? room.name}</div>
          <div class="room-topic">${room.topic ?? ''}</div>
        </div>
        ${unreadCount > 0 ? `<div class="unread-count">${unreadCount}</div>` : ''}
      `;

      return roomElement;
    };
  }

  // Required by EntityScrollerWidget - load function using data/list command
  protected getLoadFunction(): LoadFn<RoomEntity> {
    return async (cursor, limit) => {
      const result = await Commands.execute<DataListParams<RoomEntity>, DataListResult<RoomEntity>>(DATA_COMMANDS.LIST, {
        collection: RoomEntity.collection,
        orderBy: [{ field: 'name', direction: 'asc' }],
        limit: limit ?? 100
      });

      if (!result?.success || !result.items) {
        throw new Error(`Failed to load rooms: ${result?.error ?? 'Unknown error'}`);
      }

      return {
        items: result.items,
        hasMore: false, // Room lists are typically small, no pagination needed
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
    return '.room-list';
  }

  // Required by EntityScrollerWidget
  protected getEntityCollection(): string {
    return RoomEntity.collection;
  }

  protected resolveResourcePath(filename: string): string {
    return `widgets/chat/room-list/${filename}`;
  }

  // Override to add click event listeners after widget initialization
  protected override async onWidgetInitialize(): Promise<void> {
    await super.onWidgetInitialize();
    this.setupEventListeners();

    // Listen for room selection from tab clicks (so sidebar stays in sync)
    Events.subscribe(UI_EVENTS.ROOM_SELECTED, (data: { roomId: string; roomName: string }) => {
      const newRoomId = data.roomId as UUID;
      if (newRoomId !== this.currentRoomId) {
        this.updateRoomHighlighting(this.currentRoomId, newRoomId);
        this.currentRoomId = newRoomId;
      }
    });

    // Auto-select General room on startup
    setTimeout(() => {
      const generalRoom = this.scroller?.entities().find(room => room.id === DEFAULT_ROOMS.GENERAL);
      if (generalRoom) {
        this.selectRoom(DEFAULT_ROOMS.GENERAL as UUID);
      }
    }, 100); // Small delay to ensure room list is loaded
  }

  // Event subscriptions now handled automatically by EntityScrollerWidget base class

  // Room count updates now handled automatically by EntityScrollerWidget base class

  private async calculateUnreadCounts(): Promise<void> {
    // For each room, get actual unread message count from database
    if (!this.scroller) return;

    for (const room of this.scroller.entities()) {
      // Domain-owned: CommandDaemon handles optimization, caching, retries
      const roomId = room.id;
      const messageResult = await Commands.execute<DataListParams, DataListResult<ChatMessageEntity>>(DATA_COMMANDS.LIST, {
        collection: ChatMessageEntity.collection,
        filter: { roomId, isRead: false }
      });

      const count = messageResult?.success ? (messageResult.count ?? 0) : 0;
      this.unreadCounts.set(roomId, count);
    }
  }



  protected setupEventListeners(): void {
    // Only add listener once to prevent duplicate calls
    if (this.clickHandlerAdded) {
      return;
    }

    this.shadowRoot?.addEventListener('click', (e) => {
      const target = e.target as HTMLElement;
      const roomItem = target.closest('.room-item') as HTMLElement;

      if (roomItem) {
        const roomId = roomItem.dataset.roomId as UUID;
        if (roomId) {
          this.selectRoom(roomId);
        }
      }
    });

    this.clickHandlerAdded = true;
  }

  /*** 
     * 
     * Here is what normally happens in a good API:
     * 1) ONLY THING WE DO HERE IS call the set room command. That is IT
     * 2) The server processes the command and updates the state for this this user (needs api dev), currentRoomId, triggering the current room changed event (or selected room etc) to all subscribed clients EVERYWHERE:
     * server, browser, anywhere else - this is the point of events. And the only place events originate is in the event daemon on the server.  (or other servers in a Grid/continuum)
     * 3) All subscribed clients receive the updated room state and re-render their UI accordingly. Even this.currentRoomId is not done here.
     * 4. the set of currentRoomId causes re-rendering of the room list, highlighting the selected room. You can still do this here if you want, but ideally it is done in response to the event.
     * 5. The chat message list widget also receives the event and re-renders to show messages for the new room.
     * 
    **/
  private async selectRoom(roomId: UUID): Promise<void> {
    // Find room entity for the selected room by id
    const roomEntity = this.scroller?.entities().find(room => room.id === roomId);
    if (!roomEntity) {
      console.error(`❌ RoomListWidget: Room not found: "${roomId}"`);
      return;
    }

    const roomName = roomEntity.displayName || roomEntity.name;
    const isReselection = this.currentRoomId === roomId;

    // OPTIMISTIC UI: Update visuals immediately before server round-trip
    if (!isReselection) {
      this.updateRoomHighlighting(this.currentRoomId, roomId);
      this.currentRoomId = roomId;
    }

    // Emit room selection IMMEDIATELY so ChatWidget switches fast
    Events.emit(UI_EVENTS.ROOM_SELECTED, { roomId, roomName });

    // Emit content:opened for MainWidget tab update (optimistic)
    Events.emit('content:opened', {
      contentType: 'chat',
      entityId: roomId,
      title: roomName
    });

    // Persist to server in BACKGROUND (don't block UI)
    const userId = this.userState?.userId;
    if (userId) {
      Commands.execute<ContentOpenParams, ContentOpenResult>('collaboration/content/open', {
        userId,
        contentType: 'chat',
        entityId: roomId,
        title: roomName,
        subtitle: roomEntity.topic,
        setAsCurrent: true
      }).catch(err => console.error('Failed to persist room open:', err));
    }
  }

  /**
   * Update room highlighting with CSS only - no full redraw needed
   */
  private updateRoomHighlighting(oldRoomId: UUID, newRoomId: UUID): void {
    // Access DOM relative to this widget's own shadow root, not from document
    const container = this.shadowRoot;

    if (!container) {
      console.warn(`🎨 RoomListWidget: No shadow root available for highlighting`);
      return;
    }

    // Remove active class from previously selected room
    const oldActiveRoom = container.querySelector(`[data-room-id="${oldRoomId}"]`);
    if (oldActiveRoom) {
      oldActiveRoom.classList.remove('active');
      console.log(`🎨 RoomListWidget: Removed active class from "${oldRoomId}"`);
    }

    // Add active class to newly selected room
    const newActiveRoom = container.querySelector(`[data-room-id="${newRoomId}"]`);
    if (newActiveRoom) {
      newActiveRoom.classList.add('active');
      console.log(`🎨 RoomListWidget: Added active class to "${newRoomId}"`);
    } else {
      console.warn(`🎨 RoomListWidget: Could not find room element for "${newRoomId}"`);
    }

    console.log(`🎨 RoomListWidget: Updated CSS highlighting "${oldRoomId}" → "${newRoomId}"`);
  }

  // Entity count now handled automatically by EntityScrollerWidget base class

  protected getEntityTitle(_entity?: RoomEntity): string {
    return 'Rooms';
  }

  protected override async onWidgetCleanup(): Promise<void> {
    // Save current state
    await this.storeData('current_room', this.currentRoomId, { persistent: true });

    // EntityScroller cleanup now handled by base class
    await super.onWidgetCleanup();

    this.unreadCounts.clear();
    console.log('🧹 RoomListWidget: Additional cleanup complete');
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry