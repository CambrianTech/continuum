/**
 * BaseCommand Unit Tests
 * 
 * TESTING REQUIREMENTS:
 * =====================
 * CRITICAL FOUNDATION VALIDATION:
 * - Abstract class behavior: Ensure getDefinition() and execute() throw when not implemented
 * - Type safety: CommandResult interface contracts
 * - Utility functions: parseParams, createSuccessResult, createErrorResult, validateRequired
 * - Error handling: Graceful handling of malformed inputs
 * - Broadcasting: WebSocket integration works when context provided
 * - Status updates: Continuon status updates work when context provided
 * 
 * MIDDLE-OUT METHODOLOGY:
 * - These tests validate Layer 1 foundation
 * - Must pass before Layer 2 (daemon) integration
 * - Builds confidence for command composition in higher layers
 */

import { BaseCommand, CommandDefinition, CommandContext, CommandResult } from '../../BaseCommand';

// Test implementation of BaseCommand for testing
class TestCommand extends BaseCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'test',
      category: 'testing',
      icon: '🧪',
      description: 'Test command for unit testing',
      params: 'message: string',
      examples: ['test "hello world"'],
      usage: 'test <message>'
    };
  }

  static async execute(params: unknown, context?: CommandContext): Promise<CommandResult> {
    this.logExecution('test', params, context);
    
    const { valid, missing } = this.validateRequired(params, ['message']);
    if (!valid) {
      return this.createErrorResult('Missing required parameters', `Missing: ${missing.join(', ')}`);
    }

    const parsed = this.parseParams<{ message: string }>(params);
    
    // Test broadcasting if context available
    if (context) {
      await this.broadcast(context, `Test broadcast: ${parsed.message}`);
      await this.updateStatus(context, 'test_completed', { message: parsed.message });
    }

    return this.createSuccessResult('Test command executed successfully', { 
      echo: parsed.message,
      timestamp: new Date().toISOString()
    });
  }
}

describe('BaseCommand Foundation Tests', () => {
  
  describe('Abstract Class Behavior', () => {
    test('should throw error when getDefinition() not implemented', () => {
      expect(() => BaseCommand.getDefinition()).toThrow('getDefinition() must be implemented by subclass');
    });

    test('should throw error when execute() not implemented', async () => {
      try {
        await BaseCommand.execute({});
        throw new Error('Expected execute() to throw an error');
      } catch (error) {
        expect(error).toBeInstanceOf(Error);
        expect((error as Error).message).toBe('execute() must be implemented by subclass');
      }
    });
  });

  describe('Type Safety and Interfaces', () => {
    test('should return proper CommandDefinition structure', () => {
      const definition = TestCommand.getDefinition();
      
      expect(definition).toHaveProperty('name', 'test');
      expect(definition).toHaveProperty('category', 'testing');
      expect(definition).toHaveProperty('icon', '🧪');
      expect(definition).toHaveProperty('description');
      expect(definition).toHaveProperty('params');
      expect(definition).toHaveProperty('examples');
      expect(definition).toHaveProperty('usage');
      expect(Array.isArray(definition.examples)).toBe(true);
    });

    test('should return proper CommandResult structure', async () => {
      const result = await TestCommand.execute({ message: 'test' });
      
      expect(result).toHaveProperty('success', true);
      expect(result).toHaveProperty('message');
      expect(result).toHaveProperty('data');
      expect(result).toHaveProperty('timestamp');
      expect(typeof result.timestamp).toBe('string');
    });
  });

  describe('Utility Functions', () => {
    test('parseParams should handle JSON strings', () => {
      const jsonString = '{"message": "hello", "count": 42}';
      const parsed = TestCommand['parseParams'](jsonString);
      
      expect(parsed).toEqual({ message: 'hello', count: 42 });
    });

    test('parseParams should handle objects directly', () => {
      const obj = { message: 'hello', count: 42 };
      const parsed = TestCommand['parseParams'](obj);
      
      expect(parsed).toBe(obj);
    });

    test('parseParams should gracefully handle malformed JSON', () => {
      const malformed = '{"invalid": json}';
      const parsed = TestCommand['parseParams'](malformed);
      
      expect(parsed).toBe(malformed); // Should return original string
    });

    test('createSuccessResult should create proper success structure', () => {
      const result = TestCommand['createSuccessResult']('Operation completed', { id: 123 });
      
      expect(result.success).toBe(true);
      expect(result.message).toBe('Operation completed');
      expect(result.data).toEqual({ id: 123 });
      expect(result.timestamp).toBeDefined();
      expect(result.error).toBeUndefined();
    });

    test('createErrorResult should create proper error structure', () => {
      const result = TestCommand['createErrorResult']('Operation failed', 'Network timeout');
      
      expect(result.success).toBe(false);
      expect(result.message).toBe('Operation failed');
      expect(result.error).toBe('Network timeout');
      expect(result.timestamp).toBeDefined();
      expect(result.data).toBeUndefined();
    });

    test('validateRequired should identify missing required fields', () => {
      const params = { message: 'hello' };
      const { valid, missing } = TestCommand['validateRequired'](params, ['message', 'count']);
      
      expect(valid).toBe(false);
      expect(missing).toEqual(['count']);
    });

    test('validateRequired should pass when all fields present', () => {
      const params = { message: 'hello', count: 42 };
      const { valid, missing } = TestCommand['validateRequired'](params, ['message', 'count']);
      
      expect(valid).toBe(true);
      expect(missing).toEqual([]);
    });
  });

  describe('Command Execution Integration', () => {
    test('should execute command with valid parameters', async () => {
      const params = { message: 'test message' };
      const result = await TestCommand.execute(params);
      
      expect(result.success).toBe(true);
      expect(result.message).toBe('Test command executed successfully');
      expect(result.data?.echo).toBe('test message');
    });

    test('should fail with missing required parameters', async () => {
      const params = {}; // Missing required 'message'
      const result = await TestCommand.execute(params);
      
      expect(result.success).toBe(false);
      expect(result.message).toBe('Missing required parameters');
      expect(result.error).toContain('Missing: message');
    });

    test('should handle context broadcasting safely', async () => {
      const mockBroadcast = jest.fn();
      const context: CommandContext = {
        webSocketServer: { broadcast: mockBroadcast },
        sessionId: 'test-session-123'
      };
      
      const params = { message: 'broadcast test' };
      const result = await TestCommand.execute(params, context);
      
      expect(result.success).toBe(true);
      expect(mockBroadcast).toHaveBeenCalledWith('Test broadcast: broadcast test');
    });

    test('should handle status updates safely', async () => {
      const mockUpdate = jest.fn();
      const context: CommandContext = {
        continuonStatus: { update: mockUpdate },
        sessionId: 'test-session-456'
      };
      
      const params = { message: 'status test' };
      const result = await TestCommand.execute(params, context);
      
      expect(result.success).toBe(true);
      expect(mockUpdate).toHaveBeenCalledWith('test_completed', { message: 'status test' });
    });
  });

  describe('Registry Integration', () => {
    test('should create proper registry entry', () => {
      const entry = TestCommand.createRegistryEntry();
      
      expect(entry.name).toBe('TEST');
      expect(entry.execute).toBeDefined();
      expect(entry.definition).toEqual(TestCommand.getDefinition());
      expect(typeof entry.execute).toBe('function');
    });
  });

  describe('Error Handling and Edge Cases', () => {
    test('should handle context without webSocketServer gracefully', async () => {
      const context: CommandContext = { sessionId: 'test' };
      const params = { message: 'no broadcast' };
      
      const result = await TestCommand.execute(params, context);
      expect(result.success).toBe(true);
    });

    test('should handle malformed context gracefully', async () => {
      const context: CommandContext = { 
        webSocketServer: { broadcast: 'not a function' },
        continuonStatus: { update: null }
      };
      const params = { message: 'malformed context' };
      
      const result = await TestCommand.execute(params, context);
      expect(result.success).toBe(true);
    });

    test('should handle null and undefined parameters', () => {
      const { valid, missing } = TestCommand['validateRequired']({ message: null }, ['message']);
      expect(valid).toBe(false);
      expect(missing).toEqual(['message']);
    });
  });

});