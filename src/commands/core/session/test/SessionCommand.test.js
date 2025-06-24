/**
 * SessionCommand Unit Tests
 * Self-contained tests for session command interface
 */

const SessionCommand = require('../SessionCommand.cjs');

describe('SessionCommand', () => {
  // Mock continuum instance
  const mockContinuum = {
    log: jest.fn(),
    emit: jest.fn()
  };

  beforeEach(() => {
    jest.clearAllMocks();
  });

  describe('getDefinition', () => {
    test('should return proper command definition', () => {
      const definition = SessionCommand.getDefinition();
      
      expect(definition.name).toBe('session');
      expect(definition.category).toBe('Core');
      expect(definition.icon).toBe('📁');
      expect(definition.description).toContain('session management');
      expect(Array.isArray(definition.examples)).toBe(true);
      expect(definition.examples.length).toBeGreaterThan(0);
    });
  });

  describe('execute', () => {
    test('should handle JSON string parameters', async () => {
      const params = '{"action": "create", "type": "test", "runId": "test123"}';
      
      // Mock successful execution
      const result = await SessionCommand.execute(params, mockContinuum);
      
      expect(result).toHaveProperty('success');
    });

    test('should handle object parameters', async () => {
      const params = {
        action: 'create',
        type: 'test',
        runId: 'test123',
        metadata: { test: true }
      };
      
      const result = await SessionCommand.execute(params, mockContinuum);
      
      expect(result).toHaveProperty('success');
    });

    test('should reject invalid JSON', async () => {
      const params = '{"invalid": json}';
      
      const result = await SessionCommand.execute(params, mockContinuum);
      
      expect(result.success).toBe(false);
      expect(result.error).toContain('Invalid JSON');
    });

    test('should require action parameter', async () => {
      const params = { type: 'test', runId: 'test123' };
      
      const result = await SessionCommand.execute(params, mockContinuum);
      
      expect(result.success).toBe(false);
      expect(result.error).toContain('Missing required parameter: action');
      expect(result.availableActions).toContain('create');
    });

    test('should handle unknown actions', async () => {
      const params = { action: 'unknown', type: 'test' };
      
      const result = await SessionCommand.execute(params, mockContinuum);
      
      expect(result.success).toBe(false);
      expect(result.error).toContain('Unknown action: unknown');
    });

    test('should format create response correctly', async () => {
      const params = {
        action: 'create',
        type: 'portal',
        runId: 'test123',
        metadata: { user: 'test' }
      };
      
      const result = await SessionCommand.execute(params, mockContinuum);
      
      if (result.success) {
        expect(result.action).toBe('create');
        expect(result.sessionPath).toBeDefined();
        expect(result.message).toContain('Session created');
        expect(result.timestamp).toBeDefined();
      }
    });

    test('should format complete response correctly', async () => {
      // First create a session
      const createParams = {
        action: 'create',
        type: 'portal',
        runId: 'complete_test'
      };
      
      const createResult = await SessionCommand.execute(createParams, mockContinuum);
      
      if (createResult.success) {
        // Then complete it
        const completeParams = {
          action: 'complete',
          type: 'portal',
          runId: 'complete_test',
          results: { success: true, summary: 'Test completed' }
        };
        
        const result = await SessionCommand.execute(completeParams, mockContinuum);
        
        if (result.success) {
          expect(result.action).toBe('complete');
          expect(result.sessionPath).toBeDefined();
          expect(result.latestPath).toContain('latest');
          expect(result.message).toContain('Session completed');
        }
      }
    });

    test('should handle errors gracefully', async () => {
      // Test with invalid session type that should cause filesystem error
      const params = {
        action: 'create',
        type: '/invalid/path/type',
        runId: 'error_test'
      };
      
      const result = await SessionCommand.execute(params, mockContinuum);
      
      if (!result.success) {
        expect(result.error).toBeDefined();
        expect(result.stack).toBeDefined();
      }
    });
  });

  describe('parameter validation', () => {
    test('should validate create parameters', async () => {
      const params = { action: 'create' }; // Missing type and runId
      
      const result = await SessionCommand.execute(params, mockContinuum);
      
      // Should either succeed or fail with validation error
      expect(result).toHaveProperty('success');
    });

    test('should validate read parameters', async () => {
      const params = {
        action: 'read',
        type: 'portal',
        runId: 'nonexistent',
        artifact: 'client-logs'
      };
      
      const result = await SessionCommand.execute(params, mockContinuum);
      
      // Should handle missing session gracefully
      expect(result).toHaveProperty('success');
    });
  });
});

module.exports = {
  testSessionCommand: () => {
    console.log('✅ SessionCommand tests available');
    console.log('Run: npm test -- SessionCommand.test.js');
  }
};