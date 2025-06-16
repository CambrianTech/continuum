/**
 * User Selector Widget
 * Modular user and agent selection widget extending SidebarWidget
 */

// Import sidebar widget functionality
import('../shared/SidebarWidget.js');

// Guard against duplicate declarations
if (!customElements.get('user-selector')) {

class UserSelector extends SidebarWidget {
  constructor() {
    super();
    
    // Widget metadata
    this.widgetName = 'UserSelector';
    this.widgetIcon = '👥';
    this.widgetCategory = 'Sidebar';
    
    // Component state
    this.selectedAgent = 'auto';
    this.agents = this.getDefaultAgents();
    this.remoteAgents = [];
    this.connectedUsers = this.getDefaultUsers();
    this.onAgentSelect = null;
    this.onAgentInfo = null;
    this.onDrawerOpen = null;
    
    // Enhanced capabilities
    this.searchQuery = '';
    this.favoriteAgents = new Set();
    this.agentMetrics = new Map();
    this.currentGlassSubmenu = null;
    this.searchTimeout = null;
  }

  connectedCallback() {
    this.render();
    this.setupEventListeners();
    this.setupWebSocketListeners();
    this.loadRealAgents();
  }

  disconnectedCallback() {
    // Cleanup glass submenu
    this.closeGlassSubmenu();
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

  getDefaultUsers() {
    return [
      {
        id: 'joel',
        name: 'joel',
        role: 'Project Owner',
        avatar: '👤',
        gradient: 'linear-gradient(135deg, #FFD700, #FFA500)',
        status: 'online',
        type: 'user',
        sessionId: 'local',
        lastActive: new Date().toISOString()
      },
      {
        id: 'claude-code',
        name: 'Claude Code',
        role: 'AI Assistant',
        avatar: '🤖',
        gradient: 'linear-gradient(135deg, #00ff88, #00cc6a)',
        status: 'online',
        type: 'assistant',
        sessionId: 'claude-code',
        lastActive: new Date().toISOString()
      }
    ];
  }

  updateRemoteAgents(agents) {
    // Simple assignment - styling fallbacks handled in render
    this.remoteAgents = (agents || []).map(agent => ({
      ...agent,
      source: 'remote',
      id: agent.agentId || agent.id || 'unknown',
      name: agent.agentName || agent.name || 'Unknown Agent',
      type: agent.agentType || agent.type || 'ai'
    }));
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
    super.setupEventListeners(); // CRITICAL: Enable collapse functionality
    
    this.shadowRoot.addEventListener('click', (e) => {
      if (e.target.closest('.drawer-btn')) {
        e.stopPropagation();
        const agentId = e.target.closest('.drawer-btn').dataset.agentId;
        this.openDrawer(agentId);
        return;
      }
      
      if (e.target.closest('.favorite-btn')) {
        e.stopPropagation();
        const agentId = e.target.closest('.favorite-btn').dataset.agentId;
        this.toggleFavorite(agentId);
        return;
      }
      
      if (e.target.closest('.agent-dropdown-btn')) {
        e.stopPropagation();
        const agentId = e.target.closest('.agent-item').dataset.agentId;
        this.showAgentInfo(agentId);
        return;
      }
      
      if (e.target.closest('.agent-item')) {
        const agentId = e.target.closest('.agent-item').dataset.agentId;
        this.selectAgent(agentId);
      }
    });

    // Search input listener with debouncing
    this.shadowRoot.addEventListener('input', (e) => {
      if (e.target.matches('.search-input')) {
        clearTimeout(this.searchTimeout);
        this.searchTimeout = setTimeout(() => {
          this.searchQuery = e.target.value.toLowerCase();
          this.filterAndRender();
        }, 300);
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

  toggleFavorite(agentId) {
    if (this.favoriteAgents.has(agentId)) {
      this.favoriteAgents.delete(agentId);
    } else {
      this.favoriteAgents.add(agentId);
    }
    this.filterAndRender();
    
    this.dispatchEvent(new CustomEvent('agent-favorite-toggled', {
      detail: { agentId, isFavorite: this.favoriteAgents.has(agentId) },
      bubbles: true
    }));
  }

  filterAndRender() {
    this.render();
  }

  generateFavoritesSection() {
    const favoriteAgents = [...this.agents, ...this.remoteAgents, ...this.connectedUsers]
      .filter(agent => this.favoriteAgents.has(agent.id));
    
    if (favoriteAgents.length === 0) return '';
    
    return `
      <div class="favorites-section">
        <div class="favorites-title">⭐ Favorites</div>
        ${favoriteAgents.map(agent => this.generateAgentHTML(agent)).join('')}
      </div>
    `;
  }

  openDrawer(agentId) {
    // Find the agent/user data
    const allItems = [...this.connectedUsers, ...this.agents, ...this.remoteAgents];
    const targetItem = allItems.find(item => item.id === agentId);
    
    // Show glass submenu instead of traditional drawer
    this.showGlassSubmenu(agentId, targetItem);
    
    if (this.onDrawerOpen) {
      this.onDrawerOpen(agentId, targetItem);
    }
    
    this.dispatchEvent(new CustomEvent('drawer-open-requested', {
      detail: { agentId, item: targetItem },
      bubbles: true
    }));
  }

  showGlassSubmenu(agentId, item) {
    // Close any existing submenu
    this.closeGlassSubmenu();
    
    // Find the agent element in the DOM
    const agentElement = this.shadowRoot.querySelector(`[data-agent-id="${agentId}"]`);
    if (!agentElement) return;
    
    // Get position relative to viewport
    const rect = agentElement.getBoundingClientRect();
    
    // Create glass submenu
    const submenu = document.createElement('div');
    submenu.className = 'glass-submenu';
    submenu.style.cssText = `
      position: fixed;
      left: ${rect.right + 15}px;
      top: ${rect.top}px;
      width: 0px;
      height: 70px;
      background: linear-gradient(135deg, 
          rgba(0, 212, 255, 0.4) 0%, 
          rgba(100, 200, 255, 0.25) 50%, 
          rgba(0, 212, 255, 0.35) 100%);
      backdrop-filter: blur(25px);
      -webkit-backdrop-filter: blur(25px);
      border: 3px solid rgba(0, 212, 255, 0.8);
      border-radius: 15px;
      z-index: 50000;
      overflow: hidden;
      transition: all 0.7s cubic-bezier(0.23, 1, 0.32, 1);
      box-shadow: 
          0 20px 60px rgba(0, 212, 255, 0.4),
          inset 0 4px 0 rgba(255, 255, 255, 0.5),
          0 0 0 2px rgba(0, 212, 255, 0.6),
          0 0 30px rgba(0, 212, 255, 0.3);
      opacity: 0;
      display: flex;
      align-items: center;
      transform: translateX(-20px) scale(0.9);
    `;
    
    // Create content based on agent type
    const isAI = item?.type === 'ai' || item?.type === 'assistant' || item?.type === 'system';
    const name = item?.name || agentId;
    
    submenu.innerHTML = `
      <div style="display: flex; align-items: center; gap: 15px; padding: 15px 20px; white-space: nowrap; height: 100%;">
        ${isAI ? `
          <button class="glass-btn academy" onclick="this.closest('.glass-submenu').dispatchEvent(new CustomEvent('academy-clicked', {detail: {agentId: '${agentId}', name: '${name}'}, bubbles: true}))" 
                  style="background: linear-gradient(135deg, rgba(0, 212, 255, 0.4), rgba(255, 255, 255, 0.3)); 
                         border: 2px solid rgba(0, 212, 255, 0.9); color: #ffffff; padding: 8px 12px; 
                         border-radius: 8px; cursor: pointer; font-weight: bold; font-size: 11px;
                         text-shadow: 0 2px 4px rgba(0, 0, 0, 0.5); transition: all 0.3s ease;
                         box-shadow: 0 4px 12px rgba(0, 212, 255, 0.3);">
              🎓 Academy
          </button>
        ` : ''}
        <button class="glass-btn projects" onclick="this.closest('.glass-submenu').dispatchEvent(new CustomEvent('projects-clicked', {detail: {agentId: '${agentId}', name: '${name}'}, bubbles: true}))" 
                style="background: linear-gradient(135deg, rgba(0, 212, 255, 0.4), rgba(255, 255, 255, 0.3)); 
                       border: 2px solid rgba(0, 212, 255, 0.9); color: #ffffff; padding: 8px 12px; 
                       border-radius: 8px; cursor: pointer; font-weight: bold; font-size: 11px;
                       text-shadow: 0 2px 4px rgba(0, 0, 0, 0.5); transition: all 0.3s ease;
                       box-shadow: 0 4px 12px rgba(0, 212, 255, 0.3);">
            📁 Projects
        </button>
        <button class="glass-btn deploy" onclick="this.closest('.glass-submenu').dispatchEvent(new CustomEvent('deploy-clicked', {detail: {agentId: '${agentId}', name: '${name}'}, bubbles: true}))" 
                style="background: linear-gradient(135deg, rgba(0, 212, 255, 0.4), rgba(255, 255, 255, 0.3)); 
                       border: 2px solid rgba(0, 212, 255, 0.9); color: #ffffff; padding: 8px 12px; 
                       border-radius: 8px; cursor: pointer; font-weight: bold; font-size: 11px;
                       text-shadow: 0 2px 4px rgba(0, 0, 0, 0.5); transition: all 0.3s ease;
                       box-shadow: 0 4px 12px rgba(0, 212, 255, 0.3);">
            🚀 Deploy
        </button>
      </div>
    `;
    
    // Add styles for button hover effects
    const style = document.createElement('style');
    style.textContent = `
      .glass-btn:hover {
        background: linear-gradient(135deg, rgba(0, 212, 255, 0.6), rgba(255, 255, 255, 0.4)) !important;
        transform: translateY(-2px) scale(1.05);
        box-shadow: 0 6px 20px rgba(0, 212, 255, 0.5) !important;
      }
    `;
    document.head.appendChild(style);
    
    // Append to document body (outside shadow DOM for positioning)
    document.body.appendChild(submenu);
    this.currentGlassSubmenu = submenu;
    
    // Add event listeners
    submenu.addEventListener('academy-clicked', (e) => {
      console.log('🎓 Academy clicked for', e.detail.name);
      this.dispatchEvent(new CustomEvent('agent-academy-requested', {
        detail: e.detail,
        bubbles: true
      }));
      this.closeGlassSubmenu();
    });
    
    submenu.addEventListener('projects-clicked', (e) => {
      console.log('📁 Projects clicked for', e.detail.name);
      this.dispatchEvent(new CustomEvent('agent-projects-requested', {
        detail: e.detail,
        bubbles: true
      }));
      this.closeGlassSubmenu();
    });
    
    submenu.addEventListener('deploy-clicked', (e) => {
      console.log('🚀 Deploy clicked for', e.detail.name);
      this.dispatchEvent(new CustomEvent('agent-deploy-requested', {
        detail: e.detail,
        bubbles: true
      }));
      this.closeGlassSubmenu();
    });
    
    // Animate in
    requestAnimationFrame(() => {
      submenu.style.width = '380px';
      submenu.style.opacity = '1';
      submenu.style.transform = 'translateX(0px) scale(1)';
    });
    
    // Auto-close after 10 seconds
    setTimeout(() => {
      this.closeGlassSubmenu();
    }, 10000);
  }

  closeGlassSubmenu() {
    if (this.currentGlassSubmenu) {
      const submenu = this.currentGlassSubmenu;
      submenu.style.width = '0px';
      submenu.style.opacity = '0';
      submenu.style.transform = 'translateX(-20px) scale(0.9)';
      
      setTimeout(() => {
        if (submenu.parentNode) {
          submenu.parentNode.removeChild(submenu);
        }
      }, 700);
      
      this.currentGlassSubmenu = null;
    }
  }

  generateAgentHTML(agent) {
    const isSelected = agent.id === this.selectedAgent;
    const isRemote = agent.category === 'remote' || agent.source === 'remote';
    const isUser = agent.category === 'user' || agent.type === 'user' || agent.type === 'assistant';
    const isLocal = agent.category === 'local';
    const isFavorite = this.favoriteAgents.has(agent.id);
    
    // Dynamic fallbacks based on agent category
    let avatar, gradient, role, status;
    
    if (isUser) {
      avatar = agent.avatar || '👤';
      gradient = agent.gradient || 'linear-gradient(135deg, #FFD700, #FFA500)';
      role = agent.role || 'Project Owner';
      status = agent.status || 'online';
    } else if (isRemote) {
      avatar = agent.avatar || '🤖';
      gradient = agent.gradient || 'linear-gradient(135deg, #00d4ff, #0099cc)';
      role = agent.role || 'Remote AI Assistant';
      status = agent.status || 'connected';
    } else {
      // Local agents
      avatar = agent.avatar || '🧠';
      gradient = agent.gradient || 'linear-gradient(135deg, #4FC3F7, #29B6F6)';
      role = agent.role || 'AI Assistant';
      status = agent.status || 'online';
    }
    
    // Filter based on search query
    if (this.searchQuery && !agent.name.toLowerCase().includes(this.searchQuery) && 
        !role.toLowerCase().includes(this.searchQuery)) {
      return '';
    }
    
    return `
      <div class="agent-item ${isSelected ? 'selected' : ''} ${isFavorite ? 'favorite' : ''}" data-agent-id="${agent.id}">
        <div class="agent-avatar" style="background: ${gradient};">
          ${avatar}
          <div class="agent-status ${status}"></div>
          ${isFavorite ? '<div class="favorite-star">⭐</div>' : ''}
        </div>
        <div class="agent-info">
          <div class="agent-name">
            ${agent.name || 'Unknown Agent'}
            ${isUser ? '<span class="user-indicator">USER</span>' : ''}
            ${isRemote ? '<span class="remote-indicator">AI</span>' : ''}
          </div>
          <div class="agent-role">${role}</div>
          ${isRemote ? `<div class="agent-meta">${agent.hostInfo?.hostname || 'Unknown'} • ${agent.messageCount || 0} msgs</div>` : ''}
          ${isUser ? `<div class="agent-meta">Last active: ${new Date(agent.lastActive || Date.now()).toLocaleTimeString()}</div>` : ''}
        </div>
        <div class="agent-actions">
          <button class="favorite-btn" data-agent-id="${agent.id}" title="${isFavorite ? 'Remove from favorites' : 'Add to favorites'}">
            ${isFavorite ? '★' : '☆'}
          </button>
          <button class="drawer-btn" data-agent-id="${agent.id}" title="View ${agent.name || 'Unknown Agent'} details">
            <span class="drawer-icon">&gt;&gt;</span>
          </button>
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

  getAllAgentsUnified() {
    // Combine all agents into one unified, dynamically sorted list
    const allAgents = [
      ...this.connectedUsers.map(user => ({...user, category: 'user', priority: 1})),
      ...this.agents.map(agent => ({...agent, category: 'local', priority: 2})),
      ...this.remoteAgents.map(agent => ({...agent, category: 'remote', priority: 3}))
    ];
    
    // Sort by priority first, then by name
    return allAgents.sort((a, b) => {
      if (a.priority !== b.priority) return a.priority - b.priority;
      return (a.name || '').localeCompare(b.name || '');
    });
  }

  render() {
    const headerTitle = this.getAttribute('title') || 'Users & Agents';
    
    const content = `
      <div class="search-container">
        <input type="text" class="search-input" placeholder="Search agents..." value="${this.searchQuery}">
        <div class="search-icon">🔍</div>
      </div>
      ${this.favoriteAgents.size > 0 ? this.generateFavoritesSection() : ''}
      <div class="agent-list">
        ${this.getAllAgentsUnified().map(agent => this.generateAgentHTML(agent)).join('')}
      </div>
    `;
    
    this.shadowRoot.innerHTML = `
      <style>
        ${this.getHeaderStyle()}
        
        /* Search container styles */
        .search-container {
          position: relative;
          margin-bottom: 15px;
        }
        
        .search-input {
          width: 100%;
          padding: 8px 12px 8px 35px;
          background: rgba(255, 255, 255, 0.08);
          border: 1px solid rgba(255, 255, 255, 0.15);
          border-radius: 8px;
          color: #e0e6ed;
          font-size: 13px;
          box-sizing: border-box;
          transition: all 0.3s ease;
        }
        
        .search-input::placeholder {
          color: #8a92a5;
        }
        
        .search-input:focus {
          outline: none;
          border-color: rgba(0, 212, 255, 0.5);
          background: rgba(255, 255, 255, 0.12);
          box-shadow: 0 0 8px rgba(0, 212, 255, 0.2);
        }
        
        .search-icon {
          position: absolute;
          left: 12px;
          top: 50%;
          transform: translateY(-50%);
          color: #8a92a5;
          font-size: 14px;
          pointer-events: none;
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
          padding: 10px 12px;
          border-radius: 8px;
          cursor: pointer;
          transition: all 0.3s ease;
          border: 1px solid transparent;
        }

        .agent-item:hover {
          background: rgba(255, 255, 255, 0.08);
          transition: all 0.2s ease;
        }

        .agent-item.selected {
          background: linear-gradient(135deg, rgba(79, 195, 247, 0.15), rgba(41, 182, 246, 0.15));
          border-color: rgba(79, 195, 247, 0.4);
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

        .user-indicator {
          font-size: 8px;
          color: #FFD700;
          padding: 1px 4px;
          background: rgba(255, 215, 0, 0.2);
          border-radius: 2px;
          border: 1px solid rgba(255, 215, 0, 0.3);
        }

        .agent-actions {
          display: flex;
          align-items: center;
          gap: 4px;
          margin-left: auto;
        }
        
        .favorite-btn {
          background: transparent;
          border: none;
          color: #8a92a5;
          cursor: pointer;
          padding: 4px;
          width: 20px;
          height: 20px;
          border-radius: 4px;
          transition: all 0.3s ease;
          display: flex;
          align-items: center;
          justify-content: center;
          font-size: 12px;
        }
        
        .favorite-btn:hover {
          color: #FFD700;
          background: rgba(255, 215, 0, 0.1);
        }
        
        .agent-item.favorite .favorite-btn {
          color: #FFD700;
        }

        .drawer-btn {
          background: transparent;
          border: 1px solid rgba(0, 212, 255, 0.3);
          color: rgba(0, 212, 255, 0.6);
          cursor: pointer;
          padding: 4px 8px;
          height: 24px;
          border-radius: 4px;
          transition: all 0.3s ease;
          display: flex;
          align-items: center;
          justify-content: center;
          font-family: 'Courier New', monospace;
          font-size: 10px;
          letter-spacing: 1px;
          opacity: 0.7;
        }

        .drawer-btn:hover {
          border-color: rgba(0, 212, 255, 0.8);
          color: rgba(0, 212, 255, 1);
          background: rgba(0, 212, 255, 0.1);
          box-shadow: 0 0 8px rgba(0, 212, 255, 0.3);
          opacity: 1;
          transform: translateX(2px);
        }

        .drawer-icon {
          font-size: 10px;
          font-weight: normal;
        }
      </style>
      
      ${this.renderSidebarStructure(headerTitle, content)}
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

  addConnectedUser(user) {
    // Check if user already exists
    const existingIndex = this.connectedUsers.findIndex(u => u.sessionId === user.sessionId);
    if (existingIndex >= 0) {
      // Update existing user
      this.connectedUsers[existingIndex] = user;
    } else {
      // Add new user
      this.connectedUsers.push(user);
    }
    this.render();
  }

  removeConnectedUser(sessionId) {
    this.connectedUsers = this.connectedUsers.filter(u => u.sessionId !== sessionId);
    this.render();
  }

  setupWebSocketListeners() {
    if (typeof window !== 'undefined' && window.ws) {
      window.ws.addEventListener('message', (event) => {
        const data = JSON.parse(event.data);
        if (data.type === 'agents_update') {
          console.log('🤖 UserSelector: Received real agents update:', data.agents);
          this.updateRealAgents(data.agents);
        }
      });
    }
  }

  async loadRealAgents() {
    try {
      // Try to get current agents from server
      const response = await fetch('/api/agents');
      if (response.ok) {
        const agents = await response.json();
        console.log('🤖 UserSelector: Loaded real agents from API:', agents);
        this.updateRealAgents(agents);
      }
    } catch (error) {
      console.log('🤖 UserSelector: Using default agents (API not available)');
    }
  }

  updateRealAgents(realAgents) {
    // Keep the Auto Route option but replace other hardcoded agents with real ones
    const autoRoute = this.agents.find(a => a.id === 'auto');
    this.agents = [autoRoute, ...realAgents.map(agent => ({
      id: agent.id || agent.agentId || 'unknown',
      name: agent.name || agent.agentName || 'Unknown Agent',
      role: agent.role || agent.capabilities?.join(', ') || 'General',
      avatar: this.getAgentAvatar(agent.type || agent.agentType),
      gradient: this.getAgentGradient(agent.type || agent.agentType),
      status: agent.status || 'online',
      type: agent.type || agent.agentType || 'ai',
      source: agent.source || 'remote',
      host: agent.host || agent.hostInfo?.hostname || 'Unknown'
    }))];
    this.render();
  }

  getAgentAvatar(type) {
    const avatars = {
      'ai': '🤖',
      'human': '👤', 
      'user': '👨‍💻',
      'system': '⚙️'
    };
    return avatars[type] || '🤖';
  }

  getAgentGradient(type) {
    const gradients = {
      'ai': 'linear-gradient(135deg, #4FC3F7, #29B6F6)',
      'human': 'linear-gradient(135deg, #ff6b6b, #ee5a24)',
      'user': 'linear-gradient(135deg, #5f27cd, #341f97)',
      'system': 'linear-gradient(135deg, #00d2d3, #54a0ff)'
    };
    return gradients[type] || 'linear-gradient(135deg, #4FC3F7, #29B6F6)';
  }
}

// Register the custom element
customElements.define('user-selector', UserSelector);

// Export for module systems
if (typeof module !== 'undefined' && module.exports) {
  module.exports = UserSelector;
} else if (typeof window !== 'undefined') {
  window.UserSelector = UserSelector;
}

} // End guard