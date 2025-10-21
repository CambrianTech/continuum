/**
 * ChatRoom - TypeScript Component for Room Management
 * Manages room state, title, and delegates to ChatWidget for messages
 */

import { BaseWidget } from '../shared/BaseWidget';

interface Room {
  id: string;
  name: string;
  description: string;
  icon: string;
  type: 'general' | 'academy' | 'project';
  participants: Participant[];
}

interface Participant {
  id: string;
  name: string;
  type: 'human' | 'ai' | 'agent';
  status: 'online' | 'offline' | 'away';
  avatar?: string;
}

export class ChatRoom extends BaseWidget {
  private currentRoom: Room;
  private availableRooms: Room[] = [
    {
      id: 'general',
      name: 'General Chat',
      description: 'Smart agent routing with Protocol Sheriff validation',
      icon: '💬',
      type: 'general',
      participants: [
        { id: 'you', name: 'YOU', type: 'human', status: 'online' },
        { id: 'claude-code', name: 'Claude Code', type: 'ai', status: 'online', avatar: '🤖' },
        { id: 'auto-route', name: 'Auto Route', type: 'agent', status: 'online', avatar: '🎯' },
        { id: 'protocol-sheriff', name: 'Protocol Sheriff', type: 'agent', status: 'online', avatar: '🛡️' }
      ]
    },
    {
      id: 'academy',
      name: 'Academy Training',
      description: 'AI training and persona development',
      icon: '🎓',
      type: 'academy',
      participants: [
        { id: 'you', name: 'YOU', type: 'human', status: 'online' },
        { id: 'testing-droid', name: 'TestingDroid', type: 'agent', status: 'online', avatar: '🤖' },
        { id: 'protocol-sheriff', name: 'Protocol Sheriff', type: 'agent', status: 'online', avatar: '🛡️' },
        { id: 'code-specialist', name: 'Code Specialist', type: 'agent', status: 'online', avatar: '💻' }
      ]
    }
  ];

  constructor() {
    super();
    this.widgetName = 'ChatRoom';
    this.widgetIcon = '🏠';
    this.widgetTitle = 'Chat Room';
    this.currentRoom = this.availableRooms[0]; // Default to general
  }

  protected async initializeWidget(): Promise<void> {
    this.setupRoomEventListeners();
    this.render();
    console.log(`🏠 ChatRoom: Initialized with room ${this.currentRoom.name}`);
  }

  private setupRoomEventListeners(): void {
    // Listen for room switch requests
    document.addEventListener('continuum:switch-room', (e: Event) => {
      const customEvent = e as CustomEvent;
      this.switchToRoom(customEvent.detail.roomId);
    });
  }

  public switchToRoom(roomId: string): void {
    const newRoom = this.availableRooms.find(room => room.id === roomId);
    if (!newRoom || newRoom.id === this.currentRoom.id) {
      return;
    }

    console.log(`🏠 ChatRoom: Switching from ${this.currentRoom.name} to ${newRoom.name}`);
    this.currentRoom = newRoom;
    this.render();
    this.notifyRoomChange();
  }

  private notifyRoomChange(): void {
    // Notify other components about room change
    const event = new CustomEvent('continuum:room-changed', {
      detail: {
        room: this.currentRoom
      },
      bubbles: true
    });
    document.dispatchEvent(event);
  }

  public getCurrentRoom(): Room {
    return this.currentRoom;
  }

  renderContent(): string {
    const participantsList = this.currentRoom.participants
      .map(p => `
        <div class="participant ${p.status}" title="${p.name} (${p.type})">
          <span class="participant-avatar">${p.avatar || this.getDefaultAvatar(p.type)}</span>
          <span class="participant-name">${p.name}</span>
          <span class="participant-status-dot"></span>
        </div>
      `).join('');

    return `
      <div class="chat-room">
        <div class="room-header">
          <div class="room-title-section">
            <span class="room-icon">${this.currentRoom.icon}</span>
            <div class="room-info">
              <h2 class="room-title">${this.currentRoom.name}</h2>
              <p class="room-description">${this.currentRoom.description} • Connected</p>
            </div>
          </div>
          <div class="room-participants">
            <div class="participants-label">Connected:</div>
            <div class="participants-list">
              ${participantsList}
            </div>
          </div>
        </div>
        
        <div class="room-content">
          <chat-widget room="${this.currentRoom.id}"></chat-widget>
        </div>
      </div>
    `;
  }

  private getDefaultAvatar(type: 'human' | 'ai' | 'agent'): string {
    switch (type) {
      case 'human': return '👤';
      case 'ai': return '🤖';
      case 'agent': return '🔹';
      default: return '❓';
    }
  }

  async loadCSS(): Promise<string> {
    return `
      :host {
        display: flex;
        flex-direction: column;
        height: 100%;
        background: rgba(15, 20, 25, 0.95);
      }

      .chat-room {
        display: flex;
        flex-direction: column;
        height: 100%;
      }

      .room-header {
        padding: 20px;
        border-bottom: 1px solid rgba(255, 255, 255, 0.1);
        flex-shrink: 0;
        background: rgba(15, 20, 25, 0.98);
      }

      .room-title-section {
        display: flex;
        align-items: center;
        gap: 12px;
      }

      .room-icon {
        font-size: 24px;
        width: 40px;
        height: 40px;
        display: flex;
        align-items: center;
        justify-content: center;
        background: rgba(255, 255, 255, 0.1);
        border-radius: 8px;
        border: 1px solid rgba(255, 255, 255, 0.2);
      }

      .room-info {
        flex: 1;
      }

      .room-title {
        margin: 0;
        font-size: 24px;
        font-weight: 600;
        color: #e0e6ed;
        line-height: 1.2;
      }

      .room-description {
        margin: 4px 0 0 0;
        font-size: 14px;
        color: #8a92a5;
        opacity: 0.8;
      }

      .room-participants {
        margin-top: 12px;
        display: flex;
        flex-direction: column;
        gap: 8px;
      }

      .participants-label {
        font-size: 12px;
        color: #8a92a5;
        font-weight: 500;
        text-transform: uppercase;
        letter-spacing: 0.5px;
      }

      .participants-list {
        display: flex;
        flex-wrap: wrap;
        gap: 8px;
      }

      .participant {
        display: flex;
        align-items: center;
        gap: 6px;
        padding: 4px 8px;
        background: rgba(255, 255, 255, 0.05);
        border: 1px solid rgba(255, 255, 255, 0.1);
        border-radius: 16px;
        font-size: 12px;
        position: relative;
        transition: all 0.2s ease;
      }

      .participant:hover {
        background: rgba(255, 255, 255, 0.08);
        border-color: rgba(79, 195, 247, 0.3);
      }

      .participant-avatar {
        font-size: 14px;
        line-height: 1;
      }

      .participant-name {
        color: #e0e6ed;
        font-weight: 500;
        white-space: nowrap;
      }

      .participant-status-dot {
        width: 6px;
        height: 6px;
        border-radius: 50%;
        background: #4ade80;
        flex-shrink: 0;
      }

      .participant.offline .participant-status-dot {
        background: #6b7280;
      }

      .participant.away .participant-status-dot {
        background: #fbbf24;
      }

      .room-content {
        flex: 1;
        overflow: hidden;
        display: flex;
        flex-direction: column;
      }

      chat-widget {
        flex: 1;
        display: flex;
        flex-direction: column;
      }
    `;
  }

  setupEventListeners(): void {
    // Room-specific event handling can go here
  }
}

// Register the custom element
customElements.define('chat-room', ChatRoom);