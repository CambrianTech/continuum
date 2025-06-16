/**
 * SimpleAgentSelector - React-like AI Widget
 * Clean, minimal component for agent selection
 */

class SimpleAgentSelector extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
    
    // Simple state
    this.state = {
      agents: [
        { id: 'claude', name: 'Claude Code', type: 'ai', avatar: '🤖', status: 'online' },
        { id: 'joel', name: 'joel', type: 'user', avatar: '👤', status: 'online' },
        { id: 'auto', name: 'Auto Route', type: 'system', avatar: '🧠', status: 'online' },
        { id: 'codeai', name: 'CodeAI', type: 'ai', avatar: '💻', status: 'online' },
        { id: 'generalai', name: 'GeneralAI', type: 'ai', avatar: '💬', status: 'online' }
      ],
      selected: 'auto',
      searchQuery: ''
    };
  }

  // React-like setState
  setState(newState) {
    this.state = { ...this.state, ...newState };
    this.render();
  }

  // Simple render method
  render() {
    const filteredAgents = this.state.agents.filter(agent => 
      agent.name.toLowerCase().includes(this.state.searchQuery.toLowerCase())
    );

    this.shadowRoot.innerHTML = `
      <style>
        :host {
          display: block;
          margin: 20px;
          background: rgba(255, 255, 255, 0.05);
          border-radius: 12px;
          padding: 16px;
          border: 1px solid rgba(255, 255, 255, 0.1);
        }
        
        .header {
          color: #e0e6ed;
          font-size: 14px;
          font-weight: 600;
          text-transform: uppercase;
          margin-bottom: 15px;
          letter-spacing: 0.5px;
          font-weight: 600;
          margin-bottom: 12px;
          text-transform: uppercase;
          letter-spacing: 0.5px;
        }
        
        .search {
          width: calc(100% - 24px);
          box-sizing: border-box;
          background: rgba(255, 255, 255, 0.08);
          border: 1px solid rgba(255, 255, 255, 0.2);
          border-radius: 8px;
          padding: 8px 12px;
          color: #e0e6ed;
          font-size: 14px;
          margin-bottom: 12px;
          outline: none;
        }
        
        .search::placeholder {
          color: #8a92a5;
        }
        
        .agent-list {
          display: flex;
          flex-direction: column;
          gap: 8px;
        }
        
        .agent {
          display: flex;
          align-items: center;
          padding: 10px;
          border-radius: 8px;
          cursor: pointer;
          transition: all 0.2s ease;
          border: 1px solid transparent;
        }
        
        .agent:hover {
          background: rgba(255, 255, 255, 0.1);
        }
        
        .agent.selected {
          background: rgba(79, 195, 247, 0.2);
          border-color: rgba(79, 195, 247, 0.5);
        }
        
        .agent-avatar {
          width: 24px;
          height: 24px;
          margin-right: 12px;
          font-size: 16px;
          display: flex;
          align-items: center;
          justify-content: center;
        }
        
        .agent-info {
          flex: 1;
        }
        
        .agent-name {
          color: #e0e6ed;
          font-size: 14px;
          font-weight: 500;
        }
        
        .agent-type {
          color: #8a92a5;
          font-size: 12px;
          text-transform: uppercase;
          letter-spacing: 0.5px;
        }
        
        .status {
          width: 8px;
          height: 8px;
          border-radius: 50%;
          background: #4CAF50;
          margin-left: 8px;
        }
        
        .glass-menu-btn {
          background: rgba(79, 195, 247, 0.2);
          border: 1px solid rgba(79, 195, 247, 0.3);
          border-radius: 6px;
          padding: 4px 8px;
          color: rgba(79, 195, 247, 0.9);
          font-size: 14px;
          font-weight: bold;
          cursor: pointer;
          margin-left: 8px;
          transition: all 0.2s ease;
          opacity: 0;
        }
        
        .agent:hover .glass-menu-btn {
          opacity: 1;
        }
        
        .glass-menu-btn:hover {
          background: rgba(79, 195, 247, 0.4);
          border-color: rgba(79, 195, 247, 0.6);
          transform: scale(1.1);
        }
      </style>
      
      <div class="header">Users & Agents</div>
      
      <input 
        type="text" 
        class="search" 
        placeholder="Search agents..."
        value="${this.state.searchQuery}"
      />
      
      <div class="agent-list">
        ${filteredAgents.map(agent => `
          <div class="agent ${agent.id === this.state.selected ? 'selected' : ''}" data-id="${agent.id}">
            <div class="agent-avatar">${agent.avatar}</div>
            <div class="agent-info">
              <div class="agent-name">${agent.name}</div>
              <div class="agent-type">${agent.type}</div>
            </div>
            <div class="status"></div>
            <button class="glass-menu-btn" data-agent-id="${agent.id}">»</button>
          </div>
        `).join('')}
      </div>
    `;
    
    this.attachEvents();
  }

  // Simple event handling
  attachEvents() {
    // Search
    const search = this.shadowRoot.querySelector('.search');
    search.addEventListener('input', (e) => {
      this.setState({ searchQuery: e.target.value });
    });
    
    // Agent selection
    this.shadowRoot.querySelectorAll('.agent').forEach(agent => {
      agent.addEventListener('click', (e) => {
        // Don't select agent if clicking the glass menu button
        if (e.target.classList.contains('glass-menu-btn')) return;
        
        const agentId = agent.dataset.id;
        this.setState({ selected: agentId });
        console.log('Selected agent:', agentId);
      });
    });
    
    // Glass menu buttons
    this.shadowRoot.querySelectorAll('.glass-menu-btn').forEach(btn => {
      btn.addEventListener('click', (e) => {
        e.stopPropagation(); // Don't trigger agent selection
        const agentId = btn.dataset.agentId;
        const agent = this.state.agents.find(a => a.id === agentId);
        
        // Get button position for menu placement
        const rect = btn.getBoundingClientRect();
        const x = rect.left - 280; // Menu width offset
        const y = rect.top;
        
        // Show glass menu
        this.showGlassMenu(agent, x, y);
      });
    });
  }
  
  showGlassMenu(agent, x, y) {
    // Find or create glass menu
    let glassMenu = document.querySelector('glass-menu');
    if (!glassMenu) {
      glassMenu = document.createElement('glass-menu');
      document.body.appendChild(glassMenu);
    }
    
    glassMenu.show(agent, x, y);
  }

  connectedCallback() {
    this.render();
  }
}

// Register the component
customElements.define('simple-agent-selector', SimpleAgentSelector);