/**
 * Unit Tests for AgentSelector Module
 */

const AgentSelector = require('../../src/modules/ui/AgentSelector.js');

describe('AgentSelector', () => {
  let agentSelector;
  let mockOnSelectionChange;
  let mockOnGroupChatToggle;

  beforeEach(() => {
    mockOnSelectionChange = jest.fn();
    mockOnGroupChatToggle = jest.fn();
    
    agentSelector = new AgentSelector({
      onSelectionChange: mockOnSelectionChange,
      onGroupChatToggle: mockOnGroupChatToggle
    });
  });

  describe('Constructor', () => {
    test('should initialize with default agents', () => {
      const state = agentSelector.getSelectionState();
      expect(state.agents).toHaveLength(5);
      expect(state.selectedAgent).toBe('auto');
      expect(state.isGroupChat).toBe(false);
    });

    test('should accept custom agents', () => {
      const customAgents = [
        { id: 'test', name: 'Test Agent', role: 'Testing', avatar: '🧪', status: 'online' }
      ];
      
      const selector = new AgentSelector({ agents: customAgents });
      const state = selector.getSelectionState();
      
      expect(state.agents).toEqual(customAgents);
    });

    test('should accept custom default agent', () => {
      const selector = new AgentSelector({ defaultAgent: 'PlannerAI' });
      const state = selector.getSelectionState();
      
      expect(state.selectedAgent).toBe('PlannerAI');
    });
  });

  describe('Single Agent Selection', () => {
    test('should select a single agent', () => {
      const result = agentSelector.selectAgent('PlannerAI');
      
      expect(result.selectedAgent).toBe('PlannerAI');
      expect(result.selectedAgents).toEqual([]);
      expect(result.isGroupChat).toBe(false);
      expect(mockOnSelectionChange).toHaveBeenCalledWith({
        selectedAgent: 'PlannerAI',
        selectedAgents: [],
        isGroupChat: false
      });
    });

    test('should change selection when selecting different agent', () => {
      agentSelector.selectAgent('PlannerAI');
      const result = agentSelector.selectAgent('CodeAI');
      
      expect(result.selectedAgent).toBe('CodeAI');
      expect(mockOnSelectionChange).toHaveBeenCalledTimes(2);
    });
  });

  describe('Group Chat Mode', () => {
    test('should toggle group chat mode', () => {
      const result = agentSelector.toggleGroupChat();
      
      expect(result.isGroupChat).toBe(true);
      expect(result.selectedAgents).toEqual([]);
      expect(mockOnGroupChatToggle).toHaveBeenCalledWith({
        isGroupChat: true,
        selectedAgent: 'auto',
        selectedAgents: []
      });
    });

    test('should allow multi-selection in group chat mode', () => {
      agentSelector.toggleGroupChat(); // Enable group chat
      
      agentSelector.selectAgent('PlannerAI');
      agentSelector.selectAgent('CodeAI');
      
      const result = agentSelector.getSelectionState();
      
      expect(result.selectedAgents).toContain('PlannerAI');
      expect(result.selectedAgents).toContain('CodeAI');
      expect(result.selectedAgents).toHaveLength(2);
    });

    test('should toggle agent selection in group chat mode', () => {
      agentSelector.toggleGroupChat(); // Enable group chat
      
      agentSelector.selectAgent('PlannerAI');
      agentSelector.selectAgent('PlannerAI'); // Toggle off
      
      const result = agentSelector.getSelectionState();
      
      expect(result.selectedAgents).not.toContain('PlannerAI');
      expect(result.selectedAgents).toHaveLength(0);
    });

    test('should return to single mode when toggling group chat off', () => {
      agentSelector.toggleGroupChat(); // Enable
      agentSelector.selectAgent('PlannerAI');
      agentSelector.selectAgent('CodeAI');
      
      const result = agentSelector.toggleGroupChat(); // Disable
      
      expect(result.isGroupChat).toBe(false);
      expect(result.selectedAgent).toBe('auto');
      expect(result.selectedAgents).toEqual([]);
    });
  });

  describe('HTML Generation', () => {
    test('should generate valid HTML with all agents', () => {
      const html = agentSelector.generateHTML();
      
      expect(html).toContain('agent-selector');
      expect(html).toContain('Available Agents');
      expect(html).toContain('agent-auto');
      expect(html).toContain('agent-PlannerAI');
      expect(html).toContain('Start Group Chat');
    });

    test('should mark selected agent in HTML', () => {
      agentSelector.selectAgent('PlannerAI');
      const html = agentSelector.generateHTML();
      
      expect(html).toContain('agent-item selected');
      expect(html).toContain('agent-PlannerAI');
    });
  });

  describe('CSS Generation', () => {
    test('should generate valid CSS', () => {
      const css = agentSelector.generateCSS();
      
      expect(css).toContain('.agent-selector');
      expect(css).toContain('.agent-item');
      expect(css).toContain('.group-chat-btn');
    });
  });

  describe('Agent Status Management', () => {
    test('should handle different agent statuses', () => {
      const agents = agentSelector.getDefaultAgents();
      
      const onlineAgents = agents.filter(a => a.status === 'online');
      expect(onlineAgents.length).toBeGreaterThan(0);
      
      // Test that we can update status
      agents[0].status = 'busy';
      expect(agents[0].status).toBe('busy');
    });
  });
});