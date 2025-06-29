/**
 * EmotionCommand Unit Tests - Debug toLowerCase bug
 * Test both CommonJS and TypeScript versions systematically
 */

const EmotionCommand = require('./EmotionCommand.cjs');

// Mock jest functions for direct execution
global.describe = (name, fn) => {
  console.log(`\n📋 ${name}`);
  fn();
};

global.test = async (name, fn) => {
  console.log(`\n🧪 ${name}`);
  try {
    await fn();
    console.log('✅ Test passed');
  } catch (error) {
    console.log('❌ Test failed:', error.message);
  }
};

global.expect = (actual) => ({
  toBe: (expected) => {
    if (actual !== expected) {
      throw new Error(`Expected ${expected}, got ${actual}`);
    }
  },
  toContain: (substring) => {
    if (!actual.includes(substring)) {
      throw new Error(`Expected "${actual}" to contain "${substring}"`);
    }
  }
});

console.log('🚀 Running EmotionCommand toLowerCase bug investigation...\n');

describe('EmotionCommand - toLowerCase Bug Investigation', () => {
  
  test('should handle valid emotion parameter', async () => {
    const params = { emotion: 'excitement' };
    const mockContext = {
      webSocketServer: {
        broadcast: jest.fn()
      }
    };
    
    console.log('🧪 Testing valid emotion parameter:', params);
    
    try {
      const result = await EmotionCommand.execute(params, mockContext);
      console.log('✅ Result:', result);
      expect(result.success).toBe(true);
    } catch (error) {
      console.log('❌ Error caught:', error.message);
      console.log('❌ Stack:', error.stack);
      expect(error.message).toContain('toLowerCase');
    }
  });

  test('should handle undefined emotion parameter', async () => {
    const params = { emotion: undefined };
    
    console.log('🧪 Testing undefined emotion parameter:', params);
    
    try {
      const result = await EmotionCommand.execute(params, {});
      console.log('Result with undefined emotion:', result);
      expect(result.success).toBe(false);
      expect(result.message).toContain('Unknown emotion');
    } catch (error) {
      console.log('❌ Error with undefined emotion:', error.message);
      expect(error.message).toContain('toLowerCase');
    }
  });

  test('should handle missing emotion parameter', async () => {
    const params = {};
    
    console.log('🧪 Testing missing emotion parameter:', params);
    
    try {
      const result = await EmotionCommand.execute(params, {});
      console.log('Result with missing emotion:', result);
    } catch (error) {
      console.log('❌ Error with missing emotion:', error.message);
    }
  });

  test('should handle feeling parameter (TypeScript compatibility)', async () => {
    const params = { feeling: 'excitement' };
    
    console.log('🧪 Testing feeling parameter:', params);
    
    try {
      const result = await EmotionCommand.execute(params, {});
      console.log('Result with feeling parameter:', result);
    } catch (error) {
      console.log('❌ Error with feeling parameter:', error.message);
    }
  });

  test('parseParams method test', () => {
    console.log('🧪 Testing parseParams directly');
    
    const testCases = [
      { emotion: 'excitement' },
      { feeling: 'joy' },
      {},
      { emotion: undefined },
      { emotion: null }
    ];
    
    testCases.forEach((testCase, index) => {
      console.log(`\n--- Test Case ${index + 1}: ${JSON.stringify(testCase)} ---`);
      try {
        const parsed = EmotionCommand.parseParams(testCase);
        console.log('Parsed result:', parsed);
        console.log('emotion value:', parsed.emotion);
        console.log('feeling value:', parsed.feeling);
        console.log('emotion typeof:', typeof parsed.emotion);
      } catch (error) {
        console.log('❌ parseParams error:', error.message);
      }
    });
  });

});