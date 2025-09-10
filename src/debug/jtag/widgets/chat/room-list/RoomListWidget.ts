/**
 * Room List Widget - Simple BaseWidget for sidebar room navigation
 * 
 * Uses BaseWidget architecture with template/styles system.
 * Shows list of chat rooms for easy navigation.
 */

import { ChatWidgetBase } from '../shared/ChatWidgetBase';

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

  private async loadRooms(): Promise<void> {
    try {
      // Use BaseWidget data methods to load rooms
      const savedRooms = await this.getData('chat_rooms', []);
      
      if (savedRooms.length === 0) {
        // Create default rooms
        this.rooms = [
          { id: 'general', name: 'General', unreadCount: 42 },
          { id: 'academy', name: 'Academy', unreadCount: 42 }
        ];
        await this.storeData('chat_rooms', this.rooms, { persistent: true });
      } else {
        this.rooms = savedRooms;
      }
    } catch (error) {
      console.error('❌ RoomListWidget: Failed to load rooms:', error);
      // Fallback to defaults
      this.rooms = [
        { id: 'general', name: 'General', unreadCount: 69 },
        { id: 'academy', name: 'Academy', unreadCount: 69 }
      ];
    }
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

  private async selectRoom(roomId: string): Promise<void> {
    this.currentRoomId = roomId;
    
    // Broadcast event using BaseWidget methods
    await this.broadcastEvent('room:selected', { roomId });
    
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