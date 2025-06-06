/**
 * UnifiedSlideoutPanel - Same panel for everyone, different menu options
 */

class UnifiedSlideoutPanel extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
    this.isOpen = false;
  }

  static get observedAttributes() {
    return ['name', 'type', 'status', 'user-id'];
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

  get name() {
    return this.getAttribute('name') || 'Unknown';
  }

  render() {
    this.shadowRoot.innerHTML = `
      <style>
        :host {
          position: fixed;
          top: 0;
          left: 300px;
          width: 400px;
          height: 100vh;
          z-index: 150;
          pointer-events: none;
        }

        .slideout-panel {
          width: 100%;
          height: 100%;
          background: linear-gradient(135deg, 
            rgba(15, 20, 25, 0.98) 0%, 
            rgba(20, 25, 35, 0.95) 50%,
            rgba(10, 15, 20, 0.98) 100%);
          backdrop-filter: blur(10px);
          border-right: 2px solid ${this.isHuman ? 'rgba(255, 183, 77, 0.5)' : 'rgba(0, 212, 255, 0.3)'};
          border-left: 1px solid ${this.isHuman ? 'rgba(255, 183, 77, 0.3)' : 'rgba(0, 212, 255, 0.2)'};
          transform: translateX(-100%);
          transition: transform 0.4s cubic-bezier(0.25, 0.46, 0.45, 0.94);
          overflow-y: auto;
          clip-path: polygon(0 0, calc(100% - 20px) 0, 100% 20px, 100% 100%, 0 100%);
          pointer-events: all;
        }

        .slideout-panel.open {
          transform: translateX(0);
        }

        .panel-header {
          background: ${this.isHuman ? 'rgba(255, 183, 77, 0.1)' : 'rgba(0, 212, 255, 0.1)'};
          padding: 20px;
          border-bottom: 1px solid ${this.isHuman ? 'rgba(255, 183, 77, 0.2)' : 'rgba(0, 212, 255, 0.2)'};
          display: flex;
          justify-content: space-between;
          align-items: center;
        }

        .panel-title {
          color: ${this.isHuman ? '#FFB74D' : '#00d4ff'};
          font-size: 18px;
          font-weight: bold;
          display: flex;
          align-items: center;
          gap: 8px;
        }

        .panel-close {
          background: transparent;
          border: 1px solid ${this.isHuman ? 'rgba(255, 183, 77, 0.4)' : 'rgba(0, 212, 255, 0.4)'};
          color: ${this.isHuman ? '#FFB74D' : '#00d4ff'};
          border-radius: 4px;
          width: 32px;
          height: 32px;
          cursor: pointer;
          font-size: 18px;
          display: flex;
          align-items: center;
          justify-content: center;
          transition: all 0.2s ease;
        }

        .panel-close:hover {
          background: ${this.isHuman ? 'rgba(255, 183, 77, 0.2)' : 'rgba(0, 212, 255, 0.2)'};
          transform: scale(1.1);
        }

        .panel-content {
          padding: 20px;
        }

        .menu-section {
          margin-bottom: 24px;
        }

        .section-title {
          color: #e0e6ed;
          font-size: 14px;
          font-weight: 600;
          margin-bottom: 12px;
          text-transform: uppercase;
          letter-spacing: 0.5px;
          border-bottom: 1px solid rgba(255, 255, 255, 0.1);
          padding-bottom: 6px;
        }

        .menu-item {
          background: rgba(255, 255, 255, 0.05);
          border: 1px solid rgba(255, 255, 255, 0.1);
          border-radius: 6px;
          padding: 12px;
          margin-bottom: 8px;
          cursor: pointer;
          transition: all 0.2s ease;
          display: flex;
          align-items: center;
          gap: 10px;
          color: #e0e6ed;
        }

        .menu-item:hover {
          background: ${this.isHuman ? 'rgba(255, 183, 77, 0.1)' : 'rgba(0, 212, 255, 0.1)'};
          border-color: ${this.isHuman ? 'rgba(255, 183, 77, 0.3)' : 'rgba(0, 212, 255, 0.3)'};
          transform: translateX(4px);
        }

        .menu-item-icon {
          font-size: 16px;
          width: 20px;
          text-align: center;
        }

        .menu-item-text {
          flex: 1;
        }

        .menu-item-desc {
          font-size: 12px;
          color: #8a92a5;
          margin-top: 2px;
        }

        .status-badge {
          background: ${this.getStatusColor()};
          color: white;
          padding: 2px 8px;
          border-radius: 12px;
          font-size: 10px;
          font-weight: bold;
          text-transform: uppercase;
        }
      </style>

      <div class="slideout-panel ${this.isOpen ? 'open' : ''}">
        <div class="panel-header">
          <div class="panel-title">
            <span>${this.isHuman ? '👤' : '🤖'}</span>
            ${this.name}
            <span class="status-badge">${this.getAttribute('status') || 'unknown'}</span>
          </div>
          <button class="panel-close">×</button>
        </div>
        
        <div class="panel-content">
          ${this.getMenuContent()}
        </div>
      </div>
    `;
  }

  getStatusColor() {
    const status = this.getAttribute('status') || 'unknown';
    switch(status) {
      case 'online': case 'active': return '#4CAF50';
      case 'busy': case 'processing': return '#FF9800';
      case 'away': return '#FFC107';
      default: return '#757575';
    }
  }

  getMenuContent() {
    if (this.isHuman) {
      return this.getHumanMenu();
    } else {
      return this.getAgentMenu();
    }
  }

  getHumanMenu() {
    return `
      <div class="menu-section">
        <div class="section-title">Collaboration</div>
        <div class="menu-item" data-action="chat">
          <span class="menu-item-icon">💬</span>
          <div>
            <div class="menu-item-text">Start Chat</div>
            <div class="menu-item-desc">Send direct message</div>
          </div>
        </div>
        <div class="menu-item" data-action="screen-share">
          <span class="menu-item-icon">🖥️</span>
          <div>
            <div class="menu-item-text">Screen Share</div>
            <div class="menu-item-desc">View their screen</div>
          </div>
        </div>
        <div class="menu-item" data-action="voice-call">
          <span class="menu-item-icon">📞</span>
          <div>
            <div class="menu-item-text">Voice Call</div>
            <div class="menu-item-desc">Start audio chat</div>
          </div>
        </div>
      </div>

      <div class="menu-section">
        <div class="section-title">Project</div>
        <div class="menu-item" data-action="share-files">
          <span class="menu-item-icon">📎</span>
          <div>
            <div class="menu-item-text">Share Files</div>
            <div class="menu-item-desc">Send documents</div>
          </div>
        </div>
        <div class="menu-item" data-action="co-edit">
          <span class="menu-item-icon">✏️</span>
          <div>
            <div class="menu-item-text">Co-Edit</div>
            <div class="menu-item-desc">Edit files together</div>
          </div>
        </div>
      </div>

      <div class="menu-section">
        <div class="section-title">Status</div>
        <div class="menu-item" data-action="user-info">
          <span class="menu-item-icon">ℹ️</span>
          <div>
            <div class="menu-item-text">User Info</div>
            <div class="menu-item-desc">View profile & status</div>
          </div>
        </div>
      </div>
    `;
  }

  getAgentMenu() {
    return `
      <div class="menu-section">
        <div class="section-title">Agent Commands</div>
        <div class="menu-item" data-action="screenshot">
          <span class="menu-item-icon">📸</span>
          <div>
            <div class="menu-item-text">Take Screenshot</div>
            <div class="menu-item-desc">Capture interface</div>
          </div>
        </div>
        <div class="menu-item" data-action="execute">
          <span class="menu-item-icon">⚡</span>
          <div>
            <div class="menu-item-text">Execute Command</div>
            <div class="menu-item-desc">Run system command</div>
          </div>
        </div>
        <div class="menu-item" data-action="file-ops">
          <span class="menu-item-icon">📁</span>
          <div>
            <div class="menu-item-text">File Operations</div>
            <div class="menu-item-desc">Read/write files</div>
          </div>
        </div>
      </div>

      <div class="menu-section">
        <div class="section-title">AI Control</div>
        <div class="menu-item" data-action="train">
          <span class="menu-item-icon">🎓</span>
          <div>
            <div class="menu-item-text">Train Agent</div>
            <div class="menu-item-desc">Academy training</div>
          </div>
        </div>
        <div class="menu-item" data-action="configure">
          <span class="menu-item-icon">⚙️</span>
          <div>
            <div class="menu-item-text">Configure</div>
            <div class="menu-item-desc">Adjust settings</div>
          </div>
        </div>
        <div class="menu-item" data-action="debug">
          <span class="menu-item-icon">🔧</span>
          <div>
            <div class="menu-item-text">Debug Mode</div>
            <div class="menu-item-desc">View internals</div>
          </div>
        </div>
      </div>

      <div class="menu-section">
        <div class="section-title">Status</div>
        <div class="menu-item" data-action="agent-info">
          <span class="menu-item-icon">📊</span>
          <div>
            <div class="menu-item-text">Agent Stats</div>
            <div class="menu-item-desc">Performance metrics</div>
          </div>
        </div>
      </div>
    `;
  }

  setupEventListeners() {
    const closeBtn = this.shadowRoot.querySelector('.panel-close');
    const menuItems = this.shadowRoot.querySelectorAll('.menu-item');

    closeBtn.addEventListener('click', () => {
      this.close();
    });

    menuItems.forEach(item => {
      item.addEventListener('click', () => {
        const action = item.getAttribute('data-action');
        this.handleMenuAction(action);
      });
    });
  }

  handleMenuAction(action) {
    console.log(`🎯 Menu action: ${action} for ${this.connectionType} ${this.name}`);
    
    const event = new CustomEvent('panel-action', {
      detail: {
        action,
        name: this.name,
        type: this.connectionType,
        isHuman: this.isHuman
      },
      bubbles: true
    });
    
    this.dispatchEvent(event);

    // Handle specific actions
    switch(action) {
      case 'screenshot':
        this.takeScreenshot();
        break;
      case 'chat':
        this.startChat();
        break;
      case 'agent-info':
        this.showAgentInfo();
        break;
      // Add more action handlers as needed
    }
  }

  takeScreenshot() {
    console.log('📸 Taking screenshot via agent command');
    // Use the /connect API
    fetch('/connect', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        command: 'SCREENSHOT',
        params: 'selector body'
      })
    }).then(response => response.json())
      .then(data => console.log('Screenshot result:', data))
      .catch(error => console.error('Screenshot failed:', error));
  }

  startChat() {
    console.log(`💬 Starting chat with ${this.name}`);
    // TODO: Implement chat functionality
  }

  showAgentInfo() {
    console.log(`📊 Showing info for agent ${this.name}`);
    // TODO: Implement agent info display
  }

  open() {
    this.isOpen = true;
    this.shadowRoot.querySelector('.slideout-panel').classList.add('open');
  }

  close() {
    this.isOpen = false;
    this.shadowRoot.querySelector('.slideout-panel').classList.remove('open');
  }

  toggle() {
    if (this.isOpen) {
      this.close();
    } else {
      this.open();
    }
  }
}

// Register the unified panel
customElements.define('slideout-panel', UnifiedSlideoutPanel);

// Export for module use
if (typeof module !== 'undefined' && module.exports) {
  module.exports = UnifiedSlideoutPanel;
}