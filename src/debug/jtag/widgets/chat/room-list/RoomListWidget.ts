/**
 * Room List Widget - Simple BaseWidget for sidebar room navigation
 * 
 * Uses BaseWidget architecture with template/styles system.
 * Shows list of chat rooms for easy navigation.
 */

import { ChatWidgetBase } from '../shared/ChatWidgetBase';
import type { DataListParams, DataListResult } from '../../../commands/data/list/shared/DataListTypes';
import { ChatMessageEntity } from '../../../system/data/entities/ChatMessageEntity';
import { RoomEntity } from '../../../system/data/entities/RoomEntity';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../../system/core/client/shared/Commands';
import { Events } from '../../../system/core/client/shared/Events';
import { DEFAULT_ROOMS } from '../../../system/data/domains/DefaultEntities';
import { getDataEventName } from '../../../commands/data/shared/DataEventConstants';
import { createScroller, SCROLLER_PRESETS, type RenderFn, type LoadFn, type EntityScroller } from '../../shared/EntityScroller';

export class RoomListWidget extends ChatWidgetBase {
  private currentRoomId: UUID = DEFAULT_ROOMS.GENERAL as UUID; // Sync with ChatWidget's default
  private roomScroller?: EntityScroller<RoomEntity>;
  private unreadCounts: Map<string, number> = new Map();
  
  constructor() {
    super({
      widgetName: 'RoomListWidget',
      template: 'room-list-widget.html',
      styles: 'room-list-widget.css',
      enableAI: false,
      enableDatabase: true,
      enableRouterEvents: true,
      enableScreenshots: false
    });
  }

  protected async onWidgetInitialize(): Promise<void> {
    console.log('🔧 CLAUDE-FIX-' + Date.now() + ': RoomListWidget onWidgetInitialize with EntityScroller');
    console.log('🏠 RoomListWidget: Initializing with EntityScroller...');

    // Setup event subscriptions first - EntityScroller will be set up after template renders
    await this.setupRoomEventSubscriptions();

    console.log('✅ RoomListWidget: Initialized event subscriptions');
  }

  protected override async renderWidget(): Promise<void> {
    // Render template first
    await super.renderWidget();

    // Now set up EntityScroller after DOM exists
    await this.setupRoomScroller();
  }

  // Path resolution now handled automatically by ChatWidgetBase
  // Generates: widgets/chat/room-list/{filename} from "RoomListWidget"

  protected override getReplacements(): Record<string, string> {
      const roomCount = this.roomScroller?.entities().length || 0;
      return {
          '<!-- ROOM_LIST_CONTENT -->': '', // EntityScroller will populate .room-list container
          '<!-- ROOM_COUNT -->': roomCount.toString(),
      };
  }

  /**
   * Setup EntityScroller with proper deduplication and real-time updates
   */
  private async setupRoomScroller(): Promise<void> {
    const container = this.shadowRoot.querySelector('.room-list') as HTMLElement;
    if (!container) {
      console.error('❌ RoomListWidget: Could not find .room-list container');
      return;
    }

    // Render function for individual room items
    const renderRoom: RenderFn<RoomEntity> = (room: RoomEntity, context) => {
      const unreadCount = this.unreadCounts.get(room.id) || 0;
      // Compare with room.id (actual field from database)
      const isActive = room.id === this.currentRoomId;
      const activeClass = isActive ? 'active' : '';

      const roomElement = document.createElement('div');
      roomElement.className = `room-item ${activeClass}`;
      // Use room.id (actual field from database) for click handling
      roomElement.setAttribute('data-room-id', room.id);
      roomElement.innerHTML = `
        <div class="room-info">
          <div class="room-name">${room.displayName || room.name}</div>
          <div class="room-topic">${room.topic || ''}</div>
        </div>
        ${unreadCount > 0 ? `<div class="unread-count">${unreadCount}</div>` : ''}
      `;

      return roomElement;
    };

    // Load function using existing data/list command
    const loadRooms: LoadFn<RoomEntity> = async (cursor, limit) => {
      const result = await Commands.execute<DataListParams<RoomEntity>, DataListResult<RoomEntity>>('data/list', {
        collection: RoomEntity.collection,
        orderBy: [{ field: 'name', direction: 'asc' }],
        limit: limit || 100
      });

      if (!result?.success || !result.items) {
        throw new Error(`Failed to load rooms: ${result?.error || 'Unknown error'}`);
      }

      return {
        items: result.items,
        hasMore: false, // Room lists are typically small, no pagination needed
        nextCursor: undefined
      };
    };

    // Create scroller with LIST preset (no auto-scroll, larger page size)
    this.roomScroller = createScroller(
      container,
      renderRoom,
      loadRooms,
      SCROLLER_PRESETS.LIST
    );

    // Load initial data
    await this.roomScroller.load();
    console.log(`✅ RoomListWidget: Initialized EntityScroller with automatic deduplication`);

    // Update count after initial load
    this.updateRoomCount();
  }

  /**
   * Set up room event subscriptions for real-time updates
   * Uses EntityScroller's built-in deduplication via add() method
   */
  private async setupRoomEventSubscriptions(): Promise<void> {
    try {
      // Subscribe to data:Room:created events using static Events interface
      const eventName = getDataEventName(RoomEntity.collection, 'created');
      Events.subscribe<RoomEntity>(eventName, (roomEntity: RoomEntity) => {
        console.log(`🔥 SERVER-EVENT-RECEIVED: ${eventName}`, roomEntity);
        console.log(`🔧 CLAUDE-FIX-${Date.now()}: Using EntityScroller.add() for automatic room deduplication`);

        // EntityScroller automatically handles deduplication using entity.id
        this.roomScroller?.add(roomEntity);

        // Update count after adding room
        this.updateRoomCount();
      });

      console.log(`🎧 RoomListWidget: Subscribed to data:${RoomEntity.collection}:created events via Events.subscribe()`);

      // Subscribe to data:Room:updated events using static Events interface
      const updateEventName = getDataEventName(RoomEntity.collection, 'updated');
      Events.subscribe<RoomEntity>(updateEventName, (roomEntity: RoomEntity) => {
        console.log(`🔥 SERVER-EVENT-RECEIVED: ${updateEventName}`, roomEntity);
        console.log(`🔧 CLAUDE-FIX-${Date.now()}: Using EntityScroller.updateEntity() for room updates`);

        // EntityScroller updates the room in place
        const updateResult = this.roomScroller?.update(roomEntity.id, roomEntity);
        console.log(`🔧 CLAUDE-FIX-${Date.now()}: EntityScroller.update() result: ${updateResult} for room ${roomEntity.id}`);

        // Update count if needed (though should remain same for updates)
        this.updateRoomCount();
      });

      console.log(`🎧 RoomListWidget: Subscribed to data:${RoomEntity.collection}:updated events via Events.subscribe()`);

      // Subscribe to data:Room:deleted events using static Events interface
      const deleteEventName = getDataEventName(RoomEntity.collection, 'deleted');
      Events.subscribe<RoomEntity>(deleteEventName, (roomEntity: RoomEntity) => {
        console.log(`🔥 SERVER-EVENT-RECEIVED: ${deleteEventName}`, roomEntity);
        console.log(`🔧 CLAUDE-FIX-${Date.now()}: Using EntityScroller.remove() for room deletion`);

        // EntityScroller removes the room from UI
        this.roomScroller?.remove(roomEntity.id);

        // Update count after deletion
        this.updateRoomCount();
      });

      console.log(`🎧 RoomListWidget: Subscribed to data:${RoomEntity.collection}:deleted events via Events.subscribe()`);
    } catch (error) {
      console.error('❌ RoomListWidget: Failed to set up room event subscriptions:', error);
    }
  }

  /**
   * Update the room count display in the header
   */
  private updateRoomCount(): void {
    const roomCountElement = this.shadowRoot.querySelector('.room-count') as HTMLElement;
    if (roomCountElement && this.roomScroller) {
      const count = this.roomScroller.entities().length;
      roomCountElement.textContent = count.toString();
      console.log(`🔧 CLAUDE-FIX-${Date.now()}: Updated room count to ${count}`);
    }
  }

  private async calculateUnreadCounts(): Promise<void> {
    // For each room, get actual unread message count from database
    if (!this.roomScroller) return;

    for (const room of this.roomScroller.entities()) {
      // Domain-owned: CommandDaemon handles optimization, caching, retries
      const roomId = room.id;
      const messageResult = await Commands.execute<DataListParams, DataListResult<ChatMessageEntity>>('data/list', {
        collection: ChatMessageEntity.collection,
        filter: { roomId, isRead: false }
      });

      const count = messageResult?.success ? (messageResult.count || 0) : 0;
      this.unreadCounts.set(roomId, count);
    }
  }



  protected override setupEventListeners(): void {
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
    // Early exit - prevent unnecessary work if already in this room
    if (this.currentRoomId === roomId) {
      console.log(`🔄 RoomListWidget: Already in room "${roomId}", ignoring selection`);
      return;
    }

    console.log(`🎯 RoomListWidget: Room selection requested: "${roomId}"`);

    // Find room entity for the selected room by id
    const roomEntity = this.roomScroller?.entities().find(room => room.id === roomId);

    // Update visual highlighting with CSS only (no redraw needed)
    this.updateRoomHighlighting(this.currentRoomId, roomId);

    // Update local state
    this.currentRoomId = roomId;

    // Emit room change event for other widgets (like ChatWidget) to respond to
    Events.emit('chat:room-changed', { roomId, roomEntity: roomEntity as RoomEntity });

    console.log(`✅ RoomListWidget: Room changed to "${roomId}"`);
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

  protected async onWidgetCleanup(): Promise<void> {
    // Save current state
    await this.storeData('current_room', this.currentRoomId, { persistent: true });

    // Clean up EntityScroller
    this.roomScroller?.destroy();
    this.roomScroller = undefined;

    this.unreadCounts.clear();
    console.log('🧹 RoomListWidget: Cleanup complete');
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry