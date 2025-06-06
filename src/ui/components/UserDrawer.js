/**
 * User Drawer Component
 * Sliding drawer that shows detailed user/agent information
 */

class UserDrawer extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
    
    // Component state
    this.isOpen = false;
    this.currentItem = null;
  }

  connectedCallback() {
    this.render();
    this.setupEventListeners();
  }

  open(item) {
    this.currentItem = item;
    this.isOpen = true;
    this.render();
    
    // Trigger animation
    requestAnimationFrame(() => {
      this.shadowRoot.querySelector('.drawer').classList.add('open');
    });
  }

  close() {
    const drawer = this.shadowRoot.querySelector('.drawer');
    drawer.classList.remove('open');
    
    // Wait for animation before updating state
    setTimeout(() => {
      this.isOpen = false;
      this.currentItem = null;
      this.render();
    }, 300);
  }

  setupEventListeners() {
    this.shadowRoot.addEventListener('click', (e) => {
      if (e.target.closest('.close-btn')) {
        this.close();
      }
      
      if (e.target.closest('.drawer-overlay')) {
        this.close();
      }
      
      if (e.target.closest('.action-btn')) {
        const action = e.target.closest('.action-btn').dataset.action;
        this.handleAction(action);
      }
    });
  }

  handleAction(action) {
    if (!this.currentItem) return;
    
    switch (action) {
      case 'message':
        this.dispatchEvent(new CustomEvent('drawer-action', {
          detail: { action: 'message', item: this.currentItem },
          bubbles: true
        }));
        break;
      case 'details':
        this.dispatchEvent(new CustomEvent('drawer-action', {
          detail: { action: 'details', item: this.currentItem },
          bubbles: true
        }));
        break;
      case 'stats':
        this.dispatchEvent(new CustomEvent('drawer-action', {
          detail: { action: 'stats', item: this.currentItem },
          bubbles: true
        }));
        break;
    }
  }

  generateActionButtons(item) {
    const isUser = item.type === 'user' || item.type === 'assistant';
    const isAI = item.type === 'ai' || item.type === 'system';
    
    if (isUser) {
      return `
        <button class="action-btn primary" data-action="message">
          💬 Send Message
        </button>
        <button class="action-btn secondary" data-action="details">
          📋 View Profile
        </button>
        <button class="action-btn secondary" data-action="stats">
          📊 Activity Stats
        </button>
      `;
    } else if (isAI) {
      return `
        <button class="action-btn primary" data-action="message">
          🤖 Chat with ${item.name}
        </button>
        <button class="action-btn secondary" data-action="details">
          ⚙️ Agent Details
        </button>
        <button class="action-btn secondary" data-action="stats">
          📈 Performance Stats
        </button>
      `;
    }
    
    return '';
  }

  render() {
    if (!this.isOpen || !this.currentItem) {
      this.shadowRoot.innerHTML = '';
      return;
    }

    const item = this.currentItem;
    const isUser = item.type === 'user' || item.type === 'assistant';
    
    this.shadowRoot.innerHTML = `
      <style>
        :host {
          position: fixed;
          top: 0;
          left: 0;
          width: 100%;
          height: 100%;
          pointer-events: none;
          z-index: 1000;
        }

        .drawer-overlay {
          position: absolute;
          top: 0;
          left: 0;
          width: 100%;
          height: 100%;
          background: rgba(0, 0, 0, 0.5);
          opacity: 0;
          transition: opacity 0.3s ease;
          pointer-events: none;
        }

        .drawer-overlay.open {
          opacity: 1;
          pointer-events: all;
        }

        .drawer {
          position: absolute;
          top: 0;
          left: 0;
          width: 400px;
          height: 100%;
          background: linear-gradient(135deg, #1a1f2e 0%, #2d3748 100%);
          border-right: 1px solid rgba(255, 255, 255, 0.1);
          transform: translateX(-100%);
          transition: transform 0.3s cubic-bezier(0.4, 0, 0.2, 1);
          pointer-events: all;
          overflow-y: auto;
          box-shadow: 4px 0 20px rgba(0, 0, 0, 0.3);
        }

        .drawer.open {
          transform: translateX(0);
        }

        .drawer-header {
          padding: 24px;
          border-bottom: 1px solid rgba(255, 255, 255, 0.1);
          background: linear-gradient(135deg, rgba(0, 255, 136, 0.1), rgba(0, 204, 106, 0.1));
          position: relative;
        }

        .close-btn {
          position: absolute;
          top: 16px;
          right: 16px;
          background: transparent;
          border: none;
          color: #fff;
          font-size: 24px;
          cursor: pointer;
          padding: 8px;
          border-radius: 50%;
          transition: all 0.3s ease;
          width: 40px;
          height: 40px;
          display: flex;
          align-items: center;
          justify-content: center;
        }

        .close-btn:hover {
          background: rgba(255, 255, 255, 0.1);
          transform: rotate(90deg);
        }

        .user-avatar-large {
          width: 80px;
          height: 80px;
          border-radius: 50%;
          display: flex;
          align-items: center;
          justify-content: center;
          font-size: 40px;
          margin-bottom: 16px;
          background: ${item.gradient};
          box-shadow: 0 4px 16px rgba(0, 255, 136, 0.3);
        }

        .user-name {
          font-size: 24px;
          font-weight: 700;
          color: #fff;
          margin-bottom: 8px;
        }

        .user-role {
          font-size: 16px;
          color: #8a92a5;
          margin-bottom: 16px;
        }

        .status-badge {
          display: inline-flex;
          align-items: center;
          gap: 8px;
          padding: 8px 16px;
          background: rgba(76, 175, 80, 0.2);
          border: 1px solid rgba(76, 175, 80, 0.3);
          border-radius: 20px;
          color: #4CAF50;
          font-size: 14px;
          font-weight: 600;
        }

        .status-dot {
          width: 8px;
          height: 8px;
          border-radius: 50%;
          background: #4CAF50;
        }

        .drawer-content {
          padding: 24px;
        }

        .info-section {
          margin-bottom: 32px;
        }

        .section-title {
          font-size: 18px;
          font-weight: 600;
          color: #fff;
          margin-bottom: 16px;
          display: flex;
          align-items: center;
          gap: 8px;
        }

        .info-grid {
          display: grid;
          gap: 12px;
        }

        .info-item {
          display: flex;
          justify-content: space-between;
          align-items: center;
          padding: 12px 16px;
          background: rgba(255, 255, 255, 0.05);
          border-radius: 8px;
          border: 1px solid rgba(255, 255, 255, 0.1);
        }

        .info-label {
          color: #8a92a5;
          font-size: 14px;
        }

        .info-value {
          color: #fff;
          font-weight: 600;
          font-size: 14px;
        }

        .actions-section {
          border-top: 1px solid rgba(255, 255, 255, 0.1);
          padding-top: 24px;
        }

        .action-btn {
          width: 100%;
          padding: 16px 24px;
          margin-bottom: 12px;
          border: none;
          border-radius: 12px;
          font-size: 16px;
          font-weight: 600;
          cursor: pointer;
          transition: all 0.3s ease;
          display: flex;
          align-items: center;
          justify-content: center;
          gap: 8px;
        }

        .action-btn.primary {
          background: linear-gradient(135deg, #00ff88, #00cc6a);
          color: #000;
        }

        .action-btn.primary:hover {
          transform: translateY(-2px);
          box-shadow: 0 8px 20px rgba(0, 255, 136, 0.3);
        }

        .action-btn.secondary {
          background: rgba(255, 255, 255, 0.1);
          color: #fff;
          border: 1px solid rgba(255, 255, 255, 0.2);
        }

        .action-btn.secondary:hover {
          background: rgba(255, 255, 255, 0.2);
          transform: translateY(-1px);
        }

        .stats-grid {
          display: grid;
          grid-template-columns: 1fr 1fr;
          gap: 16px;
        }

        .stat-card {
          background: rgba(255, 255, 255, 0.05);
          border-radius: 12px;
          padding: 16px;
          text-align: center;
          border: 1px solid rgba(255, 255, 255, 0.1);
        }

        .stat-value {
          font-size: 24px;
          font-weight: 700;
          color: #00ff88;
          margin-bottom: 4px;
        }

        .stat-label {
          font-size: 12px;
          color: #8a92a5;
          text-transform: uppercase;
          letter-spacing: 0.5px;
        }
      </style>

      <div class="drawer-overlay ${this.isOpen ? 'open' : ''}"></div>
      <div class="drawer ${this.isOpen ? 'open' : ''}">
        <div class="drawer-header">
          <button class="close-btn">×</button>
          <div class="user-avatar-large" style="background: ${item.gradient};">
            ${item.avatar}
          </div>
          <div class="user-name">${item.name}</div>
          <div class="user-role">${item.role}</div>
          <div class="status-badge">
            <div class="status-dot"></div>
            ${item.status === 'online' ? 'Online' : 'Offline'}
          </div>
        </div>

        <div class="drawer-content">
          <div class="info-section">
            <div class="section-title">
              📋 Information
            </div>
            <div class="info-grid">
              <div class="info-item">
                <span class="info-label">Type</span>
                <span class="info-value">${isUser ? 'User' : 'AI Agent'}</span>
              </div>
              <div class="info-item">
                <span class="info-label">Session ID</span>
                <span class="info-value">${item.sessionId || 'N/A'}</span>
              </div>
              <div class="info-item">
                <span class="info-label">Last Active</span>
                <span class="info-value">${item.lastActive ? new Date(item.lastActive).toLocaleString() : 'N/A'}</span>
              </div>
              ${item.requests ? `
                <div class="info-item">
                  <span class="info-label">Requests</span>
                  <span class="info-value">${item.requests}</span>
                </div>
              ` : ''}
              ${item.cost ? `
                <div class="info-item">
                  <span class="info-label">Cost</span>
                  <span class="info-value">$${item.cost.toFixed(4)}</span>
                </div>
              ` : ''}
            </div>
          </div>

          ${isUser ? `
            <div class="info-section">
              <div class="section-title">
                📊 Activity Stats
              </div>
              <div class="stats-grid">
                <div class="stat-card">
                  <div class="stat-value">${Math.floor(Math.random() * 50) + 10}</div>
                  <div class="stat-label">Messages Sent</div>
                </div>
                <div class="stat-card">
                  <div class="stat-value">${Math.floor(Math.random() * 24) + 1}h</div>
                  <div class="stat-label">Time Online</div>
                </div>
                <div class="stat-card">
                  <div class="stat-value">${Math.floor(Math.random() * 10) + 1}</div>
                  <div class="stat-label">Projects</div>
                </div>
                <div class="stat-card">
                  <div class="stat-value">${Math.floor(Math.random() * 5) + 1}</div>
                  <div class="stat-label">AI Agents</div>
                </div>
              </div>
            </div>
          ` : ''}

          <div class="actions-section">
            <div class="section-title">
              ⚡ Actions
            </div>
            ${this.generateActionButtons(item)}
          </div>
        </div>
      </div>
    `;
  }
}

// Register the custom element
if (typeof customElements !== 'undefined') {
  customElements.define('user-drawer', UserDrawer);
}

// Export for Node.js
if (typeof module !== 'undefined' && module.exports) {
  module.exports = UserDrawer;
}