/**
 * GlassMenu - Cool blue semi-transparent glass pane widget
 * Star Trek TNG inspired glass menu overlay
 */

class GlassMenu extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
    
    this.state = {
      visible: false,
      agent: null,
      position: { x: 0, y: 0 }
    };
  }

  setState(newState) {
    this.state = { ...this.state, ...newState };
    this.render();
  }

  show(agent, x, y) {
    this.setState({
      visible: true,
      agent: agent,
      position: { x, y }
    });
  }

  hide() {
    this.setState({ visible: false });
  }

  render() {
    if (!this.state.visible) {
      this.shadowRoot.innerHTML = '';
      return;
    }

    const { agent, position } = this.state;
    
    this.shadowRoot.innerHTML = `
      <style>
        :host {
          position: fixed;
          top: 0;
          left: 0;
          width: 100vw;
          height: 100vh;
          z-index: 1000;
          pointer-events: ${this.state.visible ? 'all' : 'none'};
        }
        
        .glass-overlay {
          position: absolute;
          top: 0;
          left: 0;
          width: 100%;
          height: 100%;
          background: rgba(0, 20, 40, 0.3);
          backdrop-filter: blur(8px);
          animation: fadeIn 0.3s ease;
        }
        
        .glass-menu {
          position: absolute;
          left: ${position.x}px;
          top: ${position.y}px;
          width: 280px;
          background: rgba(79, 195, 247, 0.15);
          backdrop-filter: blur(20px);
          border: 1px solid rgba(79, 195, 247, 0.3);
          border-radius: 12px;
          padding: 20px;
          box-shadow: 
            0 8px 32px rgba(0, 0, 0, 0.3),
            inset 0 1px 0 rgba(255, 255, 255, 0.1);
          animation: glassSlideIn 0.4s cubic-bezier(0.34, 1.56, 0.64, 1);
        }
        
        .glass-header {
          display: flex;
          align-items: center;
          margin-bottom: 16px;
          padding-bottom: 12px;
          border-bottom: 1px solid rgba(79, 195, 247, 0.2);
        }
        
        .agent-avatar {
          width: 32px;
          height: 32px;
          margin-right: 12px;
          font-size: 20px;
          display: flex;
          align-items: center;
          justify-content: center;
          background: rgba(79, 195, 247, 0.2);
          border-radius: 8px;
        }
        
        .agent-details {
          flex: 1;
        }
        
        .agent-name {
          color: #ffffff;
          font-size: 16px;
          font-weight: 600;
          margin-bottom: 2px;
        }
        
        .agent-type {
          color: rgba(79, 195, 247, 0.8);
          font-size: 12px;
          text-transform: uppercase;
          letter-spacing: 1px;
        }
        
        .menu-actions {
          display: flex;
          flex-direction: column;
          gap: 8px;
        }
        
        .glass-button {
          background: rgba(79, 195, 247, 0.2);
          border: 1px solid rgba(79, 195, 247, 0.4);
          border-radius: 8px;
          padding: 12px 16px;
          color: #ffffff;
          font-size: 14px;
          font-weight: 500;
          cursor: pointer;
          transition: all 0.3s ease;
          text-align: left;
          display: flex;
          align-items: center;
          gap: 10px;
        }
        
        .glass-button:hover {
          background: rgba(79, 195, 247, 0.3);
          border-color: rgba(79, 195, 247, 0.6);
          transform: translateY(-1px);
          box-shadow: 0 4px 12px rgba(79, 195, 247, 0.2);
        }
        
        .glass-button:active {
          transform: translateY(0);
        }
        
        .close-btn {
          position: absolute;
          top: 8px;
          right: 8px;
          width: 24px;
          height: 24px;
          background: rgba(255, 255, 255, 0.1);
          border: none;
          border-radius: 50%;
          color: rgba(255, 255, 255, 0.7);
          cursor: pointer;
          display: flex;
          align-items: center;
          justify-content: center;
          font-size: 12px;
          transition: all 0.2s ease;
        }
        
        .close-btn:hover {
          background: rgba(255, 255, 255, 0.2);
          color: #ffffff;
        }
        
        @keyframes fadeIn {
          from { opacity: 0; }
          to { opacity: 1; }
        }
        
        @keyframes glassSlideIn {
          from { 
            opacity: 0;
            transform: scale(0.8) translateY(-20px);
          }
          to { 
            opacity: 1;
            transform: scale(1) translateY(0);
          }
        }
      </style>
      
      <div class="glass-overlay"></div>
      
      <div class="glass-menu">
        <button class="close-btn">×</button>
        
        <div class="glass-header">
          <div class="agent-avatar">${agent.avatar}</div>
          <div class="agent-details">
            <div class="agent-name">${agent.name}</div>
            <div class="agent-type">${agent.type}</div>
          </div>
        </div>
        
        <div class="menu-actions">
          <button class="glass-button" data-action="chat">
            💬 Start Chat
          </button>
          <button class="glass-button" data-action="info">
            ℹ️ Agent Info
          </button>
          <button class="glass-button" data-action="settings">
            ⚙️ Settings
          </button>
          <button class="glass-button" data-action="retrain">
            🎓 Retrain
          </button>
        </div>
      </div>
    `;
    
    this.attachEvents();
  }

  attachEvents() {
    // Close on overlay click
    const overlay = this.shadowRoot.querySelector('.glass-overlay');
    overlay?.addEventListener('click', () => this.hide());
    
    // Close button
    const closeBtn = this.shadowRoot.querySelector('.close-btn');
    closeBtn?.addEventListener('click', () => this.hide());
    
    // Action buttons
    this.shadowRoot.querySelectorAll('.glass-button').forEach(btn => {
      btn.addEventListener('click', (e) => {
        const action = e.target.dataset.action;
        console.log(`Glass menu action: ${action} for agent:`, this.state.agent);
        this.hide();
        
        // Emit custom event for parent to handle
        this.dispatchEvent(new CustomEvent('glass-action', {
          detail: { action, agent: this.state.agent }
        }));
      });
    });
  }

  connectedCallback() {
    this.render();
  }
}

customElements.define('glass-menu', GlassMenu);