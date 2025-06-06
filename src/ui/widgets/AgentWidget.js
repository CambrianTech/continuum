/**
 * AgentWidget - Modular agent display with cyberpunk expand functionality
 */

class AgentWidget extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
  }

  connectedCallback() {
    this.render();
    this.setupEventListeners();
  }

  static get observedAttributes() {
    return ['agent-name', 'agent-status', 'agent-type'];
  }

  attributeChangedCallback() {
    this.render();
  }

  render() {
    const agentName = this.getAttribute('agent-name') || 'Unknown Agent';
    const agentStatus = this.getAttribute('agent-status') || 'idle';
    const agentType = this.getAttribute('agent-type') || 'AI';

    this.shadowRoot.innerHTML = `
      <style>
        :host {
          display: block;
          margin: 8px 0;
        }

        .agent-widget {
          background: rgba(20, 25, 35, 0.8);
          border: 1px solid rgba(0, 212, 255, 0.3);
          border-radius: 8px;
          padding: 12px;
          position: relative;
          transition: all 0.3s ease;
          cursor: pointer;
        }

        .agent-widget:hover {
          border-color: rgba(0, 212, 255, 0.6);
          background: rgba(25, 30, 40, 0.9);
          transform: translateX(4px);
        }

        .agent-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
        }

        .agent-info {
          flex: 1;
        }

        .agent-name {
          color: #00d4ff;
          font-weight: 600;
          font-size: 14px;
          margin-bottom: 4px;
        }

        .agent-status {
          color: #8a92a5;
          font-size: 12px;
          text-transform: uppercase;
          letter-spacing: 0.5px;
        }

        .agent-status.active {
          color: #4CAF50;
        }

        .agent-status.busy {
          color: #FF9800;
        }

        .cyber-expand-btn {
          width: 24px;
          height: 24px;
          background: transparent;
          border: 1px solid rgba(0, 212, 255, 0.4);
          border-radius: 4px;
          color: #00d4ff;
          cursor: pointer;
          display: flex;
          align-items: center;
          justify-content: center;
          font-size: 12px;
          font-weight: bold;
          transition: all 0.2s ease;
          font-family: monospace;
        }

        .cyber-expand-btn:hover {
          background: rgba(0, 212, 255, 0.2);
          border-color: rgba(0, 212, 255, 0.8);
          box-shadow: 0 0 8px rgba(0, 212, 255, 0.4);
          transform: scale(1.1);
        }

        .cyber-expand-btn:active {
          transform: scale(0.95);
          background: rgba(0, 212, 255, 0.3);
        }

        .agent-type-indicator {
          position: absolute;
          top: 4px;
          right: 4px;
          width: 8px;
          height: 8px;
          border-radius: 50%;
          background: #4CAF50;
        }

        .agent-type-indicator.ai {
          background: #00d4ff;
        }

        .agent-type-indicator.human {
          background: #FFB74D;
        }
      </style>

      <div class="agent-widget">
        <div class="agent-type-indicator ${agentType.toLowerCase()}"></div>
        <div class="agent-header">
          <div class="agent-info">
            <div class="agent-name">${agentName}</div>
            <div class="agent-status ${agentStatus}">${agentStatus}</div>
          </div>
          <button class="cyber-expand-btn" title="Expand Agent Details">
            >>>
          </button>
        </div>
      </div>
    `;
  }

  setupEventListeners() {
    const expandBtn = this.shadowRoot.querySelector('.cyber-expand-btn');
    const agentWidget = this.shadowRoot.querySelector('.agent-widget');

    expandBtn.addEventListener('click', (e) => {
      e.stopPropagation();
      this.toggleAgentDrawer();
    });

    agentWidget.addEventListener('click', () => {
      this.selectAgent();
    });
  }

  toggleAgentDrawer() {
    console.log('🎯 Toggling cyberpunk drawer for agent:', this.getAttribute('agent-name'));
    
    // Dispatch custom event to trigger drawer
    const event = new CustomEvent('agent-expand', {
      detail: {
        agentName: this.getAttribute('agent-name'),
        agentType: this.getAttribute('agent-type'),
        agentStatus: this.getAttribute('agent-status')
      },
      bubbles: true
    });
    
    this.dispatchEvent(event);

    // Also trigger the existing drawer if available
    if (typeof window.toggleAgentDrawer === 'function') {
      window.toggleAgentDrawer();
    }
  }

  selectAgent() {
    console.log('🤖 Selected agent:', this.getAttribute('agent-name'));
    
    // Dispatch agent selection event
    const event = new CustomEvent('agent-select', {
      detail: {
        agentName: this.getAttribute('agent-name'),
        agentType: this.getAttribute('agent-type')
      },
      bubbles: true
    });
    
    this.dispatchEvent(event);
  }
}

// Register the custom element
customElements.define('agent-widget', AgentWidget);

// Export for module use
if (typeof module !== 'undefined' && module.exports) {
  module.exports = AgentWidget;
}