/**
 * Agent Selector Module
 * Handles Discord-style agent selection UI and logic
 */

class AgentSelector {
  constructor(options = {}) {
    this.agents = options.agents || this.getDefaultAgents();
    this.selectedAgent = options.defaultAgent || 'auto';
    this.selectedAgents = new Set();
    this.isGroupChat = false;
    this.onSelectionChange = options.onSelectionChange || (() => {});
    this.onGroupChatToggle = options.onGroupChatToggle || (() => {});
  }

  getDefaultAgents() {
    return [
      {
        id: 'auto',
        name: 'Auto Route',
        role: 'Smart agent selection',
        avatar: '🧠',
        gradient: 'linear-gradient(135deg, #4FC3F7, #29B6F6)',
        status: 'online'
      },
      {
        id: 'PlannerAI',
        name: 'PlannerAI',
        role: 'Strategy & web commands',
        avatar: '📋',
        gradient: 'linear-gradient(135deg, #9C27B0, #673AB7)',
        status: 'online'
      },
      {
        id: 'CodeAI',
        name: 'CodeAI',
        role: 'Code analysis & debugging',
        avatar: '💻',
        gradient: 'linear-gradient(135deg, #FF5722, #F44336)',
        status: 'online'
      },
      {
        id: 'GeneralAI',
        name: 'GeneralAI',
        role: 'General assistance',
        avatar: '💬',
        gradient: 'linear-gradient(135deg, #4CAF50, #8BC34A)',
        status: 'online'
      },
      {
        id: 'ProtocolSheriff',
        name: 'Protocol Sheriff',
        role: 'Response validation',
        avatar: '🛡️',
        gradient: 'linear-gradient(135deg, #FF9800, #FFC107)',
        status: 'online'
      }
    ];
  }

  selectAgent(agentId) {
    if (this.isGroupChat) {
      // Multi-select mode for group chat
      if (this.selectedAgents.has(agentId)) {
        this.selectedAgents.delete(agentId);
      } else {
        this.selectedAgents.add(agentId);
      }
    } else {
      // Single select mode
      this.selectedAgent = agentId;
      this.selectedAgents.clear();
    }

    this.onSelectionChange({
      selectedAgent: this.selectedAgent,
      selectedAgents: Array.from(this.selectedAgents),
      isGroupChat: this.isGroupChat
    });

    return this.getSelectionState();
  }

  toggleGroupChat() {
    this.isGroupChat = !this.isGroupChat;
    
    if (!this.isGroupChat) {
      // Return to single agent mode
      this.selectedAgents.clear();
      this.selectedAgent = 'auto';
    } else {
      // Clear single selection for group mode
      this.selectedAgents.clear();
    }

    this.onGroupChatToggle({
      isGroupChat: this.isGroupChat,
      selectedAgent: this.selectedAgent,
      selectedAgents: Array.from(this.selectedAgents)
    });

    return this.getSelectionState();
  }

  getSelectionState() {
    return {
      selectedAgent: this.selectedAgent,
      selectedAgents: Array.from(this.selectedAgents),
      isGroupChat: this.isGroupChat,
      agents: this.agents
    };
  }

  generateHTML() {
    return `
      <div class="agent-selector">
        <h3>Available Agents</h3>
        <div class="agent-list">
          ${this.agents.map(agent => `
            <div class="agent-item ${this.selectedAgent === agent.id ? 'selected' : ''}" 
                 onclick="selectAgent('${agent.id}')" 
                 id="agent-${agent.id}">
              <div class="agent-avatar" style="background: ${agent.gradient};">
                ${agent.avatar}
                <div class="agent-status ${agent.status}"></div>
              </div>
              <div class="agent-info">
                <div class="agent-name">${agent.name}</div>
                <div class="agent-role">${agent.role}</div>
              </div>
            </div>
          `).join('')}
        </div>
        
        <div class="multi-select">
          <button class="group-chat-btn" onclick="startGroupChat()">
            👥 Start Group Chat
          </button>
        </div>
      </div>
    `;
  }

  generateCSS() {
    return `
      .agent-selector {
        margin: 20px;
        background: rgba(255, 255, 255, 0.05);
        border-radius: 12px;
        padding: 15px;
      }
      
      .agent-selector h3 {
        font-size: 14px;
        color: #8a92a5;
        margin-bottom: 12px;
        text-transform: uppercase;
        letter-spacing: 0.5px;
      }
      
      .agent-list {
        display: flex;
        flex-direction: column;
        gap: 8px;
      }
      
      .agent-item {
        display: flex;
        align-items: center;
        padding: 10px 12px;
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
      .agent-status.training { background: #9C27B0; }
      
      .agent-info {
        flex: 1;
      }
      
      .agent-name {
        font-weight: 600;
        color: #e0e6ed;
        font-size: 14px;
      }
      
      .agent-role {
        font-size: 11px;
        color: #8a92a5;
        margin-top: 2px;
      }
      
      .multi-select {
        margin-top: 15px;
        padding-top: 15px;
        border-top: 1px solid rgba(255, 255, 255, 0.1);
      }
      
      .group-chat-btn {
        width: 100%;
        padding: 8px 12px;
        background: rgba(156, 39, 176, 0.1);
        border: 1px solid rgba(156, 39, 176, 0.3);
        border-radius: 8px;
        color: #BA68C8;
        font-size: 12px;
        cursor: pointer;
        transition: all 0.3s ease;
      }
      
      .group-chat-btn:hover {
        background: rgba(156, 39, 176, 0.2);
        border-color: rgba(156, 39, 176, 0.5);
      }
    `;
  }
}

module.exports = AgentSelector;