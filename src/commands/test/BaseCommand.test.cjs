/**
 * Unit Tests for BaseCommand
 * Tests the core command interface and helper methods
 */

const BaseCommand = require('../core/BaseCommand.cjs');

describe('BaseCommand', () => {
  
  describe('Abstract methods', () => {
    test('getDefinition throws error when not implemented', () => {
      expect(() => BaseCommand.getDefinition()).toThrow('getDefinition must be implemented by subclass');
    });
    
    test('execute throws error when not implemented', async () => {
      await expect(BaseCommand.execute({}, {})).rejects.toThrow('execute must be implemented by subclass');
    });
  });
  
  describe('parseParams', () => {
    test('parses valid JSON string', () => {
      const result = BaseCommand.parseParams('{"test": "value"}');
      expect(result).toEqual({ test: 'value' });
    });
    
    test('returns empty object for invalid JSON', () => {
      const result = BaseCommand.parseParams('invalid json');
      expect(result).toEqual({});
    });
    
    test('returns empty object for null/undefined', () => {
      expect(BaseCommand.parseParams(null)).toEqual({});
      expect(BaseCommand.parseParams(undefined)).toEqual({});
      expect(BaseCommand.parseParams('')).toEqual({});
    });
    
    test('handles complex objects', () => {
      const complex = { nested: { array: [1, 2, 3] }, bool: true };
      const result = BaseCommand.parseParams(JSON.stringify(complex));
      expect(result).toEqual(complex);
    });
  });
  
  describe('createSuccessResult', () => {
    test('creates success result with data', () => {
      const data = { screenshot: 'test.png' };
      const result = BaseCommand.createSuccessResult(data, 'Screenshot captured');
      
      expect(result.success).toBe(true);
      expect(result.message).toBe('Screenshot captured');
      expect(result.data).toEqual(data);
      expect(result.timestamp).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/);
    });
    
    test('uses default message when not provided', () => {
      const result = BaseCommand.createSuccessResult({ test: true });
      expect(result.message).toBe('Success');
    });
  });
  
  describe('createErrorResult', () => {
    test('creates error result with message', () => {
      const result = BaseCommand.createErrorResult('Something went wrong');
      
      expect(result.success).toBe(false);
      expect(result.message).toBe('Something went wrong');
      expect(result.error).toBeNull();
      expect(result.timestamp).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/);
    });
    
    test('includes error object when provided', () => {
      const error = new Error('Test error');
      const result = BaseCommand.createErrorResult('Failed', error);
      
      expect(result.error).toBe(error);
    });
  });
  
  describe('validateParams', () => {
    test('returns valid by default', () => {
      const result = BaseCommand.validateParams({});
      
      expect(result.valid).toBe(true);
      expect(result.errors).toEqual([]);
    });
    
    test('handles options parameter', () => {
      const options = { selector: '.test' };
      const result = BaseCommand.validateParams(options);
      
      expect(result.valid).toBe(true);
    });
  });
  
  describe('Inheritance pattern', () => {
    test('can be extended by subclasses', () => {
      class TestCommand extends BaseCommand {
        static getDefinition() {
          return { name: 'test', description: 'Test command' };
        }
        
        static async execute(params, continuum) {
          return this.createSuccessResult(params, 'Test executed');
        }
      }
      
      expect(TestCommand.getDefinition()).toEqual({ 
        name: 'test', 
        description: 'Test command' 
      });
      
      // Test that helper methods are inherited
      const result = TestCommand.createSuccessResult({ test: true });
      expect(result.success).toBe(true);
    });
  });
  
  describe('Error handling', () => {
    test('parseParams handles all error cases gracefully', () => {
      const testCases = [
        '{"unclosed": ',
        '{broken json}',
        'null',
        'undefined',
        '[]',
        'true',
        '42'
      ];
      
      testCases.forEach(testCase => {
        expect(() => BaseCommand.parseParams(testCase)).not.toThrow();
      });
    });
  });
  
  describe('Result consistency', () => {
    test('success and error results have consistent structure', () => {
      const successResult = BaseCommand.createSuccessResult({ test: true });
      const errorResult = BaseCommand.createErrorResult('Test error');
      
      // Both should have these fields
      expect(successResult).toHaveProperty('success');
      expect(successResult).toHaveProperty('message');
      expect(successResult).toHaveProperty('timestamp');
      
      expect(errorResult).toHaveProperty('success');
      expect(errorResult).toHaveProperty('message');  
      expect(errorResult).toHaveProperty('timestamp');
      
      // Success has data, error has error field
      expect(successResult).toHaveProperty('data');
      expect(errorResult).toHaveProperty('error');
    });
  });
});