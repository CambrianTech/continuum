/**
 * Room List Widget - Simple BaseWidget for sidebar room navigation
 * 
 * Uses BaseWidget architecture with template/styles system.
 * Shows list of chat rooms for easy navigation.
 */

import { ChatWidgetBase } from '../shared/ChatWidgetBase';
import type { DataListParams, DataListResult } from '../../../commands/data/list/shared/DataListTypes';
import type { ChatMessage } from '../../../system/data/domains/ChatMessage';
import { JTAGClient } from '../../../system/core/client/shared/JTAGClient';

interface RoomData {
  readonly roomId: string;
  readonly name: string;
  readonly type: string;
  readonly description?: string;
}

export class RoomListWidget extends ChatWidgetBase {
  private currentRoomId: string = 'general';
  private rooms: Array<{id: string, name: string, unreadCount: number}> = [];
  
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
    console.log('🏠 RoomListWidget: Initializing...');
    
    // Load rooms from data system or use defaults
    await this.loadRooms();
    
    console.log('✅ RoomListWidget: Initialized');
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
      };
  }

  private async calculateUnreadCounts(): Promise<void> {
    // For each room, get actual unread message count from database
    const client = await JTAGClient.sharedInstance;
    for (const room of this.rooms) {
      const messageResult = await this.executeCommand<DataListParams, DataListResult<ChatMessage>>('data/list', {
        context: client.context,
        sessionId: client.sessionId,
        collection: 'chat_messages',
        filter: { roomId: room.id, isRead: false }
      });
      
      room.unreadCount = messageResult?.success ? (messageResult.count || 0) : 0;
    }
  }

  private async loadRooms(): Promise<void> {
    // Load rooms from database using proper executeCommand with strict typing
    const client = await JTAGClient.sharedInstance;
    const result = await this.executeCommand<DataListParams, DataListResult<RoomData>>('data/list', {
      context: client.context,
      sessionId: client.sessionId,
      collection: 'rooms',
      orderBy: [{ field: 'name', direction: 'asc' }]
    });
    
    if (result?.success && result.items?.length > 0) {
      this.rooms = result.items.map((roomData: RoomData) => ({
        id: roomData.roomId,
        name: roomData.name,
        unreadCount: 0 // Will be calculated from messages
      }));
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
      const unreadBadge = room.unreadCount > 0 ? `<span class="unread-badge">${room.unreadCount}</span>` : '';
      
      return `
        <div class="room-item ${isActive ? 'active' : ''}" data-room-id="${room.id}">
          <span class="room-name">${room.name}</span>
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
    console.log('🧹 RoomListWidget: Cleanup complete');
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry