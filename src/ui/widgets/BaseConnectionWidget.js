/**
 * BaseConnectionWidget - Shared widget for agents and humans
 */

class BaseConnectionWidget extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
  }

  static get observedAttributes() {
    return ['name', 'status', 'type', 'user-id', 'last-seen'];
  }

  connectedCallback() {
    this.render();
    this.setupEventListeners();
  }

  attributeChangedCallback() {
    this.render();
  }

  get connectionType() {
    return this.getAttribute('type') || 'unknown';
  }

  get isHuman() {
    return this.connectionType === 'human';
  }

  get isAgent() {
    return this.connectionType === 'agent';
  }

  get name() {
    return this.getAttribute('name') || 'Unknown';
  }

  get status() {
    return this.getAttribute('status') || 'offline';
  }

  render() {
    this.shadowRoot.innerHTML = `
      <style>
        :host {
          display: block;
          margin: 6px 0;
        }

        .connection-widget {
          background: rgba(20, 25, 35, 0.8);
          border: 1px solid ${this.getBorderColor()};
          border-radius: 8px;
          padding: 12px;
          position: relative;
          transition: all 0.3s ease;
          cursor: pointer;
        }

        .connection-widget:hover {
          border-color: ${this.getHoverBorderColor()};
          background: rgba(25, 30, 40, 0.9);
          transform: translateX(4px);
        }

        .connection-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
        }

        .connection-info {
          flex: 1;
        }

        .connection-name {
          color: ${this.getNameColor()};
          font-weight: 600;
          font-size: 14px;
          margin-bottom: 4px;
          display: flex;
          align-items: center;
          gap: 6px;
        }

        .connection-type-icon {
          font-size: 12px;
        }

        .connection-status {
          color: #8a92a5;
          font-size: 12px;
          text-transform: uppercase;
          letter-spacing: 0.5px;
        }

        .connection-status.online {
          color: #4CAF50;
        }

        .connection-status.busy {
          color: #FF9800;
        }

        .connection-status.offline {
          color: #757575;
        }

        .expand-btn {
          width: 24px;
          height: 24px;
          background: transparent;
          border: 1px solid ${this.getBorderColor()};
          border-radius: 4px;
          color: ${this.getNameColor()};
          cursor: pointer;
          display: flex;
          align-items: center;
          justify-content: center;
          font-size: 12px;
          font-weight: bold;
          transition: all 0.2s ease;
          font-family: monospace;
        }

        .expand-btn:hover {
          background: ${this.getHoverBackground()};
          border-color: ${this.getHoverBorderColor()};
          box-shadow: 0 0 8px ${this.getGlowColor()};
          transform: scale(1.1);
        }

        .expand-btn:active {
          transform: scale(0.95);
        }

        .status-indicator {
          position: absolute;
          top: 8px;
          right: 8px;
          width: 8px;
          height: 8px;
          border-radius: 50%;
          background: ${this.getStatusColor()};
        }

        .you-indicator {
          position: absolute;
          top: 4px;
          left: 4px;
          font-size: 10px;
          color: #FFD700;
          font-weight: bold;
        }
      </style>

      <div class="connection-widget">
        ${this.isYou() ? '<div class="you-indicator">YOU</div>' : ''}
        <div class="status-indicator"></div>
        <div class="connection-header">
          <div class="connection-info">
            <div class="connection-name">
              <span class="connection-type-icon">${this.getTypeIcon()}</span>
              ${this.name}
            </div>
            <div class="connection-status ${this.status}">${this.getDisplayStatus()}</div>
          </div>
          <button class="expand-btn" title="${this.getExpandTooltip()}">
            >>>
          </button>
        </div>
      </div>
    `;
  }

  getBorderColor() {
    return this.isHuman ? 'rgba(255, 183, 77, 0.3)' : 'rgba(0, 212, 255, 0.3)';
  }

  getHoverBorderColor() {
    return this.isHuman ? 'rgba(255, 183, 77, 0.6)' : 'rgba(0, 212, 255, 0.6)';
  }

  getNameColor() {
    return this.isHuman ? '#FFB74D' : '#00d4ff';
  }

  getHoverBackground() {
    return this.isHuman ? 'rgba(255, 183, 77, 0.2)' : 'rgba(0, 212, 255, 0.2)';
  }

  getGlowColor() {
    return this.isHuman ? 'rgba(255, 183, 77, 0.4)' : 'rgba(0, 212, 255, 0.4)';
  }

  getStatusColor() {
    switch(this.status) {
      case 'online': return '#4CAF50';
      case 'busy': return '#FF9800';
      case 'away': return '#FFC107';
      default: return '#757575';
    }
  }

  getTypeIcon() {
    return this.isHuman ? '👤' : '🤖';
  }

  getDisplayStatus() {
    if (this.isHuman) {
      return this.status === 'online' ? 'Connected' : 
             this.status === 'busy' ? 'Busy' : 'Offline';
    } else {
      return this.status === 'online' ? 'Active' :
             this.status === 'busy' ? 'Processing' : 'Idle';
    }
  }

  getExpandTooltip() {
    return this.isHuman ? 'Collaborate with user' : 'Agent commands';
  }

  isYou() {
    // Check if this represents the current user
    const currentUser = window.currentUserId || 'joel'; // Default to joel for now
    return this.getAttribute('user-id') === currentUser;
  }

  setupEventListeners() {
    const expandBtn = this.shadowRoot.querySelector('.expand-btn');
    const widget = this.shadowRoot.querySelector('.connection-widget');

    expandBtn.addEventListener('click', (e) => {
      e.stopPropagation();
      this.toggleSlideout();
    });

    widget.addEventListener('click', () => {
      this.selectConnection();
    });
  }

  toggleSlideout() {
    console.log(`🎯 Toggling slideout for ${this.connectionType}:`, this.name);
    
    const event = new CustomEvent('connection-expand', {
      detail: {
        name: this.name,
        type: this.connectionType,
        status: this.status,
        isHuman: this.isHuman,
        userId: this.getAttribute('user-id')
      },
      bubbles: true
    });
    
    this.dispatchEvent(event);

    // Trigger appropriate slideout
    if (this.isHuman) {
      this.showHumanSlideout();
    } else {
      this.showAgentSlideout();
    }
  }

  selectConnection() {
    console.log(`💬 Selected ${this.connectionType}:`, this.name);
    
    const event = new CustomEvent('connection-select', {
      detail: {
        name: this.name,
        type: this.connectionType,
        isHuman: this.isHuman
      },
      bubbles: true
    });
    
    this.dispatchEvent(event);
  }

  showHumanSlideout() {
    // Human collaboration menu
    console.log('👥 Showing human collaboration slideout');
    // TODO: Implement human slideout with chat, screen share, etc.
  }

  showAgentSlideout() {
    // Agent command menu  
    console.log('🤖 Showing agent command slideout');
    // Use existing agent drawer or create new one
    if (typeof window.toggleAgentDrawer === 'function') {
      window.toggleAgentDrawer();
    }
  }
}

// Register the base widget
customElements.define('connection-widget', BaseConnectionWidget);

// Export for module use
if (typeof module !== 'undefined' && module.exports) {
  module.exports = BaseConnectionWidget;
}