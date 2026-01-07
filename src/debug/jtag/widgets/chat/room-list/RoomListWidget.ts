/**
 * Room List Widget - Now uses EntityScrollerWidget for automatic EntityScroller management
 *
 * Domain-agnostic room list widget - not chat-specific
 *
 * MIGRATED TO SIGNALS: Uses @preact/signals-core for reactive state management.
 * Room highlighting is now automatic - just set currentRoomId.value and DOM updates.
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

// SIGNALS: Reactive primitives - just JavaScript, bundled by existing esbuild
import { signal, effect, type Signal } from '@preact/signals-core';

// Verbose logging helper for browser
const verbose = () => typeof window !== 'undefined' && (window as any).JTAG_VERBOSE === true;

export class RoomListWidget extends EntityScrollerWidget<RoomEntity> {
  // SIGNAL: Reactive room ID - DOM updates automatically when value changes
  private currentRoomIdSignal: Signal<UUID> = signal(DEFAULT_ROOMS.GENERAL as UUID);
  private previousRoomId: UUID | null = null; // Track previous for highlighting transition
  private unreadCounts: Map<string, number> = new Map();
  private clickHandlerAdded: boolean = false;
  private highlightEffectDispose?: () => void;
  
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
      const result = await Commands.execute<DataListParams, DataListResult<RoomEntity>>(DATA_COMMANDS.LIST, {
        collection: RoomEntity.collection,
        orderBy: [{ field: 'name', direction: 'asc' }],
        limit: limit ?? 100
      });

      if (!result?.success || !result.items) {
        throw new Error(`Failed to load rooms: ${result?.error ?? 'Unknown error'}`);
      }

      // Filter out 'system' tagged rooms (Help, Settings, etc.)
      // These are accessible via dedicated buttons, not the rooms list
      // Client-side filter - room lists are small, SQLite doesn't support array queries
      const visibleRooms = result.items.filter(room => {
        const tags = room.tags ?? [];
        return !tags.includes('system');
      });

      return {
        items: visibleRooms,
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

  // Getter for backward compatibility and cleaner access
  private get currentRoomId(): UUID {
    return this.currentRoomIdSignal.value;
  }

  // Setter that triggers reactive updates automatically
  private set currentRoomId(value: UUID) {
    this.currentRoomIdSignal.value = value;
  }

  // Override to add click event listeners after widget initialization
  protected override async onWidgetInitialize(): Promise<void> {
    await super.onWidgetInitialize();
    this.setupEventListeners();

    // REACTIVE EFFECT: Automatically update highlighting when currentRoomId changes
    // This replaces the manual updateRoomHighlighting() calls
    this.highlightEffectDispose = effect(() => {
      const newRoomId = this.currentRoomIdSignal.value;
      const oldRoomId = this.previousRoomId;

      if (oldRoomId !== newRoomId) {
        this.applyHighlighting(oldRoomId, newRoomId);
        this.previousRoomId = newRoomId;
      }
    });

    // Listen for room selection from tab clicks (so sidebar stays in sync)
    Events.subscribe(UI_EVENTS.ROOM_SELECTED, (data: { roomId: string; roomName: string }) => {
      const newRoomId = data.roomId as UUID;
      // SIGNAL: Just set the value - highlighting updates automatically via effect()
      if (newRoomId !== this.currentRoomId) {
        this.currentRoomId = newRoomId;
      }
    });

    // DO NOT auto-select General here - initial content is determined by:
    // 1. URL routing in MainWidget
    // 2. Saved contentState from userState
    // 3. Default handled by MainWidget if neither exists
    // RoomListWidget only reacts to ROOM_SELECTED events, doesn't initiate them
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

    // SIGNAL: Just set the value - highlighting updates automatically via effect()
    // No manual updateRoomHighlighting() call needed anymore!
    if (!isReselection) {
      this.currentRoomId = roomId;
    }

    // Emit room selection IMMEDIATELY so ChatWidget switches fast
    // Include uniqueId for human-readable URL building
    // NOTE: MainWidget handles view switching via ROOM_SELECTED
    // NOTE: Tab creation happens via collaboration/content/open command below
    // DO NOT emit content:opened here - the command will emit it with proper contentItemId
    Events.emit(UI_EVENTS.ROOM_SELECTED, {
      roomId,
      roomName,
      uniqueId: roomEntity.uniqueId || roomEntity.name || roomId  // Prefer uniqueId for URLs
    });

    // Persist to server in BACKGROUND - command emits content:opened with proper data
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
   * Apply room highlighting via CSS classes
   * Called automatically by the reactive effect when currentRoomIdSignal changes
   */
  private applyHighlighting(oldRoomId: UUID | null, newRoomId: UUID): void {
    const container = this.shadowRoot;
    if (!container) {
      console.warn(`🎨 RoomListWidget: No shadow root available for highlighting`);
      return;
    }

    // Remove active class from previously selected room
    if (oldRoomId) {
      const oldActiveRoom = container.querySelector(`[data-room-id="${oldRoomId}"]`);
      if (oldActiveRoom) {
        oldActiveRoom.classList.remove('active');
      }
    }

    // Add active class to newly selected room
    const newActiveRoom = container.querySelector(`[data-room-id="${newRoomId}"]`);
    if (newActiveRoom) {
      newActiveRoom.classList.add('active');
    }

    verbose() && console.log(`🎨 RoomListWidget: [SIGNAL] Highlighting updated "${oldRoomId}" → "${newRoomId}"`);
  }

  // Entity count now handled automatically by EntityScrollerWidget base class

  protected getEntityTitle(_entity?: RoomEntity): string {
    return 'Rooms';
  }

  protected override async onWidgetCleanup(): Promise<void> {
    // Save current state
    await this.storeData('current_room', this.currentRoomId, { persistent: true });

    // SIGNAL: Dispose the reactive effect to prevent memory leaks
    this.highlightEffectDispose?.();
    this.highlightEffectDispose = undefined;

    // EntityScroller cleanup now handled by base class
    await super.onWidgetCleanup();

    this.unreadCounts.clear();
    verbose() && console.log('🧹 RoomListWidget: Additional cleanup complete (signals disposed)');
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry