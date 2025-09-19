/**
 * Room List Widget - Simple BaseWidget for sidebar room navigation
 * 
 * Uses BaseWidget architecture with template/styles system.
 * Shows list of chat rooms for easy navigation.
 */

import { ChatWidgetBase } from '../shared/ChatWidgetBase';
import type { DataListParams, DataListResult } from '../../../commands/data/list/shared/DataListTypes';
import type { ChatMessageData } from '../../../system/data/domains/ChatMessage';
import type { ChatRoomData } from '../../../system/data/domains/ChatRoom';
import { JTAGClient } from '../../../system/core/client/shared/JTAGClient';
import { COLLECTIONS } from '../../../system/data/core/FieldMapping';

export class RoomListWidget extends ChatWidgetBase {
  private currentRoomId: string = 'general';
  private rooms: ChatRoomData[] = [];
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
    console.log('🔧 CLAUDE-FIX-' + Date.now() + ': RoomListWidget onWidgetInitialize called');
    console.log('🏠 RoomListWidget: Initializing...');

    // Load rooms from data system or use defaults
    await this.loadRooms();

    console.log('✅ RoomListWidget: Initialized with', this.rooms.length, 'rooms');
  }

  protected override resolveResourcePath(filename: string): string {
      // Extract widget directory name from widget name (ChatWidget -> chat)
      //const widgetDir = this.config.widgetName.toLowerCase().replace('widget', '');
      // Return relative path from current working directory
      return `widgets/chat/room-list/${filename}`;
    }

  protected override getReplacements(): Record<string, string> {
      return {
          '<!-- ROOM_LIST_CONTENT -->': this.renderRoomList(),
          '<!-- ROOM_COUNT -->': this.rooms.length.toString(),
      };
  }

  private async calculateUnreadCounts(): Promise<void> {
    // For each room, get actual unread message count from database
    const client = await JTAGClient.sharedInstance;
    for (const room of this.rooms) {
      const messageResult = await this.executeCommand<DataListParams, DataListResult<ChatMessageData>>('data/list', {
        context: client.context,
        sessionId: client.sessionId,
        collection: COLLECTIONS.CHAT_MESSAGES,
        filter: { roomId: room.id, isRead: false }
      });

      const count = messageResult?.success ? (messageResult.count || 0) : 0;
      this.unreadCounts.set(room.id, count);
    }
  }

  private async loadRooms(): Promise<void> {
    // Load rooms from database using proper executeCommand with strict typing
    const client = await JTAGClient.sharedInstance;
    const result = await this.executeCommand<DataListParams, DataListResult<ChatRoomData>>('data/list', {
      context: client.context,
      sessionId: client.sessionId,
      collection: COLLECTIONS.ROOMS,
      orderBy: [{ field: 'name', direction: 'asc' }]
    });

    if (result?.success && result.items?.length > 0) {
      // Use domain objects directly - no manual transformation needed
      this.rooms = result.items
        .filter((room: ChatRoomData) => room?.id && room?.name);

      console.log(`✅ RoomListWidget: Loaded ${this.rooms.length} rooms from database`);
    } else {
      console.warn('⚠️ RoomListWidget: No rooms found in database');
      this.rooms = [];
    }

    // Calculate actual unread counts for each room
    await this.calculateUnreadCounts();
  }

  private renderRoomList(): string {
    return this.rooms.map(room => {
      const isActive = this.currentRoomId === room.id;
      const unreadCount = this.unreadCounts.get(room.id) || 0;
      const unreadBadge = unreadCount > 0 ? `<span class="unread-badge">${unreadCount}</span>` : '';
      const displayName = room.displayName || room.name;

      return `
        <div class="room-item ${isActive ? 'active' : ''}" data-room-id="${room.id}">
          <span class="room-name">${displayName}</span>
          ${unreadBadge}
        </div>
      `;
    }).join('');
  }

  protected override setupEventListeners(): void {
    this.shadowRoot?.addEventListener('click', (e) => {
      const target = e.target as HTMLElement;
      const roomItem = target.closest('.room-item') as HTMLElement;
      
      if (roomItem) {
        const roomId = roomItem.dataset.roomId;
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
  private async selectRoom(roomId: string): Promise<void> {
    // TODO: incorrect implementation - should ONLY call set room command 
    this.currentRoomId = roomId;
    
    // Re-render to update visual state
    await this.renderWidget();
    
    console.log('🏠 RoomListWidget: Selected room:', roomId);
  }

  protected async onWidgetCleanup(): Promise<void> {
    // Save current state
    await this.storeData('current_room', this.currentRoomId, { persistent: true });
    this.rooms = [];
    this.unreadCounts.clear();
    console.log('🧹 RoomListWidget: Cleanup complete');
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry