/**
 * Agent Selector Web Component
 * Self-contained component for selecting and managing agents
 */

class AgentSelector extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
    
    // Component state
    this.selectedAgent = 'auto';
    this.agents = this.getDefaultAgents();
    this.remoteAgents = [];
    this.onAgentSelect = null;
    this.onAgentInfo = null;
  }

  connectedCallback() {
    this.render();
    this.setupEventListeners();
  }

  disconnectedCallback() {
    // Cleanup if needed
  }

  getDefaultAgents() {
    return [
      {
        id: 'auto',
        name: 'Auto Route',
        role: 'Smart agent selection',
        avatar: '🧠',
        gradient: 'linear-gradient(135deg, #4FC3F7, #29B6F6)',
        status: 'online',
        type: 'system'
      },
      {
        id: 'PlannerAI',
        name: 'PlannerAI',
        role: 'Strategy & web commands',
        avatar: '📋',
        gradient: 'linear-gradient(135deg, #9C27B0, #673AB7)',
        status: 'online',
        type: 'ai'
      },
      {
        id: 'CodeAI',
        name: 'CodeAI',
        role: 'Code analysis & debugging',
        avatar: '💻',
        gradient: 'linear-gradient(135deg, #FF5722, #F44336)',
        status: 'online',
        type: 'ai'
      },
      {
        id: 'GeneralAI',
        name: 'GeneralAI',
        role: 'General assistance',
        avatar: '💬',
        gradient: 'linear-gradient(135deg, #4CAF50, #8BC34A)',
        status: 'online',
        type: 'ai'
      },
      {
        id: 'ProtocolSheriff',
        name: 'Protocol Sheriff',
        role: 'Response validation',
        avatar: '🛡️',
        gradient: 'linear-gradient(135deg, #FF9800, #FFC107)',
        status: 'online',
        type: 'ai'
      }
    ];
  }

  updateRemoteAgents(agents) {
    this.remoteAgents = agents || [];
    this.render();
  }

  selectAgent(agentId) {
    this.selectedAgent = agentId;
    this.updateSelectionState();
    
    if (this.onAgentSelect) {
      this.onAgentSelect(agentId);
    }
    
    // Dispatch custom event
    this.dispatchEvent(new CustomEvent('agent-selected', {
      detail: { agentId },
      bubbles: true
    }));
  }

  updateSelectionState() {
    const items = this.shadowRoot.querySelectorAll('.agent-item');
    items.forEach(item => {
      item.classList.remove('selected');
      if (item.dataset.agentId === this.selectedAgent) {
        item.classList.add('selected');
      }
    });
  }

  setupEventListeners() {
    this.shadowRoot.addEventListener('click', (e) => {
      if (e.target.closest('.agent-item')) {
        const agentId = e.target.closest('.agent-item').dataset.agentId;
        this.selectAgent(agentId);
      }
      
      if (e.target.closest('.agent-dropdown-btn')) {
        e.stopPropagation();
        const agentId = e.target.closest('.agent-item').dataset.agentId;
        this.showAgentInfo(agentId);
      }
    });
  }

  showAgentInfo(agentId) {
    if (this.onAgentInfo) {
      this.onAgentInfo(agentId);
    }
    
    this.dispatchEvent(new CustomEvent('agent-info-requested', {
      detail: { agentId },
      bubbles: true
    }));
  }

  generateAgentHTML(agent) {
    const isSelected = agent.id === this.selectedAgent;
    const isRemote = agent.source === 'remote';
    const isHuman = agent.type === 'human' || agent.type === 'user';
    
    return `
      <div class="agent-item ${isSelected ? 'selected' : ''}" data-agent-id="${agent.id}">
        <div class="agent-avatar" style="background: ${agent.gradient};">
          ${agent.avatar}
          <div class="agent-status ${agent.status}"></div>
        </div>
        <div class="agent-info">
          <div class="agent-name">
            ${agent.name}
            ${isRemote ? '<span class="remote-indicator">' + (isHuman ? 'H' : 'AI') + '</span>' : ''}
            ${agent.id !== 'auto' ? '<button class="agent-dropdown-btn" title="Agent details"></button>' : ''}
          </div>
          <div class="agent-role">${agent.role}</div>
          ${isRemote ? `<div class="agent-meta">${agent.hostInfo?.hostname || 'Unknown'} • ${agent.messageCount || 0} msgs</div>` : ''}
        </div>
      </div>
    `;
  }

  generateRemoteSection(title, agents, color) {
    if (agents.length === 0) return '';
    
    return `
      <div class="remote-section">
        <div class="remote-title" style="color: ${color};">${title}</div>
        ${agents.map(agent => this.generateAgentHTML(agent)).join('')}
      </div>
    `;
  }

  render() {
    const remoteHumans = this.remoteAgents.filter(agent => 
      agent.type === 'human' || agent.type === 'user');
    const remoteAIs = this.remoteAgents.filter(agent => 
      agent.type === 'ai' || agent.type === 'system');

    this.shadowRoot.innerHTML = `
      <style>
        :host {
          display: block;
          margin: 20px;
          background: rgba(255, 255, 255, 0.05);
          border-radius: 12px;
          padding: 15px;
          max-height: 400px;
          overflow-y: auto;
        }

        .title {
          font-size: 14px;
          color: #8a92a5;
          margin-bottom: 12px;
          text-transform: uppercase;
          letter-spacing: 0.5px;
          flex-shrink: 0;
        }

        .agent-list {
          display: flex;
          flex-direction: column;
          gap: 6px;
          flex: 1;
          overflow-y: auto;
          padding-right: 5px;
        }

        .agent-item {
          display: flex;
          align-items: center;
          padding: 8px 10px;
          border-radius: 8px;
          cursor: pointer;
          transition: all 0.3s ease;
          border: 1px solid transparent;
        }

        .agent-item:hover {
          background: rgba(255, 255, 255, 0.1);
        }

        .agent-item.selected {
          background: linear-gradient(135deg, rgba(79, 195, 247, 0.2), rgba(41, 182, 246, 0.2));
          border-color: rgba(79, 195, 247, 0.5);
        }

        .agent-avatar {
          width: 32px;
          height: 32px;
          border-radius: 50%;
          display: flex;
          align-items: center;
          justify-content: center;
          margin-right: 12px;
          font-size: 16px;
          position: relative;
        }

        .agent-status {
          position: absolute;
          bottom: -2px;
          right: -2px;
          width: 12px;
          height: 12px;
          border-radius: 50%;
          border: 2px solid rgba(20, 25, 35, 0.95);
        }

        .agent-status.online { background: #4CAF50; }
        .agent-status.busy { background: #FF9800; }
        .agent-status.offline { background: #666; }

        .agent-info {
          flex: 1;
          min-width: 0;
        }

        .agent-name {
          font-weight: 600;
          color: #e0e6ed;
          font-size: 14px;
          display: flex;
          align-items: center;
          gap: 6px;
        }

        .agent-role {
          font-size: 11px;
          color: #8a92a5;
          margin-top: 2px;
        }

        .agent-meta {
          font-size: 10px;
          color: #666;
          margin-top: 2px;
        }

        .remote-indicator {
          font-size: 9px;
          color: #666;
          padding: 1px 4px;
          background: rgba(0,0,0,0.3);
          border-radius: 2px;
        }

        .agent-dropdown-btn {
          background: transparent;
          border: none;
          color: rgba(0, 212, 255, 0.4);
          cursor: pointer;
          padding: 2px;
          width: 14px;
          height: 14px;
          font-size: 8px;
          border-radius: 2px;
          transition: all 0.3s ease;
          position: relative;
          display: flex;
          align-items: center;
          justify-content: center;
        }

        .agent-dropdown-btn::after {
          content: '⬢';
          font-size: 10px;
          color: rgba(0, 212, 255, 0.3);
          transition: all 0.3s ease;
        }

        .agent-dropdown-btn:hover::after {
          color: rgba(0, 212, 255, 0.8);
          text-shadow: 0 0 8px rgba(0, 212, 255, 0.6);
        }

        .remote-section {
          margin-top: 15px;
          padding-top: 15px;
          border-top: 1px solid rgba(255, 255, 255, 0.1);
        }

        .remote-title {
          font-size: 11px;
          text-transform: uppercase;
          letter-spacing: 0.5px;
          margin-bottom: 10px;
        }

        ::-webkit-scrollbar {
          width: 6px;
        }

        ::-webkit-scrollbar-track {
          background: rgba(255, 255, 255, 0.05);
          border-radius: 3px;
        }

        ::-webkit-scrollbar-thumb {
          background: rgba(0, 212, 255, 0.3);
          border-radius: 3px;
        }
      </style>

      <div class="title">Available Agents</div>
      <div class="agent-list">
        ${this.agents.map(agent => this.generateAgentHTML(agent)).join('')}
        ${this.generateRemoteSection('⚡ Network Operators', remoteHumans, '#ff6b6b')}
        ${this.generateRemoteSection('🌐 Network AI', remoteAIs, '#00d4ff')}
      </div>
    `;

    this.updateSelectionState();
  }

  // Public API
  setSelectedAgent(agentId) {
    this.selectAgent(agentId);
  }

  setOnAgentSelect(callback) {
    this.onAgentSelect = callback;
  }

  setOnAgentInfo(callback) {
    this.onAgentInfo = callback;
  }
}

// Register the custom element if in browser
if (typeof customElements !== 'undefined') {
  customElements.define('agent-selector', AgentSelector);
}

// Export for Node.js
if (typeof module !== 'undefined' && module.exports) {
  module.exports = AgentSelector;
}