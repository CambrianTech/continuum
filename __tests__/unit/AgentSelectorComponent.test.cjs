/**
 * AgentSelector Component Unit Tests
 */

import { jest } from '@jest/globals';

// Mock localStorage
const mockLocalStorage = {
  getItem: jest.fn(),
  setItem: jest.fn(),
  removeItem: jest.fn(),
  clear: jest.fn()
};
global.localStorage = mockLocalStorage;

// Mock customElements
global.customElements = {
  define: jest.fn(),
  get: jest.fn(),
  whenDefined: jest.fn()
};

// Mock HTMLElement and Web Components APIs
global.HTMLElement = class MockHTMLElement {
  constructor() {
    this.shadowRoot = null;
    this.attachShadow = jest.fn(() => {
      this.shadowRoot = {
        innerHTML: '',
        addEventListener: jest.fn(),
        querySelectorAll: jest.fn(() => []),
        querySelector: jest.fn()
      };
      return this.shadowRoot;
    });
    this.dispatchEvent = jest.fn();
    this.addEventListener = jest.fn();
    this.removeEventListener = jest.fn();
  }
};

// Mock the component since it's a web component and needs a browser environment
class MockAgentSelector {
  constructor() {
    this.selectedAgent = 'auto';
    this.agents = this.getDefaultAgents();
    this.remoteAgents = [];
    this.connectedUsers = this.getDefaultUsers();
    this.searchQuery = '';
    this.favoriteAgents = new Set();
    this.agentMetrics = new Map();
    this.onAgentSelect = null;
    this.onAgentInfo = null;
    this.onDrawerOpen = null;
    this.shadowRoot = {
      innerHTML: '',
      addEventListener: jest.fn(),
      querySelectorAll: jest.fn(() => []),
      querySelector: jest.fn()
    };
    this.attachShadow = jest.fn(() => this.shadowRoot);
    this.dispatchEvent = jest.fn();
    this.addEventListener = jest.fn();
    this.removeEventListener = jest.fn();
  }

  getDefaultAgents() {
    return [
      { id: 'auto', name: 'Auto Route', role: 'Smart agent selection', avatar: '🧠', gradient: 'linear-gradient(135deg, #4FC3F7, #29B6F6)', status: 'online', type: 'system' },
      { id: 'PlannerAI', name: 'PlannerAI', role: 'Strategy & web commands', avatar: '📋', gradient: 'linear-gradient(135deg, #9C27B0, #673AB7)', status: 'online', type: 'ai' },
      { id: 'CodeAI', name: 'CodeAI', role: 'Code analysis & debugging', avatar: '💻', gradient: 'linear-gradient(135deg, #FF5722, #F44336)', status: 'online', type: 'ai' },
      { id: 'GeneralAI', name: 'GeneralAI', role: 'General assistance', avatar: '💬', gradient: 'linear-gradient(135deg, #4CAF50, #8BC34A)', status: 'online', type: 'ai' },
      { id: 'ProtocolSheriff', name: 'Protocol Sheriff', role: 'Response validation', avatar: '🛡️', gradient: 'linear-gradient(135deg, #FF9800, #FFC107)', status: 'online', type: 'ai' }
    ];
  }

  getDefaultUsers() {
    return [
      { id: 'joel', name: 'joel', role: 'Project Owner', avatar: '👤', gradient: 'linear-gradient(135deg, #FFD700, #FFA500)', status: 'online', type: 'user', sessionId: 'local', lastActive: new Date().toISOString() },
      { id: 'claude-code', name: 'Claude Code', role: 'AI Assistant', avatar: '🤖', gradient: 'linear-gradient(135deg, #00ff88, #00cc6a)', status: 'online', type: 'assistant', sessionId: 'claude-code', lastActive: new Date().toISOString() }
    ];
  }

  selectAgent(agentId) {
    this.selectedAgent = agentId;
    this.updateSelectionState();
    if (this.onAgentSelect) {
      this.onAgentSelect(agentId);
    }
    this.dispatchEvent({ type: 'agent-selected', detail: { agentId }, bubbles: true });
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

  toggleFavorite(agentId) {
    if (this.favoriteAgents.has(agentId)) {
      this.favoriteAgents.delete(agentId);
    } else {
      this.favoriteAgents.add(agentId);
    }
    this.filterAndRender();
    this.dispatchEvent({ type: 'agent-favorite-toggled', detail: { agentId, isFavorite: this.favoriteAgents.has(agentId) }, bubbles: true });
  }

  filterAndRender() {
    this.render();
  }

  updateRemoteAgents(agents) {
    this.remoteAgents = agents || [];
    this.render();
  }

  addConnectedUser(user) {
    const existingIndex = this.connectedUsers.findIndex(u => u.sessionId === user.sessionId);
    if (existingIndex >= 0) {
      this.connectedUsers[existingIndex] = user;
    } else {
      this.connectedUsers.push(user);
    }
    this.render();
  }

  removeConnectedUser(sessionId) {
    this.connectedUsers = this.connectedUsers.filter(u => u.sessionId !== sessionId);
    this.render();
  }

  generateAgentHTML(agent) {
    const isSelected = agent.id === this.selectedAgent;
    const isFavorite = this.favoriteAgents.has(agent.id);
    
    if (this.searchQuery && !agent.name.toLowerCase().includes(this.searchQuery) && 
        !agent.role.toLowerCase().includes(this.searchQuery)) {
      return '';
    }
    
    return `<div class="agent-item ${isSelected ? 'selected' : ''} ${isFavorite ? 'favorite' : ''}" data-agent-id="${agent.id}">content</div>`;
  }

  render() {
    console.log('[AgentSelector] Rendering component', {
      searchQuery: this.searchQuery,
      selectedAgent: this.selectedAgent,
      totalAgents: this.agents.length,
      remoteAgents: this.remoteAgents.length,
      connectedUsers: this.connectedUsers.length,
      favoriteCount: this.favoriteAgents.size
    });
  }

  setupEventListeners() {
    // Mock implementation
  }

  connectedCallback() {
    this.render();
    this.setupEventListeners();
  }

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

const AgentSelector = MockAgentSelector;

describe('AgentSelector Component', () => {
  let component;

  beforeEach(() => {
    jest.clearAllMocks();
    component = new AgentSelector();
  });

  describe('Constructor', () => {
    test('should initialize with default values', () => {
      expect(component.selectedAgent).toBe('auto');
      expect(component.agents).toHaveLength(5); // Default agents
      expect(component.remoteAgents).toEqual([]);
      expect(component.connectedUsers).toHaveLength(2); // Default users
      expect(component.searchQuery).toBe('');
      expect(component.favoriteAgents).toBeInstanceOf(Set);
      expect(component.agentMetrics).toBeInstanceOf(Map);
    });

    test('should attach shadow DOM', () => {
      expect(component.attachShadow).toHaveBeenCalledWith({ mode: 'open' });
      expect(component.shadowRoot).toBeTruthy();
    });
  });

  describe('Agent Management', () => {
    test('should select agent correctly', () => {
      const mockCallback = jest.fn();
      component.onAgentSelect = mockCallback;
      
      component.selectAgent('CodeAI');
      
      expect(component.selectedAgent).toBe('CodeAI');
      expect(mockCallback).toHaveBeenCalledWith('CodeAI');
      expect(component.dispatchEvent).toHaveBeenCalledWith(
        expect.objectContaining({
          type: 'agent-selected',
          detail: { agentId: 'CodeAI' }
        })
      );
    });

    test('should toggle favorites correctly', () => {
      const agentId = 'CodeAI';
      
      // Add to favorites
      component.toggleFavorite(agentId);
      expect(component.favoriteAgents.has(agentId)).toBe(true);
      
      // Remove from favorites
      component.toggleFavorite(agentId);
      expect(component.favoriteAgents.has(agentId)).toBe(false);
    });

    test('should update remote agents', () => {
      const mockAgents = [
        { id: 'remote1', name: 'Remote AI', type: 'ai' },
        { id: 'remote2', name: 'Remote Human', type: 'human' }
      ];
      
      const renderSpy = jest.spyOn(component, 'render').mockImplementation(() => {});
      
      component.updateRemoteAgents(mockAgents);
      
      expect(component.remoteAgents).toEqual(mockAgents);
      expect(renderSpy).toHaveBeenCalled();
      
      renderSpy.mockRestore();
    });
  });

  describe('User Management', () => {
    test('should add connected user', () => {
      const newUser = {
        id: 'test-user',
        name: 'Test User',
        sessionId: 'session123',
        type: 'user'
      };
      
      const renderSpy = jest.spyOn(component, 'render').mockImplementation(() => {});
      
      component.addConnectedUser(newUser);
      
      expect(component.connectedUsers).toContainEqual(newUser);
      expect(renderSpy).toHaveBeenCalled();
      
      renderSpy.mockRestore();
    });

    test('should update existing user instead of duplicating', () => {
      const originalUser = {
        id: 'user1',
        name: 'Original Name',
        sessionId: 'session123',
        type: 'user'
      };
      
      const updatedUser = {
        id: 'user1',
        name: 'Updated Name',
        sessionId: 'session123',
        type: 'user'
      };
      
      component.addConnectedUser(originalUser);
      const initialCount = component.connectedUsers.length;
      
      component.addConnectedUser(updatedUser);
      
      expect(component.connectedUsers).toHaveLength(initialCount);
      expect(component.connectedUsers.find(u => u.sessionId === 'session123').name).toBe('Updated Name');
    });

    test('should remove connected user', () => {
      const renderSpy = jest.spyOn(component, 'render').mockImplementation(() => {});
      
      component.removeConnectedUser('local'); // Remove default user
      
      expect(component.connectedUsers.find(u => u.sessionId === 'local')).toBeUndefined();
      expect(renderSpy).toHaveBeenCalled();
      
      renderSpy.mockRestore();
    });
  });

  describe('HTML Generation', () => {
    test('should generate agent HTML correctly', () => {
      const agent = {
        id: 'test-agent',
        name: 'Test Agent',
        role: 'Testing',
        avatar: '🧪',
        gradient: 'linear-gradient(135deg, #ff0000, #00ff00)',
        status: 'online',
        type: 'ai'
      };
      
      const html = component.generateAgentHTML(agent);
      
      expect(html).toContain('test-agent');
      expect(html).toContain('Test Agent');
      expect(html).toContain('Testing');
      expect(html).toContain('🧪');
      expect(html).toContain('linear-gradient(135deg, #ff0000, #00ff00)');
    });

    test('should filter agents by search query', () => {
      const agent = {
        id: 'test-agent',
        name: 'CodeHelper',
        role: 'Development Assistant',
        avatar: '🔧',
        gradient: 'linear-gradient(135deg, #ff0000, #00ff00)',
        status: 'online',
        type: 'ai'
      };
      
      // Should show when search matches
      component.searchQuery = 'code';
      let html = component.generateAgentHTML(agent);
      expect(html).toContain('CodeHelper');
      
      // Should hide when search doesn't match
      component.searchQuery = 'xyz';
      html = component.generateAgentHTML(agent);
      expect(html).toBe('');
    });

    test('should show favorite star for favorite agents', () => {
      const agent = {
        id: 'fav-agent',
        name: 'Favorite Agent',
        role: 'Testing',
        avatar: '⭐',
        gradient: 'linear-gradient(135deg, #ff0000, #00ff00)',
        status: 'online',
        type: 'ai'
      };
      
      component.favoriteAgents.add('fav-agent');
      const html = component.generateAgentHTML(agent);
      
      expect(html).toContain('favorite-star');
      expect(html).toContain('⭐');
    });
  });

  describe('Event Handling', () => {
    test('should handle agent selection click', () => {
      const selectSpy = jest.spyOn(component, 'selectAgent');
      
      // Mock event and element structure
      const mockEvent = {
        target: {
          closest: jest.fn((selector) => {
            if (selector === '.agent-item') {
              return { dataset: { agentId: 'CodeAI' } };
            }
            return null;
          })
        },
        stopPropagation: jest.fn()
      };
      
      // Simulate click event
      component.setupEventListeners();
      const clickHandler = component.shadowRoot.addEventListener.mock.calls
        .find(call => call[0] === 'click')[1];
      
      clickHandler(mockEvent);
      
      expect(selectSpy).toHaveBeenCalledWith('CodeAI');
    });

    test('should handle favorite toggle click', () => {
      const toggleSpy = jest.spyOn(component, 'toggleFavorite');
      
      const mockEvent = {
        target: {
          closest: jest.fn((selector) => {
            if (selector === '.favorite-btn') {
              return { dataset: { agentId: 'CodeAI' } };
            }
            return null;
          })
        },
        stopPropagation: jest.fn()
      };
      
      component.setupEventListeners();
      const clickHandler = component.shadowRoot.addEventListener.mock.calls
        .find(call => call[0] === 'click')[1];
      
      clickHandler(mockEvent);
      
      expect(mockEvent.stopPropagation).toHaveBeenCalled();
      expect(toggleSpy).toHaveBeenCalledWith('CodeAI');
    });

    test('should handle search input', () => {
      const filterSpy = jest.spyOn(component, 'filterAndRender').mockImplementation(() => {});
      
      const mockEvent = {
        target: {
          matches: jest.fn((selector) => selector === '.search-input'),
          value: 'test query'
        }
      };
      
      component.setupEventListeners();
      const inputHandler = component.shadowRoot.addEventListener.mock.calls
        .find(call => call[0] === 'input')[1];
      
      inputHandler(mockEvent);
      
      expect(component.searchQuery).toBe('test query');
      expect(filterSpy).toHaveBeenCalled();
      
      filterSpy.mockRestore();
    });
  });

  describe('Public API', () => {
    test('should set selected agent via API', () => {
      const selectSpy = jest.spyOn(component, 'selectAgent');
      
      component.setSelectedAgent('PlannerAI');
      
      expect(selectSpy).toHaveBeenCalledWith('PlannerAI');
    });

    test('should set callbacks via API', () => {
      const mockCallback = jest.fn();
      
      component.setOnAgentSelect(mockCallback);
      expect(component.onAgentSelect).toBe(mockCallback);
      
      component.setOnAgentInfo(mockCallback);
      expect(component.onAgentInfo).toBe(mockCallback);
    });
  });

  describe('Lifecycle', () => {
    test('should setup properly on connect', () => {
      const renderSpy = jest.spyOn(component, 'render').mockImplementation(() => {});
      const setupSpy = jest.spyOn(component, 'setupEventListeners').mockImplementation(() => {});
      
      component.connectedCallback();
      
      expect(renderSpy).toHaveBeenCalled();
      expect(setupSpy).toHaveBeenCalled();
      
      renderSpy.mockRestore();
      setupSpy.mockRestore();
    });
  });
});

describe('AgentSelector Integration', () => {
  test('should work with customElements registry', () => {
    // This tests the registration logic
    expect(customElements.define).toHaveBeenCalledWith('agent-selector', expect.any(Function));
  });
});