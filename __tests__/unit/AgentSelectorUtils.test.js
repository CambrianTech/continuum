/**
 * AgentSelector Utils Unit Tests
 */

import { jest } from '@jest/globals';
import {
  AgentLogger,
  AgentFilter,
  AgentMetrics,
  AgentValidator,
  AgentStorage
} from '../../src/ui/utils/AgentSelectorUtils.js';

// Mock localStorage
const mockLocalStorage = {
  getItem: jest.fn(),
  setItem: jest.fn(),
  removeItem: jest.fn(),
  clear: jest.fn()
};
global.localStorage = mockLocalStorage;

describe('AgentLogger', () => {
  let logger;
  let consoleSpy;

  beforeEach(() => {
    logger = new AgentLogger('TestComponent');
    consoleSpy = {
      debug: jest.spyOn(console, 'debug').mockImplementation(() => {}),
      log: jest.spyOn(console, 'log').mockImplementation(() => {}),
      warn: jest.spyOn(console, 'warn').mockImplementation(() => {}),
      error: jest.spyOn(console, 'error').mockImplementation(() => {})
    };
  });

  afterEach(() => {
    Object.values(consoleSpy).forEach(spy => spy.mockRestore());
  });

  test('should initialize with component name', () => {
    expect(logger.componentName).toBe('TestComponent');
    expect(logger.logLevel).toBe('INFO');
  });

  test('should log info messages by default', () => {
    logger.info('Test message', { data: 'test' });
    expect(consoleSpy.log).toHaveBeenCalledWith('[TestComponent] Test message', { data: 'test' });
  });

  test('should not log debug messages by default', () => {
    logger.debug('Debug message');
    expect(consoleSpy.debug).not.toHaveBeenCalled();
  });

  test('should log debug messages when level is DEBUG', () => {
    logger.setLogLevel('DEBUG');
    logger.debug('Debug message', { test: true });
    expect(consoleSpy.debug).toHaveBeenCalledWith('[TestComponent] Debug message', { test: true });
  });

  test('should always log error messages', () => {
    logger.setLogLevel('ERROR');
    logger.error('Error message');
    expect(consoleSpy.error).toHaveBeenCalledWith('[TestComponent] Error message', {});
  });

  test('should respect log level hierarchy', () => {
    logger.setLogLevel('WARN');
    
    logger.debug('Debug');
    logger.info('Info');
    logger.warn('Warning');
    logger.error('Error');
    
    expect(consoleSpy.debug).not.toHaveBeenCalled();
    expect(consoleSpy.log).not.toHaveBeenCalled();
    expect(consoleSpy.warn).toHaveBeenCalled();
    expect(consoleSpy.error).toHaveBeenCalled();
  });
});

describe('AgentFilter', () => {
  const mockAgents = [
    { id: '1', name: 'CodeAI', role: 'Development', type: 'ai', status: 'online', lastActive: '2023-01-01T10:00:00Z' },
    { id: '2', name: 'TestBot', role: 'Testing', type: 'ai', status: 'busy', lastActive: '2023-01-02T10:00:00Z' },
    { id: '3', name: 'User1', role: 'Developer', type: 'user', status: 'online', lastActive: '2023-01-03T10:00:00Z' },
    { id: '4', name: 'Helper', role: 'Support', type: 'ai', status: 'offline', lastActive: '2023-01-01T09:00:00Z', hostInfo: { hostname: 'server1' } }
  ];

  test('should filter by search query', () => {
    const result = AgentFilter.filterBySearch(mockAgents, 'code');
    expect(result).toHaveLength(1);
    expect(result[0].name).toBe('CodeAI');
  });

  test('should filter by search query in role', () => {
    const result = AgentFilter.filterBySearch(mockAgents, 'test');
    expect(result).toHaveLength(1);
    expect(result[0].name).toBe('TestBot');
  });

  test('should filter by search query in hostname', () => {
    const result = AgentFilter.filterBySearch(mockAgents, 'server1');
    expect(result).toHaveLength(1);
    expect(result[0].name).toBe('Helper');
  });

  test('should return all agents when search query is empty', () => {
    const result = AgentFilter.filterBySearch(mockAgents, '');
    expect(result).toHaveLength(4);
  });

  test('should filter by type', () => {
    const result = AgentFilter.filterByType(mockAgents, 'ai');
    expect(result).toHaveLength(3);
    expect(result.every(agent => agent.type === 'ai')).toBe(true);
  });

  test('should filter by status', () => {
    const result = AgentFilter.filterByStatus(mockAgents, 'online');
    expect(result).toHaveLength(2);
    expect(result.every(agent => agent.status === 'online')).toBe(true);
  });

  test('should filter by favorites', () => {
    const favorites = new Set(['1', '3']);
    const result = AgentFilter.filterByFavorites(mockAgents, favorites);
    expect(result).toHaveLength(2);
    expect(result.map(a => a.id)).toEqual(['1', '3']);
  });

  test('should sort by activity', () => {
    const result = AgentFilter.sortByActivity(mockAgents);
    expect(result[0].id).toBe('3'); // Most recent (2023-01-03)
    expect(result[result.length - 1].id).toBe('4'); // Oldest (2023-01-01T09:00:00Z)
  });

  test('should sort by name', () => {
    const result = AgentFilter.sortByName(mockAgents);
    expect(result[0].name).toBe('CodeAI');
    expect(result[1].name).toBe('Helper');
  });

  test('should group by type', () => {
    const result = AgentFilter.groupByType(mockAgents);
    expect(result.ai).toHaveLength(3);
    expect(result.user).toHaveLength(1);
  });
});

describe('AgentMetrics', () => {
  let metrics;

  beforeEach(() => {
    metrics = new AgentMetrics();
  });

  test('should track agent selections', () => {
    metrics.trackSelection('agent1');
    metrics.trackSelection('agent1');
    metrics.trackSelection('agent2');
    
    const stats1 = metrics.getAgentStats('agent1');
    const stats2 = metrics.getAgentStats('agent2');
    
    expect(stats1.selections).toBe(2);
    expect(stats2.selections).toBe(1);
    expect(stats1.lastSelected).toBeTruthy();
  });

  test('should track agent interactions', () => {
    metrics.trackInteraction('agent1', 'message');
    metrics.trackInteraction('agent1', 'message');
    metrics.trackInteraction('agent1', 'command');
    
    const stats = metrics.getAgentStats('agent1');
    expect(stats.interactions.message).toBe(2);
    expect(stats.interactions.command).toBe(1);
    expect(stats.lastInteraction).toBeTruthy();
  });

  test('should get popular agents', () => {
    metrics.trackSelection('agent1');
    metrics.trackSelection('agent1');
    metrics.trackSelection('agent1');
    metrics.trackSelection('agent2');
    metrics.trackSelection('agent3');
    
    const popular = metrics.getPopularAgents(2);
    expect(popular).toHaveLength(2);
    expect(popular[0].id).toBe('agent1');
    expect(popular[0].selections).toBe(3);
  });

  test('should export and import metrics', () => {
    metrics.trackSelection('agent1');
    metrics.trackInteraction('agent1', 'test');
    
    const exported = metrics.exportMetrics();
    const newMetrics = new AgentMetrics();
    newMetrics.importMetrics(exported);
    
    const stats = newMetrics.getAgentStats('agent1');
    expect(stats.selections).toBe(1);
    expect(stats.interactions.test).toBe(1);
  });

  test('should return null for unknown agent stats', () => {
    const stats = metrics.getAgentStats('unknown');
    expect(stats).toBeNull();
  });
});

describe('AgentValidator', () => {
  test('should validate valid agent', () => {
    const validAgent = {
      id: 'test-agent',
      name: 'Test Agent',
      role: 'Testing',
      avatar: '🧪',
      gradient: 'linear-gradient(135deg, #ff0000, #00ff00)',
      status: 'online',
      type: 'ai'
    };
    
    const result = AgentValidator.validateAgent(validAgent);
    expect(result.isValid).toBe(true);
    expect(result.warnings).toHaveLength(0);
  });

  test('should throw error for missing required fields', () => {
    const invalidAgent = { name: 'Test' };
    
    expect(() => AgentValidator.validateAgent(invalidAgent))
      .toThrow('Agent missing required fields: id, role');
  });

  test('should generate warnings for missing optional fields', () => {
    const minimalAgent = {
      id: 'test',
      name: 'Test',
      role: 'Testing'
    };
    
    const result = AgentValidator.validateAgent(minimalAgent);
    expect(result.isValid).toBe(true);
    expect(result.warnings).toContain('No avatar specified');
    expect(result.warnings).toContain('No gradient specified');
    expect(result.warnings).toContain('No status specified');
  });

  test('should warn about unknown agent type', () => {
    const agentWithUnknownType = {
      id: 'test',
      name: 'Test',
      role: 'Testing',
      type: 'unknown-type'
    };
    
    const result = AgentValidator.validateAgent(agentWithUnknownType);
    expect(result.warnings).toContain('Unknown agent type: unknown-type');
  });

  test('should sanitize agent data', () => {
    const dirtyAgent = {
      id: '  test-id  ',
      name: '  Test Name  ',
      role: '  Testing  ',
      extraField: 'should be preserved'
    };
    
    const sanitized = AgentValidator.sanitizeAgent(dirtyAgent);
    expect(sanitized.id).toBe('test-id');
    expect(sanitized.name).toBe('Test Name');
    expect(sanitized.role).toBe('Testing');
    expect(sanitized.avatar).toBe('🤖');
    expect(sanitized.status).toBe('offline');
    expect(sanitized.type).toBe('ai');
    expect(sanitized.extraField).toBe('should be preserved');
  });
});

describe('AgentStorage', () => {
  let storage;

  beforeEach(() => {
    jest.clearAllMocks();
    storage = new AgentStorage('test-storage');
  });

  test('should save and load favorites', () => {
    const favorites = new Set(['agent1', 'agent2']);
    mockLocalStorage.getItem.mockReturnValue(JSON.stringify(['agent1', 'agent2']));
    
    storage.saveFavorites(favorites);
    const loaded = storage.loadFavorites();
    
    expect(mockLocalStorage.setItem).toHaveBeenCalledWith(
      'test-storage-favorites',
      JSON.stringify(['agent1', 'agent2'])
    );
    expect(loaded).toEqual(favorites);
  });

  test('should handle localStorage errors gracefully', () => {
    const consoleSpy = jest.spyOn(console, 'warn').mockImplementation(() => {});
    mockLocalStorage.setItem.mockImplementation(() => {
      throw new Error('Storage quota exceeded');
    });
    
    const favorites = new Set(['agent1']);
    storage.saveFavorites(favorites);
    
    expect(consoleSpy).toHaveBeenCalledWith(
      'Failed to save favorites to localStorage:',
      expect.any(Error)
    );
    
    consoleSpy.mockRestore();
  });

  test('should save and load metrics', () => {
    const metrics = new AgentMetrics();
    metrics.trackSelection('agent1');
    
    const mockExported = { agent1: { selections: 1 } };
    mockLocalStorage.getItem.mockReturnValue(JSON.stringify(mockExported));
    
    storage.saveMetrics(metrics);
    const loaded = storage.loadMetrics();
    
    expect(mockLocalStorage.setItem).toHaveBeenCalledWith(
      'test-storage-metrics',
      expect.stringContaining('agent1')
    );
    expect(loaded).toBeInstanceOf(AgentMetrics);
  });

  test('should return empty set for missing favorites', () => {
    mockLocalStorage.getItem.mockReturnValue(null);
    
    const loaded = storage.loadFavorites();
    expect(loaded).toEqual(new Set());
  });

  test('should return new metrics for missing data', () => {
    mockLocalStorage.getItem.mockReturnValue(null);
    
    const loaded = storage.loadMetrics();
    expect(loaded).toBeInstanceOf(AgentMetrics);
  });

  test('should clear storage', () => {
    storage.clear();
    
    expect(mockLocalStorage.removeItem).toHaveBeenCalledWith('test-storage-favorites');
    expect(mockLocalStorage.removeItem).toHaveBeenCalledWith('test-storage-metrics');
  });
});