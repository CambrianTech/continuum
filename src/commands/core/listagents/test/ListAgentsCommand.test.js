/**
 * ListAgentsCommand Tests
 * Test suite for agent discovery and management functionality
 */

const ListAgentsCommand = require('../ListAgentsCommand.cjs');
const CreateRoomCommand = require('../../createroom/CreateRoomCommand.cjs');
const fs = require('fs');
const path = require('path');
const os = require('os');

describe('ListAgentsCommand', () => {
  let testDataDir;
  let mockContinuum;

  beforeEach(async () => {
    // Create temp directory for testing
    testDataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'continuum-test-'));
    
    // Mock continuum context with agent registry
    mockContinuum = {
      dataDir: testDataDir,
      userId: 'test-user',
      username: 'TestUser',
      agentRegistry: {
        getAllAgents: jest.fn().mockReturnValue([
          {
            id: 'claude',
            name: 'Claude',
            model: 'claude-3-sonnet',
            capabilities: ['text', 'code', 'analysis'],
            status: 'online',
            lastSeen: '2024-01-01T12:00:00Z',
            description: 'Advanced AI assistant'
          },
          {
            id: 'gpt-4',
            name: 'GPT-4',
            model: 'gpt-4-turbo',
            capabilities: ['text', 'code', 'vision'],
            status: 'online',
            lastSeen: '2024-01-01T12:05:00Z',
            description: 'OpenAI GPT-4 model'
          },
          {
            id: 'specialist-agent',
            name: 'Code Specialist',
            model: 'custom-model',
            capabilities: ['code', 'debugging'],
            status: 'offline',
            lastSeen: '2024-01-01T10:00:00Z',
            description: 'Specialized coding assistant'
          }
        ]),
        getAgent: jest.fn().mockImplementation((id) => {
          const agents = mockContinuum.agentRegistry.getAllAgents();
          return agents.find(agent => agent.id === id);
        }),
        getOnlineAgents: jest.fn().mockReturnValue([
          { id: 'claude', name: 'Claude', status: 'online' },
          { id: 'gpt-4', name: 'GPT-4', status: 'online' }
        ])
      }
    };

    // Ensure rooms directory exists
    const roomsDir = path.join(testDataDir, 'rooms');
    if (!fs.existsSync(roomsDir)) {
      fs.mkdirSync(roomsDir, { recursive: true });
    }
  });

  afterEach(() => {
    // Cleanup test directory
    if (fs.existsSync(testDataDir)) {
      fs.rmSync(testDataDir, { recursive: true, force: true });
    }
    jest.clearAllMocks();
  });

  describe('Command Definition', () => {
    test('should have valid command definition', () => {
      const definition = ListAgentsCommand.getDefinition();
      
      expect(definition).toBeDefined();
      expect(definition.name).toBe('LIST_AGENTS');
      expect(definition.category).toBe('Core');
      expect(definition.description).toContain('agent discovery');
      expect(definition.icon).toBe('🤖');
    });

    test('should have proper parameter definition', () => {
      const definition = ListAgentsCommand.getDefinition();
      
      expect(definition.parameters).toBeDefined();
      expect(definition.parameters.status).toBeDefined();
      expect(definition.parameters.capability).toBeDefined();
      expect(definition.parameters.roomId).toBeDefined();
    });
  });

  describe('Basic Agent Listing', () => {
    test('should list all agents successfully', async () => {
      const result = await ListAgentsCommand.execute('{}', mockContinuum);
      
      expect(result.success).toBe(true);
      expect(result.data.agents).toBeDefined();
      expect(result.data.agents).toHaveLength(3);
      expect(result.data.totalAgents).toBe(3);
    });

    test('should include agent metadata', async () => {
      const result = await ListAgentsCommand.execute('{}', mockContinuum);
      
      const claude = result.data.agents.find(agent => agent.id === 'claude');
      expect(claude).toBeDefined();
      expect(claude.name).toBe('Claude');
      expect(claude.model).toBe('claude-3-sonnet');
      expect(claude.capabilities).toContain('text');
      expect(claude.status).toBe('online');
      expect(claude.description).toBeDefined();
    });

    test('should show agent availability status', async () => {
      const result = await ListAgentsCommand.execute('{}', mockContinuum);
      
      const onlineAgents = result.data.agents.filter(agent => agent.status === 'online');
      const offlineAgents = result.data.agents.filter(agent => agent.status === 'offline');
      
      expect(onlineAgents).toHaveLength(2);
      expect(offlineAgents).toHaveLength(1);
      expect(result.data.onlineCount).toBe(2);
      expect(result.data.offlineCount).toBe(1);
    });
  });

  describe('Agent Filtering', () => {
    test('should filter by status', async () => {
      const result = await ListAgentsCommand.execute(JSON.stringify({
        status: 'online'
      }), mockContinuum);
      
      expect(result.success).toBe(true);
      expect(result.data.agents).toHaveLength(2);
      result.data.agents.forEach(agent => {
        expect(agent.status).toBe('online');
      });
    });

    test('should filter by capability', async () => {
      const result = await ListAgentsCommand.execute(JSON.stringify({
        capability: 'code'
      }), mockContinuum);
      
      expect(result.success).toBe(true);
      result.data.agents.forEach(agent => {
        expect(agent.capabilities).toContain('code');
      });
    });

    test('should filter by multiple criteria', async () => {
      const result = await ListAgentsCommand.execute(JSON.stringify({
        status: 'online',
        capability: 'vision'
      }), mockContinuum);
      
      expect(result.success).toBe(true);
      expect(result.data.agents).toHaveLength(1);
      expect(result.data.agents[0].id).toBe('gpt-4');
    });

    test('should handle no matches', async () => {
      const result = await ListAgentsCommand.execute(JSON.stringify({
        capability: 'nonexistent-capability'
      }), mockContinuum);
      
      expect(result.success).toBe(true);
      expect(result.data.agents).toHaveLength(0);
      expect(result.data.totalAgents).toBe(0);
    });
  });

  describe('Room-specific Agent Listing', () => {
    beforeEach(async () => {
      // Create room with specific agents
      await CreateRoomCommand.execute(JSON.stringify({
        name: 'Agent Test Room',
        description: 'Room for agent testing',
        inviteAgents: ['claude', 'gpt-4']
      }), mockContinuum);
    });

    test('should list agents in specific room', async () => {
      const result = await ListAgentsCommand.execute(JSON.stringify({
        roomId: 'agent-test-room'
      }), mockContinuum);
      
      expect(result.success).toBe(true);
      expect(result.data.roomAgents).toBeDefined();
      expect(result.data.roomAgents).toHaveLength(2);
      
      const roomAgentIds = result.data.roomAgents.map(agent => agent.id);
      expect(roomAgentIds).toContain('claude');
      expect(roomAgentIds).toContain('gpt-4');
    });

    test('should show agent subscription status in room', async () => {
      const result = await ListAgentsCommand.execute(JSON.stringify({
        roomId: 'agent-test-room'
      }), mockContinuum);
      
      expect(result.success).toBe(true);
      result.data.roomAgents.forEach(agent => {
        expect(agent.subscriptionStatus).toBeDefined();
        expect(['invited', 'active', 'inactive']).toContain(agent.subscriptionStatus);
      });
    });

    test('should handle non-existent room', async () => {
      const result = await ListAgentsCommand.execute(JSON.stringify({
        roomId: 'non-existent-room'
      }), mockContinuum);
      
      expect(result.success).toBe(false);
      expect(result.message).toContain('Room not found');
    });
  });

  describe('Agent Search and Discovery', () => {
    test('should search agents by name', async () => {
      const result = await ListAgentsCommand.execute(JSON.stringify({
        search: 'Claude'
      }), mockContinuum);
      
      expect(result.success).toBe(true);
      expect(result.data.agents).toHaveLength(1);
      expect(result.data.agents[0].name).toBe('Claude');
    });

    test('should search agents by description', async () => {
      const result = await ListAgentsCommand.execute(JSON.stringify({
        search: 'coding'
      }), mockContinuum);
      
      expect(result.success).toBe(true);
      const foundAgent = result.data.agents.find(agent => 
        agent.description.toLowerCase().includes('coding')
      );
      expect(foundAgent).toBeDefined();
    });

    test('should handle case-insensitive search', async () => {
      const result = await ListAgentsCommand.execute(JSON.stringify({
        search: 'claude'
      }), mockContinuum);
      
      expect(result.success).toBe(true);
      expect(result.data.agents).toHaveLength(1);
      expect(result.data.agents[0].name).toBe('Claude');
    });
  });

  describe('Agent Statistics and Metrics', () => {
    test('should provide agent statistics', async () => {
      const result = await ListAgentsCommand.execute(JSON.stringify({
        includeStats: true
      }), mockContinuum);
      
      expect(result.success).toBe(true);
      expect(result.data.statistics).toBeDefined();
      expect(result.data.statistics.totalAgents).toBe(3);
      expect(result.data.statistics.onlineAgents).toBe(2);
      expect(result.data.statistics.offlineAgents).toBe(1);
    });

    test('should show capability distribution', async () => {
      const result = await ListAgentsCommand.execute(JSON.stringify({
        includeStats: true
      }), mockContinuum);
      
      expect(result.success).toBe(true);
      expect(result.data.statistics.capabilities).toBeDefined();
      expect(result.data.statistics.capabilities.text).toBeGreaterThan(0);
      expect(result.data.statistics.capabilities.code).toBeGreaterThan(0);
    });

    test('should show model distribution', async () => {
      const result = await ListAgentsCommand.execute(JSON.stringify({
        includeStats: true
      }), mockContinuum);
      
      expect(result.success).toBe(true);
      expect(result.data.statistics.models).toBeDefined();
      expect(result.data.statistics.models).toHaveProperty('claude-3-sonnet');
      expect(result.data.statistics.models).toHaveProperty('gpt-4-turbo');
    });
  });

  describe('Agent Sorting and Pagination', () => {
    test('should sort agents by name', async () => {
      const result = await ListAgentsCommand.execute(JSON.stringify({
        sortBy: 'name'
      }), mockContinuum);
      
      expect(result.success).toBe(true);
      const names = result.data.agents.map(agent => agent.name);
      const sortedNames = [...names].sort();
      expect(names).toEqual(sortedNames);
    });

    test('should sort agents by last seen', async () => {
      const result = await ListAgentsCommand.execute(JSON.stringify({
        sortBy: 'lastSeen'
      }), mockContinuum);
      
      expect(result.success).toBe(true);
      // Most recently seen should be first
      expect(result.data.agents[0].id).toBe('gpt-4');
    });

    test('should limit results', async () => {
      const result = await ListAgentsCommand.execute(JSON.stringify({
        limit: 2
      }), mockContinuum);
      
      expect(result.success).toBe(true);
      expect(result.data.agents).toHaveLength(2);
      expect(result.data.hasMore).toBe(true);
    });

    test('should handle pagination', async () => {
      const page1 = await ListAgentsCommand.execute(JSON.stringify({
        limit: 2,
        offset: 0
      }), mockContinuum);

      const page2 = await ListAgentsCommand.execute(JSON.stringify({
        limit: 2,
        offset: 2
      }), mockContinuum);
      
      expect(page1.success).toBe(true);
      expect(page2.success).toBe(true);
      expect(page1.data.agents).toHaveLength(2);
      expect(page2.data.agents).toHaveLength(1);
      
      // No overlap between pages
      const page1Ids = page1.data.agents.map(a => a.id);
      const page2Ids = page2.data.agents.map(a => a.id);
      const intersection = page1Ids.filter(id => page2Ids.includes(id));
      expect(intersection).toHaveLength(0);
    });
  });

  describe('Agent Detail Levels', () => {
    test('should provide summary view by default', async () => {
      const result = await ListAgentsCommand.execute('{}', mockContinuum);
      
      expect(result.success).toBe(true);
      const agent = result.data.agents[0];
      expect(agent).toHaveProperty('id');
      expect(agent).toHaveProperty('name');
      expect(agent).toHaveProperty('status');
      expect(agent).toHaveProperty('capabilities');
    });

    test('should provide detailed view when requested', async () => {
      const result = await ListAgentsCommand.execute(JSON.stringify({
        includeDetails: true
      }), mockContinuum);
      
      expect(result.success).toBe(true);
      const agent = result.data.agents[0];
      expect(agent).toHaveProperty('model');
      expect(agent).toHaveProperty('description');
      expect(agent).toHaveProperty('lastSeen');
    });

    test('should provide minimal view when requested', async () => {
      const result = await ListAgentsCommand.execute(JSON.stringify({
        view: 'minimal'
      }), mockContinuum);
      
      expect(result.success).toBe(true);
      const agent = result.data.agents[0];
      expect(agent).toHaveProperty('id');
      expect(agent).toHaveProperty('name');
      expect(agent).toHaveProperty('status');
      expect(agent).not.toHaveProperty('description');
      expect(agent).not.toHaveProperty('model');
    });
  });

  describe('Error Handling', () => {
    test('should handle missing agent registry', async () => {
      const noRegistryContinuum = { ...mockContinuum, agentRegistry: null };
      const result = await ListAgentsCommand.execute('{}', noRegistryContinuum);
      
      expect(result.success).toBe(false);
      expect(result.message).toContain('Agent registry not available');
    });

    test('should handle agent registry errors', async () => {
      mockContinuum.agentRegistry.getAllAgents.mockImplementation(() => {
        throw new Error('Registry failure');
      });

      const result = await ListAgentsCommand.execute('{}', mockContinuum);
      
      expect(result.success).toBe(false);
      expect(result.message).toContain('Failed to retrieve agents');
    });

    test('should handle invalid parameters', async () => {
      const result = await ListAgentsCommand.execute('invalid-json', mockContinuum);
      
      expect(result.success).toBe(false);
      expect(result.message).toContain('Invalid parameters');
    });

    test('should handle missing continuum context', async () => {
      const result = await ListAgentsCommand.execute('{}', null);
      
      expect(result.success).toBe(false);
      expect(result.message).toContain('Continuum context required');
    });
  });

  describe('Agent Availability and Health', () => {
    test('should check agent health status', async () => {
      const result = await ListAgentsCommand.execute(JSON.stringify({
        checkHealth: true
      }), mockContinuum);
      
      expect(result.success).toBe(true);
      result.data.agents.forEach(agent => {
        expect(agent).toHaveProperty('health');
        expect(['healthy', 'degraded', 'offline']).toContain(agent.health);
      });
    });

    test('should show response time metrics', async () => {
      const result = await ListAgentsCommand.execute(JSON.stringify({
        includeMetrics: true
      }), mockContinuum);
      
      expect(result.success).toBe(true);
      const onlineAgents = result.data.agents.filter(agent => agent.status === 'online');
      onlineAgents.forEach(agent => {
        expect(agent.metrics).toBeDefined();
        expect(agent.metrics).toHaveProperty('avgResponseTime');
        expect(agent.metrics).toHaveProperty('successRate');
      });
    });
  });

  describe('Integration with BaseCommand', () => {
    test('should inherit from BaseCommand', () => {
      const BaseCommand = require('../../BaseCommand.cjs');
      expect(ListAgentsCommand.prototype.__proto__.constructor).toBe(BaseCommand);
    });

    test('should use BaseCommand helper methods', async () => {
      const result = await ListAgentsCommand.execute('{}', mockContinuum);
      
      expect(result).toHaveProperty('success');
      expect(result).toHaveProperty('message');
      expect(result).toHaveProperty('data');
      expect(result).toHaveProperty('timestamp');
    });
  });
});