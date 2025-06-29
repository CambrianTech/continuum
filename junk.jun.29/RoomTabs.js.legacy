/**
 * Room Tabs Web Component
 * Handles switching between different chat rooms/contexts
 */

class RoomTabs extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
    
    // Component state
    this.activeRoom = 'general';
    this.rooms = [
      { id: 'general', name: 'General', icon: '💬' },
      { id: 'academy', name: 'Academy', icon: '🎓' }
    ];
    this.onRoomChange = null;
  }

  connectedCallback() {
    this.render();
    this.setupEventListeners();
  }

  setupEventListeners() {
    this.shadowRoot.addEventListener('click', (e) => {
      if (e.target.closest('.room-tab')) {
        const roomId = e.target.closest('.room-tab').dataset.roomId;
        this.selectRoom(roomId);
      }
    });
  }

  selectRoom(roomId) {
    if (this.activeRoom === roomId) return;
    
    this.activeRoom = roomId;
    this.updateTabState();
    
    if (this.onRoomChange) {
      this.onRoomChange(roomId);
    }
    
    this.dispatchEvent(new CustomEvent('room-changed', {
      detail: { roomId },
      bubbles: true
    }));
  }

  updateTabState() {
    const tabs = this.shadowRoot.querySelectorAll('.room-tab');
    tabs.forEach(tab => {
      tab.classList.remove('active');
      if (tab.dataset.roomId === this.activeRoom) {
        tab.classList.add('active');
      }
    });
  }

  render() {
    this.shadowRoot.innerHTML = `
      <style>
        :host {
          display: block;
          margin: 20px;
          margin-bottom: 0;
        }

        .room-tabs {
          display: flex;
          background: rgba(255, 255, 255, 0.05);
          border-radius: 12px;
          padding: 4px;
        }

        .room-tab {
          flex: 1;
          padding: 10px 12px;
          text-align: center;
          border-radius: 8px;
          cursor: pointer;
          transition: all 0.3s ease;
          font-size: 14px;
          font-weight: 500;
          color: #8a92a5;
          display: flex;
          align-items: center;
          justify-content: center;
          gap: 6px;
        }

        .room-tab.active {
          background: linear-gradient(135deg, #4FC3F7, #29B6F6);
          color: white;
          box-shadow: 0 2px 8px rgba(79, 195, 247, 0.3);
        }

        .room-tab:hover:not(.active) {
          background: rgba(255, 255, 255, 0.1);
          color: #e0e6ed;
        }

        .room-icon {
          font-size: 16px;
        }

        .room-name {
          font-size: 14px;
        }
      </style>

      <div class="room-tabs">
        ${this.rooms.map(room => `
          <div class="room-tab ${room.id === this.activeRoom ? 'active' : ''}" data-room-id="${room.id}">
            <span class="room-icon">${room.icon}</span>
            <span class="room-name">${room.name}</span>
          </div>
        `).join('')}
      </div>
    `;
  }

  // Public API
  setActiveRoom(roomId) {
    this.selectRoom(roomId);
  }

  setRooms(rooms) {
    this.rooms = rooms;
    this.render();
  }

  setOnRoomChange(callback) {
    this.onRoomChange = callback;
  }
}

// Register the custom element
if (typeof customElements !== 'undefined') {
  customElements.define('room-tabs', RoomTabs);
}

// Export for Node.js
if (typeof module !== 'undefined' && module.exports) {
  module.exports = RoomTabs;
}